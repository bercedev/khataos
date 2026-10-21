use super::*;
use chrono::{NaiveDate, Timelike};
use khata_core::{Entry, Voucher, VoucherId, VoucherKind};
use rust_decimal_macros::dec;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_760_000_000, 0).unwrap()
}

/// A fresh folder per test, removed at the end.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("khataos-backup-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn sample() -> Book {
    let mut b = Book::new("Demo Traders");
    let bank = b.create_ledger(now(), "u", "HDFC Bank", b.group_by_name("Bank Accounts").unwrap().id).unwrap();
    let rent = b.create_ledger(now(), "u", "Rent", b.group_by_name("Indirect Expenses").unwrap().id).unwrap();
    b.set_opening_balance(now(), "u", bank, dec!(1000)).unwrap();
    for (day, amount, narration) in [(1, dec!(100), "April rent"), (30, dec!(250.50), "May advance")] {
        b.post_voucher(
            now(),
            "u",
            Voucher {
                id: VoucherId::new(),
                kind: VoucherKind::Payment,
                number: day.to_string(),
                date: NaiveDate::from_ymd_opt(2026, 4, day).unwrap(),
                entries: vec![Entry::dr(rent, amount), Entry::cr(bank, amount)],
                narration: narration.into(),
                cancelled: false,
                stock: Vec::new(),
            },
        )
        .unwrap();
    }
    b
}

const TOKEN: &str = "tok-0123456789-very-secret";
const RELAY_KEY: &str = "relaykey-abcdef0123456789-secret";

/// A company file with books, shared records and every kind of
/// device-local data: credentials, cursors, a device chain.
fn company(dir: &Dir) -> (PathBuf, Store, Book) {
    let path = dir.0.join("demo-traders.khata");
    let book = sample();
    let mut store = Store::create(&path, &book.name).unwrap();
    store.save(&book).unwrap();
    store.set_record("desk.note.0001", Some("{\"title\":\"Call the CA\"}"), "asha").unwrap();
    store.set_record("invoice.design", Some("{\"template\":\"classic\"}"), "asha").unwrap();
    for (k, v) in [
        ("sync.token", TOKEN),
        ("sync.url", "https://books.example.in"),
        ("share.key", RELAY_KEY),
        ("share.relay_url", "https://relay.example.in"),
        ("package.sent_to", "4"),
        ("zatca.chain", "{\"counter\":7}"),
        ("verifactu.chain", "{\"last\":\"abc\"}"),
        ("tds.file_sequence", "3"),
    ] {
        store.set_setting(k, Some(v)).unwrap();
    }
    store.set_sync_state("https://books.example.in", "c1", 4).unwrap();
    store
        .set_record_cursor("https://books.example.in", &khata_store::RecordCursor { company_id: "c1".into(), pulled: 1, pushed: 1, supported: Some(true) })
        .unwrap();
    (path, store, book)
}

fn opts<'a>(scratch: &'a Path, password: Option<&'a str>, automatic: bool) -> Options<'a> {
    Options { automatic, password, app_version: "0.1.0-test", now: now(), scratch }
}

#[test]
fn a_backup_of_an_open_company_equals_the_books() {
    let dir = Dir::new("equals");
    let (_path, store, book) = company(&dir);
    let scratch = dir.0.join("scratch");
    let dest = dir.0.join("out").join("demo-traders_2026-10-21_1430.khatabak");
    let w = write(&store, &dest, &opts(&scratch, None, false)).unwrap();
    assert_eq!(w.path, dest);
    assert!(dest.is_file() && !partial_path(&dest).exists());
    assert_eq!(w.size, fs::metadata(&dest).unwrap().len());
    let s = &w.summary;
    assert_eq!(s.entries, book.audit().entries().len() as u64);
    assert_eq!(s.last_hash.as_ref(), book.audit().entries().last().map(|e| &e.hash));
    assert_eq!(s.company_id.as_ref(), book.audit().entries().first().map(|e| &e.hash));
    assert_eq!((s.vouchers, s.records), (2, 2));
    assert_eq!(s.first_voucher, NaiveDate::from_ymd_opt(2026, 4, 1));
    assert_eq!(s.last_voucher, NaiveDate::from_ymd_opt(2026, 4, 30));
    assert_eq!((s.company.as_str(), s.automatic, s.encrypted, s.made_at), ("Demo Traders", false, false, now()));

    // Read back: the same trail, the same report totals, the same records.
    let opened = open(&dest, None, &scratch).unwrap();
    assert_eq!(opened.summary, w.summary);
    assert_eq!(opened.book.trial_balance(), book.trial_balance());
    assert_eq!(opened.book.audit().entries(), book.audit().entries());
    let ro = Store::open_read_only(&dest).unwrap();
    assert_eq!(ro.all_records().unwrap(), store.all_records().unwrap());
    // Work files are cleaned up; the company file is unchanged and still open.
    assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
    assert_eq!(store.setting("sync.token").unwrap().as_deref(), Some(TOKEN));
}

