//! Invoice Registration Portal (IRP) API: the crypto and message shapes of
//! the NIC e-invoice API, without any I/O. The desktop connector
//! (`apps/desktop/src-tauri/src/irp_client.rs`) does the HTTP calls and
//! storage. See `docs/einvoice.md` for setup and what was tested.
//!
//! Sources (NIC e-Invoice API developer portal, einv-apisandbox.nic.in,
//! read through archived copies because the portal refused connections
//! from outside India when this was written):
//! - Authentication v1.04 (`version1.04/authentication.html`): headers
//!   `client_id`, `client_secret`, `Gstin`; the payload JSON `UserName`,
//!   `Password`, `AppKey` (32 random bytes, Base64, 44 characters),
//!   `ForceRefreshAccessToken` is Base64-encoded and then encrypted with
//!   the e-Invoice public key; the response `Sek` is the session key
//!   encrypted with the AppKey using AES-256 (AES/ECB/PKCS7Padding); the
//!   token lives 360 minutes (60 on the sandbox) [PRIMARY].
//! - Sample code in Java and C# (`sample-code-in-java.html`,
//!   `sample-code-in-c-sharp-dot-net.html`): RSA is
//!   `RSA/ECB/PKCS1PADDING` (C#: `RSACryptoServiceProvider.Encrypt(.., false)`),
//!   i.e. PKCS#1 v1.5, not OAEP; payloads are encrypted with the SEK
//!   (AES/ECB/PKCS5Padding = PKCS7 for 16-byte blocks) and Base64-encoded
//!   [PRIMARY].
//! - Generate IRN, Cancel IRN, Get IRN details, Get IRN by document
//!   details, Generate e-way bill by IRN, Get GSTIN details (v1.03/v1.04
//!   pages): headers add `user_name` and `AuthToken`; `{"Data": ..}` in
//!   and out; response fields as modelled below [PRIMARY].
//! - Release notes (27 Nov 2020, 27 Jan 2023): `ErrorDetails` and
//!   `InfoDtls` are plain JSON, not Base64; `InfoDtls` `DUPIRN` carries the
//!   existing IRN on a duplicate; `DUPIRNGLP` (error 2295) when another IRP
//!   registered it [PRIMARY].
//! - Error codes: `einvoice1.gst.gov.in/others/geterrorcodes/{VITAL,INV}`
//!   [PRIMARY, archived].
//!
//! Endpoint paths come from Microsoft's Dynamics 365 India e-invoice setup
//! page (sandbox addresses) and the NIC pages' `<URL>/eivital/v1.04/...`
//! [SECONDARY for `eicore`/`eiewb` paths]. Production hosts
//! `api.einvoice1.gst.gov.in` / `api.einvoice2.gst.gov.in` are from a NIC
//! advisory reproduced by TaxTMI [SECONDARY].

use std::fmt;

use aes::Aes256;
use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyInit};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::pkcs8::DecodePublicKey;
use rsa::rand_core::{OsRng, RngCore};
use rsa::traits::PublicKeyParts;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;

use crate::ewaybill::{TransMode, VehicleType};

// ------------------------------------------------------------ endpoints

/// Authentication (v1.04).
pub const AUTH_PATH: &str = "/eivital/v1.04/auth";
/// Generate IRN (POST).
pub const GENERATE_PATH: &str = "/eicore/v1.03/Invoice";
/// Cancel IRN (POST).
pub const CANCEL_PATH: &str = "/eicore/v1.03/Invoice/Cancel";
/// Get IRN details: GET this + the IRN.
pub const IRN_PATH: &str = "/eicore/v1.03/Invoice/irn/";
/// Get IRN details by document type, number and date (GET with a query).
/// [UNCERTAIN: the page shows `<URL>/irnbydocdetails`; the `eicore`
/// prefix is assumed like the other invoice APIs]
pub const BY_DOC_PATH: &str = "/eicore/v1.03/Invoice/irnbydocdetails";
/// Get GSTIN details: GET this + the GSTIN.
pub const GSTIN_PATH: &str = "/eivital/v1.04/Master/gstin/";
/// Generate e-way bill by IRN (POST).
pub const EWB_PATH: &str = "/eiewb/v1.03/ewaybill";

/// A known IRP API host. Private IRPs (IRP 3 to 6) and GSPs are entered as
/// a custom base URL; whether they accept NIC's request shapes is
/// [UNCERTAIN] and must be checked with the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Endpoint {
    pub id: &'static str,
    pub label: &'static str,
    pub base_url: &'static str,
    pub production: bool,
}

pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint { id: "nic-sandbox", label: "NIC sandbox (testing)", base_url: "https://einv-apisandbox.nic.in", production: false },
    Endpoint { id: "nic1", label: "NIC IRP 1 (einvoice1)", base_url: "https://api.einvoice1.gst.gov.in", production: true },
    Endpoint { id: "nic2", label: "NIC IRP 2 (einvoice2)", base_url: "https://api.einvoice2.gst.gov.in", production: true },
    Endpoint { id: "custom", label: "Other IRP or GSP (enter the base URL)", base_url: "", production: true },
];

pub fn endpoint(id: &str) -> Option<&'static Endpoint> {
    ENDPOINTS.iter().find(|e| e.id == id)
}

// ------------------------------------------------------------ errors

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrpError {
    /// The IRP public key or the verification certificate can't be read.
    Key(String),
    /// Encryption or decryption failed (wrong key, damaged data).
    Crypto(String),
    /// A response or JWT that isn't in the expected shape.
    Format(String),
    /// A request that the IRP would reject (checked before sending).
    Invalid(String),
}

impl fmt::Display for IrpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrpError::Key(m) => write!(f, "IRP key: {m}"),
            IrpError::Crypto(m) => write!(f, "IRP encryption: {m}"),
            IrpError::Format(m) => write!(f, "IRP response: {m}"),
            IrpError::Invalid(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for IrpError {}

type Result<T> = std::result::Result<T, IrpError>;

// ------------------------------------------------------------ keys

/// The PEM label and body of the first PEM block, or `None` for text that
/// isn't PEM.
fn pem_block(text: &str) -> Option<(String, Vec<u8>)> {
    let start = text.find("-----BEGIN ")?;
    let rest = &text[start + 11..];
    let label_end = rest.find("-----")?;
    let label = rest[..label_end].trim().to_owned();
    let body_start = label_end + 5;
    let end = rest.find(&format!("-----END {label}-----"))?;
    let body: String = rest[body_start..end].chars().filter(|c| !c.is_whitespace()).collect();
    STANDARD.decode(body).ok().map(|der| (label, der))
}

/// One DER TLV at `at`: (tag, start of contents, end of contents).
fn tlv(der: &[u8], at: usize) -> Option<(u8, usize, usize)> {
    let tag = *der.get(at)?;
    let first = *der.get(at + 1)? as usize;
    let (len, head) = if first < 0x80 {
        (first, 2)
    } else {
        let n = first & 0x7f;
        if n == 0 || n > 4 {
            return None;
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | *der.get(at + 2 + i)? as usize;
        }
        (len, 2 + n)
    };
    let start = at + head;
    let end = start.checked_add(len)?;
    (end <= der.len()).then_some((tag, start, end))
}

/// The SubjectPublicKeyInfo of an X.509 certificate (DER):
/// Certificate ::= SEQUENCE { tbsCertificate SEQUENCE { [0] version
/// OPTIONAL, serialNumber, signature, issuer, validity, subject,
/// subjectPublicKeyInfo, .. }, .. } (RFC 5280 §4.1).
fn certificate_spki(der: &[u8]) -> Option<&[u8]> {
    let (t, cs, _) = tlv(der, 0)?;
    let (tt, ts, te) = tlv(der, cs)?;
    if t != 0x30 || tt != 0x30 {
        return None;
    }
    let mut at = ts;
    let (tag, _, end) = tlv(der, at)?;
    if tag == 0xa0 {
        at = end;
    }
    // serialNumber, signature, issuer, validity, subject.
    for _ in 0..5 {
        at = tlv(der, at)?.2;
    }
    let (tag, _, end) = tlv(der, at)?;
    (tag == 0x30 && end <= te).then(|| &der[at..end])
}

fn key_from_der(der: &[u8]) -> Option<RsaPublicKey> {
    RsaPublicKey::from_public_key_der(der)
        .ok()
        .or_else(|| RsaPublicKey::from_pkcs1_der(der).ok())
        .or_else(|| certificate_spki(der).and_then(|spki| RsaPublicKey::from_public_key_der(spki).ok()))
}

/// An RSA public key from what NIC distributes or the user pastes: a PEM
/// `PUBLIC KEY`, `RSA PUBLIC KEY` or `CERTIFICATE`, or the bare Base64 of
/// one of them (the C# sample takes the key as a Base64 string).
pub fn parse_public_key(text: &str) -> Result<RsaPublicKey> {
    let text = text.trim();
    if text.is_empty() {
        return Err(IrpError::Key("no key was given".into()));
    }
    let der = match pem_block(text) {
        Some((_, der)) => der,
        None => STANDARD
            .decode(text.chars().filter(|c| !c.is_whitespace()).collect::<String>())
            .map_err(|_| IrpError::Key("this is not a PEM key or certificate".into()))?,
    };
    let key = key_from_der(&der).ok_or_else(|| IrpError::Key("not an RSA public key or certificate".into()))?;
    if key.size() < 256 {
        return Err(IrpError::Key(format!("a {}-bit key is too short (NIC's keys are 2048-bit)", key.size() * 8)));
    }
    Ok(key)
}

/// Key size in bits, for the settings screen.
pub fn key_bits(key: &RsaPublicKey) -> usize {
    key.size() * 8
}

// ------------------------------------------------------------ crypto

/// A new AppKey: 32 random bytes (an AES-256 key).
pub fn new_app_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    OsRng.fill_bytes(&mut k);
    k
}

