//! IRP connector (India e-invoicing): sign in to an Invoice Registration
//! Portal, register a Sales or Credit Note voucher for an IRN, fetch,
//! cancel, and generate an e-way bill by IRN. The message shapes and the
//! crypto are in `khata_in::irp`; the INV-01 JSON is the one the offline
//! export builds (`crate::einvoice::einvoice_json`). See `docs/einvoice.md`.
//!
//! Where things live:
//! - **Credentials** (client id and secret, API user name and password, the
//!   IRP public key, NIC's verification certificate) are in
//!   `irp-credentials.json` in the app data directory, keyed by GSTIN and
//!   readable only by this OS user (mode 0600 on Unix; on Windows the
//!   per-user AppData folder). They are never in the company file, the
//!   audit trail, sync or exports, never sent back to the webview, and never
//!   written to logs or error messages.
//! - **The session** (AuthToken and the decrypted SEK) is kept in memory
//!   only, until shortly before `TokenExpiry`.
//! - **The IRP's answer** (IRN, Ack No and date, SignedInvoice,
//!   SignedQRCode, status, e-way bill, the decrypted responses verbatim) is
//!   a shared record of the company under `irp.irn.<voucher id>`: outside
//!   the books, synced with co-workers and the accountant so every copy
//!   prints the IRN (`docs/sync-settings.md`). [`print_info`] and [`print_block_html`]
//!   give the invoice print what it needs.
//!
//! Network calls run without the company lock: the request is built under
//! the lock, sent, and the answer stored under the lock again.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Local, Utc};
use khata_core::{UserRole, VoucherId, VoucherKind};
use khata_in::irp::{self, CancelReason, Envelope, EwbByIrn, GstinDetails, IrnData, SignedQr};
use khata_store::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::{ApiResult, Slot, err, open_ref, parse_uuid};
use crate::einvoice::BuyerDetails;

pub const CREDENTIALS_FILE: &str = "irp-credentials.json";
const RECORD_PREFIX: &str = "irp.irn.";

// ------------------------------------------------------------ credentials

/// One GSTIN's API access. Secrets never leave this module except in the
/// request headers and the encrypted sign-in.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Credentials {
    /// An id from `khata_in::irp::ENDPOINTS`.
    pub endpoint: String,
    /// The API base URL for `custom` (a GSP or a private IRP).
    pub base_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub user_name: String,
    pub password: String,
    /// The IRP's public key for the sign-in (PEM or Base64).
    pub public_key_pem: String,
    /// NIC's certificate or key for checking SignedQRCode and
    /// SignedInvoice; optional.
    pub verify_cert_pem: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("endpoint", &self.endpoint)
            .field("base_url", &self.base_url)
            .field("client_id", &"<redacted>")
            .field("client_secret", &"<redacted>")
            .field("user_name", &"<redacted>")
            .field("password", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl Credentials {
    fn base(&self) -> ApiResult<String> {
        let url = match irp::endpoint(&self.endpoint) {
            Some(e) if e.id != "custom" => e.base_url.to_owned(),
            Some(_) => self.base_url.clone(),
            None => return Err("choose the IRP in IRP settings".into()),
        };
        check_base_url(&url)
    }

    fn production(&self) -> bool {
        irp::endpoint(&self.endpoint).is_none_or(|e| e.production)
    }

    fn complete(&self) -> ApiResult<()> {
        let missing: Vec<&str> = [
            ("the client id", self.client_id.is_empty()),
            ("the client secret", self.client_secret.is_empty()),
            ("the API user name", self.user_name.is_empty()),
            ("the API password", self.password.is_empty()),
            ("the IRP public key", self.public_key_pem.is_empty()),
        ]
        .into_iter()
        .filter_map(|(what, gone)| gone.then_some(what))
        .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!("IRP settings are missing {}", missing.join(", ")))
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Vault {
    version: u32,
    by_gstin: BTreeMap<String, Credentials>,
}

fn vault_path(dir: &Path) -> PathBuf {
    dir.join(CREDENTIALS_FILE)
}

fn load_vault(dir: &Path) -> ApiResult<Vault> {
    match std::fs::read_to_string(vault_path(dir)) {
        Ok(s) => serde_json::from_str(&s).map_err(|_| format!("{CREDENTIALS_FILE} is damaged; enter the IRP settings again")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vault { version: 1, ..Vault::default() }),
        Err(e) => Err(format!("cannot read the IRP settings: {e}")),
    }
}

/// Write the vault readable by this user only: a temporary file created
/// with mode 0600, then renamed over the old one.
fn save_vault(dir: &Path, vault: &Vault) -> ApiResult<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create the app data folder: {e}"))?;
    let json = serde_json::to_vec_pretty(vault).map_err(err)?;
    let tmp = dir.join(format!("{CREDENTIALS_FILE}.tmp"));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(&tmp)
        .and_then(|mut f| f.write_all(&json).and_then(|_| f.sync_all()))
        .map_err(|e| format!("cannot save the IRP settings: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, vault_path(dir)).map_err(|e| format!("cannot save the IRP settings: {e}"))
}

/// HTTPS only; plain HTTP just for a mock IRP on this computer.
fn check_base_url(url: &str) -> ApiResult<String> {
    let url = url.trim().trim_end_matches('/');
    let local = ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"].iter().any(|p| url.starts_with(p));
    if url.is_empty() {
        return Err("enter the IRP or GSP API base URL".into());
    }
    if !(url.starts_with("https://") || local) || url.contains(char::is_whitespace) || url.contains('?') || url.contains('#') {
        return Err("the IRP base URL must start with https:// (for example https://einv-apisandbox.nic.in)".into());
    }
    Ok(url.to_owned())
}

fn company_gstin(slot: &Slot) -> ApiResult<String> {
    let book = open_ref(slot)?;
    if book.country() != Some("IN") {
        return Err("IRP e-invoicing is for companies in India".into());
    }
    book.profile()
        .and_then(|p| p.tax_id.clone())
        .map(|g| g.trim().to_ascii_uppercase())
        .filter(|g| !g.is_empty())
        .ok_or_else(|| "set the company's GSTIN first (Company → Tax registration)".into())
}

/// What the settings screen may see: no secrets, only whether they are set.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IrpSettingsDto {
    pub gstin: String,
    pub endpoint: String,
    pub base_url: String,
    pub endpoints: Vec<irp::Endpoint>,
    pub production: bool,
    pub client_id: String,
    pub user_name: String,
    pub has_client_secret: bool,
    pub has_password: bool,
    /// "RSA 2048-bit" when a usable key is stored.
    pub public_key: Option<String>,
    pub verify_cert: Option<String>,
    /// Signed in now (a cached session).
    pub session_active: bool,
    /// Where the credentials file is.
    pub stored_in: String,
}

/// From the settings screen. An empty secret keeps the stored one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct IrpSettingsInput {
    pub endpoint: String,
    pub base_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub user_name: String,
    pub password: String,
    pub public_key_pem: String,
    pub verify_cert_pem: String,
    /// Remove the stored verification certificate.
    pub clear_verify_cert: bool,
}

fn key_label(pem: &str) -> Option<String> {
    (!pem.is_empty()).then(|| irp::parse_public_key(pem).map(|k| format!("RSA {}-bit", irp::key_bits(&k))).unwrap_or_else(|e| e.to_string()))
}

fn settings_dto(dir: &Path, gstin: &str, c: &Credentials) -> IrpSettingsDto {
    IrpSettingsDto {
        gstin: gstin.to_owned(),
        endpoint: c.endpoint.clone(),
        base_url: c.base().unwrap_or_else(|_| c.base_url.clone()),
        endpoints: irp::ENDPOINTS.to_vec(),
        production: c.production(),
        client_id: c.client_id.clone(),
        user_name: c.user_name.clone(),
        has_client_secret: !c.client_secret.is_empty(),
        has_password: !c.password.is_empty(),
        public_key: key_label(&c.public_key_pem),
        verify_cert: key_label(&c.verify_cert_pem),
        session_active: c.base().is_ok_and(|b| cached_session(&session_key(&b, gstin, c)).is_some()),
        stored_in: vault_path(dir).display().to_string(),
    }
}

pub fn settings(dir: &Path, slot: &Slot) -> ApiResult<IrpSettingsDto> {
    let gstin = company_gstin(slot)?;
    let vault = load_vault(dir)?;
    let c = vault.by_gstin.get(&gstin).cloned().unwrap_or_else(|| Credentials { endpoint: "nic-sandbox".into(), ..Credentials::default() });
    Ok(settings_dto(dir, &gstin, &c))
}

pub fn set_settings(dir: &Path, slot: &Slot, input: &IrpSettingsInput) -> ApiResult<IrpSettingsDto> {
    crate::api::require(open_ref(slot)?, UserRole::Owner)?;
    let gstin = company_gstin(slot)?;
    let mut vault = load_vault(dir)?;
    let mut c = vault.by_gstin.get(&gstin).cloned().unwrap_or_default();
    let old = c.clone();
    let endpoint = irp::endpoint(input.endpoint.trim()).ok_or("choose the IRP: NIC sandbox, NIC IRP 1, NIC IRP 2 or another IRP/GSP")?;
    c.endpoint = endpoint.id.to_owned();
    c.base_url = if endpoint.id == "custom" { check_base_url(&input.base_url)? } else { String::new() };
    let keep = |new: &str, old: &str| if new.trim().is_empty() { old.to_owned() } else { new.trim().to_owned() };
    c.client_id = input.client_id.trim().to_owned();
    c.user_name = input.user_name.trim().to_owned();
    c.client_secret = keep(&input.client_secret, &c.client_secret);
    // Passwords may have meaningful spaces at the ends: keep them as typed.
    if !input.password.is_empty() {
        c.password = input.password.clone();
    }
    if !input.public_key_pem.trim().is_empty() {
        irp::parse_public_key(&input.public_key_pem).map_err(err)?;
        c.public_key_pem = input.public_key_pem.trim().to_owned();
    }
    if input.clear_verify_cert {
        c.verify_cert_pem.clear();
    } else if !input.verify_cert_pem.trim().is_empty() {
        irp::parse_public_key(&input.verify_cert_pem).map_err(|e| format!("verification certificate: {e}"))?;
        c.verify_cert_pem = input.verify_cert_pem.trim().to_owned();
    }
    for (what, v) in [("client id", &c.client_id), ("client secret", &c.client_secret), ("user name", &c.user_name), ("password", &c.password)] {
        if v.chars().any(|ch| ch.is_control() || !ch.is_ascii()) {
            return Err(format!("the {what} can only have plain ASCII characters"));
        }
    }
    if c != old
        && let Ok(b) = old.base()
    {
        forget_session(&session_key(&b, &gstin, &old));
    }
    vault.version = 1;
    vault.by_gstin.insert(gstin.clone(), c.clone());
    save_vault(dir, &vault)?;
    Ok(settings_dto(dir, &gstin, &c))
}

fn credentials_for(dir: &Path, gstin: &str) -> ApiResult<Credentials> {
    let c = load_vault(dir)?
        .by_gstin
        .remove(gstin)
        .ok_or_else(|| format!("IRP API access for {gstin} isn't set up: open e-Invoice → IRP settings"))?;
    c.complete()?;
    Ok(c)
}

// ------------------------------------------------------------ sessions

#[derive(Clone)]
struct Session {
    auth_token: String,
    sek: [u8; 32],
    valid_until: DateTime<Utc>,
}

fn sessions() -> &'static Mutex<HashMap<String, Session>> {
    static S: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn session_key(base: &str, gstin: &str, c: &Credentials) -> String {
    format!("{base}|{gstin}|{}|{}", c.client_id, c.user_name)
}

fn cached_session(key: &str) -> Option<Session> {
    let map = sessions().lock().ok()?;
    map.get(key).filter(|s| s.valid_until > Utc::now()).cloned()
}

fn forget_session(key: &str) {
    if let Ok(mut map) = sessions().lock() {
        map.remove(key);
    }
}

// ------------------------------------------------------------ HTTP

enum Method {
    Get,
    Post,
}

/// A decrypted success, or the IRP's refusal.
enum Outcome {
    Ok { data: Value, env: Envelope },
    Refused(Envelope),
}

struct Client {
    base: String,
    gstin: String,
    creds: Credentials,
    key: String,
    agent: ureq::Agent,
}

fn net(e: ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_) => "the IRP did not answer in time; try again in a minute".into(),
        ureq::Error::HostNotFound => "couldn't find the IRP server; check the internet connection and the base URL".into(),
        ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
            "couldn't connect to the IRP (production access is allowed only from the static IPs whitelisted with NIC)".into()
        }
        other => format!("couldn't reach the IRP ({other})"),
    }
}

