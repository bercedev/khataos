//! The cryptographic stamp in XAdES form: the enveloped `ds:Signature`
//! inside `ext:UBLExtensions`, as ZATCA's validator expects it.
//!
//! Sources: Security Features Implementation Standard v1.1 §2.3.3 (the
//! structure, the transforms, the algorithms) [PRIMARY], and the signed
//! sample invoices of ZATCA's e-invoicing SDK (`Data/Samples/…/
//! Simplified_Invoice.xml`, SDK 3.2.x) for the details the standard leaves
//! open [PRIMARY for the layout; the computations below were checked against
//! the sample's digest values, see the tests]. Where the two differ KhataOS
//! follows the SDK sample, which is what the validator was built from:
//!
//! - **SignatureValue**: ECDSA (secp256k1) with SHA-256 over the 32 bytes of
//!   the invoice hash, Base64 of the DER signature. Not over the
//!   canonicalised `SignedInfo` as plain XMLDSig would do: the standard's QR
//!   table calls it "ECDSA signature of the XML Hash", and the SDK signs the
//!   hash [PRIMARY: standard §4; SDK behaviour SECONDARY].
//! - **Invoice reference digest**: the invoice hash of [`crate::zatca`]
//!   (Base64 of the SHA-256 bytes), the same value as the PIH chain uses
//!   (standard §3).
//! - **Certificate digest** (`xades:CertDigest`): Base64 of the *hex text*
//!   of SHA-256 over the certificate's *Base64 text* (not its DER). This
//!   reproduces the SDK sample's value exactly (test
//!   `the_sdk_sample_digests_are_reproduced`).
//! - **SignedProperties digest**: Base64 of the hex text of SHA-256 over the
//!   `xades:SignedProperties` element exactly as it appears in the
//!   document, with `xmlns:xades` added to it and `xmlns:ds` added to every
//!   `ds:` element in it (an exclusive-C14N-like rendering, but keeping
//!   the `/>` empty-element form). This reproduces the SDK sample's value
//!   exactly. [UNCERTAIN] Whether the validator re-renders the element or
//!   hashes its text: KhataOS writes the element in the SDK's own layout
//!   (same indentation) so both readings give the same bytes.
//! - **IssuerName**: the issuer's RDNs in RFC 4514 order (last first),
//!   joined with ", " ("CN=TSZEINVOICE-SubCA-1, DC=extgazt, DC=gov,
//!   DC=local" in the sample); **SerialNumber** in decimal.
//! - **SigningTime**: UTC, `yyyy-mm-ddThh:mm:ssZ` as in the sample.
//! - The standard's table also lists SigningCertificateV2 with the whole
//!   chain, SignaturePolicyIdentifier and SignedDataObjectProperties; the
//!   SDK sample carries none of them (only SigningCertificate with the
//!   signing certificate) and neither does KhataOS [UNCERTAIN: follows the
//!   sample].
//!
//! The QR code of a stamped document carries tags 6 to 9 (see
//! [`qr_stamp`]).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::NaiveDateTime;
use sha2::{Digest, Sha256};
use x509_cert::Certificate;
use x509_cert::der::{Decode, Encode};

use crate::qr::QrStamp;
use crate::sign::{self, Signer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XadesError(pub String);

impl std::fmt::Display for XadesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "signature: {}", self.0)
    }
}

impl std::error::Error for XadesError {}

fn bad(why: impl Into<String>) -> XadesError {
    XadesError(why.into())
}

/// The CSID certificate ZATCA issued for the device's key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningCertificate {
    /// The DER certificate as Base64 text, one line (what `ds:X509Certificate` holds).
    base64: String,
    issuer: String,
    serial: String,
    public_key: Vec<u8>,
    signature: Vec<u8>,
    not_after_unix: u64,
}