/// RSA PKCS#1 v1.5 encryption with the IRP public key, Base64.
pub fn rsa_encrypt(key: &RsaPublicKey, plain: &[u8]) -> Result<String> {
    let max = key.size().saturating_sub(11);
    if plain.len() > max {
        return Err(IrpError::Invalid(format!(
            "the sign-in details are too long to encrypt ({} bytes, at most {max}); use a shorter API user name or password",
            plain.len()
        )));
    }
    key.encrypt(&mut OsRng, Pkcs1v15Encrypt, plain)
        .map(|c| STANDARD.encode(c))
        .map_err(|e| IrpError::Crypto(e.to_string()))
}

/// AES-256-ECB with PKCS#7 padding, Base64.
pub fn aes_encrypt(key: &[u8; 32], plain: &[u8]) -> String {
    let c = ecb::Encryptor::<Aes256>::new(key.into()).encrypt_padded_vec_mut::<Pkcs7>(plain);
    STANDARD.encode(c)
}

/// The inverse of [`aes_encrypt`].
pub fn aes_decrypt(key: &[u8; 32], b64: &str) -> Result<Vec<u8>> {
    let c = STANDARD
        .decode(b64.trim())
        .map_err(|_| IrpError::Crypto("encrypted data is not Base64".into()))?;
    if c.is_empty() || c.len() % 16 != 0 {
        return Err(IrpError::Crypto("encrypted data has the wrong length".into()));
    }
    ecb::Decryptor::<Aes256>::new(key.into())
        .decrypt_padded_vec_mut::<Pkcs7>(&c)
        .map_err(|_| IrpError::Crypto("wrong key or damaged data".into()))
}

/// The authentication request body: `{"Data": RSA(Base64(json))}` where the
/// JSON has `UserName`, `Password`, `AppKey` (Base64) and
/// `ForceRefreshAccessToken` (v1.04).
pub fn auth_request(key: &RsaPublicKey, user_name: &str, password: &str, app_key: &[u8; 32], force_refresh: bool) -> Result<String> {
    let payload = json!({
        "UserName": user_name,
        "Password": password,
        "AppKey": STANDARD.encode(app_key),
        "ForceRefreshAccessToken": force_refresh,
    })
    .to_string();
    let data = rsa_encrypt(key, STANDARD.encode(payload.as_bytes()).as_bytes())?;
    Ok(json!({ "Data": data }).to_string())
}

/// The session key from the authentication response's `Sek`.
pub fn decrypt_sek(sek_b64: &str, app_key: &[u8; 32]) -> Result<[u8; 32]> {
    let sek = aes_decrypt(app_key, sek_b64)?;
    sek.try_into().map_err(|_| IrpError::Crypto("the session key is not 32 bytes".into()))
}

/// A request body for the SEK-encrypted APIs: `{"Data": AES(sek, json)}`.
pub fn encrypted_request(sek: &[u8; 32], payload: &Value) -> String {
    json!({ "Data": aes_encrypt(sek, payload.to_string().as_bytes()) }).to_string()
}

/// A response's `Data` decrypted with the SEK. The pages say "Base 64
/// encoded string of decrypted invoice JSON using Sek"; whether the
/// decrypted bytes are the JSON itself (as in the Java sample) or Base64 of
/// it (as in the e-way bill API) is [UNCERTAIN], so both are accepted.
pub fn decrypt_data(sek: &[u8; 32], data_b64: &str) -> Result<Value> {
    let plain = aes_decrypt(sek, data_b64)?;
    let text = String::from_utf8(plain).map_err(|_| IrpError::Format("decrypted data is not text".into()))?;
    let text = text.trim().trim_start_matches('\u{feff}');
    if let Ok(v) = serde_json::from_str::<Value>(text)
        && (v.is_object() || v.is_array())
    {
        return Ok(v);
    }
    STANDARD
        .decode(text)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .ok_or_else(|| IrpError::Format("decrypted data is not JSON".into()))
}

// ------------------------------------------------------------ JSON helpers

/// A field by name, ignoring case and surrounding spaces (the NIC samples
/// mix `Status`/`status` and even `" Status "`).
pub fn field<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    v.as_object()?
        .iter()
        .find(|(k, _)| k.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| v)
        .filter(|v| !v.is_null())
}

/// A field as text: strings trimmed, numbers as written (AckNo and EwbNo
/// are 12 to 15 digit numbers).
pub fn text(v: &Value, name: &str) -> Option<String> {
    match field(v, name)? {
        Value::String(s) => Some(s.trim().to_owned()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

// ------------------------------------------------------------ envelopes

/// One entry of `ErrorDetails`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

/// One entry of `InfoDtls`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoDetail {
    /// `DUPIRN`, `DUPIRNGLP`, `EWBERR`, `ADDNLNFO`, or empty for a plain
    /// alert message.
    pub code: String,
    pub desc: Value,
}

/// The outer response of every IRP API.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub ok: bool,
    /// `Data`: an object for authentication, an encrypted string otherwise.
    pub data: Option<Value>,
    pub errors: Vec<ErrorDetail>,
    pub info: Vec<InfoDetail>,
}

fn error_list(v: &Value, out: &mut Vec<ErrorDetail>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| error_list(i, out)),
        Value::Object(_) => {
            let code = text(v, "ErrorCode").or_else(|| text(v, "error_code")).or_else(|| text(v, "errorCodes")).unwrap_or_default();
            let message = text(v, "ErrorMessage").or_else(|| text(v, "error_message")).or_else(|| text(v, "message")).unwrap_or_default();
            if !code.is_empty() || !message.is_empty() {
                out.push(ErrorDetail { code, message });
            }
        }
        Value::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                return;
            }
            // Older documentation: Base64 of the JSON; later: the JSON itself.
            if let Ok(inner) = serde_json::from_str::<Value>(s)
                && !inner.is_string()
            {
                return error_list(&inner, out);
            }
            if let Some(inner) = STANDARD.decode(s).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
                return error_list(&inner, out);
            }
            out.push(ErrorDetail { code: String::new(), message: s.to_owned() });
        }
        _ => {}
    }
}

fn info_list(v: &Value) -> Vec<InfoDetail> {
    match v {
        Value::Array(items) => items
            .iter()
            .filter_map(|i| match i {
                Value::Object(_) => Some(InfoDetail {
                    code: text(i, "InfCd").unwrap_or_default(),
                    desc: field(i, "Desc").cloned().unwrap_or(Value::Null),
                }),
                Value::String(s) if !s.trim().is_empty() => Some(InfoDetail { code: String::new(), desc: Value::String(s.trim().into()) }),
                _ => None,
            })
            .collect(),
        Value::String(s) => {
            let s = s.trim();
            match serde_json::from_str::<Value>(s) {
                Ok(inner @ Value::Array(_)) => info_list(&inner),
                _ if s.is_empty() => Vec::new(),
                _ => vec![InfoDetail { code: String::new(), desc: Value::String(s.into()) }],
            }
        }
        Value::Object(_) => info_list(&Value::Array(vec![v.clone()])),
        _ => Vec::new(),
    }
}

/// Parse any IRP response body.
pub fn parse_envelope(body: &str) -> Result<Envelope> {
    let v: Value = serde_json::from_str(body.trim().trim_start_matches('\u{feff}'))
        .map_err(|_| IrpError::Format(format!("not JSON: {}", preview(body))))?;
    if !v.is_object() {
        return Err(IrpError::Format("not a JSON object".into()));
    }
    let ok = text(&v, "Status").is_some_and(|s| s == "1");
    let mut errors = Vec::new();
    if let Some(e) = field(&v, "ErrorDetails") {
        error_list(e, &mut errors);
    }
    // The Java sample reads `error` on a failed authentication.
    if let Some(e) = field(&v, "error") {
        error_list(e, &mut errors);
    }
    Ok(Envelope {
        ok,
        data: field(&v, "Data").cloned(),
        errors,
        info: field(&v, "InfoDtls").map(info_list).unwrap_or_default(),
    })
}