impl Client {
    fn new(creds: Credentials, gstin: &str) -> ApiResult<Client> {
        let base = creds.base()?;
        let key = session_key(&base, gstin, &creds);
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(StdDuration::from_secs(15)))
            .timeout_global(Some(StdDuration::from_secs(90)))
            .build()
            .into();
        Ok(Client { base, gstin: gstin.to_owned(), creds, key, agent })
    }

    /// The documented header names (`client_id`, `client_secret`) and the
    /// ones NIC's Java and C# samples send (`client-id`, `client-secret`):
    /// which one the gateway reads is [UNCERTAIN], so both are sent.
    fn headers<B>(&self, mut req: ureq::RequestBuilder<B>, token: Option<&str>) -> ureq::RequestBuilder<B> {
        req = req
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("client_id", &self.creds.client_id)
            .header("client_secret", &self.creds.client_secret)
            .header("client-id", &self.creds.client_id)
            .header("client-secret", &self.creds.client_secret)
            .header("Gstin", &self.gstin);
        if let Some(t) = token {
            req = req.header("user_name", &self.creds.user_name).header("AuthToken", t);
        }
        req
    }

    fn read(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> ApiResult<Envelope> {
        let mut resp = resp.map_err(net)?;
        let status = resp.status().as_u16();
        let body = resp.body_mut().read_to_string().map_err(|e| format!("couldn't read the IRP's answer: {e}"))?;
        match irp::parse_envelope(&body) {
            Ok(env) => Ok(env),
            Err(_) if !(200..300).contains(&status) => Err(format!("the IRP answered HTTP {status}: {}", irp::preview(&body))),
            Err(e) => Err(e.to_string()),
        }
    }

    fn authenticate(&self, force: bool) -> ApiResult<Session> {
        let key = irp::parse_public_key(&self.creds.public_key_pem).map_err(err)?;
        let app_key = irp::new_app_key();
        let body = irp::auth_request(&key, &self.creds.user_name, &self.creds.password, &app_key, force).map_err(err)?;
        let req = self.headers(self.agent.post(format!("{}{}", self.base, irp::AUTH_PATH)), None);
        let env = Self::read(req.send(body.as_str()))?;
        if !env.ok {
            return Err(format!("IRP sign-in failed: {}", env.explain()));
        }
        let auth = irp::parse_auth(&env).map_err(err)?;
        let sek = irp::decrypt_sek(&auth.sek, &app_key).map_err(|_| "IRP sign-in failed: the session key could not be decrypted".to_owned())?;
        let s = Session { auth_token: auth.auth_token, sek, valid_until: irp::token_valid_until(auth.token_expiry.as_deref(), Utc::now()) };
        if let Ok(mut map) = sessions().lock() {
            map.insert(self.key.clone(), s.clone());
        }
        Ok(s)
    }

    fn session(&self) -> ApiResult<Session> {
        match cached_session(&self.key) {
            Some(s) => Ok(s),
            None => self.authenticate(false),
        }
    }

    fn send(&self, s: &Session, method: &Method, path: &str, payload: Option<&Value>) -> ApiResult<Envelope> {
        let url = format!("{}{path}", self.base);
        match method {
            Method::Post => {
                let body = irp::encrypted_request(&s.sek, payload.unwrap_or(&Value::Null));
                Self::read(self.headers(self.agent.post(url), Some(&s.auth_token)).send(body.as_str()))
            }
            Method::Get => Self::read(self.headers(self.agent.get(url), Some(&s.auth_token)).call()),
        }
    }

    /// One API call, signing in first when needed and once more when the
    /// IRP says the token is no longer valid (1005).
    fn call(&self, method: Method, path: &str, payload: Option<&Value>) -> ApiResult<Outcome> {
        let mut s = self.session()?;
        let mut env = self.send(&s, &method, path, payload)?;
        if !env.ok && env.has_error("1005") {
            forget_session(&self.key);
            s = self.authenticate(false)?;
            env = self.send(&s, &method, path, payload)?;
        }
        if !env.ok {
            return Ok(Outcome::Refused(env));
        }
        let data = irp::decrypt_data(&s.sek, env.encrypted_data().map_err(err)?).map_err(err)?;
        Ok(Outcome::Ok { data, env })
    }

    fn irn_details(&self, irn: &str) -> ApiResult<Result<(Value, Envelope), Envelope>> {
        Ok(match self.call(Method::Get, &format!("{}{irn}", irp::IRN_PATH), None)? {
            Outcome::Ok { data, env } => Ok((data, env)),
            Outcome::Refused(env) => Err(env),
        })
    }

    fn by_doc(&self, doc: &DocKey) -> ApiResult<Result<(Value, Envelope), Envelope>> {
        let path = format!("{}?doctype={}&docnum={}&docdate={}", irp::BY_DOC_PATH, query(&doc.typ), query(&doc.no), query(&doc.date));
        Ok(match self.call(Method::Get, &path, None)? {
            Outcome::Ok { data, env } => Ok((data, env)),
            Outcome::Refused(env) => Err(env),
        })
    }
}

