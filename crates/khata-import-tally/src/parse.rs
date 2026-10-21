//! Tally XML → plain Rust values.
//!
//! Tally wraps objects in an `ENVELOPE`. A file from Export (and most HTTP
//! "Data" exports) uses the import shape
//! `ENVELOPE/BODY/IMPORTDATA/REQUESTDATA/TALLYMESSAGE/{GROUP,LEDGER,VOUCHER,…}`;
//! some responses use `BODY/DATA/TALLYMESSAGE`, and "Collection" exports put
//! objects straight under `BODY/DATA/COLLECTION`. The reader streams the
//! document and only materializes the objects it cares about, so large Day
//! Book exports stay cheap.

use chrono::NaiveDate;
use khata_core::{Amount, Quantity};
use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::text::{decode, normalize, sanitize, tally_amount, tally_bool, tally_date, tally_qty, tally_rate};
use crate::{ImportError, ItemKind, SkipReason, Skipped};

/// A group master.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyGroup {
    pub name: String,
    pub aliases: Vec<String>,
    /// `None` when the parent is Tally's "Primary".
    pub parent: Option<String>,
    /// Tally's `RESERVEDNAME`, set on its predefined groups.
    pub reserved: Option<String>,
}

/// A ledger master.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyLedger {
    pub name: String,
    pub aliases: Vec<String>,
    /// `None` when the parent is "Primary" (e.g. "Profit & Loss A/c").
    pub parent: Option<String>,
    pub reserved: Option<String>,
    /// Opening balance converted to KhataOS sign: debit positive.
    pub opening: Amount,
}

/// A voucher type master; custom types name their base type as `parent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyVoucherType {
    pub name: String,
    pub parent: Option<String>,
}

/// One ledger line of a voucher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyEntry {
    pub ledger: String,
    /// KhataOS sign: debit positive. Taken from the sign of Tally's
    /// `AMOUNT` (negative = debit), which stays right for negative-value
    /// lines such as a discount on the credit side of a sales invoice.
    pub amount: Amount,
    /// Tally's `ISDEEMEDPOSITIVE`, kept for reference. On ordinary lines
    /// `Some(true)` coincides with a debit.
    pub deemed_positive: Option<bool>,
}

/// A unit of measure master (`UNIT`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyUnit {
    /// The symbol, e.g. "Nos" or "kg".
    pub name: String,
    /// `ORIGINALNAME`, the formal name ("Numbers"); may be empty.
    pub formal_name: String,
    /// `DECIMALPLACES`, when given.
    pub decimals: Option<u32>,
    /// A compound unit ("Box of 10 Nos"): `ISSIMPLEUNIT` No, or both
    /// `BASEUNITS` and `ADDITIONALUNITS` set.
    pub compound: bool,
}

/// A stock group master (`STOCKGROUP`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyStockGroup {
    pub name: String,
    pub aliases: Vec<String>,
    /// `None` when the parent is "Primary".
    pub parent: Option<String>,
}

/// A stock item master (`STOCKITEM`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyStockItem {
    pub name: String,
    pub aliases: Vec<String>,
    /// Stock group; `None` when "Primary".
    pub parent: Option<String>,
    /// `BASEUNITS`; `None` when blank or "Not Applicable".
    pub unit: Option<String>,
    /// `OPENINGBALANCE` quantity (may be negative when Tally has one).
    pub opening_qty: Quantity,
    /// `OPENINGVALUE` made positive; derived from `OPENINGRATE` × quantity
    /// when Tally gave a rate but no value.
    pub opening_value: Amount,
    /// The opening is split across godowns other than the default or
    /// named batches (`BATCHALLOCATIONS.LIST`); only the total is kept.
    pub godown_detail: bool,
    /// HSN/SAC code (`HSNCODE`, directly or in `GSTDETAILS.LIST` /
    /// `HSNDETAILS.LIST`), kept as the item's tax class.
    pub hsn: Option<String>,
    /// `COSTINGMETHOD` ("Avg. Cost", "FIFO", "Last Purchase Cost"…), as given.
    pub costing: Option<String>,
}

/// A godown master (`GODOWN`). Tally's "Main Location" is not one: it is
/// where stock without a godown sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyGodown {
    pub name: String,
    pub aliases: Vec<String>,
    /// `None` when "Primary" or "Main Location".
    pub parent: Option<String>,
}