/// At most 200 characters of a body, for messages.
pub fn preview(body: &str) -> String {
    let b: String = body.trim().chars().take(200).collect();
    if body.trim().chars().count() > 200 { format!("{b}…") } else { b }
}

impl Envelope {
    pub fn has_error(&self, code: &str) -> bool {
        self.errors.iter().any(|e| e.code == code)
    }

    /// The SEK-encrypted `Data` string.
    pub fn encrypted_data(&self) -> Result<&str> {
        match &self.data {
            Some(Value::String(s)) if !s.trim().is_empty() => Ok(s),
            _ => Err(IrpError::Format("the response has no data".into())),
        }
    }

    /// Every error explained, one per line.
    pub fn explain(&self) -> String {
        if self.errors.is_empty() {
            return "the IRP refused the request without saying why".into();
        }
        self.errors.iter().map(|e| explain(&e.code, &e.message)).collect::<Vec<_>>().join("\n")
    }
}

/// The authentication response's `Data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthData {
    pub auth_token: String,
    /// Still encrypted with the AppKey.
    pub sek: String,
    /// `TokenExpiry`, as IST wall time `yyyy-MM-dd HH:mm:ss`.
    pub token_expiry: Option<String>,
}

pub fn parse_auth(env: &Envelope) -> Result<AuthData> {
    let d = env.data.as_ref().ok_or_else(|| IrpError::Format("authentication returned no data".into()))?;
    // Some gateways return the object as a JSON string.
    let parsed;
    let d = match d {
        Value::String(s) => {
            parsed = serde_json::from_str::<Value>(s).map_err(|_| IrpError::Format("authentication data is not JSON".into()))?;
            &parsed
        }
        other => other,
    };
    Ok(AuthData {
        auth_token: text(d, "AuthToken").ok_or_else(|| IrpError::Format("no AuthToken".into()))?,
        sek: text(d, "Sek").ok_or_else(|| IrpError::Format("no Sek".into()))?,
        token_expiry: text(d, "TokenExpiry"),
    })
}

// ------------------------------------------------------------ IST times

/// India Standard Time, UTC+05:30. The IRP's dates (AckDt, CancelDate,
/// TokenExpiry, EwbValidTill) carry no zone; IST is assumed [UNCERTAIN].
pub fn ist() -> FixedOffset {
    FixedOffset::east_opt(5 * 3600 + 30 * 60).expect("valid offset")
}

/// `yyyy-MM-dd HH:mm:ss` (also with a `T` or fractional seconds) as IST.
pub fn parse_ist(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    let naive = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f", "%d/%m/%Y %H:%M:%S", "%d/%m/%Y %I:%M:%S %p"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())?;
    ist().from_local_datetime(&naive).single().map(|t| t.with_timezone(&Utc))
}

/// When a token stops being used: `TokenExpiry` less a margin, else 6 hours
/// from `now` less the margin. The portal allows a forced refresh in the
/// last 10 minutes.
pub fn token_valid_until(expiry: Option<&str>, now: DateTime<Utc>) -> DateTime<Utc> {
    let margin = Duration::minutes(10);
    expiry.and_then(parse_ist).unwrap_or(now + Duration::hours(6)) - margin
}

/// The 24-hour cancellation window from `AckDt`.
pub fn cancel_deadline(ack_dt: &str) -> Option<DateTime<Utc>> {
    parse_ist(ack_dt).map(|t| t + Duration::hours(24))
}

/// Whether the IRN can still be cancelled at `now`; the error says why not.
pub fn check_cancel_window(ack_dt: &str, now: DateTime<Utc>) -> std::result::Result<DateTime<Utc>, String> {
    let deadline = cancel_deadline(ack_dt).ok_or_else(|| format!("the acknowledgement date \"{ack_dt}\" can't be read, so the 24-hour limit can't be checked"))?;
    if now > deadline {
        let at = deadline.with_timezone(&ist()).format("%d %b %Y %H:%M");
        return Err(format!(
            "an IRN can be cancelled only within 24 hours of generation (this one closed at {at} IST). Issue a credit note instead"
        ));
    }
    Ok(deadline)
}

// ------------------------------------------------------------ IRN data

/// What Generate IRN and Get IRN details return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrnData {
    pub irn: String,
    pub ack_no: String,
    pub ack_dt: String,
    pub signed_invoice: String,
    pub signed_qr_code: String,
    /// `ACT`, `CNL` or `REJ`.
    pub status: String,
    pub ewb_no: Option<String>,
    pub ewb_dt: Option<String>,
    pub ewb_valid_till: Option<String>,
    pub remarks: Option<String>,
}

pub fn parse_irn(v: &Value) -> Result<IrnData> {
    let irn = text(v, "Irn").ok_or_else(|| IrpError::Format("no Irn in the response".into()))?;
    Ok(IrnData {
        irn,
        ack_no: text(v, "AckNo").unwrap_or_default(),
        ack_dt: text(v, "AckDt").unwrap_or_default(),
        signed_invoice: text(v, "SignedInvoice").unwrap_or_default(),
        signed_qr_code: text(v, "SignedQRCode").unwrap_or_default(),
        status: text(v, "Status").unwrap_or_else(|| "ACT".into()),
        ewb_no: text(v, "EwbNo"),
        ewb_dt: text(v, "EwbDt"),
        ewb_valid_till: text(v, "EwbValidTill"),
        remarks: text(v, "Remarks"),
    })
}

/// The existing registration reported with error 2150 (`InfCd` `DUPIRN`)
/// or 2295 (`DUPIRNGLP`: registered on another IRP).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duplicate {
    pub irn: String,
    pub ack_no: Option<String>,
    pub ack_dt: Option<String>,
    /// For `DUPIRNGLP`: the IRP that holds it, and `A` active / `C`
    /// cancelled.
    pub other_irp: Option<String>,
    pub other_status: Option<String>,
}

pub fn duplicate(env: &Envelope) -> Option<Duplicate> {
    env.info.iter().find_map(|i| {
        let d = &i.desc;
        match i.code.to_ascii_uppercase().as_str() {
            "DUPIRN" => Some(Duplicate {
                irn: text(d, "Irn")?,
                ack_no: text(d, "AckNo"),
                ack_dt: text(d, "AckDt"),
                other_irp: None,
                other_status: None,
            }),
            "DUPIRNGLP" => {
                let dtl = field(d, "IrnDtl");
                Some(Duplicate {
                    irn: text(d, "Irn")?,
                    ack_no: None,
                    ack_dt: dtl.and_then(|x| text(x, "RegTime")),
                    other_irp: dtl.and_then(|x| text(x, "RegIrp")),
                    other_status: dtl.and_then(|x| text(x, "Status")),
                })
            }
            _ => None,
        }
    })
}

/// Plain `InfoDtls` messages worth showing (e-way bill failures, notes).
pub fn info_messages(env: &Envelope) -> Vec<String> {
    let mut out = Vec::new();
    for i in &env.info {
        match i.code.to_ascii_uppercase().as_str() {
            "DUPIRN" | "DUPIRNGLP" => {}
            "EWBERR" => {
                let mut errs = Vec::new();
                error_list(&i.desc, &mut errs);
                for e in errs {
                    out.push(format!("The e-way bill was not generated: {}", explain(&e.code, &e.message)));
                }
            }
            _ => match &i.desc {
                Value::String(s) if !s.trim().is_empty() => out.push(s.trim().to_owned()),
                Value::Null => {}
                other if !other.is_string() => out.push(other.to_string()),
                _ => {}
            },
        }
    }
    out
}

// ------------------------------------------------------------ cancel

/// `CnlRsn`: 1 duplicate, 2 data entry mistake, 3 order cancelled, 4 others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancelReason {
    Duplicate,
    DataEntryMistake,
    OrderCancelled,
    Other,
}

impl CancelReason {
    pub fn code(self) -> &'static str {
        match self {
            CancelReason::Duplicate => "1",
            CancelReason::DataEntryMistake => "2",
            CancelReason::OrderCancelled => "3",
            CancelReason::Other => "4",
        }
    }

    pub fn parse(s: &str) -> Option<CancelReason> {
        match s.trim().to_ascii_lowercase().as_str() {
            "1" | "duplicate" => Some(CancelReason::Duplicate),
            "2" | "data_entry_mistake" | "dataentrymistake" | "data entry mistake" => Some(CancelReason::DataEntryMistake),
            "3" | "order_cancelled" | "ordercancelled" | "order cancelled" => Some(CancelReason::OrderCancelled),
            "4" | "other" | "others" => Some(CancelReason::Other),
            _ => None,
        }
    }
}