fn query(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

// ------------------------------------------------------------ records

/// The IRP's answer for one voucher, a shared record of the company.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IrnRecord {
    pub voucher_id: String,
    pub voucher_number: String,
    /// INV, CRN or DBN; the IRP's document number and date (dd/mm/yyyy).
    pub doc_type: String,
    pub doc_no: String,
    pub doc_date: String,
    pub gstin: String,
    pub endpoint: String,
    pub irn: String,
    pub ack_no: String,
    pub ack_dt: String,
    /// ACT, CNL or REJ.
    pub status: String,
    pub signed_invoice: String,
    pub signed_qr_code: String,
    pub ewb_no: Option<String>,
    pub ewb_dt: Option<String>,
    pub ewb_valid_till: Option<String>,
    pub remarks: Option<String>,
    pub cancel_date: Option<String>,
    pub cancel_reason: Option<String>,
    pub cancel_remark: Option<String>,
    /// DocDtls and ValDtls as registered, to notice later edits.
    pub basis: Value,
    pub notes: Vec<String>,
    /// Local time of the last answer.
    pub updated_at: String,
    /// The decrypted IRP answers, verbatim: `{api, at, data}`.
    pub responses: Vec<Value>,
}

impl IrnRecord {
    fn apply(&mut self, d: &IrnData) {
        self.irn = d.irn.clone();
        if !d.ack_no.is_empty() {
            self.ack_no = d.ack_no.clone();
        }
        if !d.ack_dt.is_empty() {
            self.ack_dt = d.ack_dt.clone();
        }
        if !d.signed_invoice.is_empty() {
            self.signed_invoice = d.signed_invoice.clone();
        }
        if !d.signed_qr_code.is_empty() {
            self.signed_qr_code = d.signed_qr_code.clone();
        }
        self.status = d.status.clone();
        self.ewb_no = d.ewb_no.clone().or(self.ewb_no.take());
        self.ewb_dt = d.ewb_dt.clone().or(self.ewb_dt.take());
        self.ewb_valid_till = d.ewb_valid_till.clone().or(self.ewb_valid_till.take());
        self.remarks = d.remarks.clone().or(self.remarks.take());
    }

    fn log(&mut self, api: &str, data: &Value) {
        let at = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.updated_at = at.clone();
        self.responses.push(json!({ "api": api, "at": at, "data": data }));
    }
}

fn record_key(voucher_id: &str) -> String {
    format!("{RECORD_PREFIX}{voucher_id}")
}

fn read_record(store: &Store, voucher_id: &str) -> ApiResult<Option<IrnRecord>> {
    match store.record(&record_key(voucher_id)).map_err(err)? {
        Some(s) => serde_json::from_str(&s).map(Some).map_err(|_| "the stored IRN record is damaged".into()),
        None => Ok(None),
    }
}

fn write_record(store: &Store, r: &IrnRecord) -> ApiResult<()> {
    let s = serde_json::to_string(r).map_err(err)?;
    // A shared record of the company: co-workers see the IRN too
    // (`docs/sync-settings.md`). The API credentials stay in app data.
    store.set_record(&record_key(&r.voucher_id), Some(&s), &crate::api::os_user()).map(|_| ()).map_err(err)
}

fn store_ref(slot: &Slot) -> ApiResult<&Store> {
    slot.as_ref().map(|(s, _)| s).ok_or_else(|| "no company is open".to_owned())
}

/// What the result screen shows.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IrnDto {
    #[serde(flatten)]
    pub record: IrnRecord,
    /// The signed QR code's fields, decoded (no signature check needed).
    pub qr: Option<SignedQr>,
    /// "verified", "invalid", or "not_checked" (no NIC certificate in IRP
    /// settings).
    pub signature: String,
    /// The end of the 24-hour cancellation window, IST, while it is open.
    pub cancel_until: Option<String>,
    /// The QR code as SVG, for the screen.
    pub qr_svg: Option<String>,
    /// What happened in this call, and warnings.
    pub messages: Vec<String>,
}

fn signature_state(r: &IrnRecord, verify: Option<&str>) -> String {
    let Some(pem) = verify.filter(|p| !p.trim().is_empty()) else {
        return "not_checked".into();
    };
    let Ok(key) = irp::parse_public_key(pem) else {
        return "not_checked".into();
    };
    let ok = |t: &str| t.is_empty() || irp::decode_jwt(t).and_then(|j| j.verify(&key)).unwrap_or(false);
    if r.signed_qr_code.is_empty() && r.signed_invoice.is_empty() {
        "not_checked".into()
    } else if ok(&r.signed_qr_code) && ok(&r.signed_invoice) {
        "verified".into()
    } else {
        "invalid".into()
    }
}

fn qr_symbol(signed_qr_code: &str) -> Option<khata_print::QrSymbol> {
    let text = irp::qr_text(signed_qr_code)?;
    let m = khata_sa::qr::matrix(text).ok()?;
    Some(khata_print::QrSymbol { width: m.width, dark: m.dark })
}

fn dto(r: IrnRecord, verify: Option<&str>, messages: Vec<String>, now: DateTime<Utc>) -> IrnDto {
    let qr = irp::signed_qr(&r.signed_qr_code).ok();
    let open = if r.status == "ACT" { irp::check_cancel_window(&r.ack_dt, now).ok() } else { None };
    let cancel_until = open.map(|d| d.with_timezone(&irp::ist()).format("%d %b %Y %H:%M IST").to_string());
    IrnDto {
        signature: signature_state(&r, verify),
        qr_svg: qr_symbol(&r.signed_qr_code).map(|q| khata_print::qr_svg(&q, 30)),
        qr,
        cancel_until,
        messages,
        record: r,
    }
}

fn verify_pem(dir: &Path, gstin: &str) -> Option<String> {
    load_vault(dir).ok()?.by_gstin.remove(gstin).map(|c| c.verify_cert_pem).filter(|p| !p.is_empty())
}

// ------------------------------------------------------------ preparing

#[derive(Debug, Clone)]
struct DocKey {
    typ: String,
    no: String,
    date: String,
}

fn lock(company: &Mutex<Slot>) -> ApiResult<MutexGuard<'_, Slot>> {
    company.lock().map_err(|_| "company state is poisoned".to_owned())
}

fn e_invoiceable(slot: &Slot, voucher_id: &str) -> ApiResult<(String, VoucherKind)> {
    let book = open_ref(slot)?;
    let v = book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or("unknown voucher")?;
    if !matches!(v.kind, VoucherKind::Sales | VoucherKind::CreditNote) {
        return Err("IRNs are generated here for Sales and Credit Note vouchers".into());
    }
    Ok((v.number.clone(), v.kind))
}

fn basis(json: &Value) -> Value {
    json!({ "DocDtls": json["DocDtls"], "ValDtls": json["ValDtls"] })
}

fn doc_key(json: &Value) -> DocKey {
    let s = |k: &str| json["DocDtls"][k].as_str().unwrap_or_default().to_owned();
    DocKey { typ: s("Typ"), no: s("No"), date: s("Dt") }
}

/// The voucher's record, or why it can't be used for `what`.
fn active_record(slot: &Slot, voucher_id: &str, what: &str) -> ApiResult<IrnRecord> {
    let r = read_record(store_ref(slot)?, voucher_id)?.ok_or_else(|| format!("this voucher has no IRN yet; generate it before {what}"))?;
    if r.irn.is_empty() {
        return Err("the stored IRN record has no IRN".into());
    }
    Ok(r)
}

/// Store an updated record under the lock, after the network call.
fn save(company: &Mutex<Slot>, r: &IrnRecord) -> ApiResult<()> {
    let slot = lock(company)?;
    let book = open_ref(&slot)?;
    let id = VoucherId(parse_uuid(&r.voucher_id, "voucher")?);
    if book.voucher(id).is_none() {
        return Err(format!(
            "the company was changed or closed while the IRP answered; IRN {} (Ack {}) was not saved. Open the company and use Get IRN details",
            r.irn, r.ack_no
        ));
    }
    write_record(store_ref(&slot)?, r)
}