/// One `BATCHALLOCATIONS.LIST` of a stock line: how much of it sits at
/// which godown, in which batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyBatch {
    /// `None` for Tally's default "Main Location" (or blank).
    pub godown: Option<String>,
    /// `None` for Tally's default "Primary Batch" (or blank).
    pub batch: Option<String>,
    /// Positive.
    pub qty: Quantity,
    /// KhataOS sign, like the line's.
    pub amount: Amount,
    pub rate: Option<Amount>,
}

/// One stock item line of a voucher (`ALLINVENTORYENTRIES.LIST`,
/// `INVENTORYENTRIES.LIST`, or `INVENTORYALLOCATIONS.LIST` nested in a
/// ledger line). Batch allocations are summed into the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyStockLine {
    pub item: String,
    /// Ledgers the line's value is booked to: the line's
    /// `ACCOUNTINGALLOCATIONS.LIST` ledgers, or the ledger line it is
    /// nested in. Exactly one is needed to keep the line.
    pub ledgers: Vec<String>,
    /// `ACTUALQTY` (the quantity that moves), else `BILLEDQTY`, made
    /// positive; the direction comes from the amount.
    pub qty: Quantity,
    /// Positive; derived from amount ÷ quantity when Tally gave none.
    pub rate: Amount,
    /// KhataOS sign, debit positive: positive brings stock in.
    pub amount: Amount,
    /// `ISDEEMEDPOSITIVE`; decides the direction of a zero-value line.
    pub deemed_positive: Option<bool>,
    pub batches: Vec<TallyBatch>,
}

impl TallyStockLine {
    /// Whether the batch allocations say more than "all at the main
    /// location, no batch".
    pub fn has_godown_detail(&self) -> bool {
        self.batches.iter().any(|b| b.godown.is_some() || b.batch.is_some())
    }

    /// Whether this line brings stock in.
    pub fn inward(&self) -> bool {
        if self.amount.is_zero() { self.deemed_positive == Some(true) } else { self.amount > Amount::ZERO }
    }
}

/// An accounting voucher with its stock item lines. Tax detail is not
/// carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TallyVoucher {
    /// `VOUCHERTYPENAME` (or the `VCHTYPE` attribute).
    pub type_name: String,
    /// Empty when Tally did not number the voucher.
    pub number: String,
    pub date: NaiveDate,
    pub narration: String,
    pub cancelled: bool,
    pub optional: bool,
    pub entries: Vec<TallyEntry>,
    pub inventory: Vec<TallyStockLine>,
    /// Why the stock lines cannot be used (an unreadable quantity, rate or
    /// amount); the accounting is still imported.
    pub inventory_problem: Option<String>,
}

/// Everything read from one Tally XML document.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TallyData {
    pub groups: Vec<TallyGroup>,
    pub ledgers: Vec<TallyLedger>,
    pub voucher_types: Vec<TallyVoucherType>,
    pub units: Vec<TallyUnit>,
    pub stock_groups: Vec<TallyStockGroup>,
    pub stock_items: Vec<TallyStockItem>,
    pub godowns: Vec<TallyGodown>,
    pub vouchers: Vec<TallyVoucher>,
    /// Objects that could not be read (bad amount or date).
    pub rejected: Vec<Skipped>,
}

impl TallyData {
    /// Append another document's objects.
    pub fn extend(&mut self, other: TallyData) {
        self.groups.extend(other.groups);
        self.ledgers.extend(other.ledgers);
        self.voucher_types.extend(other.voucher_types);
        self.units.extend(other.units);
        self.stock_groups.extend(other.stock_groups);
        self.stock_items.extend(other.stock_items);
        self.godowns.extend(other.godowns);
        self.vouchers.extend(other.vouchers);
        self.rejected.extend(other.rejected);
    }
}

/// Elements that hold a list of Tally objects.
const CONTAINERS: &[&str] = &["TALLYMESSAGE", "COLLECTION", "DATA", "REQUESTDATA"];
const OBJECTS: &[&str] = &["GROUP", "LEDGER", "VOUCHERTYPE", "VOUCHER", "UNIT", "STOCKGROUP", "STOCKITEM", "GODOWN"];

