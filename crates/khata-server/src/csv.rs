//! RFC 4180 CSV for report downloads.
//!
//! Amounts are written as plain decimal strings (no grouping, no currency
//! symbol) so spreadsheets and CA tools read them as numbers. Free-text cells
//! (ledger names, users, references) are also guarded against spreadsheet
//! formula injection: a cell that starts with `=`, `+`, `-`, `@`, tab or CR
//! is prefixed with `'`, as OWASP recommends, so a crafted ledger name
//! cannot run a formula on the auditor's machine.

use std::borrow::Cow;

/// Quote a field if it needs it (comma, quote, CR or LF), doubling quotes.
pub(crate) fn field(s: &str) -> Cow<'_, str> {
    if s.contains([',', '"', '\n', '\r']) {
        Cow::Owned(format!("\"{}\"", s.replace('"', "\"\"")))
    } else {
        Cow::Borrowed(s)
    }
}

/// A free-text value made safe to open in a spreadsheet.
pub(crate) fn text(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_owned() }
}

/// A CSV document built row by row, CRLF line endings.
pub(crate) struct Csv(String);

impl Csv {
    pub(crate) fn new(headers: &[&str]) -> Self {
        let mut c = Self(String::new());
        c.row(headers.iter().copied());
        c
    }

    pub(crate) fn row<S: AsRef<str>>(&mut self, cells: impl IntoIterator<Item = S>) {
        for (i, cell) in cells.into_iter().enumerate() {
            if i > 0 {
                self.0.push(',');
            }
            self.0.push_str(&field(cell.as_ref()));
        }
        self.0.push_str("\r\n");
    }

    pub(crate) fn finish(self) -> String {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(field("Rent"), "Rent");
        assert_eq!(field("Capital, Owner"), "\"Capital, Owner\"");
        assert_eq!(field("Rent \"office\""), "\"Rent \"\"office\"\"\"");
        assert_eq!(field("two\nlines"), "\"two\nlines\"");
        assert_eq!(field("cr\r"), "\"cr\r\"");
    }

    #[test]
    fn neutralises_formulas_in_text() {
        assert_eq!(text("=HYPERLINK(\"x\")"), "'=HYPERLINK(\"x\")");
        assert_eq!(text("+91 98"), "'+91 98");
        assert_eq!(text("@sum"), "'@sum");
        assert_eq!(text("-x"), "'-x");
        assert_eq!(text("Mehta & Co."), "Mehta & Co.");
    }

    #[test]
    fn builds_rows_with_crlf() {
        let mut c = Csv::new(&["A", "B"]);
        c.row(["x,y", "1"]);
        assert_eq!(c.finish(), "A,B\r\n\"x,y\",1\r\n");
    }
}