#[test]
fn a_backup_contains_no_device_local_or_credential_keys() {
    let dir = Dir::new("secrets");
    let (_path, store, _book) = company(&dir);
    let scratch = dir.0.join("scratch");
    assert!(store.device_local_rows().unwrap() > 0, "the live file has them");
    for password in [None, Some("correct horse battery")] {
        let dest = dir.0.join(format!("b-{}.khatabak", password.is_some()));
        write(&store, &dest, &opts(&scratch, password, false)).unwrap();
        // Not even in free pages of the file.
        let bytes = fs::read(&dest).unwrap();
        for secret in [TOKEN, RELAY_KEY, "books.example.in", "relay.example.in", "zatca", "verifactu"] {
            assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()), "{secret} in the backup file");
        }
        let opened = open(&dest, password, &scratch).unwrap();
        let ro = Store::open_read_only(&opened.plain).unwrap();
        assert_eq!(ro.device_local_rows().unwrap(), 0);
        assert!(ro.settings().unwrap().is_empty());
        assert!(ro.all_records().unwrap().iter().all(|r| !khata_store::records::is_device_local(&r.key)));
        let plain = fs::read(&opened.plain).unwrap();
        assert!(!plain.windows(TOKEN.len()).any(|w| w == TOKEN.as_bytes()));
        // A restored copy has no sync settings either: it connects again.
        let restored = dir.0.join(format!("restored-{}.khata", password.is_some()));
        opened.restore_to(&restored).unwrap();
        let r = Store::open(&restored).unwrap();
        assert_eq!(r.setting("sync.token").unwrap(), None);
        assert_eq!(r.setting("share.key").unwrap(), None);
        assert_eq!(r.sync_state("https://books.example.in").unwrap(), None);
    }
}

#[test]
fn a_sealed_backup_needs_its_password_and_shows_nothing_without_it() {
    let dir = Dir::new("sealed");
    let (_path, store, book) = company(&dir);
    let scratch = dir.0.join("scratch");
    let dest = dir.0.join("demo_auto.khatabak");
    assert!(matches!(write(&store, &dest, &opts(&scratch, Some("short"), false)), Err(BackupError::WeakPassword)));
    let w = write(&store, &dest, &opts(&scratch, Some("correct horse battery"), true)).unwrap();
    assert!(w.summary.encrypted && w.summary.automatic);
    assert_eq!(kind(&dest).unwrap(), Kind::Sealed);
    let bytes = fs::read(&dest).unwrap();
    for visible in ["Demo Traders", "HDFC Bank", "SQLite format 3"] {
        assert!(!bytes.windows(visible.len()).any(|w| w == visible.as_bytes()), "{visible} readable");
    }
    assert!(matches!(open(&dest, None, &scratch), Err(BackupError::NeedsPassword)));
    assert!(matches!(open(&dest, Some("wrong horse battery"), &scratch), Err(BackupError::WrongPassword)));
    let opened = open(&dest, Some("correct horse battery"), &scratch).unwrap();
    assert_eq!(opened.book.trial_balance(), book.trial_balance());
    drop(opened);
    assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0, "the decrypted copy is deleted");
}