/// Tally's default godown and batch; allocations to them carry no detail.
const DEFAULT_GODOWN: &str = "mainlocation";
const DEFAULT_BATCH: &str = "primarybatch";

/// Parse a Tally XML document (UTF-8 or UTF-16).
pub fn parse(xml: &[u8]) -> Result<TallyData, ImportError> {
    let text = sanitize(&decode(xml));
    if text.trim().is_empty() {
        return Err(ImportError::Empty);
    }
    let mut reader = Reader::from_str(&text);
    let xml_err = |reader: &Reader<&[u8]>, e: &dyn std::fmt::Display| ImportError::Xml {
        position: reader.error_position(),
        message: e.to_string(),
    };

    let mut data = TallyData::default();
    // Element path outside captured objects, and the object being captured.
    let mut path: Vec<String> = Vec::new();
    let mut capture: Vec<Node> = Vec::new();
    let mut status = None::<String>;
    let mut error_text = String::new();

    loop {
        let event = reader.read_event().map_err(|e| xml_err(&reader, &e))?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(event, Event::Empty(_));
                let name = tag_name(e);
                if path.is_empty() && capture.is_empty() && !name.eq_ignore_ascii_case("ENVELOPE") {
                    return Err(ImportError::NotTallyXml(name));
                }
                let starts_object = capture.is_empty()
                    && OBJECTS.iter().any(|o| name.eq_ignore_ascii_case(o))
                    && path.last().is_some_and(|p| CONTAINERS.iter().any(|c| p.eq_ignore_ascii_case(c)));
                if !capture.is_empty() || starts_object {
                    let node = Node { attrs: attributes(e), name, ..Node::default() };
                    capture.push(node);
                    if empty {
                        close_node(&mut capture, &mut data);
                    }
                } else if !empty {
                    path.push(name);
                }
            }
            Event::End(_) => {
                if capture.is_empty() {
                    path.pop();
                } else {
                    close_node(&mut capture, &mut data);
                }
            }
            Event::Text(ref t) => {
                let s = t.xml10_content().map_err(|e| xml_err(&reader, &e))?;
                push_text(&s, &mut capture, &path, &status, &mut error_text);
                if capture.is_empty() && at(&path, &["ENVELOPE", "HEADER", "STATUS"]) {
                    status = Some(s.trim().to_owned());
                }
            }
            Event::CData(ref t) => {
                let s = t.decode().map_err(|e| xml_err(&reader, &e))?;
                push_text(&s, &mut capture, &path, &status, &mut error_text);
            }
            Event::GeneralRef(ref r) => {
                let resolved = match r.resolve_char_ref().map_err(|e| xml_err(&reader, &e))? {
                    Some(c) => c.to_string(),
                    None => predefined(&r.decode().map_err(|e| xml_err(&reader, &e))?).to_owned(),
                };
                push_text(&resolved, &mut capture, &path, &status, &mut error_text);
            }
            Event::Eof if path.is_empty() && capture.is_empty() => break,
            Event::Eof => {
                let open = capture.last().map_or_else(|| path.last().cloned().unwrap_or_default(), |n| n.name.clone());
                return Err(ImportError::Xml {
                    position: reader.buffer_position(),
                    message: format!("document ends inside <{open}>; the export looks truncated"),
                });
            }
            _ => {}
        }
    }
    if status.as_deref() == Some("0") {
        let msg = error_text.split_whitespace().collect::<Vec<_>>().join(" ");
        return Err(ImportError::TallyStatus(if msg.is_empty() { "status 0".into() } else { msg }));
    }
    Ok(data)
}

fn push_text(s: &str, capture: &mut [Node], path: &[String], status: &Option<String>, error_text: &mut String) {
    if let Some(node) = capture.last_mut() {
        node.text.push_str(s);
    } else if status.as_deref() == Some("0") && path.iter().any(|p| p.eq_ignore_ascii_case("BODY")) {
        error_text.push_str(s);
        error_text.push(' ');
    }
}

fn at(path: &[String], want: &[&str]) -> bool {
    path.len() == want.len() && path.iter().zip(want).all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// The five entities XML predefines; anything else (e.g. a stray `&tab;`)
/// is dropped rather than failing the whole import.
fn predefined(name: &str) -> &'static str {
    match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => "",
    }
}

