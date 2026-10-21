//! HMRC Making Tax Digital (MTD) for VAT: connect to HMRC, read the VAT
//! obligations, submit the 9-box return, view returns, liabilities and
//! payments. See `docs/uk-vat.md` ("Submitting to HMRC") for the setup.
//!
//! Sources (HMRC Developer Hub, read 20 Oct 2026) [PRIMARY]:
//! - VAT (MTD) API 1.0 OpenAPI file
//!   (`/api-documentation/docs/api/service/vat-api/1.0/oas/file`):
//!   endpoints, `Accept: application/vnd.hmrc.1.0+json`, scopes `read:vat`
//!   and `write:vat`, error codes, the `Receipt-ID` / `Receipt-Timestamp` /
//!   `X-CorrelationId` response headers, `Gov-Test-Scenario` (sandbox only);
//! - *User-restricted endpoints* (`/api-documentation/docs/authorisation/user-restricted-endpoints`):
//!   the authorisation-code grant; "We also support PKCE, but you do not
//!   have to use it"; the token endpoint needs `client_secret` (a missing
//!   one is `invalid_request`), so the user registers **their own** HMRC
//!   developer application: an open-source app can't ship a secret.
//!   Authorise at `test-www.tax.service.gov.uk` (sandbox) or
//!   `www.tax.service.gov.uk`; tokens from `{api}/oauth/token`. The code
//!   expires after 10 minutes; an access token lasts 4 hours; a refresh
//!   token is single-use and stops working after 18 months. Installed apps
//!   may use `http://localhost:[PORT]` as the redirect URI;
//! - *Application-restricted endpoints*: the `client_credentials` grant,
//!   used only for the Test Fraud Prevention Headers API;
//! - *Send fraud prevention data* v3.3 and *Desktop application direct*:
//!   the `Gov-Client-*` / `Gov-Vendor-*` headers (`khata_gb::fraud`);
//! - VAT (MTD) end-to-end service guide, *Obligations*: the declaration text,
//!   `finalised: true` only after the user confirms it, and `#` in a period
//!   key percent-encoded (`%23001`).
//!
//! Where things are kept (never in the books, never synced):
//! - `<app data>/mtd/settings.json`: environment, client ID, client secret,
//!   redirect port;
//! - `<app data>/mtd/tokens.json`: access and refresh tokens per
//!   environment and VRN;
//! - `<app data>/mtd/device-id`: the `Gov-Client-Device-ID` UUID.
//!
//! On Unix the folder is `0700` and the files `0600`. On Windows they
//! inherit the per-user ACL of the app data folder (`%APPDATA%`).
//! Submission receipts are shared records of the company
//! (`mtd.receipt.<environment>.<period key>`): next to the books but not in
//! the audit trail, and synced (`docs/sync-settings.md`).
//!
//! Secrets and tokens are never logged, never sent to the webview and never
//! put in error messages; the structs holding them print redacted.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Local, Months, Utc};
use khata_gb::fraud::{self, DesktopFacts, Screen, UserAgent};
use khata_store::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::api::{ApiResult, Slot, err, open_ref};

/// Both scopes: obligations, returns, liabilities and payments read; submit writes.
pub const SCOPES: &str = "read:vat write:vat";
const ACCEPT: &str = "application/vnd.hmrc.1.0+json";
/// The redirect port when none is set (a KhataOS choice; any free port the
/// user registers with HMRC works).
pub const DEFAULT_PORT: u16 = 48765;
/// How long to wait for the user to finish signing in to HMRC.
const SIGN_IN_WAIT: Duration = Duration::from_secs(5 * 60);
/// Refresh this long before the access token runs out.
const REFRESH_MARGIN: i64 = 60;
const PRODUCT: &str = "KhataOS";
/// The `Gov-Client-User-IDs` key for the KhataOS company user.
const APP_USER_KEY: &str = "khataos";
pub const DECLARATION: &str = "When you submit this VAT information you are making a legal declaration that the information is true and complete. A false declaration can result in prosecution.";
const RECEIPT_PREFIX: &str = "mtd.receipt.";

// ------------------------------------------------------------ settings

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    #[default]
    Sandbox,
    Production,
}

impl Environment {
    fn key(self) -> &'static str {
        match self {
            Self::Sandbox => "sandbox",
            Self::Production => "production",
        }
    }

    fn endpoints(self) -> Endpoints {
        match self {
            Self::Sandbox => Endpoints {
                api: "https://test-api.service.hmrc.gov.uk".into(),
                auth: "https://test-www.tax.service.gov.uk".into(),
            },
            Self::Production => Endpoints {
                api: "https://api.service.hmrc.gov.uk".into(),
                auth: "https://www.tax.service.gov.uk".into(),
            },
        }
    }
}

/// Where the APIs (`api`) and the sign-in pages (`auth`) are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub api: String,
    pub auth: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    environment: Environment,
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    client_secret: String,
    #[serde(default)]
    redirect_port: u16,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("environment", &self.environment)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("redirect_port", &self.redirect_port)
            .finish()
    }
}

impl Config {
    fn port(&self) -> u16 {
        if self.redirect_port == 0 { DEFAULT_PORT } else { self.redirect_port }
    }

    fn redirect_uri(&self) -> String {
        format!("http://localhost:{}/callback", self.port())
    }

    fn ready(&self) -> ApiResult<()> {
        if self.client_id.trim().is_empty() || self.client_secret.trim().is_empty() {
            return Err("enter your HMRC application's client ID and client secret in MTD settings first".into());
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    expires_at: DateTime<Utc>,
    #[serde(default)]
    scope: String,
    /// When the user granted access; refresh tokens stop working 18 months later.
    granted_at: DateTime<Utc>,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens")
            .field("expires_at", &self.expires_at)
            .field("scope", &self.scope)
            .field("granted_at", &self.granted_at)
            .finish_non_exhaustive()
    }
}

/// What the settings screen shows. The secret and tokens never leave Rust.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MtdSettingsDto {
    pub environment: Environment,
    pub client_id: String,
    pub has_secret: bool,
    pub redirect_port: u16,
    /// What to register as the redirect URI of the HMRC application.
    pub redirect_uri: String,
    /// The open company's VRN (nine digits), if it has a valid one.
    pub vrn: Option<String>,
    /// Tokens are held for this environment and VRN.
    pub connected: bool,
    pub access_expires_at: Option<String>,
    /// After this date HMRC needs a new sign-in (18 months from granting).
    pub reauthorise_by: Option<String>,
    /// The HMRC-required declaration, for the submit screen.
    pub declaration: String,
    /// Submissions recorded in this company file, newest first.
    pub receipts: Vec<SubmissionReceipt>,
}

// --------------------------------------------------------- private files

/// Write a file only this OS user can read (Unix `0600`, folder `0700`),
/// through a temporary file so a crash never leaves half a token file.
pub(crate) fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::other("no folder"))?;
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    let tmp = path.with_extension("tmp");
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(data)?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp, path)
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> ApiResult<T> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| format!("{} is damaged; delete it and set MTD up again", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!("couldn't read {}: {e}", path.display())),
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> ApiResult<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(err)?;
    write_private(path, &bytes).map_err(|e| format!("couldn't save {}: {e}", path.display()))
}

/// Token files are read, refreshed and written under this lock: a refresh
/// token is single-use, so two calls must not refresh at once.
static TOKENS: Mutex<()> = Mutex::new(());

fn tokens_lock() -> std::sync::MutexGuard<'static, ()> {
    TOKENS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// MTD state for this install: its folder, and (tests only) mock endpoints.
pub struct Mtd {
    dir: PathBuf,
    endpoints: Option<Endpoints>,
}

impl Mtd {
    /// `dir` is `<app data>/mtd`.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, endpoints: None }
    }

    #[cfg(test)]
    fn with_endpoints(dir: PathBuf, endpoints: Endpoints) -> Self {
        Self { dir, endpoints: Some(endpoints) }
    }

    fn endpoints(&self, env: Environment) -> Endpoints {
        self.endpoints.clone().unwrap_or_else(|| env.endpoints())
    }

    fn config(&self) -> ApiResult<Config> {
        read_json(&self.dir.join("settings.json"))
    }

    fn tokens_path(&self) -> PathBuf {
        self.dir.join("tokens.json")
    }

    fn all_tokens(&self) -> ApiResult<BTreeMap<String, Tokens>> {
        read_json(&self.tokens_path())
    }

    fn tokens(&self, key: &str) -> ApiResult<Option<Tokens>> {
        Ok(self.all_tokens()?.remove(key))
    }

    fn save_tokens(&self, key: &str, tokens: Option<Tokens>) -> ApiResult<()> {
        let mut all = self.all_tokens()?;
        match tokens {
            Some(t) => all.insert(key.to_owned(), t),
            None => all.remove(key),
        };
        write_json(&self.tokens_path(), &all)
    }

    /// The `Gov-Client-Device-ID`: a UUID made on first use and kept. If the
    /// user deletes the file a new one is made, as HMRC asks.
    pub fn device_id(&self) -> ApiResult<String> {
        let path = self.dir.join("device-id");
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(id) = uuid::Uuid::parse_str(s.trim()) {
                return Ok(id.hyphenated().to_string());
            }
        }
        let id = uuid::Uuid::new_v4().hyphenated().to_string();
        write_private(&path, id.as_bytes()).map_err(|e| format!("couldn't save the device ID: {e}"))?;
        Ok(id)
    }

    pub fn settings(&self, vrn: Option<&str>, store: Option<&Store>) -> ApiResult<MtdSettingsDto> {
        let cfg = self.config()?;
        let tokens = match vrn {
            Some(v) => self.tokens(&token_key(cfg.environment, v))?,
            None => None,
        };
        let receipts = match store {
            Some(s) => receipts(s)?,
            None => Vec::new(),
        };
        Ok(MtdSettingsDto {
            environment: cfg.environment,
            client_id: cfg.client_id.clone(),
            has_secret: !cfg.client_secret.is_empty(),
            redirect_port: cfg.port(),
            redirect_uri: cfg.redirect_uri(),
            vrn: vrn.map(str::to_owned),
            connected: tokens.is_some(),
            access_expires_at: tokens.as_ref().map(|t| t.expires_at.to_rfc3339()),
            reauthorise_by: tokens
                .as_ref()
                .and_then(|t| t.granted_at.checked_add_months(Months::new(18)))
                .map(|d| d.date_naive().to_string()),
            declaration: DECLARATION.into(),
            receipts,
        })
    }

    /// Save the settings. An empty `client_secret` keeps the one on file.
    /// Changing the environment or client ID forgets the tokens (they belong
    /// to the old application).
    pub fn set_settings(&self, environment: Environment, client_id: &str, client_secret: &str, redirect_port: u16) -> ApiResult<()> {
        let old = self.config()?;
        let client_id = client_id.trim();
        if client_id.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("the client ID has spaces in it; copy it again from the HMRC Developer Hub".into());
        }
        if client_secret.chars().any(char::is_control) {
            return Err("the client secret has invalid characters".into());
        }
        if redirect_port != 0 && redirect_port < 1024 {
            return Err("use a redirect port from 1024 to 65535".into());
        }
        let cfg = Config {
            environment,
            client_id: client_id.to_owned(),
            client_secret: if client_secret.trim().is_empty() { old.client_secret.clone() } else { client_secret.trim().to_owned() },
            redirect_port,
        };
        if old.environment != cfg.environment || old.client_id != cfg.client_id {
            let _guard = tokens_lock();
            write_json(&self.tokens_path(), &BTreeMap::<String, Tokens>::new())?;
        }
        write_json(&self.dir.join("settings.json"), &cfg)
    }

    /// Forget the tokens for this VRN (HMRC keeps the grant until it is
    /// revoked in "Manage authorised applications").
    pub fn disconnect(&self, vrn: &str) -> ApiResult<()> {
        let cfg = self.config()?;
        let _guard = tokens_lock();
        self.save_tokens(&token_key(cfg.environment, vrn), None)
    }
}