/// The schema forbids `"` and `\` in free text without a defined format.
fn check_free_text(what: &str, s: &str, max: usize) -> Result<()> {
    if s.chars().count() > max {
        return Err(IrpError::Invalid(format!("the {what} can be at most {max} characters")));
    }
    if s.contains('"') || s.contains('\\') {
        return Err(IrpError::Invalid(format!("the {what} can't contain \" or \\")));
    }
    Ok(())
}

fn check_irn(irn: &str) -> Result<()> {
    if irn.len() != 64 || !irn.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(IrpError::Invalid("an IRN is 64 letters and digits".into()));
    }
    Ok(())
}

/// The Cancel IRN payload: `Irn` (64), `CnlRsn` (one of 1 to 4), `CnlRem`
/// (up to 100 characters).
pub fn cancel_payload(irn: &str, reason: CancelReason, remark: &str) -> Result<Value> {
    check_irn(irn)?;
    let remark = remark.trim();
    check_free_text("cancellation remark", remark, 100)?;
    Ok(json!({ "Irn": irn, "CnlRsn": reason.code(), "CnlRem": remark }))
}

/// Cancel IRN's response data: `Irn` and `CancelDate`.
pub fn parse_cancel(v: &Value) -> Result<(String, String)> {
    Ok((
        text(v, "Irn").ok_or_else(|| IrpError::Format("no Irn in the cancellation".into()))?,
        text(v, "CancelDate").unwrap_or_default(),
    ))
}

// ------------------------------------------------------------ e-way bill by IRN

/// Part B (and optional Part A transporter) for Generate e-way bill by IRN.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EwbByIrn {
    /// 0 lets the system use its PIN-to-PIN distance; at most 4000.
    pub distance_km: u32,
    pub trans_mode: Option<TransMode>,
    /// 15-character transporter id or GSTIN.
    pub transporter_id: String,
    pub transporter_name: String,
    pub trans_doc_no: String,
    /// `dd/mm/yyyy` or `yyyy-mm-dd`.
    pub trans_doc_date: String,
    pub vehicle_no: String,
    pub over_dimensional: bool,
}

fn date_ddmmyyyy(s: &str) -> Option<String> {
    let s = s.trim();
    NaiveDate::parse_from_str(s, "%d/%m/%Y")
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .ok()
        .map(|d| d.format("%d/%m/%Y").to_string())
}

/// The payload, checked against the schema and validations on the NIC page
/// (`version1.03/ewaybill-generation-irn.html`).
pub fn ewb_payload(irn: &str, d: &EwbByIrn) -> Result<Value> {
    check_irn(irn)?;
    let mut problems = Vec::new();
    if d.distance_km > 4000 {
        problems.push("the distance can be at most 4000 km (0 lets the portal work it out from the PIN codes)".to_owned());
    }
    let tid = d.transporter_id.trim().to_ascii_uppercase();
    if !tid.is_empty() && (tid.len() != 15 || !tid.chars().all(|c| c.is_ascii_alphanumeric()) || !tid[..2].chars().all(|c| c.is_ascii_digit())) {
        problems.push("the transporter id is 15 characters, starting with two digits".to_owned());
    }
    let tname = d.transporter_name.trim();
    if !tname.is_empty() && !(3..=100).contains(&tname.chars().count()) {
        problems.push("the transporter name is 3 to 100 characters".to_owned());
    }
    if tname.contains('"') {
        problems.push("the transporter name can't contain \"".to_owned());
    }
    let vehicle = crate::ewaybill::normalise_vehicle(&d.vehicle_no);
    let doc_no = d.trans_doc_no.trim();
    let doc_dt = if d.trans_doc_date.trim().is_empty() { None } else { date_ddmmyyyy(&d.trans_doc_date) };
    if !d.trans_doc_date.trim().is_empty() && doc_dt.is_none() {
        problems.push("the transport document date must be a date (dd/mm/yyyy)".to_owned());
    }
    if !doc_no.is_empty() && (doc_no.len() > 15 || !doc_no.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '-')) {
        problems.push("the transport document number is up to 15 letters, digits, / and -".to_owned());
    }
    if !vehicle.is_empty() && !(4..=20).contains(&vehicle.len()) {
        problems.push("the vehicle number is 4 to 20 letters and digits".to_owned());
    }
    match d.trans_mode {
        None => {
            if tid.is_empty() {
                problems.push("give a mode of transport (Part B), or a transporter id for Part A only".to_owned());
            }
            if !vehicle.is_empty() || !doc_no.is_empty() {
                problems.push("a vehicle or transport document needs a mode of transport".to_owned());
            }
        }
        Some(TransMode::Road) => {
            if vehicle.is_empty() {
                problems.push("road transport needs the vehicle number".to_owned());
            }
            if !doc_no.is_empty() && doc_dt.is_none() {
                problems.push("a transport document number needs its date".to_owned());
            }
        }
        Some(TransMode::Rail | TransMode::Air) => {
            if doc_no.is_empty() || doc_dt.is_none() {
                problems.push("rail and air transport need the transport document number and date".to_owned());
            }
            if !vehicle.is_empty() {
                problems.push("a vehicle number is only for road or ship transport".to_owned());
            }
        }
        Some(TransMode::Ship) => {
            if vehicle.is_empty() && (doc_no.is_empty() || doc_dt.is_none()) {
                problems.push("ship transport needs a vehicle number or the transport document number and date".to_owned());
            }
        }
    }
    if !problems.is_empty() {
        return Err(IrpError::Invalid(problems.join("; ")));
    }
    let mut p = serde_json::Map::new();
    p.insert("Irn".into(), json!(irn));
    p.insert("Distance".into(), json!(d.distance_km));
    if let Some(m) = d.trans_mode {
        p.insert("TransMode".into(), json!(m.code()));
    }
    if !tid.is_empty() {
        p.insert("TransId".into(), json!(tid));
    }
    if !tname.is_empty() {
        p.insert("TransName".into(), json!(tname));
    }
    if !doc_no.is_empty() {
        p.insert("TransDocNo".into(), json!(doc_no));
    }
    if let Some(dt) = doc_dt {
        p.insert("TransDocDt".into(), json!(dt));
    }
    if !vehicle.is_empty() && matches!(d.trans_mode, Some(TransMode::Road | TransMode::Ship)) {
        p.insert("VehNo".into(), json!(vehicle));
        let vt = if d.over_dimensional { VehicleType::OverDimensional } else { VehicleType::Regular };
        p.insert("VehType".into(), json!(vt.code()));
    }
    Ok(Value::Object(p))
}

/// Generate e-way bill by IRN's response data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EwbData {
    pub ewb_no: String,
    pub ewb_dt: Option<String>,
    pub ewb_valid_till: Option<String>,
    pub remarks: Option<String>,
}

pub fn parse_ewb(v: &Value) -> Result<EwbData> {
    Ok(EwbData {
        ewb_no: text(v, "EwbNo").ok_or_else(|| IrpError::Format("no EwbNo in the response".into()))?,
        ewb_dt: text(v, "EwbDt"),
        ewb_valid_till: text(v, "EwbValidTill"),
        remarks: text(v, "Remarks"),
    })
}

// ------------------------------------------------------------ GSTIN details

/// Get GSTIN details (v1.04).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GstinDetails {
    pub gstin: String,
    pub trade_name: Option<String>,
    pub legal_name: Option<String>,
    /// Building name, number, floor, street, as given.
    pub address: Vec<String>,
    pub location: Option<String>,
    pub state_code: Option<String>,
    pub pin: Option<String>,
    /// `REG`, `SEZ`, `COM`, ...
    pub taxpayer_type: Option<String>,
    /// `ACT` active, `CNL` cancelled, `INA` inactive, `PRO` provisional.
    pub status: Option<String>,
    /// `B`: e-way bill generation blocked (returns not filed).
    pub block_status: Option<String>,
    pub registered_on: Option<String>,
    pub deregistered_on: Option<String>,
}

pub fn parse_gstin(v: &Value) -> Result<GstinDetails> {
    Ok(GstinDetails {
        gstin: text(v, "Gstin").ok_or_else(|| IrpError::Format("no Gstin in the response".into()))?,
        trade_name: text(v, "TradeName"),
        legal_name: text(v, "LegalName"),
        address: ["AddrBnm", "AddrBno", "AddrFlno", "AddrSt"].iter().filter_map(|k| text(v, k)).collect(),
        location: text(v, "AddrLoc"),
        state_code: text(v, "StateCode"),
        pin: text(v, "AddrPncd"),
        taxpayer_type: text(v, "TxpType"),
        status: text(v, "Status"),
        block_status: text(v, "BlkStatus"),
        registered_on: text(v, "DtReg"),
        deregistered_on: text(v, "DtDReg"),
    })
}

// ------------------------------------------------------------ JWT

