//! PAC connector (Mexico): send a sealed CFDI to a proveedor autorizado de
//! certificación for timbrado, keep the TimbreFiscalDigital it adds, and
//! ask it to cancel a CFDI. The documents are built by `tax_mx.rs`
//! ([`crate::tax_mx::prepare_for_pac`]); the TFD, the QR text and the
//! cancellation rules are in `khata_mx::timbre`. See `docs/mexico-cfdi.md`.
//!
//! **Providers.** [`PacConnector`] is the interface. Two adapters:
//! - `sw`: SW Sapien (SW smarter web) REST API, from its public PHP SDK
//!   (`github.com/lunasoft/sw-sdk-php`, `SWServices/Authentication`,
//!   `Stamp`, `Cancelation`) [MEDIUM: read from the SDK source, never
//!   called live]: `POST /v2/security/authenticate` (JSON `user`,
//!   `password` → `data.token`), `POST /cfdi33/stamp/v4` (multipart field
//!   `xml`, `Authorization: Bearer`; the path keeps its 3.3-era name for
//!   CFDI 4.0), `POST /cfdi33/cancel/csd` (JSON `rfc`, `uuid`, `motivo`,
//!   `foliosustitucion`, `b64Cer`, `b64Key`, `password`) and
//!   `POST /cfdi33/cancel/{rfc}/{uuid}/{motivo}/{folio}` (with the CSD kept
//!   in the SW account). Test `https://services.test.sw.com.mx`,
//!   production `https://services.sw.com.mx`. Both need an SW account:
//!   **there is no public sandbox without credentials**, so KhataOS has
//!   never made a live call; the tests run against a local mock server;
//! - `mock`: an in-process PAC that stamps with a made-up UUID and the
//!   Leyenda "PAC simulado: sin validez fiscal", for trying the flow. Its
//!   records are marked `test` and print "SIN VALIDEZ FISCAL".
//!
//! Where things live:
//! - **Credentials** (provider, environment, user, password or account
//!   token) are in `mx-pac.json` in the app data directory, keyed by RFC,
//!   readable only by this OS user (0600, written to a temporary file and
//!   renamed). Never in the company file, sync, exports or the webview.
//! - **The session token** is kept in memory only.
//! - **The stamp record** (UUID, FechaTimbrado, SelloSAT, NoCertificadoSAT,
//!   the stamped XML, the invoice's totals and taxes for later payment
//!   receipts, cancellation status) is a shared record of the company under
//!   `mx.cfdi.<voucher id>`, like the IRP records: outside the books, synced
//!   with co-workers and the accountant (`docs/sync-settings.md`).
//!
//! Network calls run without the company lock, as in `irp_client.rs`.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration as StdDuration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::Local;
use khata_core::{UserRole, VoucherId};
use khata_mx::cfdi::{Csd, TipoDeComprobante};
use khata_mx::pagos::DocTaxes;
use khata_mx::timbre::{self, Motivo, Timbre};
use khata_store::Store;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::api::{ApiResult, Slot, err, open_ref, parse_uuid};
use crate::tax_mx::CfdiOptions;

pub const SETTINGS_FILE: &str = "mx-pac.json";
const RECORD_PREFIX: &str = "mx.cfdi.";

/// A record's status.
pub const VIGENTE: &str = "vigente";
/// The PAC took the cancellation request; the SAT may still need the
/// receiver's acceptance.
pub const CANCEL_REQUESTED: &str = "cancelacion_solicitada";
pub const CANCELADO: &str = "cancelado";

// ------------------------------------------------------------ providers

/// A PAC known to KhataOS.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    /// (environment id, base URL); `custom` takes the URL from the settings.
    pub environments: Vec<(&'static str, &'static str)>,
    pub note: &'static str,
}

pub fn providers() -> Vec<Provider> {
    vec![
        Provider {
            id: "sw",
            name: "SW Sapien (REST)",
            environments: vec![("test", "https://services.test.sw.com.mx"), ("production", "https://services.sw.com.mx"), ("custom", "")],
            note: "Needs an SW account (test or production user and password, or an account token).",
        },
        Provider {
            id: "mock",
            name: "Mock PAC (test only)",
            environments: vec![("test", "")],
            note: "Stamps on this computer with a made-up UUID: no validity before the SAT.",
        },
    ]
}

// ------------------------------------------------------------ credentials

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Credentials {
    /// "sw" or "mock".
    pub provider: String,
    /// "test", "production" or "custom".
    pub environment: String,
    /// For `custom`: the API base URL.
    pub base_url: String,
    pub user: String,
    pub password: String,
    /// An account token (used instead of user and password).
    pub token: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("provider", &self.provider)
            .field("environment", &self.environment)
            .field("base_url", &self.base_url)
            .field("user", &"<redacted>")
            .field("password", &"<redacted>")
            .field("token", &"<redacted>")
            .finish()
    }
}

/// HTTPS only; plain HTTP just for a mock server on this computer.
fn check_base_url(url: &str) -> ApiResult<String> {
    let url = url.trim().trim_end_matches('/');
    let local = ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"].iter().any(|p| url.starts_with(p));
    if url.is_empty() {
        return Err("enter the PAC's API base URL".into());
    }
    if !(url.starts_with("https://") || local) || url.contains(char::is_whitespace) || url.contains('?') || url.contains('#') {
        return Err("the PAC base URL must start with https://".into());
    }
    Ok(url.to_owned())
}

impl Credentials {
    fn base(&self) -> ApiResult<String> {
        let p = providers().into_iter().find(|p| p.id == self.provider).ok_or("choose the PAC in PAC settings")?;
        let (_, url) = p.environments.iter().find(|(e, _)| *e == self.environment).ok_or("choose the PAC environment (test or production)")?;
        if self.provider == "mock" {
            return Ok(String::new());
        }
        if self.environment == "custom" { check_base_url(&self.base_url) } else { Ok((*url).to_owned()) }
    }

