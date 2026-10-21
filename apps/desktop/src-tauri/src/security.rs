//! Company users and roles on a shared computer (Tally's "security
//! control"): who may open the company and what they may change.
//!
//! Users and their roles live in the company profile, so they sync and every
//! change is in the audit trail; passwords are kept only as Argon2id hashes.
//! A company with no users needs no sign-in and its user has every right.
//! Rights are checked here, in the command layer, before a change reaches
//! the books; the audit trail records who made each change.

#[cfg(not(test))]
use std::sync::Mutex;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use khata_core::{Book, CompanyUser, UserRole};
use serde::Serialize;

use crate::api::{ApiResult, Slot, err, now, open_mut_unchecked, open_ref_unchecked};

/// Who is signed in, and to which company (its first audit hash).
struct Session {
    company: String,
    name: String,
    role: UserRole,
}

#[cfg(not(test))]
static SESSION: Mutex<Option<Session>> = Mutex::new(None);

/// Run `f` on the session. Commands run on several threads, so the app
/// shares one session; tests run in parallel, so each test thread has its own.
fn with_session<R>(f: impl FnOnce(&mut Option<Session>) -> R) -> R {
    #[cfg(not(test))]
    {
        f(&mut SESSION.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }
    #[cfg(test)]
    {
        thread_local!(static SESSION: std::cell::RefCell<Option<Session>> = const { std::cell::RefCell::new(None) });
        SESSION.with(|s| f(&mut s.borrow_mut()))
    }
}

/// A stable id for a company: the hash of its first audit entry.
fn company_key(book: &Book) -> String {
    book.audit().entries().first().map(|e| e.hash.clone()).unwrap_or_default()
}

/// The signed-in user of this company, if any.
fn current(book: &Book) -> Option<(String, UserRole)> {
    let key = company_key(book);
    with_session(|s| s.as_ref().filter(|s| s.company == key).map(|s| (s.name.clone(), s.role)))
}

/// Forget the session (another company opened, or the company closed).
pub fn sign_out() {
    with_session(|s| *s = None);
}

/// The name to record in the audit trail: the signed-in user, if any.
pub fn signed_in_name() -> Option<String> {
    with_session(|s| s.as_ref().map(|s| s.name.clone()))
}

/// Allow an action needing `need`, or say why not.
pub fn require(book: &Book, need: UserRole) -> ApiResult<()> {
    if book.users().is_empty() {
        return Ok(());
    }
    let Some((name, role)) = current(book) else {
        return Err("sign in to this company first".into());
    };
    if role < need {
        return Err(format!("{name} is signed in as {}, which can't do this; it needs {}", label(role), label(need)));
    }
    Ok(())
}

pub fn label(role: UserRole) -> &'static str {
    match role {
        UserRole::Auditor => "Auditor (view only)",
        UserRole::DataEntry => "Data Entry",
        UserRole::Accountant => "Accountant",
        UserRole::Owner => "Owner",
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionDto {
    /// The company has users, so someone must sign in.
    pub required: bool,
    pub user: Option<String>,
    pub role: Option<UserRole>,
}

pub fn session_info(slot: &Slot) -> ApiResult<SessionDto> {
    let book = open_ref_unchecked(slot)?;
    let cur = current(book);
    Ok(SessionDto { required: !book.users().is_empty(), user: cur.as_ref().map(|c| c.0.clone()), role: cur.map(|c| c.1) })
}

pub fn sign_in(slot: &Slot, name: &str, password: &str) -> ApiResult<SessionDto> {
    let book = open_ref_unchecked(slot)?;
    // The same message for an unknown name and a wrong password.
    let refused = || "that user name and password don't match".to_owned();
    let u = book.users().iter().find(|u| u.name.eq_ignore_ascii_case(name.trim())).ok_or_else(refused)?;
    if u.disabled {
        return Err(format!("{} has been switched off by the company's owner", u.name));
    }
    let hash = PasswordHash::new(&u.password_hash).map_err(|_| refused())?;
    Argon2::default().verify_password(password.as_bytes(), &hash).map_err(|_| refused())?;
    let signed = Session { company: company_key(book), name: u.name.clone(), role: u.role };
    with_session(|s| *s = Some(signed));
    session_info(slot)
}

fn hash_password(password: &str) -> ApiResult<String> {
    if password.chars().count() < 8 {
        return Err("passwords need at least 8 characters".into());
    }
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|_| "the system random number generator failed".to_owned())?;
    let salt = SaltString::encode_b64(&salt).map_err(err)?;
    Ok(Argon2::default().hash_password(password.as_bytes(), &salt).map_err(err)?.to_string())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UserDto {
    pub name: String,
    pub role: UserRole,
    pub disabled: bool,
}

pub fn list_users(slot: &Slot) -> ApiResult<Vec<UserDto>> {
    let book = open_ref_unchecked(slot)?;
    require(book, UserRole::Owner)?;
    Ok(book.users().iter().map(|u| UserDto { name: u.name.clone(), role: u.role, disabled: u.disabled }).collect())
}

/// Add or change a user (Owner only; anyone when the company has no users
/// yet — the first user must be an Owner). An empty password keeps the
/// existing one. Creating the first user signs them in.
pub fn save_user(slot: &mut Slot, name: &str, role: UserRole, password: &str, disabled: bool) -> ApiResult<Vec<UserDto>> {
    let (store, book) = open_mut_unchecked(slot)?;
    require(book, UserRole::Owner)?;
    let name = name.trim();
    let first = book.users().is_empty();
    let existing = book.users().iter().find(|u| u.name.eq_ignore_ascii_case(name)).cloned();
    let password_hash = match (password.is_empty(), existing) {
        (false, _) => hash_password(password)?,
        (true, Some(e)) => e.password_hash,
        (true, None) => return Err("give the new user a password".into()),
    };
    if let Some((me, _)) = current(book) {
        if me.eq_ignore_ascii_case(name) && (role != UserRole::Owner || disabled) {
            return Err("you can't lower or switch off your own access; ask another Owner".into());
        }
    }
    let actor = crate::api::os_user();
    book.save_user(now(), &actor, CompanyUser { name: name.to_owned(), role, password_hash, disabled }).map_err(err)?;
    store.save(book).map_err(err)?;
    if first {
        let signed = Session { company: company_key(book), name: name.to_owned(), role };
        with_session(|s| *s = Some(signed));
    }
    list_users(slot)
}

pub fn remove_user(slot: &mut Slot, name: &str) -> ApiResult<Vec<UserDto>> {
    let (store, book) = open_mut_unchecked(slot)?;
    require(book, UserRole::Owner)?;
    if current(book).is_some_and(|(me, _)| me.eq_ignore_ascii_case(name.trim())) && book.users().len() > 1 {
        return Err("you can't remove yourself; ask another Owner".into());
    }
    book.remove_user(now(), &crate::api::os_user(), name.trim()).map_err(err)?;
    store.save(book).map_err(err)?;
    if book.users().is_empty() {
        sign_out();
        return Ok(Vec::new());
    }
    list_users(slot)
}

#[cfg(test)]
mod tests {
    use khata_core::DrCr;

    use super::*;
    use crate::api::tests::{TempFile, demo, entry, voucher};
    use crate::api::{create_ledger, delete_voucher, list_ledgers, open_company, post_voucher, set_country};

    #[test]
    fn users_sign_in_and_roles_limit_changes() {
        let f = TempFile::new("security");
        let (mut slot, bank, _, rent, ..) = demo(&f);
        set_country(&mut slot, Some(&f.0), "IN").unwrap();
        // No users: everyone may do everything, no sign-in.
        assert!(!session_info(&slot).unwrap().required);

        // The first user must be an Owner, and is signed in.
        assert!(save_user(&mut slot, "Clerk", UserRole::DataEntry, "password1", false).is_err());
        assert!(save_user(&mut slot, "Meera", UserRole::Owner, "short", false).is_err(), "8 characters at least");
        save_user(&mut slot, "Meera", UserRole::Owner, "owner-pass-1", false).unwrap();
        assert_eq!(session_info(&slot).unwrap().user.as_deref(), Some("Meera"));
        save_user(&mut slot, "Ravi", UserRole::DataEntry, "clerk-pass-1", false).unwrap();
        assert!(save_user(&mut slot, "Meera", UserRole::Accountant, "", false).is_err(), "can't demote yourself");
        let hashes = serde_json::to_string(&slot.as_ref().unwrap().1.users()).unwrap();
        assert!(!hashes.contains("clerk-pass-1"), "only hashes are kept");

        // Reopening the company forgets who was signed in.
        drop(slot);
        let mut slot = None;
        open_company(&mut slot, &f.0).unwrap();
        assert!(session_info(&slot).unwrap().required);
        assert!(list_ledgers(&slot, "", "all").unwrap_err().contains("sign in"));
        assert_eq!(sign_in(&slot, "ravi", "wrong-pass").unwrap_err(), "that user name and password don't match");
        assert_eq!(sign_in(&slot, "nobody", "x").unwrap_err(), "that user name and password don't match");

        // Data Entry posts, but can't delete or manage users.
        sign_in(&slot, "ravi", "clerk-pass-1").unwrap();
        let v = post_voucher(&mut slot, voucher("Payment", "2026-04-09", vec![entry(&rent, DrCr::Dr, "10"), entry(&bank, DrCr::Cr, "10")])).unwrap();
        let book = &slot.as_ref().unwrap().1;
        assert_eq!(book.audit().entries().last().unwrap().user, "Ravi", "the trail names the signed-in user");
        assert!(delete_voucher(&mut slot, &v.id).unwrap_err().contains("Accountant"));
        assert!(list_users(&slot).is_err());
        let group = crate::api::tests::group_id(&slot, "Indirect Expenses");
        create_ledger(&mut slot, "Tea", &group, "").unwrap();

        // The Owner can; switching Ravi off stops his sign-in.
        sign_in(&slot, "Meera", "owner-pass-1").unwrap();
        delete_voucher(&mut slot, &v.id).unwrap();
        save_user(&mut slot, "Ravi", UserRole::DataEntry, "", true).unwrap();
        assert!(sign_in(&slot, "Ravi", "clerk-pass-1").unwrap_err().contains("switched off"));
        remove_user(&mut slot, "Ravi").unwrap();
        assert_eq!(list_users(&slot).unwrap().len(), 1);
        sign_out();
    }
}
