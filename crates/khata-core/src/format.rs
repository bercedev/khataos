//! Amount formatting for screens, prints and exports.

use rust_decimal::RoundingStrategy;

use crate::Amount;

/// Digit grouping convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grouping {
    /// 12,34,56,789.00 — India (lakh/crore).
    Indian,
    /// 123,456,789.00 — most other locales.
    Western,
}

/// Format `amount` with `places` decimals, grouping and a leading minus.
pub fn format_amount(amount: Amount, grouping: Grouping, places: u32) -> String {
    let rounded = amount.round_dp_with_strategy(places, RoundingStrategy::MidpointAwayFromZero);
    let negative = rounded.is_sign_negative() && !rounded.is_zero();
    let text = format!("{:.*}", places as usize, rounded.abs());
    let (int, frac) = text.split_once('.').map_or((text.as_str(), None), |(i, f)| (i, Some(f)));
    let grouped = group(int, grouping);
    let mut out = String::with_capacity(grouped.len() + places as usize + 2);
    if negative {
        out.push('-');
    }
    out.push_str(&grouped);
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    out
}

/// Format a signed balance (debit positive) as `2,50,000.00 Dr` / `… Cr`.
pub fn format_balance(balance: Amount, grouping: Grouping) -> String {
    if balance.is_zero() {
        return format_amount(balance, grouping, 2);
    }
    let side = if balance.is_sign_positive() { "Dr" } else { "Cr" };
    format!("{} {side}", format_amount(balance.abs(), grouping, 2))
}

