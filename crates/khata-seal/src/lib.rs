//! End-to-end encryption for sharing books through parties that must not
//! read them: a managed relay server, or files sent over WhatsApp, e-mail or
//! cloud drives.
//!
//! - A [`CompanyKey`] (32 random bytes) is created once per shared company
//!   and travels only inside invite codes, never to the relay.
//! - A passphrase can derive a key instead ([`CompanyKey::from_passphrase`],
//!   Argon2id) for file packages a person types a password into.
//! - [`seal`] / [`open`] use XChaCha20-Poly1305 with a random 24-byte nonce.
//!   The associated data binds a ciphertext to its context (company id,
//!   entry number), so a sealed entry cannot be replayed elsewhere.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

const NONCE_LEN: usize = 24;
/// Format byte prefixed to every sealed blob, so the scheme can evolve.
const VERSION: u8 = 1;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SealError {
    #[error("wrong key or passphrase, or the data was tampered with")]
    Decrypt,
    #[error("not KhataOS sealed data")]
    Format,
    #[error("the system random number generator failed")]
    Random,
    #[error("key must be 64 hex characters")]
    BadKey,
}

/// A 256-bit symmetric key. Wiped from memory when dropped.
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct CompanyKey([u8; 32]);

impl std::fmt::Debug for CompanyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CompanyKey(…)")
    }
}

impl CompanyKey {
    pub fn generate() -> Result<Self, SealError> {
        let mut k = [0u8; 32];
        getrandom::fill(&mut k).map_err(|_| SealError::Random)?;
        Ok(Self(k))
    }

    /// Derive a key from a passphrase and a stored random salt (Argon2id,
    /// 64 MiB, 3 passes).
    pub fn from_passphrase(passphrase: &str, salt: &[u8; 16]) -> Result<Self, SealError> {
        let params = Params::new(64 * 1024, 3, 1, Some(32)).map_err(|_| SealError::Format)?;
        let mut k = [0u8; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(passphrase.as_bytes(), salt, &mut k)
            .map_err(|_| SealError::Format)?;
        Ok(Self(k))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self, SealError> {
        let bytes = hex::decode(s.trim()).map_err(|_| SealError::BadKey)?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| SealError::BadKey)?;
        Ok(Self(arr))
    }
}

/// A fresh random salt for [`CompanyKey::from_passphrase`].
pub fn new_salt() -> Result<[u8; 16], SealError> {
    let mut s = [0u8; 16];
    getrandom::fill(&mut s).map_err(|_| SealError::Random)?;
    Ok(s)
}

/// Encrypt `plaintext`, bound to `context` (authenticated, not secret).
/// Layout: version (1) ‖ nonce (24) ‖ ciphertext+tag.
pub fn seal(key: &CompanyKey, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).map_err(|_| SealError::Random)?;
    let cipher = XChaCha20Poly1305::new((&key.0).into());
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad: context })
        .map_err(|_| SealError::Decrypt)?;
    let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
    out.push(VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt data made by [`seal`] with the same key and context.
pub fn open(key: &CompanyKey, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, SealError> {
    if sealed.len() < 1 + NONCE_LEN + 16 || sealed[0] != VERSION {
        return Err(SealError::Format);
    }
    let (nonce, ct) = sealed[1..].split_at(NONCE_LEN);
    XChaCha20Poly1305::new((&key.0).into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad: context })
        .map_err(|_| SealError::Decrypt)
}

/// A sealed audit entry as a blind relay stores it. Only ordering metadata
/// is visible: the entry number and the hash chain (hashes reveal nothing
/// about content but let the relay keep the chain continuous).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedEntry {
    pub seq: u64,
    pub prev_hash: String,
    pub hash: String,
    /// Hex of [`seal`] output; context = company id ‖ seq.
    pub blob: String,
}

/// Context bytes binding a sealed entry to its company and position.
pub fn entry_context(company_id: &str, seq: u64) -> Vec<u8> {
    let mut c = Vec::with_capacity(company_id.len() + 9);
    c.extend_from_slice(company_id.as_bytes());
    c.push(0);
    c.extend_from_slice(&seq.to_be_bytes());
    c
}

/// Context bytes binding a sealed shared record (a note, the invoice
/// design…) to its company. Records have no position in the trail; the
/// tag (6 bytes, never 8) keeps them apart from [`entry_context`]. A
/// relay replaying an old record changes nothing: the newer version wins.
pub fn record_context(company_id: &str) -> Vec<u8> {
    let mut c = Vec::with_capacity(company_id.len() + 7);
    c.extend_from_slice(company_id.as_bytes());
    c.push(0);
    c.extend_from_slice(b"record");
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_context_binding() {
        let k = CompanyKey::generate().unwrap();
        let sealed = seal(&k, b"company-a\x00seq1", b"voucher json").unwrap();
        assert_eq!(open(&k, b"company-a\x00seq1", &sealed).unwrap(), b"voucher json");
        assert_eq!(open(&k, b"company-b\x00seq1", &sealed), Err(SealError::Decrypt), "moved to another company");
        let other = CompanyKey::generate().unwrap();
        assert_eq!(open(&other, b"company-a\x00seq1", &sealed), Err(SealError::Decrypt), "wrong key");
    }

    #[test]
    fn tampering_and_garbage_are_rejected() {
        let k = CompanyKey::generate().unwrap();
        let mut sealed = seal(&k, b"ctx", b"secret books").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert_eq!(open(&k, b"ctx", &sealed), Err(SealError::Decrypt));
        assert_eq!(open(&k, b"ctx", b"short"), Err(SealError::Format));
    }

    #[test]
    fn a_sealed_record_cannot_pass_for_an_entry_or_another_company() {
        let k = CompanyKey::generate().unwrap();
        let sealed = seal(&k, &record_context("company-a"), b"note").unwrap();
        assert_eq!(open(&k, &record_context("company-a"), &sealed).unwrap(), b"note");
        assert_eq!(open(&k, &record_context("company-b"), &sealed), Err(SealError::Decrypt));
        assert!((0..4).all(|seq| open(&k, &entry_context("company-a", seq), &sealed).is_err()));
        assert_ne!(record_context("c").len(), entry_context("c", 0).len());
    }

    #[test]
    fn nonces_differ_so_equal_plaintexts_look_different() {
        let k = CompanyKey::generate().unwrap();
        assert_ne!(seal(&k, b"c", b"same").unwrap(), seal(&k, b"c", b"same").unwrap());
    }

    #[test]
    fn passphrase_keys_are_deterministic_per_salt() {
        let salt = new_salt().unwrap();
        let a = CompanyKey::from_passphrase("correct horse", &salt).unwrap();
        let b = CompanyKey::from_passphrase("correct horse", &salt).unwrap();
        let c = CompanyKey::from_passphrase("wrong horse", &salt).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, CompanyKey::from_passphrase("correct horse", &new_salt().unwrap()).unwrap());
    }

    #[test]
    fn hex_round_trip_and_debug_hides_key() {
        let k = CompanyKey::generate().unwrap();
        assert_eq!(CompanyKey::from_hex(&k.to_hex()).unwrap(), k);
        assert_eq!(format!("{k:?}"), "CompanyKey(…)");
        assert_eq!(CompanyKey::from_hex("abcd"), Err(SealError::BadKey));
    }
}
