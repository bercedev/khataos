//! Sharing books the TallyPrime way: one office PC "acts as" the server and
//! the others connect to it. No Docker, no separate install.
//!
//! - **Device settings** (`sharing.json` in the app data folder): what this
//!   computer acts as (None / Client / Server / Both), the port, and the
//!   relay address offered for internet sharing. They are per computer, not
//!   per company, because one embedded server hosts every company shared
//!   from this PC and must run before any company is opened.
//! - **Server mode** embeds `khata-server` as a library on a background
//!   thread, listening on `0.0.0.0:<port>` (default 9090, so it never clashes
//!   with TallyPrime's 9000). Its data lives in `<app data>/server`, with a
//!   random admin key kept in `admin.key` there.
//! - **Hosting a company** creates it on the embedded server, pushes the local
//!   trail and links this file to it over loopback, so the server PC's own
//!   entries are synced within seconds.
//! - **Per company file** (the device-local `settings` table, never synced):
//!   whether this PC hosts it, the access role it was shared with, "Connect
//!   Company on loading" and the sync interval. The company's shared records
//!   (notes, invoice design, tax settings) sync with the books on every
//!   path here (`khata_sync::sync_all`, `docs/sync-settings.md`).
//! - **Internet** (no office server): the company syncs through a KhataOS
//!   relay with end-to-end encryption (`khata_sync::sync_sealed`). The relay
//!   stores only ciphertext; the company key stays in this file's settings
//!   and in the invite codes. No relay address is built in unless the build
//!   sets `KHATA_DEFAULT_RELAY`; a company can move to another relay later
//!   ([`move_relay`]).
//! - **Invite codes** carry everything a colleague needs in one string:
//!   `khata1:` + base64url(JSON {url, company_id, token, company_name, role,
//!   key?}); `key` is present for relay companies.
//!
//! Security rules stay those of `khata-sync`: https, or plain http only on
//! this computer or a private office network.

use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use khata_store::Store;
use khata_sync::{Remote, SyncError, SyncOutcome};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::api::{self, ApiResult, Slot, err};
use crate::sync::{self, SyncResult};

pub const DEFAULT_PORT: u16 = 9090;
pub const DEFAULT_INTERVAL_MINUTES: u32 = 2;
const CONFIG_FILE: &str = "sharing.json";
const SERVER_DIR: &str = "server";
const ADMIN_KEY_FILE: &str = "admin.key";
const INVITE_PREFIX: &str = "khata1:";
const MAX_INVITE_LEN: usize = 4096;

/// Company-file settings (device-local, never synced).
pub const HOSTED: &str = "share.hosted";
pub const ROLE: &str = "share.role";
pub const CONNECT_ON_LOAD: &str = "share.connect_on_load";
pub const INTERVAL: &str = "share.interval_minutes";
/// "lan" (office server) or "relay" (internet, end-to-end encrypted).
pub const VIA: &str = "share.via";
/// The relay this company uses, when it differs from the device's.
pub const RELAY_URL: &str = "share.relay_url";
/// The relay company's encryption key (hex). A secret: anyone with it and
/// a token can read the books.
pub const KEY: &str = "share.key";
/// The relay offered when the user hasn't set one: chosen when the app is
/// built (`KHATA_DEFAULT_RELAY=https://… cargo build`), empty otherwise.
/// Users set their own in Exchange → Configure, or per company.
pub const BUILD_DEFAULT_RELAY: &str = match option_env!("KHATA_DEFAULT_RELAY") {
    Some(url) => url,
    None => "",
};

/// Said when internet sharing is chosen but no relay is set anywhere.
pub const NO_RELAY: &str = "No relay address is set. Enter the address of a relay someone runs for you \
     (Exchange → Configure → Relay address), host your own (see docs/deploy-relay.md), \
     or send accountant files instead (Alt+M)";

pub const READ_ONLY: &str = "This company was shared with you as a Viewer, so it is read-only on this computer: \
     every report is available, but nothing can be entered or changed. Ask the owner for an Accountant invite to make entries.";

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ------------------------------------------------------ device settings

/// "KhataOS acts as", as in TallyPrime's Client/Server configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActsAs {
    #[default]
    None,
    Client,
    Server,
    Both,
}

impl ActsAs {
    pub fn serves(self) -> bool {
        matches!(self, ActsAs::Server | ActsAs::Both)
    }

    pub fn connects(self) -> bool {
        matches!(self, ActsAs::Client | ActsAs::Both)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub acts_as: ActsAs,
    pub port: u16,
    /// Relay for internet sharing; empty = the build's default, if any.
    #[serde(default)]
    pub relay_url: String,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            acts_as: ActsAs::None,
            port: DEFAULT_PORT,
            relay_url: String::new(),
        }
    }
}

impl DeviceConfig {
    /// The relay offered on this computer, or "" when there is none.
    pub fn relay_address(&self) -> String {
        if self.relay_url.is_empty() {
            BUILD_DEFAULT_RELAY.trim().trim_end_matches('/').to_owned()
        } else {
            self.relay_url.clone()
        }
    }

    /// Phones never host the office server: Server falls back to None and
    /// Both to Client. Sync, the relay and .khatapkg files still work.
    #[cfg(mobile)]
    pub fn on_phone(mut self) -> Self {
        self.acts_as = match self.acts_as {
            ActsAs::Server => ActsAs::None,
            ActsAs::Both => ActsAs::Client,
            other => other,
        };
        self
    }
}