fn token_key(env: Environment, vrn: &str) -> String {
    format!("{}:{vrn}", env.key())
}

// ------------------------------------------------------------- encoding

/// Decode `%XX` and `+` (a query string or form body).
fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3])
                    .ok()
                    .filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit()))
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(q: &str) -> BTreeMap<String, String> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (pct_decode(k), pct_decode(v))
        })
        .collect()
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{}={}", fraud::pct(k), fraud::pct(v))).collect::<Vec<_>>().join("&")
}

fn random_b64(bytes: usize) -> ApiResult<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| format!("no secure random numbers: {e}"))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

/// The PKCE S256 challenge for a verifier.
fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

// ----------------------------------------------------- the device facts

/// Screens and window as the webview sees them at the time of the call.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ClientInfo {
    pub screens: Vec<ScreenInfo>,
    pub window: WindowInfo,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ScreenInfo {
    pub width: u32,
    pub height: u32,
    pub scaling_factor: f64,
    pub colour_depth: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct WindowInfo {
    pub width: u32,
    pub height: u32,
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = std::process::Command::new(cmd);
    c.args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    let out = c.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned()).filter(|s| !s.is_empty())
}

#[cfg(windows)]
fn reg_value(name: &str) -> Option<String> {
    let out = run("reg", &["query", r"HKLM\HARDWARE\DESCRIPTION\System\BIOS", "/v", name])?;
    out.lines().find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_owned())).filter(|s| !s.is_empty())
}