fn group(int: &str, grouping: Grouping) -> String {
    let digits: Vec<char> = int.chars().collect();
    let n = digits.len();
    if n <= 3 {
        return int.to_owned();
    }
    // Positions (from the right) after which a separator goes.
    let mut out = String::with_capacity(n + n / 2);
    for (i, c) in digits.iter().enumerate() {
        let from_right = n - i;
        let sep = match grouping {
            Grouping::Western => from_right % 3 == 0,
            Grouping::Indian => from_right == 3 || (from_right > 3 && (from_right - 3) % 2 == 0),
        };
        if sep && i != 0 {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

const ONES: [&str; 20] = [
    "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten", "Eleven", "Twelve",
    "Thirteen", "Fourteen", "Fifteen", "Sixteen", "Seventeen", "Eighteen", "Nineteen",
];
const TENS: [&str; 10] = ["", "", "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety"];

/// 0–999 in words ("Three Hundred Forty-Five").
fn hundreds(n: u64) -> String {
    let (h, rest) = (n / 100, n % 100);
    let tail = match rest {
        0 => String::new(),
        1..=19 => ONES[rest as usize].to_owned(),
        _ if rest % 10 == 0 => TENS[(rest / 10) as usize].to_owned(),
        _ => format!("{}-{}", TENS[(rest / 10) as usize], ONES[(rest % 10) as usize]),
    };
    match (h, tail.is_empty()) {
        (0, _) => tail,
        (_, true) => format!("{} Hundred", ONES[h as usize]),
        (_, false) => format!("{} Hundred {tail}", ONES[h as usize]),
    }
}

/// A whole number in words, with Indian (lakh, crore) or Western
/// (thousand, million, billion) scales.
pub fn number_in_words(n: u64, grouping: Grouping) -> String {
    if n == 0 {
        return ONES[0].to_owned();
    }
    // (divisor, name) from the largest scale down; the last part is < 1000.
    let scales: &[(u64, &str)] = match grouping {
        Grouping::Indian => &[(10_000_000, "Crore"), (100_000, "Lakh"), (1_000, "Thousand")],
        Grouping::Western => &[(1_000_000_000_000, "Trillion"), (1_000_000_000, "Billion"), (1_000_000, "Million"), (1_000, "Thousand")],
    };
    let mut parts = Vec::new();
    let mut rest = n;
    for &(div, name) in scales {
        let q = rest / div;
        if q > 0 {
            // Crores can exceed 99: "Twelve Thousand Crore".
            let words = if q >= 1000 { number_in_words(q, grouping) } else { hundreds(q) };
            parts.push(format!("{words} {name}"));
            rest %= div;
        }
    }
    if rest > 0 {
        parts.push(hundreds(rest));
    }
    parts.join(" ")
}

/// An amount in words for invoices: "Rupees Twelve Thousand Three Hundred
/// Forty-Five and Fifty Paise Only". `major`/`minor` name the currency units.
pub fn amount_in_words(amount: Amount, grouping: Grouping, major: &str, minor: &str) -> String {
    let a = amount.abs().round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    let whole = a.trunc();
    let fraction = ((a - whole) * Amount::ONE_HUNDRED).trunc();
    let whole: u64 = whole.try_into().unwrap_or(u64::MAX);
    let fraction: u64 = fraction.try_into().unwrap_or(0);
    let mut out = format!("{major} {}", number_in_words(whole, grouping));
    if fraction > 0 {
        out.push_str(&format!(" and {} {minor}", number_in_words(fraction, grouping)));
    }
    out.push_str(" Only");
    if amount.is_sign_negative() && !amount.is_zero() {
        out.insert_str(0, "Minus ");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::inconsistent_digit_grouping)] // Indian grouping, on purpose
    fn amounts_in_words() {
        use rust_decimal_macros::dec;
        assert_eq!(number_in_words(0, Grouping::Indian), "Zero");
        assert_eq!(number_in_words(1_25_07_015, Grouping::Indian), "One Crore Twenty-Five Lakh Seven Thousand Fifteen");
        assert_eq!(number_in_words(12_500_000, Grouping::Western), "Twelve Million Five Hundred Thousand");
        assert_eq!(number_in_words(1_200_00_00_000, Grouping::Indian), "One Thousand Two Hundred Crore");
        assert_eq!(
            amount_in_words(dec!(12345.50), Grouping::Indian, "Rupees", "Paise"),
            "Rupees Twelve Thousand Three Hundred Forty-Five and Fifty Paise Only"
        );
        assert_eq!(amount_in_words(dec!(100), Grouping::Western, "Dirhams", "Fils"), "Dirhams One Hundred Only");
    }
    use rust_decimal_macros::dec;

    #[test]
    fn indian_grouping() {
        let f = |a| format_amount(a, Grouping::Indian, 2);
        assert_eq!(f(dec!(0)), "0.00");
        assert_eq!(f(dec!(999)), "999.00");
        assert_eq!(f(dec!(1000)), "1,000.00");
        assert_eq!(f(dec!(250000)), "2,50,000.00");
        assert_eq!(f(dec!(12345678.9)), "1,23,45,678.90");
        assert_eq!(f(dec!(-25000)), "-25,000.00");
    }

    #[test]
    fn western_grouping() {
        let f = |a| format_amount(a, Grouping::Western, 2);
        assert_eq!(f(dec!(250000)), "250,000.00");
        assert_eq!(f(dec!(123456789)), "123,456,789.00");
        assert_eq!(f(dec!(100)), "100.00");
    }

    #[test]
    fn rounds_half_away_from_zero() {
        assert_eq!(format_amount(dec!(0.005), Grouping::Indian, 2), "0.01");
        assert_eq!(format_amount(dec!(-0.004), Grouping::Indian, 2), "0.00");
        assert_eq!(format_amount(dec!(1.5), Grouping::Western, 0), "2");
    }

    #[test]
    fn balances_carry_dr_cr() {
        assert_eq!(format_balance(dec!(250000), Grouping::Indian), "2,50,000.00 Dr");
        assert_eq!(format_balance(dec!(-25000), Grouping::Indian), "25,000.00 Cr");
        assert_eq!(format_balance(dec!(0), Grouping::Indian), "0.00");
    }
}
