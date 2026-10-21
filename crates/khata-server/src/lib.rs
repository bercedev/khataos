//! Self-hostable KhataOS sync server.
//!
//! The server is the sequencer for each company's audit trail. Devices push
//! the entries they appended on top of the trail they last saw; each entry
//! goes through [`khata_core::Book::apply_entry`] — the same validation as a
//! local edit — before it is stored, so a modified client cannot smuggle in
//! an unbalanced voucher, a dangling reference or a forged state change.
//!
//! Access is by bearer token, scoped to one company and one role:
//! - **owner**: everything, including issuing and revoking access;
//! - **accountant**: read and write (team members, the company's CA);
//! - **viewer**: read-only (external auditors, financial advisors), usually
//!   with an expiry date.
//!
//! Only SHA-256 hashes of tokens are stored. Every accepted push is logged
//! with the token that made it, so authorship does not rest on the
//! client-asserted `user` field alone.
//!
//! Beside the trail, each company has **shared records** (`records`):
//! notes, the invoice design and other non-accounting data, merged last
//! writer wins (`docs/sync-settings.md`). The same roles apply: owners and
//! accountants write them, viewers only read them.

mod auth;
mod csv;
mod error;
mod portal;
pub mod protocol;
mod records;
pub mod relay;
mod routes;
mod views;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use khata_core::Book;
use khata_store::Store;
use rusqlite::Connection;
use uuid::Uuid;

pub use auth::Role;
pub use error::ApiError;
pub use relay::RelayLimits;
pub use routes::router;

pub struct Config {
    /// Directory holding `server.db` and one `<company>.khata` per company.
    pub data_dir: PathBuf,
    /// Key allowed to create companies (from `KHATA_ADMIN_KEY`).
    pub admin_key: String,
}

/// An open company: its store and the verified books, kept in memory so
/// pushes validate only the new entries and reports need no replay.
pub(crate) struct Company {
    pub(crate) store: Store,
    pub(crate) book: Book,
}

pub struct AppState {
    data_dir: PathBuf,
    admin_key_hash: String,
    registry: Mutex<Connection>,
    companies: Mutex<HashMap<Uuid, Arc<Mutex<Company>>>>,
    /// Anyone may create an end-to-end encrypted relay company.
    pub(crate) open_relay: std::sync::atomic::AtomicBool,
    /// Abuse limits for the relay, and today's sign-up counts.
    pub(crate) relay: relay::Guard,
}

pub type SharedState = Arc<AppState>;

/// Lock a mutex, recovering from poisoning: a panic in one request must not
/// take the server down for everyone. Company state is only replaced after
/// validation succeeds, so a poisoned guard still holds consistent data.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AppState {
    pub fn open(config: Config) -> Result<SharedState, ApiError> {
        std::fs::create_dir_all(&config.data_dir).map_err(|e| ApiError::internal(e.to_string()))?;
        let registry = Connection::open(config.data_dir.join("server.db"))?;
        registry.execute_batch(auth::SCHEMA)?;
        relay::init(&registry)?;
        records::init(&registry)?;
        Ok(Arc::new(Self {
            data_dir: config.data_dir,
            admin_key_hash: auth::hash_token(&config.admin_key),
            registry: Mutex::new(registry),
            companies: Mutex::new(HashMap::new()),
            open_relay: std::sync::atomic::AtomicBool::new(false),
            relay: relay::Guard::default(),
        }))
    }

    /// Replace the relay's abuse limits (defaults: [`RelayLimits::default`]).
    pub fn set_relay_limits(&self, limits: RelayLimits) {
        *lock(&self.relay.limits) = limits;
    }

    /// Allow relay sign-up without the admin key (managed relay service).
    pub fn set_open_relay(&self, open: bool) {
        self.open_relay.store(open, std::sync::atomic::Ordering::Relaxed);
    }

    fn company_path(&self, id: Uuid) -> PathBuf {
        self.data_dir.join(format!("{id}.khata"))
    }

    /// The company's handle, opening and verifying it on first use. The
    /// global map lock is held only for the lookup; work happens under the
    /// company's own lock.
    fn company(&self, id: Uuid) -> Result<Arc<Mutex<Company>>, ApiError> {
        if let Some(c) = lock(&self.companies).get(&id) {
            return Ok(c.clone());
        }
        let path = self.company_path(id);
        if !path.exists() {
            return Err(ApiError::NotFound);
        }
        let store = Store::open(path)?;
        let book = store.load()?;
        let handle = Arc::new(Mutex::new(Company { store, book }));
        Ok(lock(&self.companies).entry(id).or_insert(handle).clone())
    }

    /// Run `f` with the company locked.
    fn with_company<T>(&self, id: Uuid, f: impl FnOnce(&mut Company) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let handle = self.company(id)?;
        let mut company = lock(&handle);
        f(&mut company)
    }
}