    fn production(&self) -> bool {
        self.environment == "production"
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Vault {
    version: u32,
    by_rfc: BTreeMap<String, Credentials>,
}

fn vault_path(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE)
}

fn load_vault(dir: &Path) -> ApiResult<Vault> {
    match std::fs::read_to_string(vault_path(dir)) {
        Ok(s) => serde_json::from_str(&s).map_err(|_| format!("{SETTINGS_FILE} is damaged; enter the PAC settings again")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vault { version: 1, ..Vault::default() }),
        Err(e) => Err(format!("cannot read the PAC settings: {e}")),
    }
}

/// Readable by this user only: a temporary file made 0600, then renamed.
fn save_vault(dir: &Path, vault: &Vault) -> ApiResult<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create the app data folder: {e}"))?;
    let json = serde_json::to_vec_pretty(vault).map_err(err)?;
    let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(&tmp)
        .and_then(|mut f| f.write_all(&json).and_then(|_| f.sync_all()))
        .map_err(|e| format!("cannot save the PAC settings: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, vault_path(dir)).map_err(|e| format!("cannot save the PAC settings: {e}"))
}

fn company_rfc(slot: &Slot) -> ApiResult<String> {
    let book = open_ref(slot)?;
    crate::tax_mx::mx_only(book)?;
    crate::tax_mx::company_rfc(book).map(|r| khata_mx::rfc::normalise(&r))
}

/// What the settings screen may see: no secrets.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PacSettingsDto {
    pub rfc: String,
    pub provider: String,
    pub environment: String,
    pub base_url: String,
    pub production: bool,
    pub user: String,
    pub has_password: bool,
    pub has_token: bool,
    pub providers: Vec<Provider>,
    pub stored_in: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PacSettingsInput {
    pub provider: String,
    pub environment: String,
    pub base_url: String,
    pub user: String,
    /// Empty keeps the stored one.
    pub password: String,
    /// Empty keeps the stored one.
    pub token: String,
    pub clear_token: bool,
}

fn settings_dto(dir: &Path, rfc: &str, c: &Credentials) -> PacSettingsDto {
    PacSettingsDto {
        rfc: rfc.to_owned(),
        provider: c.provider.clone(),
        environment: c.environment.clone(),
        base_url: c.base().unwrap_or_else(|_| c.base_url.clone()),
        production: c.production(),
        user: c.user.clone(),
        has_password: !c.password.is_empty(),
        has_token: !c.token.is_empty(),
        providers: providers(),
        stored_in: vault_path(dir).display().to_string(),
    }
}

pub fn settings(dir: &Path, slot: &Slot) -> ApiResult<PacSettingsDto> {
    let rfc = company_rfc(slot)?;
    let c = load_vault(dir)?.by_rfc.remove(&rfc).unwrap_or_else(|| Credentials { provider: "sw".into(), environment: "test".into(), ..Credentials::default() });
    Ok(settings_dto(dir, &rfc, &c))
}

pub fn set_settings(dir: &Path, slot: &Slot, input: &PacSettingsInput) -> ApiResult<PacSettingsDto> {
    crate::api::require(open_ref(slot)?, UserRole::Owner)?;
    let rfc = company_rfc(slot)?;
    let mut vault = load_vault(dir)?;
    let mut c = vault.by_rfc.get(&rfc).cloned().unwrap_or_default();
    let provider = providers().into_iter().find(|p| p.id == input.provider.trim()).ok_or("choose the PAC: sw or mock")?;
    if !provider.environments.iter().any(|(e, _)| *e == input.environment.trim()) {
        return Err(format!("{} has the environments {}", provider.name, provider.environments.iter().map(|e| e.0).collect::<Vec<_>>().join(", ")));
    }
    c.provider = provider.id.to_owned();
    c.environment = input.environment.trim().to_owned();
    c.base_url = if c.environment == "custom" { check_base_url(&input.base_url)? } else { String::new() };
    c.user = input.user.trim().to_owned();
    if !input.password.is_empty() {
        c.password = input.password.clone();
    }
    if input.clear_token {
        c.token.clear();
    } else if !input.token.trim().is_empty() {
        c.token = input.token.trim().to_owned();
    }
    for (what, v) in [("user", &c.user), ("password", &c.password), ("token", &c.token)] {
        if v.chars().any(char::is_control) {
            return Err(format!("the {what} has control characters"));
        }
    }
    forget_sessions();
    vault.version = 1;
    vault.by_rfc.insert(rfc.clone(), c.clone());
    save_vault(dir, &vault)?;
    Ok(settings_dto(dir, &rfc, &c))
}

fn credentials_for(dir: &Path, rfc: &str) -> ApiResult<Credentials> {
    let c = load_vault(dir)?.by_rfc.remove(rfc).ok_or_else(|| format!("no PAC is set up for {rfc}: open CFDI → PAC settings"))?;
    if c.provider == "sw" && c.token.is_empty() && (c.user.is_empty() || c.password.is_empty()) {
        return Err("PAC settings need the SW user and password, or an account token".into());
    }
    Ok(c)
}

// ------------------------------------------------------------ the connector

/// A stamped CFDI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamped {
    pub timbre: Timbre,
    /// The CFDI with its TimbreFiscalDigital.
    pub xml: String,
}

/// The CSD sent with a cancellation (SW's "cancelación por CSD").
#[derive(Clone)]
pub struct CsdFiles {
    pub cer_b64: String,
    pub key_b64: String,
    pub password: String,
}

impl fmt::Debug for CsdFiles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CsdFiles(<redacted>)")
    }
}

#[derive(Debug, Clone)]
pub struct CancelRequest {
    pub rfc: String,
    pub uuid: String,
    pub motivo: Motivo,
    pub folio_sustitucion: Option<String>,
    /// `None`: the PAC holds the CSD already.
    pub csd: Option<CsdFiles>,
}

/// The PAC's answer to a cancellation: the SAT's code for the UUID (201
/// request received, 202 already cancelled) and its acuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelAnswer {
    pub code: String,
    pub acuse: String,
}

/// What every PAC adapter does.
pub trait PacConnector {
    /// Certify a sealed CFDI.
    fn stamp(&self, xml: &str) -> ApiResult<Stamped>;
    /// Ask the SAT, through the PAC, to cancel a CFDI.
    fn cancel(&self, request: &CancelRequest) -> ApiResult<CancelAnswer>;
    /// Stamps made by this adapter have no validity before the SAT.
    fn test_only(&self) -> bool {
        false
    }
}

pub fn connector(c: &Credentials) -> ApiResult<Box<dyn PacConnector + Send>> {
    match c.provider.as_str() {
        "mock" => Ok(Box::new(MockPac)),
        "sw" => Ok(Box::new(SwSapien::new(c)?)),
        _ => Err("choose the PAC in PAC settings".into()),
    }
}

// ------------------------------------------------------------ mock

/// Stamps locally. The SelloSAT is a hash, not a signature, and the
/// Leyenda says the stamp is not valid.
pub struct MockPac;

pub const MOCK_LEYENDA: &str = "PAC simulado: sin validez fiscal";

impl PacConnector for MockPac {
    fn stamp(&self, xml: &str) -> ApiResult<Stamped> {
        let root = timbre::start_tag(xml, "Comprobante").ok_or("not a CFDI")?;
        let sello = timbre::attribute(root, "Sello").filter(|s| !s.is_empty()).ok_or("the CFDI is not sealed")?;
        if timbre::attribute(root, "NoCertificado").is_none() {
            return Err("the CFDI has no NoCertificado".into());
        }
        let mut t = Timbre {
            version: "1.1".into(),
            uuid: uuid::Uuid::new_v4().to_string().to_uppercase(),
            fecha_timbrado: Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
            rfc_prov_certif: "AAA010101AAA".into(),
            leyenda: Some(MOCK_LEYENDA.into()),
            sello_cfd: sello,
            no_certificado_sat: "00000000000000000000".into(),
            sello_sat: String::new(),
        };
        t.sello_sat = B64.encode(Sha256::digest(t.cadena_original().as_bytes()));
        let xml = t.attach(xml)?;
        Ok(Stamped { timbre: t, xml })
    }

    fn cancel(&self, r: &CancelRequest) -> ApiResult<CancelAnswer> {
        Ok(CancelAnswer {
            code: "201".into(),
            acuse: format!("<Acuse Simulado=\"true\" RfcEmisor=\"{}\"><Folios><UUID>{}</UUID><EstatusUUID>201</EstatusUUID></Folios></Acuse>", r.rfc, r.uuid),
        })
    }