fn tag_name(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn attributes(e: &BytesStart<'_>) -> Vec<(String, String)> {
    e.attributes()
        .with_checks(false)
        .flatten()
        .map(|a| {
            let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
            let value = a
                .normalized_value_with(XmlVersion::Implicit1_0, 1, |n| Some(predefined(n)))
                .map(|v| v.into_owned())
                .unwrap_or_else(|_| String::from_utf8_lossy(&a.value).into_owned());
            (key, value)
        })
        .collect()
}

fn close_node(capture: &mut Vec<Node>, data: &mut TallyData) {
    let node = capture.pop().expect("capture stack is non-empty");
    match capture.last_mut() {
        Some(parent) => parent.children.push(node),
        None => read_object(&node, data),
    }
}

/// A captured element: just enough DOM to read one Tally object.
#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    text: String,
}

impl Node {
    fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }

    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.trim()).filter(|v| !v.is_empty())
    }

    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.is(name))
    }

    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.is(name))
    }

    /// Trimmed text of a direct child; `None` when missing or blank.
    fn text_of(&self, name: &str) -> Option<&str> {
        self.child(name).map(|c| c.text.trim()).filter(|t| !t.is_empty())
    }

    /// The object's name and aliases: the `NAME` attribute, a `NAME` child,
    /// or the `NAME` entries of `LANGUAGENAME.LIST/NAME.LIST`.
    fn names(&self) -> (Option<String>, Vec<String>) {
        let mut names: Vec<String> = Vec::new();
        let mut push = |n: &str| {
            let n = n.trim();
            if !n.is_empty() && !names.iter().any(|x| x.eq_ignore_ascii_case(n)) {
                names.push(n.to_owned());
            }
        };
        if let Some(n) = self.attr("NAME") {
            push(n);
        }
        if let Some(n) = self.text_of("NAME") {
            push(n);
        }
        let lists =
            self.children("NAME.LIST").chain(self.children("LANGUAGENAME.LIST").flat_map(|l| l.children("NAME.LIST")));
        for list in lists {
            for n in list.children("NAME") {
                push(&n.text);
            }
        }
        let mut it = names.into_iter();
        (it.next(), it.collect())
    }

    fn parent_name(&self) -> Option<String> {
        self.text_of("PARENT").filter(|p| normalize(p) != "primary").map(str::to_owned)
    }

    /// `GODOWNNAME`, `None` for the default godown.
    fn godown_name(&self) -> Option<&str> {
        self.text_of("GODOWNNAME").filter(|g| normalize(g) != DEFAULT_GODOWN)
    }

    /// `BATCHNAME`, `None` for the default batch.
    fn batch_name(&self) -> Option<&str> {
        self.text_of("BATCHNAME").filter(|n| normalize(n) != DEFAULT_BATCH)
    }

    /// Whether `BATCHALLOCATIONS.LIST` children name a godown other than
    /// Tally's default or a batch other than its default.
    fn godown_detail(&self) -> bool {
        self.children("BATCHALLOCATIONS.LIST").any(|b| b.godown_name().is_some() || b.batch_name().is_some())
    }
}

