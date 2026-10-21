//! Lookup tables for voucher numbers, so that duplicate checks and the next
//! number do not scan every voucher.
//!
//! The index is derived state: the [`Book`](crate::Book) keeps it in step
//! with every voucher it stores or removes, and rebuilds it when it is
//! loaded or when the numbering settings change. It is never written to the
//! audit trail.

use std::collections::{BTreeSet, HashMap};

use chrono::NaiveDate;

use crate::numbering::NumberSeries;
use crate::profile::Profile;
use crate::voucher::{Voucher, VoucherId, VoucherKind};

/// A series period: the voucher type and the first day of the period
/// (`None` for a series that never restarts).
type Period = (VoucherKind, Option<NaiveDate>);

#[derive(Debug, Default, Clone)]
pub(crate) struct NumberIndex {
    /// Every voucher (cancelled ones too) by type and [`loose`] number.
    by_number: HashMap<(VoucherKind, String), BTreeSet<VoucherId>>,
    /// Running numbers in each series period, under the current numbering.
    running: HashMap<Period, BTreeSet<(u64, VoucherId)>>,
}

/// The number with ASCII letters lower-cased and every other character
/// turned into `-`. Numbers equal ignoring ASCII case have the same loose
/// form, as do numbers that only differ in punctuation ("26-27/1" and
/// "26/27-1"), so a lookup by loose form finds every candidate for a
/// stricter comparison.
pub(crate) fn loose(number: &str) -> String {
    number.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect()
}

/// The numbering settings that the running numbers depend on.
pub(crate) fn numbering_basis(profile: Option<&Profile>) -> (u32, Option<&std::collections::BTreeMap<VoucherKind, NumberSeries>>) {
    (profile.and_then(|p| p.fy_start_month).unwrap_or(1), profile.map(|p| &p.numbering).filter(|n| !n.is_empty()))
}

/// The series period of a voucher dated `date` and its running number, if
/// its number belongs to the series.
fn place(profile: Option<&Profile>, kind: VoucherKind, number: &str, date: NaiveDate) -> (Period, Option<u64>) {
    let month = profile.and_then(|p| p.fy_start_month).unwrap_or(1);
    let default = NumberSeries::default();
    let series = profile.and_then(|p| p.numbering.get(&kind)).unwrap_or(&default);
    let period = series.period(date, month).map(|(start, _)| start);
    ((kind, period), series.running(number, date, month))
}