    fn test_only(&self) -> bool {
        true
    }
}

// ------------------------------------------------------------ SW Sapien

pub const SW_AUTH_PATH: &str = "/v2/security/authenticate";
pub const SW_STAMP_PATH: &str = "/cfdi33/stamp/v4";
pub const SW_CANCEL_CSD_PATH: &str = "/cfdi33/cancel/csd";
pub const SW_CANCEL_UUID_PATH: &str = "/cfdi33/cancel/";

/// How long a session token is reused [UNCERTAIN: SW's token lifetime is
/// not in the SDK; an hour is well inside it].
const SESSION_TTL: StdDuration = StdDuration::from_secs(3600);

fn sessions() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    static S: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn forget_sessions() {
    if let Ok(mut m) = sessions().lock() {
        m.clear();
    }
}

pub struct SwSapien {
    base: String,
    user: String,
    password: String,
    token: String,
    agent: ureq::Agent,
}

fn net(e: ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_) => "the PAC did not answer in time; try again in a minute".into(),
        ureq::Error::HostNotFound => "couldn't find the PAC's server; check the internet connection".into(),
        ureq::Error::ConnectionFailed | ureq::Error::Io(_) => "couldn't connect to the PAC".into(),
        other => format!("couldn't reach the PAC ({other})"),
    }
}

fn preview(s: &str) -> String {
    let s: String = s.chars().take(200).collect();
    s.replace(['\n', '\r'], " ")
}

/// SW's answer: `{status: "success", data}` or `{status: "error",
/// message, messageDetail}`.
fn sw_answer(status: u16, body: &str) -> ApiResult<Value> {
    let v: Value = serde_json::from_str(body).map_err(|_| format!("the PAC answered HTTP {status}: {}", preview(body)))?;
    if v["status"].as_str() == Some("success") {
        return Ok(v["data"].clone());
    }
    let msg = v["message"].as_str().unwrap_or("refused");
    let detail = v["messageDetail"].as_str().unwrap_or_default();
    Err(if detail.is_empty() { format!("the PAC refused it: {msg}") } else { format!("the PAC refused it: {msg} — {detail}") })
}

impl SwSapien {
    pub fn new(c: &Credentials) -> ApiResult<SwSapien> {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(StdDuration::from_secs(15)))
            .timeout_global(Some(StdDuration::from_secs(90)))
            .build()
            .into();
        Ok(SwSapien { base: c.base()?, user: c.user.clone(), password: c.password.clone(), token: c.token.clone(), agent })
    }

    fn session_key(&self) -> String {
        format!("{}|{}", self.base, self.user)
    }

    fn read(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> ApiResult<(u16, String)> {
        let mut resp = resp.map_err(net)?;
        let status = resp.status().as_u16();
        let body = resp.body_mut().read_to_string().map_err(|e| format!("couldn't read the PAC's answer: {e}"))?;
        Ok((status, body))
    }

    fn token(&self, fresh: bool) -> ApiResult<String> {
        if !self.token.is_empty() {
            return Ok(self.token.clone());
        }
        if !fresh
            && let Some((t, at)) = sessions().lock().ok().and_then(|m| m.get(&self.session_key()).cloned())
            && at.elapsed() < SESSION_TTL
        {
            return Ok(t);
        }
        let body = json!({ "user": self.user, "password": self.password }).to_string();
        let req = self.agent.post(format!("{}{SW_AUTH_PATH}", self.base)).header("Content-Type", "application/json");
        let (status, body) = Self::read(req.send(body.as_str()))?;
        let data = sw_answer(status, &body).map_err(|e| e.replace("refused it", "refused the sign-in"))?;
        let token = data["token"].as_str().filter(|t| !t.is_empty()).ok_or("the PAC's sign-in answer has no token")?.to_owned();
        if let Ok(mut m) = sessions().lock() {
            m.insert(self.session_key(), (token.clone(), Instant::now()));
        }
        Ok(token)
    }

    /// Send with a session token; on HTTP 401 sign in again once.
    fn with_token(&self, send: impl Fn(&str) -> ApiResult<(u16, String)>) -> ApiResult<Value> {
        let (status, body) = send(&self.token(false)?)?;
        let (status, body) = if status == 401 && self.token.is_empty() { send(&self.token(true)?)? } else { (status, body) };
        sw_answer(status, &body)
    }
}

impl PacConnector for SwSapien {
    fn stamp(&self, xml: &str) -> ApiResult<Stamped> {
        let boundary = format!("----khataos{}", uuid::Uuid::new_v4().simple());
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"xml\"; filename=\"xml\"\r\nContent-Type: text/xml\r\n\r\n{xml}\r\n--{boundary}--\r\n"
        );
        let data = self.with_token(|token| {
            let req = self
                .agent
                .post(format!("{}{SW_STAMP_PATH}", self.base))
                .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
                .header("Authorization", &format!("Bearer {token}"));
            Self::read(req.send(body.as_str()))
        })?;
        let stamped = data["cfdi"].as_str().filter(|x| !x.is_empty()).ok_or("the PAC's answer has no stamped CFDI")?.to_owned();
        let timbre = Timbre::from_xml(&stamped).ok_or("the PAC's CFDI has no TimbreFiscalDigital")?;
        if let Some(u) = data["uuid"].as_str()
            && !u.eq_ignore_ascii_case(&timbre.uuid)
        {
            return Err(format!("the PAC's answer is inconsistent: uuid {u} but the TFD says {}", timbre.uuid));
        }
        Ok(Stamped { timbre, xml: stamped })
    }

    fn cancel(&self, r: &CancelRequest) -> ApiResult<CancelAnswer> {
        let data = match &r.csd {
            Some(csd) => {
                let body = json!({
                    "rfc": r.rfc,
                    "uuid": r.uuid,
                    "motivo": r.motivo.code(),
                    "foliosustitucion": r.folio_sustitucion,
                    "b64Cer": csd.cer_b64,
                    "b64Key": csd.key_b64,
                    "password": csd.password,
                })
                .to_string();
                self.with_token(|token| {
                    let req = self
                        .agent
                        .post(format!("{}{SW_CANCEL_CSD_PATH}", self.base))
                        .header("Content-Type", "application/json")
                        .header("Authorization", &format!("Bearer {token}"));
                    Self::read(req.send(body.as_str()))
                })?
            }
            None => {
                let mut path = format!("{}{SW_CANCEL_UUID_PATH}{}/{}/{}", self.base, r.rfc, r.uuid, r.motivo.code());
                if let Some(f) = &r.folio_sustitucion {
                    path.push('/');
                    path.push_str(f);
                }
                self.with_token(|token| {
                    let req = self.agent.post(&path).header("Content-Type", "application/json").header("Authorization", &format!("Bearer {token}"));
                    Self::read(req.send(""))
                })?
            }
        };
        let code = data["uuid"]
            .as_object()
            .and_then(|m| m.iter().find(|(k, _)| k.eq_ignore_ascii_case(&r.uuid)).map(|(_, v)| v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string())))
            .unwrap_or_default();
        Ok(CancelAnswer { code, acuse: data["acuse"].as_str().unwrap_or_default().to_owned() })
    }
}

// ------------------------------------------------------------ records