fn read_object(node: &Node, data: &mut TallyData) {
    let (Some(name), aliases) = node.names() else {
        if node.is("VOUCHER") {
            read_voucher(node, data);
        }
        return;
    };
    let reserved = node.attr("RESERVEDNAME").map(str::to_owned);
    if node.is("GROUP") {
        data.groups.push(TallyGroup { name, aliases, parent: node.parent_name(), reserved });
    } else if node.is("LEDGER") {
        let raw = node.text_of("OPENINGBALANCE").unwrap_or("");
        match tally_amount(raw) {
            Some(opening) => data.ledgers.push(TallyLedger {
                name,
                aliases,
                parent: node.parent_name(),
                reserved,
                opening: -opening,
            }),
            None => data.rejected.push(Skipped {
                kind: ItemKind::Ledger,
                name,
                reason: SkipReason::InvalidAmount(raw.to_owned()),
            }),
        }
    } else if node.is("VOUCHERTYPE") {
        data.voucher_types.push(TallyVoucherType { name, parent: node.parent_name() });
    } else if node.is("UNIT") {
        let base = node.text_of("BASEUNITS");
        let additional = node.text_of("ADDITIONALUNITS");
        data.units.push(TallyUnit {
            name,
            formal_name: node.text_of("ORIGINALNAME").unwrap_or("").to_owned(),
            decimals: node.text_of("DECIMALPLACES").and_then(|d| d.parse().ok()),
            compound: node.text_of("ISSIMPLEUNIT").is_some_and(|s| !tally_bool(s))
                || (base.is_some() && additional.is_some()),
        });
    } else if node.is("STOCKGROUP") {
        data.stock_groups.push(TallyStockGroup { name, aliases, parent: node.parent_name() });
    } else if node.is("STOCKITEM") {
        read_stock_item(node, name, aliases, data);
    } else if node.is("GODOWN") {
        if normalize(&name) != DEFAULT_GODOWN {
            let parent = node.parent_name().filter(|p| normalize(p) != DEFAULT_GODOWN);
            data.godowns.push(TallyGodown { name, aliases, parent });
        }
    } else if node.is("VOUCHER") {
        read_voucher(node, data);
    }
}

fn read_stock_item(node: &Node, name: String, aliases: Vec<String>, data: &mut TallyData) {
    let reject = |data: &mut TallyData, name: String, raw: &str| {
        data.rejected.push(Skipped {
            kind: ItemKind::StockItem,
            name,
            reason: SkipReason::InvalidAmount(raw.to_owned()),
        });
    };
    let raw_qty = node.text_of("OPENINGBALANCE").unwrap_or("");
    let Some((opening_qty, _)) = tally_qty(raw_qty) else {
        return reject(data, name, raw_qty);
    };
    let raw_value = node.text_of("OPENINGVALUE").unwrap_or("");
    let Some(value) = tally_amount(raw_value) else {
        return reject(data, name, raw_value);
    };
    let raw_rate = node.text_of("OPENINGRATE").unwrap_or("");
    let Some(rate) = tally_rate(raw_rate) else {
        return reject(data, name, raw_rate);
    };
    let opening_value = if value.is_zero() { (opening_qty.abs() * rate).round_dp(2) } else { value.abs() };
    let unit = node.text_of("BASEUNITS").filter(|u| normalize(u) != "notapplicable").map(str::to_owned);
    data.stock_items.push(TallyStockItem {
        name,
        aliases,
        parent: node.parent_name(),
        unit,
        opening_qty,
        opening_value,
        godown_detail: node.godown_detail(),
        hsn: hsn_code(node),
        costing: node.text_of("COSTINGMETHOD").map(str::to_owned),
    });
}

/// The item's HSN/SAC code: `HSNCODE`, else the last one given in
/// `GSTDETAILS.LIST` or `HSNDETAILS.LIST` (Tally lists them by date).
fn hsn_code(node: &Node) -> Option<String> {
    let nested = node
        .children("GSTDETAILS.LIST")
        .chain(node.children("HSNDETAILS.LIST"))
        .filter_map(|d| d.text_of("HSNCODE"))
        .last();
    node.text_of("HSNCODE").or(nested).map(str::to_owned)
}

