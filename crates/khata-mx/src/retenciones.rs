//! Withholdings (retenciones) a customer makes on a CFDI, from a party's
//! tax class.
//!
//! Sources: `docs/research/mexico-compliance.md` §1.3 and §13 (LIVA
//! art. 1-A; RLIVA art. 3; LISR arts. 106, 116, 113-J; Anexo 20 and the
//! c_TasaOCuota catalogue). The common cases:
//! - a persona moral paying a persona física for professional services
//!   (honorarios) or rent withholds **ISR 10%** (LISR arts. 106, 116) and
//!   **two thirds of the IVA** (LIVA art. 1-A fr. II a; RLIVA art. 3
//!   fr. I): 2/3 × 16% = 10.6667%, TasaOCuota `0.106667`;
//! - a persona moral paying a RESICO persona física withholds **ISR
//!   1.25%** (LISR art. 113-J), plus two thirds of the IVA for services or
//!   rent;
//! - freight (autotransporte terrestre de bienes) paid by a persona moral:
//!   **IVA 4%** (RLIVA art. 3 fr. II).
//!
//! # The class
//!
//! KhataOS has no withholding master, so the **customer's ledger tax
//! class** says what that customer withholds: `RET:` followed by tokens
//! joined with `+` (or `,`), at most 24 characters (the books' class
//! limit; spaces are dropped and letters upper-cased by the books):
//!
//! | token      | meaning                                              |
//! |------------|------------------------------------------------------|
//! | `ISR10`    | ISR at 10% of the concepto's base (any `ISR<n>`, n ≤ 35) |
//! | `RESICO`   | ISR at 1.25% (same as `ISR1.25`)                     |
//! | `IVA2/3`   | two thirds of the concepto's IVA (16% → 0.106667, 8% → 0.053333) |
//! | `IVAALL`   | all of the concepto's IVA (e.g. LIVA art. 1-A fr. III) |
//! | `IVA4`     | IVA at 4% of the base (freight; any `IVA<n>`, n ≤ 16) |
//! | `NONE`     | nothing (overrides a class elsewhere)                |
//!
//! Examples: `RET:ISR10+IVA2/3` (honorarios), `RET:RESICO+IVA2/3`,
//! `RET:IVA4`. ISR applies to every concepto with a base; IVA withholdings
//! apply only to conceptos taxed at a positive rate.
//!
//! **Rounding.** The Importe is Base × TasaOCuota rounded to the centavo,
//! which is what a PAC checks. With `0.106667` this can differ from exactly
//! two thirds of the IVA by a centavo on large bases [UNCERTAIN which one
//! the SAT prefers; research §1.3].
//!
//! Whether a withholding applies (the parties' régimen, the kind of
//! service) is the user's call: KhataOS applies the class as given.

use rust_decimal::{Decimal, RoundingStrategy};
use serde::Serialize;

use crate::cfdi::{Impuesto, Iva, Retencion};

/// The class prefix.
pub const PREFIX: &str = "RET:";

/// One withholding rule from a class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Rule {
    /// ISR at a percent of the base.
    Isr(Decimal),
    /// IVA at a fixed percent of the base (only on taxed conceptos).
    Iva(Decimal),
    /// A fraction of the concepto's IVA (2/3).
    IvaFraction { num: u32, den: u32 },
    /// All of the concepto's IVA.
    IvaAll,
}

fn pct(s: &str) -> Option<Decimal> {
    let d: Decimal = s.parse().ok()?;
    (d >= Decimal::ZERO).then_some(d)
}

/// Parse a class. `Ok(None)`: not a withholding class (no `RET:` prefix).
/// `Ok(Some(vec![]))`: `RET:NONE`.
pub fn parse_class(class: &str) -> Result<Option<Vec<Rule>>, String> {
    let class: String = class.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    let Some(rest) = class.strip_prefix(PREFIX) else {
        return Ok(None);
    };
    let mut rules = Vec::new();
    for token in rest.split(['+', ',', ';']).filter(|t| !t.is_empty()) {
        let rule = match token {
            "NONE" => continue,
            "RESICO" => Rule::Isr(Decimal::new(125, 2)),
            "IVA2/3" => Rule::IvaFraction { num: 2, den: 3 },
            "IVAALL" => Rule::IvaAll,
            t if t.starts_with("ISR") => {
                let p = pct(&t[3..]).filter(|p| *p <= Decimal::from(35)).ok_or_else(|| format!("{t}: ISR is withheld at 0 to 35%"))?;
                Rule::Isr(p)
            }
            t if t.starts_with("IVA") => {
                let p = pct(&t[3..]).filter(|p| *p <= Decimal::from(16)).ok_or_else(|| format!("{t}: IVA is withheld at 0 to 16%, IVA2/3 or IVAALL"))?;
                Rule::Iva(p)
            }
            t => return Err(format!("{t} is not a withholding (use ISR10, RESICO, IVA2/3, IVA4, IVAALL or NONE)")),
        };
        if rules.iter().any(|r: &Rule| r.impuesto() == rule.impuesto()) {
            return Err(format!("{class}: one ISR and one IVA withholding at most"));
        }
        rules.push(rule);
    }
    Ok(Some(rules))
}