/// The stamp of one voucher, a shared record of the company.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PacRecord {
    pub voucher_id: String,
    pub voucher_number: String,
    /// "I", "E" or "P".
    pub tipo: String,
    pub provider: String,
    pub environment: String,
    /// Stamped by the mock PAC: no validity.
    pub test: bool,
    pub uuid: String,
    pub fecha_timbrado: String,
    pub rfc_prov_certif: String,
    pub leyenda: Option<String>,
    pub no_certificado_sat: String,
    pub sello_sat: String,
    pub sello_cfd: String,
    /// The issuer's certificate number.
    pub no_certificado: String,
    pub rfc_emisor: String,
    pub rfc_receptor: String,
    pub total: Decimal,
    /// "PUE" or "PPD" for invoices.
    pub metodo_pago: Option<String>,
    pub serie: Option<String>,
    pub folio: String,
    /// An invoice's taxes, for its payment receipts.
    pub taxes: Option<DocTaxes>,
    /// The stamped CFDI.
    pub xml: String,
    /// [`VIGENTE`], [`CANCEL_REQUESTED`] or [`CANCELADO`].
    pub status: String,
    pub cancel_motivo: Option<String>,
    pub cancel_folio_sustitucion: Option<String>,
    pub cancel_code: Option<String>,
    pub cancel_acuse: Option<String>,
    pub cancel_requested_at: Option<String>,
    pub stamped_at: String,
    pub updated_at: String,
}

impl PacRecord {
    /// Stamped and not cancelled (nor being cancelled).
    pub fn is_active(&self) -> bool {
        !self.uuid.is_empty() && self.status == VIGENTE
    }

    pub fn timbre(&self) -> Timbre {
        Timbre {
            version: "1.1".into(),
            uuid: self.uuid.clone(),
            fecha_timbrado: self.fecha_timbrado.clone(),
            rfc_prov_certif: self.rfc_prov_certif.clone(),
            leyenda: self.leyenda.clone(),
            sello_cfd: self.sello_cfd.clone(),
            no_certificado_sat: self.no_certificado_sat.clone(),
            sello_sat: self.sello_sat.clone(),
        }
    }
}

fn record_key(voucher_id: &str) -> String {
    format!("{RECORD_PREFIX}{voucher_id}")
}

pub fn read_record(store: &Store, voucher_id: &str) -> ApiResult<Option<PacRecord>> {
    match store.record(&record_key(voucher_id)).map_err(err)? {
        Some(s) => serde_json::from_str(&s).map(Some).map_err(|_| "the stored CFDI stamp record is damaged".into()),
        None => Ok(None),
    }
}

fn write_record(store: &Store, r: &PacRecord) -> ApiResult<()> {
    let s = serde_json::to_string(r).map_err(err)?;
    // A shared record of the company (`docs/sync-settings.md`); the PAC
    // credentials and the CSD stay in app data.
    store.set_record(&record_key(&r.voucher_id), Some(&s), &crate::api::os_user()).map(|_| ()).map_err(err)
}

/// What the screen shows.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PacDto {
    #[serde(flatten)]
    pub record: PacRecord,
    /// The TFD's cadena original (for the printed representation).
    pub tfd_cadena: String,
    /// The text of the printed QR code.
    pub qr_text: String,
    pub qr_svg: Option<String>,
    pub messages: Vec<String>,
}

fn qr_text_of(r: &PacRecord) -> String {
    timbre::qr_text(&r.uuid, &r.rfc_emisor, &r.rfc_receptor, r.total, &r.sello_cfd)
}

fn qr_symbol(text: &str) -> Option<khata_print::QrSymbol> {
    let m = khata_sa::qr::matrix(text).ok()?;
    Some(khata_print::QrSymbol { width: m.width, dark: m.dark })
}

fn dto(r: PacRecord, messages: Vec<String>) -> PacDto {
    let qr_text = qr_text_of(&r);
    PacDto { tfd_cadena: r.timbre().cadena_original(), qr_svg: qr_symbol(&qr_text).map(|q| khata_print::qr_svg(&q, 30)), qr_text, messages, record: r }
}

fn lock(company: &Mutex<Slot>) -> ApiResult<MutexGuard<'_, Slot>> {
    company.lock().map_err(|_| "company state is poisoned".to_owned())
}

fn store_ref(slot: &Slot) -> ApiResult<&Store> {
    crate::tax_mx::store_ref(slot)
}

/// Store a record under the lock, after the network call.
fn save(company: &Mutex<Slot>, r: &PacRecord) -> ApiResult<()> {
    let slot = lock(company)?;
    let book = open_ref(&slot)?;
    if book.voucher(VoucherId(parse_uuid(&r.voucher_id, "voucher")?)).is_none() {
        return Err(format!(
            "the company was changed or closed while the PAC answered; folio fiscal {} was not saved. Keep this UUID: the CFDI exists for the SAT",
            r.uuid
        ));
    }
    write_record(store_ref(&slot)?, r)
}

fn now_text() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

// ------------------------------------------------------------ operations

pub fn record(slot: &Slot, voucher_id: &str) -> ApiResult<Option<PacDto>> {
    Ok(read_record(store_ref(slot)?, voucher_id)?.map(|r| dto(r, Vec::new())))
}

/// Seal a Sales, Credit Note or Receipt voucher's CFDI with the CSD and
/// have the PAC stamp it. A voucher with a stamp in force is not sent
/// again.
pub fn stamp(dir: &Path, company: &Mutex<Slot>, voucher_id: &str, opts: &CfdiOptions) -> ApiResult<PacDto> {
    let (prepared, creds) = {
        let slot = lock(company)?;
        crate::api::require(open_ref(&slot)?, UserRole::Accountant)?;
        if let Some(r) = read_record(store_ref(&slot)?, voucher_id)?
            && r.status != CANCELADO
            && !r.uuid.is_empty()
        {
            return Ok(dto(r, vec!["This voucher is already stamped; nothing was sent.".into()]));
        }
        if opts.password.is_empty() {
            return Err("type the CSD password: the PAC stamps sealed CFDIs only".into());
        }
        let rfc = company_rfc(&slot)?;
        let creds = credentials_for(dir, &rfc)?;
        (crate::tax_mx::prepare_for_pac(dir, &slot, voucher_id, opts)?, creds)
    };
    let pac = connector(&creds)?;
    let stamped = pac.stamp(&prepared.generated.xml)?;
    let t = &stamped.timbre;
    if !khata_mx::pagos::is_uuid(&t.uuid) {
        return Err(format!("the PAC returned {:?}, which is not a UUID", t.uuid));
    }
    if t.sello_cfd != prepared.generated.sello {
        return Err("the PAC's stamp does not carry this CFDI's sello; it was not saved".into());
    }
    let now = now_text();
    let r = PacRecord {
        voucher_id: voucher_id.to_owned(),
        voucher_number: prepared.voucher_number.clone(),
        tipo: prepared.tipo.code().into(),
        provider: creds.provider.clone(),
        environment: creds.environment.clone(),
        test: pac.test_only(),
        uuid: t.uuid.clone(),
        fecha_timbrado: t.fecha_timbrado.clone(),
        rfc_prov_certif: t.rfc_prov_certif.clone(),
        leyenda: t.leyenda.clone(),
        no_certificado_sat: t.no_certificado_sat.clone(),
        sello_sat: t.sello_sat.clone(),
        sello_cfd: t.sello_cfd.clone(),
        no_certificado: prepared.generated.no_certificado.clone(),
        rfc_emisor: prepared.rfc_emisor.clone(),
        rfc_receptor: prepared.rfc_receptor.clone(),
        total: prepared.total,
        metodo_pago: prepared.metodo_pago.map(|m| m.code().to_owned()),
        serie: prepared.serie.clone(),
        folio: prepared.folio.clone(),
        taxes: prepared.taxes.clone().filter(|_| prepared.tipo == TipoDeComprobante::Ingreso),
        xml: stamped.xml.clone(),
        status: VIGENTE.into(),
        stamped_at: now.clone(),
        updated_at: now,
        ..PacRecord::default()
    };
    save(company, &r)?;
    let mut messages = prepared.warnings;
    if r.test {
        messages.insert(0, "Stamped by the mock PAC: this is NOT a valid CFDI.".into());
    }
    Ok(dto(r, messages))
}