#[test]
fn a_corrupted_backup_is_refused_with_a_clear_message() {
    let dir = Dir::new("corrupt");
    let (_path, store, _book) = company(&dir);
    let scratch = dir.0.join("scratch");
    let good = dir.0.join("good.khatabak");
    write(&store, &good, &opts(&scratch, None, false)).unwrap();
    let bytes = fs::read(&good).unwrap();
    let flip = |at: usize, name: &str| {
        let mut b = bytes.clone();
        b[at] ^= 0x01;
        let p = dir.0.join(name);
        fs::write(&p, b).unwrap();
        p
    };
    let find = |s: &str| bytes.windows(s.len()).position(|w| w == s.as_bytes()).unwrap();

    // One flipped bit in a voucher's narration: the hash chain catches it.
    let p = flip(find("April rent"), "narration.khatabak");
    let e = open(&p, None, &scratch).unwrap_err();
    assert!(matches!(e, BackupError::Damaged(_)), "{e}");
    assert!(e.to_string().contains("audit trail"), "{e}");
    // In a note: the digest catches it.
    let p = flip(find("Call the CA"), "note.khatabak");
    let e = open(&p, None, &scratch).unwrap_err();
    assert!(e.to_string().contains("notes and settings"), "{e}");
    // In the file header: not a backup at all.
    let p = flip(3, "magic.khatabak");
    assert!(matches!(open(&p, None, &scratch), Err(BackupError::NotABackup)));

    // A sealed backup: any flipped byte, or a cut, is refused.
    let sealed = dir.0.join("sealed.khatabak");
    write(&store, &sealed, &opts(&scratch, Some("correct horse battery"), false)).unwrap();
    let sb = fs::read(&sealed).unwrap();
    for (at, name) in [(sb.len() / 2, "s-mid.khatabak"), (sb.len() - 1, "s-end.khatabak")] {
        let mut b = sb.clone();
        b[at] ^= 0x80;
        let p = dir.0.join(name);
        fs::write(&p, b).unwrap();
        let e = open(&p, Some("correct horse battery"), &scratch).unwrap_err();
        assert!(matches!(e, BackupError::Damaged(_) | BackupError::WrongPassword), "{e}");
    }
    let p = dir.0.join("s-cut.khatabak");
    fs::write(&p, &sb[..sb.len() - 10]).unwrap();
    let e = open(&p, Some("correct horse battery"), &scratch).unwrap_err();
    assert!(e.to_string().contains("cut short") || e.to_string().contains("changed or damaged"), "{e}");
    // Nothing half-made was left behind.
    assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0);
}

#[test]
fn a_company_file_or_another_file_is_not_a_backup() {
    let dir = Dir::new("notbackup");
    let (path, store, _book) = company(&dir);
    drop(store);
    let scratch = dir.0.join("scratch");
    assert!(matches!(open(&path, None, &scratch), Err(BackupError::CompanyFile)));
    let txt = dir.0.join("notes.khatabak");
    fs::write(&txt, "hello, not a backup at all").unwrap();
    assert!(matches!(open(&txt, None, &scratch), Err(BackupError::NotABackup)));
    fs::write(&txt, "short").unwrap();
    assert!(matches!(kind(&txt), Err(BackupError::NotABackup)));
}

#[test]
fn a_restored_backup_is_an_ordinary_company_file() {
    let dir = Dir::new("restore");
    let (_path, store, book) = company(&dir);
    let scratch = dir.0.join("scratch");
    let dest = dir.0.join("b.khatabak");
    write(&store, &dest, &opts(&scratch, None, false)).unwrap();
    let opened = open(&dest, None, &scratch).unwrap();
    let target = dir.0.join("companies").join("demo-traders-restored.khata");
    opened.restore_to(&target).unwrap();
    assert!(matches!(opened.restore_to(&target), Err(BackupError::Io(_))), "never overwrites");
    let r = Store::open(&target).unwrap();
    assert!(r.backup_info().unwrap().is_empty());
    let loaded = r.load().unwrap();
    assert_eq!(loaded.trial_balance(), book.trial_balance());
    assert_eq!(r.record("desk.note.0001").unwrap().as_deref(), Some("{\"title\":\"Call the CA\"}"));
    // The backup itself is unchanged, and still verifies.
    assert!(open(&dest, None, &scratch).is_ok());
}

#[test]
fn a_backup_can_be_taken_from_a_read_only_connection() {
    // On close the file is reopened read-only for the automatic backup.
    let dir = Dir::new("readonly");
    let (path, store, book) = company(&dir);
    drop(store);
    let ro = Store::open_read_only(&path).unwrap();
    let scratch = dir.0.join("scratch");
    let w = write(&ro, &dir.0.join("ro.khatabak"), &opts(&scratch, None, true)).unwrap();
    assert_eq!(w.summary.entries, book.audit().entries().len() as u64);
}

#[test]
fn names_and_slugs() {
    let at = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap().and_hms_opt(14, 30, 59).unwrap();
    assert_eq!(slug("Sharma & Sons"), "sharma-sons");
    assert_eq!(slug("  "), "company");
    assert_eq!(file_name("sharma-sons", at, false, 1), "sharma-sons_2026-10-21_1430.khatabak");
    assert_eq!(file_name("sharma-sons", at, true, 1), "sharma-sons_2026-10-21_1430_auto.khatabak");
    assert_eq!(file_name("sharma-sons", at, true, 3), "sharma-sons_2026-10-21_1430-3_auto.khatabak");
    let minute = at.with_second(0).unwrap();
    assert_eq!(parse_automatic("sharma-sons_2026-10-21_1430_auto.khatabak", "sharma-sons"), Some((minute, 1)));
    assert_eq!(parse_automatic("sharma-sons_2026-10-21_1430-3_auto.khatabak", "sharma-sons"), Some((minute, 3)));
    for not in [
        "sharma-sons_2026-10-21_1430.khatabak",
        "sharma-sons_2026-10-21_1430_auto.khatabak.partial",
        "sharma-sons_2026-13-21_1430_auto.khatabak",
        "sharma-sons_2026-10-21_14300_auto.khatabak",
        "sharma-sons_2026-10-21_1430-1_auto.khatabak",
        "sharma-sons_2026-10-21_1430-02_auto.khatabak",
        "sharma-sons_2026-10-21_1430-x_auto.khatabak",
        "sharma-sons-2_2026-10-21_1430_auto.khatabak",
        "sharma_2026-10-21_1430_auto.khatabak",
        "Sharma-sons_2026-10-21_1430_auto.khatabak",
        "sharma-sons_2026-10-21_1430_auto.khata",
    ] {
        assert_eq!(parse_automatic(not, "sharma-sons"), None, "{not}");
    }
}