// ------------------------------------------------------------ operations

/// The stored answer for a voucher, without calling the IRP. Warns when the
/// voucher changed after its IRN was generated.
pub fn record(dir: &Path, slot: &Slot, voucher_id: &str) -> ApiResult<Option<IrnDto>> {
    let Some(r) = read_record(store_ref(slot)?, voucher_id)? else {
        return Ok(None);
    };
    let mut messages = Vec::new();
    let hsn = open_ref(slot)?.profile().and_then(|p| p.hsn_digits).map_or(4, usize::from);
    if r.status == "ACT"
        && !r.basis.is_null()
        && let Ok(json) = crate::einvoice::einvoice_json(slot, voucher_id, &BuyerDetails::default(), hsn)
        && let Ok(v) = serde_json::from_str::<Value>(&json)
        && basis(&v) != r.basis
    {
        messages.push(
            "This voucher was changed after its IRN was generated; the IRP still holds the original. Cancel the IRN (within 24 hours) and register again, or issue a credit or debit note.".into(),
        );
    }
    let verify = verify_pem(dir, &r.gstin);
    Ok(Some(dto(r, verify.as_deref(), messages, Utc::now())))
}

/// Register a Sales or Credit Note voucher with the IRP and keep its IRN.
/// A voucher that already has an active IRN is not sent again. On a
/// duplicate (2150) the existing IRN's details are fetched and kept.
pub fn generate(dir: &Path, company: &Mutex<Slot>, voucher_id: &str, buyer: &BuyerDetails) -> ApiResult<IrnDto> {
    let (json, gstin, number) = {
        let slot = lock(company)?;
        crate::api::require(open_ref(&slot)?, UserRole::Accountant)?;
        let (number, _) = e_invoiceable(&slot, voucher_id)?;
        let book = open_ref(&slot)?;
        let v = book.voucher(VoucherId(parse_uuid(voucher_id, "voucher")?)).ok_or("unknown voucher")?;
        if v.cancelled {
            return Err(format!("{number} is cancelled in the books; an IRN is not generated for it"));
        }
        if let Some(r) = read_record(store_ref(&slot)?, voucher_id)? {
            match r.status.as_str() {
                "ACT" if !r.signed_qr_code.is_empty() => {
                    let verify = verify_pem(dir, &r.gstin);
                    return Ok(dto(r, verify.as_deref(), vec!["This voucher already has an IRN; nothing was sent.".into()], Utc::now()));
                }
                "CNL" => {
                    return Err(
                        "this voucher's IRN was cancelled, and the IRP never accepts a cancelled document number again. Give the invoice a new number (alter the voucher) and register that"
                            .into(),
                    );
                }
                _ => {}
            }
        }
        let hsn = book.profile().and_then(|p| p.hsn_digits).map_or(4, usize::from);
        let json: Value = serde_json::from_str(&crate::einvoice::einvoice_json(&slot, voucher_id, buyer, hsn)?).map_err(err)?;
        let gstin = json["SellerDtls"]["Gstin"].as_str().unwrap_or_default().to_owned();
        (json, gstin, number)
    };
    let creds = credentials_for(dir, &gstin)?;
    let verify = (!creds.verify_cert_pem.is_empty()).then(|| creds.verify_cert_pem.clone());
    let endpoint = creds.endpoint.clone();
    let client = Client::new(creds, &gstin)?;
    let doc = doc_key(&json);
    let mut r = IrnRecord {
        voucher_id: voucher_id.to_owned(),
        voucher_number: number,
        doc_type: doc.typ.clone(),
        doc_no: doc.no.clone(),
        doc_date: doc.date.clone(),
        gstin,
        endpoint,
        basis: basis(&json),
        ..IrnRecord::default()
    };
    let mut messages = Vec::new();
    match client.call(Method::Post, irp::GENERATE_PATH, Some(&json))? {
        Outcome::Ok { data, env } => {
            r.apply(&irp::parse_irn(&data).map_err(err)?);
            r.log("generate", &data);
            r.notes = irp::info_messages(&env);
            messages.push(format!("IRN generated: Ack No {} at {}.", r.ack_no, r.ack_dt));
        }
        Outcome::Refused(env) if env.has_error("2150") => {
            // Registered before (perhaps the first answer was lost): fetch it.
            let dup = irp::duplicate(&env);
            let fetched = match &dup {
                Some(d) => client.irn_details(&d.irn)?,
                None => client.by_doc(&doc)?,
            };
            let fetched = match fetched {
                Ok(f) => Ok(f),
                Err(_) if dup.is_some() => client.by_doc(&doc)?,
                Err(e) => Err(e),
            };
            match fetched {
                Ok((data, _)) => {
                    r.apply(&irp::parse_irn(&data).map_err(err)?);
                    r.log("details", &data);
                    messages.push("The IRP already had an IRN for this document number; its details were fetched and saved.".into());
                }
                Err(e) => {
                    let Some(d) = dup else {
                        return Err(format!("{}\nThe existing IRN could not be fetched: {}", env.explain(), e.explain()));
                    };
                    r.irn = d.irn.clone();
                    r.ack_no = d.ack_no.clone().unwrap_or_default();
                    r.ack_dt = d.ack_dt.clone().unwrap_or_default();
                    r.status = "ACT".into();
                    r.log("generate-duplicate", &json!({ "Irn": d.irn, "AckNo": d.ack_no, "AckDt": d.ack_dt }));
                    let note = format!(
                        "The IRP already had IRN {} for this document, but its signed QR code could not be fetched ({}). Download the e-invoice from the IRP portal for the QR code.",
                        d.irn,
                        e.explain()
                    );
                    r.notes.push(note.clone());
                    messages.push(note);
                }
            }
        }
        Outcome::Refused(env) if env.has_error("2295") => {
            let where_ = irp::duplicate(&env).map(|d| format!(" (IRN {}, {})", d.irn, d.other_irp.unwrap_or_else(|| "another IRP".into()))).unwrap_or_default();
            return Err(format!(
                "another IRP already registered this document{where_}. Fetch the e-invoice from that IRP; it is not registered again here"
            ));
        }
        Outcome::Refused(env) => return Err(env.explain()),
    }
    save(company, &r)?;
    Ok(dto(r, verify.as_deref(), messages, Utc::now()))
}

/// Fetch the IRN's details again (status, signed QR, e-way bill). Without a
/// stored IRN, look the document up by type, number and date.
pub fn details(dir: &Path, company: &Mutex<Slot>, voucher_id: &str) -> ApiResult<IrnDto> {
    let (mut r, doc) = {
        let slot = lock(company)?;
        let (number, _) = e_invoiceable(&slot, voucher_id)?;
        match read_record(store_ref(&slot)?, voucher_id)? {
            Some(r) if !r.irn.is_empty() => {
                let doc = DocKey { typ: r.doc_type.clone(), no: r.doc_no.clone(), date: r.doc_date.clone() };
                (r, doc)
            }
            _ => {
                let hsn = open_ref(&slot)?.profile().and_then(|p| p.hsn_digits).map_or(4, usize::from);
                let json: Value = serde_json::from_str(&crate::einvoice::einvoice_json(&slot, voucher_id, &BuyerDetails::default(), hsn)?).map_err(err)?;
                let doc = doc_key(&json);
                let r = IrnRecord {
                    voucher_id: voucher_id.to_owned(),
                    voucher_number: number,
                    doc_type: doc.typ.clone(),
                    doc_no: doc.no.clone(),
                    doc_date: doc.date.clone(),
                    gstin: json["SellerDtls"]["Gstin"].as_str().unwrap_or_default().to_owned(),
                    basis: basis(&json),
                    ..IrnRecord::default()
                };
                (r, doc)
            }
        }
    };
    let creds = credentials_for(dir, &r.gstin)?;
    let verify = (!creds.verify_cert_pem.is_empty()).then(|| creds.verify_cert_pem.clone());
    if r.endpoint.is_empty() {
        r.endpoint = creds.endpoint.clone();
    }
    let client = Client::new(creds, &r.gstin)?;
    let fetched = if r.irn.is_empty() { client.by_doc(&doc)? } else { client.irn_details(&r.irn)? };
    let (data, _) = fetched.map_err(|env| env.explain())?;
    r.apply(&irp::parse_irn(&data).map_err(err)?);
    r.log("details", &data);
    save(company, &r)?;
    let status = match r.status.as_str() {
        "CNL" => "cancelled",
        "REJ" => "rejected",
        _ => "active",
    };
    Ok(dto(r, verify.as_deref(), vec![format!("IRN details fetched: {status}.")], Utc::now()))
}