/// A compact JWS split into its parts. NIC signs with RSA-SHA256; the
/// header's `alg` is `RS256` (Generate IRN sample) or the XML-DSig URI
/// `http://www.w3.org/2001/04/xmldsig-more#rsa-sha256` (Get IRN sample).
#[derive(Debug, Clone, PartialEq)]
pub struct Jwt {
    pub header: Value,
    pub claims: Value,
    signing_input: String,
    signature: Vec<u8>,
}

fn b64url(part: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(part.trim_end_matches('='))
        .map_err(|_| IrpError::Format("the signed data is not Base64url".into()))
}

pub fn decode_jwt(token: &str) -> Result<Jwt> {
    let token = token.trim();
    let mut parts = token.split('.');
    let (Some(h), Some(c), Some(s), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err(IrpError::Format("the signed data is not a JWT (three dot-separated parts)".into()));
    };
    let json = |b: Vec<u8>| serde_json::from_slice::<Value>(&b).map_err(|_| IrpError::Format("the JWT is not JSON".into()));
    Ok(Jwt { header: json(b64url(h)?)?, claims: json(b64url(c)?)?, signing_input: format!("{h}.{c}"), signature: b64url(s)? })
}

impl Jwt {
    /// NIC puts the content in a `data` claim as a JSON string.
    pub fn data(&self) -> Result<Value> {
        match field(&self.claims, "data") {
            Some(Value::String(s)) => serde_json::from_str(s).map_err(|_| IrpError::Format("the JWT data is not JSON".into())),
            Some(v) => Ok(v.clone()),
            None => Err(IrpError::Format("the JWT has no data".into())),
        }
    }

    pub fn issuer(&self) -> Option<String> {
        text(&self.claims, "iss")
    }

    /// Check the RSA-SHA256 signature with NIC's verification key or
    /// certificate. `Ok(false)` means the signature does not match.
    pub fn verify(&self, key: &RsaPublicKey) -> Result<bool> {
        use rsa::signature::Verifier;
        let alg = text(&self.header, "alg").unwrap_or_default();
        if alg != "RS256" && !alg.eq_ignore_ascii_case("http://www.w3.org/2001/04/xmldsig-more#rsa-sha256") {
            return Err(IrpError::Format(format!("unsupported signature algorithm {alg}")));
        }
        let vk = rsa::pkcs1v15::VerifyingKey::<Sha256>::new(key.clone());
        let Ok(sig) = rsa::pkcs1v15::Signature::try_from(self.signature.as_slice()) else {
            return Ok(false);
        };
        Ok(vk.verify(self.signing_input.as_bytes(), &sig).is_ok())
    }
}

/// The fields of the signed QR code (the `data` claim of `SignedQRCode`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedQr {
    pub seller_gstin: String,
    pub buyer_gstin: String,
    pub doc_no: String,
    pub doc_type: String,
    pub doc_date: String,
    pub total_invoice_value: String,
    pub item_count: String,
    pub main_hsn: String,
    pub irn: String,
    /// `IrnDt`, when present.
    pub irn_date: Option<String>,
    pub issuer: Option<String>,
}

pub fn signed_qr(token: &str) -> Result<SignedQr> {
    let jwt = decode_jwt(token)?;
    let d = jwt.data()?;
    let t = |k: &str| text(&d, k).unwrap_or_default();
    Ok(SignedQr {
        seller_gstin: t("SellerGstin"),
        buyer_gstin: t("BuyerGstin"),
        doc_no: t("DocNo"),
        doc_type: t("DocTyp"),
        doc_date: t("DocDt"),
        total_invoice_value: t("TotInvVal"),
        item_count: t("ItemCnt"),
        main_hsn: t("MainHsnCode"),
        irn: t("Irn"),
        irn_date: text(&d, "IrnDt"),
        issuer: jwt.issuer(),
    })
}

/// The text to put in the printed QR code: the `SignedQRCode` JWT exactly
/// as the IRP returned it (Rule 46(r): a self-made QR is not valid).
/// [SECONDARY: NIC FAQs and GSP guides say the signed QR string is printed
/// as is; the dimension/version of the symbol is not prescribed.]
pub fn qr_text(signed_qr_code: &str) -> Option<&str> {
    let t = signed_qr_code.trim();
    (t.split('.').count() == 3).then_some(t)
}

// ------------------------------------------------------------ messages

/// A plain message for an IRP error code (VITAL and INV lists on
/// einvoice1.gst.gov.in), with the IRP's own text in brackets.
pub fn explain(code: &str, message: &str) -> String {
    let plain = match code.trim() {
        "1004" => "the request had no GSTIN header",
        "1005" => "the IRP session expired; sign in again",
        "1007" | "1013" | "1016" => {
            "the IRP could not read the encrypted sign-in. Check the IRP public key in IRP settings (the sandbox and production keys differ)"
        }
        "1008" | "1017" | "1019" => "the IRP rejected the API user name or password. Check them in IRP settings (they are the API user, not the portal login)",
        "1010" => "the IRP rejected the client id or client secret. Check them in IRP settings",
        "1011" | "1012" => "the client id or client secret is missing",
        "1014" => "this GSTIN or API user is inactive or not enabled for e-invoicing on the IRP",
        "1015" => "the API user belongs to a different GSTIN than this company's",
        "1018" => "the API user is not linked to this client id on the IRP portal",
        "2143" => "this IRN belongs to another GSTIN",
        "2148" => "the IRP has no such IRN",
        "2150" => "an IRN was already generated for this document number",
        "2163" => "the document date is in the future",
        "2172" => "an intra-state supply needs CGST and SGST, not IGST",
        "2173" => "too many items (at most 1000)",
        "2174" => "an inter-state supply needs IGST, not CGST and SGST",
        "2176" => "an HSN code is not in the IRP's HSN master; use the code as listed (no padding with zeros)",
        "2189" => "the invoice total does not match the items, discounts, other charges and round-off",
        "2192" => "an item's total is not quantity × unit price",
        "2193" => "an item's taxable value is not total minus discount",
        "2194" => "an item's total value does not match its taxable value and taxes",
        "2211" => "the buyer's GSTIN is the same as the seller's",
        "2212" => "this supply type needs the buyer's GSTIN (not URP)",
        "2227" => "an item's CGST and SGST are not equal",
        "2230" => "this IRN has an e-way bill; cancel the e-way bill first, then the IRN",
        "2233" => "two items have the same serial number",
        "2234" => "an item's CGST/SGST does not match its taxable value and rate",
        "2235" => "an item's IGST does not match its taxable value and rate",
        "2240" => "an item's GST rate is not a notified rate",
        "2242" => "the place of supply does not fit the supply type",
        "2258" | "2259" => "the seller's state code does not match its GSTIN",
        "2265" => "the buyer's state code does not match its GSTIN",
        "2270" => "the 24-hour cancellation period is over; issue a credit note instead",
        "2278" => "this document number had an IRN that was cancelled; a cancelled number can't be used again, so issue a new invoice number",
        "2283" => "the IRN is too old to fetch with this API",
        "2284" => "the document date is before the earliest date the IRP accepts",
        "2295" => "another IRP already registered this document",
        "3028" => "a GSTIN is not in the e-invoice system; check it, or use \"Update from GST Common Portal\" on the e-invoice portal",
        "3029" => "a GSTIN is inactive or cancelled",
        "3074" | "3075" => "the buyer's GSTIN was cancelled before the document date",
        _ => "",
    };
    let message = message.trim();
    match (plain.is_empty(), code.trim().is_empty(), message.is_empty()) {
        (false, _, true) => format!("{plain} (IRP {code})"),
        (false, _, false) => format!("{plain} (IRP {code}: {message})"),
        (true, false, false) => format!("IRP error {code}: {message}"),
        (true, false, true) => format!("IRP error {code}"),
        (true, true, _) => format!("IRP: {message}"),
    }
}

// ------------------------------------------------------------ testing

/// A test key pair and the IRP side of the crypto, for unit tests and the
/// desktop's mock IRP. Never use these keys outside tests.
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    use super::*;
    use rsa::RsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;

    /// A throwaway 2048-bit key made for these tests with OpenSSL.
    pub const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCvQg2ySe5/SP78
