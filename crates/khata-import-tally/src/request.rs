//! Export requests for Tally's XML-over-HTTP server.
//!
//! A request is an `ENVELOPE` whose header asks to `Export` a `Data` report
//! by name; static variables pick the company, period and XML format. Send
//! it with `http::fetch` (feature `http`) or any HTTP client, then
//! pass the response to [`crate::import`].

use chrono::NaiveDate;
use quick_xml::escape::escape;

/// One "Export Data" request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRequest {
    /// The Tally report to export, e.g. "List of Accounts" or "Day Book".
    pub report: String,
    /// Loaded company to export from; Tally's current company when `None`.
    pub company: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Further static variables, as `(TAG, value)`.
    pub variables: Vec<(String, String)>,
}

impl ExportRequest {
    pub fn new(report: impl Into<String>) -> Self {
        Self { report: report.into(), company: None, from: None, to: None, variables: Vec::new() }
    }

    /// All masters via List of Accounts: groups, ledgers and voucher types,
    /// and the inventory masters (units, stock groups, stock items,
    /// godowns) too.
    pub fn masters() -> Self {
        Self::new("List of Accounts").variable("ACCOUNTTYPE", "All Masters")
    }

    /// Only the inventory masters (units, stock groups, stock items,
    /// godowns) via List of Accounts, e.g. to add stock to a company whose
    /// accounts were imported earlier. Import it together with the Day Book
    /// so item invoices find their items.
    pub fn inventory_masters() -> Self {
        Self::new("List of Accounts").variable("ACCOUNTTYPE", "All Inventory Masters")
    }

    /// Every voucher dated within `[from, to]`, via the Day Book.
    pub fn day_book(from: NaiveDate, to: NaiveDate) -> Self {
        Self { from: Some(from), to: Some(to), ..Self::new("Day Book") }.variable("EXPLODEFLAG", "Yes")
    }

    pub fn company(mut self, name: impl Into<String>) -> Self {
        self.company = Some(name.into());
        self
    }

    pub fn variable(mut self, tag: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.push((tag.into(), value.into()));
        self
    }

    /// The request envelope as XML text.
    pub fn to_xml(&self) -> String {
        let date = |d: NaiveDate| d.format("%-d-%b-%Y").to_string();
        let mut vars = vec![("SVEXPORTFORMAT".to_owned(), "$$SysName:XML".to_owned())];
        vars.extend(self.company.iter().map(|c| ("SVCURRENTCOMPANY".to_owned(), c.clone())));
        vars.extend(self.from.map(|d| ("SVFROMDATE".to_owned(), date(d))));
        vars.extend(self.to.map(|d| ("SVTODATE".to_owned(), date(d))));
        vars.extend(self.variables.iter().cloned());
        let vars: String = vars.iter().map(|(t, v)| format!("<{t}>{}</{t}>", escape(v.as_str()))).collect();
        format!(
            "<ENVELOPE><HEADER><VERSION>1</VERSION><TALLYREQUEST>Export</TALLYREQUEST><TYPE>Data</TYPE>\
             <ID>{}</ID></HEADER><BODY><DESC><STATICVARIABLES>{vars}</STATICVARIABLES></DESC></BODY></ENVELOPE>",
            escape(self.report.as_str())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_documented_export_envelope() {
        let from = NaiveDate::from_ymd_opt(2024, 4, 1).unwrap();
        let to = NaiveDate::from_ymd_opt(2025, 3, 31).unwrap();
        let xml = ExportRequest::day_book(from, to).company("Shah & Sons").to_xml();
        assert!(
            xml.starts_with(
                "<ENVELOPE><HEADER><VERSION>1</VERSION><TALLYREQUEST>Export</TALLYREQUEST><TYPE>Data</TYPE>"
            )
        );
        assert!(xml.contains("<ID>Day Book</ID>"));
        assert!(xml.contains("<SVEXPORTFORMAT>$$SysName:XML</SVEXPORTFORMAT>"));
        assert!(xml.contains("<SVCURRENTCOMPANY>Shah &amp; Sons</SVCURRENTCOMPANY>"));
        assert!(xml.contains("<SVFROMDATE>1-Apr-2024</SVFROMDATE><SVTODATE>31-Mar-2025</SVTODATE>"));
        assert!(ExportRequest::masters().to_xml().contains("<ID>List of Accounts</ID>"));
        let inv = ExportRequest::inventory_masters().to_xml();
        assert!(inv.contains("<ID>List of Accounts</ID>"));
        assert!(inv.contains("<ACCOUNTTYPE>All Inventory Masters</ACCOUNTTYPE>"));
    }
}