pub fn load_config(app_dir: &Path) -> DeviceConfig {
    std::fs::read_to_string(app_dir.join(CONFIG_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn check_port(port: u16) -> ApiResult<()> {
    if port < 1024 {
        return Err(format!(
            "port {port} needs administrator rights; use a port from 1024 to 65535 (9090 is the default)"
        ));
    }
    Ok(())
}

pub fn save_config(app_dir: &Path, cfg: &DeviceConfig) -> ApiResult<()> {
    check_port(cfg.port)?;
    normalize_relay(&cfg.relay_url)?;
    std::fs::create_dir_all(app_dir).map_err(err)?;
    let tmp = app_dir.join(format!("{CONFIG_FILE}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec_pretty(cfg).map_err(err)?).map_err(err)?;
    std::fs::rename(&tmp, app_dir.join(CONFIG_FILE)).map_err(err)
}

// ------------------------------------------------------ embedded server

/// The server's admin key: created once, random, readable only by this user.
fn admin_key(data_dir: &Path) -> ApiResult<String> {
    let path = data_dir.join(ADMIN_KEY_FILE);
    if let Ok(k) = std::fs::read_to_string(&path) {
        let k = k.trim();
        if k.len() >= 32 {
            return Ok(k.to_owned());
        }
    }
    let key = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    opts.open(&path)
        .and_then(|mut f| f.write_all(key.as_bytes()))
        .map_err(|e| format!("cannot save the server key: {e}"))?;
    Ok(key)
}

fn bind_error(port: u16, e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::AddrInUse => format!(
            "port {port} is already in use on this computer (perhaps by another KhataOS or TallyPrime). \
             Choose another port in Exchange (Alt+Z) → Configure"
        ),
        std::io::ErrorKind::PermissionDenied => {
            format!(
                "this computer does not allow KhataOS to use port {port}; choose a port above 1024"
            )
        }
        _ => format!("cannot listen on port {port}: {e}"),
    }
}

/// `khata-server`, running inside the app on its own tokio runtime.
pub struct EmbeddedServer {
    port: u16,
    data_dir: PathBuf,
    admin_key: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl EmbeddedServer {
    /// Serve on `bind` (`0.0.0.0:<port>` for the office network; port 0
    /// picks a free one). Fails at once if the port is taken.
    pub fn start(data_dir: &Path, bind: SocketAddr) -> ApiResult<Self> {
        Self::start_with(data_dir, bind, false)
    }

    /// [`start`](Self::start), optionally accepting relay sign-ups without
    /// the admin key (a relay for testing on this computer).
    pub fn start_with(data_dir: &Path, bind: SocketAddr, open_relay: bool) -> ApiResult<Self> {
        std::fs::create_dir_all(data_dir)
            .map_err(|e| format!("cannot create {}: {e}", data_dir.display()))?;
        let admin_key = admin_key(data_dir)?;
        let state = khata_server::AppState::open(khata_server::Config {
            data_dir: data_dir.to_owned(),
            admin_key: admin_key.clone(),
        })
        .map_err(|e| format!("cannot start the server: {e}"))?;
        state.set_open_relay(open_relay);
        let listener = TcpListener::bind(bind).map_err(|e| bind_error(bind.port(), &e))?;
        listener.set_nonblocking(true).map_err(err)?;
        let port = listener.local_addr().map_err(err)?.port();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("khata-server")
            .enable_all()
            .build()
            .map_err(|e| format!("cannot start the server: {e}"))?;
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("khata-server".into())
            .spawn(move || {
                rt.block_on(async move {
                    use std::future::IntoFuture;
                    let listener = match tokio::net::TcpListener::from_std(listener) {
                        Ok(l) => l,
                        Err(e) => return eprintln!("khata-server: {e}"),
                    };
                    let serve = axum::serve(listener, khata_server::router(state)).into_future();
                    tokio::select! {
                        r = serve => if let Err(e) = r { eprintln!("khata-server stopped: {e}") },
                        _ = rx => {}
                    }
                });
                // Open connections are dropped; the port is free again.
                rt.shutdown_timeout(Duration::from_secs(2));
            })
            .map_err(|e| format!("cannot start the server: {e}"))?;
        Ok(Self {
            port,
            data_dir: data_dir.to_owned(),
            admin_key,
            stop: Some(tx),
            thread: Some(thread),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// How this computer reaches its own server.
    pub fn local_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Whether the company is on this server.
    pub fn hosts(&self, company_id: &str) -> bool {
        uuid::Uuid::parse_str(company_id)
            .is_ok_and(|id| self.data_dir.join(format!("{id}.khata")).is_file())
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for EmbeddedServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// This computer's office-network addresses (private IPv4), the main one
/// first. These are what colleagues type, or what invite codes carry.
pub fn lan_addresses() -> Vec<String> {
    let mut ips: Vec<IpAddr> = Vec::new();
    #[cfg(not(unix))]
    if let Ok(ip) = local_ip_address::local_ip() {
        ips.push(ip);
    }
    let mut list = interface_addrs();
    // Wi-Fi or Ethernet before a phone's mobile data.
    list.sort_by_key(|(name, _)| name.starts_with("rmnet") || name.starts_with("pdp_ip"));
    ips.extend(list.into_iter().map(|(_, ip)| ip));
    let mut out: Vec<String> = Vec::new();
    for ip in ips {
        if let IpAddr::V4(v4) = ip
            && v4.is_private()
            && !out.contains(&v4.to_string())
        {
            out.push(v4.to_string());
        }
    }
    out
}

/// Every interface address as (interface name, address), in `getifaddrs`
/// order. Not `local_ip_address` on Unix: on macOS, iOS and Android it never
/// calls `freeifaddrs`, leaking the whole list on every call.
#[cfg(unix)]
pub(crate) fn interface_addrs() -> Vec<(String, IpAddr)> {
    use std::net::{Ipv4Addr, Ipv6Addr};
    let mut out = Vec::new();
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: on success `head` is a list we own until `freeifaddrs` below.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return out;
    }
    let mut at = head;
    while !at.is_null() {
        // SAFETY: `at` is a node of that list; the addresses are read as the
        // type their family says.
        unsafe {
            let ifa = &*at;
            let addr = ifa.ifa_addr;
            let ip = if addr.is_null() {
                None
            } else {
                match i32::from((*addr).sa_family) {
                    libc::AF_INET => {
                        let sa = &*addr.cast::<libc::sockaddr_in>();
                        Some(IpAddr::V4(Ipv4Addr::from(u32::from_be(sa.sin_addr.s_addr))))
                    }
                    libc::AF_INET6 => {
                        let sa = &*addr.cast::<libc::sockaddr_in6>();
                        Some(IpAddr::V6(Ipv6Addr::from(sa.sin6_addr.s6_addr)))
                    }
                    _ => None,
                }
            };
            if let Some(ip) = ip {
                let name = if ifa.ifa_name.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned()
                };
                out.push((name, ip));
            }
            at = ifa.ifa_next;
        }
    }
    // SAFETY: `head` came from the successful `getifaddrs` above.
    unsafe { libc::freeifaddrs(head) };
    out
}

#[cfg(not(unix))]
pub(crate) fn interface_addrs() -> Vec<(String, IpAddr)> {
    local_ip_address::list_afinet_netifas().unwrap_or_default()
}

// ------------------------------------------------------------- HTTP

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into()
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

fn read<T: DeserializeOwned>(mut resp: ureq::http::Response<ureq::Body>) -> ApiResult<T> {
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        return resp.body_mut().read_json().map_err(err);
    }
    let message = resp
        .body_mut()
        .read_json::<ErrorBody>()
        .map(|b| b.error)
        .unwrap_or_default();
    Err(match status {
        401 => "the server refused this computer's access (revoked or expired); ask the owner for a new invite".into(),
        403 => "only the company's owner can do that; this computer has narrower access".into(),
        404 => "the server doesn't have this company".into(),
        _ => format!("the server said {status}: {message}"),
    })
}

fn net(e: ureq::Error) -> String {
    format!(
        "couldn't reach the server ({e}). Is the server PC on, KhataOS open there in Server mode, and on the same network?"
    )
}

fn base(remote: &Remote) -> String {
    format!(
        "{}/v1/companies/{}",
        remote.url.trim_end_matches('/'),
        remote.company_id
    )
}

fn checked(remote: &Remote) -> ApiResult<()> {
    khata_sync::validate_remote(remote).map_err(|e| sync::explain(&e))
}

// ------------------------------------------------- per-company settings

fn get(store: &Store, key: &str) -> ApiResult<Option<String>> {
    store
        .setting(key)
        .map_err(err)
        .map(|v| v.filter(|s| !s.is_empty()))
}

fn set(store: &Store, key: &str, value: &str) -> ApiResult<()> {
    store.set_setting(key, Some(value)).map_err(err)
}

/// Refuse changes to a company shared as Viewer. Called by `api::open_mut`.
pub fn check_writable(store: &Store) -> ApiResult<()> {
    if store.setting(ROLE).ok().flatten().as_deref() == Some("viewer") {
        return Err(READ_ONLY.into());
    }
    Ok(())
}

/// The end-to-end key of a relay company, if this file syncs through one.
pub fn company_key(store: &Store) -> ApiResult<Option<khata_seal::CompanyKey>> {
    match get(store, KEY)? {
        Some(hex) => khata_seal::CompanyKey::from_hex(&hex)
            .map(Some)
            .map_err(|_| {
                "this company's encryption key is damaged; connect again with an invite code".into()
            }),
        None => Ok(None),
    }
}

/// Sync the books and the shared records with the server or relay this
/// file is linked to: sealed through a relay when the company has a key,
/// plain with an office server.
pub fn sync_any(
    store: &mut Store,
    book: &mut khata_core::Book,
    remote: &Remote,
) -> Result<(SyncOutcome, khata_sync::RecordsOutcome), SyncError> {
    let key = company_key(store).map_err(SyncError::File)?;
    khata_sync::sync_all(store, book, remote, key.as_ref())
}

fn store_of(slot: &Slot) -> ApiResult<&Store> {
    slot.as_ref()
        .map(|(s, _)| s)
        .ok_or_else(|| "no company is open".to_owned())
}

/// "Connect Company on loading" and the interval, plus how the file is shared.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OnlineAccess {
    pub connect_on_load: bool,
    pub interval_minutes: u32,
    /// "owner", "accountant", "viewer", or "" when not shared.
    pub role: String,
    /// This computer's embedded server hosts the company.
    pub hosted: bool,
    /// Sync is set up (by hosting, an invite or Sync settings).
    pub linked: bool,
    pub url: String,
    /// "lan" or "relay": how Connect shares this company.
    pub via: String,
    pub relay_url: String,
    /// Synced end-to-end encrypted (through a relay).
    pub encrypted: bool,
}

pub fn online_access(slot: &Slot) -> ApiResult<OnlineAccess> {
    let store = store_of(slot)?;
    let linked = sync::remote(store).is_ok();
    Ok(OnlineAccess {
        connect_on_load: get(store, CONNECT_ON_LOAD)?.as_deref() == Some("yes"),
        interval_minutes: get(store, INTERVAL)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_INTERVAL_MINUTES),
        role: get(store, ROLE)?.unwrap_or_else(|| {
            if linked {
                "owner".into()
            } else {
                String::new()
            }
        }),
        hosted: get(store, HOSTED)?.as_deref() == Some("1"),
        linked,
        url: get(store, sync::URL)?.unwrap_or_default(),
        via: get(store, VIA)?.unwrap_or_else(|| "lan".into()),
        // Empty: the device's relay applies (see `with_device_relay`).
        relay_url: get(store, RELAY_URL)?.unwrap_or_default(),
        encrypted: get(store, KEY)?.is_some(),
    })
}

pub fn set_online_access(
    slot: &Slot,
    connect_on_load: bool,
    interval_minutes: u32,
    via: &str,
    relay_url: &str,
) -> ApiResult<OnlineAccess> {
    if !(1..=1440).contains(&interval_minutes) {
        return Err("sync every: enter 1 to 1440 minutes".into());
    }
    if !["lan", "relay"].contains(&via) {
        return Err("sync via: choose Office server or KhataOS relay".into());
    }
    let relay_url = normalize_relay(relay_url)?;
    let store = store_of(slot)?;
    set(
        store,
        CONNECT_ON_LOAD,
        if connect_on_load { "yes" } else { "no" },
    )?;
    set(store, INTERVAL, &interval_minutes.to_string())?;
    set(store, VIA, via)?;
    store
        .set_setting(
            RELAY_URL,
            Some(&relay_url)
                .filter(|u| !u.is_empty())
                .map(String::as_str),
        )
        .map_err(err)?;
    online_access(slot)
}

/// Show the device's relay for a company that hasn't chosen its own.
pub fn with_device_relay(mut access: OnlineAccess, cfg: &DeviceConfig) -> OnlineAccess {
    if access.relay_url.is_empty() {
        access.relay_url = cfg.relay_address();
    }
    access
}

/// A relay is reached over the internet, so it must be https (plain http
/// only for a relay on this computer or the office network, for testing).
/// Empty stays empty: no relay chosen.
fn normalize_relay(url: &str) -> ApiResult<String> {
    let url = url.trim().trim_end_matches('/');
    let url = if url.is_empty() {
        return Ok(String::new());
    } else if url.contains("://") {
        url.to_owned()
    } else {
        format!("https://{url}")
    };
    let probe = Remote {
        url: url.clone(),
        company_id: "00000000-0000-0000-0000-000000000000".into(),
        token: String::new(),
    };
    khata_sync::validate_remote(&probe)
        .map_err(|_| "the relay URL must start with https://".to_owned())?;
    Ok(url)
}

/// Share the open company over the internet through an end-to-end
/// encrypted relay: create it there once (keeping the new key in this
/// file), then push. Later calls just sync.
pub fn connect_relay(
    slot: &mut Slot,
    relay_url: &str,
    signup_key: Option<&str>,
) -> ApiResult<HostReport> {
    let store = store_of(slot)?;
    check_writable(store)?;
    let relay_url = normalize_relay(relay_url)?;
    if relay_url.is_empty() {
        return Err(NO_RELAY.into());
    }
    let name = store.company_name().map_err(err)?;
    let created = match (get(store, sync::URL)?, get(store, KEY)?) {
        (Some(url), Some(_)) if url == relay_url => false,
        (Some(url), _) => {
            return Err(format!(
                "{name} already syncs with {url}; a company syncs with one server or relay at a time"
            ));
        }
        (None, _) => {
            let signup = signup_key.map(str::trim).filter(|k| !k.is_empty());
            let (remote, key) =
                khata_sync::create_relay_company(&relay_url, signup).map_err(|e| match e {
                    SyncError::Server {
                        status: 401 | 403, ..
                    } => "this relay doesn't accept new companies without a sign-up key".to_owned(),
                    SyncError::Server {
                        status: 413 | 429 | 507,
                        message,
                    } => format!("the relay at {relay_url} said: {message}"),
                    SyncError::Network(m) => {
                        format!("couldn't reach the relay at {relay_url} ({m})")
                    }
                    other => sync::explain(&other),
                })?;
            store
                .set_sync_state(&remote.url, &remote.company_id, 0)
                .map_err(err)?;
            set(store, sync::URL, &remote.url)?;
            set(store, sync::COMPANY, &remote.company_id)?;
            set(store, sync::TOKEN, &remote.token)?;
            set(store, KEY, &key.to_hex())?;
            set(store, ROLE, "owner")?;
            set(store, VIA, "relay")?;
            set(store, RELAY_URL, &relay_url)?;
            store.set_setting(HOSTED, None).map_err(err)?;
            if get(store, CONNECT_ON_LOAD)?.is_none() {
                set(store, CONNECT_ON_LOAD, "yes")?;
            }
            true
        }
    };
    let sync = sync::sync_now(slot)?;
    Ok(HostReport {
        company_id: get(store_of(slot)?, sync::COMPANY)?.unwrap_or_default(),
        created,
        sync,
    })
}

/// What "Move to another relay" did.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MoveReport {
    pub from: String,
    pub to: String,
    /// Entries now on the new relay (the whole trail).
    pub entries: u64,
    /// Colleagues' access revoked on the old relay; `None` if it couldn't be
    /// reached after the copy.
    pub revoked: Option<usize>,
}

/// Move the open relay company to the relay at `to_url` (with `signup_key`
/// if that relay needs one): the whole trail is copied there, sealed with a
/// new key, and this file switches to it. Colleagues need new invite codes;
/// their old ones are revoked on the old relay. See `khata_sync::move_relay`.
pub fn move_relay(
    slot: &mut Slot,
    to_url: &str,
    signup_key: Option<&str>,
) -> ApiResult<MoveReport> {
    let to_url = normalize_relay(to_url)?;
    if to_url.is_empty() {
        return Err("enter the new relay's address, e.g. https://relay.example.in".into());
    }
    let (store, book) = sync::slot_mut(slot)?;
    check_writable(store)?;
    if matches!(get(store, ROLE)?.as_deref(), Some("viewer" | "accountant")) {
        return Err(
            "only the company's owner can move it to another relay; this computer was invited"
                .into(),
        );
    }
    let key = company_key(store)?.ok_or(
        "only a company shared through a relay can be moved to another relay; this one isn't (Company → Configure Online Access)",
    )?;
    let from = sync::remote(store)?;
    let signup = signup_key.map(str::trim).filter(|k| !k.is_empty());
    let moved = khata_sync::move_relay(store, book, &from, &key, &to_url, signup).map_err(|e| match e {
        SyncError::NotSynced { .. } => format!(
            "{e}. Press Ctrl+Alt+S to sync (resolve any conflict), ask colleagues to sync too, then move"
        ),
        SyncError::SameRelay(_) => "the company already uses that relay; enter the new relay's address".into(),
        SyncError::Server {
            status: 401 | 403, ..
        } => format!("the relay at {to_url} doesn't accept new companies without a sign-up key; ask its operator for one"),
        SyncError::Server {
            status: 413 | 429 | 507,
            message,
        } => format!("the relay said: {message}"),
        SyncError::Network(m) => format!("couldn't reach a relay ({m}); both the old and the new relay must be reachable"),
        other => sync::explain(&other),
    })?;
    set(store, sync::URL, &moved.remote.url)?;
    set(store, sync::COMPANY, &moved.remote.company_id)?;
    set(store, sync::TOKEN, &moved.remote.token)?;
    set(store, KEY, &moved.key.to_hex())?;
    set(store, ROLE, "owner")?;
    set(store, VIA, "relay")?;
    set(store, RELAY_URL, &to_url)?;
    sync::stamp(store, &format!("Moved to {to_url}"));
    Ok(MoveReport {
        from: from.url,
        to: to_url,
        entries: moved.entries,
        revoked: moved.revoked,
    })
}

// ------------------------------------------------------------- hosting

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HostReport {
    pub company_id: String,
    /// The company was created on this computer's server just now.
    pub created: bool,
    pub sync: SyncResult,
}

#[derive(Deserialize)]
struct CompanyCreated {
    company_id: String,
    owner_token: String,
}

/// Host the open company on this computer's embedded server: create it there
/// if needed, push the trail and link this file to it. Safe to repeat; after
/// a port change it moves the link to the new port.
pub fn host_company(slot: &mut Slot, server: &EmbeddedServer) -> ApiResult<HostReport> {
    let store = store_of(slot)?;
    check_writable(store)?;
    let name = store.company_name().map_err(err)?;
    let local = server.local_url();
    let hosted = get(store, HOSTED)?.as_deref() == Some("1");
    let (url, company) = (get(store, sync::URL)?, get(store, sync::COMPANY)?);
    let created = match (hosted, url, company) {
        (true, Some(old), Some(id)) if server.hosts(&id) => {
            if old != local {
                if let Some((linked, len)) = store.sync_state(&old).map_err(err)? {
                    store.set_sync_state(&local, &linked, len).map_err(err)?;
                }
                set(store, sync::URL, &local)?;
            }
            None
        }
        (false, Some(other), Some(_)) => {
            return Err(format!(
                "{name} already syncs with {other}. Server mode shares companies kept on this computer; \
                 this one is kept on that server, so keep connecting to it as a Client"
            ));
        }
        _ => {
            let resp = agent()
                .post(&format!("{local}/v1/companies"))
                .header("authorization", &format!("Bearer {}", server.admin_key))
                .send_json(serde_json::json!({ "name": name }))
                .map_err(net)?;
            let made: CompanyCreated = read(resp)?;
            store
                .set_sync_state(&local, &made.company_id, 0)
                .map_err(err)?;
            set(store, sync::URL, &local)?;
            set(store, sync::COMPANY, &made.company_id)?;
            set(store, sync::TOKEN, &made.owner_token)?;
            set(store, HOSTED, "1")?;
            set(store, ROLE, "owner")?;
            Some(made.company_id)
        }
    };
    let sync = sync::sync_now(slot)?;
    let company_id = get(store_of(slot)?, sync::COMPANY)?.unwrap_or_default();
    debug_assert!(created.as_ref().is_none_or(|c| *c == company_id));
    Ok(HostReport {
        company_id,
        created: created.is_some(),
        sync,
    })
}

// -------------------------------------------------------------- invites

/// What an invite code carries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteData {
    pub url: String,
    pub company_id: String,
    pub token: String,
    pub company_name: String,
    /// "accountant" or "viewer"; older codes without it count as accountant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The end-to-end key (hex) of a company synced through a relay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

pub fn encode_invite(data: &InviteData) -> String {
    let json = serde_json::to_vec(data).expect("invite serializes");
    format!("{INVITE_PREFIX}{}", URL_SAFE_NO_PAD.encode(json))
}

/// Read a pasted invite code. Line breaks and spaces that chat apps add are
/// ignored; anything that isn't a well-formed, safe invite is refused.
pub fn decode_invite(code: &str) -> ApiResult<InviteData> {
    const BAD: &str = "that is not a KhataOS invite code (it should start with khata1:). Copy the whole code again";
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() > MAX_INVITE_LEN {
        return Err(BAD.into());
    }
    let body = code.strip_prefix(INVITE_PREFIX).ok_or(BAD)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(body.trim_end_matches('='))
        .map_err(|_| {
            "this invite code is damaged or incomplete. Copy the whole code again".to_owned()
        })?;
    let mut d: InviteData = serde_json::from_slice(&bytes).map_err(|_| {
        "this invite code is damaged or incomplete. Copy the whole code again".to_owned()
    })?;
    d.url = d.url.trim().trim_end_matches('/').to_owned();
    d.company_name = d.company_name.trim().to_owned();
    let printable = |s: &str| !s.chars().any(char::is_control);
    if d.company_name.is_empty()
        || d.company_name.chars().count() > 200
        || !printable(&d.company_name)
    {
        return Err("this invite code has no valid company name".into());
    }
    if d.token.is_empty() || d.token.len() > 512 || !d.token.chars().all(|c| c.is_ascii_graphic()) {
        return Err("this invite code has no valid access token".into());
    }
    match d.role.as_deref() {
        None | Some("accountant") | Some("viewer") => {}
        Some(other) => return Err(format!("this invite code grants an unknown role {other:?}")),
    }
    if let Some(k) = &d.key
        && khata_seal::CompanyKey::from_hex(k).is_err()
    {
        return Err(
            "this invite code's encryption key is damaged. Copy the whole code again".into(),
        );
    }
    checked(&remote_of(&d))?;
    Ok(d)
}

fn remote_of(d: &InviteData) -> Remote {
    Remote {
        url: d.url.clone(),
        company_id: d.company_id.clone(),
        token: d.token.clone(),
    }
}

/// An invite without its secret, to confirm before connecting.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InvitePreview {
    pub company_name: String,
    pub url: String,
    pub role: String,
    /// Synced end-to-end encrypted through a relay.
    pub encrypted: bool,
}

pub fn inspect_invite(code: &str) -> ApiResult<InvitePreview> {
    let d = decode_invite(code)?;
    Ok(InvitePreview {
        encrypted: d.key.is_some(),
        company_name: d.company_name,
        url: d.url,
        role: d.role.unwrap_or_else(|| "accountant".into()),
    })
}

/// A new invite, shown once.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Invite {
    pub code: String,
    pub role: String,
    pub label: String,
    pub url: String,
    pub company_id: String,
    /// For Viewers who use a browser instead of the app; empty for relay
    /// companies (the relay cannot read the books).
    pub portal_url: String,
    pub token: String,
}

/// Someone with access, from the server's list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessRow {
    pub id: String,
    pub role: String,
    pub label: String,
    pub expires_at: Option<i64>,
    pub revoked: bool,
    #[serde(default)]
    pub expired: bool,
}

/// The remote and company name of the open company, read under the lock so
/// the network calls can run without it.
pub fn owner_link(slot: &Slot) -> ApiResult<(Remote, String)> {
    owner_link_keyed(slot).map(|(r, n, _)| (r, n))
}

/// [`owner_link`] plus the relay key, for invites.
pub fn owner_link_keyed(slot: &Slot) -> ApiResult<(Remote, String, Option<String>)> {
    let store = store_of(slot)?;
    let remote = sync::remote(store).map_err(|_| {
        "this company isn't shared yet: turn on Server mode in Exchange (Alt+Z) → Configure and host it".to_owned()
    })?;
    if matches!(get(store, ROLE)?.as_deref(), Some("viewer" | "accountant")) {
        return Err("only the company's owner can manage access; this computer was invited".into());
    }
    Ok((remote, store.company_name().map_err(err)?, get(store, KEY)?))
}

/// Where colleagues should connect: the office-network address for a company
/// hosted here, else the server this file syncs with.
pub fn default_invite_url(remote: &Remote, hosted_port: Option<u16>) -> String {
    match hosted_port {
        Some(port) => match lan_addresses().first() {
            Some(ip) => format!("http://{ip}:{port}"),
            None => format!("http://127.0.0.1:{port}"),
        },
        None => remote.url.clone(),
    }
}

pub fn list_access(remote: &Remote) -> ApiResult<Vec<AccessRow>> {
    checked(remote)?;
    let resp = agent()
        .get(&format!("{}/shares", base(remote)))
        .header("authorization", &format!("Bearer {}", remote.token))
        .call()
        .map_err(net)?;
    let now = chrono::Utc::now().timestamp();
    let mut rows: Vec<AccessRow> = read(resp)?;
    // The owner's own access is not someone they invited.
    rows.retain(|r| r.role != "owner");
    for r in &mut rows {
        r.expired = r.expires_at.is_some_and(|t| t <= now);
    }
    Ok(rows)
}

pub fn revoke_access(remote: &Remote, id: &str) -> ApiResult<()> {
    checked(remote)?;
    let id = uuid::Uuid::parse_str(id.trim()).map_err(|_| format!("invalid access id {id:?}"))?;
    let resp = agent()
        .delete(&format!("{}/shares/{id}", base(remote)))
        .header("authorization", &format!("Bearer {}", remote.token))
        .call()
        .map_err(net)?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        read::<serde_json::Value>(resp).map(|_| ())
    }
}