/// Ask the PAC to cancel the voucher's CFDI. With a password the CSD is
/// sent with the request (SW's cancelación por CSD); without one the PAC
/// must hold the CSD already.
pub fn cancel(dir: &Path, company: &Mutex<Slot>, voucher_id: &str, motivo: &str, folio_sustitucion: &str, password: &str) -> ApiResult<PacDto> {
    let (mut r, creds, request) = {
        let slot = lock(company)?;
        crate::api::require(open_ref(&slot)?, UserRole::Accountant)?;
        let r = read_record(store_ref(&slot)?, voucher_id)?.filter(|r| !r.uuid.is_empty()).ok_or("this voucher has no stamped CFDI")?;
        if r.status == CANCELADO {
            return Ok(dto(r, vec!["This CFDI is already cancelled.".into()]));
        }
        let motivo = Motivo::parse(motivo)?;
        let folio = timbre::check_cancellation(&r.uuid, motivo, Some(folio_sustitucion))?;
        let creds = credentials_for(dir, &r.rfc_emisor)?;
        let csd = if password.is_empty() {
            None
        } else {
            let (cer, key) = crate::tax_mx::csd_paths(dir, &r.rfc_emisor);
            let cer = std::fs::read(&cer).map_err(|_| "no CSD is installed for this RFC".to_owned())?;
            let key = std::fs::read(&key).map_err(|_| "no CSD is installed for this RFC".to_owned())?;
            Csd::from_der(&cer, &key, password)?;
            Some(CsdFiles { cer_b64: B64.encode(&cer), key_b64: B64.encode(&key), password: password.to_owned() })
        };
        let request = CancelRequest { rfc: r.rfc_emisor.clone(), uuid: r.uuid.clone(), motivo, folio_sustitucion: folio, csd };
        (r, creds, request)
    };
    let answer = connector(&creds)?.cancel(&request)?;
    let mut messages = Vec::new();
    r.status = match answer.code.as_str() {
        "202" => {
            messages.push("The SAT already had this CFDI as cancelled.".into());
            CANCELADO.into()
        }
        "201" => {
            messages.push(
                "The SAT received the cancellation request. When the receiver's acceptance is needed it has 3 working days to answer; check the CFDI's status at the SAT and mark it cancelled here when it is.".into(),
            );
            CANCEL_REQUESTED.into()
        }
        other => return Err(format!("the PAC answered code {other:?} for the cancellation; nothing was recorded")),
    };
    r.cancel_motivo = Some(request.motivo.code().into());
    r.cancel_folio_sustitucion = request.folio_sustitucion.clone();
    r.cancel_code = Some(answer.code);
    r.cancel_acuse = Some(answer.acuse).filter(|a| !a.is_empty());
    r.cancel_requested_at = Some(now_text());
    r.updated_at = now_text();
    save(company, &r)?;
    Ok(dto(r, messages))
}

/// Record what the SAT says after a cancellation request: cancelled, or
/// still in force (the receiver rejected it). Owner only.
pub fn confirm_cancellation(company: &Mutex<Slot>, voucher_id: &str, cancelled: bool) -> ApiResult<PacDto> {
    let slot = lock(company)?;
    crate::api::require(open_ref(&slot)?, UserRole::Owner)?;
    let mut r = read_record(store_ref(&slot)?, voucher_id)?.ok_or("this voucher has no stamped CFDI")?;
    if r.status != CANCEL_REQUESTED {
        return Err("no cancellation was requested for this CFDI".into());
    }
    r.status = if cancelled { CANCELADO } else { VIGENTE }.into();
    r.updated_at = now_text();
    write_record(store_ref(&slot)?, &r)?;
    Ok(dto(r, Vec::new()))
}

// ------------------------------------------------------------ printing

/// What the invoice print needs for a stamped CFDI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfdiPrint {
    pub uuid: String,
    pub fecha_timbrado: String,
    pub no_certificado: String,
    pub no_certificado_sat: String,
    pub sello_cfd: String,
    pub sello_sat: String,
    pub tfd_cadena: String,
    pub cancelled: bool,
    pub test: bool,
    pub qr: Option<khata_print::QrSymbol>,
}