/// Read one stock item line. `ledgers` are the ledgers its value goes to.
fn stock_line(node: &Node, ledgers: Vec<String>) -> Result<Option<TallyStockLine>, String> {
    let Some(item) = node.text_of("STOCKITEMNAME") else {
        return Ok(None);
    };
    let qty_of = |n: &Node| -> Result<Option<Quantity>, String> {
        match n.text_of("ACTUALQTY").or(n.text_of("BILLEDQTY")) {
            None => Ok(None),
            Some(raw) => tally_qty(raw)
                .map(|(q, _)| Some(q.abs()))
                .ok_or_else(|| format!("quantity {raw:?} of {item:?} is not a plain quantity")),
        }
    };
    let amount_of = |n: &Node| -> Result<Option<Amount>, String> {
        match n.text_of("AMOUNT") {
            None => Ok(None),
            Some(raw) => {
                tally_amount(raw).map(|a| Some(-a)).ok_or_else(|| format!("amount {raw:?} of {item:?} is not a number"))
            }
        }
    };
    let rate_of = |n: &Node| -> Result<Option<Amount>, String> {
        match n.text_of("RATE") {
            None => Ok(None),
            Some(raw) => tally_rate(raw).map(Some).ok_or_else(|| format!("rate {raw:?} of {item:?} is not a number")),
        }
    };
    let mut batches = Vec::new();
    for b in node.children("BATCHALLOCATIONS.LIST") {
        batches.push(TallyBatch {
            godown: b.godown_name().map(str::to_owned),
            batch: b.batch_name().map(str::to_owned),
            qty: qty_of(b)?.unwrap_or_default(),
            amount: amount_of(b)?.unwrap_or_default(),
            rate: rate_of(b)?,
        });
    }
    // Item totals, else the sum of its batch allocations.
    let qty = qty_of(node)?.unwrap_or_else(|| batches.iter().map(|b| b.qty).sum());
    let amount = amount_of(node)?.unwrap_or_else(|| batches.iter().map(|b| b.amount).sum());
    let rate = match rate_of(node)? {
        Some(r) => r,
        None if qty.is_zero() => Amount::ZERO,
        None => (amount.abs() / qty).round_dp(2),
    };
    Ok(Some(TallyStockLine {
        item: item.to_owned(),
        ledgers,
        qty: qty.normalize(),
        rate,
        amount,
        deemed_positive: node.text_of("ISDEEMEDPOSITIVE").map(tally_bool),
        batches,
    }))
}

/// Stock lines of a voucher: inventory entries (with their accounting
/// allocations) and inventory allocations nested in ledger lines.
fn stock_lines(node: &Node) -> Result<Vec<TallyStockLine>, String> {
    let mut out = Vec::new();
    for inv in node.children("ALLINVENTORYENTRIES.LIST").chain(node.children("INVENTORYENTRIES.LIST")) {
        let ledgers = inv
            .children("ACCOUNTINGALLOCATIONS.LIST")
            .filter_map(|a| a.text_of("LEDGERNAME"))
            .map(str::to_owned)
            .collect();
        out.extend(stock_line(inv, ledgers)?);
    }
    for entry in node.children("ALLLEDGERENTRIES.LIST").chain(node.children("LEDGERENTRIES.LIST")) {
        let Some(ledger) = entry.text_of("LEDGERNAME") else {
            continue;
        };
        for inv in entry.children("INVENTORYALLOCATIONS.LIST") {
            out.extend(stock_line(inv, vec![ledger.to_owned()])?);
        }
    }
    Ok(out)
}

/// Ledger lines directly inside `node`'s `list` children. Nested lists such
/// as bill, bank or cost-centre allocations are deliberately not descended.
fn entries_in<'a>(lists: impl Iterator<Item = &'a Node>) -> Result<Vec<TallyEntry>, String> {
    let mut out = Vec::new();
    for e in lists {
        let Some(ledger) = e.text_of("LEDGERNAME") else {
            continue;
        };
        let raw = e.text_of("AMOUNT").unwrap_or("");
        let amount = tally_amount(raw).ok_or_else(|| raw.to_owned())?;
        let deemed_positive = e.text_of("ISDEEMEDPOSITIVE").map(tally_bool);
        out.push(TallyEntry { ledger: ledger.to_owned(), amount: -amount, deemed_positive });
    }
    Ok(out)
}

fn balanced(entries: &[TallyEntry]) -> bool {
    !entries.is_empty() && entries.iter().map(|e| e.amount).sum::<Amount>().is_zero()
}

