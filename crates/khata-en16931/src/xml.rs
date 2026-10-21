//! A small indenting XML writer: enough for the two invoice syntaxes, with
//! text and attribute escaping. Empty optional values are left out.

use rust_decimal::Decimal;

use crate::model::round2;

pub(crate) struct Xml {
    out: String,
    depth: usize,
}

/// Escape text or an attribute value.
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters other than tab, newline and carriage
            // return are not allowed in XML 1.0: drop them.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// An amount with exactly two decimals ("100.00").
pub(crate) fn amount(a: Decimal) -> String {
    round2(a).to_string()
}

/// A quantity, price or percentage without trailing zeros ("2", "10.1").
pub(crate) fn number(a: Decimal) -> String {
    a.normalize().to_string()
}

impl Xml {
    pub(crate) fn new() -> Self {
        Xml { out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"), depth: 0 }
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
    }

    fn start(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        for (k, v) in attrs {
            self.out.push(' ');
            self.out.push_str(k);
            self.out.push_str("=\"");
            self.out.push_str(&escape(v));
            self.out.push('"');
        }
    }

    pub(crate) fn open(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        self.start(tag, attrs);
        self.out.push_str(">\n");
        self.depth += 1;
    }

    pub(crate) fn close(&mut self, tag: &str) {
        self.depth -= 1;
        self.indent();
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    /// `<tag attrs>text</tag>`, written even when `text` is empty.
    pub(crate) fn leaf(&mut self, tag: &str, attrs: &[(&str, &str)], text: &str) {
        self.start(tag, attrs);
        self.out.push('>');
        self.out.push_str(&escape(text));
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    pub(crate) fn text(&mut self, tag: &str, text: &str) {
        self.leaf(tag, &[], text);
    }

    /// A leaf only when `text` is given and not blank.
    pub(crate) fn opt(&mut self, tag: &str, text: Option<&str>) {
        if let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) {
            self.leaf(tag, &[], t);
        }
    }

    /// `<tag/>`.
    pub(crate) fn empty(&mut self, tag: &str) {
        self.start(tag, &[]);
        self.out.push_str("/>\n");
    }

    pub(crate) fn finish(self) -> String {
        debug_assert_eq!(self.depth, 0, "unbalanced XML");
        self.out
    }
}

/// Text that is set and not blank.
pub(crate) fn some(s: &str) -> Option<&str> {
    let t = s.trim();
    (!t.is_empty()).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn escapes_and_formats() {
        assert_eq!(escape("A & B <\"C\"> 'd'\u{1}"), "A &amp; B &lt;&quot;C&quot;&gt; &apos;d&apos;");
        assert_eq!((amount(dec!(100)), amount(dec!(2.125)), amount(dec!(-0.5))), ("100.00".into(), "2.13".into(), "-0.50".into()));
        assert_eq!((number(dec!(2.000)), number(dec!(19.00)), number(dec!(10.10))), ("2".into(), "19".into(), "10.1".into()));
        let mut x = Xml::new();
        x.open("a", &[("k", "v&")]);
        x.text("b", "1");
        x.opt("c", Some("  "));
        x.empty("d");
        x.close("a");
        assert_eq!(x.finish(), "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a k=\"v&amp;\">\n  <b>1</b>\n  <d/>\n</a>\n");
    }
}