impl NumberIndex {
    pub(crate) fn build<'a>(vouchers: impl IntoIterator<Item = &'a Voucher>, profile: Option<&Profile>) -> Self {
        let mut index = Self::default();
        for v in vouchers {
            index.insert(v, profile);
        }
        index
    }

    pub(crate) fn insert(&mut self, v: &Voucher, profile: Option<&Profile>) {
        self.by_number.entry((v.kind, loose(&v.number))).or_default().insert(v.id);
        let (period, running) = place(profile, v.kind, &v.number, v.date);
        if let Some(n) = running {
            self.running.entry(period).or_default().insert((n, v.id));
        }
    }

    /// Forget `v`, which must be the voucher as it was indexed.
    pub(crate) fn remove(&mut self, v: &Voucher, profile: Option<&Profile>) {
        let key = (v.kind, loose(&v.number));
        if let Some(ids) = self.by_number.get_mut(&key) {
            ids.remove(&v.id);
            if ids.is_empty() {
                self.by_number.remove(&key);
            }
        }
        let (period, running) = place(profile, v.kind, &v.number, v.date);
        if let (Some(n), Some(set)) = (running, self.running.get_mut(&period)) {
            set.remove(&(n, v.id));
            if set.is_empty() {
                self.running.remove(&period);
            }
        }
    }

    /// Ids of the vouchers of `kind` whose number has the same [`loose`]
    /// form as `number`, in id order.
    pub(crate) fn same_number<'a>(&'a self, kind: VoucherKind, number: &str) -> impl Iterator<Item = VoucherId> + 'a {
        self.by_number.get(&(kind, loose(number))).into_iter().flatten().copied()
    }

    /// Highest running number used in a series period.
    pub(crate) fn max_running(&self, kind: VoucherKind, period: Option<NaiveDate>) -> Option<u64> {
        self.running.get(&(kind, period)).and_then(|s| s.last()).map(|&(n, _)| n)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::numbering::Restart;
    use crate::{Book, BookError, Entry, LedgerId};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_760_000_000, 0).unwrap()
    }

    #[derive(Clone)]
    struct Fixture {
        book: Book,
        cash: LedgerId,
        rent: LedgerId,
    }

    fn fixture(restart: Restart, prefix: &str) -> Fixture {
        let mut book = Book::new("Index Traders");
        let g = |b: &Book, n: &str| b.group_by_name(n).unwrap().id;
        let cash = book.create_ledger(at(), "u", "Cash", g(&book, "Cash-in-Hand")).unwrap();
        let rent = book.create_ledger(at(), "u", "Rent", g(&book, "Indirect Expenses")).unwrap();
        book.set_country(at(), "u", "IN").unwrap();
        book.set_fy_start_month(at(), "u", 4).unwrap();
        let series = NumberSeries { restart, prefix: prefix.into(), suffix: String::new(), width: 0 };
        book.set_numbering(at(), "u", VoucherKind::Payment, series).unwrap();
        Fixture { book, cash, rent }
    }

    impl Fixture {
        fn voucher(&self, number: &str, date: NaiveDate) -> Voucher {
            let a = crate::Amount::from(10);
            serde_json::from_value(serde_json::json!({
                "id": VoucherId::new(), "kind": VoucherKind::Payment, "number": number, "date": date,
                "entries": [Entry::dr(self.rent, a), Entry::cr(self.cash, a)], "narration": "",
            }))
            .unwrap()
        }

        /// Post the next automatic number on `date`; returns its id and number.
        fn post_next(&mut self, date: NaiveDate) -> (VoucherId, String) {
            let number = self.book.next_voucher_number(VoucherKind::Payment, date);
            let v = self.voucher(&number, date);
            (self.book.post_voucher(at(), "u", v).unwrap(), number)
        }

        fn post(&mut self, number: &str, date: NaiveDate) -> Result<VoucherId, BookError> {
            let v = self.voucher(number, date);
            self.book.post_voucher(at(), "u", v)
        }
    }

    /// The index must agree with one built from scratch, and the answers
    /// must agree with a plain scan of every voucher.
    fn assert_consistent(book: &Book) {
        let fresh = NumberIndex::build(book.vouchers(), book.profile());
        let ix = book.number_index();
        assert_eq!(ix.by_number, fresh.by_number, "number lookup drifted");
        assert_eq!(ix.running, fresh.running, "running numbers drifted");
        let month = book.fy_start_month();
        for v in book.vouchers() {
            for probe in [v.date, d(2025, 4, 1), d(2026, 3, 31), d(2026, 4, 1), d(2026, 5, 15)] {
                let series = book.numbering(v.kind);
                let period = series.period(probe, month);
                let max = book
                    .vouchers()
                    .filter(|o| o.kind == v.kind && period.is_none_or(|(a, b)| o.date >= a && o.date <= b))
                    .filter_map(|o| series.running(&o.number, o.date, month))
                    .max()
                    .unwrap_or(0);
                assert_eq!(book.next_voucher_number(v.kind, probe), series.format(max + 1, probe, month));
            }
        }
    }

    #[test]
    fn loose_form_folds_case_and_punctuation() {
        assert_eq!(loose("26-27/Ab"), "26-27-ab");
        assert_eq!(loose("26/27-aB"), loose("26-27/ab"));
        assert_ne!(loose("1"), loose("10"));
    }

    #[test]
    fn index_follows_post_alter_cancel_delete_and_replay_in_every_restart_mode() {
        let modes = [(Restart::Never, ""), (Restart::Yearly, ""), (Restart::Yearly, "{FY}/"), (Restart::Monthly, "{MM}-")];
        for (restart, prefix) in modes {
            let mut f = fixture(restart, prefix);
            let (a, a_no) = f.post_next(d(2026, 3, 30));
            let (b, b_no) = f.post_next(d(2026, 3, 31));
            let (c, _) = f.post_next(d(2026, 4, 2));
            assert_consistent(&f.book);
            // A taken number is refused in its period, case ignored.
            assert_eq!(f.post(&a_no.to_uppercase(), d(2026, 3, 30)), Err(BookError::DuplicateVoucherNumber(a_no.to_uppercase())));

            // Alter that changes the number: the old number is free again.
            let renumbered = Voucher { number: format!("{prefix}X9").replace("{FY}", "25-26").replace("{MM}", "03"), ..f.book.voucher(a).unwrap().clone() };
            f.book.alter_voucher(at(), "u", renumbered.clone()).unwrap();
            assert_consistent(&f.book);
            assert_eq!(f.post(&renumbered.number, d(2026, 3, 30)), Err(BookError::DuplicateVoucherNumber(renumbered.number.clone())));
            let reused = f.post(&a_no, d(2026, 3, 30)).unwrap();
            assert_consistent(&f.book);

            // Alter that moves a voucher into the next period (FY and month).
            let moved = Voucher { date: d(2026, 4, 3), ..f.book.voucher(b).unwrap().clone() };
            f.book.alter_voucher(at(), "u", moved).unwrap();
            if restart != Restart::Never {
                // The old period's number can be used again.
                f.post(&b_no, d(2026, 3, 31)).unwrap();
            }
            assert_consistent(&f.book);

            // Cancel: the number stays taken.
            f.book.cancel_voucher(at(), "u", c).unwrap();
            let c_no = f.book.voucher(c).unwrap().number.clone();
            assert_eq!(f.post(&c_no, d(2026, 4, 2)), Err(BookError::DuplicateVoucherNumber(c_no.clone())));
            assert_consistent(&f.book);

            // Delete: the number is free.
            f.book.delete_voucher(at(), "u", reused).unwrap();
            assert_consistent(&f.book);
            f.post(&a_no, d(2026, 3, 30)).unwrap();

            // Replay (company open) rebuilds the same index.
            let replayed = Book::replay("Index Traders", f.book.audit().clone()).unwrap();
            assert_consistent(&replayed);
            assert_eq!(replayed.number_index().by_number, f.book.number_index().by_number);
            assert_eq!(replayed.number_index().running, f.book.number_index().running);
            for date in [d(2026, 3, 1), d(2026, 4, 1)] {
                assert_eq!(replayed.next_voucher_number(VoucherKind::Payment, date), f.book.next_voucher_number(VoucherKind::Payment, date));
            }
        }
    }

    #[test]
    fn restart_modes_number_and_check_per_period() {
        let mut f = fixture(Restart::Never, "");
        f.post_next(d(2026, 3, 31));
        assert_eq!(f.post_next(d(2026, 4, 1)).1, "2");
        assert!(f.post("1", d(2027, 1, 1)).is_err(), "never restarts: unique for life");

        let mut f = fixture(Restart::Yearly, "{FY}/");
        assert_eq!(f.post_next(d(2026, 3, 31)).1, "25-26/1");
        assert_eq!(f.post_next(d(2026, 4, 1)).1, "26-27/1");
        assert_eq!(f.post_next(d(2027, 3, 31)).1, "26-27/2");
        assert_eq!(f.book.next_voucher_number(VoucherKind::Payment, d(2025, 6, 1)), "25-26/2");

        let mut f = fixture(Restart::Yearly, "");
        f.post_next(d(2026, 3, 31));
        assert_eq!(f.post_next(d(2026, 4, 1)).1, "1");
        f.post("7", d(2026, 5, 1)).unwrap();
        assert!(f.post("7", d(2027, 3, 31)).is_err(), "same financial year");
        f.post("7", d(2027, 4, 1)).unwrap();
        assert_eq!(f.book.next_voucher_number(VoucherKind::Payment, d(2026, 4, 1)), "8");

        let mut f = fixture(Restart::Monthly, "{YY}{MM}/");
        assert_eq!(f.post_next(d(2026, 4, 30)).1, "2604/1");
        assert_eq!(f.post_next(d(2026, 5, 1)).1, "2605/1");
        assert_eq!(f.post_next(d(2026, 5, 31)).1, "2605/2");
        f.post("Z", d(2026, 6, 1)).unwrap();
        f.post("z", d(2026, 7, 1)).unwrap();
        assert!(f.post("Z", d(2026, 7, 31)).is_err(), "same month");
        assert_consistent(&f.book);
    }

    #[test]
    fn changing_the_numbering_rebuilds_the_running_numbers() {
        let mut f = fixture(Restart::Never, "");
        f.post_next(d(2026, 3, 31));
        f.post_next(d(2026, 4, 1));
        f.post("26-27/5", d(2026, 4, 2)).unwrap();
        assert_eq!(f.book.next_voucher_number(VoucherKind::Payment, d(2026, 4, 3)), "3");
        f.book.set_numbering(at(), "u", VoucherKind::Payment, NumberSeries::yearly_with_year()).unwrap();
        assert_consistent(&f.book);
        assert_eq!(f.book.next_voucher_number(VoucherKind::Payment, d(2026, 4, 3)), "26-27/6");
        // A financial-year change moves every period.
        f.book.set_fy_start_month(at(), "u", 1).unwrap();
        assert_consistent(&f.book);
        // Other profile changes leave the index alone and correct.
        f.book.set_company_tax(at(), "u", None, Some("27".into())).unwrap();
        assert_consistent(&f.book);
    }

    #[test]
    fn sync_of_remote_entries_keeps_the_index() {
        // Two copies of one company. The remote posts and alters; the local
        // copy pulls the remote's entries and rebases its offline ones.
        let mut remote = fixture(Restart::Yearly, "{FY}/");
        let mut local = remote.clone();
        let base = remote.book.audit().entries().len();
        let (r1, _) = remote.post_next(d(2026, 4, 1));
        remote.post_next(d(2026, 4, 2));
        let moved = Voucher { date: d(2026, 3, 31), number: "25-26/1".into(), ..remote.book.voucher(r1).unwrap().clone() };
        remote.book.alter_voucher(at(), "u", moved).unwrap();

        // Offline, the local copy numbered two vouchers of its own.
        let mut offline = local.clone();
        offline.post("26-27/1", d(2026, 4, 5)).unwrap();
        offline.post("26-27/2", d(2026, 4, 6)).unwrap();
        let pending = offline.book.audit().entries()[base..].to_vec();

        // Pull: replay the remote's entries on the local copy.
        for e in &remote.book.audit().entries()[base..] {
            local.book.apply_entry(e).unwrap();
        }
        assert_consistent(&local.book);
        assert_eq!(local.book.next_voucher_number(VoucherKind::Payment, d(2026, 4, 9)), "26-27/3");
        // Rebase: "26-27/1" was freed by the remote's alteration, "26-27/2" is taken.
        local.book.reapply(&pending[0]).unwrap();
        assert_eq!(local.book.reapply(&pending[1]), Err(BookError::DuplicateVoucherNumber("26-27/2".into())));
        assert_consistent(&local.book);
        assert_eq!(local.post_next(d(2026, 4, 9)).1, "26-27/3");
        assert_consistent(&local.book);
    }
}