/// Decimal digits of a big-endian unsigned integer.
fn decimal(bytes: &[u8]) -> String {
    let mut digits: Vec<u8> = vec![0];
    for &b in bytes {
        let mut carry = u32::from(b);
        for d in &mut digits {
            let v = u32::from(*d) * 256 + carry;
            *d = (v % 10) as u8;
            carry = v / 10;
        }
        while carry > 0 {
            digits.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    digits.iter().rev().map(|d| char::from(b'0' + d)).collect()
}

impl SigningCertificate {
    /// From the certificate's Base64 text (a PEM body, line breaks allowed).
    pub fn from_base64(text: &str) -> Result<Self, XadesError> {
        let base64: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let der = STANDARD.decode(&base64).map_err(|_| bad("the certificate is not Base64"))?;
        let cert = Certificate::from_der(&der).map_err(|e| bad(format!("not an X.509 certificate ({e})")))?;
        let tbs = &cert.tbs_certificate;
        let issuer = tbs.issuer.0.iter().rev().map(|rdn| rdn.to_string()).collect::<Vec<_>>().join(", ");
        let public_key = tbs.subject_public_key_info.to_der().map_err(|e| bad(e.to_string()))?;
        Ok(Self {
            base64,
            issuer,
            serial: decimal(tbs.serial_number.as_bytes()),
            public_key,
            signature: cert.signature.raw_bytes().to_vec(),
            not_after_unix: tbs.validity.not_after.to_unix_duration().as_secs(),
        })
    }

    /// From a PEM "CERTIFICATE".
    pub fn from_pem(pem: &str) -> Result<Self, XadesError> {
        let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
        Self::from_base64(&body)
    }

    /// From the CSID APIs' `binarySecurityToken`: Base64 of the
    /// certificate's Base64 text (the sandbox's issued tokens decode to
    /// "MII…", checked live).
    pub fn from_binary_security_token(token: &str) -> Result<Self, XadesError> {
        let inner = STANDARD.decode(token.trim()).map_err(|_| bad("the security token is not Base64"))?;
        let text = String::from_utf8(inner).map_err(|_| bad("the security token is not a certificate"))?;
        Self::from_base64(&text)
    }

    pub fn base64(&self) -> &str {
        &self.base64
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The serial number in decimal.
    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// The certified public key (DER SubjectPublicKeyInfo): QR tag 8.
    pub fn public_key_der(&self) -> &[u8] {
        &self.public_key
    }

    /// The CA's signature over the certificate (DER): QR tag 9.
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    /// Expiry, Unix seconds.
    pub fn not_after_unix(&self) -> u64 {
        self.not_after_unix
    }

    /// `xades:CertDigest`: Base64 of the hex SHA-256 of the Base64 text.
    pub fn digest(&self) -> String {
        hex_digest_base64(self.base64.as_bytes())
    }
}

fn hex_digest_base64(bytes: &[u8]) -> String {
    let hex: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    STANDARD.encode(hex)
}

/// Everything needed to stamp a document.
#[derive(Debug, Clone, Copy)]
pub struct Stamp<'a> {
    pub signer: &'a Signer,
    pub certificate: &'a SigningCertificate,
    /// UTC.
    pub signing_time: NaiveDateTime,
}

impl<'a> Stamp<'a> {
    /// Refuses a certificate issued for another key.
    pub fn new(signer: &'a Signer, certificate: &'a SigningCertificate, signing_time: NaiveDateTime) -> Result<Self, XadesError> {
        if signer.public_key_der() != certificate.public_key_der() {
            return Err(bad("the certificate was issued for another key"));
        }
        Ok(Self { signer, certificate, signing_time })
    }
}

const NS_XADES: &str = "http://uri.etsi.org/01903/v1.3.2#";
const NS_DS: &str = "http://www.w3.org/2000/09/xmldsig#";

/// "2023-01-24T11:16:44Z".
pub fn signing_time_text(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The `xades:SignedProperties` element as it appears in the document, in
/// the SDK sample's layout (it sits 32 spaces in).
pub fn signed_properties(cert: &SigningCertificate, signing_time: NaiveDateTime) -> String {
    let p = |n: usize| " ".repeat(n);
    [
        "<xades:SignedProperties Id=\"xadesSignedProperties\">".to_owned(),
        format!("{}<xades:SignedSignatureProperties>", p(36)),
        format!("{}<xades:SigningTime>{}</xades:SigningTime>", p(40), signing_time_text(signing_time)),
        format!("{}<xades:SigningCertificate>", p(40)),
        format!("{}<xades:Cert>", p(44)),
        format!("{}<xades:CertDigest>", p(48)),
        format!("{}<ds:DigestMethod Algorithm=\"http://www.w3.org/2001/04/xmlenc#sha256\"/>", p(52)),
        format!("{}<ds:DigestValue>{}</ds:DigestValue>", p(52), cert.digest()),
        format!("{}</xades:CertDigest>", p(48)),
        format!("{}<xades:IssuerSerial>", p(48)),
        format!("{}<ds:X509IssuerName>{}</ds:X509IssuerName>", p(52), xml_text(cert.issuer())),
        format!("{}<ds:X509SerialNumber>{}</ds:X509SerialNumber>", p(52), cert.serial()),
        format!("{}</xades:IssuerSerial>", p(48)),
        format!("{}</xades:Cert>", p(44)),
        format!("{}</xades:SigningCertificate>", p(40)),
        format!("{}</xades:SignedSignatureProperties>", p(36)),
        format!("{}</xades:SignedProperties>", p(32)),
    ]
    .join("\n")
}

/// The digest of a `xades:SignedProperties` element's text (see the module
/// notes).
pub fn signed_properties_digest(element: &str) -> String {
    let with_ns = element.replacen("<xades:SignedProperties ", &format!("<xades:SignedProperties xmlns:xades=\"{NS_XADES}\" "), 1);
    let mut out = String::with_capacity(with_ns.len() + 512);
    let mut rest = with_ns.as_str();
    while let Some(i) = rest.find("<ds:") {
        let after = &rest[i + 4..];
        let name_len = after.find([' ', '>', '/']).unwrap_or(after.len());
        out.push_str(&rest[..i + 4 + name_len]);
        out.push_str(&format!(" xmlns:ds=\"{NS_DS}\""));
        rest = &after[name_len..];
    }
    out.push_str(rest);
    hex_digest_base64(out.as_bytes())
}

/// The signature value: Base64 of the DER ECDSA signature over the
/// invoice hash bytes.
pub fn signature_value(signer: &Signer, digest: &[u8; 32]) -> String {
    STANDARD.encode(signer.sign_hash(digest))
}

/// The whole `ext:UBLExtensions` block, written straight after the
/// `Invoice` start tag (as in the SDK sample, so cutting it out leaves the
/// hashed text unchanged).
pub fn ubl_extensions(invoice_hash: &str, signature_value: &str, cert: &SigningCertificate, signing_time: NaiveDateTime) -> String {
    let props = signed_properties(cert, signing_time);
    let props_digest = signed_properties_digest(&props);
    format!(
        r##"<ext:UBLExtensions>
    <ext:UBLExtension>
        <ext:ExtensionURI>urn:oasis:names:specification:ubl:dsig:enveloped:xades</ext:ExtensionURI>
        <ext:ExtensionContent>
            <sig:UBLDocumentSignatures xmlns:sig="urn:oasis:names:specification:ubl:schema:xsd:CommonSignatureComponents-2" xmlns:sac="urn:oasis:names:specification:ubl:schema:xsd:SignatureAggregateComponents-2" xmlns:sbc="urn:oasis:names:specification:ubl:schema:xsd:SignatureBasicComponents-2">
                <sac:SignatureInformation>
                    <cbc:ID>urn:oasis:names:specification:ubl:signature:1</cbc:ID>
                    <sbc:ReferencedSignatureID>urn:oasis:names:specification:ubl:signature:Invoice</sbc:ReferencedSignatureID>
                    <ds:Signature xmlns:ds="{NS_DS}" Id="signature">
                        <ds:SignedInfo>
                            <ds:CanonicalizationMethod Algorithm="http://www.w3.org/2006/12/xml-c14n11"/>
                            <ds:SignatureMethod Algorithm="http://www.w3.org/2001/04/xmldsig-more#ecdsa-sha256"/>
                            <ds:Reference Id="invoiceSignedData" URI="">
                                <ds:Transforms>
                                    <ds:Transform Algorithm="http://www.w3.org/TR/1999/REC-xpath-19991116">
                                        <ds:XPath>not(//ancestor-or-self::ext:UBLExtensions)</ds:XPath>
                                    </ds:Transform>
                                    <ds:Transform Algorithm="http://www.w3.org/TR/1999/REC-xpath-19991116">
                                        <ds:XPath>not(//ancestor-or-self::cac:Signature)</ds:XPath>
                                    </ds:Transform>
                                    <ds:Transform Algorithm="http://www.w3.org/TR/1999/REC-xpath-19991116">
                                        <ds:XPath>not(//ancestor-or-self::cac:AdditionalDocumentReference[cbc:ID='QR'])</ds:XPath>
                                    </ds:Transform>
                                    <ds:Transform Algorithm="http://www.w3.org/2006/12/xml-c14n11"/>
                                </ds:Transforms>
                                <ds:DigestMethod Algorithm="http://www.w3.org/2001/04/xmlenc#sha256"/>
                                <ds:DigestValue>{invoice_hash}</ds:DigestValue>
                            </ds:Reference>
                            <ds:Reference Type="http://www.w3.org/2000/09/xmldsig#SignatureProperties" URI="#xadesSignedProperties">
                                <ds:DigestMethod Algorithm="http://www.w3.org/2001/04/xmlenc#sha256"/>
                                <ds:DigestValue>{props_digest}</ds:DigestValue>
                            </ds:Reference>
                        </ds:SignedInfo>
                        <ds:SignatureValue>{signature_value}</ds:SignatureValue>
                        <ds:KeyInfo>
                            <ds:X509Data>
                                <ds:X509Certificate>{cert_b64}</ds:X509Certificate>
                            </ds:X509Data>
                        </ds:KeyInfo>
                        <ds:Object>
                            <xades:QualifyingProperties xmlns:xades="{NS_XADES}" Target="signature">
                                {props}
                            </xades:QualifyingProperties>
                        </ds:Object>
                    </ds:Signature>
                </sac:SignatureInformation>
            </sig:UBLDocumentSignatures>
        </ext:ExtensionContent>
    </ext:UBLExtension>
</ext:UBLExtensions>"##,
        cert_b64 = cert.base64(),
    )
}

/// The QR tags 6 to 9 of a stamped document, as the SDK sample has them:
/// tag 6 the invoice hash's **Base64 text** (44 bytes), tag 7 the
/// signature value's **Base64 text**, tag 8 the public key (DER
/// SubjectPublicKeyInfo, 88 bytes), tag 9 (simplified documents only) the
/// certificate's signature by ZATCA's CA (DER).
///
/// [UNCERTAIN] The standard's §4 says tag 6 is the 32-byte hash; the SDK's
/// sample invoice carries the 44-character Base64 text in tag 6 and the
/// Base64 signature text in tag 7. KhataOS follows the sample; ZATCA's
/// sandbox validator accepted it (see docs/ksa-zatca.md for the test).
pub fn qr_stamp(invoice_hash: &str, signature_value: &str, cert: &SigningCertificate, simplified: bool) -> QrStamp {
    QrStamp {
        invoice_hash: invoice_hash.as_bytes().to_vec(),
        signature: signature_value.as_bytes().to_vec(),
        public_key: cert.public_key_der().to_vec(),
        certificate_signature: simplified.then(|| cert.signature().to_vec()),
    }
}

/// The text between `open` and `close`, after `from`.
fn between<'x>(xml: &'x str, from: usize, open: &str, close: &str) -> Option<(&'x str, usize)> {
    let a = xml[from..].find(open)? + from + open.len();
    let b = xml[a..].find(close)? + a;
    Some((&xml[a..b], b))
}

/// What [`verify`] found in a stamped document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub invoice_hash: String,
    pub signing_time: String,
    pub certificate: SigningCertificate,
}

/// Check a stamp KhataOS wrote: the invoice digest is the document's hash,
/// the SignedProperties and certificate digests match, the signature value
/// verifies with the certificate's key, and the QR's tags 6 to 8 agree.
/// Not a general XAdES validator (no chain or revocation check).
pub fn verify(xml: &str) -> Result<Verified, XadesError> {
    let (hash, _) = crate::zatca::invoice_hash(xml).map_err(|e| bad(e.to_string()))?;
    let (digest_value, at) = between(xml, 0, "<ds:DigestValue>", "</ds:DigestValue>").ok_or(bad("no invoice digest"))?;
    if digest_value != hash {
        return Err(bad("the invoice digest is not the document's hash"));
    }
    let (props_digest, _) = between(xml, at, "<ds:DigestValue>", "</ds:DigestValue>").ok_or(bad("no SignedProperties digest"))?;
    let start = xml.find("<xades:SignedProperties ").ok_or(bad("no SignedProperties"))?;
    let end = xml[start..].find("</xades:SignedProperties>").ok_or(bad("SignedProperties is not closed"))? + start + "</xades:SignedProperties>".len();
    if signed_properties_digest(&xml[start..end]) != props_digest {
        return Err(bad("the SignedProperties digest does not match"));
    }
    let (cert_b64, _) = between(xml, 0, "<ds:X509Certificate>", "</ds:X509Certificate>").ok_or(bad("no certificate"))?;
    let certificate = SigningCertificate::from_base64(cert_b64)?;
    let (cert_digest, _) = between(xml, start, "<ds:DigestValue>", "</ds:DigestValue>").ok_or(bad("no certificate digest"))?;
    if cert_digest != certificate.digest() {
        return Err(bad("the certificate digest does not match"));
    }
    let (sig_b64, _) = between(xml, 0, "<ds:SignatureValue>", "</ds:SignatureValue>").ok_or(bad("no signature value"))?;
    let sig = STANDARD.decode(sig_b64).map_err(|_| bad("the signature value is not Base64"))?;
    let digest: [u8; 32] = STANDARD.decode(&hash).ok().and_then(|d| d.try_into().ok()).ok_or(bad("bad hash"))?;
    if !sign::verify(certificate.public_key_der(), &digest, &sig) {
        return Err(bad("the signature value does not verify with the certificate's key"));
    }
    let (signing_time, _) = between(xml, start, "<xades:SigningTime>", "</xades:SigningTime>").ok_or(bad("no signing time"))?;
    let qr_start = xml.find("<cbc:ID>QR</cbc:ID>").ok_or(bad("no QR"))?;
    let (qr, _) = between(xml, qr_start, "mimeCode=\"text/plain\">", "</cbc:EmbeddedDocumentBinaryObject>").ok_or(bad("no QR"))?;
    let tags = crate::qr::decode(qr).map_err(|e| bad(e.to_string()))?;
    let tag = |n: u8| tags.iter().find(|t| t.0 == n).map(|t| t.1.as_slice());
    if tag(6) != Some(hash.as_bytes()) || tag(7) != Some(sig_b64.as_bytes()) || tag(8) != Some(certificate.public_key_der()) {
        return Err(bad("the QR's tags 6 to 8 do not match the stamp"));
    }
    Ok(Verified { invoice_hash: hash, signing_time: signing_time.to_owned(), certificate })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The certificate in ZATCA's SDK sample `Simplified_Invoice.xml`
    /// (a public test certificate of ZATCA's test CA).
    pub const SDK_SAMPLE_CERT: &str = "MIID6TCCA5CgAwIBAgITbwAAf8tem6jngr16DwABAAB/yzAKBggqhkjOPQQDAjBjMRUwEwYKCZImiZPyLGQBGRYFbG9jYWwxEzARBgoJkiaJk/IsZAEZFgNnb3YxFzAVBgoJkiaJk/IsZAEZFgdleHRnYXp0MRwwGgYDVQQDExNUU1pFSU5WT0lDRS1TdWJDQS0xMB4XDTIyMDkxNDEzMjYwNFoXDTI0MDkxMzEzMjYwNFowTjELMAkGA1UEBhMCU0ExEzARBgNVBAoTCjMxMTExMTExMTExDDAKBgNVBAsTA1RTVDEcMBoGA1UEAxMTVFNULTMxMTExMTExMTEwMTExMzBWMBAGByqGSM49AgEGBSuBBAAKA0IABGGDDKDmhWAITDv7LXqLX2cmr6+qddUkpcLCvWs5rC2O29W/hS4ajAK4Qdnahym6MaijX75Cg3j4aao7ouYXJ9GjggI5MIICNTCBmgYDVR0RBIGSMIGPpIGMMIGJMTswOQYDVQQEDDIxLVRTVHwyLVRTVHwzLWE4NjZiMTQyLWFjOWMtNDI0MS1iZjhlLTdmNzg3YTI2MmNlMjEfMB0GCgmSJomT8ixkAQEMDzMxMTExMTExMTEwMTExMzENMAsGA1UEDAwEMTEwMDEMMAoGA1UEGgwDVFNUMQwwCgYDVQQPDANUU1QwHQYDVR0OBBYEFDuWYlOzWpFN3no1WtyNktQdrA8JMB8GA1UdIwQYMBaAFHZgjPsGoKxnVzWdz5qspyuZNbUvME4GA1UdHwRHMEUwQ6BBoD+GPWh0dHA6Ly90c3RjcmwuemF0Y2EuZ292LnNhL0NlcnRFbnJvbGwvVFNaRUlOVk9JQ0UtU3ViQ0EtMS5jcmwwga0GCCsGAQUFBwEBBIGgMIGdMG4GCCsGAQUFBzABhmJodHRwOi8vdHN0Y3JsLnphdGNhLmdvdi5zYS9DZXJ0RW5yb2xsL1RTWkVpbnZvaWNlU0NBMS5leHRnYXp0Lmdvdi5sb2NhbF9UU1pFSU5WT0lDRS1TdWJDQS0xKDEpLmNydDArBggrBgEFBQcwAYYfaHR0cDovL3RzdGNybC56YXRjYS5nb3Yuc2Evb2NzcDAOBgNVHQ8BAf8EBAMCB4AwHQYDVR0lBBYwFAYIKwYBBQUHAwIGCCsGAQUFBwMDMCcGCSsGAQQBgjcVCgQaMBgwCgYIKwYBBQUHAwIwCgYIKwYBBQUHAwMwCgYIKoZIzj0EAwIDRwAwRAIgOgjNPJW017lsIijmVQVkP7GzFO2KQKd9GHaukLgIWFsCIFJF9uwKhTMxDjWbN+1awsnFI7RLBRxA/6hZ+F1wtaqU";

    /// A self-signed secp256k1 test certificate for the test key `[0x11; 32]`
    /// (made with OpenSSL for these tests; issuer "CN=KhataOS Test CA,
    /// O=KhataOS"). Never a real key.
    pub const TEST_CERT: &str = include_str!("../tests/data/test-cert.b64");

    pub fn test_signer() -> Signer {
        Signer::from_secret_bytes(&[0x11; 32]).unwrap()
    }

    pub fn test_cert() -> SigningCertificate {
        SigningCertificate::from_base64(TEST_CERT).unwrap()
    }

    pub fn time() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap().and_hms_opt(7, 15, 30).unwrap()
    }

    #[test]
    fn the_sdk_sample_digests_are_reproduced() {
        let cert = SigningCertificate::from_base64(SDK_SAMPLE_CERT).unwrap();
        assert_eq!(cert.issuer(), "CN=TSZEINVOICE-SubCA-1, DC=extgazt, DC=gov, DC=local");
        assert_eq!(cert.serial(), "2475382886904809774818644480820936050208702411");
        assert_eq!(cert.digest(), "YTJkM2JhYTcwZTBhZTAxOGYwODMyNzY3NTdkZDM3YzhjY2IxOTIyZDZhM2RlZGJiMGY0NDUzZWJhYWI4MDhmYg==");
        assert_eq!(cert.public_key_der().len(), 88);
        assert_eq!(cert.signature().len(), 70, "the sample QR's tag 9 is 70 bytes");
        let t = NaiveDateTime::parse_from_str("2023-01-24T11:16:44", "%Y-%m-%dT%H:%M:%S").unwrap();
        // The sample's SignedProperties DigestValue.
        assert_eq!(
            signed_properties_digest(&signed_properties(&cert, t)),
            "N2MxMGJkZWM4MDdlYWY0ODY1ZDk0YTk3NGZhYmE4NjI5ZDM2ODYxMjg3ZDAwZDQzMmRjOTNjZTkxYjU0OWJmNw=="
        );
    }

    #[test]
    fn certificates_load_from_every_form() {
        let c = test_cert();
        assert_eq!(c.public_key_der(), test_signer().public_key_der().as_slice());
        let pem = format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n", TEST_CERT.trim());
        assert_eq!(SigningCertificate::from_pem(&pem).unwrap(), c);
        let token = STANDARD.encode(TEST_CERT.trim());
        assert_eq!(SigningCertificate::from_binary_security_token(&token).unwrap(), c);
        assert!(SigningCertificate::from_base64("AAAA").is_err());
        assert!(Stamp::new(&Signer::from_secret_bytes(&[0x22; 32]).unwrap(), &c, time()).is_err(), "a certificate for another key");
        assert_eq!(decimal(&[0x00, 0x01, 0x00]), "256");
        assert_eq!(decimal(&[]), "0");
    }

    #[test]
    fn the_signed_properties_digest_adds_the_namespaces() {
        let el = "<xades:SignedProperties Id=\"x\">\n <ds:DigestMethod Algorithm=\"a\"/>\n <ds:DigestValue>v</ds:DigestValue></xades:SignedProperties>";
        let expected = "<xades:SignedProperties xmlns:xades=\"http://uri.etsi.org/01903/v1.3.2#\" Id=\"x\">\n <ds:DigestMethod xmlns:ds=\"http://www.w3.org/2000/09/xmldsig#\" Algorithm=\"a\"/>\n <ds:DigestValue xmlns:ds=\"http://www.w3.org/2000/09/xmldsig#\">v</ds:DigestValue></xades:SignedProperties>";
        assert_eq!(signed_properties_digest(el), hex_digest_base64(expected.as_bytes()));
    }
}