impl Rule {
    pub fn impuesto(&self) -> Impuesto {
        match self {
            Rule::Isr(_) => Impuesto::Isr,
            _ => Impuesto::Iva,
        }
    }

    /// The TasaOCuota for a concepto with this IVA, or `None` when the
    /// rule does not apply to it.
    pub fn tasa(&self, iva: Option<Iva>) -> Option<Decimal> {
        let six = |d: Decimal| d.round_dp_with_strategy(6, RoundingStrategy::MidpointAwayFromZero);
        let taxed = match iva {
            Some(Iva::Tasa(p)) if p > Decimal::ZERO => Some(p),
            _ => None,
        };
        match *self {
            Rule::Isr(p) => Some(six(p / Decimal::ONE_HUNDRED)),
            Rule::Iva(p) => taxed.map(|_| six(p / Decimal::ONE_HUNDRED)),
            Rule::IvaFraction { num, den } => taxed.map(|r| six(r * Decimal::from(num) / Decimal::from(den) / Decimal::ONE_HUNDRED)),
            Rule::IvaAll => taxed.map(|r| six(r / Decimal::ONE_HUNDRED)),
        }
        .filter(|t| !t.is_zero())
    }
}

/// The retenciones for one concepto.
pub fn for_concepto(rules: &[Rule], iva: Option<Iva>) -> Vec<Retencion> {
    let mut out: Vec<Retencion> = rules.iter().filter_map(|r| r.tasa(iva).map(|tasa| Retencion { impuesto: r.impuesto(), tasa })).collect();
    out.sort_by_key(|r| r.impuesto);
    out
}

/// A short description for screens ("ISR 10% + IVA 10.6667%").
pub fn describe(rules: &[Rule]) -> String {
    if rules.is_empty() {
        return "no withholding".into();
    }
    rules
        .iter()
        .map(|r| match r {
            Rule::Isr(p) => format!("ISR {}%", p.normalize()),
            Rule::Iva(p) => format!("IVA {}%", p.normalize()),
            Rule::IvaFraction { num, den } => format!("IVA {num}/{den} of the IVA"),
            Rule::IvaAll => "all the IVA".into(),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn classes_parse() {
        assert_eq!(parse_class("STANDARD"), Ok(None));
        assert_eq!(parse_class("RET:NONE"), Ok(Some(vec![])));
        assert_eq!(parse_class("ret: isr10 + iva2/3"), Ok(Some(vec![Rule::Isr(dec!(10)), Rule::IvaFraction { num: 2, den: 3 }])));
        assert_eq!(parse_class("RET:RESICO"), Ok(Some(vec![Rule::Isr(dec!(1.25))])));
        assert_eq!(parse_class("RET:IVA4"), Ok(Some(vec![Rule::Iva(dec!(4))])));
        assert!(parse_class("RET:ISR40").is_err());
        assert!(parse_class("RET:IEPS3").is_err());
        assert!(parse_class("RET:ISR10+ISR1.25").is_err(), "one ISR rule");
        assert!("RET:RESICO+IVA2/3".len() <= 24 && "RET:ISR10+IVA2/3".len() <= 24, "fits the class limit");
    }

    #[test]
    fn rates_per_concepto() {
        let rules = parse_class("RET:ISR10+IVA2/3").unwrap().unwrap();
        let r = for_concepto(&rules, Some(Iva::Tasa(dec!(16))));
        assert_eq!(r, [Retencion { impuesto: Impuesto::Isr, tasa: dec!(0.1) }, Retencion { impuesto: Impuesto::Iva, tasa: dec!(0.106667) }]);
        assert_eq!(for_concepto(&rules, Some(Iva::Tasa(dec!(8))))[1].tasa, dec!(0.053333));
        // Exempt and 0% conceptos: ISR only.
        assert_eq!(for_concepto(&rules, Some(Iva::Exento)).len(), 1);
        assert_eq!(for_concepto(&rules, Some(Iva::Tasa(dec!(0)))).len(), 1);
        assert_eq!(for_concepto(&[Rule::Isr(dec!(1.25))], None)[0].tasa, dec!(0.0125));
        assert_eq!(for_concepto(&[Rule::IvaAll], Some(Iva::Tasa(dec!(16))))[0].tasa, dec!(0.16));
        assert_eq!(describe(&rules), "ISR 10% + IVA 2/3 of the IVA");
    }
}