/// Cancel the voucher's IRN: whole document, within 24 hours of its
/// acknowledgement, and not while an e-way bill is active. The voucher in
/// the books is not changed.
pub fn cancel(dir: &Path, company: &Mutex<Slot>, voucher_id: &str, reason: &str, remark: &str) -> ApiResult<IrnDto> {
    cancel_at(dir, company, voucher_id, reason, remark, Utc::now())
}

fn cancel_at(dir: &Path, company: &Mutex<Slot>, voucher_id: &str, reason: &str, remark: &str, now: DateTime<Utc>) -> ApiResult<IrnDto> {
    let reason = CancelReason::parse(reason).ok_or("choose a reason: 1 duplicate, 2 data entry mistake, 3 order cancelled, 4 other")?;
    let (mut r, payload) = {
        let slot = lock(company)?;
        crate::api::require(open_ref(&slot)?, UserRole::Accountant)?;
        e_invoiceable(&slot, voucher_id)?;
        let r = active_record(&slot, voucher_id, "cancelling")?;
        if r.status == "CNL" {
            return Err("this IRN is already cancelled".into());
        }
        irp::check_cancel_window(&r.ack_dt, now)?;
        if r.ewb_no.is_some() {
            return Err(format!(
                "e-way bill {} was generated for this IRN; cancel the e-way bill on the e-way bill portal first, then the IRN",
                r.ewb_no.as_deref().unwrap_or_default()
            ));
        }
        let payload = irp::cancel_payload(&r.irn, reason, remark).map_err(err)?;
        (r, payload)
    };
    let creds = credentials_for(dir, &r.gstin)?;
    let verify = (!creds.verify_cert_pem.is_empty()).then(|| creds.verify_cert_pem.clone());
    let client = Client::new(creds, &r.gstin)?;
    match client.call(Method::Post, irp::CANCEL_PATH, Some(&payload))? {
        Outcome::Ok { data, .. } => {
            let (_, date) = irp::parse_cancel(&data).map_err(err)?;
            r.status = "CNL".into();
            r.cancel_date = Some(date).filter(|d| !d.is_empty());
            r.cancel_reason = Some(reason.code().into());
            r.cancel_remark = Some(remark.trim().to_owned()).filter(|s| !s.is_empty());
            r.log("cancel", &data);
        }
        Outcome::Refused(env) => return Err(env.explain()),
    }
    save(company, &r)?;
    let msg = "IRN cancelled. The voucher is unchanged in the books: cancel it there too if the sale did not happen. Its number can't get an IRN again.".to_owned();
    Ok(dto(r, verify.as_deref(), vec![msg], now))
}

/// Generate an e-way bill for the voucher's IRN (Part A from the IRN, Part
/// B as given; a transporter id alone gives Part A only).
pub fn ewaybill(dir: &Path, company: &Mutex<Slot>, voucher_id: &str, details: &EwbByIrn) -> ApiResult<IrnDto> {
    let (mut r, payload) = {
        let slot = lock(company)?;
        crate::api::require(open_ref(&slot)?, UserRole::Accountant)?;
        let (_, kind) = e_invoiceable(&slot, voucher_id)?;
        if kind != VoucherKind::Sales {
            return Err("e-way bills by IRN are for invoices, not credit or debit notes".into());
        }
        let r = active_record(&slot, voucher_id, "an e-way bill")?;
        if r.status != "ACT" {
            return Err("the IRN is not active".into());
        }
        if let Some(no) = &r.ewb_no {
            return Err(format!("e-way bill {no} was already generated for this IRN"));
        }
        let payload = irp::ewb_payload(&r.irn, details).map_err(err)?;
        (r, payload)
    };
    let creds = credentials_for(dir, &r.gstin)?;
    let verify = (!creds.verify_cert_pem.is_empty()).then(|| creds.verify_cert_pem.clone());
    let client = Client::new(creds, &r.gstin)?;
    let mut messages = Vec::new();
    match client.call(Method::Post, irp::EWB_PATH, Some(&payload))? {
        Outcome::Ok { data, env } => {
            let e = irp::parse_ewb(&data).map_err(err)?;
            messages.push(format!("e-Way bill {} generated{}.", e.ewb_no, e.ewb_valid_till.as_deref().map(|v| format!(", valid till {v}")).unwrap_or_default()));
            messages.extend(irp::info_messages(&env));
            r.ewb_no = Some(e.ewb_no);
            r.ewb_dt = e.ewb_dt;
            r.ewb_valid_till = e.ewb_valid_till;
            if e.remarks.is_some() {
                r.remarks = e.remarks;
            }
            r.log("ewaybill", &data);
        }
        Outcome::Refused(env) => return Err(env.explain()),
    }
    save(company, &r)?;
    Ok(dto(r, verify.as_deref(), messages, Utc::now()))
}

/// A GSTIN as the e-invoice system has it (status, legal name, address).
pub fn gstin_details(dir: &Path, company: &Mutex<Slot>, gstin: &str) -> ApiResult<GstinDetails> {
    let (own, wanted) = {
        let slot = lock(company)?;
        (company_gstin(&slot)?, gstin.trim().to_ascii_uppercase())
    };
    khata_in::gstin::validate(&wanted).map_err(err)?;
    let client = Client::new(credentials_for(dir, &own)?, &own)?;
    match client.call(Method::Get, &format!("{}{wanted}", irp::GSTIN_PATH), None)? {
        Outcome::Ok { data, .. } => irp::parse_gstin(&data).map_err(err),
        Outcome::Refused(env) => Err(env.explain()),
    }
}

// ------------------------------------------------------------ printing

/// What the invoice print needs for an e-invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrnPrint {
    pub irn: String,
    pub ack_no: String,
    pub ack_dt: String,
    pub cancelled: bool,
    pub qr: Option<khata_print::QrSymbol>,
}

/// The voucher's IRN for printing, when it has one (active or cancelled).
pub fn print_info(slot: &Slot, voucher_id: VoucherId) -> Option<IrnPrint> {
    let r = read_record(store_ref(slot).ok()?, &voucher_id.0.to_string()).ok().flatten()?;
    (!r.irn.is_empty()).then(|| IrnPrint {
        qr: qr_symbol(&r.signed_qr_code),
        cancelled: r.status == "CNL",
        irn: r.irn,
        ack_no: r.ack_no,
        ack_dt: r.ack_dt,
    })
}