/// "192.168.1.5:9090" → "http://192.168.1.5:9090".
fn normalize_url(url: &str) -> String {
    let url = url.trim().trim_end_matches('/');
    if url.contains("://") {
        url.to_owned()
    } else {
        format!("http://{url}")
    }
}

#[derive(Deserialize)]
struct ShareCreated {
    id: String,
    token: String,
}

/// Issue access on the server and wrap it in one invite code that points
/// colleagues at `invite_url`.
pub fn create_invite(
    remote: &Remote,
    key: Option<&str>,
    company_name: &str,
    role: &str,
    label: &str,
    expires_in_days: Option<u32>,
    invite_url: &str,
) -> ApiResult<Invite> {
    let role = role.trim().to_ascii_lowercase();
    if !["accountant", "viewer"].contains(&role.as_str()) {
        return Err("invite as Accountant or Viewer".into());
    }
    let label = label.trim();
    if label.is_empty() {
        return Err("type who the invite is for, e.g. \"Ravi (accounts)\"".into());
    }
    if expires_in_days == Some(0) {
        return Err("expiry: leave it blank for no expiry, or enter 1 or more days".into());
    }
    let url = normalize_url(invite_url);
    // Validate the address colleagues will use before issuing anything.
    let data = InviteData {
        url: url.clone(),
        company_id: remote.company_id.clone(),
        token: "x".into(),
        company_name: company_name.to_owned(),
        role: Some(role.clone()),
        key: key.map(str::to_owned),
    };
    checked(&remote_of(&data))?;
    checked(remote)?;
    let resp = agent()
        .post(&format!("{}/shares", base(remote)))
        .header("authorization", &format!("Bearer {}", remote.token))
        .send_json(
            serde_json::json!({ "role": role, "label": label, "expires_in_days": expires_in_days }),
        )
        .map_err(net)?;
    let made: ShareCreated = read(resp)?;
    let _ = made.id;
    let data = InviteData {
        token: made.token.clone(),
        ..data
    };
    Ok(Invite {
        code: encode_invite(&data),
        role,
        label: label.to_owned(),
        portal_url: if key.is_some() {
            String::new()
        } else {
            format!("{url}/portal")
        },
        url,
        company_id: remote.company_id.clone(),
        token: made.token,
    })
}

