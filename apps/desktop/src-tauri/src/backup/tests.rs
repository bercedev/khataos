use super::*;
use crate::api::tests::{entry, ledger, voucher};
use crate::api::{create_company, open_company, post_voucher, trial_balance};
use khata_core::DrCr;

/// A folder per test holding the company, app data, backups and work
/// files; removed at the end.
struct Sandbox {
    root: PathBuf,
    env: Env,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("khataos-desktop-backup-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let env = Env { app_data: root.join("app-data"), backups_root: root.join("KhataOS Backups"), scratch: root.join("work") };
        Self { root, env }
    }

    fn company(&self) -> PathBuf {
        self.root.join("companies").join("demo-traders.khata")
    }

    fn folder(&self) -> PathBuf {
        self.env.backups_root.join("Demo Traders")
    }

    fn files(&self, dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .map(|r| r.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Demo Traders with opening balances, a payment and a sale, plus a sync
/// token and a desk note.
fn demo(sb: &Sandbox) -> Mutex<Slot> {
    let mut slot = None;
    create_company(&mut slot, &sb.company(), "Demo Traders").unwrap();
    let bank = ledger(&mut slot, "HDFC Bank", "Bank Accounts", "250000");
    ledger(&mut slot, "Owner's Capital", "Capital Account", "-2,50,000.00");
    let rent = ledger(&mut slot, "Office Rent", "Indirect Expenses", "");
    post_voucher(&mut slot, voucher("Payment", "2026-04-01", vec![entry(&rent, DrCr::Dr, "25000"), entry(&bank, DrCr::Cr, "25000")])).unwrap();
    let (store, _) = slot.as_ref().unwrap();
    store.set_setting("sync.token", Some("secret-token-for-the-test")).unwrap();
    store.set_setting("share.key", Some("relay-key-for-the-test")).unwrap();
    store.set_record("desk.note.0001", Some("{\"title\":\"Call the CA\"}"), "asha").unwrap();
    Mutex::new(slot)
}

fn pay_rent(slot: &Mutex<Slot>, date: &str, amount: &str) {
    let mut s = slot.lock().unwrap();
    let ids = |name: &str| api::list_ledgers(&s, name, "all").unwrap().into_iter().find(|l| l.name == name).unwrap().id;
    let (rent, bank) = (ids("Office Rent"), ids("HDFC Bank"));
    post_voucher(&mut s, voucher("Payment", date, vec![entry(&rent, DrCr::Dr, amount), entry(&bank, DrCr::Cr, amount)])).unwrap();
}

fn entries(slot: &Mutex<Slot>) -> u64 {
    slot.lock().unwrap().as_ref().unwrap().1.audit().entries().len() as u64
}

#[test]
fn a_backup_of_the_open_company_equals_its_books() {
    let sb = Sandbox::new("equals");
    let slot = demo(&sb);
    let path = sb.company();
    let done = back_up(&slot, &sb.env, &path, Target::Folder, None).unwrap();

    // In the default folder, named <company>_<YYYY-MM-DD_HHMM>.khatabak.
    assert_eq!(done.folder, sb.folder().display().to_string());
    assert!(done.file.starts_with("demo-traders_") && done.file.ends_with(".khatabak") && !done.file.contains("_auto"), "{}", done.file);
    assert_eq!(done.size, std::fs::metadata(&done.path).unwrap().len());
    let s = slot.lock().unwrap();
    let book = &s.as_ref().unwrap().1;
    assert_eq!(done.summary.entries, book.audit().entries().len() as u64);
    assert_eq!(done.summary.last_hash.as_ref(), book.audit().entries().last().map(|e| &e.hash));
    assert_eq!((done.summary.vouchers, done.summary.records), (1, 1));

    // The backup's Trial Balance is the books'.
    let opened = khata_backup::open(Path::new(&done.path), None, &sb.env.scratch).unwrap();
    let mut restored = None;
    let copy = sb.root.join("check.khata");
    opened.restore_to(&copy).unwrap();
    open_company(&mut restored, &copy).unwrap();
    assert_eq!(trial_balance(&restored, None).unwrap(), trial_balance(&s, None).unwrap());
    let (rs, _) = restored.as_ref().unwrap();
    assert_eq!(rs.setting("sync.token").unwrap(), None, "no credentials in a backup");
    assert_eq!(rs.setting("share.key").unwrap(), None);
    assert_eq!(rs.record("desk.note.0001").unwrap().as_deref(), Some("{\"title\":\"Call the CA\"}"));
    let bytes = std::fs::read(&done.path).unwrap();
    assert!(!bytes.windows(12).any(|w| w == b"secret-token"));

    // Remembered on this computer as the last backup; nothing changed since.
    let st = status(&s, &sb.env, &path).unwrap();
    let last = st.last.unwrap();
    assert_eq!((last.path.as_str(), last.automatic, last.entries), (done.path.as_str(), false, done.summary.entries));
    assert!(!st.changed_since && !st.remind);
    assert!(sb.files(&sb.env.scratch).is_empty(), "work files are removed");
}

#[test]
fn a_sealed_backup_and_a_picked_file() {
    let sb = Sandbox::new("sealed");
    let slot = demo(&sb);
    let picked = sb.root.join("usb").join("for-the-ca.khatabak");
    std::fs::create_dir_all(picked.parent().unwrap()).unwrap();
    let shown = picked.display().to_string();
    assert!(back_up(&slot, &sb.env, &sb.company(), Target::File { local: &picked, shown: &shown }, Some("short")).is_err());
    let done = back_up(&slot, &sb.env, &sb.company(), Target::File { local: &picked, shown: &shown }, Some("correct horse battery")).unwrap();
    assert!(done.summary.encrypted);
    assert!(encrypted(&picked).unwrap());
    let err = inspect(&slot, &sb.env, &picked, &shown, None).unwrap_err();
    assert!(err.contains("password"), "{err}");
    let p = inspect(&slot, &sb.env, &picked, &shown, Some("correct horse battery")).unwrap();
    assert_eq!((p.relation, p.summary.company.as_str(), p.file.as_str()), (Relation::Same, "Demo Traders", "for-the-ca.khatabak"));
}

#[test]
fn restore_as_a_new_company_and_replace_keeping_the_old_file() {
    let sb = Sandbox::new("restore");
    let slot = demo(&sb);
    let path = sb.company();
    let backup = PathBuf::from(back_up(&slot, &sb.env, &path, Target::Folder, None).unwrap().path);
    let at_backup = entries(&slot);
    pay_rent(&slot, "2026-05-01", "25000");
    let later = entries(&slot);

    // The open books are newer: the preview says how many entries the backup lacks.
    let p = inspect(&slot, &sb.env, &backup, "b.khatabak", None).unwrap();
    assert_eq!((p.relation, p.later_entries, p.can_replace), (Relation::Older, later - at_backup, true));
    assert_eq!(p.summary.last_voucher, chrono::NaiveDate::from_ymd_opt(2026, 4, 1));

    // As a new company file: beside the others, nothing overwritten.
    let dir = sb.root.join("companies");
    let made = restore_new(&sb.env, &dir, &backup, None).unwrap();
    assert_eq!(made.file_name().unwrap(), "demo-traders-restored.khata");
    assert_eq!(restore_new(&sb.env, &dir, &backup, None).unwrap().file_name().unwrap(), "demo-traders-restored-2.khata");
    let mut fresh = None;
    open_company(&mut fresh, &made).unwrap();
    assert_eq!(fresh.as_ref().unwrap().1.audit().entries().len() as u64, at_backup);
    assert_eq!(entries(&slot), later, "the open company is untouched");

    // Replace: the name must be typed.
    let mut held = Some(CompanyLock::acquire(&path).unwrap());
    let mut s = slot.lock().unwrap();
    let e = restore_replace(&mut s, &mut held, &sb.env, &path, &backup, None, "Demo").unwrap_err();
    assert!(e.contains("type the company name"), "{e}");
    assert_eq!(s.as_ref().unwrap().1.audit().entries().len() as u64, later);

    let r = restore_replace(&mut s, &mut held, &sb.env, &path, &backup, None, " demo traders ").unwrap();
    // The books are the backup's now, reopened in this window under its lock.
    assert_eq!(s.as_ref().unwrap().1.audit().entries().len() as u64, at_backup);
    assert_eq!(r.info.voucher_count, 1);
    assert!(held.is_some());
    // The replaced file is kept, unchanged, beside it.
    let kept = PathBuf::from(&r.kept);
    let name = kept.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("demo-traders.before-restore-") && name.ends_with(".khata"), "{name}");
    assert_eq!(Store::open(&kept).unwrap().len().unwrap(), later);
    assert_eq!(Store::open(&kept).unwrap().setting("sync.token").unwrap().as_deref(), Some("secret-token-for-the-test"));
    // And an automatic backup of it was taken first.
    let safety = PathBuf::from(r.safety_backup.unwrap());
    assert!(safety.file_name().unwrap().to_string_lossy().ends_with("_auto.khatabak"));
    assert_eq!(khata_backup::open(&safety, None, &sb.env.scratch).unwrap().summary.entries, later);
}

#[test]
fn replace_is_refused_while_another_window_holds_the_company() {
    let sb = Sandbox::new("locked");
    let slot = demo(&sb);
    let path = sb.company();
    let backup = PathBuf::from(back_up(&slot, &sb.env, &path, Target::Folder, None).unwrap().path);
    pay_rent(&slot, "2026-05-01", "100");
    let before = std::fs::read(&path).unwrap();
    // Another KhataOS has the company open; this window holds no lock.
    let other = CompanyLock::acquire(&path).unwrap();
    let mut held = None;
    let mut s = slot.lock().unwrap();
    let e = restore_replace(&mut s, &mut held, &sb.env, &path, &backup, None, "Demo Traders").unwrap_err();
    assert_eq!(e, api::COMPANY_IN_USE);
    assert_eq!(std::fs::read(&path).unwrap(), before, "nothing changed");
    assert!(!sb.files(path.parent().unwrap()).iter().any(|f| f.contains("before-restore")));
    drop(other);
}

#[test]
fn replace_refuses_a_backup_of_another_company() {
    let sb = Sandbox::new("other");
    let slot = demo(&sb);
    let mut other_slot = None;
    let other_path = sb.root.join("companies").join("other.khata");
    create_company(&mut other_slot, &other_path, "Other Traders").unwrap();
    ledger(&mut other_slot, "Cash Box", "Cash-in-Hand", "10");
    let other = Mutex::new(other_slot);
    let backup = PathBuf::from(back_up(&other, &sb.env, &other_path, Target::Folder, None).unwrap().path);
    let p = inspect(&slot, &sb.env, &backup, "x", None).unwrap();
    assert_eq!((p.relation, p.can_replace), (Relation::OtherCompany, false));
    let mut held = Some(CompanyLock::acquire(&sb.company()).unwrap());
    let e = restore_replace(&mut slot.lock().unwrap(), &mut held, &sb.env, &sb.company(), &backup, None, "Demo Traders").unwrap_err();
    assert!(e.contains("new company file"), "{e}");
}

#[test]
fn automatic_backups_on_close_and_daily_rotate_to_keep_n() {
    let sb = Sandbox::new("auto");
    let slot = demo(&sb);
    let path = sb.company();
    let s = slot.lock().unwrap();
    // Keep 2; an unrelated file in the folder must survive rotation.
    set_prefs(&s, &sb.env, &path, Prefs { keep: 2, ..Prefs::default() }).unwrap();
    assert!(set_prefs(&s, &sb.env, &path, Prefs { keep: 0, ..Prefs::default() }).is_err());
    assert!(set_prefs(&s, &sb.env, &path, Prefs { folder: Some("relative/folder".into()), ..Prefs::default() }).is_err());
    std::fs::create_dir_all(sb.folder()).unwrap();
    std::fs::write(sb.folder().join("my notes.txt"), "keep").unwrap();
    std::fs::write(sb.folder().join("demo-traders_2020-01-01_0000.khatabak"), "a manual backup").unwrap();
    drop(s);

    let job = due_on_close(&slot.lock().unwrap(), &sb.env, &path).expect("due: never backed up");
    job.run().unwrap();
    assert!(due_on_close(&slot.lock().unwrap(), &sb.env, &path).is_none(), "nothing changed since");
    // The daily one waits a day, and for a change.
    let now = Utc::now();
    assert!(daily(&slot, &sb.env, &path, now + Duration::hours(25)).unwrap().is_none(), "unchanged");
    pay_rent(&slot, "2026-05-01", "1");
    assert!(daily(&slot, &sb.env, &path, now + Duration::hours(1)).unwrap().is_none(), "too soon");
    let d = daily(&slot, &sb.env, &path, now + Duration::hours(25)).unwrap().expect("due");
    assert!(d.file.ends_with("_auto.khatabak"));
    // Two more changes and closes: only the newest two automatic backups stay.
    for amount in ["2", "3"] {
        pay_rent(&slot, "2026-05-02", amount);
        due_on_close(&slot.lock().unwrap(), &sb.env, &path).unwrap().run().unwrap();
    }
    let files = sb.files(&sb.folder());
    let autos: Vec<&String> = files.iter().filter(|f| f.ends_with("_auto.khatabak")).collect();
    assert_eq!(autos.len(), 2, "{files:?}");
    assert!(files.contains(&"my notes.txt".to_owned()) && files.contains(&"demo-traders_2020-01-01_0000.khatabak".to_owned()), "{files:?}");
    // The newest has every entry.
    let newest = autos.iter().max_by_key(|f| khata_backup::parse_automatic(f, "demo-traders")).unwrap();
    let newest = khata_backup::open(&sb.folder().join(newest), None, &sb.env.scratch).unwrap();
    assert_eq!(newest.summary.entries, entries(&slot));

    // Switched off: nothing on close.
    pay_rent(&slot, "2026-05-03", "4");
    set_prefs(&slot.lock().unwrap(), &sb.env, &path, Prefs { automatic: false, ..Prefs::default() }).unwrap();
    assert!(due_on_close(&slot.lock().unwrap(), &sb.env, &path).is_none());
}

#[test]
fn an_automatic_backup_is_taken_before_a_tally_import_into_a_company_with_books() {
    let sb = Sandbox::new("tally");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../crates/khata-import-tally/tests/fixtures/masters.xml");
    let files = vec![fixture.display().to_string()];

    // A new, empty company: nothing to lose, no backup.
    let mut empty = None;
    let empty_path = sb.root.join("companies").join("fresh.khata");
    create_company(&mut empty, &empty_path, "Fresh Co").unwrap();
    let (_, safety) = import_tally(&mut empty, &sb.env, Some(&empty_path), &files).unwrap();
    assert!(safety.is_none());

    // A company with books: backed up first, with every entry it had.
    let slot = demo(&sb);
    let before = entries(&slot);
    let (summary, safety) = import_tally(&mut slot.lock().unwrap(), &sb.env, Some(&sb.company()), &files).unwrap();
    let safety = safety.expect("a backup first");
    assert!(safety.file.ends_with("_auto.khatabak"));
    assert_eq!(safety.summary.entries, before);
    assert!(summary.ledgers_created > 0 && entries(&slot) > before);
    let st = status(&slot.lock().unwrap(), &sb.env, &sb.company()).unwrap();
    assert_eq!(st.last.unwrap().reason, "Tally import");

    // When the backup can't be made, the import doesn't start.
    let blocked = Env { backups_root: sb.root.join("a-file"), ..sb.env.clone() };
    std::fs::write(sb.root.join("a-file"), "not a folder").unwrap();
    let at = entries(&slot);
    let e = import_tally(&mut slot.lock().unwrap(), &blocked, Some(&sb.company()), &files).unwrap_err();
    assert!(e.contains("nothing was changed"), "{e}");
    assert_eq!(entries(&slot), at);
}

#[test]
fn the_desk_reminds_after_a_week_without_a_backup() {
    let sb = Sandbox::new("remind");
    let slot = demo(&sb);
    let path = sb.company();
    let now = Utc::now();
    let r = reminder(&slot.lock().unwrap(), &sb.env, &path, now).expect("never backed up");
    assert_eq!((r.last, r.days), (None, None));
    back_up(&slot, &sb.env, &path, Target::Folder, None).unwrap();
    let s = slot.lock().unwrap();
    assert!(reminder(&s, &sb.env, &path, now + Duration::days(30)).is_none(), "nothing changed since");
    drop(s);
    pay_rent(&slot, "2026-05-01", "1");
    let s = slot.lock().unwrap();
    assert!(reminder(&s, &sb.env, &path, now + Duration::days(6)).is_none(), "within the week");
    let r = reminder(&s, &sb.env, &path, now + Duration::days(8) + Duration::hours(1)).unwrap();
    assert_eq!(r.days, Some(8));
    // The desk itself carries no reminder until the command fills it in.
    assert!(crate::desk::overview(&s, "2026-05-01", "2026-05-31").unwrap().backup.is_none());
}

#[test]
fn names_of_folders_and_kept_files() {
    assert_eq!(folder_name("Sharma & Sons Pvt. Ltd."), "Sharma & Sons Pvt. Ltd");
    assert_eq!(folder_name("A/B: C?"), "A B C");
    assert_eq!(folder_name("  ..  "), "Company");
    let at = chrono::NaiveDate::from_ymd_opt(2026, 10, 21).unwrap().and_hms_opt(14, 30, 5).unwrap();
    let dir = std::env::temp_dir().join(format!("khataos-kept-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = kept_path(&dir.join("sharma-sons.khata"), at);
    assert_eq!(p.file_name().unwrap(), "sharma-sons.before-restore-2026-10-21_143005.khata");
    std::fs::write(&p, "x").unwrap();
    assert_eq!(kept_path(&dir.join("sharma-sons.khata"), at).file_name().unwrap(), "sharma-sons.before-restore-2026-10-21_143005-2.khata");
    let _ = std::fs::remove_dir_all(&dir);
}