w/VwVWlExbGQ73dVJjIJNANkYqKFW2Kv1Z+1t9Jnw5AAJwZNdKWqvn9AH+Hf99Do
EGq4Uw5Be8jg0tWhF2LzrmpXS4WgmpLnMnRamrxGVW0AO31WGptFZE9+Q32eWzwp
wBizYqlbsa07rENrqJDMitP9hfJx5sx3agyoz+3+OovAt5I//yXB04Iauom9Xj3V
dbq9tWtzaKtZU0g4q0aBbD1lQRUfvrnAL7CXfbu/aQn5/iS7ZgZy66b7inhAIP0x
vrNlblUgB+XHmrJxWuG+okzgpB6YHcUZxn09pJBN4eycRX7vWnD0ErcD8EvzZ7cv
p0PXSm3RAgMBAAECggEAHK9wS/X/YByLxcXyKoaEUiIUf+8+BjUX+/skMf5V8QNl
6fdKaoURnnO7IldA2BUnV4TSEaYWQ6ENc6paOVy4yN2QocHIfxMzmkR6OdsN3l1m
fYp70meguNdbHa5fA6SbtPYn77fuu+hddszvDg9wPmacvtdheqHOv3fanlSZ++vg
PMuWDAB3IwzJlvJYS2SXi9TEKmQ3rI7RgE5sfXV/Sqt/KVK0K41xTZnW//h3Nav7
j2U6flmlm9YWNar8OzYSgRF4O432ZLWVIi9i+CnbuzftwxvNtQA7qS+t7ClcUG0d
COHPF39IB2mn2RNd0syMArcTDcOBXTCkEurPcyXP1QKBgQDad8EntHA7/YcUe3Xu
dZg7GvJnfQ0xsZSuPLlKYTRJkXxi7mZ7fZKsMHCXi1ajdLi+YpazElgBS/WAuy91
eWg9jgSKeubyLvwo6sWz1S9CaAxGW6iL+px5/FxhixW++ERmNeccte3rb7bOoU/n
t6MI5zlB2+ZI+BfXPJjzU1gwrQKBgQDNXe1PDAAcQ4nSt8JOJjo94QCeWBmPkA51
nCmfbtn0OsqeWZcCVrb+/gFFkfMyyPPBR5xMYEQIWT4EN/KD46xfx+aY1efhxC2P
0YSMbD5JvrD3OmlNspH5xr1jQatb3V1FSFCGPIQdL/y0TEzNycz5OWLz5HTUtSi3
j8EI9iMCNQKBgEHUfBuev4MULjP5z9t/C9Qjlo+gmu0yeX7XRW4oRD023ahh9feY
Ky4/mm3PyteOQGz+ADMNf/9ECEI2JWLcppCFztB7oEOx7MHXnKNHxenYuHoq/GrR
pX0MkjnCU1csMFAsRevc1khvbUJuUUmdiD8SjapOvOGYAHcXB2+q2ZQxAoGAMhGW
S9FXLYzhyzf17Wuwg2mZ5wD/qlqqy8248B/sj3jN8XFSOcW+c8ilsrp0mosXsqWs
24oUzBxF3G1veOxHGEr6U4TDNe3IVen08O9q+Du/lWbuSX0hzc3AWrSl/I2psbyB
tViHtT3/1E1N5FYw3Qo6r8+hFELenHI10+N+gYkCgYEAoglVviAkucJMe6b6RIIS
bZXZBauTcjkpZ4BaY19krI9FATBofsQg9AyPOLxot8dtUARZ03mrax7vhtnuIuGE
sN+3C5UfHENDUG/Q4VPGr4yHVF8ff7swDPFiIwH9G5GciYZzhxAC0CxUWA7pU+c9
ZJkhNmU9izb8vlU60/UMVI0=
-----END PRIVATE KEY-----
";

    pub const TEST_PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAr0INsknuf0j+/MP1cFVp
RMWxkO93VSYyCTQDZGKihVtir9WftbfSZ8OQACcGTXSlqr5/QB/h3/fQ6BBquFMO
QXvI4NLVoRdi865qV0uFoJqS5zJ0Wpq8RlVtADt9VhqbRWRPfkN9nls8KcAYs2Kp
W7GtO6xDa6iQzIrT/YXycebMd2oMqM/t/jqLwLeSP/8lwdOCGrqJvV491XW6vbVr
c2irWVNIOKtGgWw9ZUEVH765wC+wl327v2kJ+f4ku2YGcuum+4p4QCD9Mb6zZW5V
IAflx5qycVrhvqJM4KQemB3FGcZ9PaSQTeHsnEV+71pw9BK3A/BL82e3L6dD10pt
0QIDAQAB
-----END PUBLIC KEY-----
";

    /// A self-signed certificate for the same key.
    pub const TEST_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIDNzCCAh+gAwIBAgIUfT5VccgOaORsI172gL3Df/0Y6C8wDQYJKoZIhvcNAQEL