// ---------------------------------------------------------------- join

/// A company file in `dir` already linked to this server company.
fn find_linked(dir: &Path, url: &str, company_id: &str) -> Option<PathBuf> {
    api::list_company_files(dir).into_iter().find_map(|f| {
        let store = Store::open(&f.path).ok()?;
        let same = get(&store, sync::COMPANY).ok()?? == company_id
            && get(&store, sync::URL).ok()??.trim_end_matches('/') == url;
        same.then(|| PathBuf::from(f.path))
    })
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let file = api::company_file_name(name);
    let stem = file.trim_end_matches(".khata");
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(&file)
            } else {
                dir.join(format!("{stem}-{n}.khata"))
            }
        })
        .find(|p| !p.exists())
        .expect("a free name")
}

fn remove_company_file(path: &Path) {
    for suffix in ["", "-wal", "-shm", "-journal", ".lock"] {
        let mut p = path.as_os_str().to_owned();
        p.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(p));
    }
}

/// [`join_locked`], dropping the company lock (tests).
#[cfg(test)]
pub fn join(slot: &mut Slot, dir: &Path, code: &str) -> ApiResult<(api::CompanyInfo, PathBuf)> {
    join_locked(slot, None, dir, code).map(|(info, path, _)| (info, path))
}

/// Connect to a shared company with an invite code: create (or reuse) a
/// local company file in `dir`, link it, pull everything and open it.
/// `open` is the company whose lock this window holds; the joined
/// company's lock comes back (`None`: it is that company).
pub fn join_locked(
    slot: &mut Slot,
    open: Option<&Path>,
    dir: &Path,
    code: &str,
) -> ApiResult<(api::CompanyInfo, PathBuf, Option<api::CompanyLock>)> {
    let d = decode_invite(code)?;
    std::fs::create_dir_all(dir).map_err(err)?;
    let existing = find_linked(dir, &d.url, &d.company_id);
    let fresh = existing.is_none();
    let path = existing.unwrap_or_else(|| unique_path(dir, &d.company_name));
    // Before anything is written: the file may be open in another KhataOS.
    let lock = api::CompanyLock::for_switch(open, &path)?;
    if fresh {
        api::create_company(slot, &path, &d.company_name)?;
    } else {
        api::open_company(slot, &path)?;
    }
    let role = d.role.clone().unwrap_or_else(|| "accountant".into());
    {
        let store = store_of(slot)?;
        set(store, sync::URL, &d.url)?;
        set(store, sync::COMPANY, &d.company_id)?;
        set(store, sync::TOKEN, &d.token)?;
        set(store, ROLE, &role)?;
        store.set_setting(HOSTED, None).map_err(err)?;
        store.set_setting(KEY, d.key.as_deref()).map_err(err)?;
        set(store, VIA, if d.key.is_some() { "relay" } else { "lan" })?;
        if get(store, CONNECT_ON_LOAD)?.is_none() {
            set(store, CONNECT_ON_LOAD, "yes")?;
        }
    }
    if let Err(e) = sync::sync_now(slot) {
        if fresh {
            *slot = None;
            drop(lock);
            remove_company_file(&path);
        }
        return Err(format!("couldn't connect to {}: {e}", d.company_name));
    }
    Ok((api::company_info(slot, Some(&path))?, path, lock))
}

// ---------------------------------------------------------- connectivity

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Connectivity {
    pub acts_as: ActsAs,
    pub port: u16,
    pub server_running: bool,
    pub server_port: Option<u16>,
    pub server_error: Option<String>,
    pub addresses: Vec<String>,
    pub company: Option<CompanyLink>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompanyLink {
    pub name: String,
    pub access: OnlineAccess,
    /// Auto-sync paused with Company → Disconnect.
    pub paused: bool,
    pub last_status: Option<String>,
    pub local_len: u64,
    pub synced_len: u64,
    /// Entries made here and not yet on the server.
    pub pending: u64,
    /// Entries on the server; `None` when it couldn't be reached.
    pub server_len: Option<u64>,
    pub reach_error: Option<String>,
}

/// What the open company's link looks like locally, read under the lock.
pub fn local_link(slot: &Slot, paused: bool) -> ApiResult<Option<(CompanyLink, Option<Remote>)>> {
    let Some((store, book)) = slot.as_ref() else {
        return Ok(None);
    };
    let access = online_access(slot)?;
    let remote = sync::remote(store).ok();
    let synced_len = match &remote {
        Some(r) => store.sync_state(&r.url).map_err(err)?.map_or(0, |(_, n)| n),
        None => 0,
    };
    let local_len = book.audit().entries().len() as u64;
    Ok(Some((
        CompanyLink {
            name: book.name.clone(),
            access,
            paused,
            last_status: get(store, sync::LAST)?,
            local_len,
            synced_len,
            pending: local_len.saturating_sub(synced_len),
            server_len: None,
            reach_error: None,
        },
        remote,
    )))
}

/// Entries on the server, counted from `from` (no lock needed).
pub fn server_len(remote: &Remote, from: u64, relay: bool) -> ApiResult<u64> {
    checked(remote)?;
    #[derive(Deserialize)]
    struct Page {
        entries: Vec<serde_json::Value>,
        #[serde(default)]
        more: bool,
    }
    let mut at = from;
    loop {
        let resp = agent()
            .get(&if relay {
                format!(
                    "{}/v1/relay/{}/pull?from={at}",
                    remote.url.trim_end_matches('/'),
                    remote.company_id
                )
            } else {
                format!("{}/pull?from={at}", base(remote))
            })
            .header("authorization", &format!("Bearer {}", remote.token))
            .call()
            .map_err(net)?;
        let page: Page = read(resp)?;
        at += page.entries.len() as u64;
        if !page.more || page.entries.is_empty() {
            return Ok(at);
        }
    }
}

// ------------------------------------------------------------ auto sync

/// Whether a quick TCP connect to the server works (so an unreachable server
/// never holds the company lock for long).
fn reachable(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let authority = rest.split('/').next().unwrap_or(rest);
    let with_port = if authority
        .rsplit_once(':')
        .is_some_and(|(_, p)| p.chars().all(|c| c.is_ascii_digit()))
        && !authority.ends_with(']')
    {
        authority.to_owned()
    } else if url.starts_with("https://") {
        format!("{authority}:443")
    } else {
        format!("{authority}:80")
    };
    with_port
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .is_some_and(|a| TcpStream::connect_timeout(&a, Duration::from_secs(2)).is_ok())
}

/// How the background loop treats the open company.
#[derive(Debug, Clone)]
pub struct Plan {
    pub remote: Remote,
    pub hosted: bool,
    pub pending: u64,
    pub read_only: bool,
    pub interval: Duration,
    /// The last attempt ended in a conflict, waiting for a manual rebase.
    pub conflicted: bool,
}

/// Whether the open company should sync in the background at all.
pub fn plan(slot: &Slot, cfg: &DeviceConfig, serving: Option<u16>) -> Option<Plan> {
    let (store, book) = slot.as_ref()?;
    let remote = sync::remote(store).ok()?;
    let access = online_access(slot).ok()?;
    let hosted_here =
        access.hosted && serving.is_some_and(|p| remote.url == format!("http://127.0.0.1:{p}"));
    let on = if access.hosted {
        cfg.acts_as.serves() && hosted_here
    } else {
        // The relay is an internet service, not the office Client/Server set-up.
        access.connect_on_load && (cfg.acts_as.connects() || access.encrypted)
    };
    if !on {
        return None;
    }
    let synced = store
        .sync_state(&remote.url)
        .ok()
        .flatten()
        .map_or(0, |(_, n)| n);
    // Notes and settings changed here count too, unless the server is too
    // old to take them (then they wait for the interval, not every tick).
    let records = match store.record_cursor(&remote.url) {
        Ok(c) if c.supported != Some(false) => store.pending_records(&remote.url).unwrap_or(0),
        _ => 0,
    };
    Some(Plan {
        pending: (book.audit().entries().len() as u64).saturating_sub(synced) + records,
        hosted: access.hosted,
        read_only: access.role == "viewer",
        interval: Duration::from_secs(u64::from(access.interval_minutes.max(1)) * 60),
        conflicted: get(store, sync::LAST)
            .ok()
            .flatten()
            .is_some_and(|s| s.starts_with("Conflict")),
        remote,
    })
}

/// Memory of the background loop for the open file.
#[derive(Debug, Default)]
pub struct Ticker {
    pub path: Option<PathBuf>,
    pub last_attempt: Option<Instant>,
    pub last_failed: bool,
    pub last_message: Option<String>,
    pub last_stamp: Option<Instant>,
}

/// How often the server PC checks its own server for colleagues' entries.
const HOSTED_POLL: Duration = Duration::from_secs(3);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(30);

/// Sync now? On opening, after every local save (pending entries), and on
/// the interval (a few seconds when the company is hosted here).
pub fn due(plan: &Plan, t: &Ticker, now: Instant, kicked: bool) -> bool {
    if plan.conflicted && !kicked {
        return false;
    }
    let Some(last) = t.last_attempt else {
        return true;
    };
    let since = now.saturating_duration_since(last);
    if t.last_failed && !kicked {
        return since >= RETRY_AFTER_FAILURE.min(plan.interval);
    }
    kicked
        || (plan.pending > 0 && !plan.read_only)
        || since
            >= if plan.hosted {
                HOSTED_POLL
            } else {
                plan.interval
            }
}

/// What the webview hears after a background sync (`khata-sync` event).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SyncEvent {
    /// "up_to_date", "pushed", "pulled", "conflict" or "error".
    pub status: String,
    pub count: u64,
    pub message: String,
}