fn read_voucher(node: &Node, data: &mut TallyData) {
    let type_name = node.text_of("VOUCHERTYPENAME").or(node.attr("VCHTYPE")).unwrap_or("").to_owned();
    let number = node.text_of("VOUCHERNUMBER").unwrap_or("").to_owned();
    let raw_date = node.text_of("DATE").or(node.attr("DATE"));
    let label = |date: &str| {
        let mut l = type_name.clone();
        if !number.is_empty() {
            l.push_str(&format!(" {number}"));
        }
        if !date.is_empty() {
            l.push_str(&format!(" on {date}"));
        }
        l
    };
    let Some(raw_date) = raw_date else {
        data.rejected.push(Skipped { kind: ItemKind::Voucher, name: label(""), reason: SkipReason::MissingDate });
        return;
    };
    let Some(date) = tally_date(raw_date) else {
        data.rejected.push(Skipped {
            kind: ItemKind::Voucher,
            name: label(raw_date),
            reason: SkipReason::InvalidDate(raw_date.to_owned()),
        });
        return;
    };

    // Accounting vouchers carry ALLLEDGERENTRIES.LIST. Invoice-mode vouchers
    // carry LEDGERENTRIES.LIST plus the sales/purchase ledger inside each
    // inventory line's ACCOUNTINGALLOCATIONS.LIST. Use whichever balances.
    let inventory = || {
        node.children("ALLINVENTORYENTRIES.LIST")
            .chain(node.children("INVENTORYENTRIES.LIST"))
            .flat_map(|i| i.children("ACCOUNTINGALLOCATIONS.LIST"))
    };
    let all = entries_in(node.children("ALLLEDGERENTRIES.LIST"));
    let invoice = entries_in(node.children("LEDGERENTRIES.LIST").chain(inventory()));
    let all_plus_inventory = entries_in(node.children("ALLLEDGERENTRIES.LIST").chain(inventory()));
    let (all, invoice, mixed) = match (all, invoice, all_plus_inventory) {
        (Ok(a), Ok(b), Ok(c)) => (a, b, c),
        (Err(raw), ..) | (_, Err(raw), _) | (.., Err(raw)) => {
            data.rejected.push(Skipped {
                kind: ItemKind::Voucher,
                name: label(raw_date),
                reason: SkipReason::InvalidAmount(raw),
            });
            return;
        }
    };
    let entries = [&all, &invoice, &mixed]
        .into_iter()
        .find(|e| balanced(e))
        .or([&all, &invoice].into_iter().find(|e| !e.is_empty()))
        .cloned()
        .unwrap_or_default();
    let (inventory, inventory_problem) = match stock_lines(node) {
        Ok(lines) => (lines, None),
        Err(problem) => (Vec::new(), Some(problem)),
    };

    data.vouchers.push(TallyVoucher {
        type_name,
        number,
        date,
        narration: node.text_of("NARRATION").unwrap_or("").to_owned(),
        cancelled: node.text_of("ISCANCELLED").is_some_and(tally_bool),
        optional: node.text_of("ISOPTIONAL").is_some_and(tally_bool),
        entries,
        inventory,
        inventory_problem,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn envelope(body: &str) -> Vec<u8> {
        format!(
            "<ENVELOPE><HEADER><TALLYREQUEST>Import Data</TALLYREQUEST></HEADER><BODY><IMPORTDATA>\
             <REQUESTDESC><REPORTNAME>All Masters</REPORTNAME></REQUESTDESC><REQUESTDATA>\
             <TALLYMESSAGE xmlns:UDF=\"TallyUDF\">{body}</TALLYMESSAGE></REQUESTDATA></IMPORTDATA></BODY></ENVELOPE>"
        )
        .into_bytes()
    }

    #[test]
    fn reads_names_from_attribute_or_name_list() {
        let data = parse(&envelope(
            r#"<GROUP NAME="Current A/cs" RESERVEDNAME=""><PARENT>Bank Accounts</PARENT></GROUP>
               <GROUP><LANGUAGENAME.LIST><NAME.LIST TYPE="String"><NAME>Freight</NAME><NAME>Cartage</NAME></NAME.LIST></LANGUAGENAME.LIST>
                 <PARENT>&#4; Primary</PARENT></GROUP>"#,
        ))
        .unwrap();
        assert_eq!(data.groups[0].name, "Current A/cs");
        assert_eq!(data.groups[0].reserved, None);
        assert_eq!(data.groups[1].name, "Freight");
        assert_eq!(data.groups[1].aliases, ["Cartage"]);
        assert_eq!(data.groups[1].parent, None, "&#4; Primary means no parent");
    }

    #[test]
    fn opening_balance_sign_is_flipped() {
        let data = parse(&envelope(
            r#"<LEDGER NAME="Cash"><PARENT>Cash-in-Hand</PARENT><OPENINGBALANCE>-5,000.00</OPENINGBALANCE></LEDGER>
               <LEDGER NAME="Capital"><PARENT>Capital Account</PARENT><OPENINGBALANCE>5000.00</OPENINGBALANCE></LEDGER>
               <LEDGER NAME="Bad"><PARENT>Capital Account</PARENT><OPENINGBALANCE>lots</OPENINGBALANCE></LEDGER>"#,
        ))
        .unwrap();
        assert_eq!(data.ledgers[0].opening, dec!(5000), "Tally negative = debit = KhataOS positive");
        assert_eq!(data.ledgers[1].opening, dec!(-5000));
        assert_eq!(
            data.rejected,
            [Skipped { kind: ItemKind::Ledger, name: "Bad".into(), reason: SkipReason::InvalidAmount("lots".into()) }]
        );
    }

    #[test]
    fn nested_allocations_are_not_ledger_lines() {
        let data = parse(&envelope(
            r#"<VOUCHER VCHTYPE="Payment"><DATE>20240405</DATE><VOUCHERNUMBER>1</VOUCHERNUMBER>
                 <ALLLEDGERENTRIES.LIST><LEDGERNAME>Rent</LEDGERNAME><ISDEEMEDPOSITIVE>Yes</ISDEEMEDPOSITIVE><AMOUNT>-100.00</AMOUNT>
                   <CATEGORYALLOCATIONS.LIST><COSTCENTREALLOCATIONS.LIST><NAME>Office</NAME><AMOUNT>-100.00</AMOUNT></COSTCENTREALLOCATIONS.LIST></CATEGORYALLOCATIONS.LIST>
                 </ALLLEDGERENTRIES.LIST>
                 <ALLLEDGERENTRIES.LIST><LEDGERNAME>Cash</LEDGERNAME><ISDEEMEDPOSITIVE>No</ISDEEMEDPOSITIVE><AMOUNT>100.00</AMOUNT>
                   <BANKALLOCATIONS.LIST><AMOUNT>100.00</AMOUNT></BANKALLOCATIONS.LIST></ALLLEDGERENTRIES.LIST>
               </VOUCHER>"#,
        ))
        .unwrap();
        let v = &data.vouchers[0];
        assert_eq!(v.type_name, "Payment", "falls back to VCHTYPE");
        assert_eq!(v.entries.len(), 2);
        assert_eq!((v.entries[0].amount, v.entries[0].deemed_positive), (dec!(100), Some(true)));
        assert_eq!((v.entries[1].amount, v.entries[1].deemed_positive), (dec!(-100), Some(false)));
    }

    #[test]
    fn rejects_non_tally_xml_and_error_status() {
        assert_eq!(parse(b"<html/>"), Err(ImportError::NotTallyXml("html".into())));
        assert_eq!(parse(b"  "), Err(ImportError::Empty));
        assert!(matches!(parse(b"<ENVELOPE><BODY><TALLYMESSAGE><LEDGER NAME=\"x\">"), Err(ImportError::Xml { .. })));
        assert!(matches!(parse(b"<ENVELOPE><BODY></HEAD></ENVELOPE>"), Err(ImportError::Xml { .. })));
        let err = b"<ENVELOPE><HEADER><VERSION>1</VERSION><STATUS>0</STATUS></HEADER><BODY><DATA>Unknown Request</DATA></BODY></ENVELOPE>";
        assert_eq!(parse(err), Err(ImportError::TallyStatus("Unknown Request".into())));
    }

    #[test]
    fn reads_collection_exports() {
        let xml = br#"<ENVELOPE><HEADER><VERSION>1</VERSION><STATUS>1</STATUS></HEADER><BODY><DESC></DESC><DATA><COLLECTION>
            <LEDGER NAME="HDFC Bank" RESERVEDNAME=""><PARENT TYPE="String">Bank Accounts</PARENT><OPENINGBALANCE TYPE="Amount">-1000.00</OPENINGBALANCE></LEDGER>
            </COLLECTION></DATA></BODY></ENVELOPE>"#;
        let data = parse(xml).unwrap();
        assert_eq!(data.ledgers[0].opening, dec!(1000));
        assert_eq!(data.ledgers[0].parent.as_deref(), Some("Bank Accounts"));
    }
}