#[test]
fn rotation_keeps_the_newest_and_touches_nothing_else() {
    let dir = Dir::new("rotate");
    let slug = "sharma-sons";
    let day = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    let mut autos = Vec::new();
    for i in 0..12u32 {
        let at = day.and_hms_opt(9, i, 0).unwrap();
        let p = dir.0.join(file_name(slug, at, true, 1));
        fs::write(&p, b"x").unwrap();
        autos.push(p);
    }
    // Two in the same minute as the newest.
    let same = dir.0.join(file_name(slug, day.and_hms_opt(9, 11, 0).unwrap(), true, 2));
    fs::write(&same, b"x").unwrap();
    let others = [
        file_name(slug, day.and_hms_opt(8, 0, 0).unwrap(), false, 1), // a manual backup, older than all
        file_name("sharma", day.and_hms_opt(8, 0, 0).unwrap(), true, 1), // another company
        format!("{}.partial", file_name(slug, day.and_hms_opt(7, 0, 0).unwrap(), true, 1)),
        "sharma-sons_2026-10-01_0700_auto.khatabak.txt".into(),
        "sharma-sons_2026-10-01_0700_AUTO.khatabak".into(),
        "notes.txt".into(),
        "sharma-sons.khata".into(),
    ];
    for o in &others {
        fs::write(dir.0.join(o), b"keep me").unwrap();
    }
    // A folder with a matching name is not a file.
    let folder = dir.0.join(file_name(slug, day.and_hms_opt(6, 0, 0).unwrap(), true, 1));
    fs::create_dir(&folder).unwrap();

    let deleted = rotate(&dir.0, slug, 10, None).unwrap();
    assert_eq!(deleted, vec![autos[2].clone(), autos[1].clone(), autos[0].clone()], "the three oldest of 13");
    for p in autos[3..].iter().chain([&same]) {
        assert!(p.is_file(), "{}", p.display());
    }
    for o in &others {
        assert_eq!(fs::read(dir.0.join(o)).unwrap(), b"keep me", "{o}");
    }
    assert!(folder.is_dir());
    // Keeping zero still keeps one; running again changes nothing more.
    assert_eq!(rotate(&dir.0, slug, 10, None).unwrap(), Vec::<PathBuf>::new());
    assert_eq!(rotate(&dir.0, slug, 0, None).unwrap().len(), 9);
    assert!(same.is_file(), "the newest stays");

    // The backup just made stays even when the clock went back.
    let earlier = dir.0.join(file_name(slug, day.and_hms_opt(1, 0, 0).unwrap(), true, 1));
    fs::write(&earlier, b"x").unwrap();
    assert_eq!(rotate(&dir.0, slug, 1, Some(&earlier)).unwrap(), vec![same.clone()]);
    assert!(earlier.is_file());
}

#[test]
fn a_new_backup_in_the_same_minute_sorts_after_the_others() {
    let dir = Dir::new("next");
    let slug = "demo";
    let at = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap().and_hms_opt(14, 30, 0).unwrap();
    let first = next_path(&dir.0, slug, at, true);
    assert_eq!(first.file_name().unwrap(), "demo_2026-10-21_1430_auto.khatabak");
    for n in 2..=3 {
        fs::write(dir.0.join(file_name(slug, at, true, n)), b"x").unwrap();
    }
    // n=1 is free (rotated away), but a new one must come after n=3.
    assert_eq!(next_path(&dir.0, slug, at, true).file_name().unwrap(), "demo_2026-10-21_1430-4_auto.khatabak");
    assert_eq!(next_path(&dir.0, slug, at, false).file_name().unwrap(), "demo_2026-10-21_1430.khatabak");
    assert_eq!(parse_name("demo_2026-10-21_1430-2.khatabak", slug), Some((at, 2, false)));
}