/// One background sync of the open company. Quiet when nothing changed: the
/// status line is re-stamped at most once a minute.
pub fn auto_sync(slot: &mut Slot, t: &mut Ticker, now: Instant) -> SyncEvent {
    let Ok((store, book)) = sync::slot_mut(slot) else {
        return SyncEvent {
            status: "error".into(),
            count: 0,
            message: "no company is open".into(),
        };
    };
    let event = match sync::remote(store).map(|r| sync_any(store, book, &r)) {
        Err(m) => SyncEvent {
            status: "error".into(),
            count: 0,
            message: m,
        },
        Ok(Ok((books, records))) => {
            let (status, count, message) = sync::outcome(books, &records);
            SyncEvent {
                status: status.into(),
                count,
                message,
            }
        }
        Ok(Err(SyncError::Conflict { local, remote })) => SyncEvent {
            status: "conflict".into(),
            count: local,
            message: format!("Conflict: {local} change(s) here, {remote} on the server"),
        },
        Ok(Err(e)) => SyncEvent {
            status: "error".into(),
            count: 0,
            message: sync::explain(&e),
        },
    };
    let failed = event.status == "error";
    let quiet = event.status == "up_to_date"
        && t.last_message.as_deref() == Some("Up to date")
        && t.last_stamp
            .is_some_and(|s| now.saturating_duration_since(s) < Duration::from_secs(60));
    if !quiet {
        sync::stamp(
            store,
            if failed {
                "Sync failed"
            } else {
                &event.message
            },
        );
        t.last_stamp = Some(now);
    }
    t.last_attempt = Some(now);
    t.last_failed = failed;
    t.last_message = Some(event.message.clone());
    event
}

// ------------------------------------------------------- Tauri wiring

/// App-wide sharing state, managed by Tauri next to `crate::AppState`.
#[derive(Default)]
pub struct LanState {
    pub app_dir: Mutex<Option<PathBuf>>,
    pub config: Mutex<DeviceConfig>,
    pub server: Mutex<Option<EmbeddedServer>>,
    pub server_error: Mutex<Option<String>>,
    /// Company → Disconnect: no background sync until Connect.
    pub paused: AtomicBool,
    /// Sync at the next tick (Connect, or a setting changed).
    pub kick: AtomicBool,
}

impl LanState {
    pub fn serving_port(&self) -> Option<u16> {
        lock(&self.server).as_ref().map(EmbeddedServer::port)
    }

    /// Start or stop the embedded server to match `cfg`.
    pub fn apply(&self, cfg: DeviceConfig) {
        #[cfg(mobile)]
        let cfg = cfg.on_phone();
        *lock(&self.config) = cfg.clone();
        let Some(dir) = lock(&self.app_dir).clone() else {
            return;
        };
        let mut server = lock(&self.server);
        let keep = cfg.acts_as.serves() && server.as_ref().is_some_and(|s| s.port() == cfg.port);
        if !keep && let Some(old) = server.take() {
            old.stop();
        }
        let mut error = None;
        if cfg.acts_as.serves() && server.is_none() {
            let bind = SocketAddr::from(([0, 0, 0, 0], cfg.port));
            match EmbeddedServer::start(&dir.join(SERVER_DIR), bind) {
                Ok(s) => *server = Some(s),
                Err(e) => error = Some(e),
            }
        }
        *lock(&self.server_error) = error;
        self.kick.store(true, Ordering::Relaxed);
    }
}

/// Called once from `setup`: load device settings, start the server in
/// Server/Both mode and run the background sync loop.
pub fn init(app: &tauri::AppHandle, app_dir: PathBuf) {
    use tauri::Manager;
    let lan = app.state::<LanState>();
    let cfg = load_config(&app_dir);
    *lock(&lan.app_dir) = Some(app_dir);
    lan.apply(cfg);
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("khata-autosync".into())
        .spawn(move || {
            let mut t = Ticker::default();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                tick(&app, &mut t);
            }
        });
}

fn tick(app: &tauri::AppHandle, t: &mut Ticker) {
    use tauri::{Emitter, Manager};
    let lan = app.state::<LanState>();
    let state = app.state::<crate::AppState>();
    let path = state.path.lock().ok().and_then(|p| p.clone());
    if path != t.path {
        *t = Ticker {
            path: path.clone(),
            ..Ticker::default()
        };
    }
    if path.is_none() || lan.paused.load(Ordering::Relaxed) {
        return;
    }
    let cfg = lock(&lan.config).clone();
    let serving = lan.serving_port();
    let Some(plan) = plan(&lock(&state.company), &cfg, serving) else {
        return;
    };
    let kicked = lan.kick.swap(false, Ordering::Relaxed);
    let now = Instant::now();
    if !due(&plan, t, now, kicked) {
        return;
    }
    let event = if plan.hosted || reachable(&plan.remote.url) {
        let mut slot = lock(&state.company);
        // Still the same company, and no manual sync resolved it meanwhile.
        if state.path.lock().ok().and_then(|p| p.clone()) != path {
            return;
        }
        auto_sync(&mut slot, t, now)
    } else {
        let message = "Offline: the server isn't reachable".to_owned();
        if let Some((store, _)) = lock(&state.company).as_ref()
            && t.last_message.as_deref() != Some(&message)
        {
            sync::stamp(store, &message);
        }
        t.last_attempt = Some(now);
        t.last_failed = true;
        t.last_message = Some(message.clone());
        SyncEvent {
            status: "error".into(),
            count: 0,
            message,
        }
    };
    let _ = app.emit("khata-sync", &event);
}

/// Tauri commands for sharing. Network calls run off the main thread and
/// never hold the company lock longer than needed.
pub mod cmd {
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;

    use serde::Serialize;
    use tauri::{AppHandle, Manager, State};

    use super::*;
    use crate::AppState;