pub fn print_info(slot: &Slot, voucher_id: VoucherId) -> Option<CfdiPrint> {
    let r = read_record(store_ref(slot).ok()?, &voucher_id.0.to_string()).ok().flatten()?;
    (!r.uuid.is_empty()).then(|| CfdiPrint {
        qr: qr_symbol(&qr_text_of(&r)),
        tfd_cadena: r.timbre().cadena_original(),
        cancelled: r.status == CANCELADO,
        test: r.test,
        uuid: r.uuid,
        fecha_timbrado: r.fecha_timbrado,
        no_certificado: r.no_certificado,
        no_certificado_sat: r.no_certificado_sat,
        sello_cfd: r.sello_cfd,
        sello_sat: r.sello_sat,
    })
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// An HTML block with the folio fiscal and the TFD's data and QR code, the
/// elements of the CFDI's printed representation (Anexo 20 §I.D), for the
/// invoice print (`irp_client::splice_print_block` puts it in place).
pub fn print_block_html(slot: &Slot, voucher_id: VoucherId) -> Option<String> {
    let p = print_info(slot, voucher_id)?;
    let qr = p.qr.as_ref().map(|q| khata_print::qr_svg(q, 28)).unwrap_or_default();
    let mut flags = String::new();
    if p.test {
        flags.push_str("<div style=\"font-weight:700\">SIN VALIDEZ FISCAL (PAC simulado)</div>");
    }
    if p.cancelled {
        flags.push_str("<div style=\"font-weight:700\">CFDI CANCELADO</div>");
    }
    Some(format!(
        "<div class=\"cfdi-block\" style=\"display:flex;justify-content:space-between;align-items:flex-start;gap:12px;margin:6px 0;font-size:8pt\">\
         <div style=\"word-break:break-all\">{flags}<div><b>Folio fiscal (UUID):</b> {}</div><div><b>Fecha de certificación:</b> {}</div>\
         <div><b>No. de serie del CSD:</b> {}</div><div><b>No. de serie del certificado del SAT:</b> {}</div>\
         <div><b>Sello digital del CFDI:</b> {}</div><div><b>Sello del SAT:</b> {}</div>\
         <div><b>Cadena original del complemento de certificación digital del SAT:</b> {}</div>\
         <div>Este documento es una representación impresa de un CFDI.</div></div>{qr}</div>",
        esc(&p.uuid),
        esc(&p.fecha_timbrado),
        esc(&p.no_certificado),
        esc(&p.no_certificado_sat),
        esc(&p.sello_cfd),
        esc(&p.sello_sat),
        esc(&p.tfd_cadena),
    ))
}

// ------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, entry, group_id, ledger, voucher};
    use crate::api::{BillInput, create_company, create_party_ledger, post_voucher, set_country};
    use crate::tax::{set_company_tax, set_ledger_tax, set_tax_rate};
    use crate::tax_mx::{MxIssuer, set_mx_csd, set_mx_issuer};
    use axum::Router;
    use axum::extract::{Path as UrlPath, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use khata_core::{BillKind, DrCr, TaxSide};
    use rust_decimal_macros::dec;
    use std::sync::Arc;

    const CER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../crates/khata-mx/testdata/test-csd.cer");
    const KEY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../crates/khata-mx/testdata/test-csd.key");
    const PASSWORD: &str = "12345678a";

    // ------------------------------------------------ mock SW server

    #[derive(Default)]
    struct Sw {
        auths: u32,
        stamps: u32,
        /// Answer the next stamp with HTTP 401 (token expired).
        expire_next: bool,
        last_cancel: Option<Value>,
        last_cancel_path: Option<String>,
        last_auth_header: String,
    }

    type Shared = Arc<Mutex<Sw>>;

    async fn auth(State(s): State<Shared>, body: String) -> (StatusCode, String) {
        let v: Value = serde_json::from_str(&body).unwrap_or_default();
        let mut m = s.lock().unwrap();
        if v["user"] != "demo" || v["password"] != "s3cret" {
            return (StatusCode::BAD_REQUEST, json!({"status": "error", "message": "AU2000", "messageDetail": "Usuario o contraseña inválidos"}).to_string());
        }
        m.auths += 1;
        (StatusCode::OK, json!({"status": "success", "data": {"token": format!("tok{}", m.auths)}}).to_string())
    }

    async fn stamp_v4(State(s): State<Shared>, h: HeaderMap, body: String) -> (StatusCode, String) {
        let mut m = s.lock().unwrap();
        m.last_auth_header = h.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
        if m.expire_next {
            m.expire_next = false;
            return (StatusCode::UNAUTHORIZED, json!({"status": "error", "message": "token expirado"}).to_string());
        }
        if !m.last_auth_header.starts_with("Bearer tok") {
            return (StatusCode::UNAUTHORIZED, json!({"status": "error", "message": "sin token"}).to_string());
        }
        let ct = h.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default();
        let boundary = ct.split("boundary=").nth(1).unwrap_or_default();
        let start = body.find("\r\n\r\n").unwrap() + 4;
        let end = body.rfind(&format!("\r\n--{boundary}--")).unwrap();
        let xml = &body[start..end];
        if !body.contains("name=\"xml\"") || !xml.contains("Sello=\"") {
            return (StatusCode::BAD_REQUEST, json!({"status": "error", "message": "CFDI40102", "messageDetail": "sello inválido"}).to_string());
        }
        m.stamps += 1;
        let sello = timbre::attribute(timbre::start_tag(xml, "Comprobante").unwrap(), "Sello").unwrap();
        let t = Timbre {
            version: "1.1".into(),
            uuid: format!("1147a19d-8fd5-44f6-9c83-6749745185{:02}", m.stamps),
            fecha_timbrado: "2026-06-02T10:00:00".into(),
            rfc_prov_certif: "SPR190613I52".into(),
            leyenda: None,
            sello_cfd: sello,
            no_certificado_sat: "30001000000500003456".into(),
            sello_sat: "U0FUU0VMTE8=".into(),
        };
        let cfdi = t.attach(xml).unwrap();
        (
            StatusCode::OK,
            json!({"status": "success", "data": {"uuid": t.uuid, "fechaTimbrado": t.fecha_timbrado, "noCertificadoSAT": t.no_certificado_sat, "selloSAT": t.sello_sat, "selloCFDI": t.sello_cfd, "cadenaOriginalSAT": t.cadena_original(), "qrCode": "", "cfdi": cfdi}})
                .to_string(),
        )
    }

    async fn cancel_csd(State(s): State<Shared>, body: String) -> (StatusCode, String) {
        let v: Value = serde_json::from_str(&body).unwrap();
        let uuid = v["uuid"].as_str().unwrap().to_owned();
        s.lock().unwrap().last_cancel = Some(v);
        let mut codes = serde_json::Map::new();
        codes.insert(uuid, "201".into());
        (StatusCode::OK, json!({"status": "success", "data": {"acuse": "<Acuse/>", "uuid": codes}}).to_string())
    }

    async fn cancel_uuid(State(s): State<Shared>, UrlPath((rfc, uuid, motivo)): UrlPath<(String, String, String)>) -> (StatusCode, String) {
        s.lock().unwrap().last_cancel_path = Some(format!("{rfc}/{uuid}/{motivo}"));
        let mut codes = serde_json::Map::new();
        codes.insert(uuid, "202".into());
        (StatusCode::OK, json!({"status": "success", "data": {"acuse": "<Acuse/>", "uuid": codes}}).to_string())
    }

    fn start_sw() -> (String, Shared) {
        let shared: Shared = Arc::default();
        let app = Router::new()
            .route(SW_AUTH_PATH, post(auth))
            .route(SW_STAMP_PATH, post(stamp_v4))
            .route(SW_CANCEL_CSD_PATH, post(cancel_csd))
            .route(&format!("{SW_CANCEL_UUID_PATH}{{rfc}}/{{uuid}}/{{motivo}}"), post(cancel_uuid))
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
        customer: String,
        sales: String,
        output: String,
        ret_iva: String,
        ret_isr: String,
        bank: String,
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn setup(name: &str) -> Setup {
        let f = TempFile::new(&format!("pac-{name}"));
        let mut slot = None;
        create_company(&mut slot, &f.0, "Escuela Kemper Urgate").unwrap();
        set_country(&mut slot, Some(&f.0), "MX").unwrap();
        set_company_tax(&mut slot, "EKU9003173C9", "AGU").unwrap();
        set_tax_rate(&mut slot, "STANDARD", "2010-01-01", "16").unwrap();
        set_mx_issuer(&mut slot, &MxIssuer { regimen_fiscal: "601".into(), lugar_expedicion: "20000".into(), serie: "A".into(), nombre: String::new() }).unwrap();
        let debtors = group_id(&slot, "Sundry Debtors");
        let customer = create_party_ledger(&mut slot, "Universidad Robotica Española", &debtors, "", None, Some("URE180429TM6"), None).unwrap().ledger.id;
        // A persona moral customer that withholds ISR 10% and 2/3 of the IVA.
        let input: crate::masters::LedgerInput = serde_json::from_value(json!({
            "name": "Universidad Robotica Española", "group_id": debtors, "tax_id": "URE180429TM6",
            "mailing": { "postal_code": "86991" }, "tax_class": "RET:ISR10+IVA2/3",
        }))
        .unwrap();
        crate::masters::alter_ledger(&mut slot, &customer, &input).unwrap();
        let sales = ledger(&mut slot, "Honorarios", "Sales Accounts", "");
        let classed: crate::masters::LedgerInput = serde_json::from_value(json!({ "name": "Honorarios", "group_id": group_id(&slot, "Sales Accounts"), "tax_class": "STANDARD" })).unwrap();
        crate::masters::alter_ledger(&mut slot, &sales, &classed).unwrap();
        let output = ledger(&mut slot, "IVA trasladado", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "IVA", TaxSide::Output).unwrap();
        let ret_iva = ledger(&mut slot, "IVA retenido por clientes", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &ret_iva, "RET-IVA", TaxSide::Input).unwrap();
        let ret_isr = ledger(&mut slot, "ISR retenido por clientes", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &ret_isr, "RET-ISR", TaxSide::Input).unwrap();
        let bank = ledger(&mut slot, "Banco", "Bank Accounts", "");
        let dir = std::env::temp_dir().join(format!("khataos-pac-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        set_mx_csd(&dir, &slot, CER, KEY, PASSWORD).unwrap();
        Setup { company: Mutex::new(slot), _file: f, dir, customer, sales, output, ret_iva, ret_isr, bank }
    }

    fn bill(kind: BillKind, reference: &str, amount: &str) -> BillInput {
        BillInput { kind, reference: reference.into(), amount: amount.into(), due: None }
    }

    /// Honorarios 1000 + IVA 160 − ISR 100 − IVA 106.67 = 1453.33… for a
    /// 1000 base: the customer owes 953.33. Bill "F-1".
    fn sale(s: &Setup, number: &str) -> String {
        let mut party = entry(&s.customer, DrCr::Dr, "953.33");
        party.bills = vec![bill(BillKind::New, number, "953.33")];
        let mut v = voucher("Sales", "2026-06-01", vec![
            party,
            entry(&s.ret_isr, DrCr::Dr, "100"),
            entry(&s.ret_iva, DrCr::Dr, "106.67"),
            entry(&s.sales, DrCr::Cr, "1000"),
            entry(&s.output, DrCr::Cr, "160"),
        ]);
        v.number = Some(number.into());
        post_voucher(&mut s.company.lock().unwrap(), v).unwrap().id
    }

    fn receipt(s: &Setup, date: &str, reference: &str, amount: &str) -> String {
        let mut party = entry(&s.customer, DrCr::Cr, amount);
        party.bills = vec![bill(BillKind::Against, reference, amount)];
        post_voucher(&mut s.company.lock().unwrap(), voucher("Receipt", date, vec![entry(&s.bank, DrCr::Dr, amount), party])).unwrap().id
    }

    fn opts() -> CfdiOptions {
        CfdiOptions {
            forma_pago: "99".into(),
            metodo_pago: "PPD".into(),
            uso_cfdi: "G03".into(),
            regimen_fiscal_receptor: "601".into(),
            password: PASSWORD.into(),
            ..CfdiOptions::default()
        }
    }

    fn use_pac(s: &Setup, provider: &str, base_url: &str) {
        let input = PacSettingsInput {
            provider: provider.into(),
            environment: if base_url.is_empty() { "test".into() } else { "custom".into() },
            base_url: base_url.into(),
            user: "demo".into(),
            password: "s3cret".into(),
            ..PacSettingsInput::default()
        };
        set_settings(&s.dir, &s.company.lock().unwrap(), &input).unwrap();
    }

    // ------------------------------------------------ tests

    #[test]
    fn settings_keep_secrets_out_of_the_webview_and_the_books() {
        let s = setup("settings");
        let slot = s.company.lock().unwrap();
        let fresh = settings(&s.dir, &slot).unwrap();
        assert_eq!((fresh.provider.as_str(), fresh.environment.as_str(), fresh.base_url.as_str()), ("sw", "test", "https://services.test.sw.com.mx"));
        assert!(set_settings(&s.dir, &slot, &PacSettingsInput { provider: "finkok".into(), ..Default::default() }).is_err());
        assert!(
            set_settings(&s.dir, &slot, &PacSettingsInput { provider: "sw".into(), environment: "custom".into(), base_url: "http://pac.example".into(), ..Default::default() })
                .is_err(),
            "plain http only on this computer"
        );
        let saved = set_settings(&s.dir, &slot, &PacSettingsInput {
            provider: "sw".into(),
            environment: "production".into(),
            user: "demo".into(),
            password: "s3cret".into(),
            ..Default::default()
        })
        .unwrap();
        assert!(saved.production && saved.has_password && !saved.has_token);
        assert_eq!(saved.base_url, "https://services.sw.com.mx");
        let json = serde_json::to_string(&saved).unwrap();
        assert!(!json.contains("s3cret"), "no secret reaches the webview");
        // An empty password keeps the stored one.
        let again = set_settings(&s.dir, &slot, &PacSettingsInput { provider: "sw".into(), environment: "production".into(), user: "demo".into(), ..Default::default() }).unwrap();
        assert!(again.has_password);
        assert!(!format!("{:?}", load_vault(&s.dir).unwrap().by_rfc["EKU9003173C9"]).contains("s3cret"), "Debug redacts");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(vault_path(&s.dir)).unwrap().permissions().mode() & 0o077, 0);
        }
        let raw = std::fs::read(&s._file.0).unwrap();
        assert!(!raw.windows(6).any(|w| w == b"s3cret"), "credentials are not in the books");
    }

    #[test]
    fn mock_pac_stamps_prints_pays_and_cancels() {
        let s = setup("mock");
        use_pac(&s, "mock", "");
        let invoice = sale(&s, "F1");
        // The invoice's CFDI carries the customer's withholdings.
        let draft = crate::tax_mx::cfdi_xml(&s.dir, &s.company.lock().unwrap(), &invoice, &CfdiOptions { password: String::new(), ..opts() }).unwrap();
        assert!(draft.xml.contains("<cfdi:Impuestos TotalImpuestosRetenidos=\"206.67\" TotalImpuestosTrasladados=\"160.00\"><cfdi:Retenciones><cfdi:Retencion Impuesto=\"001\" Importe=\"100.00\"/><cfdi:Retencion Impuesto=\"002\" Importe=\"106.67\"/>"), "{}", draft.xml);
        assert_eq!(draft.total, "953.33");
        assert!(!draft.warnings.iter().any(|w| w.contains("withheld in the books")), "{:?}", draft.warnings);
        assert!(stamp(&s.dir, &s.company, &invoice, &CfdiOptions { password: String::new(), ..opts() }).unwrap_err().contains("password"));

        let st = stamp(&s.dir, &s.company, &invoice, &opts()).unwrap();
        assert!(st.record.test && st.messages[0].contains("NOT a valid CFDI"));
        assert!(khata_mx::pagos::is_uuid(&st.record.uuid));
        assert_eq!((st.record.status.as_str(), st.record.metodo_pago.as_deref(), st.record.total), (VIGENTE, Some("PPD"), dec!(953.33)));
        assert!(st.record.xml.contains("<tfd:TimbreFiscalDigital") && st.record.xml.contains(MOCK_LEYENDA));
        assert!(st.qr_text.contains(&format!("id={}&re=EKU9003173C9&rr=URE180429TM6&tt=953.33&fe=", st.record.uuid)));
        assert!(st.tfd_cadena.starts_with(&format!("||1.1|{}|", st.record.uuid)));
        // Stamping again sends nothing.
        let again = stamp(&s.dir, &s.company, &invoice, &opts()).unwrap();
        assert_eq!((again.record.uuid.as_str(), again.messages[0].contains("already stamped")), (st.record.uuid.as_str(), true));
        // The print block.
        let block = print_block_html(&s.company.lock().unwrap(), VoucherId(parse_uuid(&invoice, "v").unwrap())).unwrap();
        assert!(block.contains(&st.record.uuid) && block.contains("SIN VALIDEZ FISCAL") && block.contains("<svg"));

        // Two payments: the Pago CFDIs use the stamped UUID.
        let first = receipt(&s, "2026-06-10", "F1", "500");
        let o = CfdiOptions { forma_pago: "03".into(), num_operacion: "SPEI-1".into(), ..opts() };
        let p1 = crate::tax_mx::pago_xml(&s.dir, &s.company.lock().unwrap(), &first, &o).unwrap();
        assert!(p1.xml.contains(&format!("IdDocumento=\"{}\" Serie=\"A\" Folio=\"F1\" MonedaDR=\"MXN\" EquivalenciaDR=\"1\" NumParcialidad=\"1\" ImpSaldoAnt=\"953.33\" ImpPagado=\"500.00\" ImpSaldoInsoluto=\"453.33\"", st.record.uuid)), "{}", p1.xml);
        assert!(p1.xml.contains("TotalRetencionesIVA=\"55.94\" TotalRetencionesISR=\"52.45\" TotalTrasladosBaseIVA16=\"524.48\" TotalTrasladosImpuestoIVA16=\"83.92\" MontoTotalPagos=\"500.00\""), "{}", p1.xml);
        let stamped_p1 = stamp(&s.dir, &s.company, &first, &o).unwrap();
        assert_eq!(stamped_p1.record.tipo, "P");
        let second = receipt(&s, "2026-06-20", "F1", "453.33");
        let p2 = crate::tax_mx::pago_xml(&s.dir, &s.company.lock().unwrap(), &second, &o).unwrap();
        assert!(p2.xml.contains("NumParcialidad=\"2\" ImpSaldoAnt=\"453.33\" ImpPagado=\"453.33\" ImpSaldoInsoluto=\"0.00\""), "{}", p2.xml);
        let over = receipt(&s, "2026-06-21", "F1", "1");
        assert!(crate::tax_mx::pago_xml(&s.dir, &s.company.lock().unwrap(), &over, &o).unwrap_err().contains("settled in full"));
        assert!(crate::tax_mx::pago_xml(&s.dir, &s.company.lock().unwrap(), &over, &CfdiOptions { forma_pago: "99".into(), ..o.clone() }).is_err());

        // Cancellation: motivo 01 needs the replacement; 02 is requested.
        assert!(cancel(&s.dir, &s.company, &invoice, "01", "", "").unwrap_err().contains("replaces"));
        let c = cancel(&s.dir, &s.company, &invoice, "02", "", PASSWORD).unwrap();
        assert_eq!((c.record.status.as_str(), c.record.cancel_motivo.as_deref()), (CANCEL_REQUESTED, Some("02")));
        assert!(crate::tax_mx::pago_xml(&s.dir, &s.company.lock().unwrap(), &second, &o).unwrap_err().contains("no folio fiscal"), "a CFDI being cancelled takes no payments");
        let done = confirm_cancellation(&s.company, &invoice, true).unwrap();
        assert_eq!(done.record.status, CANCELADO);
        let block = print_block_html(&s.company.lock().unwrap(), VoucherId(parse_uuid(&invoice, "v").unwrap())).unwrap();
        assert!(block.contains("CFDI CANCELADO"));
    }

    #[test]
    fn sw_sapien_adapter_against_a_mock_server() {
        let (base, sw) = start_sw();
        let s = setup("sw");
        use_pac(&s, "sw", &base);
        let invoice = sale(&s, "F7");
        let st = stamp(&s.dir, &s.company, &invoice, &opts()).unwrap();
        assert!(!st.record.test);
        assert_eq!(st.record.uuid, "1147A19D-8FD5-44F6-9C83-674974518501");
        assert_eq!((st.record.rfc_prov_certif.as_str(), st.record.no_certificado_sat.as_str()), ("SPR190613I52", "30001000000500003456"));
        assert_eq!(st.record.environment, "custom");
        {
            let m = sw.lock().unwrap();
            assert_eq!((m.auths, m.stamps), (1, 1));
            assert_eq!(m.last_auth_header, "Bearer tok1");
        }
        // An expired token signs in again once.
        sw.lock().unwrap().expire_next = true;
        let other = sale(&s, "F8");
        stamp(&s.dir, &s.company, &other, &opts()).unwrap();
        assert_eq!(sw.lock().unwrap().auths, 2);
        // Cancel by CSD: the request carries the CSD and the motive.
        let c = cancel(&s.dir, &s.company, &invoice, "01", "1147a19d-8fd5-44f6-9c83-674974518502", PASSWORD).unwrap();
        assert_eq!(c.record.status, CANCEL_REQUESTED);
        let sent = sw.lock().unwrap().last_cancel.clone().unwrap();
        assert_eq!((sent["rfc"].as_str(), sent["motivo"].as_str(), sent["foliosustitucion"].as_str()), (Some("EKU9003173C9"), Some("01"), Some("1147A19D-8FD5-44F6-9C83-674974518502")));
        assert_eq!(B64.decode(sent["b64Cer"].as_str().unwrap()).unwrap(), std::fs::read(CER).unwrap());
        assert!(cancel(&s.dir, &s.company, &other, "02", "", "wrong").is_err(), "the CSD password is checked before sending");
        // Cancel by UUID (CSD held by the PAC): 202 = already cancelled.
        let c2 = cancel(&s.dir, &s.company, &other, "02", "", "").unwrap();
        assert_eq!(c2.record.status, CANCELADO);
        assert_eq!(sw.lock().unwrap().last_cancel_path.as_deref(), Some(format!("EKU9003173C9/{}/02", c2.record.uuid).as_str()));
        // Refusals become plain messages; a bad sign-in too.
        let mut bad = PacSettingsInput { provider: "sw".into(), environment: "custom".into(), base_url: base.clone(), user: "demo".into(), password: "nope".into(), ..Default::default() };
        set_settings(&s.dir, &s.company.lock().unwrap(), &bad).unwrap();
        let third = sale(&s, "F9");
        let e = stamp(&s.dir, &s.company, &third, &opts()).unwrap_err();
        assert!(e.contains("refused the sign-in") && e.contains("inválidos"), "{e}");
        bad.base_url = "http://127.0.0.1:1".into();
        set_settings(&s.dir, &s.company.lock().unwrap(), &bad).unwrap();
        assert!(stamp(&s.dir, &s.company, &third, &opts()).unwrap_err().contains("couldn't"));
    }
}