/// The OS and device, read once per run.
fn user_agent() -> UserAgent {
    static UA: std::sync::OnceLock<UserAgent> = std::sync::OnceLock::new();
    UA.get_or_init(|| {
        #[cfg(target_os = "macos")]
        {
            UserAgent {
                os_family: "MacOS".into(),
                os_version: run("sw_vers", &["-productVersion"]).unwrap_or_default(),
                device_manufacturer: "Apple".into(),
                device_model: run("sysctl", &["-n", "hw.model"]).unwrap_or_default(),
            }
        }
        #[cfg(windows)]
        {
            // "Microsoft Windows [Version 10.0.22631.4317]"
            let version = run("cmd", &["/C", "ver"])
                .and_then(|v| v.split("Version").nth(1).map(|s| s.trim_matches(|c: char| c == ']' || c.is_whitespace()).to_owned()))
                .unwrap_or_default();
            UserAgent {
                os_family: "Windows".into(),
                os_version: version,
                device_manufacturer: reg_value("SystemManufacturer").unwrap_or_default(),
                device_model: reg_value("SystemProductName").unwrap_or_default(),
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let release = fs::read_to_string("/etc/os-release").unwrap_or_default();
            let field = |k: &str| {
                release
                    .lines()
                    .find_map(|l| l.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
                    .map(|v| v.trim_matches('"').to_owned())
                    .unwrap_or_default()
            };
            let version = [field("NAME"), field("VERSION_ID")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
            let dmi = |f: &str| fs::read_to_string(format!("/sys/class/dmi/id/{f}")).map(|s| s.trim().to_owned()).unwrap_or_default();
            UserAgent {
                os_family: "Linux".into(),
                os_version: if version.is_empty() { run("uname", &["-r"]).unwrap_or_default() } else { version },
                device_manufacturer: dmi("sys_vendor"),
                device_model: dmi("product_name"),
            }
        }
    })
    .clone()
}

/// The user signed in to the OS: `DOMAIN\name` on Windows (HMRC's example
/// is "domain\alice"), the login name elsewhere.
fn os_user() -> String {
    let var = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    if cfg!(windows) {
        return match (var("USERDOMAIN"), var("USERNAME")) {
            (Some(d), Some(u)) => format!("{d}\\{u}"),
            (None, Some(u)) => u,
            _ => String::new(),
        };
    }
    var("USER").or_else(|| var("LOGNAME")).unwrap_or_default()
}

/// Every local address except loopback (KhataOS's choice: a loopback
/// address is the same on every device).
fn local_ips() -> Vec<IpAddr> {
    let mut out: Vec<IpAddr> = Vec::new();
    for (_, ip) in crate::lan::interface_addrs() {
        if !ip.is_loopback() && !ip.is_unspecified() && !out.contains(&ip) {
            out.push(ip);
        }
    }
    out
}

/// iOS gives apps no MAC addresses (and `mac_address` has no iOS backend).
#[cfg(target_os = "ios")]
fn mac_addresses() -> Vec<[u8; 6]> {
    Vec::new()
}

#[cfg(not(target_os = "ios"))]
fn mac_addresses() -> Vec<[u8; 6]> {
    let mut out: Vec<[u8; 6]> = Vec::new();
    if let Ok(it) = mac_address::MacAddressIterator::new() {
        for m in it {
            let b = m.bytes();
            if b.iter().any(|x| *x != 0) && !out.contains(&b) {
                out.push(b);
            }
        }
    }
    out
}

impl Mtd {
    /// The fraud-prevention headers for one request, gathered now (local
    /// IPs and their timestamp "immediately before each request").
    pub fn fraud_headers(&self, info: &ClientInfo, app_user: Option<&str>) -> ApiResult<Vec<(&'static str, String)>> {
        let facts = DesktopFacts {
            device_id: self.device_id()?,
            local_ips: local_ips(),
            local_ips_at: Utc::now(),
            mac_addresses: mac_addresses(),
            multi_factor: Vec::new(),
            screens: info
                .screens
                .iter()
                .map(|s| Screen { width: s.width, height: s.height, scaling_factor: s.scaling_factor, colour_depth: s.colour_depth })
                .collect(),
            window: (info.window.width, info.window.height),
            timezone_offset_seconds: Local::now().offset().local_minus_utc(),
            user_agent: user_agent(),
            os_user: os_user(),
            app_users: app_user.filter(|u| !u.trim().is_empty()).map(|u| vec![(APP_USER_KEY.to_owned(), u.to_owned())]).unwrap_or_default(),
            licence_ids: Vec::new(),
            product_name: PRODUCT.into(),
            versions: vec![(PRODUCT.to_owned(), env!("CARGO_PKG_VERSION").to_owned())],
        };
        fraud::desktop_app_direct(&facts).map_err(err)
    }
}

// ------------------------------------------------------------------ HTTP

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .into()
}

fn net(e: ureq::Error) -> String {
    format!("couldn't reach HMRC ({e}). Check the internet connection and try again")
}

struct Reply {
    status: u16,
    body: String,
    receipt_id: Option<String>,
    receipt_timestamp: Option<String>,
    correlation_id: Option<String>,
}

fn reply(mut resp: ureq::http::Response<ureq::Body>) -> ApiResult<Reply> {
    let header = |n: &str| resp.headers().get(n).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let (receipt_id, receipt_timestamp, correlation_id) = (header("Receipt-ID"), header("Receipt-Timestamp"), header("X-CorrelationId"));
    let status = resp.status().as_u16();
    let body = resp.body_mut().read_to_string().map_err(net)?;
    Ok(Reply { status, body, receipt_id, receipt_timestamp, correlation_id })
}

#[derive(Deserialize)]
struct WireError {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    errors: Vec<WireError>,
}

/// A plain message for an HMRC error code (VAT (MTD) API OAS and the
/// reference guide's common errors).
fn plain(code: &str, message: &str) -> String {
    let s = match code {
        "VRN_INVALID" | "INVALID_VRN" => "HMRC says the VAT registration number is not valid. Check the VAT Reg No in Company → Tax registration",
        "PERIOD_KEY_INVALID" | "INVALID_PERIODKEY" => "HMRC doesn't recognise the period key. Refresh the obligations and pick the period again",
        "INVALID_REQUEST" | "INVALID_PAYLOAD" => "HMRC refused the request as badly formed",
        "VAT_TOTAL_VALUE" => "HMRC says Box 3 is not Box 1 + Box 2",
        "VAT_NET_VALUE" => "HMRC says Box 5 is not the difference between Boxes 3 and 4",
        "INVALID_NUMERIC_VALUE" | "INVALID_MONETARY_AMOUNT" => "HMRC refused an amount on the return as out of range or not to 2 decimal places",
        "NOT_FINALISED" => "HMRC needs the declaration to be confirmed before it accepts the return",
        "DUPLICATE_SUBMISSION" => {
            "HMRC already has a VAT return for this period. A return can't be sent again: correct mistakes on a later return or tell HMRC (VAT Notice 700/45)"
        }
        "TAX_PERIOD_NOT_ENDED" => "This VAT period hasn't ended yet. Submit the return after the period's end date",
        "CLIENT_OR_AGENT_NOT_AUTHORISED" => {
            "HMRC says this business isn't signed up for Making Tax Digital for VAT, or the HMRC account you signed in with can't act for it"
        }
        "RULE_INSOLVENT_TRADER" => "HMRC's API does not handle returns for insolvent traders; contact HMRC",
        "INVALID_DATE_FROM" | "DATE_FROM_INVALID" => "HMRC refused the 'from' date",
        "INVALID_DATE_TO" | "DATE_TO_INVALID" => "HMRC refused the 'to' date",
        "INVALID_DATE_RANGE" | "DATE_RANGE_INVALID" => "The date range is too long for HMRC (at most a year)",
        "DATE_RANGE_TOO_LARGE" => "HMRC only returns VAT returns from the last four years",
        "INVALID_STATUS" => "HMRC refused the obligation status",
        "NOT_FOUND" => "HMRC has nothing for this request",
        "MISSING_CREDENTIALS" | "INVALID_CREDENTIALS" | "UNAUTHORIZED" | "INCORRECT_ACCESS_TOKEN_TYPE" => {
            "HMRC didn't accept KhataOS's sign-in. Connect to HMRC again"
        }
        "RESOURCE_FORBIDDEN" => "Your HMRC developer application isn't subscribed to the VAT (MTD) API. Subscribe it on the HMRC Developer Hub",
        "INVALID_SCOPE" => "The HMRC sign-in doesn't allow this. Connect to HMRC again",
        "FORBIDDEN" => "The HMRC account you signed in with can't see this VAT registration number",
        "MATCHING_RESOURCE_NOT_FOUND" => "HMRC doesn't know this address; the API may have changed",
        "ACCEPT_HEADER_INVALID" => "HMRC refused the API version",
        "MESSAGE_THROTTLED_OUT" => "HMRC is limiting requests from your application. Wait a minute and try again",
        "SERVER_ERROR" | "INTERNAL_SERVER_ERROR" | "GATEWAY_TIMEOUT" | "NOT_IMPLEMENTED" | "SCHEDULED_MAINTENANCE" => {
            "HMRC's service is unavailable at the moment. Try again later"
        }
        _ => "",
    };
    if s.is_empty() {
        let message = message.trim();
        if code.is_empty() { message.to_owned() } else { format!("HMRC said {code}: {message}") }
    } else {
        s.to_owned()
    }
}

fn api_error(r: &Reply) -> String {
    let parsed: Option<WireError> = serde_json::from_str(&r.body).ok();
    let mut text = match parsed {
        Some(e) if !e.errors.is_empty() => e.errors.iter().map(|x| plain(&x.code, &x.message)).collect::<Vec<_>>().join("; "),
        Some(e) if !e.code.is_empty() => plain(&e.code, &e.message),
        _ => format!("HMRC answered with HTTP {}", r.status),
    };
    if let Some(id) = &r.correlation_id {
        text.push_str(&format!(" (HMRC reference {id})"));
    }
    text
}

#[derive(Deserialize)]
struct WireToken {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Deserialize)]
struct WireTokenError {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

/// POST to `/oauth/token`. Only HMRC's `error` and `error_description`
/// fields can reach a message; the body is never shown.
fn token_request(ep: &Endpoints, fields: &[(&str, &str)]) -> ApiResult<WireToken> {
    let resp = agent()
        .post(format!("{}/oauth/token", ep.api))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(form(fields).as_str())
        .map_err(net)?;
    let r = reply(resp)?;
    if (200..300).contains(&r.status) {
        return serde_json::from_str(&r.body).map_err(|_| "HMRC's sign-in answer couldn't be read".to_owned());
    }
    let e: WireTokenError = serde_json::from_str(&r.body).unwrap_or(WireTokenError { error: String::new(), error_description: String::new() });
    Err(match e.error.as_str() {
        "invalid_client" => "HMRC refused the client ID or client secret. Check them in MTD settings".into(),
        "invalid_grant" => "The HMRC sign-in has expired or was revoked. Connect to HMRC again".into(),
        "" => format!("HMRC's sign-in answered with HTTP {}", r.status),
        other => format!("HMRC's sign-in refused the request ({other}: {})", e.error_description),
    })
}

fn tokens_from(w: WireToken, granted_at: DateTime<Utc>, old_refresh: Option<&str>) -> ApiResult<Tokens> {
    let refresh_token = w.refresh_token.or_else(|| old_refresh.map(str::to_owned)).ok_or("HMRC sent no refresh token")?;
    Ok(Tokens {
        access_token: w.access_token,
        refresh_token,
        expires_at: Utc::now() + chrono::Duration::seconds(w.expires_in.unwrap_or(4 * 3600)),
        scope: w.scope.unwrap_or_default(),
        granted_at,
    })
}

// ----------------------------------------------------------------- OAuth

/// Open the system browser: `open` (macOS), `xdg-open` (Linux and BSD),
/// `rundll32 url.dll,FileProtocolHandler` (Windows, which avoids `cmd`
/// re-parsing the `&` in the address). The app has no opener plugin.
pub fn open_in_browser(url: &str) -> ApiResult<()> {
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("open");
    #[cfg(windows)]
    let mut c = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    let mut c = std::process::Command::new("xdg-open");
    let mut child = c
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("couldn't open your web browser ({e})"))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

const PAGE_OK: &str = "<!doctype html><meta charset=utf-8><title>KhataOS</title><body style=\"font:16px system-ui;margin:3em\"><h1>Connected to HMRC</h1><p>You can close this window and go back to KhataOS.</p>";
const PAGE_REFUSED: &str = "<!doctype html><meta charset=utf-8><title>KhataOS</title><body style=\"font:16px system-ui;margin:3em\"><h1>Not connected</h1><p>HMRC did not grant access. Go back to KhataOS to try again.</p>";

fn respond(stream: &mut TcpStream, status: &str, html: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.flush();
}

/// The request target (`/callback?code=…`) of one HTTP request, if it is a GET.
fn read_target(stream: &mut TcpStream) -> Option<String> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut parts = text.lines().next()?.split_whitespace();
    (parts.next()? == "GET").then(|| parts.next().map(str::to_owned)).flatten()
}

enum Callback {
    Code(String),
    Refused(String),
}

/// Wait for HMRC to send the browser back to `http://localhost:<port>/callback`.
/// Requests to other paths, or with another `state`, are answered and ignored.
fn wait_for_callback(listeners: &[TcpListener], state: &str, deadline: Instant) -> ApiResult<Callback> {
    loop {
        if Instant::now() > deadline {
            return Err("the HMRC sign-in took too long (5 minutes). Connect again".into());
        }
        let mut got = None;
        for l in listeners {
            match l.accept() {
                Ok((s, _)) => {
                    got = Some(s);
                    break;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("the sign-in listener failed: {e}")),
            }
        }
        let Some(mut stream) = got else {
            std::thread::sleep(Duration::from_millis(40));
            continue;
        };
        let Some(target) = read_target(&mut stream) else {
            respond(&mut stream, "400 Bad Request", "");
            continue;
        };
        let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
        if path != "/callback" {
            respond(&mut stream, "404 Not Found", "");
            continue;
        }
        let q = parse_query(query);
        if q.get("state").map(String::as_str) != Some(state) {
            respond(&mut stream, "400 Bad Request", PAGE_REFUSED);
            continue;
        }
        if let Some(code) = q.get("code").filter(|c| !c.is_empty()) {
            respond(&mut stream, "200 OK", PAGE_OK);
            return Ok(Callback::Code(code.clone()));
        }
        respond(&mut stream, "200 OK", PAGE_REFUSED);
        let why = q.get("error_description").or_else(|| q.get("error")).cloned().unwrap_or_else(|| "no reason given".into());
        return Ok(Callback::Refused(why));
    }
}

fn listen(port: u16) -> ApiResult<Vec<TcpListener>> {
    let v4 = TcpListener::bind(("127.0.0.1", port)).map_err(|e| {
        format!("couldn't listen on port {port} for HMRC's answer ({e}). Close what uses it, or pick another port in MTD settings and register http://localhost:<port>/callback with HMRC")
    })?;
    let mut out = vec![v4];
    // Browsers may send "localhost" to ::1; listen there too when IPv6 is on.
    if let Ok(v6) = TcpListener::bind(("::1", port)) {
        out.push(v6);
    }
    for l in &out {
        l.set_nonblocking(true).map_err(err)?;
    }
    Ok(out)
}

impl Mtd {
    /// The whole sign-in: listen on the redirect port, open HMRC's
    /// authorisation page (`open`), wait for the code, exchange it (with the
    /// PKCE verifier) and keep the tokens for this environment and VRN.
    pub fn connect(&self, vrn: &str, open: impl FnOnce(&str) -> ApiResult<()>, wait: Duration) -> ApiResult<()> {
        let cfg = self.config()?;
        cfg.ready()?;
        let ep = self.endpoints(cfg.environment);
        let listeners = listen(cfg.port())?;
        let state = random_b64(24)?;
        let verifier = random_b64(32)?; // 43 characters, within PKCE's 43 to 128
        let redirect = cfg.redirect_uri();
        let url = format!(
            "{}/oauth/authorize?response_type=code&client_id={}&scope={}&state={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256",
            ep.auth,
            fraud::pct(&cfg.client_id),
            fraud::pct(SCOPES),
            fraud::pct(&state),
            fraud::pct(&redirect),
            fraud::pct(&challenge(&verifier)),
        );
        open(&url)?;
        let code = match wait_for_callback(&listeners, &state, Instant::now() + wait)? {
            Callback::Code(c) => c,
            Callback::Refused(why) => return Err(format!("HMRC did not grant access: {why}")),
        };
        drop(listeners);
        let w = token_request(
            &ep,
            &[
                ("client_secret", &cfg.client_secret),
                ("client_id", &cfg.client_id),
                ("grant_type", "authorization_code"),
                ("redirect_uri", &redirect),
                ("code", &code),
                ("code_verifier", &verifier),
            ],
        )?;
        let tokens = tokens_from(w, Utc::now(), None)?;
        let _guard = tokens_lock();
        self.save_tokens(&token_key(cfg.environment, vrn), Some(tokens))
    }

    /// A current access token, refreshed first when it is (nearly) out of
    /// time or `force` (HMRC said `INVALID_CREDENTIALS`).
    fn access_token(&self, cfg: &Config, vrn: &str, force: bool) -> ApiResult<String> {
        let _guard = tokens_lock();
        let key = token_key(cfg.environment, vrn);
        let t = self.tokens(&key)?.ok_or("connect to HMRC first (MTD settings → Connect)")?;
        if !force && t.expires_at - chrono::Duration::seconds(REFRESH_MARGIN) > Utc::now() {
            return Ok(t.access_token);
        }
        let ep = self.endpoints(cfg.environment);
        let refreshed = token_request(
            &ep,
            &[
                ("client_secret", &cfg.client_secret),
                ("client_id", &cfg.client_id),
                ("grant_type", "refresh_token"),
                ("refresh_token", &t.refresh_token),
            ],
        );
        match refreshed {
            Ok(w) => {
                let new = tokens_from(w, t.granted_at, Some(&t.refresh_token))?;
                let access = new.access_token.clone();
                self.save_tokens(&key, Some(new))?;
                Ok(access)
            }
            Err(e) => {
                if e.contains("expired or was revoked") {
                    self.save_tokens(&key, None)?;
                }
                Err(e)
            }
        }
    }

    /// One VAT API call with the bearer token and fraud-prevention headers,
    /// retried once after a refresh if HMRC says the token is invalid.
    fn call(&self, cfg: &Config, vrn: &str, method: &str, path: &str, body: Option<&str>, ctx: &CallContext) -> ApiResult<Reply> {
        cfg.ready()?;
        let ep = self.endpoints(cfg.environment);
        let url = format!("{}{path}", ep.api);
        let mut force = false;
        loop {
            let token = self.access_token(cfg, vrn, force)?;
            let mut headers: Vec<(&str, String)> = vec![("Accept", ACCEPT.into()), ("Authorization", format!("Bearer {token}"))];
            headers.extend(self.fraud_headers(&ctx.info, ctx.app_user.as_deref())?);
            if let Some(s) = ctx.scenario.as_deref().filter(|s| !s.trim().is_empty() && cfg.environment == Environment::Sandbox) {
                headers.push(("Gov-Test-Scenario", s.trim().to_owned()));
            }
            let a = agent();
            let resp = match (method, body) {
                ("POST", Some(b)) => {
                    let mut req = a.post(&url).header("Content-Type", "application/json");
                    for (k, v) in &headers {
                        req = req.header(*k, v.as_str());
                    }
                    req.send(b)
                }
                _ => {
                    let mut req = a.get(&url);
                    for (k, v) in &headers {
                        req = req.header(*k, v.as_str());
                    }
                    req.call()
                }
            }
            .map_err(net)?;
            let r = reply(resp)?;
            if r.status == 401 && !force {
                let code = serde_json::from_str::<WireError>(&r.body).map(|e| e.code).unwrap_or_default();
                if code == "INVALID_CREDENTIALS" {
                    force = true;
                    continue;
                }
            }
            return Ok(r);
        }
    }
}

/// What every API call needs besides its own arguments.
#[derive(Debug, Clone)]
pub struct CallContext {
    pub info: ClientInfo,
    /// The KhataOS user signed in to the company, for `Gov-Client-User-IDs`.
    pub app_user: Option<String>,
    /// A sandbox `Gov-Test-Scenario` (ignored in production).
    pub scenario: Option<String>,
}

// --------------------------------------------------------- the VAT API

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ObligationDto {
    pub start: String,
    pub end: String,
    pub due: String,
    /// "O" open, "F" fulfilled.
    pub status: String,
    pub period_key: String,
    pub received: Option<String>,
    /// KhataOS's receipt, when this file recorded a submission for the period.
    pub receipt: Option<SubmissionReceipt>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireObligation {
    start: String,
    end: String,
    due: String,
    status: String,
    period_key: String,
    #[serde(default)]
    received: Option<String>,
}

#[derive(Deserialize)]
struct WireObligations {
    #[serde(default)]
    obligations: Vec<WireObligation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubmissionReceipt {
    pub environment: Environment,
    pub vrn: String,
    pub period_key: String,
    pub from: String,
    pub to: String,
    pub processing_date: String,
    pub form_bundle_number: String,
    pub payment_indicator: Option<String>,
    pub charge_ref_number: Option<String>,
    /// HMRC's `Receipt-ID` response header.
    pub receipt_id: Option<String>,
    pub receipt_timestamp: Option<String>,
    pub correlation_id: Option<String>,
    /// When KhataOS sent it (local time, RFC 3339).
    pub submitted_at: String,
    pub submitted_by: String,
    /// The body sent (the nine boxes, `finalised: true`).
    pub return_json: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireSubmitted {
    processing_date: String,
    form_bundle_number: String,
    #[serde(default)]
    payment_indicator: Option<String>,
    #[serde(default)]
    charge_ref_number: Option<String>,
}

/// A return as HMRC holds it. Amounts are HMRC's JSON numbers as text.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ViewedReturnDto {
    pub period_key: String,
    pub vat_due_sales: String,
    pub vat_due_acquisitions: String,
    pub total_vat_due: String,
    pub vat_reclaimed_curr_period: String,
    pub net_vat_due: String,
    pub total_value_sales_ex_vat: String,
    pub total_value_purchases_ex_vat: String,
    pub total_value_goods_supplied_ex_vat: String,
    pub total_acquisitions_ex_vat: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireReturn {
    period_key: String,
    vat_due_sales: serde_json::Number,
    vat_due_acquisitions: serde_json::Number,
    total_vat_due: serde_json::Number,
    vat_reclaimed_curr_period: serde_json::Number,
    net_vat_due: serde_json::Number,
    #[serde(rename = "totalValueSalesExVAT")]
    total_value_sales_ex_vat: serde_json::Number,
    #[serde(rename = "totalValuePurchasesExVAT")]
    total_value_purchases_ex_vat: serde_json::Number,
    #[serde(rename = "totalValueGoodsSuppliedExVAT")]
    total_value_goods_supplied_ex_vat: serde_json::Number,
    #[serde(rename = "totalAcquisitionsExVAT")]
    total_acquisitions_ex_vat: serde_json::Number,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LiabilityDto {
    pub from: Option<String>,
    pub to: Option<String>,
    /// HMRC's charge type, e.g. "VAT Return Debit Charge".
    pub kind: String,
    pub original_amount: String,
    pub outstanding_amount: Option<String>,
    pub due: Option<String>,
}

#[derive(Deserialize)]
struct WirePeriod {
    from: String,
    to: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireLiability {
    #[serde(default)]
    tax_period: Option<WirePeriod>,
    #[serde(rename = "type")]
    kind: String,
    original_amount: serde_json::Number,
    #[serde(default)]
    outstanding_amount: Option<serde_json::Number>,
    #[serde(default)]
    due: Option<String>,
}

#[derive(Deserialize)]
struct WireLiabilities {
    #[serde(default)]
    liabilities: Vec<WireLiability>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PaymentDto {
    pub amount: String,
    pub received: Option<String>,
}

#[derive(Deserialize)]
struct WirePayment {
    amount: serde_json::Number,
    #[serde(default)]
    received: Option<String>,
}

#[derive(Deserialize)]
struct WirePayments {
    #[serde(default)]
    payments: Vec<WirePayment>,
}

fn date_arg(s: &str, what: &str) -> ApiResult<String> {
    crate::api::parse_date(s).map(|d| d.to_string()).map_err(|_| format!("the {what} date {s:?} is not a date"))
}

fn ok_json<T: for<'de> Deserialize<'de>>(r: &Reply, what: &str) -> ApiResult<T> {
    serde_json::from_str(&r.body).map_err(|_| format!("HMRC's {what} couldn't be read"))
}

impl Mtd {
    /// `GET /organisations/vat/{vrn}/obligations`. `status` "O" (open; the
    /// dates may then be empty), "F" (fulfilled) or `None` (both). HMRC's
    /// `NOT_FOUND` ("no data") is an empty list.
    pub fn obligations(&self, vrn: &str, from: &str, to: &str, status: Option<&str>, ctx: &CallContext) -> ApiResult<Vec<ObligationDto>> {
        let cfg = self.config()?;
        let mut q: Vec<String> = Vec::new();
        if !from.trim().is_empty() {
            q.push(format!("from={}", date_arg(from, "from")?));
        }
        if !to.trim().is_empty() {
            q.push(format!("to={}", date_arg(to, "to")?));
        }
        match status.map(str::trim).filter(|s| !s.is_empty()) {
            Some(s @ ("O" | "F")) => q.push(format!("status={s}")),
            Some(other) => return Err(format!("the obligation status is O or F, not {other:?}")),
            None if q.is_empty() => return Err("give a date range, or ask for open obligations only".into()),
            None => {}
        }
        let path = format!("/organisations/vat/{vrn}/obligations?{}", q.join("&"));
        let r = self.call(&cfg, vrn, "GET", &path, None, ctx)?;
        if r.status == 404 {
            return Ok(Vec::new());
        }
        if r.status != 200 {
            return Err(api_error(&r));
        }
        let w: WireObligations = ok_json(&r, "obligations")?;
        Ok(w.obligations
            .into_iter()
            .map(|o| ObligationDto { start: o.start, end: o.end, due: o.due, status: o.status, period_key: o.period_key, received: o.received, receipt: None })
            .collect())
    }

    /// `POST /organisations/vat/{vrn}/returns`: send the return for an open
    /// obligation. `build(from, to)` renders the body from the books
    /// (`finalised: true`); it runs only after the period key is found among
    /// HMRC's open obligations, so the dates are HMRC's.
    pub fn submit(
        &self,
        vrn: &str,
        period_key: &str,
        declaration_accepted: bool,
        ctx: &CallContext,
        build: impl FnOnce(&str, &str) -> ApiResult<String>,
    ) -> ApiResult<SubmissionReceipt> {
        if !declaration_accepted {
            return Err("Tick the declaration before you submit the return to HMRC".into());
        }
        let period_key = period_key.trim();
        if !khata_gb::mtd::valid_period_key(period_key) {
            return Err(format!("{period_key:?} is not an HMRC period key"));
        }
        let cfg = self.config()?;
        let open = self.obligations(vrn, "", "", Some("O"), ctx)?;
        let ob = open
            .iter()
            .find(|o| o.period_key == period_key)
            .ok_or_else(|| format!("HMRC has no open VAT obligation with period key {period_key}. Refresh the obligations"))?;
        let body = build(&ob.start, &ob.end)?;
        let r = self.call(&cfg, vrn, "POST", &format!("/organisations/vat/{vrn}/returns"), Some(&body), ctx)?;
        if r.status != 201 && r.status != 200 {
            return Err(api_error(&r));
        }
        let w: WireSubmitted = ok_json(&r, "receipt")?;
        Ok(SubmissionReceipt {
            environment: cfg.environment,
            vrn: vrn.to_owned(),
            period_key: period_key.to_owned(),
            from: ob.start.clone(),
            to: ob.end.clone(),
            processing_date: w.processing_date,
            form_bundle_number: w.form_bundle_number,
            payment_indicator: w.payment_indicator,
            charge_ref_number: w.charge_ref_number,
            receipt_id: r.receipt_id,
            receipt_timestamp: r.receipt_timestamp,
            correlation_id: r.correlation_id,
            submitted_at: Local::now().to_rfc3339(),
            submitted_by: ctx.app_user.clone().unwrap_or_else(os_user),
            return_json: body,
        })
    }

    /// `GET /organisations/vat/{vrn}/returns/{periodKey}` (`#` encoded).
    pub fn view_return(&self, vrn: &str, period_key: &str, ctx: &CallContext) -> ApiResult<ViewedReturnDto> {
        let period_key = period_key.trim();
        if !khata_gb::mtd::valid_period_key(period_key) {
            return Err(format!("{period_key:?} is not an HMRC period key"));
        }
        let cfg = self.config()?;
        let r = self.call(&cfg, vrn, "GET", &format!("/organisations/vat/{vrn}/returns/{}", fraud::pct(period_key)), None, ctx)?;
        if r.status != 200 {
            return Err(api_error(&r));
        }
        let w: WireReturn = ok_json(&r, "return")?;
        Ok(ViewedReturnDto {
            period_key: w.period_key,
            vat_due_sales: w.vat_due_sales.to_string(),
            vat_due_acquisitions: w.vat_due_acquisitions.to_string(),
            total_vat_due: w.total_vat_due.to_string(),
            vat_reclaimed_curr_period: w.vat_reclaimed_curr_period.to_string(),
            net_vat_due: w.net_vat_due.to_string(),
            total_value_sales_ex_vat: w.total_value_sales_ex_vat.to_string(),
            total_value_purchases_ex_vat: w.total_value_purchases_ex_vat.to_string(),
            total_value_goods_supplied_ex_vat: w.total_value_goods_supplied_ex_vat.to_string(),
            total_acquisitions_ex_vat: w.total_acquisitions_ex_vat.to_string(),
        })
    }

    fn range_path(vrn: &str, what: &str, from: &str, to: &str) -> ApiResult<String> {
        Ok(format!("/organisations/vat/{vrn}/{what}?from={}&to={}", date_arg(from, "from")?, date_arg(to, "to")?))
    }

    /// `GET /organisations/vat/{vrn}/liabilities` (at most 365 days).
    pub fn liabilities(&self, vrn: &str, from: &str, to: &str, ctx: &CallContext) -> ApiResult<Vec<LiabilityDto>> {
        let cfg = self.config()?;
        let r = self.call(&cfg, vrn, "GET", &Self::range_path(vrn, "liabilities", from, to)?, None, ctx)?;
        if r.status == 404 {
            return Ok(Vec::new());
        }
        if r.status != 200 {
            return Err(api_error(&r));
        }
        let w: WireLiabilities = ok_json(&r, "liabilities")?;
        Ok(w.liabilities
            .into_iter()
            .map(|l| LiabilityDto {
                from: l.tax_period.as_ref().map(|p| p.from.clone()),
                to: l.tax_period.map(|p| p.to),
                kind: l.kind,
                original_amount: l.original_amount.to_string(),
                outstanding_amount: l.outstanding_amount.map(|n| n.to_string()),
                due: l.due,
            })
            .collect())
    }

    /// `GET /organisations/vat/{vrn}/payments` (at most 365 days).
    pub fn payments(&self, vrn: &str, from: &str, to: &str, ctx: &CallContext) -> ApiResult<Vec<PaymentDto>> {
        let cfg = self.config()?;
        let r = self.call(&cfg, vrn, "GET", &Self::range_path(vrn, "payments", from, to)?, None, ctx)?;
        if r.status == 404 {
            return Ok(Vec::new());
        }
        if r.status != 200 {
            return Err(api_error(&r));
        }
        let w: WirePayments = ok_json(&r, "payments")?;
        Ok(w.payments.into_iter().map(|p| PaymentDto { amount: p.amount.to_string(), received: p.received }).collect())
    }
}

// ------------------------------------- Test Fraud Prevention Headers API

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HeaderProblem {
    pub code: String,
    pub message: String,
    pub headers: Vec<String>,
}

/// HMRC's verdict on one request's headers.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HeaderCheckDto {
    pub spec_version: Option<String>,
    /// VALID_HEADERS, POTENTIALLY_INVALID_HEADERS or INVALID_HEADERS.
    pub code: String,
    pub message: String,
    pub errors: Vec<HeaderProblem>,
    /// HMRC's "advisories".
    pub warnings: Vec<HeaderProblem>,
    /// The headers that were sent, for checking by eye.
    pub sent: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct WireProblem {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    headers: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireCheck {
    #[serde(default)]
    spec_version: Option<String>,
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    errors: Vec<WireProblem>,
    #[serde(default)]
    warnings: Vec<WireProblem>,
}

/// HMRC's feedback on the latest sandbox request to each VAT endpoint.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FeedbackDto {
    pub method: String,
    pub path: String,
    pub request_timestamp: String,
    pub code: String,
    /// "header: message" for each error and warning.
    pub problems: Vec<String>,
}

impl Mtd {
    /// An application-restricted token (`client_credentials`), held only
    /// for this call.
    fn app_token(&self, cfg: &Config) -> ApiResult<String> {
        cfg.ready()?;
        if cfg.environment != Environment::Sandbox {
            return Err("test the fraud-prevention headers in the sandbox: switch MTD settings to Sandbox".into());
        }
        let ep = self.endpoints(cfg.environment);
        let w = token_request(&ep, &[("client_secret", &cfg.client_secret), ("client_id", &cfg.client_id), ("grant_type", "client_credentials")])?;
        Ok(w.access_token)
    }

    /// `GET /test/fraud-prevention-headers/validate` with this device's headers.
    pub fn test_headers(&self, info: &ClientInfo, app_user: Option<&str>) -> ApiResult<HeaderCheckDto> {
        let cfg = self.config()?;
        let token = self.app_token(&cfg)?;
        let ep = self.endpoints(cfg.environment);
        let fh = self.fraud_headers(info, app_user)?;
        let mut req = agent()
            .get(format!("{}/test/fraud-prevention-headers/validate", ep.api))
            .header("Accept", ACCEPT)
            .header("Authorization", format!("Bearer {token}"));
        for (k, v) in &fh {
            req = req.header(*k, v.as_str());
        }
        let r = reply(req.call().map_err(net)?)?;
        if r.status != 200 {
            return Err(api_error(&r));
        }
        let w: WireCheck = ok_json(&r, "header check")?;
        let conv = |v: Vec<WireProblem>| v.into_iter().map(|p| HeaderProblem { code: p.code, message: p.message, headers: p.headers }).collect();
        Ok(HeaderCheckDto {
            spec_version: w.spec_version,
            code: w.code,
            message: w.message,
            errors: conv(w.errors),
            warnings: conv(w.warnings),
            sent: fh.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        })
    }

    /// `GET /test/fraud-prevention-headers/vat-mtd/validation-feedback`.
    pub fn header_feedback(&self) -> ApiResult<Vec<FeedbackDto>> {
        let cfg = self.config()?;
        let token = self.app_token(&cfg)?;
        let ep = self.endpoints(cfg.environment);
        let r = reply(
            agent()
                .get(format!(
                    "{}/test/fraud-prevention-headers/vat-mtd/validation-feedback?connectionMethod={}",
                    ep.api,
                    fraud::CONNECTION_METHOD
                ))
                .header("Accept", ACCEPT)
                .header("Authorization", format!("Bearer {token}"))
                .call()
                .map_err(net)?,
        )?;
        if r.status == 404 {
            return Ok(Vec::new());
        }
        if r.status != 200 {
            return Err(api_error(&r));
        }
        let v: serde_json::Value = ok_json(&r, "header feedback")?;
        let s = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_owned();
        let strings = |v: Option<&serde_json::Value>| -> Vec<String> {
            v.and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default()
        };
        let mut out = Vec::new();
        for req in v.get("requests").and_then(|x| x.as_array()).into_iter().flatten() {
            let mut problems = Vec::new();
            for h in req.get("headers").and_then(|x| x.as_array()).into_iter().flatten() {
                let name = s(h, "header");
                for m in strings(h.get("errors")).into_iter().chain(strings(h.get("warnings"))) {
                    problems.push(format!("{name}: {m}"));
                }
            }
            for c in req.get("crossValidation").and_then(|x| x.as_array()).into_iter().flatten() {
                for m in strings(c.get("errors")) {
                    problems.push(format!("{}: {m}", strings(c.get("headers")).join(", ")));
                }
            }
            out.push(FeedbackDto { method: s(req, "method"), path: s(req, "path"), request_timestamp: s(req, "requestTimestamp"), code: s(req, "code"), problems });
        }
        Ok(out)
    }
}

// ---------------------------------------------- the company file's part

fn receipt_key(env: Environment, period_key: &str) -> String {
    format!("{RECEIPT_PREFIX}{}.{period_key}", env.key())
}

/// Receipts recorded in this file, newest first.
pub fn receipts(store: &Store) -> ApiResult<Vec<SubmissionReceipt>> {
    let mut out: Vec<SubmissionReceipt> = store
        .records(RECEIPT_PREFIX)
        .map_err(err)?
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_str(&v).ok())
        .collect();
    out.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
    Ok(out)
}

fn record_receipt(store: &Store, r: &SubmissionReceipt) -> ApiResult<()> {
    let json = serde_json::to_string(r).map_err(err)?;
    // A shared record of the company (`docs/sync-settings.md`); the OAuth
    // tokens stay in app data.
    store.set_record(&receipt_key(r.environment, &r.period_key), Some(&json), &crate::api::os_user()).map(|_| ()).map_err(err)
}

fn lock(slot: &Mutex<Slot>) -> ApiResult<std::sync::MutexGuard<'_, Slot>> {
    slot.lock().map_err(|_| "company state is poisoned".to_owned())
}

/// The open company's VRN (nine digits), checked for reading.
pub fn company_vrn(slot: &Slot) -> ApiResult<String> {
    let book = open_ref(slot)?;
    if book.country() != Some(khata_gb::COUNTRY.code) {
        return Err("Making Tax Digital is for companies in the United Kingdom".into());
    }
    let id = book.profile().and_then(|p| p.tax_id.clone()).unwrap_or_default();
    khata_gb::vrn::vrn(&id).ok_or_else(|| "enter the company's VAT Reg No in Company → Tax registration first".into())
}

fn vrn_and_user(slot: &Mutex<Slot>) -> ApiResult<(String, Option<String>)> {
    let g = lock(slot)?;
    Ok((company_vrn(&g)?, crate::security::signed_in_name()))
}

fn store_of(slot: &Slot) -> Option<&Store> {
    slot.as_ref().map(|(s, _)| s)
}

/// Settings for the screen, with the open company's VRN and receipts.
pub fn settings_cmd(mtd: &Mtd, slot: &Mutex<Slot>) -> ApiResult<MtdSettingsDto> {
    let g = lock(slot)?;
    let vrn = company_vrn(&g).ok();
    mtd.settings(vrn.as_deref(), if vrn.is_some() { store_of(&g) } else { None })
}

pub fn set_settings_cmd(
    mtd: &Mtd,
    slot: &Mutex<Slot>,
    environment: Environment,
    client_id: &str,
    client_secret: &str,
    redirect_port: u16,
) -> ApiResult<MtdSettingsDto> {
    mtd.set_settings(environment, client_id, client_secret, redirect_port)?;
    settings_cmd(mtd, slot)
}

/// Connect needs Accountant rights: it lets KhataOS file for the business.
pub fn connect_cmd(mtd: &Mtd, slot: &Mutex<Slot>, open: impl FnOnce(&str) -> ApiResult<()>) -> ApiResult<MtdSettingsDto> {
    let vrn = {
        let g = lock(slot)?;
        let vrn = company_vrn(&g)?;
        crate::api::require(open_ref(&g)?, khata_core::UserRole::Accountant)?;
        vrn
    };
    mtd.connect(&vrn, open, SIGN_IN_WAIT)?;
    settings_cmd(mtd, slot)
}

pub fn disconnect_cmd(mtd: &Mtd, slot: &Mutex<Slot>) -> ApiResult<MtdSettingsDto> {
    let (vrn, _) = vrn_and_user(slot)?;
    mtd.disconnect(&vrn)?;
    settings_cmd(mtd, slot)
}

fn ctx(info: ClientInfo, app_user: Option<String>, scenario: Option<String>) -> CallContext {
    CallContext { info, app_user, scenario }
}

/// Obligations, each with this file's receipt for the period if any.
pub fn obligations_cmd(
    mtd: &Mtd,
    slot: &Mutex<Slot>,
    from: &str,
    to: &str,
    status: Option<&str>,
    info: ClientInfo,
    scenario: Option<String>,
) -> ApiResult<Vec<ObligationDto>> {
    let (vrn, user) = vrn_and_user(slot)?;
    let mut list = mtd.obligations(&vrn, from, to, status, &ctx(info, user, scenario))?;
    let env = mtd.config()?.environment;
    let g = lock(slot)?;
    if let Some(store) = store_of(&g) {
        for o in &mut list {
            o.receipt = store
                .record(&receipt_key(env, &o.period_key))
                .map_err(err)?
                .and_then(|j| serde_json::from_str(&j).ok());
        }
    }
    Ok(list)
}

/// Submit the books' return for an open obligation and record the receipt
/// in the company file. The lock is not held while HMRC is called.
pub fn submit_cmd(
    mtd: &Mtd,
    slot: &Mutex<Slot>,
    period_key: &str,
    declaration_accepted: bool,
    info: ClientInfo,
    scenario: Option<String>,
) -> ApiResult<SubmissionReceipt> {
    let (vrn, user) = {
        let g = lock(slot)?;
        let vrn = company_vrn(&g)?;
        crate::api::require(open_ref(&g)?, khata_core::UserRole::Accountant)?;
        // A copy shared read-only (Viewer) can't file for the business.
        crate::lan::check_writable(store_of(&g).ok_or("no company is open")?)?;
        (vrn, crate::security::signed_in_name())
    };
    let receipt = mtd.submit(&vrn, period_key, declaration_accepted, &ctx(info, user, scenario), |from, to| {
        let g = lock(slot)?;
        let dto = crate::vat_gb::vat_return_gb(&g, from, to)?;
        khata_gb::mtd::vat_return_json(&dto.ret, period_key.trim(), true).map_err(err)
    })?;
    let g = lock(slot)?;
    let saved = match store_of(&g) {
        Some(store) => record_receipt(store, &receipt),
        None => Err("the company was closed".into()),
    };
    if let Err(e) = saved {
        return Err(format!(
            "HMRC accepted the return (form bundle {}, processed {}{}), but KhataOS couldn't record the receipt ({e}). Write these down",
            receipt.form_bundle_number,
            receipt.processing_date,
            receipt.receipt_id.as_deref().map(|r| format!(", receipt {r}")).unwrap_or_default()
        ));
    }
    Ok(receipt)
}

pub fn view_return_cmd(mtd: &Mtd, slot: &Mutex<Slot>, period_key: &str, info: ClientInfo) -> ApiResult<ViewedReturnDto> {
    let (vrn, user) = vrn_and_user(slot)?;
    mtd.view_return(&vrn, period_key, &ctx(info, user, None))
}

pub fn liabilities_cmd(mtd: &Mtd, slot: &Mutex<Slot>, from: &str, to: &str, info: ClientInfo, scenario: Option<String>) -> ApiResult<Vec<LiabilityDto>> {
    let (vrn, user) = vrn_and_user(slot)?;
    mtd.liabilities(&vrn, from, to, &ctx(info, user, scenario))
}

pub fn payments_cmd(mtd: &Mtd, slot: &Mutex<Slot>, from: &str, to: &str, info: ClientInfo, scenario: Option<String>) -> ApiResult<Vec<PaymentDto>> {
    let (vrn, user) = vrn_and_user(slot)?;
    mtd.payments(&vrn, from, to, &ctx(info, user, scenario))
}

pub fn test_headers_cmd(mtd: &Mtd, info: ClientInfo) -> ApiResult<HeaderCheckDto> {
    mtd.test_headers(&info, crate::security::signed_in_name().as_deref())
}

#[cfg(test)]
mod tests {
    //! A local mock of HMRC (axum): the OAuth code exchange with PKCE,
    //! refresh, the VAT endpoints and the header validator. Nothing here
    //! reaches HMRC.
    use super::*;
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::extract::{Path as AxPath, RawQuery, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::Response;
    use axum::routing::{get, post};
    use khata_core::{DrCr, TaxSide};

    use crate::api::tests::{TempFile, entry, ledger, voucher};
    use crate::api::{create_company, post_voucher, set_country};
    use crate::tax::{set_company_tax, set_ledger_tax, set_tax_rate};

    const VRN: &str = "562235945";

    #[derive(Default)]
    struct Mock {
        challenge: Option<String>,
        redirect: Option<String>,
        code_used: bool,
        access: Vec<String>,
        refresh: Vec<String>,
        issued: u32,
        grants: Vec<String>,
        last_headers: Option<HeaderMap>,
        submitted: BTreeMap<String, String>,
    }

    type Shared = Arc<Mutex<Mock>>;

    fn json(status: StatusCode, body: serde_json::Value, extra: &[(&str, &str)]) -> Response {
        let mut b = Response::builder().status(status).header("Content-Type", "application/json").header("X-CorrelationId", "c75f40a6-a3df-4429-a697-471eeec46435");
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        b.body(Body::from(body.to_string())).unwrap()
    }

    fn hmrc_err(status: StatusCode, code: &str) -> Response {
        json(status, serde_json::json!({ "code": code, "message": code.to_lowercase() }), &[])
    }

    fn issue(m: &mut Mock) -> Response {
        m.issued += 1;
        let (a, r) = (format!("at-{}", m.issued), format!("rt-{}", m.issued));
        m.access.push(a.clone());
        m.refresh.push(r.clone());
        json(StatusCode::OK, serde_json::json!({ "access_token": a, "refresh_token": r, "expires_in": 14400, "token_type": "bearer", "scope": SCOPES }), &[])
    }

    async fn token(State(s): State<Shared>, body: String) -> Response {
        let f = parse_query(&body);
        let mut m = s.lock().unwrap();
        let grant = f.get("grant_type").cloned().unwrap_or_default();
        m.grants.push(grant.clone());
        let bad = |e: &str| json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": e, "error_description": "nope" }), &[]);
        if f.get("client_id").map(String::as_str) != Some("cid") || f.get("client_secret").map(String::as_str) != Some("csecret") {
            return json(StatusCode::UNAUTHORIZED, serde_json::json!({ "error": "invalid_client", "error_description": "invalid client id or secret" }), &[]);
        }
        match grant.as_str() {
            "authorization_code" => {
                if f.get("code").map(String::as_str) != Some("good-code") || m.code_used {
                    return bad("invalid_request");
                }
                if f.get("redirect_uri") != m.redirect.as_ref() {
                    return bad("invalid_request");
                }
                let verifier = f.get("code_verifier").cloned().unwrap_or_default();
                if !(43..=128).contains(&verifier.len()) || Some(challenge(&verifier)) != m.challenge {
                    return bad("invalid_grant");
                }
                m.code_used = true;
                issue(&mut m)
            }
            "refresh_token" => {
                let rt = f.get("refresh_token").cloned().unwrap_or_default();
                let Some(i) = m.refresh.iter().position(|r| *r == rt) else { return bad("invalid_grant") };
                m.refresh.remove(i); // single use
                issue(&mut m)
            }
            "client_credentials" => json(StatusCode::OK, serde_json::json!({ "access_token": "app-token", "expires_in": 14400, "token_type": "bearer" }), &[]),
            _ => bad("invalid_request"),
        }
    }

    /// The checks every VAT endpoint makes (records the headers); the
    /// refusal, if any.
    fn refused(s: &Shared, h: &HeaderMap) -> Option<Response> {
        let mut m = s.lock().unwrap();
        m.last_headers = Some(h.clone());
        if h.get("Accept").and_then(|v| v.to_str().ok()) != Some(ACCEPT) {
            return Some(hmrc_err(StatusCode::NOT_ACCEPTABLE, "ACCEPT_HEADER_INVALID"));
        }
        let bearer = h.get("Authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or_default();
        if !m.access.iter().any(|a| a == bearer) {
            return Some(hmrc_err(StatusCode::UNAUTHORIZED, "INVALID_CREDENTIALS"));
        }
        None
    }

    fn scenario(h: &HeaderMap) -> &str {
        h.get("Gov-Test-Scenario").and_then(|v| v.to_str().ok()).unwrap_or_default()
    }

    async fn obligations(State(s): State<Shared>, AxPath(vrn): AxPath<String>, RawQuery(q): RawQuery, h: HeaderMap) -> Response {
        if let Some(r) = refused(&s, &h) {
            return r;
        }
        if vrn != VRN {
            return hmrc_err(StatusCode::BAD_REQUEST, "VRN_INVALID");
        }
        if scenario(&h) == "NOT_FOUND" {
            return hmrc_err(StatusCode::NOT_FOUND, "NOT_FOUND");
        }
        let q = parse_query(&q.unwrap_or_default());
        let all = serde_json::json!([
            { "start": "2017-01-01", "end": "2017-03-31", "due": "2017-05-07", "status": "F", "periodKey": "18A1", "received": "2017-05-06" },
            { "start": "2017-04-01", "end": "2017-06-30", "due": "2017-08-07", "status": "O", "periodKey": "18A2" },
            { "start": "2017-07-01", "end": "2017-09-30", "due": "2017-11-07", "status": "O", "periodKey": "#001" }
        ]);
        let list: Vec<_> = all.as_array().unwrap().iter().filter(|o| q.get("status").is_none_or(|st| o["status"] == st.as_str())).cloned().collect();
        json(StatusCode::OK, serde_json::json!({ "obligations": list }), &[])
    }

    async fn submit(State(s): State<Shared>, AxPath(vrn): AxPath<String>, h: HeaderMap, body: String) -> Response {
        if let Some(r) = refused(&s, &h) {
            return r;
        }
        if vrn != VRN {
            return hmrc_err(StatusCode::BAD_REQUEST, "VRN_INVALID");
        }
        if h.get("Content-Type").and_then(|v| v.to_str().ok()) != Some("application/json") {
            return hmrc_err(StatusCode::BAD_REQUEST, "INVALID_REQUEST");
        }
        if scenario(&h) == "TAX_PERIOD_NOT_ENDED" {
            return hmrc_err(StatusCode::FORBIDDEN, "TAX_PERIOD_NOT_ENDED");
        }
        let v: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => return hmrc_err(StatusCode::BAD_REQUEST, "INVALID_REQUEST"),
        };
        if v["finalised"] != serde_json::json!(true) {
            return hmrc_err(StatusCode::FORBIDDEN, "NOT_FINALISED");
        }
        let key = v["periodKey"].as_str().unwrap_or_default().to_owned();
        let mut m = s.lock().unwrap();
        if m.submitted.contains_key(&key) {
            return hmrc_err(StatusCode::FORBIDDEN, "DUPLICATE_SUBMISSION");
        }
        m.submitted.insert(key, body);
        json(
            StatusCode::CREATED,
            serde_json::json!({ "processingDate": "2018-01-16T08:20:27.895+0000", "paymentIndicator": "BANK", "formBundleNumber": "256660290587", "chargeRefNumber": "aCxFaNx0FZsCvyWF" }),
            &[("Receipt-ID", "2dd537bc-4244-4ebf-bac9-96321be13cdc"), ("Receipt-Timestamp", "2018-02-14T09:32:15Z")],
        )
    }

    async fn view(State(s): State<Shared>, AxPath((_vrn, key)): AxPath<(String, String)>, h: HeaderMap) -> Response {
        if let Some(r) = refused(&s, &h) {
            return r;
        }
        let m = s.lock().unwrap();
        let Some(body) = m.submitted.get(&key) else { return hmrc_err(StatusCode::NOT_FOUND, "NOT_FOUND") };
        let mut v: serde_json::Value = serde_json::from_str(body).unwrap();
        v.as_object_mut().unwrap().remove("finalised");
        json(StatusCode::OK, v, &[])
    }

    async fn liabilities(State(s): State<Shared>, h: HeaderMap) -> Response {
        if let Some(r) = refused(&s, &h) {
            return r;
        }
        json(
            StatusCode::OK,
            serde_json::json!({ "liabilities": [{ "taxPeriod": { "from": "2017-04-01", "to": "2017-06-30" }, "type": "VAT Return Debit Charge", "originalAmount": 463872, "outstandingAmount": 463872.5, "due": "2017-08-07" }] }),
            &[],
        )
    }

    async fn payments(State(s): State<Shared>, h: HeaderMap) -> Response {
        if let Some(r) = refused(&s, &h) {
            return r;
        }
        json(StatusCode::OK, serde_json::json!({ "payments": [{ "amount": 1534.65, "received": "2017-02-11" }] }), &[])
    }

    async fn validate(State(s): State<Shared>, h: HeaderMap) -> Response {
        s.lock().unwrap().last_headers = Some(h.clone());
        if h.get("Authorization").and_then(|v| v.to_str().ok()) != Some("Bearer app-token") {
            return hmrc_err(StatusCode::UNAUTHORIZED, "INVALID_CREDENTIALS");
        }
        let missing = h.get("Gov-Client-Connection-Method").is_none();
        json(
            StatusCode::OK,
            serde_json::json!({
                "specVersion": "3.3",
                "code": if missing { "INVALID_HEADERS" } else { "POTENTIALLY_INVALID_HEADERS" },
                "message": "At least 1 header is potentially invalid",
                "warnings": [{ "code": "EMPTY_HEADER", "message": "Header required", "headers": ["gov-vendor-license-ids"] }]
            }),
            &[],
        )
    }

    fn serve(state: Shared) -> String {
        let app = Router::new()
            .route("/oauth/token", post(token))
            .route("/organisations/vat/{vrn}/obligations", get(obligations))
            .route("/organisations/vat/{vrn}/returns", post(submit))
            .route("/organisations/vat/{vrn}/returns/{key}", get(view))
            .route("/organisations/vat/{vrn}/liabilities", get(liabilities))
            .route("/organisations/vat/{vrn}/payments", get(payments))
            .route("/test/fraud-prevention-headers/validate", get(validate))
            .with_state(state);
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        l.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                let l = tokio::net::TcpListener::from_std(l).unwrap();
                axum::serve(l, app).await.unwrap();
            });
        });
        format!("http://{addr}")
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("khataos-mtd-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn info() -> ClientInfo {
        ClientInfo {
            screens: vec![ScreenInfo { width: 1920, height: 1080, scaling_factor: 2.0, colour_depth: 24 }],
            window: WindowInfo { width: 1256, height: 803 },
        }
    }

    fn cx(scenario: Option<&str>) -> CallContext {
        CallContext { info: info(), app_user: Some("Alice Smith".into()), scenario: scenario.map(str::to_owned) }
    }

    /// A mock HMRC and an MTD folder connected to it through the real
    /// OAuth flow: the "browser" checks HMRC's authorise URL, then calls the
    /// localhost callback.
    fn connected(name: &str) -> (Shared, Mtd) {
        let shared: Shared = Arc::default();
        let url = serve(shared.clone());
        let mtd = Mtd::with_endpoints(temp_dir(name), Endpoints { api: url.clone(), auth: url.clone() });
        let port = free_port();
        mtd.set_settings(Environment::Sandbox, " cid ", "csecret", port).unwrap();
        let mut browser = None;
        let s2 = shared.clone();
        mtd.connect(
            VRN,
            |auth_url| {
                let (base, q) = auth_url.split_once('?').unwrap();
                assert_eq!(base, format!("{url}/oauth/authorize"));
                let q = parse_query(q);
                assert_eq!(q["response_type"], "code");
                assert_eq!(q["client_id"], "cid");
                assert_eq!(q["scope"], "read:vat write:vat");
                assert_eq!(q["redirect_uri"], format!("http://localhost:{port}/callback"));
                assert_eq!(q["code_challenge_method"], "S256");
                assert_eq!(q["code_challenge"].len(), 43);
                assert!(!q.contains_key("client_secret"), "the secret never goes to the browser");
                {
                    let mut m = s2.lock().unwrap();
                    m.challenge = Some(q["code_challenge"].clone());
                    m.redirect = Some(q["redirect_uri"].clone());
                }
                let state = q["state"].clone();
                browser = Some(std::thread::spawn(move || {
                    let a = agent();
                    let status = |u: String| a.get(u).call().unwrap().status().as_u16();
                    assert_eq!(status(format!("http://127.0.0.1:{port}/favicon.ico")), 404);
                    assert_eq!(status(format!("http://127.0.0.1:{port}/callback?code=evil&state=forged")), 400, "another state is ignored");
                    let mut ok = a.get(format!("http://127.0.0.1:{port}/callback?code=good-code&state={}", fraud::pct(&state))).call().unwrap();
                    assert_eq!(ok.status().as_u16(), 200);
                    assert!(ok.body_mut().read_to_string().unwrap().contains("Connected to HMRC"));
                }));
                Ok(())
            },
            Duration::from_secs(20),
        )
        .unwrap();
        browser.unwrap().join().unwrap();
        (shared, mtd)
    }

    fn gov(h: &HeaderMap, name: &str) -> String {
        h.get(name).unwrap_or_else(|| panic!("{name} missing")).to_str().unwrap().to_owned()
    }

    #[test]
    fn oauth_code_exchange_stores_tokens_privately() {
        let (shared, mtd) = connected("oauth");
        assert_eq!(shared.lock().unwrap().grants, ["authorization_code"]);
        let s = mtd.settings(Some(VRN), None).unwrap();
        assert!(s.connected && s.has_secret);
        assert_eq!(s.client_id, "cid");
        let shown = serde_json::to_string(&s).unwrap();
        assert!(!shown.contains("at-1") && !shown.contains("rt-1") && !shown.contains("csecret"), "{shown}");
        assert!(s.reauthorise_by.is_some());
        let cfg = mtd.config().unwrap();
        assert!(!format!("{cfg:?}").contains("csecret"), "Debug is redacted");
        let t = mtd.tokens(&token_key(Environment::Sandbox, VRN)).unwrap().unwrap();
        assert!(!format!("{t:?}").contains("at-1"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for f in ["settings.json", "tokens.json", "device-id"] {
                let _ = mtd.device_id();
                let mode = fs::metadata(mtd.dir.join(f)).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o600, "{f}");
            }
            assert_eq!(fs::metadata(&mtd.dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
        // Another environment or application has no tokens.
        mtd.set_settings(Environment::Sandbox, "cid", "", 0).unwrap();
        assert!(mtd.settings(Some(VRN), None).unwrap().connected, "an empty secret keeps the old one and the tokens");
        assert!(mtd.config().unwrap().client_secret == "csecret");
        mtd.disconnect(VRN).unwrap();
        assert!(!mtd.settings(Some(VRN), None).unwrap().connected);
    }

    #[test]
    fn a_refused_sign_in_is_reported() {
        let shared: Shared = Arc::default();
        let url = serve(shared);
        let mtd = Mtd::with_endpoints(temp_dir("refused"), Endpoints { api: url.clone(), auth: url });
        let port = free_port();
        assert!(mtd.connect(VRN, |_| Ok(()), Duration::from_secs(1)).unwrap_err().contains("client ID"), "settings first");
        mtd.set_settings(Environment::Sandbox, "cid", "csecret", port).unwrap();
        let mut browser = None;
        let e = mtd
            .connect(
                VRN,
                |u| {
                    let state = parse_query(u.split_once('?').unwrap().1)["state"].clone();
                    browser = Some(std::thread::spawn(move || {
                        agent()
                            .get(format!("http://127.0.0.1:{port}/callback?error=access_denied&error_description=user+denied+the+authorization&state={}", fraud::pct(&state)))
                            .call()
                            .unwrap();
                    }));
                    Ok(())
                },
                Duration::from_secs(20),
            )
            .unwrap_err();
        browser.unwrap().join().unwrap();
        assert_eq!(e, "HMRC did not grant access: user denied the authorization");
        let e = mtd.connect(VRN, |_| Ok(()), Duration::from_millis(100)).unwrap_err();
        assert!(e.contains("took too long"), "{e}");
    }

    #[test]
    fn calls_carry_the_fraud_prevention_headers_and_refresh_tokens() {
        let (shared, mtd) = connected("headers");
        let list = mtd.obligations(VRN, "2017-01-01", "2017-12-31", None, &cx(None)).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!((list[0].period_key.as_str(), list[0].status.as_str(), list[0].received.as_deref()), ("18A1", "F", Some("2017-05-06")));
        let h = shared.lock().unwrap().last_headers.clone().unwrap();
        assert_eq!(gov(&h, "Authorization"), "Bearer at-1");
        assert_eq!(gov(&h, "Gov-Client-Connection-Method"), "DESKTOP_APP_DIRECT");
        assert_eq!(gov(&h, "Gov-Client-Device-ID"), mtd.device_id().unwrap());
        assert!(uuid::Uuid::parse_str(&gov(&h, "Gov-Client-Device-ID")).is_ok());
        assert_eq!(gov(&h, "Gov-Client-Screens"), "width=1920&height=1080&scaling-factor=2&colour-depth=24");
        assert_eq!(gov(&h, "Gov-Client-Window-Size"), "width=1256&height=803");
        let tz = gov(&h, "Gov-Client-Timezone");
        assert!(tz.len() == 9 && tz.starts_with("UTC") && (tz.as_bytes()[3] == b'+' || tz.as_bytes()[3] == b'-') && tz.as_bytes()[6] == b':', "{tz}");
        let ts = gov(&h, "Gov-Client-Local-IPs-Timestamp");
        let tb = ts.as_bytes();
        assert!(tb.len() == 24 && tb[10] == b'T' && tb[19] == b'.' && tb[23] == b'Z', "{ts}");
        let ids = gov(&h, "Gov-Client-User-IDs");
        assert!(ids.starts_with("os=") && ids.ends_with("&khataos=Alice%20Smith"), "{ids}");
        assert_eq!(gov(&h, "Gov-Vendor-Version"), format!("KhataOS={}", env!("CARGO_PKG_VERSION")));
        assert_eq!(gov(&h, "Gov-Vendor-Product-Name"), "KhataOS");
        assert_eq!(gov(&h, "Gov-Vendor-License-IDs"), "", "free software: no licence to hash");
        assert_eq!(gov(&h, "Gov-Client-Multi-Factor"), "", "KhataOS signs in with a password only");
        assert!(gov(&h, "Gov-Client-User-Agent").starts_with("os-family="));
        for name in ["Gov-Client-Local-IPs", "Gov-Client-MAC-Addresses"] {
            let v = gov(&h, name);
            assert!(v.is_ascii() && !v.contains(' ') && !v.contains(':'), "{name}: {v}");
        }
        assert!(h.get("Gov-Test-Scenario").is_none());

        // An expired access token is refreshed first; the refresh token is single-use.
        {
            let _g = tokens_lock();
            let key = token_key(Environment::Sandbox, VRN);
            let mut t = mtd.tokens(&key).unwrap().unwrap();
            t.expires_at = Utc::now() - chrono::Duration::minutes(1);
            mtd.save_tokens(&key, Some(t)).unwrap();
        }
        mtd.obligations(VRN, "", "", Some("O"), &cx(None)).unwrap();
        assert_eq!(gov(&shared.lock().unwrap().last_headers.clone().unwrap(), "Authorization"), "Bearer at-2");
        // HMRC says INVALID_CREDENTIALS: refresh once and retry.
        shared.lock().unwrap().access.retain(|a| a != "at-2");
        let open = mtd.obligations(VRN, "", "", Some("O"), &cx(Some("QUARTERLY_ONE_MET"))).unwrap();
        assert_eq!(open.iter().map(|o| o.period_key.as_str()).collect::<Vec<_>>(), ["18A2", "#001"]);
        let h = shared.lock().unwrap().last_headers.clone().unwrap();
        assert_eq!(gov(&h, "Authorization"), "Bearer at-3");
        assert_eq!(gov(&h, "Gov-Test-Scenario"), "QUARTERLY_ONE_MET", "sandbox only");
        assert_eq!(shared.lock().unwrap().grants, ["authorization_code", "refresh_token", "refresh_token"]);
        // No data is an empty list.
        assert!(mtd.obligations(VRN, "", "", Some("O"), &cx(Some("NOT_FOUND"))).unwrap().is_empty());
        assert!(mtd.obligations(VRN, "", "", None, &cx(None)).unwrap_err().contains("date range"));
        assert!(mtd.obligations(VRN, "", "", Some("X"), &cx(None)).is_err());

        // Liabilities and payments; numbers keep HMRC's digits.
        let l = mtd.liabilities(VRN, "2017-04-05", "2017-12-21", &cx(None)).unwrap();
        assert_eq!((l[0].kind.as_str(), l[0].original_amount.as_str(), l[0].outstanding_amount.as_deref(), l[0].to.as_deref()), ("VAT Return Debit Charge", "463872", Some("463872.5"), Some("2017-06-30")));
        let p = mtd.payments(VRN, "2017-01-01", "2017-12-31", &cx(None)).unwrap();
        assert_eq!((p[0].amount.as_str(), p[0].received.as_deref()), ("1534.65", Some("2017-02-11")));

        // A revoked grant: the refresh fails and the tokens are dropped.
        {
            let mut m = shared.lock().unwrap();
            m.access.clear();
            m.refresh.clear();
        }
        let e = mtd.obligations(VRN, "", "", Some("O"), &cx(None)).unwrap_err();
        assert!(e.contains("Connect to HMRC again"), "{e}");
        assert!(!mtd.settings(Some(VRN), None).unwrap().connected);
        assert!(mtd.obligations(VRN, "", "", Some("O"), &cx(None)).unwrap_err().contains("connect to HMRC first"));
    }

    #[test]
    fn bad_window_sizes_are_refused_before_sending() {
        let (shared, mtd) = connected("window");
        let mut c = cx(None);
        c.info.window.width = 0;
        assert!(mtd.obligations(VRN, "", "", Some("O"), &c).unwrap_err().contains("window"));
        c.info = info();
        c.info.screens.clear();
        assert!(mtd.obligations(VRN, "", "", Some("O"), &c).is_err());
        assert!(shared.lock().unwrap().last_headers.is_none(), "nothing reached HMRC");
    }

    fn uk_company(name: &str) -> (TempFile, Mutex<Slot>) {
        let f = TempFile::new(name);
        let mut slot = None;
        create_company(&mut slot, &f.0, "Demo Ltd").unwrap();
        set_country(&mut slot, Some(&f.0), "GB").unwrap();
        set_company_tax(&mut slot, "GB 562 2359 45", "ENG").unwrap();
        set_tax_rate(&mut slot, "STANDARD", "2011-01-04", "20").unwrap();
        let cash = ledger(&mut slot, "Cash", "Cash-in-Hand", "");
        let sales = ledger(&mut slot, "Sales", "Sales Accounts", "");
        let output = ledger(&mut slot, "Output VAT", "Duties & Taxes", "");
        set_ledger_tax(&mut slot, &output, "VAT", TaxSide::Output).unwrap();
        post_voucher(
            &mut slot,
            voucher("Sales", "2017-05-10", vec![entry(&cash, DrCr::Dr, "120"), entry(&sales, DrCr::Cr, "100"), entry(&output, DrCr::Cr, "20")]),
        )
        .unwrap();
        (f, Mutex::new(slot))
    }

    #[test]
    fn submit_needs_the_declaration_and_records_the_receipt() {
        let (shared, mtd) = connected("submit");
        let (_f, slot) = uk_company("mtd-submit");
        assert_eq!(company_vrn(&slot.lock().unwrap()).unwrap(), VRN);

        let e = submit_cmd(&mtd, &slot, "18A2", false, info(), None).unwrap_err();
        assert!(e.contains("Tick the declaration"), "{e}");
        assert!(shared.lock().unwrap().last_headers.is_none(), "nothing is sent without the declaration");
        let e = submit_cmd(&mtd, &slot, "18A2", true, info(), Some("TAX_PERIOD_NOT_ENDED".into())).unwrap_err();
        assert!(e.contains("hasn't ended yet") && e.contains("HMRC reference c75f40a6"), "{e}");
        let e = submit_cmd(&mtd, &slot, "18A1", true, info(), None).unwrap_err();
        assert!(e.contains("no open VAT obligation"), "a fulfilled period can't be submitted: {e}");

        let r = submit_cmd(&mtd, &slot, "18A2", true, info(), None).unwrap();
        assert_eq!((r.from.as_str(), r.to.as_str()), ("2017-04-01", "2017-06-30"), "HMRC's dates");
        assert_eq!(r.form_bundle_number, "256660290587");
        assert_eq!(r.charge_ref_number.as_deref(), Some("aCxFaNx0FZsCvyWF"));
        assert_eq!(r.receipt_id.as_deref(), Some("2dd537bc-4244-4ebf-bac9-96321be13cdc"));
        assert_eq!(r.receipt_timestamp.as_deref(), Some("2018-02-14T09:32:15Z"));
        let body = shared.lock().unwrap().submitted["18A2"].clone();
        for want in ["\"periodKey\": \"18A2\"", "\"vatDueSales\": 20.00", "\"totalVatDue\": 20.00", "\"netVatDue\": 20.00", "\"totalValueSalesExVAT\": 100", "\"finalised\": true"] {
            assert!(body.contains(want), "{want} in {body}");
        }
        assert_eq!(r.return_json, body);

        // The receipt is in the company file and on the obligation.
        {
            let g = slot.lock().unwrap();
            let list = receipts(store_of(&g).unwrap()).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].period_key, "18A2");
        }
        let obs = obligations_cmd(&mtd, &slot, "", "", Some("O"), info(), None).unwrap();
        assert_eq!(obs[0].receipt.as_ref().map(|r| r.form_bundle_number.as_str()), Some("256660290587"));
        assert!(obs[1].receipt.is_none());
        assert_eq!(settings_cmd(&mtd, &slot).unwrap().receipts.len(), 1);

        let e = submit_cmd(&mtd, &slot, "18A2", true, info(), None).unwrap_err();
        assert!(e.contains("already has a VAT return"), "{e}");

        // View it back; a '#' in a period key is sent as %23.
        let v = view_return_cmd(&mtd, &slot, "18A2", info()).unwrap();
        assert_eq!((v.vat_due_sales.as_str(), v.total_value_sales_ex_vat.as_str(), v.net_vat_due.as_str()), ("20.0", "100", "20.0"));
        submit_cmd(&mtd, &slot, "#001", true, info(), None).unwrap();
        assert_eq!(view_return_cmd(&mtd, &slot, "#001", info()).unwrap().period_key, "#001");
        assert!(view_return_cmd(&mtd, &slot, "18A1", info()).unwrap_err().contains("HMRC has nothing"));
    }

    #[test]
    fn the_header_validator_uses_an_application_token() {
        let (shared, mtd) = connected("validate");
        let r = mtd.test_headers(&info(), None).unwrap();
        assert_eq!(r.code, "POTENTIALLY_INVALID_HEADERS");
        assert_eq!(r.warnings[0].headers, ["gov-vendor-license-ids"]);
        assert_eq!(r.sent.len(), 14);
        assert!(r.sent.iter().all(|(k, _)| k.starts_with("Gov-")));
        assert_eq!(shared.lock().unwrap().grants.last().map(String::as_str), Some("client_credentials"));
        let h = shared.lock().unwrap().last_headers.clone().unwrap();
        assert_eq!(gov(&h, "Gov-Client-User-IDs").split('&').count(), 1, "no KhataOS user signed in: os only");
        mtd.set_settings(Environment::Production, "cid", "", 0).unwrap();
        assert!(mtd.test_headers(&info(), None).unwrap_err().contains("sandbox"));
        assert!(!mtd.settings(Some(VRN), None).unwrap().connected, "a new environment forgets the tokens");
    }

    #[test]
    fn errors_read_plainly() {
        assert!(plain("DUPLICATE_SUBMISSION", "").contains("already has a VAT return"));
        assert!(plain("TAX_PERIOD_NOT_ENDED", "").contains("hasn't ended"));
        assert!(plain("VRN_INVALID", "").contains("VAT registration number"));
        assert!(plain("INVALID_VRN", "").contains("VAT registration number"));
        assert_eq!(plain("SOMETHING_NEW", "a message"), "HMRC said SOMETHING_NEW: a message");
        let r = Reply {
            status: 400,
            body: r#"{"code":"INVALID_REQUEST","message":"Invalid request","errors":[{"code":"VAT_TOTAL_VALUE","message":"x"},{"code":"VAT_NET_VALUE","message":"y"}]}"#.into(),
            receipt_id: None,
            receipt_timestamp: None,
            correlation_id: Some("abc".into()),
        };
        assert_eq!(api_error(&r), "HMRC says Box 3 is not Box 1 + Box 2; HMRC says Box 5 is not the difference between Boxes 3 and 4 (HMRC reference abc)");
        let r = Reply { status: 502, body: "<html>".into(), receipt_id: None, receipt_timestamp: None, correlation_id: None };
        assert_eq!(api_error(&r), "HMRC answered with HTTP 502");
    }

    #[test]
    fn encoding_helpers() {
        assert_eq!(pct_decode("user+denied%20the%2Cx%2"), "user denied the,x%2");
        assert_eq!(pct_decode("%zz%41"), "%zzA");
        let q = parse_query("code=6589c5d9&state=30de877c-ee2f&empty=&flag");
        assert_eq!((q["code"].as_str(), q["empty"].as_str(), q["flag"].as_str()), ("6589c5d9", "", ""));
        assert_eq!(form(&[("redirect_uri", "http://localhost:1/callback"), ("scope", "read:vat write:vat")]), "redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcallback&scope=read%3Avat%20write%3Avat");
        // Python's hashlib gives this S256 challenge for this verifier.
        assert_eq!(challenge("dBjftJeZ4CVP-mA92xMyc8VPwxHFz2T5fgdeSMHhrmw"), "hfne4p1-WRkAXLhOM3LFTPD9vdrcQbVVku9EweVtKak");
        assert_eq!(random_b64(32).unwrap().len(), 43);
    }

    #[test]
    fn the_device_id_is_kept_and_remade_when_deleted() {
        let mtd = Mtd::new(temp_dir("device"));
        let a = mtd.device_id().unwrap();
        assert_eq!(mtd.device_id().unwrap(), a);
        fs::remove_file(mtd.dir.join("device-id")).unwrap();
        assert_ne!(mtd.device_id().unwrap(), a);
    }
}
