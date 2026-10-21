//! The cryptographic stamp: ECDSA on secp256k1 over the invoice hash.
//!
//! Security Features Standard v1.2 §2.2.1 (`docs/research/mena-compliance.md`
//! §6.3): ECDSA with a 256-bit key and SHA-256 [PRIMARY]; the curve is
//! **secp256k1** in ZATCA's SDK and developer guides [SECONDARY: the
//! standard itself does not name it].
//!
//! What is signed [UNCERTAIN]: the 32 bytes of the invoice hash are the
//! message, hashed again with SHA-256 by ECDSA (the usual reading of the SDK
//! samples). In the full XAdES stamp the signature value covers the
//! `SignedInfo` block, which is not built here (see [`crate::zatca`]); verify
//! against the SDK before production. Signatures are deterministic (RFC
//! 6979), so stamping the same hash twice gives the same bytes.
//!
//! Keys are never stored by this crate. The caller loads one from wherever
//! it keeps it (never the books: research §8 point 7 asks for a
//! non-exportable key kept out of ordinary backups) and drops it after use.

use k256::ecdsa::signature::{Signer as _, Verifier as _};
use k256::ecdsa::{Signature, SigningKey, VerifyingKey};
use k256::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePublicKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyError(pub String);

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "signing key: {}", self.0)
    }
}

impl std::error::Error for KeyError {}

/// A device's (EGS's) signing key, with its CSID certificate's signature
/// for QR tag 9 when it has one.
#[derive(Clone)]
pub struct Signer {
    key: SigningKey,
    /// The signature bytes of the CSID certificate, issued by ZATCA's CA at
    /// onboarding (not implemented: the caller supplies them).
    pub certificate_signature: Option<Vec<u8>>,
}

impl std::fmt::Debug for Signer {
    /// Never prints the private key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signer").field("public_key", &self.public_key_der().len()).finish_non_exhaustive()
    }
}

impl Signer {
    /// From the 32-byte private scalar.
    pub fn from_secret_bytes(secret: &[u8; 32]) -> Result<Self, KeyError> {
        let key = SigningKey::from_bytes(secret.into()).map_err(|_| KeyError("not a valid secp256k1 private key".into()))?;
        Ok(Self { key, certificate_signature: None })
    }

    /// From a PEM file as OpenSSL writes it: "EC PRIVATE KEY" (SEC1) or
    /// "PRIVATE KEY" (PKCS#8).
    pub fn from_pem(pem: &str) -> Result<Self, KeyError> {
        let secret = if pem.contains("BEGIN EC PRIVATE KEY") {
            k256::SecretKey::from_sec1_pem(pem).map_err(|e| KeyError(format!("not a secp256k1 SEC1 key ({e})")))?
        } else {
            k256::SecretKey::from_pkcs8_pem(pem).map_err(|e| KeyError(format!("not a secp256k1 PKCS#8 key ({e})")))?
        };
        Ok(Self { key: SigningKey::from(secret), certificate_signature: None })
    }

    pub fn with_certificate_signature(mut self, signature: Vec<u8>) -> Self {
        self.certificate_signature = Some(signature);
        self
    }

    /// The public key as DER SubjectPublicKeyInfo (QR tag 8).
    pub fn public_key_der(&self) -> Vec<u8> {
        self.key
            .verifying_key()
            .to_public_key_der()
            .map(|d| d.as_bytes().to_vec())
            .expect("a valid key always encodes")
    }

    /// The DER signature over an invoice hash (QR tag 7).
    pub fn sign_hash(&self, digest: &[u8; 32]) -> Vec<u8> {
        self.sign_message(digest)
    }

    /// ECDSA with SHA-256 over any message (the message is hashed here), as
    /// DER: the CSR signature ([`crate::csr`]) and the invoice stamp.
    pub fn sign_message(&self, message: &[u8]) -> Vec<u8> {
        let sig: Signature = self.key.sign(message);
        sig.to_der().as_bytes().to_vec()
    }

    /// The private key as a SEC1 "EC PRIVATE KEY" PEM, the form OpenSSL and
    /// the ZATCA SDK write. Wiped from memory when dropped. For the caller's
    /// user-only key file only: never log it or put it in the books.
    pub fn to_sec1_pem(&self) -> zeroize::Zeroizing<String> {
        k256::SecretKey::from(&self.key)
            .to_sec1_pem(k256::pkcs8::LineEnding::LF)
            .expect("a valid key always encodes")
    }
}

/// A fresh key from 32 random bytes the caller supplies (this crate has no
/// randomness of its own). `None` in the vanishingly rare case the bytes
/// are not a valid scalar: ask for new bytes and try again.
pub fn key_from_entropy(bytes: &[u8; 32]) -> Option<Signer> {
    Signer::from_secret_bytes(bytes).ok()
}

/// Check a DER signature over an invoice hash with a DER public key.
pub fn verify(public_key_der: &[u8], digest: &[u8; 32], signature_der: &[u8]) -> bool {
    let (Ok(key), Ok(sig)) = (VerifyingKey::from_public_key_der(public_key_der), Signature::from_der(signature_der)) else {
        return false;
    };
    key.verify(digest, &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test key; never a real one.
    const SECRET: [u8; 32] = [0x11; 32];

    #[test]
    fn signs_deterministically_and_verifies() {
        let s = Signer::from_secret_bytes(&SECRET).unwrap();
        let digest = [0xAB; 32];
        let sig = s.sign_hash(&digest);
        assert_eq!(sig, s.sign_hash(&digest), "RFC 6979: same hash, same signature");
        assert!(verify(&s.public_key_der(), &digest, &sig));
        assert!(!verify(&s.public_key_der(), &[0xAC; 32], &sig), "another hash fails");
        assert!(!verify(&[1, 2, 3], &digest, &sig));
        assert!(!verify(&s.public_key_der(), &digest, &[0x30, 0x00]));
        assert_eq!(s.public_key_der().len(), 88, "SPKI for a compressed-or-not secp256k1 point");
    }

    #[test]
    fn loads_pem_keys_and_rejects_bad_ones() {
        // The same key written as PKCS#8 PEM, then loaded back.
        use k256::pkcs8::{EncodePrivateKey, LineEnding};
        let secret = k256::SecretKey::from_bytes(&SECRET.into()).unwrap();
        let pkcs8 = secret.to_pkcs8_pem(LineEnding::LF).unwrap();
        let sec1 = secret.to_sec1_pem(LineEnding::LF).unwrap();
        let a = Signer::from_pem(&pkcs8).unwrap();
        let b = Signer::from_pem(&sec1).unwrap();
        let s = Signer::from_secret_bytes(&SECRET).unwrap();
        assert_eq!((a.public_key_der(), b.public_key_der()), (s.public_key_der(), s.public_key_der()));
        assert!(Signer::from_pem("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----").is_err());
        assert!(Signer::from_secret_bytes(&[0; 32]).is_err(), "zero is not a key");
        assert!(!format!("{s:?}").contains("11"), "Debug never shows the key");
        let pem = s.to_sec1_pem();
        assert!(pem.starts_with("-----BEGIN EC PRIVATE KEY-----"));
        assert_eq!(Signer::from_pem(&pem).unwrap().public_key_der(), s.public_key_der(), "the PEM loads back");
        assert!(key_from_entropy(&[0; 32]).is_none() && key_from_entropy(&SECRET).is_some());
    }
}