/// The text for khata-print's QR renderer: the IRP's SignedQRCode as is.
pub fn qr_text(slot: &Slot, voucher_id: VoucherId) -> Option<String> {
    let r = read_record(store_ref(slot).ok()?, &voucher_id.0.to_string()).ok().flatten()?;
    irp::qr_text(&r.signed_qr_code).map(str::to_owned)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// An HTML block with the IRN, Ack No and date and the signed QR code, for
/// the invoice print ([`splice_print_block`] puts it in place).
pub fn print_block_html(slot: &Slot, voucher_id: VoucherId) -> Option<String> {
    let p = print_info(slot, voucher_id)?;
    let qr = p.qr.as_ref().map(|q| khata_print::qr_svg(q, 32)).unwrap_or_default();
    let status = if p.cancelled { "<div style=\"font-weight:600\">IRN CANCELLED</div>" } else { "" };
    Some(format!(
        "<div class=\"irn-block\" style=\"display:flex;justify-content:space-between;align-items:flex-start;gap:12px;margin:6px 0;font-size:9pt\">\
         <div style=\"word-break:break-all\">{status}<div><b>IRN:</b> {}</div><div><b>Ack No:</b> {}</div><div><b>Ack Date:</b> {}</div></div>{qr}</div>",
        esc(&p.irn),
        esc(&p.ack_no),
        esc(&p.ack_dt)
    ))
}

/// Insert the IRN block before the buyer/number boxes of
/// `khata_print::invoice_html`, else before `</body>`.
pub fn splice_print_block(html: String, block: &str) -> String {
    for anchor in ["<div class=\"inv-meta\">", "</body>"] {
        if let Some(i) = html.find(anchor) {
            let mut out = html;
            out.insert_str(i, block);
            return out;
        }
    }
    html + block
}

// ------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, indian_company, ledger, voucher as voucher_input};
    use crate::api::{create_party_ledger, post_voucher};
    use crate::masters::{LedgerInput, StockItemInput, alter_ledger, save_stock_item, save_unit};
    use crate::tax::{set_company_address, set_company_tax, set_ledger_tax, set_tax_rate};
    use axum::Router;
    use axum::extract::{Path as UrlPath, State};
    use axum::http::HeaderMap;
    use axum::routing::{get, post};
    use base64::Engine;
    use khata_core::{DrCr, Mailing, TaxSide};
    use khata_in::irp::testing;
    use std::sync::Arc;

    // ------------------------------------------------ mock IRP

    #[derive(Default)]
    struct Mock {
        sek: [u8; 32],
        tokens: u32,
        auth_calls: u32,
        generate_calls: u32,
        /// Answer the next call with 1005 (token expired).
        expire_next: bool,
        /// (doc type, doc no) → the registration.
        irns: HashMap<(String, String), Value>,
        /// Seen secrets, to check they arrive.
        last_client_secret: String,
        /// Every request body as received (to check secrets aren't plain).
        bodies: Vec<String>,
    }

    type Shared = Arc<Mutex<Mock>>;

    fn ist_now() -> String {
        Utc::now().with_timezone(&irp::ist()).format("%Y-%m-%d %H:%M:%S").to_string()
    }

    fn reply_ok(m: &Mock, data: &Value) -> axum::Json<Value> {
        axum::Json(json!({ "Status": "1", "Data": irp::aes_encrypt(&m.sek, data.to_string().as_bytes()), "ErrorDetails": null, "InfoDtls": null }))
    }

    fn reply_err(code: &str, msg: &str, info: Value) -> axum::Json<Value> {
        axum::Json(json!({ "Status": "0", "Data": null, "ErrorDetails": [{ "ErrorCode": code, "ErrorMessage": msg }], "InfoDtls": info }))
    }

    /// Checks the headers and token; `Err` is the reply to send instead.
    fn guard(m: &mut Mock, h: &HeaderMap) -> Result<(), axum::Json<Value>> {
        let hv = |k: &str| h.get(k).and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
        if hv("client_id") != "test-client" || hv("client-id") != "test-client" {
            return Err(reply_err("1010", "Invalid Client-ID/Client-Secret", Value::Null));
        }
        if m.expire_next {
            m.expire_next = false;
            return Err(reply_err("1005", "Invalid Token", Value::Null));
        }
        if hv("AuthToken") != format!("tok-{}", m.tokens) || hv("user_name") != "apiuser" || hv("Gstin") != "27AAPFU0939F1ZV" {
            return Err(reply_err("1005", "Invalid Token", Value::Null));
        }
        Ok(())
    }

    fn body_json(m: &Mock, body: &str) -> Value {
        let env: Value = serde_json::from_str(body).unwrap();
        irp::decrypt_data(&m.sek, env["Data"].as_str().unwrap()).unwrap()
    }

    async fn auth(State(s): State<Shared>, h: HeaderMap, body: String) -> axum::Json<Value> {
        let mut m = s.lock().unwrap();
        m.auth_calls += 1;
        m.bodies.push(body.clone());
        m.last_client_secret = h.get("client_secret").and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
        let env: Value = serde_json::from_str(&body).unwrap();
        let creds = testing::open_auth_request(env["Data"].as_str().unwrap()).unwrap();
        if creds["UserName"] != "apiuser" || creds["Password"] != "Pa55word!" || m.last_client_secret != "test-secret" {
            return reply_err("1008", "Invalid login credentials", Value::Null);
        }
        let app_key: [u8; 32] = base64::engine::general_purpose::STANDARD.decode(creds["AppKey"].as_str().unwrap()).unwrap().try_into().unwrap();
        m.sek = irp::new_app_key();
        m.tokens += 1;
        let expiry = (Utc::now() + chrono::Duration::hours(6)).with_timezone(&irp::ist()).format("%Y-%m-%d %H:%M:%S").to_string();
        axum::Json(json!({
            "Status": 1,
            "Data": { "ClientId": "test-client", "UserName": "apiuser", "AuthToken": format!("tok-{}", m.tokens), "Sek": irp::aes_encrypt(&app_key, &m.sek), "TokenExpiry": expiry },
            "ErrorDetails": null, "InfoDtls": null
        }))
    }

    async fn generate_irn(State(s): State<Shared>, h: HeaderMap, body: String) -> axum::Json<Value> {
        let mut m = s.lock().unwrap();
        if let Err(e) = guard(&mut m, &h) {
            return e;
        }
        m.generate_calls += 1;
        let inv = body_json(&m, &body);
        let key = (inv["DocDtls"]["Typ"].as_str().unwrap().to_owned(), inv["DocDtls"]["No"].as_str().unwrap().to_owned());
        if let Some(existing) = m.irns.get(&key) {
            let info = json!([{ "InfCd": "DUPIRN", "Desc": { "AckNo": existing["AckNo"], "AckDt": existing["AckDt"], "Irn": existing["Irn"] } }]);
            return reply_err("2150", "Duplicate IRN", info);
        }
        let irn = format!("{:0>64}", format!("{:x}", m.irns.len() + 1)).replace('0', "a");
        let qr = testing::sign_jwt(&json!({
            "SellerGstin": inv["SellerDtls"]["Gstin"], "BuyerGstin": inv["BuyerDtls"]["Gstin"], "DocNo": key.1, "DocTyp": key.0,
            "DocDt": inv["DocDtls"]["Dt"], "TotInvVal": inv["ValDtls"]["TotInvVal"], "ItemCnt": inv["ItemList"].as_array().map_or(0, Vec::len),
            "MainHsnCode": inv["ItemList"][0]["HsnCd"], "Irn": irn
        }));
        let mut signed = inv.clone();
        signed["Irn"] = json!(irn);
        let data = json!({
            "AckNo": 112010000002315_u64 + m.irns.len() as u64, "AckDt": ist_now(), "Irn": irn,
            "SignedInvoice": testing::sign_jwt(&signed), "SignedQRCode": qr, "Status": "ACT",
            "EwbNo": null, "EwbDt": null, "EwbValidTill": null, "Remarks": null
        });
        m.irns.insert(key, data.clone());
        reply_ok(&m, &data)
    }

    async fn get_irn(State(s): State<Shared>, h: HeaderMap, UrlPath(irn): UrlPath<String>) -> axum::Json<Value> {
        let mut m = s.lock().unwrap();
        if let Err(e) = guard(&mut m, &h) {
            return e;
        }
        match m.irns.values().find(|v| v["Irn"] == irn.as_str()).cloned() {
            Some(v) => reply_ok(&m, &v),
            None => reply_err("2148", "Requested IRN data is not available", Value::Null),
        }
    }

    async fn cancel_irn(State(s): State<Shared>, h: HeaderMap, body: String) -> axum::Json<Value> {
        let mut m = s.lock().unwrap();
        if let Err(e) = guard(&mut m, &h) {
            return e;
        }
        let req = body_json(&m, &body);
        assert_eq!(req["CnlRsn"].as_str().map(str::len), Some(1));
        let irn = req["Irn"].as_str().unwrap().to_owned();
        let Some(v) = m.irns.values_mut().find(|v| v["Irn"] == irn.as_str()) else {
            return reply_err("2148", "Requested IRN data is not available", Value::Null);
        };
        v["Status"] = json!("CNL");
        let data = json!({ "Irn": irn, "CancelDate": ist_now() });
        reply_ok(&m, &data)
    }

    async fn ewb(State(s): State<Shared>, h: HeaderMap, body: String) -> axum::Json<Value> {
        let mut m = s.lock().unwrap();
        if let Err(e) = guard(&mut m, &h) {
            return e;
        }
        let req = body_json(&m, &body);
        assert_eq!(req["TransMode"], "1");
        assert_eq!(req["VehNo"], "MH12AB1234");
        let data = json!({ "EwbNo": 191008688443_u64, "EwbDt": ist_now(), "EwbValidTill": "2026-12-31 23:59:00", "Remarks": null });
        reply_ok(&m, &data)
    }

    async fn gstin(State(s): State<Shared>, h: HeaderMap, UrlPath(g): UrlPath<String>) -> axum::Json<Value> {
        let mut m = s.lock().unwrap();
        if let Err(e) = guard(&mut m, &h) {
            return e;
        }
        let data = json!({ "Gstin": g, "TradeName": "Bengaluru Co", "LegalName": "Bengaluru Co Pvt Ltd", "StateCode": 29, "AddrPncd": 560025, "TxpType": "REG", "Status": "ACT", "BlkStatus": "U" });
        reply_ok(&m, &data)
    }

    fn start_mock() -> (String, Shared) {
        let shared: Shared = Arc::default();
        let app = Router::new()
            .route(irp::AUTH_PATH, post(auth))
            .route(irp::GENERATE_PATH, post(generate_irn))
            .route(irp::CANCEL_PATH, post(cancel_irn))
            .route(&format!("{}{{irn}}", irp::IRN_PATH), get(get_irn))
            .route(irp::EWB_PATH, post(ewb))
            .route(&format!("{}{{gstin}}", irp::GSTIN_PATH), get(gstin))
            .with_state(shared.clone());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        });
        (format!("http://{addr}"), shared)
    }

    // ------------------------------------------------ the books

    struct Setup {
        company: Mutex<Slot>,
        _file: TempFile,
        dir: PathBuf,
        buyer: String,
        sales: String,
        igst: String,
        laptop: String,
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn setup(name: &str, base_url: &str) -> Setup {
        let (file, mut slot) = indian_company(&format!("irp-{name}"));
        set_company_tax(&mut slot, "27AAPFU0939F1ZV", "").unwrap();
        set_company_address(&mut slot, "12 MG Road\nCamp\nPune 411001").unwrap();
        set_tax_rate(&mut slot, "847130", "2026-04-01", "18").unwrap();
        let debtors = group_id(&slot, "Sundry Debtors");
        let buyer = create_party_ledger(&mut slot, "Bengaluru Co", &debtors, "", None, Some("29AAGCB7383J1Z4"), None).unwrap().ledger.id;
        let l = crate::masters::get_ledger(&slot, &buyer).unwrap();
        alter_ledger(&mut slot, &buyer, &LedgerInput {
            name: l.name,
            group_id: debtors,
            opening: String::new(),
            credit_days: None,
            tax_id: Some("29AAGCB7383J1Z4".into()),
            tax_id_override: None,
            tax_component: None,
            tax_side: None,
            mailing: Some(Mailing {
                address: "4 Residency Road\nShanti Nagar\nBengaluru".into(),
                region: Some("29".into()),
                postal_code: "560025".into(),
                ..Mailing::default()
            }),
            tax_class: None,
            pan: None,
            tds: None,
        })
        .unwrap();
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let igst = ledger(&mut slot, "Output IGST", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &igst, "IGST", TaxSide::Output).unwrap();
        let nos = save_unit(&mut slot, "", "Nos", "", 0).unwrap();
        let laptop = save_stock_item(&mut slot, &StockItemInput {
            id: String::new(),
            name: "Laptop".into(),
            group_id: String::new(),
            unit_id: nos.id,
            opening_qty: "10".into(),
            opening_rate: "30000".into(),
            opening_value: String::new(),
            tax_class: "847130".into(),
            costing: None,
        })
        .unwrap()
        .id;
        let dir = std::env::temp_dir().join(format!("khataos-irp-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = Setup { company: Mutex::new(slot), _file: file, dir, buyer, sales, igst, laptop };
        if !base_url.is_empty() {
            set_settings(&s.dir, &s.company.lock().unwrap(), &IrpSettingsInput {
                endpoint: "custom".into(),
                base_url: base_url.into(),
                client_id: "test-client".into(),
                client_secret: "test-secret".into(),
                user_name: "apiuser".into(),
                password: "Pa55word!".into(),
                public_key_pem: testing::TEST_PUBLIC_KEY_PEM.into(),
                verify_cert_pem: testing::TEST_CERT_PEM.into(),
                clear_verify_cert: false,
            })
            .unwrap();
        }
        s
    }

    fn sale(s: &Setup, kind: &str, date: &str) -> String {
        let (party_side, other) = if kind == "Sales" { (DrCr::Dr, DrCr::Cr) } else { (DrCr::Cr, DrCr::Dr) };
        let mut line = entry(&s.sales, other, "100000");
        line.items = vec![crate::api::StockLineInput {
            item_id: s.laptop.clone(),
            qty: "2".into(),
            rate: "50000".into(),
            amount: String::new(),
            godown_id: String::new(),
            batch: String::new(),
        }];
        let entries = vec![entry(&s.buyer, party_side, "118000"), line, entry(&s.igst, other, "18000")];
        post_voucher(&mut s.company.lock().unwrap(), voucher_input(kind, date, entries)).unwrap().id
    }

    fn today() -> String {
        Local::now().format("%Y-%m-%d").to_string()
    }

    // ------------------------------------------------ tests

    #[test]
    fn settings_keep_secrets_out_of_the_webview_and_the_books() {
        let s = setup("settings", "");
        let slot = s.company.lock().unwrap();
        let fresh = settings(&s.dir, &slot).unwrap();
        assert_eq!((fresh.gstin.as_str(), fresh.endpoint.as_str(), fresh.has_password), ("27AAPFU0939F1ZV", "nic-sandbox", false));
        let mut input = IrpSettingsInput {
            endpoint: "nic-sandbox".into(),
            client_id: "cid".into(),
            client_secret: "very-secret".into(),
            user_name: "apiuser".into(),
            password: "Pa55word!".into(),
            public_key_pem: testing::TEST_PUBLIC_KEY_PEM.into(),
            ..IrpSettingsInput::default()
        };
        let saved = set_settings(&s.dir, &slot, &input).unwrap();
        assert!(saved.has_client_secret && saved.has_password && !saved.production);
        assert_eq!(saved.public_key.as_deref(), Some("RSA 2048-bit"));
        assert_eq!(saved.base_url, "https://einv-apisandbox.nic.in");
        let shown = serde_json::to_string(&saved).unwrap();
        assert!(!shown.contains("very-secret") && !shown.contains("Pa55word!"), "{shown}");
        // Empty secrets keep the stored ones.
        input.client_secret.clear();
        input.password.clear();
        input.public_key_pem.clear();
        let again = set_settings(&s.dir, &slot, &input).unwrap();
        assert!(again.has_client_secret && again.has_password && again.public_key.is_some());
        let file = std::fs::read_to_string(s.dir.join(CREDENTIALS_FILE)).unwrap();
        assert!(file.contains("very-secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(s.dir.join(CREDENTIALS_FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // Nothing in the company file's settings or trail.
        let (store, _) = slot.as_ref().unwrap();
        assert!(store.settings().unwrap().iter().all(|(k, v)| !k.starts_with("irp") && !v.contains("very-secret")));
        assert!(store.all_records().unwrap().iter().all(|r| !r.value.as_deref().unwrap_or_default().contains("very-secret")));
        let trail = serde_json::to_string(&store.entries_from(0).unwrap()).unwrap();
        assert!(!trail.contains("very-secret") && !trail.contains("Pa55word!"));
        // Debug output is redacted.
        let c = credentials_for(&s.dir, "27AAPFU0939F1ZV").unwrap();
        assert!(!format!("{c:?}").contains("very-secret"));
        // Bad input.
        let bad = |f: &dyn Fn(&mut IrpSettingsInput)| {
            let mut i = input.clone();
            f(&mut i);
            set_settings(&s.dir, &slot, &i).unwrap_err()
        };
        assert!(bad(&|i| i.endpoint = "nope".into()).contains("choose the IRP"));
        assert!(bad(&|i| {
            i.endpoint = "custom".into();
            i.base_url = "http://gsp.example.in".into();
        })
        .contains("https://"));
        assert!(bad(&|i| i.public_key_pem = "junk".into()).contains("key"));
        assert!(bad(&|i| i.password = "pässword".into()).contains("ASCII"));
    }

    #[test]
    fn generate_store_print_and_not_resend() {
        let (url, mock) = start_mock();
        let s = setup("generate", &url);
        let id = sale(&s, "Sales", &today());
        let r = generate(&s.dir, &s.company, &id, &BuyerDetails::default()).unwrap();
        assert_eq!(r.record.status, "ACT");
        assert_eq!(r.record.irn.len(), 64);
        assert_eq!(r.record.doc_type, "INV");
        assert_eq!(r.signature, "verified");
        let qr = r.qr.as_ref().unwrap();
        assert_eq!((qr.seller_gstin.as_str(), qr.buyer_gstin.as_str(), qr.irn.as_str()), ("27AAPFU0939F1ZV", "29AAGCB7383J1Z4", r.record.irn.as_str()));
        assert_eq!(qr.total_invoice_value.parse::<f64>().ok(), Some(118000.0));
        assert!(r.cancel_until.is_some() && r.qr_svg.as_deref().is_some_and(|s| s.starts_with("<svg")));
        assert_eq!(r.record.responses.len(), 1);
        {
            let m = mock.lock().unwrap();
            assert_eq!((m.auth_calls, m.generate_calls), (1, 1));
            // The sign-in body carries no plain credentials.
            assert!(m.bodies.iter().all(|b| !b.contains("Pa55word!") && !b.contains("apiuser")));
        }
        // Second time: the stored IRN, no call.
        let again = generate(&s.dir, &s.company, &id, &BuyerDetails::default()).unwrap();
        assert_eq!(again.record.irn, r.record.irn);
        assert!(again.messages[0].contains("nothing was sent"));
        assert_eq!(mock.lock().unwrap().generate_calls, 1);
        // Printing.
        let slot = s.company.lock().unwrap();
        let vid = VoucherId(parse_uuid(&id, "voucher").unwrap());
        let p = print_info(&slot, vid).unwrap();
        assert_eq!((p.irn.as_str(), p.cancelled), (r.record.irn.as_str(), false));
        assert!(p.qr.is_some_and(|q| q.width >= 21));
        assert_eq!(qr_text(&slot, vid).as_deref(), Some(r.record.signed_qr_code.as_str()));
        let block = print_block_html(&slot, vid).unwrap();
        assert!(block.contains(&r.record.irn) && block.contains("<svg"));
        // The printed invoice carries the block once, just above the buyer boxes.
        let printed = crate::invoice::voucher_html(&slot, &id).unwrap();
        assert_eq!(printed.matches("<div class=\"irn-block\"").count(), 1);
        let block_at = printed.find("<div class=\"irn-block\"").unwrap();
        assert_eq!(printed[block_at + block.len()..].find("<div class=\"inv-meta\">"), Some(0));
        // The stored record, without a call; unchanged voucher, no warning.
        let rec = record(&s.dir, &slot, &id).unwrap().unwrap();
        assert!(rec.messages.is_empty(), "{:?}", rec.messages);
        // A shared record under irp.irn.<id> (it syncs), not in the trail.
        let (store, _) = slot.as_ref().unwrap();
        assert!(store.record(&record_key(&id)).unwrap().is_some());
        let trail = serde_json::to_string(&store.entries_from(0).unwrap()).unwrap();
        assert!(!trail.contains(&r.record.irn));
    }

    #[test]
    fn duplicate_fetches_the_existing_irn_and_expired_tokens_sign_in_again() {
        let (url, mock) = start_mock();
        let s = setup("duplicate", &url);
        let id = sale(&s, "Sales", &today());
        let first = generate(&s.dir, &s.company, &id, &BuyerDetails::default()).unwrap();
        // Lose the local record (as if the first answer never arrived).
        store_ref(&s.company.lock().unwrap()).unwrap().set_record(&record_key(&id), None, "test").unwrap();
        mock.lock().unwrap().expire_next = true;
        let dup = generate(&s.dir, &s.company, &id, &BuyerDetails::default()).unwrap();
        assert_eq!(dup.record.irn, first.record.irn);
        assert_eq!(dup.record.ack_no, first.record.ack_no);
        assert!(!dup.record.signed_qr_code.is_empty());
        assert!(dup.messages[0].contains("already had an IRN"));
        let m = mock.lock().unwrap();
        // 1005 once → one more sign-in; 2150 → details fetched.
        assert_eq!((m.auth_calls, m.generate_calls), (2, 2));
    }

    #[test]
    fn cancel_within_24_hours_then_never_again() {
        let (url, _mock) = start_mock();
        let s = setup("cancel", &url);
        let id = sale(&s, "Sales", &today());
        let r = generate(&s.dir, &s.company, &id, &BuyerDetails::default()).unwrap();
        // Client-side: past the window, nothing is sent.
        let late = Utc::now() + chrono::Duration::hours(25);
        let e = cancel_at(&s.dir, &s.company, &id, "2", "Wrong entry", late).unwrap_err();
        assert!(e.contains("24 hours") && e.contains("credit note"), "{e}");
        assert!(cancel(&s.dir, &s.company, &id, "9", "").unwrap_err().contains("reason"));
        let c = cancel(&s.dir, &s.company, &id, "2", "Wrong entry").unwrap();
        assert_eq!(c.record.status, "CNL");
        assert_eq!(c.record.irn, r.record.irn);
        assert_eq!((c.record.cancel_reason.as_deref(), c.record.cancel_remark.as_deref()), (Some("2"), Some("Wrong entry")));
        assert!(c.record.cancel_date.is_some() && c.cancel_until.is_none());
        assert!(cancel(&s.dir, &s.company, &id, "2", "").unwrap_err().contains("already cancelled"));
        assert!(generate(&s.dir, &s.company, &id, &BuyerDetails::default()).unwrap_err().contains("new number"));
        let slot = s.company.lock().unwrap();
        assert!(print_info(&slot, VoucherId(parse_uuid(&id, "voucher").unwrap())).unwrap().cancelled);
        drop(slot);
        // The IRP's own view after cancelling.
        let d = details(&s.dir, &s.company, &id).unwrap();
        assert_eq!(d.record.status, "CNL");
        assert!(d.messages[0].contains("cancelled"));
    }

    #[test]
    fn eway_bill_by_irn_and_gstin_lookup() {
        let (url, _mock) = start_mock();
        let s = setup("ewb", &url);
        let id = sale(&s, "Sales", &today());
        assert!(ewaybill(&s.dir, &s.company, &id, &EwbByIrn::default()).unwrap_err().contains("no IRN yet"));
        generate(&s.dir, &s.company, &id, &BuyerDetails::default()).unwrap();
        let details = EwbByIrn { distance_km: 840, trans_mode: Some(khata_in::ewaybill::TransMode::Road), vehicle_no: "MH 12 AB 1234".into(), ..EwbByIrn::default() };
        let r = ewaybill(&s.dir, &s.company, &id, &details).unwrap();
        assert_eq!(r.record.ewb_no.as_deref(), Some("191008688443"));
        assert!(r.messages[0].contains("191008688443"));
        assert!(ewaybill(&s.dir, &s.company, &id, &details).unwrap_err().contains("already generated"));
        // An active e-way bill blocks cancellation.
        assert!(cancel(&s.dir, &s.company, &id, "1", "").unwrap_err().contains("cancel the e-way bill"));
        let g = gstin_details(&s.dir, &s.company, "29AAGCB7383J1Z4").unwrap();
        assert_eq!((g.state_code.as_deref(), g.status.as_deref()), (Some("29"), Some("ACT")));
        assert!(gstin_details(&s.dir, &s.company, "29AAGCB7383J1Z5").is_err());
    }

    #[test]
    fn refusals_become_plain_messages() {
        let (url, mock) = start_mock();
        let s = setup("refusals", &url);
        let id = sale(&s, "Sales", &today());
        // Wrong password at the IRP.
        set_settings(&s.dir, &s.company.lock().unwrap(), &IrpSettingsInput {
            endpoint: "custom".into(),
            base_url: url.clone(),
            client_id: "test-client".into(),
            user_name: "apiuser".into(),
            password: "wrong".into(),
            ..IrpSettingsInput::default()
        })
        .unwrap();
        let e = generate(&s.dir, &s.company, &id, &BuyerDetails::default()).unwrap_err();
        assert!(e.starts_with("IRP sign-in failed") && e.contains("password") && e.contains("1008"), "{e}");
        assert!(!e.contains("wrong"));
        assert_eq!(mock.lock().unwrap().last_client_secret, "test-secret");
        // Not an e-invoice voucher kind; no settings for another company.
        let receipt = {
            let mut slot = s.company.lock().unwrap();
            let cash = group_id(&slot, "Cash-in-Hand");
            let cash = crate::api::create_ledger(&mut slot, "Petty", &cash, "").unwrap().ledger.id;
            post_voucher(&mut slot, voucher_input("Receipt", &today(), vec![entry(&cash, DrCr::Dr, "10"), entry(&s.buyer, DrCr::Cr, "10")])).unwrap().id
        };
        assert!(generate(&s.dir, &s.company, &receipt, &BuyerDetails::default()).unwrap_err().contains("Sales and Credit Note"));
        let other = setup("refusals-none", "");
        let id2 = sale(&other, "Sales", &today());
        assert!(generate(&other.dir, &other.company, &id2, &BuyerDetails::default()).unwrap_err().contains("IRP settings"));
        // The client refuses plain HTTP to anything but this computer.
        assert!(check_base_url("http://einv-apisandbox.nic.in").is_err());
        assert_eq!(check_base_url("https://gsp.example.in/einv/").unwrap(), "https://gsp.example.in/einv");
    }

    #[test]
    fn unreachable_irp_is_explained() {
        // A port nothing listens on.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let s = setup("unreachable", &url);
        let id = sale(&s, "Sales", &today());
        let e = generate(&s.dir, &s.company, &id, &BuyerDetails::default()).unwrap_err();
        assert!(e.contains("couldn't connect") || e.contains("couldn't reach"), "{e}");
        assert!(read_record(store_ref(&s.company.lock().unwrap()).unwrap(), &id).unwrap().is_none());
    }
}