    fn slot<'a>(state: &'a AppState) -> MutexGuard<'a, Slot> {
        lock(&state.company)
    }

    async fn blocking<T: Send + 'static>(
        f: impl FnOnce() -> ApiResult<T> + Send + 'static,
    ) -> ApiResult<T> {
        tauri::async_runtime::spawn_blocking(f)
            .await
            .map_err(|e| e.to_string())?
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct SharingConfig {
        pub acts_as: ActsAs,
        pub port: u16,
        pub server_running: bool,
        pub server_port: Option<u16>,
        pub server_error: Option<String>,
        pub addresses: Vec<String>,
        /// The relay offered for internet sharing; empty when there is none.
        pub relay_url: String,
        /// `relay_url` is the one built into this copy of KhataOS.
        pub relay_builtin: bool,
    }

    fn view(lan: &LanState) -> SharingConfig {
        let cfg = lock(&lan.config).clone();
        let server_port = lan.serving_port();
        SharingConfig {
            acts_as: cfg.acts_as,
            port: cfg.port,
            server_running: server_port.is_some(),
            server_port,
            server_error: lock(&lan.server_error).clone(),
            addresses: lan_addresses(),
            relay_builtin: cfg.relay_url.is_empty() && !cfg.relay_address().is_empty(),
            relay_url: cfg.relay_address(),
        }
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn sharing_config(lan: State<'_, LanState>) -> SharingConfig {
        view(&lan)
    }

    /// Save "acts as" and the port, then start, restart or stop the server.
    #[tauri::command(rename_all = "snake_case")]
    pub async fn set_sharing_config(
        app: AppHandle,
        acts_as: ActsAs,
        port: u16,
    ) -> ApiResult<SharingConfig> {
        blocking(move || {
            #[cfg(mobile)]
            if acts_as.serves() {
                return Err(
                    "a phone can't host the office server: choose Client (or None) here and run Server mode on a computer"
                        .to_owned(),
                );
            }
            let lan = app.state::<LanState>();
            let dir = lock(&lan.app_dir)
                .clone()
                .ok_or("the app data folder is unknown")?;
            let cfg = DeviceConfig {
                acts_as,
                port,
                relay_url: lock(&lan.config).relay_url.clone(),
            };
            save_config(&dir, &cfg)?;
            lan.apply(cfg);
            // A company hosted here follows the server to its new port.
            let state = app.state::<AppState>();
            let server = lock(&lan.server);
            if let Some(server) = server.as_ref() {
                let mut slot = slot(&state);
                if super::online_access(&slot).is_ok_and(|a| a.hosted) {
                    let _ = super::host_company(&mut slot, server);
                }
            }
            drop(server);
            Ok(view(&lan))
        })
        .await
    }

    /// Exchange → Configure → Relay address. Empty goes back to the one
    /// built into the app, if any.
    #[tauri::command(rename_all = "snake_case")]
    pub fn set_relay_address(
        lan: State<'_, LanState>,
        relay_url: String,
    ) -> ApiResult<SharingConfig> {
        let dir = lock(&lan.app_dir)
            .clone()
            .ok_or("the app data folder is unknown")?;
        let mut cfg = lock(&lan.config).clone();
        cfg.relay_url = normalize_relay(&relay_url)?;
        if cfg.relay_url == BUILD_DEFAULT_RELAY.trim().trim_end_matches('/') {
            cfg.relay_url.clear();
        }
        save_config(&dir, &cfg)?;
        *lock(&lan.config) = cfg;
        Ok(view(&lan))
    }

    /// Company → Move to another relay…
    #[tauri::command(rename_all = "snake_case")]
    pub async fn move_relay(
        app: AppHandle,
        relay_url: String,
        signup_key: Option<String>,
    ) -> ApiResult<MoveReport> {
        blocking(move || {
            let lan = app.state::<LanState>();
            let state = app.state::<AppState>();
            let report = super::move_relay(&mut slot(&state), &relay_url, signup_key.as_deref())?;
            // The old relay is going away: offer the new one for other
            // companies too, if this computer was using the old one.
            let mut cfg = lock(&lan.config).clone();
            if cfg.relay_address() == report.from
                && let Some(dir) = lock(&lan.app_dir).clone()
            {
                cfg.relay_url = report.to.clone();
                if save_config(&dir, &cfg).is_ok() {
                    *lock(&lan.config) = cfg;
                }
            }
            lan.kick.store(true, Ordering::Relaxed);
            Ok(report)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn host_company(app: AppHandle) -> ApiResult<HostReport> {
        blocking(move || {
            let lan = app.state::<LanState>();
            let server = lock(&lan.server);
            let server = server.as_ref().ok_or(
                "the server isn't running: set \"KhataOS acts as\" to Server or Both in Exchange (Alt+Z) → Configure",
            )?;
            let state = app.state::<AppState>();
            let report = super::host_company(&mut slot(&state), server)?;
            lan.paused.store(false, Ordering::Relaxed);
            Ok(report)
        })
        .await
    }

    /// Online Access → Connect through the relay (internet, encrypted).
    #[tauri::command(rename_all = "snake_case")]
    pub async fn connect_relay(
        app: AppHandle,
        relay_url: String,
        signup_key: Option<String>,
    ) -> ApiResult<HostReport> {
        blocking(move || {
            let lan = app.state::<LanState>();
            let state = app.state::<AppState>();
            let report =
                super::connect_relay(&mut slot(&state), &relay_url, signup_key.as_deref())?;
            lan.paused.store(false, Ordering::Relaxed);
            Ok(report)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn online_access(
        state: State<'_, AppState>,
        lan: State<'_, LanState>,
    ) -> ApiResult<OnlineAccess> {
        let access = super::online_access(&slot(&state))?;
        Ok(with_device_relay(access, &lock(&lan.config)))
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn set_online_access(
        state: State<'_, AppState>,
        lan: State<'_, LanState>,
        connect_on_load: bool,
        interval_minutes: u32,
        via: String,
        relay_url: String,
    ) -> ApiResult<OnlineAccess> {
        // The device's relay is the default: keep it out of the company file
        // so a later change in Exchange → Configure still applies.
        let device = lock(&lan.config).relay_address();
        let relay_url = if relay_url.trim().trim_end_matches('/') == device {
            ""
        } else {
            &relay_url
        };
        let out = super::set_online_access(
            &slot(&state),
            connect_on_load,
            interval_minutes,
            &via,
            relay_url,
        )?;
        lan.kick.store(true, Ordering::Relaxed);
        Ok(with_device_relay(out, &lock(&lan.config)))
    }

    /// Company → Connect / Disconnect: resume or pause background sync.
    #[tauri::command(rename_all = "snake_case")]
    pub fn set_connected(lan: State<'_, LanState>, connected: bool) {
        lan.paused.store(!connected, Ordering::Relaxed);
        if connected {
            lan.kick.store(true, Ordering::Relaxed);
        }
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn connectivity_status(app: AppHandle) -> ApiResult<Connectivity> {
        blocking(move || {
            let lan = app.state::<LanState>();
            let state = app.state::<AppState>();
            let base = view(&lan);
            let local = local_link(&slot(&state), lan.paused.load(Ordering::Relaxed))?;
            let company = local.map(|(mut link, remote)| {
                if let Some(remote) = remote {
                    match server_len(&remote, link.synced_len, link.access.encrypted) {
                        Ok(n) => link.server_len = Some(n),
                        Err(e) => link.reach_error = Some(e),
                    }
                }
                link
            });
            Ok(Connectivity {
                acts_as: base.acts_as,
                port: base.port,
                server_running: base.server_running,
                server_port: base.server_port,
                server_error: base.server_error,
                addresses: base.addresses,
                company,
            })
        })
        .await
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct AccessList {
        pub company: String,
        pub hosted: bool,
        pub default_url: String,
        pub rows: Vec<AccessRow>,
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn list_access(app: AppHandle) -> ApiResult<AccessList> {
        blocking(move || {
            let lan = app.state::<LanState>();
            let state = app.state::<AppState>();
            let (remote, company, hosted) = {
                let slot = slot(&state);
                let (remote, company) = owner_link(&slot)?;
                (remote, company, super::online_access(&slot)?.hosted)
            };
            let port = if hosted { lan.serving_port() } else { None };
            Ok(AccessList {
                default_url: default_invite_url(&remote, port),
                rows: super::list_access(&remote)?,
                company,
                hosted,
            })
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn create_invite(
        app: AppHandle,
        role: String,
        label: String,
        expires_in_days: Option<u32>,
        url: String,
    ) -> ApiResult<Invite> {
        blocking(move || {
            let state = app.state::<AppState>();
            let (remote, company, key) = owner_link_keyed(&slot(&state))?;
            super::create_invite(
                &remote,
                key.as_deref(),
                &company,
                &role,
                &label,
                expires_in_days,
                &url,
            )
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn revoke_access(app: AppHandle, id: String) -> ApiResult<()> {
        blocking(move || {
            let state = app.state::<AppState>();
            let (remote, _) = owner_link(&slot(&state))?;
            super::revoke_access(&remote, &id)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn inspect_invite(code: String) -> ApiResult<InvitePreview> {
        super::inspect_invite(&code)
    }

    /// Connect to a shared company and open it. This computer becomes a
    /// Client (or Both, if it also serves).
    #[tauri::command(rename_all = "snake_case")]
    pub async fn join_company(app: AppHandle, code: String) -> ApiResult<api::CompanyInfo> {
        blocking(move || {
            let lan = app.state::<LanState>();
            let state = app.state::<AppState>();
            let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let dir = api::default_company_dir(&base);
            // Join into a fresh slot: if it fails, the open company stays open.
            let open = lock(&state.company_lock).as_ref().map(|l| l.company().to_owned());
            let mut joined: Slot = None;
            let (info, path, joined_lock) = super::join_locked(&mut joined, open.as_deref(), &dir, &code)?;
            {
                let mut open_slot = slot(&state);
                *open_slot = joined;
                if let Some(l) = joined_lock {
                    *lock(&state.company_lock) = Some(l);
                }
            }
            if let Ok(mut p) = state.path.lock() {
                *p = Some(PathBuf::from(&path));
            }
            let mut cfg = lock(&lan.config).clone();
            cfg.acts_as = match cfg.acts_as {
                ActsAs::None => ActsAs::Client,
                ActsAs::Server => ActsAs::Both,
                other => other,
            };
            if save_config(&base, &cfg).is_ok() {
                lan.apply(cfg);
            }
            lan.paused.store(false, Ordering::Relaxed);
            Ok(info)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tests::{TempFile, demo, entry, voucher};
    use crate::api::{post_voucher, trial_balance};
    use khata_core::DrCr;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("khataos-lan-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn loopback() -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], 0))
    }

    fn health(port: u16) -> Result<String, String> {
        agent()
            .get(&format!("http://127.0.0.1:{port}/v1/health"))
            .call()
            .map_err(|e| e.to_string())
            .and_then(|mut r| r.body_mut().read_to_string().map_err(|e| e.to_string()))
    }

    #[test]
    fn embedded_server_starts_and_stops_on_an_ephemeral_port() {
        let dir = temp_dir("start-stop");
        let server = EmbeddedServer::start(&dir, loopback()).unwrap();
        let port = server.port();
        assert_ne!(port, 0);
        assert_eq!(health(port).unwrap(), "ok");
        let key = std::fs::read_to_string(dir.join(ADMIN_KEY_FILE)).unwrap();
        assert!(key.len() >= 64, "random admin key persisted");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(ADMIN_KEY_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "admin key readable only by this user");
        }
        // The same port can't be taken twice.
        let clash = EmbeddedServer::start(&dir, SocketAddr::from(([127, 0, 0, 1], port)));
        assert!(clash.err().unwrap().contains("already in use"));

        server.stop();
        assert!(health(port).is_err(), "port closed after stop");
        // Restart keeps the admin key, and the port can be reused.
        let again = EmbeddedServer::start(&dir, SocketAddr::from(([127, 0, 0, 1], port))).unwrap();
        assert_eq!(again.admin_key, key.trim());
        assert_eq!(health(port).unwrap(), "ok");
        drop(again);
        assert!(health(port).is_err(), "dropping the server stops it");
    }

    #[test]
    fn device_config_persists_and_rejects_privileged_ports() {
        let dir = temp_dir("config");
        assert_eq!(
            load_config(&dir),
            DeviceConfig {
                acts_as: ActsAs::None,
                port: 9090,
                relay_url: String::new(),
            }
        );
        let cfg = DeviceConfig {
            acts_as: ActsAs::Both,
            port: 9191,
            relay_url: "https://relay.example.in".into(),
        };
        save_config(&dir, &cfg).unwrap();
        assert_eq!(load_config(&dir), cfg);
        assert!(
            save_config(
                &dir,
                &DeviceConfig {
                    acts_as: ActsAs::Server,
                    port: 80,
                    ..DeviceConfig::default()
                }
            )
            .is_err()
        );
        // Internet relays must be https.
        assert!(
            save_config(
                &dir,
                &DeviceConfig {
                    relay_url: "http://relay.example.in".into(),
                    ..DeviceConfig::default()
                }
            )
            .is_err()
        );
        // Files from before the relay setting still load.
        std::fs::write(dir.join(CONFIG_FILE), r#"{"acts_as":"client","port":9090}"#).unwrap();
        assert_eq!(load_config(&dir).acts_as, ActsAs::Client);
        assert_eq!(load_config(&dir).relay_address(), BUILD_DEFAULT_RELAY);
    }

    #[test]
    fn server_mode_links_pushes_and_follows_a_port_change() {
        let dir = temp_dir("host");
        let f = TempFile::new("lan-host");
        let (mut slot, ..) = demo(&f);
        let server = EmbeddedServer::start(&dir, loopback()).unwrap();

        let first = host_company(&mut slot, &server).unwrap();
        assert!(first.created);
        assert_eq!(first.sync.status, "pushed");
        assert!(server.hosts(&first.company_id));
        let access = online_access(&slot).unwrap();
        assert!(access.hosted && access.linked);
        assert_eq!(
            (access.role.as_str(), access.url),
            ("owner", server.local_url())
        );

        // The server has the whole trail.
        let (remote, _) = owner_link(&slot).unwrap();
        let local_len = local_link(&slot, false).unwrap().unwrap().0.local_len;
        assert_eq!(server_len(&remote, 0, false).unwrap(), local_len);

        // Again: nothing new is created.
        let again = host_company(&mut slot, &server).unwrap();
        assert!(!again.created);
        assert_eq!(again.sync.status, "up_to_date");

        // Restart on another port: the link moves without a conflict.
        drop(server);
        let moved = EmbeddedServer::start(&dir, loopback()).unwrap();
        let r = host_company(&mut slot, &moved).unwrap();
        assert_eq!((r.created, r.sync.status.as_str()), (false, "up_to_date"));
        assert_eq!(online_access(&slot).unwrap().url, moved.local_url());

        // The background loop syncs a hosted company only while serving it.
        let cfg = DeviceConfig {
            acts_as: ActsAs::Server,
            port: moved.port(),
            ..DeviceConfig::default()
        };
        assert!(plan(&slot, &cfg, Some(moved.port())).is_some());
        assert!(plan(&slot, &cfg, None).is_none());
        assert!(
            plan(
                &slot,
                &DeviceConfig {
                    acts_as: ActsAs::Client,
                    ..cfg.clone()
                },
                Some(moved.port())
            )
            .is_none()
        );
    }

    #[test]
    fn a_company_linked_elsewhere_is_not_rehosted() {
        let dir = temp_dir("elsewhere");
        let f = TempFile::new("lan-elsewhere");
        let (mut slot, ..) = demo(&f);
        sync::set_settings(
            &slot,
            "https://books.example.in",
            "0190f3c4-1111-7aaa-8bbb-0123456789ab",
            "t",
        )
        .unwrap();
        let server = EmbeddedServer::start(&dir, loopback()).unwrap();
        assert!(
            host_company(&mut slot, &server)
                .unwrap_err()
                .contains("already syncs with")
        );
    }

    fn sample() -> InviteData {
        InviteData {
            url: "http://192.168.1.20:9090".into(),
            company_id: "0190f3c4-1111-7aaa-8bbb-0123456789ab".into(),
            token: "abc123".into(),
            company_name: "Sharma & Sons".into(),
            role: Some("viewer".into()),
            key: None,
        }
    }

    #[test]
    fn invite_codes_round_trip() {
        let d = sample();
        let code = encode_invite(&d);
        assert!(code.starts_with("khata1:"));
        assert!(
            code[7..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_eq!(decode_invite(&code).unwrap(), d);
        // Chat apps wrap long lines and add spaces.
        let wrapped = format!("  {}\n{} ", &code[..30], &code[30..]);
        assert_eq!(decode_invite(&wrapped).unwrap(), d);
        // Codes without a role are Accountant invites.
        let old = encode_invite(&InviteData {
            role: None,
            ..sample()
        });
        assert_eq!(inspect_invite(&old).unwrap().role, "accountant");
        assert!(
            serde_json::to_string(&inspect_invite(&code).unwrap())
                .unwrap()
                .find("abc123")
                .is_none(),
            "no token in previews"
        );
        // Relay companies carry their end-to-end key.
        let key = khata_seal::CompanyKey::generate().unwrap().to_hex();
        let keyed = InviteData {
            url: "https://relay.example.org".into(),
            key: Some(key.clone()),
            ..sample()
        };
        let code = encode_invite(&keyed);
        assert_eq!(decode_invite(&code).unwrap(), keyed);
        let preview = inspect_invite(&code).unwrap();
        assert!(preview.encrypted);
        assert!(
            !serde_json::to_string(&preview).unwrap().contains(&key),
            "no key in previews"
        );
        assert!(!inspect_invite(&encode_invite(&sample())).unwrap().encrypted);
    }

    #[test]
    fn malformed_or_unsafe_invite_codes_are_refused() {
        let enc = |d: &InviteData| encode_invite(d);
        let raw = |json: &str| format!("khata1:{}", URL_SAFE_NO_PAD.encode(json));
        for bad in [
            String::new(),
            "hello".into(),
            "khata2:abc".into(),
            "khata1:".into(),
            "khata1:!!!not-base64!!!".into(),
            raw("not json"),
            raw(r#"{"url":"http://192.168.1.20:9090"}"#),
            "khata1:".to_owned() + &"A".repeat(5000),
            // Plain http across the internet would leak the token.
            enc(&InviteData {
                url: "http://203.0.113.9:9090".into(),
                ..sample()
            }),
            enc(&InviteData {
                url: "ftp://192.168.1.20".into(),
                ..sample()
            }),
            enc(&InviteData {
                company_id: "../../admin".into(),
                ..sample()
            }),
            enc(&InviteData {
                token: String::new(),
                ..sample()
            }),
            enc(&InviteData {
                token: "a b".into(),
                ..sample()
            }),
            enc(&InviteData {
                company_name: " ".into(),
                ..sample()
            }),
            enc(&InviteData {
                company_name: "x\u{7}".into(),
                ..sample()
            }),
            enc(&InviteData {
                role: Some("owner".into()),
                ..sample()
            }),
            enc(&InviteData {
                key: Some("not-a-key".into()),
                ..sample()
            }),
            enc(&InviteData {
                key: Some("ab".repeat(16)),
                ..sample()
            }),
        ] {
            assert!(decode_invite(&bad).is_err(), "{bad}");
        }
        assert!(
            decode_invite(&enc(&InviteData {
                url: "https://books.example.in".into(),
                ..sample()
            }))
            .is_ok()
        );
    }

    /// Server PC hosts the demo company and invites a colleague.
    fn hosted_demo(name: &str) -> (TempFile, Slot, EmbeddedServer) {
        let f = TempFile::new(&format!("lan-{name}"));
        let (mut slot, ..) = demo(&f);
        let server =
            EmbeddedServer::start(&temp_dir(&format!("{name}-server")), loopback()).unwrap();
        host_company(&mut slot, &server).unwrap();
        (f, slot, server)
    }

    fn invite(slot: &Slot, server: &EmbeddedServer, role: &str) -> Invite {
        let (remote, company) = owner_link(slot).unwrap();
        create_invite(
            &remote,
            None,
            &company,
            role,
            "Ravi (accounts)",
            Some(30),
            &server.local_url(),
        )
        .unwrap()
    }

    #[test]
    fn joining_by_invite_gives_identical_books_both_ways() {
        let (_f, mut host, server) = hosted_demo("join");
        let inv = invite(&host, &server, "Accountant");
        assert_eq!(inv.role, "accountant");
        assert_eq!(inv.portal_url, format!("{}/portal", server.local_url()));

        let dir = temp_dir("join-client");
        let mut client = None;
        let (info, path) = join(&mut client, &dir, &inv.code).unwrap();
        assert_eq!(info.name, "Demo Traders");
        assert!(path.starts_with(&dir));
        assert_eq!(
            trial_balance(&client, None).unwrap(),
            trial_balance(&host, None).unwrap()
        );
        let access = online_access(&client).unwrap();
        assert_eq!(
            (access.role.as_str(), access.connect_on_load, access.hosted),
            ("accountant", true, false)
        );

        // The colleague posts; the server PC receives it on its next sync.
        let ids = |slot: &Slot, name: &str| {
            api::list_ledgers(slot, name, "all")
                .unwrap()
                .into_iter()
                .find(|l| l.name == name)
                .unwrap()
                .id
        };
        let (rent, bank) = (ids(&client, "Office Rent"), ids(&client, "HDFC Bank"));
        post_voucher(
            &mut client,
            voucher(
                "Payment",
                "2026-04-03",
                vec![entry(&rent, DrCr::Dr, "500"), entry(&bank, DrCr::Cr, "500")],
            ),
        )
        .unwrap();
        let mut t = Ticker::default();
        let cfg = DeviceConfig {
            acts_as: ActsAs::Client,
            ..DeviceConfig::default()
        };
        let p = plan(&client, &cfg, None).unwrap();
        assert!(due(&p, &t, Instant::now(), false), "sync on open");
        assert_eq!(
            auto_sync(&mut client, &mut t, Instant::now()).status,
            "pushed"
        );
        assert_eq!(sync::sync_now(&mut host).unwrap().status, "pulled");
        assert_eq!(
            trial_balance(&client, None).unwrap(),
            trial_balance(&host, None).unwrap()
        );

        // Joining again reuses the same file.
        drop(client);
        let mut again = None;
        let (_, path2) = join(&mut again, &dir, &inv.code).unwrap();
        assert_eq!(path2, path);
        assert_eq!(api::list_company_files(&dir).len(), 1);

        // A revoked invite stops working.
        let (remote, _) = owner_link(&host).unwrap();
        let rows = list_access(&remote).unwrap();
        let row = rows.iter().find(|r| r.label == "Ravi (accounts)").unwrap();
        assert_eq!(
            (row.role.as_str(), row.revoked, row.expired),
            ("accountant", false, false)
        );
        revoke_access(&remote, &row.id).unwrap();
        assert!(
            list_access(&remote)
                .unwrap()
                .iter()
                .any(|r| r.id == row.id && r.revoked)
        );
        assert!(sync::sync_now(&mut again).unwrap_err().contains("refused"));
    }

    #[test]
    fn a_viewer_join_is_read_only() {
        let (_f, host, server) = hosted_demo("viewer");
        let inv = invite(&host, &server, "viewer");
        let dir = temp_dir("viewer-client");
        let mut viewer = None;
        join(&mut viewer, &dir, &inv.code).unwrap();
        assert_eq!(
            trial_balance(&viewer, None).unwrap(),
            trial_balance(&host, None).unwrap()
        );
        let ids = api::list_ledgers(&viewer, "", "all").unwrap();
        let (a, b) = (ids[0].id.clone(), ids[1].id.clone());
        let refused = post_voucher(
            &mut viewer,
            voucher(
                "Journal",
                "2026-04-03",
                vec![entry(&a, DrCr::Dr, "1"), entry(&b, DrCr::Cr, "1")],
            ),
        )
        .unwrap_err();
        assert!(
            refused.contains("Viewer") && refused.contains("read-only"),
            "{refused}"
        );
        let group = crate::api::tests::group_id(&viewer, "Indirect Expenses");
        assert!(
            api::create_ledger(&mut viewer, "Tea", &group, "")
                .unwrap_err()
                .contains("read-only")
        );
        // It can still receive the owner's entries, and can't invite anyone.
        assert_eq!(sync::sync_now(&mut viewer).unwrap().status, "up_to_date");
        assert!(owner_link(&viewer).unwrap_err().contains("owner"));
        assert_eq!(online_access(&viewer).unwrap().role, "viewer");
    }

    #[test]
    fn notes_and_the_invoice_design_reach_colleagues_over_the_office_server() {
        use crate::desk::{self, NoteInput};
        let (_f, mut host, server) = hosted_demo("records");
        let note = desk::save_note(
            &mut host,
            &NoteInput {
                title: "Ask Mehta for the TDS certificate".into(),
                ..NoteInput::default()
            },
        )
        .unwrap();
        let design = khata_print::InvoiceDesign {
            footer: "Thank you for your business".into(),
            ..khata_print::InvoiceDesign::default()
        };
        crate::invoice_design::set_design(&mut host, design.clone()).unwrap();
        let p = plan(
            &host,
            &DeviceConfig {
                acts_as: ActsAs::Server,
                ..DeviceConfig::default()
            },
            Some(server.port()),
        )
        .unwrap();
        assert_eq!(p.pending, 2, "a note or design change makes the loop sync");
        let r = sync::sync_now(&mut host).unwrap();
        assert_eq!(
            (r.status.as_str(), r.message.as_str()),
            ("pushed", "Sent 2 update(s) to notes and settings")
        );

        // An accountant joining gets them with the books.
        let inv = invite(&host, &server, "accountant");
        let mut client = None;
        join(&mut client, &temp_dir("records-client"), &inv.code).unwrap();
        assert_eq!(desk::notes(&client, None, None).unwrap()[0].title, note.title);
        assert_eq!(crate::invoice_design::branding(&client).design, design);
        let overview = desk::overview(&client, "2026-10-21", "2026-10-28").unwrap();
        assert!(overview.can_write);

        // The accountant ticks it done; the server PC hears it.
        desk::set_note_done(&mut client, &note.id, true).unwrap();
        let mut t = Ticker::default();
        assert_eq!(auto_sync(&mut client, &mut t, Instant::now()).status, "pushed");
        let r = sync::sync_now(&mut host).unwrap();
        assert_eq!(
            (r.status.as_str(), r.message.as_str()),
            ("pulled", "Received 1 update(s) to notes and settings")
        );
        assert!(desk::notes(&host, None, None).unwrap()[0].done);

        // The owner deletes the note while the accountant, offline, edits it:
        // the deletion wins on both.
        desk::save_note(
            &mut client,
            &NoteInput {
                id: Some(note.id.clone()),
                title: "Edited offline".into(),
                ..NoteInput::default()
            },
        )
        .unwrap();
        desk::delete_note(&mut host, &note.id).unwrap();
        sync::sync_now(&mut host).unwrap();
        sync::sync_now(&mut client).unwrap();
        sync::sync_now(&mut host).unwrap();
        assert!(desk::notes(&host, None, None).unwrap().is_empty());
        assert!(desk::notes(&client, None, None).unwrap().is_empty());

        // A viewer reads the design but can't change notes or the design.
        let view = invite(&host, &server, "viewer");
        let mut viewer = None;
        join(&mut viewer, &temp_dir("records-viewer"), &view.code).unwrap();
        assert_eq!(crate::invoice_design::branding(&viewer).design, design);
        assert!(!desk::overview(&viewer, "2026-10-21", "2026-10-28").unwrap().can_write);
        assert!(
            desk::save_note(&mut viewer, &NoteInput { title: "x".into(), ..NoteInput::default() })
                .unwrap_err()
                .contains("read-only")
        );
        assert!(
            crate::invoice_design::set_design(&mut viewer, khata_print::InvoiceDesign::default())
                .unwrap_err()
                .contains("read-only")
        );

        // No sync token, and no device-local row, ever reaches the server PC's store.
        let token = |slot: &Slot| sync::remote(&slot.as_ref().unwrap().0).unwrap().token;
        let tokens = [token(&host), token(&client), token(&viewer)];
        let dir = std::env::temp_dir().join(format!("khataos-lan-{}-records-server", std::process::id()));
        let mut stored = Vec::new();
        for e in std::fs::read_dir(&dir).unwrap() {
            stored.extend(std::fs::read(e.unwrap().path()).unwrap_or_default());
        }
        let text = String::from_utf8_lossy(&stored);
        for t in &tokens {
            assert!(!text.contains(t.as_str()), "a sync token reached the server");
        }
        for key in [sync::TOKEN, sync::URL, ROLE, HOSTED, CONNECT_ON_LOAD] {
            assert!(!text.contains(key), "{key} reached the server");
        }
    }

    #[test]
    fn join_failure_leaves_no_file_behind() {
        let dir = temp_dir("join-fail");
        let code = encode_invite(&InviteData {
            url: "http://127.0.0.1:9".into(),
            ..sample()
        });
        let mut slot = None;
        assert!(
            join(&mut slot, &dir, &code)
                .unwrap_err()
                .contains("couldn't connect")
        );
        assert!(slot.is_none());
        assert!(api::list_company_files(&dir).is_empty());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn relay_sharing_is_end_to_end_encrypted() {
        let dir = temp_dir("relay");
        let relay = EmbeddedServer::start_with(&dir, loopback(), true).unwrap();
        let f = TempFile::new("lan-relay-owner");
        let (mut owner, ..) = demo(&f);
        // Online Access: Sync via the relay, then Connect.
        set_online_access(&owner, true, 2, "relay", &relay.local_url()).unwrap();
        let access = online_access(&owner).unwrap();
        assert_eq!(
            (access.via.as_str(), access.relay_url.as_str()),
            ("relay", relay.local_url().as_str())
        );
        assert!(
            set_online_access(&owner, true, 2, "relay", "http://203.0.113.9")
                .unwrap_err()
                .contains("https://")
        );

        let r = connect_relay(&mut owner, &access.relay_url, None).unwrap();
        assert_eq!((r.created, r.sync.status.as_str()), (true, "pushed"));
        let access = online_access(&owner).unwrap();
        assert!(access.encrypted && access.linked && !access.hosted);
        assert_eq!(access.role, "owner");
        let again = connect_relay(&mut owner, &relay.local_url(), None).unwrap();
        assert_eq!(
            (again.created, again.sync.status.as_str()),
            (false, "up_to_date")
        );
        // A company can't be hosted on the office server and the relay at once.
        let office = EmbeddedServer::start(&temp_dir("relay-office"), loopback()).unwrap();
        assert!(
            host_company(&mut owner, &office)
                .unwrap_err()
                .contains("already syncs with")
        );

        // The relay holds the whole trail, but only as ciphertext.
        let (remote, company, key) = owner_link_keyed(&owner).unwrap();
        let local_len = local_link(&owner, false).unwrap().unwrap().0.local_len;
        assert_eq!(server_len(&remote, 0, true).unwrap(), local_len);
        let mut stored = Vec::new();
        for e in std::fs::read_dir(&dir).unwrap() {
            stored.extend(std::fs::read(e.unwrap().path()).unwrap_or_default());
        }
        let text = String::from_utf8_lossy(&stored);
        assert!(
            !text.contains("HDFC Bank") && !text.contains("Sharma Enterprises"),
            "relay sees no plaintext"
        );

        // Invites (shares work for relay companies) carry the key.
        let inv = create_invite(
            &remote,
            key.as_deref(),
            &company,
            "accountant",
            "CA",
            None,
            &remote.url,
        )
        .unwrap();
        assert!(
            inv.portal_url.is_empty(),
            "no browser portal: the relay can't read the books"
        );
        assert_eq!(decode_invite(&inv.code).unwrap().key, key);
        assert!(
            list_access(&remote)
                .unwrap()
                .iter()
                .any(|r| r.label == "CA")
        );

        let mut client = None;
        join(&mut client, &temp_dir("relay-client"), &inv.code).unwrap();
        assert!(online_access(&client).unwrap().encrypted);
        assert_eq!(
            trial_balance(&client, None).unwrap(),
            trial_balance(&owner, None).unwrap()
        );

        // Both ways, through the background loop and Sync now.
        let ids = |slot: &Slot, name: &str| {
            api::list_ledgers(slot, name, "all")
                .unwrap()
                .into_iter()
                .find(|l| l.name == name)
                .unwrap()
                .id
        };
        let (rent, bank) = (ids(&client, "Office Rent"), ids(&client, "HDFC Bank"));
        post_voucher(
            &mut client,
            voucher(
                "Payment",
                "2026-04-04",
                vec![entry(&rent, DrCr::Dr, "900"), entry(&bank, DrCr::Cr, "900")],
            ),
        )
        .unwrap();
        // Client mode isn't needed for the relay: Connect on loading is enough.
        let none = DeviceConfig::default();
        assert!(plan(&client, &none, None).is_some());
        let mut t = Ticker::default();
        assert_eq!(
            auto_sync(&mut client, &mut t, Instant::now()).status,
            "pushed"
        );
        assert_eq!(sync::sync_now(&mut owner).unwrap().status, "pulled");
        assert_eq!(
            trial_balance(&client, None).unwrap(),
            trial_balance(&owner, None).unwrap()
        );

        // A viewer of a relay company is read-only too, and rebase is refused politely.
        let view = create_invite(
            &remote,
            key.as_deref(),
            &company,
            "viewer",
            "Auditor",
            Some(7),
            &remote.url,
        )
        .unwrap();
        let mut auditor = None;
        join(&mut auditor, &temp_dir("relay-viewer"), &view.code).unwrap();
        assert!(
            api::post_voucher(&mut auditor, voucher("Payment", "2026-04-05", vec![]))
                .unwrap_err()
                .contains("read-only")
        );
        // Rebase works through the relay too, keeping the key.
        let report = sync::rebase(&mut owner, &f.0).unwrap();
        assert_eq!(report.pushed, 0);
        assert!(company_key(&owner.as_ref().unwrap().0).unwrap().is_some());
    }

    #[test]
    fn no_relay_is_built_in_and_a_company_can_move_to_another() {
        // Without a relay address, Connect explains the options.
        let f = TempFile::new("lan-move-owner");
        let (mut owner, ..) = demo(&f);
        if BUILD_DEFAULT_RELAY.is_empty() {
            assert_eq!(DeviceConfig::default().relay_address(), "");
            set_online_access(&owner, true, 2, "relay", "").unwrap();
            assert_eq!(online_access(&owner).unwrap().relay_url, "");
            let e = connect_relay(&mut owner, "", None).unwrap_err();
            assert!(e.contains("No relay address is set") && e.contains("deploy-relay"));
        }
        let cfg = DeviceConfig {
            relay_url: "https://relay.example.in".into(),
            ..DeviceConfig::default()
        };
        assert_eq!(
            with_device_relay(online_access(&owner).unwrap(), &cfg).relay_url,
            "https://relay.example.in"
        );

        let old = EmbeddedServer::start_with(&temp_dir("move-old"), loopback(), true).unwrap();
        let new_dir = temp_dir("move-new");
        let new = EmbeddedServer::start_with(&new_dir, loopback(), true).unwrap();
        // Only relay companies move.
        assert!(
            move_relay(&mut owner, &new.local_url(), None)
                .unwrap_err()
                .contains("only a company shared through a relay")
        );
        connect_relay(&mut owner, &old.local_url(), None).unwrap();
        let (remote, company, key) = owner_link_keyed(&owner).unwrap();
        let inv = create_invite(
            &remote,
            key.as_deref(),
            &company,
            "accountant",
            "CA",
            None,
            &remote.url,
        )
        .unwrap();
        let mut client = None;
        join(&mut client, &temp_dir("move-client"), &inv.code).unwrap();
        assert!(
            move_relay(&mut client, &new.local_url(), None)
                .unwrap_err()
                .contains("only the company's owner")
        );
        assert!(
            move_relay(&mut owner, "", None)
                .unwrap_err()
                .contains("new relay's address")
        );
        assert!(
            move_relay(&mut owner, &old.local_url(), None)
                .unwrap_err()
                .contains("already uses that relay")
        );

        let r = move_relay(&mut owner, &new.local_url(), None).unwrap();
        assert_eq!(
            (r.from.as_str(), r.to.as_str(), r.revoked),
            (old.local_url().as_str(), new.local_url().as_str(), Some(1))
        );
        let access = online_access(&owner).unwrap();
        assert_eq!(
            (access.url.as_str(), access.relay_url.as_str()),
            (r.to.as_str(), r.to.as_str())
        );
        assert!(access.encrypted && access.role == "owner");
        let (moved, _, new_key) = owner_link_keyed(&owner).unwrap();
        assert_ne!(new_key, key, "moved under a new key");
        assert_eq!(server_len(&moved, 0, true).unwrap(), r.entries);
        assert_eq!(sync::sync_now(&mut owner).unwrap().status, "up_to_date");

        // The colleague's old invite stops working; a new one works.
        assert!(sync::sync_now(&mut client).unwrap_err().contains("refused"));
        let inv = create_invite(
            &moved,
            new_key.as_deref(),
            &company,
            "accountant",
            "CA",
            None,
            &moved.url,
        )
        .unwrap();
        let mut rejoined = None;
        join(&mut rejoined, &temp_dir("move-rejoin"), &inv.code).unwrap();
        assert_eq!(
            trial_balance(&rejoined, None).unwrap(),
            trial_balance(&owner, None).unwrap()
        );
        let mut stored = Vec::new();
        for e in std::fs::read_dir(&new_dir).unwrap() {
            stored.extend(std::fs::read(e.unwrap().path()).unwrap_or_default());
        }
        assert!(!String::from_utf8_lossy(&stored).contains("HDFC Bank"));
    }

    #[test]
    fn background_sync_timing() {
        let remote = Remote {
            url: "http://192.168.1.20:9090".into(),
            company_id: String::new(),
            token: String::new(),
        };
        let p = Plan {
            remote,
            hosted: false,
            pending: 0,
            read_only: false,
            interval: Duration::from_secs(120),
            conflicted: false,
        };
        let now = Instant::now();
        let mut t = Ticker::default();
        assert!(due(&p, &t, now, false), "on opening");
        t.last_attempt = Some(now);
        assert!(!due(&p, &t, now + Duration::from_secs(5), false));
        assert!(
            due(
                &Plan {
                    pending: 1,
                    ..p.clone()
                },
                &t,
                now + Duration::from_secs(1),
                false
            ),
            "after a save"
        );
        assert!(!due(
            &Plan {
                pending: 1,
                read_only: true,
                ..p.clone()
            },
            &t,
            now + Duration::from_secs(1),
            false
        ));
        assert!(
            due(&p, &t, now + Duration::from_secs(120), false),
            "on the interval"
        );
        assert!(
            due(
                &Plan {
                    hosted: true,
                    ..p.clone()
                },
                &t,
                now + Duration::from_secs(3),
                false
            ),
            "hosted polls fast"
        );
        t.last_failed = true;
        assert!(
            !due(
                &Plan {
                    pending: 1,
                    ..p.clone()
                },
                &t,
                now + Duration::from_secs(5),
                false
            ),
            "back off after failure"
        );
        assert!(due(&p, &t, now + Duration::from_secs(30), false));
        assert!(
            !due(
                &Plan {
                    conflicted: true,
                    ..p.clone()
                },
                &Ticker::default(),
                now,
                false
            ),
            "waits for a rebase"
        );
        assert!(due(&p, &t, now, true), "Connect syncs at once");
    }
}