BQAwKjEZMBcGA1UEAwwQS2hhdGFPUyBJUlAgdGVzdDENMAsGA1UECgwEVGVzdDAg
Fw0yNjEwMjAxNjUwMjRaGA8yMTI2MDkyNjE2NTAyNFowKjEZMBcGA1UEAwwQS2hh
dGFPUyBJUlAgdGVzdDENMAsGA1UECgwEVGVzdDCCASIwDQYJKoZIhvcNAQEBBQAD
ggEPADCCAQoCggEBAK9CDbJJ7n9I/vzD9XBVaUTFsZDvd1UmMgk0A2RiooVbYq/V
n7W30mfDkAAnBk10paq+f0Af4d/30OgQarhTDkF7yODS1aEXYvOualdLhaCakucy
dFqavEZVbQA7fVYam0VkT35DfZ5bPCnAGLNiqVuxrTusQ2uokMyK0/2F8nHmzHdq
DKjP7f46i8C3kj//JcHTghq6ib1ePdV1ur21a3Noq1lTSDirRoFsPWVBFR++ucAv
sJd9u79pCfn+JLtmBnLrpvuKeEAg/TG+s2VuVSAH5ceasnFa4b6iTOCkHpgdxRnG
fT2kkE3h7JxFfu9acPQStwPwS/Nnty+nQ9dKbdECAwEAAaNTMFEwHQYDVR0OBBYE
FMRsmXfXaNTlilQaSwy8Ijc5Ajp2MB8GA1UdIwQYMBaAFMRsmXfXaNTlilQaSwy8
Ijc5Ajp2MA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQELBQADggEBAHzm8raL
9DKHHEPrITIgVMRXyn2rGXdXhum3Os9kYXNf9TLBh2f4mi/VE5nQnHnRw1BK57Zi
hJ1N7BFuTIFPktZjxrLRpl7Z8mtrpR/hiypQymvu9b8njDueeqgs3Gyysn+FJJZ8
hpEAV2i58Gee8CJAKp91iqFoBOdNnjB35kWyWvqD9H5wkrSVBmXZCNDV7vPAKMBJ
va0ZUFgiLNo6F3U0/E72jHBhEDr6dgfHxJ+VRBVeB45qfBSMq7zP1qWQL1HLW47+
OZpdXe/qruv1KloI4hdg6VtXJiOpD/hhYIjMCQBKqNuk+egSO8jQHdaLFufESpxW
Gx66YzbxZ5/wJh8=
-----END CERTIFICATE-----
";

    /// The sample `SignedQRCode` from NIC's Generate IRN page (signed by
    /// NIC's sandbox key, which isn't public here).
    pub const NIC_SAMPLE_SIGNED_QR: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6IjExNUY0NDI2NjE3QTc5MzhCRTFCQTA2REJFRTkxQTQyNzU4NEVEQUIiLCJ0eXAiOiJKV1QiLCJ4NXQiOiJFVjlFSm1GNmVUaS1HNkJ0dnVrYVFuV0U3YXMifQ.eyJkYXRhIjoie1wiU2VsbGVyR3N0aW5cIjpcIjM3QVJaUFQ0Mzg0UTFNVFwiLFwiQnV5ZXJHc3RpblwiOlwiMTFBQUFDVDM5MDRGMVpaXCIsXCJEb2NOb1wiOlwiREQtMjAyMDA4MDQtOVwiLFwiRG9jVHlwXCI6XCJJTlZcIixcIkRvY0R0XCI6XCIwNC8wOC8yMDIwXCIsXCJUb3RJbnZWYWxcIjo3Nzc2LjMsXCJJdGVtQ250XCI6MyxcIk1haW5Ic25Db2RlXCI6XCIxMDAxXCIsXCJJcm5cIjpcIjExZjhlZjcwMWZlMjk0ZDRhMTRhYWQwYjEyNDU3ZTYyNzc1ZDBmZGM0MWEwYWNmMDViNzRmYmIyZGRjNDdhY2JcIn0iLCJpc3MiOiJOSUMifQ.fya8oD85f2_K8pDWSf8N94_T24O1lA9OPpIuUwk14el_r1lhL13OFxGkklhiewSMUom8DvO9JKu4jjz2l5farRTJhiBWJ43EtEky2SLzRhJf23JYW_6PyLErYL2RTzv2PlZ75eXIBZzPkxc2erCx61T50oHmExLgl1Q6HclvgiQUAVxysq1VFv96zEZVH8I0xDNqjdvqdtsW74ZHqzpV28kDIvuyV4Z5j3bR39GE6YKMetext_x3bJ4Wt4F1z3DOzfUjuKGdEjP0fTSwNg1RpiDoH4wcaMP7RJgtbQYXn4j3YoppCEw916AmbihiT2gSODPn04vhCbBecI7oOZvxpw";

    fn private_key() -> RsaPrivateKey {
        RsaPrivateKey::from_pkcs8_pem(TEST_PRIVATE_KEY_PEM).expect("test key")
    }

    /// What the IRP does with the auth `Data`: RSA-decrypt with the test
    /// private key, then Base64-decode, giving the credentials JSON.
    pub fn open_auth_request(data_b64: &str) -> Result<Value> {
        let c = STANDARD.decode(data_b64).map_err(|_| IrpError::Crypto("not Base64".into()))?;
        let b64 = private_key().decrypt(Pkcs1v15Encrypt, &c).map_err(|e| IrpError::Crypto(e.to_string()))?;
        let json = STANDARD.decode(b64).map_err(|_| IrpError::Crypto("payload not Base64".into()))?;
        serde_json::from_slice(&json).map_err(|_| IrpError::Format("payload not JSON".into()))
    }

    /// An RS256 JWT over `{"data": "<payload JSON as a string>", "iss": "NIC"}`, signed with the test key.
    pub fn sign_jwt(payload: &Value) -> String {
        use rsa::signature::{SignatureEncoding, Signer};
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let claims = URL_SAFE_NO_PAD.encode(json!({ "data": payload.to_string(), "iss": "NIC" }).to_string());
        let input = format!("{header}.{claims}");
        let sk = rsa::pkcs1v15::SigningKey::<Sha256>::new(private_key());
        let sig = sk.sign(input.as_bytes()).to_vec();
        format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig))
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    fn key() -> RsaPublicKey {
        parse_public_key(TEST_PUBLIC_KEY_PEM).unwrap()
    }

    #[test]
    fn public_keys_from_pem_certificate_and_bare_base64() {
        let k = key();
        assert_eq!(key_bits(&k), 2048);
        assert_eq!(parse_public_key(TEST_CERT_PEM).unwrap(), k);
        let bare: String = TEST_PUBLIC_KEY_PEM.lines().filter(|l| !l.starts_with("-----")).collect();
        assert_eq!(parse_public_key(&bare).unwrap(), k);
        assert!(matches!(parse_public_key(""), Err(IrpError::Key(_))));
        assert!(matches!(parse_public_key("-----BEGIN PUBLIC KEY-----\nAAAA\n-----END PUBLIC KEY-----"), Err(IrpError::Key(_))));
    }

    #[test]
    fn auth_request_round_trips_through_the_irp_side() {
        let app_key = new_app_key();
        assert_ne!(app_key, new_app_key());
        let body: Value = serde_json::from_str(&auth_request(&key(), "testuser", "S3cret!pw", &app_key, true).unwrap()).unwrap();
        let creds = open_auth_request(body["Data"].as_str().unwrap()).unwrap();
        assert_eq!(creds["UserName"], "testuser");
        assert_eq!(creds["Password"], "S3cret!pw");
        assert_eq!(creds["ForceRefreshAccessToken"], true);
        let sent = creds["AppKey"].as_str().unwrap();
        assert_eq!(sent.len(), 44);
        assert_eq!(STANDARD.decode(sent).unwrap(), app_key);
        // Too long for one RSA block.
        let long = "x".repeat(200);
        assert!(matches!(auth_request(&key(), &long, "p", &app_key, false), Err(IrpError::Invalid(_))));
    }

    #[test]
    fn sek_and_payload_round_trips() {
        let app_key = new_app_key();
        let sek = new_app_key();
        let enc_sek = aes_encrypt(&app_key, &sek);
        assert_eq!(decrypt_sek(&enc_sek, &app_key).unwrap(), sek);
        assert_ne!(decrypt_sek(&enc_sek, &new_app_key()).ok(), Some(sek));
        // 16 bytes of plaintext pad to 32 (PKCS#7 adds a full block).
        assert_eq!(STANDARD.decode(aes_encrypt(&sek, &[7u8; 16])).unwrap().len(), 32);
        let payload = json!({ "Version": "1.1", "DocDtls": { "No": "INV/1" } });
        let body: Value = serde_json::from_str(&encrypted_request(&sek, &payload)).unwrap();
        assert_eq!(decrypt_data(&sek, body["Data"].as_str().unwrap()).unwrap(), payload);
        // Base64 of the JSON inside, as the e-way bill API does.
        let wrapped = aes_encrypt(&sek, STANDARD.encode(payload.to_string()).as_bytes());
        assert_eq!(decrypt_data(&sek, &wrapped).unwrap(), payload);
        assert!(aes_decrypt(&sek, "not base64!").is_err());
        assert!(aes_decrypt(&sek, &STANDARD.encode([1u8; 15])).is_err());
    }

    #[test]
    fn aes_matches_a_known_vector() {
        // FIPS-197 C.3 (AES-256), one block, then PKCS#7 adds a block of 0x10.
        let key: [u8; 32] = std::array::from_fn(|i| i as u8);
        let plain: Vec<u8> = (0..16u8).map(|i| i * 0x11).collect();
        let c = STANDARD.decode(aes_encrypt(&key, &plain)).unwrap();
        assert_eq!(c[..16], [0x8e, 0xa2, 0xb7, 0xca, 0x51, 0x67, 0x45, 0xbf, 0xea, 0xfc, 0x49, 0x90, 0x4b, 0x49, 0x60, 0x89]);
        assert_eq!(aes_decrypt(&key, &STANDARD.encode(&c)).unwrap(), plain);
    }

    #[test]
    fn envelopes_in_the_shapes_the_samples_use() {
        let ok = parse_envelope(r#"{ " Status ":"1", " Data ":"abc=", "ErrorDetails": null, "InfoDtls":"<alert message >" }"#).unwrap();
        assert!(ok.ok);
        assert_eq!(ok.encrypted_data().unwrap(), "abc=");
        assert_eq!(ok.info[0].desc, "<alert message >");

        let auth = parse_envelope(r#"{"Status":1,"Data":{"ClientId":"c","UserName":"u","AuthToken":"tok","Sek":"s==","TokenExpiry":"2019-11-30 14:18:00"},"ErrorDetails":null,"InfoDtls":" "}"#).unwrap();
        let a = parse_auth(&auth).unwrap();
        assert_eq!((a.auth_token.as_str(), a.sek.as_str(), a.token_expiry.as_deref()), ("tok", "s==", Some("2019-11-30 14:18:00")));
        assert!(auth.info.is_empty());

        let dup = parse_envelope(
            r#"{"Status":"0","Data":null,"ErrorDetails":[{"ErrorCode":"2150","ErrorMessage":"Duplicate IRN"}],
                "InfoDtls":[{"InfCd":"DUPIRN","Desc":{"AckNo":12100028671,"AckDt":"2020-02-29 14:51:00","Irn":"bc368ad93c9fa76a40167f2e16190dc56c93e6c0caf0d0a65c0a8317c9b19f4a"}}]}"#,
        )
        .unwrap();
        assert!(!dup.ok && dup.has_error("2150"));
        let d = duplicate(&dup).unwrap();
        assert_eq!(d.irn, "bc368ad93c9fa76a40167f2e16190dc56c93e6c0caf0d0a65c0a8317c9b19f4a");
        assert_eq!(d.ack_no.as_deref(), Some("12100028671"));
        assert!(dup.explain().contains("already generated"));

        let other = parse_envelope(
            r#"{"Status":0,"ErrorDetails":[{"ErrorCode":"2295","ErrorMessage":"IRN is already generated and registered with GSTN Lookup Portal by other IRP"}],"Data":null,
                "InfoDtls":[{"InfCd":"DUPIRNGLP","Desc":{"Irn":"8ae80bccf8c819f30f77564ca29510df8a74fd1d0b3aad931e516cbb1b50c717","IrnDtl":{"RegIrp":"IRP","Status":"A","Statecd":"29","RegTime":"2023-01-07 10:32:37","CnclTime":null}}}]}"#,
        )
        .unwrap();
        let d = duplicate(&other).unwrap();
        assert_eq!((d.other_irp.as_deref(), d.other_status.as_deref()), (Some("IRP"), Some("A")));

        // Error details as a string: JSON, or Base64 of JSON (older docs).
        let s = parse_envelope(r#"{"status":"0","ErrorDetails":"[{\"ErrorCode\":\"1005\",\"ErrorMessage\":\"Invalid Token\"}]"}"#).unwrap();
        assert!(s.has_error("1005"));
        let b64 = STANDARD.encode(r#"[{"ErrorCode":"2270","ErrorMessage":"time over"}]"#);
        let s = parse_envelope(&format!(r#"{{"status":"0","ErrorDetails":"{b64}"}}"#)).unwrap();
        assert!(s.has_error("2270"));
        assert!(s.explain().contains("credit note"));

        let ewb = parse_envelope(r#"{"Status":"1","Data":"x","InfoDtls":[{"InfCd":"EWBERR","Desc":[{"ErrorCode":"4019","ErrorMessage":"Provide Transporter ID"}]}]}"#).unwrap();
        assert_eq!(info_messages(&ewb), ["The e-way bill was not generated: IRP error 4019: Provide Transporter ID"]);

        assert!(parse_envelope("<html>Gateway timeout</html>").is_err());
    }

    #[test]
    fn irn_response_fields() {
        let v = json!({
            "AckNo": 112010000002315_u64, "AckDt": "2020-08-05 15:18:00",
            "Irn": "11f8ef701fe294d4a14aad0b12457e62775d0fdc41a0acf05b74fbb2ddc47acb",
            "SignedInvoice": "a.b.c", "SignedQRCode": NIC_SAMPLE_SIGNED_QR, "Status": "ACT",
            "EwbNo": 191008688443_u64, "EwbDt": "2020-08-05 15:18:00", "EwbValidTill": "2020-08-06 23:59:00", "Remarks": null
        });
        let r = parse_irn(&v).unwrap();
        assert_eq!((r.ack_no.as_str(), r.status.as_str(), r.ewb_no.as_deref(), r.remarks), ("112010000002315", "ACT", Some("191008688443"), None));
        assert!(parse_irn(&json!({})).is_err());
        let g = parse_gstin(&json!({"Gstin":"29AWGPV7107B1Z1","TradeName":"VIKAS EXPORTS","LegalName":"VIKASEXPORTS ","AddrBnm":"RAMANAGARA","AddrBno":"562160","AddrFlno":"1st floor","AddrSt":"6th main","AddrLoc":"CHANNAPATNA","StateCode":29,"AddrPncd":562160,"TxpType":"REG","Status":"ACT","BlkStatus":null,"DtReg":"2021-05-03"})).unwrap();
        assert_eq!((g.state_code.as_deref(), g.pin.as_deref(), g.legal_name.as_deref(), g.address.len()), (Some("29"), Some("562160"), Some("VIKASEXPORTS"), 4));
    }

    #[test]
    fn nic_sample_signed_qr_decodes() {
        let q = signed_qr(NIC_SAMPLE_SIGNED_QR).unwrap();
        assert_eq!(q.seller_gstin, "37ARZPT4384Q1MT");
        assert_eq!(q.doc_no, "DD-20200804-9");
        assert_eq!((q.doc_type.as_str(), q.doc_date.as_str(), q.total_invoice_value.as_str(), q.item_count.as_str()), ("INV", "04/08/2020", "7776.3", "3"));
        assert_eq!(q.irn, "11f8ef701fe294d4a14aad0b12457e62775d0fdc41a0acf05b74fbb2ddc47acb");
        assert_eq!(q.issuer.as_deref(), Some("NIC"));
        assert_eq!(qr_text(&format!(" {NIC_SAMPLE_SIGNED_QR}\n")), Some(NIC_SAMPLE_SIGNED_QR));
        assert_eq!(qr_text("not a jwt"), None);
        // NIC's key is not ours: the signature must not verify.
        assert!(!decode_jwt(NIC_SAMPLE_SIGNED_QR).unwrap().verify(&key()).unwrap());
    }

    #[test]
    fn jwt_signatures_verify_with_the_right_key_only() {
        let payload = json!({ "SellerGstin": "27AAPFU0939F1ZV", "Irn": "x" });
        let token = sign_jwt(&payload);
        let jwt = decode_jwt(&token).unwrap();
        assert_eq!(jwt.data().unwrap(), payload);
        assert!(jwt.verify(&parse_public_key(TEST_CERT_PEM).unwrap()).unwrap());
        // Tampered claims.
        let mut parts: Vec<&str> = token.split('.').collect();
        let forged = URL_SAFE_NO_PAD.encode(json!({ "data": "{\"Irn\":\"y\"}", "iss": "NIC" }).to_string());
        parts[1] = &forged;
        assert!(!decode_jwt(&parts.join(".")).unwrap().verify(&key()).unwrap());
        assert!(decode_jwt("a.b").is_err());
    }

    #[test]
    fn cancellation_payload_and_the_24_hour_window() {
        let irn = "a5c12dca80e743321740b001fd70953e8738d109865d28ba4013750f2046f229";
        assert_eq!(cancel_payload(irn, CancelReason::DataEntryMistake, " Wrong entry ").unwrap(), json!({ "Irn": irn, "CnlRsn": "2", "CnlRem": "Wrong entry" }));
        assert!(cancel_payload("short", CancelReason::Other, "").is_err());
        assert!(cancel_payload(irn, CancelReason::Other, &"r".repeat(101)).is_err());
        assert!(cancel_payload(irn, CancelReason::Other, "say \"no\"").is_err());
        assert_eq!(CancelReason::parse("order cancelled"), Some(CancelReason::OrderCancelled));
        assert_eq!(CancelReason::parse("9"), None);

        // 10:00 IST is 04:30 UTC; the window closes 24 hours later.
        let ack = "2026-10-20 10:00:00";
        let at = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        assert_eq!(cancel_deadline(ack), Some(at("2026-10-21T04:30:00Z")));
        assert!(check_cancel_window(ack, at("2026-10-21T04:29:59Z")).is_ok());
        let e = check_cancel_window(ack, at("2026-10-21T04:30:01Z")).unwrap_err();
        assert!(e.contains("21 Oct 2026 10:00 IST") && e.contains("credit note"), "{e}");
        assert!(check_cancel_window("yesterday", at("2026-10-21T00:00:00Z")).is_err());
        assert_eq!(parse_ist("2026-10-20T10:00:00.123"), Some(at("2026-10-20T04:30:00.123Z")));
        // Token: TokenExpiry less ten minutes.
        assert_eq!(token_valid_until(Some("2026-10-20 16:00:00"), at("2026-10-20T04:30:00Z")), at("2026-10-20T10:20:00Z"));
        assert_eq!(token_valid_until(None, at("2026-10-20T04:30:00Z")), at("2026-10-20T10:20:00Z"));
    }

    #[test]
    fn ewb_by_irn_payloads() {
        let irn = "7f8ea31dacf78a197d42b04f0c8dd511fe3649f1ef9062146d053ff26dafe2e3";
        let road = EwbByIrn {
            distance_km: 100,
            trans_mode: Some(TransMode::Road),
            transporter_id: "12awgpv7107b1z1".into(),
            transporter_name: "trans name".into(),
            vehicle_no: "KA 12 ER 1234".into(),
            ..EwbByIrn::default()
        };
        assert_eq!(
            ewb_payload(irn, &road).unwrap(),
            json!({ "Irn": irn, "Distance": 100, "TransMode": "1", "TransId": "12AWGPV7107B1Z1", "TransName": "trans name", "VehNo": "KA12ER1234", "VehType": "R" })
        );
        let rail = EwbByIrn { trans_mode: Some(TransMode::Rail), trans_doc_no: "TRAN/DOC/11".into(), trans_doc_date: "2026-10-01".into(), ..EwbByIrn::default() };
        assert_eq!(ewb_payload(irn, &rail).unwrap()["TransDocDt"], "01/10/2026");
        // Part A only.
        let part_a = EwbByIrn { transporter_id: "12AWGPV7107B1Z1".into(), ..EwbByIrn::default() };
        assert_eq!(ewb_payload(irn, &part_a).unwrap(), json!({ "Irn": irn, "Distance": 0, "TransId": "12AWGPV7107B1Z1" }));
        let e = ewb_payload(irn, &EwbByIrn { trans_mode: Some(TransMode::Air), vehicle_no: "KA12".into(), distance_km: 5000, ..EwbByIrn::default() }).unwrap_err().to_string();
        assert!(e.contains("4000") && e.contains("rail and air") && e.contains("only for road"), "{e}");
        assert!(ewb_payload(irn, &EwbByIrn::default()).is_err());
        assert!(ewb_payload(irn, &EwbByIrn { trans_mode: Some(TransMode::Road), ..EwbByIrn::default() }).is_err());
        let e = parse_ewb(&json!({"EwbNo":111008683051_u64,"EwbDt":"2020-04-24 11:28:00","EwbValidTill":"2020-04-26 23:59:00","Remarks":"Pin-Pin calc distance: 997KM"})).unwrap();
        assert_eq!((e.ewb_no.as_str(), e.remarks.as_deref()), ("111008683051", Some("Pin-Pin calc distance: 997KM")));
    }

    #[test]
    fn error_codes_have_plain_messages() {
        assert_eq!(explain("2150", "Duplicate IRN"), "an IRN was already generated for this document number (IRP 2150: Duplicate IRN)");
        assert!(explain("1013", "").contains("public key"));
        assert_eq!(explain("9999", "Something"), "IRP error 9999: Something");
        assert_eq!(explain("", "Server busy"), "IRP: Server busy");
        assert!(endpoint("nic-sandbox").is_some_and(|e| !e.production));
        assert!(endpoint("nope").is_none());
    }
}
