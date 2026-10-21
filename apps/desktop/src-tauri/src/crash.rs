//! Crash reports (docs/crash-reporting.md): saved on this device, offered
//! as a pre-filled GitHub issue that the user reads and submits themselves.
//!
//! Nothing is sent anywhere by the app. A Rust panic (through the panic
//! hook) or an uncaught page error (`crash_record_js`) is written as a small
//! JSON file in `<app data>/crashes/`, readable by this OS user only. On the
//! next start the page offers to open a new-issue page on GitHub with the
//! report filled in; the user sees the exact text first.
//!
//! A report never holds company data. It has the app version, OS, arch,
//! platform, country pack code, display language, the error and where it
//! happened, the class names of the screens visited and the time. The text
//! goes through [`scrub`] twice, when saved and again when the issue link is
//! built: home folders become `~`, company files and names `<company>`,
//! and emails, tax IDs, long digit runs and amounts are taken out.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::api::ApiResult;

/// Where "Report a problem" opens a new issue: chosen when the app is built
/// (`KHATA_ISSUES_URL=https://github.com/<you>/<repo>/issues cargo build`),
/// KhataOS's own tracker otherwise.
pub const BUILD_ISSUES_URL: &str = match option_env!("KHATA_ISSUES_URL") {
    Some(url) => url,
    None => "https://github.com/khataos/khataos/issues",
};

/// The issue form in `.github/ISSUE_TEMPLATE/`; its field ids are the URL parameters.
pub const TEMPLATE: &str = "crash_report.yml";
/// Folder under the app data folder.
pub const DIR: &str = "crashes";
/// In the crash folder: `{"ask_on_start": false}` after "Don't ask again".
pub const SETTINGS_FILE: &str = "settings.json";
/// Crash files kept; older ones are deleted.
pub const KEEP: usize = 20;
/// GitHub refuses very long new-issue links; stay well under its limit.
pub const MAX_URL: usize = 7000;
/// Records written per run, so an error in a loop can't fill the folder.
const MAX_PER_RUN: usize = 5;
const MAX_SCREENS: usize = 12;
const MAX_MESSAGE: usize = 2000;
const MAX_STACK: usize = 8000;

// ---------------------------------------------------------------- records

/// One saved crash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct CrashRecord {
    /// File stem: `20261021T101500123Z-1a2b3c`.
    pub id: String,
    /// `panic` (Rust) or `js` (the page).
    pub kind: String,
    /// UTC, RFC 3339.
    pub at: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    /// `desktop`, `android` or `ios`.
    pub platform: String,
    /// Country pack code (`IN`, `AE`), empty with no company open.
    pub country: String,
    /// Display language (`en`, `ar`).
    pub language: String,
    pub message: String,
    /// `file:line:column` of a panic.
    pub location: String,
    pub stack: String,
    /// Class names of the last screens opened, oldest first.
    pub screens: Vec<String>,
    /// Same error, same place: used to skip repeats.
    pub signature: String,
    /// Reported or dismissed: no longer offered at start-up.
    pub handled: bool,
}

/// What the page knows: sent with each screen change and each page error.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Context {
    pub language: String,
    pub country: String,
    pub screens: Vec<String>,
}

impl Context {
    /// Keep only short codes and identifier-like class names.
    pub fn clean(mut self) -> Self {
        let code = |s: &str, max: usize| -> String { s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(max).collect() };
        self.language = code(&self.language, 12);
        self.country = code(&self.country, 3).to_ascii_uppercase();
        let names: Vec<String> = self
            .screens
            .iter()
            .filter(|s| !s.is_empty() && s.len() <= 48 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$'))
            .cloned()
            .collect();
        self.screens = names[names.len().saturating_sub(MAX_SCREENS)..].to_vec();
        self
    }
}

/// Start-up check: anything to offer, and whether to ask at all.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Pending {
    pub ask: bool,
    /// Newest first.
    pub crashes: Vec<CrashSummary>,
    /// Desktop only: `crash_open_folder` works.
    pub can_open_folder: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CrashSummary {
    pub id: String,
    pub kind: String,
    pub at: String,
    /// First line of the scrubbed message.
    pub message: String,
}

/// The issue as it will be filled in, for the review dialog, and its link.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Report {
    pub title: String,
    pub version: String,
    pub os: String,
    pub platform: String,
    pub country: String,
    pub language: String,
    /// The details textarea.
    pub details: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    ask_on_start: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { ask_on_start: true }
    }
}

pub fn platform() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else {
        "desktop"
    }
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn new_id() -> String {
    let now = chrono::Utc::now();
    format!("{}-{}", now.format("%Y%m%dT%H%M%S%3fZ"), &uuid::Uuid::new_v4().simple().to_string()[..6])
}

/// Ids come back from the page: never let one name another file.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 48 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn signature(kind: &str, message: &str, location: &str, stack: &str) -> String {
    let first = stack.lines().map(str::trim).find(|l| !l.is_empty() && *l != message).unwrap_or("");
    let digest = Sha256::digest(format!("{kind}\n{message}\n{location}\n{first}").as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// A new record for `kind`, scrubbed with `secrets`.
pub fn new_record(kind: &str, message: &str, location: &str, stack: &str, ctx: &Context, secrets: &Secrets) -> CrashRecord {
    let message = clip(&scrub(message, secrets), MAX_MESSAGE);
    let location = clip(&scrub(location, secrets), 300);
    let stack = clip(&scrub(stack, secrets), MAX_STACK);
    let ctx = ctx.clone().clean();
    CrashRecord {
        id: new_id(),
        kind: kind.to_owned(),
        at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        version: version().to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        platform: platform().to_owned(),
        country: ctx.country,
        language: ctx.language,
        signature: signature(kind, &message, &location, &stack),
        message,
        location,
        stack,
        screens: ctx.screens,
        handled: false,
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

// ------------------------------------------------------------------ files

fn crash_dir(app_dir: &Path) -> PathBuf {
    app_dir.join(DIR)
}

/// Every record, newest first. Damaged files are skipped.
pub fn list(app_dir: &Path) -> Vec<CrashRecord> {
    let Ok(read) = std::fs::read_dir(crash_dir(app_dir)) else {
        return Vec::new();
    };
    let mut out: Vec<CrashRecord> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?.to_owned();
            let mut r: CrashRecord = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
            r.id = stem;
            (valid_id(&r.id) && !r.kind.is_empty()).then_some(r)
        })
        .collect();
    out.sort_by(|a, b| b.id.cmp(&a.id));
    out
}

fn write(app_dir: &Path, r: &CrashRecord) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(r).map_err(std::io::Error::other)?;
    crate::mtd_client::write_private(&crash_dir(app_dir).join(format!("{}.json", r.id)), &json)
}

/// Save `r` unless an unhandled record has the same signature; keep the
/// newest [`KEEP`]. Returns whether it was written.
pub fn save(app_dir: &Path, r: &CrashRecord) -> std::io::Result<bool> {
    let existing = list(app_dir);
    if existing.iter().any(|x| !x.handled && x.signature == r.signature) {
        return Ok(false);
    }
    write(app_dir, r)?;
    for old in list(app_dir).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(crash_dir(app_dir).join(format!("{}.json", old.id)));
    }
    Ok(true)
}

fn load(app_dir: &Path, id: &str) -> ApiResult<CrashRecord> {
    if !valid_id(id) {
        return Err("no such crash report".into());
    }
    list(app_dir).into_iter().find(|r| r.id == id).ok_or_else(|| "that crash report is no longer on this device".into())
}

/// Mark records handled so start-up stops offering them.
pub fn dismiss(app_dir: &Path, ids: &[String]) -> ApiResult<()> {
    for mut r in list(app_dir).into_iter().filter(|r| ids.contains(&r.id) && !r.handled) {
        r.handled = true;
        write(app_dir, &r).map_err(|e| format!("cannot update the crash report: {e}"))?;
    }
    Ok(())
}

pub fn pending(app_dir: &Path) -> Pending {
    let secrets = Secrets::current();
    Pending {
        ask: ask_on_start(app_dir),
        crashes: list(app_dir)
            .into_iter()
            .filter(|r| !r.handled)
            .map(|r| CrashSummary {
                message: clip(scrub(&r.message, &secrets).lines().next().unwrap_or(""), 200),
                id: r.id,
                kind: r.kind,
                at: r.at,
            })
            .collect(),
        can_open_folder: cfg!(desktop),
    }
}

fn ask_on_start(app_dir: &Path) -> bool {
    std::fs::read(crash_dir(app_dir).join(SETTINGS_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .unwrap_or_default()
        .ask_on_start
}

fn set_ask_on_start(app_dir: &Path, ask: bool) -> ApiResult<()> {
    let json = serde_json::to_vec_pretty(&Settings { ask_on_start: ask }).map_err(|e| e.to_string())?;
    crate::mtd_client::write_private(&crash_dir(app_dir).join(SETTINGS_FILE), &json).map_err(|e| format!("cannot save the setting: {e}"))?;
    RECORDING.store(ask, Ordering::Relaxed);
    Ok(())
}

// --------------------------------------------------------------- scrubbing

/// Text to hide besides the patterns [`scrub`] always takes out.
#[derive(Debug, Clone, Default)]
pub struct Secrets {
    /// Home folders: become `~`.
    pub home: Vec<String>,
    /// Company file paths, file names and company names: become `<company>`.
    pub company: Vec<String>,
    /// OS and company user names: become `<user>`.
    pub users: Vec<String>,
}

impl Secrets {
    /// This device's: home folder, OS user, and the companies seen this run.
    pub fn current() -> Self {
        let mut s = LIVE.lock().map(|l| l.secrets.clone()).unwrap_or_default();
        s.add_env();
        s
    }

    fn add_env(&mut self) {
        for k in ["HOME", "USERPROFILE"] {
            if let Ok(v) = std::env::var(k) {
                self.home.push(v);
            }
        }
        for k in ["USER", "USERNAME", "LOGNAME"] {
            if let Ok(v) = std::env::var(k) {
                self.users.push(v);
            }
        }
    }

    /// Remember an open company's path and name.
    pub fn add_company(&mut self, path: &str, name: &str) {
        let mut add = |v: &str| {
            let v = v.trim();
            if v.chars().count() >= 3 && !self.company.iter().any(|x| x == v) {
                self.company.push(v.to_owned());
            }
        };
        add(path);
        add(name);
        if let Some(file) = Path::new(path).file_name().and_then(|f| f.to_str()) {
            add(file);
        }
        if let Some(stem) = Path::new(path).file_stem().and_then(|f| f.to_str()) {
            add(stem);
        }
        // Bounded: a long session switching companies keeps the most recent.
        let n = self.company.len();
        if n > 32 {
            self.company.drain(..n - 32);
        }
    }
}

/// Take company data and personal details out of an error message or stack.
pub fn scrub(text: &str, s: &Secrets) -> String {
    let mut out = text.replace('\0', "");
    // Known strings, longest first, so a company path goes before its home folder.
    let mut exact: Vec<(String, &str)> = Vec::new();
    for c in &s.company {
        exact.push((c.clone(), "<company>"));
    }
    for h in &s.home {
        exact.push((h.trim_end_matches(['/', '\\']).to_owned(), "~"));
    }
    for u in &s.users {
        exact.push((u.clone(), "<user>"));
    }
    // Rust's `{:?}` doubles Windows backslashes.
    let doubled: Vec<(String, &str)> = exact.iter().filter(|(k, _)| k.contains('\\')).map(|(k, v)| (k.replace('\\', "\\\\"), *v)).collect();
    exact.extend(doubled);
    exact.retain(|(k, _)| k.chars().count() >= 3);
    exact.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    for (k, v) in &exact {
        out = replace_ignore_case(&out, k, v);
    }
    out = home_paths(&out);
    out = company_files(&out);
    out = quoted(&out);
    out = emails(&out);
    out = amounts(&out);
    out = digit_groups(&out);
    words(&out)
}

fn replace_ignore_case(hay: &str, needle: &str, with: &str) -> String {
    if needle.is_empty() {
        return hay.to_owned();
    }
    let lower_hay = hay.to_lowercase();
    let lower_needle = needle.to_lowercase();
    // Lower-casing can change byte lengths outside ASCII; fall back to exact.
    if lower_hay.len() != hay.len() || lower_needle.len() != needle.len() {
        return if hay.contains(needle) { hay.replace(needle, with) } else { hay.to_owned() };
    }
    // A name inside a longer word ("dev" in "device") stays.
    let word_start = needle.chars().next().is_some_and(char::is_alphanumeric);
    let word_end = needle.chars().next_back().is_some_and(char::is_alphanumeric);
    let mut out = String::with_capacity(hay.len());
    let mut last = 0;
    for (i, _) in lower_hay.match_indices(&lower_needle) {
        let before = hay[..i].chars().next_back().is_some_and(char::is_alphanumeric);
        let after = hay[i + needle.len()..].chars().next().is_some_and(char::is_alphanumeric);
        if i < last || (word_start && before) || (word_end && after) {
            continue;
        }
        out.push_str(&hay[last..i]);
        out.push_str(with);
        last = i + needle.len();
    }
    out.push_str(&hay[last..]);
    out
}

/// Ends a path or file token.
fn stops_token(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '<' | '>' | '=' | ',' | ';' | '{' | '}' | '|')
}

/// `/Users/<name>`, `/home/<name>`, `C:\Users\<name>` → `~`.
fn home_paths(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    'outer: while i < c.len() {
        for pat in ["/Users/", "/home/", "\\Users\\", "\\\\Users\\\\", "/Users\\", "\\Users/"] {
            let p: Vec<char> = pat.chars().collect();
            if i + p.len() <= c.len() && c[i..i + p.len()].iter().zip(&p).all(|(a, b)| a.eq_ignore_ascii_case(b)) {
                let mut j = i + p.len();
                let start = j;
                while j < c.len() && !matches!(c[j], '/' | '\\') && !stops_token(c[j]) && c[j] != ':' {
                    j += 1;
                }
                if j > start {
                    // A drive letter before `\Users\` goes too.
                    if out.len() >= 2 && out.ends_with(':') {
                        let mut it = out.chars().rev();
                        it.next();
                        if it.next().is_some_and(|d| d.is_ascii_alphabetic()) && it.next().is_none_or(|x| !x.is_ascii_alphanumeric()) {
                            out.pop();
                            out.pop();
                        }
                    }
                    out.push('~');
                    i = j;
                    continue 'outer;
                }
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// Any token ending `.khata` or `.khatapkg` (a company file, with its path) → `<company>`.
fn company_files(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let lower: Vec<char> = c.iter().map(|x| x.to_ascii_lowercase()).collect();
    let ext: Vec<char> = ".khata".chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(c.len());
    let mut i = 0;
    while i < c.len() {
        if i + ext.len() <= c.len() && lower[i..i + ext.len()] == ext[..] {
            let mut end = i + ext.len();
            if end + 3 <= c.len() && lower[end..end + 3] == ['p', 'k', 'g'] {
                end += 3;
            }
            if end == c.len() || !c[end].is_alphanumeric() {
                // Walk back over the file name and its folders.
                while out.last().is_some_and(|&x| !stops_token(x)) {
                    out.pop();
                }
                out.extend("<company>".chars());
                i = end;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out.into_iter().collect()
}

/// Quoted text that reads like a name or a value → `"…"`. Kept: one
/// identifier-like word (`'length'`), or a lower-case phrase after its
/// first letter (`"No such file or directory"`).
fn quoted(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < c.len() {
        let open = c[i];
        let close = match open {
            '"' => Some('"'),
            '\'' if i == 0 || !c[i - 1].is_alphanumeric() => Some('\''),
            '“' => Some('”'),
            '‘' => Some('’'),
            '«' => Some('»'),
            _ => None,
        };
        if let Some(close) = close {
            let end = (i + 1..c.len()).find(|&j| c[j] == close && (close != '\'' || j + 1 == c.len() || !c[j + 1].is_alphanumeric()));
            if let Some(end) = end {
                let inner: String = c[i + 1..end].iter().collect();
                out.push(open);
                out.push_str(if keep_quoted(&inner) { &inner } else { "…" });
                out.push(close);
                i = end + 1;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

fn keep_quoted(s: &str) -> bool {
    if s.is_empty() || s == "<company>" || s == "~" {
        return true;
    }
    if !s.is_ascii() || s.len() > 100 {
        return false;
    }
    if !s.contains(char::is_whitespace) {
        return s.chars().all(|c| c.is_ascii_alphanumeric() || "_.:$-<>/\\~".contains(c));
    }
    let mut chars = s.chars();
    chars.next();
    chars.all(|c| !c.is_ascii_uppercase() && !c.is_ascii_digit())
}

/// `name@example.com` → `<email>`.
fn emails(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let local = |x: char| x.is_ascii_alphanumeric() || "._%+-".contains(x);
    let domain = |x: char| x.is_ascii_alphanumeric() || ".-".contains(x);
    let mut out: Vec<char> = Vec::with_capacity(c.len());
    let mut i = 0;
    while i < c.len() {
        if c[i] == '@' {
            let left = out.iter().rev().take_while(|&&x| local(x)).count();
            let mut j = i + 1;
            while j < c.len() && domain(c[j]) {
                j += 1;
            }
            let mut end = j;
            while end > i + 1 && c[end - 1] == '.' {
                end -= 1;
            }
            let dom: String = c[i + 1..end].iter().collect();
            if left > 0 && dom.contains('.') && !dom.starts_with('.') {
                out.truncate(out.len() - left);
                out.extend("<email>".chars());
                i = end;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out.into_iter().collect()
}

fn is_digit(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_ascii_digit())
}

/// `12,500.00`, `1,23,456`, `99.5` → `<amount>`. Dotted versions
/// (`2.12.1`) and digits inside words stay.
fn amounts(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < c.len() {
        let starts = c[i].is_ascii_digit() && (i == 0 || (!c[i - 1].is_alphanumeric() && c[i - 1] != '.' && c[i - 1] != '_'));
        if !starts {
            out.push(c[i]);
            i += 1;
            continue;
        }
        let mut j = i;
        while is_digit(c.get(j)) {
            j += 1;
        }
        let lead = j - i;
        let mut grouped = false;
        // Thousands (or Indian lakh) groups.
        loop {
            if c.get(j) == Some(&',') {
                let mut k = j + 1;
                while is_digit(c.get(k)) {
                    k += 1;
                }
                let n = k - j - 1;
                if (2..=3).contains(&n) && lead <= 3 {
                    grouped = true;
                    j = k;
                    continue;
                }
            }
            break;
        }
        let mut decimal = false;
        if c.get(j) == Some(&'.') && is_digit(c.get(j + 1)) {
            let mut k = j + 1;
            while is_digit(c.get(k)) {
                k += 1;
            }
            if c.get(k) == Some(&'.') && is_digit(c.get(k + 1)) {
                // A version or address: copy the whole dotted run.
                while c.get(k).is_some_and(|x| x.is_ascii_digit() || *x == '.') {
                    k += 1;
                }
                out.extend(&c[i..k]);
                i = k;
                continue;
            }
            decimal = true;
            j = k;
        }
        let ends = c.get(j).is_none_or(|x| !x.is_alphanumeric() && *x != '_');
        if (grouped || decimal) && ends {
            out.push_str("<amount>");
        } else {
            out.extend(&c[i..j]);
        }
        i = j;
    }
    out
}

/// Digit groups joined by `-`, `/` or a space with nine digits or more in
/// all (`123-45-6789`, `98765 43210`, card numbers) → `<number>`.
fn digit_groups(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < c.len() {
        if !(c[i].is_ascii_digit() && (i == 0 || !c[i - 1].is_alphanumeric())) {
            out.push(c[i]);
            i += 1;
            continue;
        }
        let (mut j, mut digits, mut groups) = (i, 0, 0);
        let end = loop {
            let g = j;
            while is_digit(c.get(j)) {
                j += 1;
            }
            digits += j - g;
            groups += 1;
            if c.get(j).is_some_and(|x| matches!(x, '-' | '/' | ' ')) && is_digit(c.get(j + 1)) {
                j += 1;
            } else {
                break j;
            }
        };
        let bounded = c.get(end).is_none_or(|x| !x.is_alphanumeric());
        if groups >= 2 && digits >= 9 && bounded {
            out.push_str("<number>");
            i = end;
        } else {
            // Only the first group: later ones get their own look.
            let mut k = i;
            while is_digit(c.get(k)) {
                k += 1;
            }
            out.extend(&c[i..k]);
            i = k;
        }
    }
    out
}

/// Per word (a run of ASCII letters and digits): GSTIN, PAN, other
/// upper-case tax IDs (TRN, VAT, RFC, EIN) → `<tax-id>`; more than six
/// digits in a row → `<number>`.
fn words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |w: &mut String, out: &mut String| {
        if !w.is_empty() {
            out.push_str(classify(w).unwrap_or(w));
            w.clear();
        }
    };
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            word.push(ch);
        } else {
            flush(&mut word, &mut out);
            out.push(ch);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn classify(w: &str) -> Option<&'static str> {
    let b = w.as_bytes();
    let digits = b.iter().filter(|c| c.is_ascii_digit()).count();
    let upper = b.iter().filter(|c| c.is_ascii_uppercase()).count();
    let lower = b.iter().filter(|c| c.is_ascii_lowercase()).count();
    let run = b.split(|c| !c.is_ascii_digit()).map(<[u8]>::len).max().unwrap_or(0);
    let pat = |p: &str| p.len() == b.len() && p.bytes().zip(b).all(|(k, c)| match k {
        b'9' => c.is_ascii_digit(),
        b'A' => c.is_ascii_uppercase(),
        b'X' => c.is_ascii_uppercase() || c.is_ascii_digit(),
        _ => k == *c,
    });
    if pat("99AAAAA9999AXZX") || pat("99AAAAA9999AXXX") || pat("AAAAA9999A") {
        return Some("<tax-id>");
    }
    if lower == 0 && upper >= 2 && digits >= 5 && (8..=20).contains(&b.len()) {
        return Some("<tax-id>");
    }
    if run > 6 {
        return Some(if digits == b.len() { "<number>" } else { "<id>" });
    }
    None
}

// ------------------------------------------------------------------ issue

/// Percent-encode for a query value (RFC 3986 unreserved characters stay).
fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn os_label(os: &str, arch: &str) -> String {
    format!("{os} {arch}")
}

const ASK_STEPS: &str = "What were you doing when it happened? Please list the steps. Do not paste company data, names or amounts.";
const ASK_PROBLEM: &str = "What happened, and what did you expect? Please list the steps. Do not paste company data, names or amounts.";

struct Parts {
    crash: Option<(String, String, String)>, // kind, at, location
    message: String,
    stack: Vec<String>,
    screens: Vec<String>,
}

fn details(p: &Parts) -> String {
    let mut d = String::new();
    d.push_str(if p.crash.is_some() { ASK_STEPS } else { ASK_PROBLEM });
    d.push_str("\n\n\n---\n");
    if let Some((kind, at, location)) = &p.crash {
        d.push_str("Crash details, collected by KhataOS with company data taken out:\n\n");
        d.push_str(&format!("- Kind: {}\n", if kind == "panic" { "Rust panic" } else { "JavaScript error" }));
        d.push_str(&format!("- Time: {at}\n"));
        if !location.is_empty() {
            d.push_str(&format!("- Location: `{location}`\n"));
        }
    }
    if !p.screens.is_empty() {
        d.push_str(&format!("- Screens: {}\n", p.screens.join(" > ")));
    }
    if !p.message.is_empty() {
        d.push_str(&format!("\nMessage:\n```\n{}\n```\n", p.message));
    }
    if !p.stack.is_empty() {
        d.push_str(&format!("\nStack:\n```\n{}\n```\n", p.stack.join("\n")));
    }
    d
}

fn url_of(base: &str, r: &Report) -> String {
    format!(
        "{}/new?template={}&title={}&version={}&os={}&platform={}&country={}&language={}&details={}",
        base.trim_end_matches('/'),
        enc(TEMPLATE),
        enc(&r.title),
        enc(&r.version),
        enc(&r.os),
        enc(&r.platform),
        enc(&r.country),
        enc(&r.language),
        enc(&r.details)
    )
}

/// The issue for `crash` (or a blank problem report with `None`), scrubbed
/// once more and cut to fit [`MAX_URL`]: the stack goes first, then the
/// message, then the screens.
pub fn build_report(base: &str, crash: Option<&CrashRecord>, ctx: &Context, secrets: &Secrets) -> Report {
    let ctx = ctx.clone().clean();
    let sc = |s: &str| scrub(s, secrets);
    let (mut r, mut parts) = match crash {
        Some(c) => {
            let message = sc(&c.message);
            let first = message.lines().next().unwrap_or("").trim().to_owned();
            (
                Report {
                    title: format!("Crash: {}", clip(&first, 80)),
                    version: c.version.clone(),
                    os: os_label(&c.os, &c.arch),
                    platform: c.platform.clone(),
                    country: c.country.clone(),
                    language: c.language.clone(),
                    ..Report::default()
                },
                Parts {
                    crash: Some((c.kind.clone(), c.at.clone(), sc(&c.location))),
                    stack: sc(&c.stack).lines().map(str::to_owned).filter(|l| l.trim() != message.trim()).collect(),
                    message,
                    screens: Context { screens: c.screens.clone(), ..Context::default() }.clean().screens,
                },
            )
        }
        None => (
            Report {
                title: "Problem: ".into(),
                version: version().to_owned(),
                os: os_label(std::env::consts::OS, std::env::consts::ARCH),
                platform: platform().to_owned(),
                country: ctx.country.clone(),
                language: ctx.language.clone(),
                ..Report::default()
            },
            Parts { crash: None, message: String::new(), stack: Vec::new(), screens: ctx.screens.clone() },
        ),
    };
    let full_stack = std::mem::take(&mut parts.stack);
    let mut keep = full_stack.len();
    loop {
        parts.stack = full_stack[..keep].to_vec();
        if keep < full_stack.len() && keep > 0 {
            parts.stack.push("…".into());
        }
        r.details = details(&parts);
        r.url = url_of(base, &r);
        if r.url.len() <= MAX_URL {
            return r;
        }
        if keep > 0 {
            // The stack goes first, a quarter at a time, from the end.
            keep -= (keep / 4).max(1);
        } else if parts.message.chars().count() > 40 {
            let n = parts.message.chars().count() * 3 / 4;
            let cut: String = parts.message.chars().take(n).collect();
            parts.message = format!("{}…", cut.trim_end_matches('…'));
        } else if !parts.screens.is_empty() {
            parts.screens.remove(0);
        } else {
            // Nothing left to cut but the title.
            r.title = clip(&r.title, 40);
            r.details = details(&parts);
            r.url = url_of(base, &r);
            return r;
        }
    }
}

/// Only our own new-issue links may be opened from the page.
fn allowed_url(url: &str) -> bool {
    let base = format!("{}/new?", BUILD_ISSUES_URL.trim_end_matches('/'));
    url.starts_with(&base) && url.starts_with("https://") && url.len() <= MAX_URL + 200 && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

// -------------------------------------------------------- the panic hook

/// Shared with the panic hook.
struct Live {
    ctx: Context,
    secrets: Secrets,
}

static LIVE: Mutex<Live> = Mutex::new(Live { ctx: Context { language: String::new(), country: String::new(), screens: Vec::new() }, secrets: Secrets { home: Vec::new(), company: Vec::new(), users: Vec::new() } });
static APP_DIR: OnceLock<PathBuf> = OnceLock::new();
/// Off after "Don't ask again": nothing is written.
static RECORDING: AtomicBool = AtomicBool::new(true);
static WRITTEN: AtomicUsize = AtomicUsize::new(0);
static IN_HOOK: AtomicBool = AtomicBool::new(false);

/// Install the panic hook (chained to the one before it). Call once, from setup.
pub fn install(app_dir: PathBuf) {
    RECORDING.store(ask_on_start(&app_dir), Ordering::Relaxed);
    if APP_DIR.set(app_dir).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        if IN_HOOK.swap(true, Ordering::SeqCst) {
            return; // a panic while saving a panic: give up quietly
        }
        record_panic(info);
        IN_HOOK.store(false, Ordering::SeqCst);
    }));
}

fn record_panic(info: &std::panic::PanicHookInfo<'_>) {
    let Some(dir) = APP_DIR.get() else { return };
    if !RECORDING.load(Ordering::Relaxed) || WRITTEN.load(Ordering::Relaxed) >= MAX_PER_RUN {
        return;
    }
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(panic without a message)".into());
    let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
    // Never block in a panic: the panicking thread may hold the lock.
    let (ctx, mut secrets) = match LIVE.try_lock() {
        Ok(l) => (l.ctx.clone(), l.secrets.clone()),
        Err(_) => (Context::default(), Secrets::default()),
    };
    secrets.add_env();
    let thread = std::thread::current().name().map(|n| format!("thread '{n}'")).unwrap_or_default();
    let r = new_record("panic", &message, &location, &thread, &ctx, &secrets);
    if matches!(save(dir, &r), Ok(true)) {
        WRITTEN.fetch_add(1, Ordering::Relaxed);
    }
}

/// A page error: saved unless recording is off, it repeats one already
/// waiting, or this run has saved [`MAX_PER_RUN`] already.
pub fn record_js(app_dir: &Path, message: &str, stack: &str, ctx: &Context) -> bool {
    if !RECORDING.load(Ordering::Relaxed) || WRITTEN.load(Ordering::Relaxed) >= MAX_PER_RUN {
        return false;
    }
    let r = new_record("js", message, "", stack, ctx, &Secrets::current());
    let saved = matches!(save(app_dir, &r), Ok(true));
    if saved {
        WRITTEN.fetch_add(1, Ordering::Relaxed);
    }
    saved
}

/// Remember what the page shows, for the panic hook.
pub fn set_context(ctx: Context, company: Option<(String, String)>, user: Option<String>) {
    if let Ok(mut l) = LIVE.lock() {
        l.ctx = ctx.clean();
        if let Some((path, name)) = company {
            l.secrets.add_company(&path, &name);
        }
        if let Some(u) = user.filter(|u| u.chars().count() >= 3 && !l.secrets.users.contains(u)) {
            l.secrets.users.push(u);
        }
    }
}

/// Open a link in the browser: the desktop's own (`mtd_client`), or the
/// phone's through tauri-plugin-opener.
fn open_external<R: tauri::Runtime>(app: &tauri::AppHandle<R>, target: &str) -> ApiResult<()> {
    #[cfg(mobile)]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(target, None::<&str>).map_err(|e| format!("couldn't open your web browser ({e})"))
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        crate::mtd_client::open_in_browser(target)
    }
}

// ---------------------------------------------------------------- commands

pub mod cmd {
    use tauri::{AppHandle, Manager, State};

    use super::*;
    use crate::AppState;

    fn app_dir(app: &AppHandle) -> ApiResult<PathBuf> {
        app.path().app_data_dir().map_err(|e| e.to_string())
    }

    /// Crashes not yet reported or dismissed, and whether to ask about them.
    #[tauri::command(rename_all = "snake_case")]
    pub fn crash_pending(app: AppHandle) -> ApiResult<Pending> {
        Ok(pending(&app_dir(&app)?))
    }

    /// The page's screens and language, for reports. Never waits for the company lock.
    #[tauri::command(rename_all = "snake_case")]
    pub fn crash_context(state: State<'_, AppState>, context: Context) {
        let path = state.path.try_lock().ok().and_then(|p| p.as_ref().map(|p| p.display().to_string()));
        let name = state.company.try_lock().ok().and_then(|s| s.as_ref().map(|(_, b)| b.name.clone()));
        let company = match (path, name) {
            (None, None) => None,
            (p, n) => Some((p.unwrap_or_default(), n.unwrap_or_default())),
        };
        set_context(context, company, crate::security::signed_in_name());
    }

    /// Save an uncaught page error. Returns whether it was saved.
    #[tauri::command(rename_all = "snake_case")]
    pub fn crash_record_js(app: AppHandle, message: String, stack: String, context: Context) -> ApiResult<bool> {
        Ok(record_js(&app_dir(&app)?, &message, &stack, &context))
    }

    /// The issue for crash `id`, or a blank problem report: the text the user
    /// reviews and the link that fills it in on GitHub.
    #[tauri::command(rename_all = "snake_case")]
    pub fn crash_report_url(app: AppHandle, id: Option<String>, context: Context) -> ApiResult<Report> {
        let crash = match id.as_deref().filter(|s| !s.is_empty()) {
            Some(id) => Some(load(&app_dir(&app)?, id)?),
            None => None,
        };
        Ok(build_report(BUILD_ISSUES_URL, crash.as_ref(), &context, &Secrets::current()))
    }

    /// Open a link from `crash_report_url` in the browser.
    #[tauri::command(rename_all = "snake_case")]
    pub fn crash_open_url(app: AppHandle, url: String) -> ApiResult<()> {
        if !allowed_url(&url) {
            return Err("that is not a KhataOS issue link".into());
        }
        open_external(&app, &url)
    }

    /// Stop offering these crashes; `never_ask` also turns the start-up
    /// question and crash saving off ("Don't ask again").
    #[tauri::command(rename_all = "snake_case")]
    pub fn crash_dismiss(app: AppHandle, ids: Vec<String>, never_ask: bool) -> ApiResult<()> {
        let dir = app_dir(&app)?;
        dismiss(&dir, &ids)?;
        if never_ask {
            set_ask_on_start(&dir, false)?;
        }
        Ok(())
    }

    /// Show the crash folder in the file manager (desktop).
    #[tauri::command(rename_all = "snake_case")]
    pub fn crash_open_folder(app: AppHandle) -> ApiResult<()> {
        if cfg!(mobile) {
            return Err("crash files can't be shown on a phone".into());
        }
        let dir = crash_dir(&app_dir(&app)?);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        open_external(&app, &dir.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> Secrets {
        let mut s = Secrets {
            home: vec!["/Users/krunal".into()],
            company: Vec::new(),
            users: vec!["krunal".into()],
        };
        s.add_company("/Users/krunal/Documents/KhataOS/sharma-sons.khata", "Sharma & Sons Pvt Ltd");
        s
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("khataos-crash-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn home_folders_become_tilde() {
        let s = Secrets::default();
        assert_eq!(scrub("open /Users/alice/Library/x.log failed", &s), "open ~/Library/x.log failed");
        assert_eq!(scrub("at /home/bob/.cache/k", &s), "at ~/.cache/k");
        assert_eq!(scrub(r"C:\Users\Asha\AppData\x", &s), r"~\AppData\x");
        assert_eq!(scrub(r#"path: "C:\\Users\\Asha\\AppData""#, &s), r#"path: "~\\AppData""#);
        // The known home folder, even outside /Users.
        let s = Secrets { home: vec!["/var/mobile/Containers/Data/Application/ABC".into()], ..Secrets::default() };
        assert_eq!(scrub("/var/mobile/Containers/Data/Application/ABC/Library", &s), "~/Library");
    }

    #[test]
    fn company_files_names_and_users_go() {
        let s = secrets();
        assert_eq!(scrub("cannot open /Users/krunal/Documents/KhataOS/sharma-sons.khata: locked", &s), "cannot open <company>: locked");
        assert_eq!(scrub("cannot open /tmp/other books.khata", &s), "cannot open /tmp/other <company>");
        assert_eq!(scrub("read D:/Backups/acme.KHATA failed", &s), "read <company> failed");
        assert_eq!(scrub("file acme-2026.khatapkg is damaged", &s), "file <company> is damaged");
        assert_eq!(scrub("company SHARMA & SONS PVT LTD has no ledgers", &s), "company <company> has no ledgers");
        assert_eq!(scrub("signed in as krunal", &s), "signed in as <user>");
        // The file name without its folder, and its stem.
        assert_eq!(scrub("sharma-sons.khata-wal", &s), "<company>-wal");
        assert_eq!(scrub("slug sharma-sons taken", &s), "slug <company> taken");
        // `.khata` inside a longer word is not a company file.
        assert_eq!(scrub("crate khatacore and .khataos", &Secrets::default()), "crate khatacore and .khataos");
    }

    #[test]
    fn emails_go() {
        let s = Secrets::default();
        assert_eq!(scrub("mail accounts@sharma.co.in bounced.", &s), "mail <email> bounced.");
        assert_eq!(scrub("a.b+c@x-y.example.com", &s), "<email>");
        // WebKit stack frames are `fn@url`, not emails.
        assert_eq!(scrub("render@tauri://localhost/assets/index.js:1:20", &s), "render@tauri://localhost/assets/index.js:1:20");
        assert_eq!(scrub("@decorator and x@y", &s), "@decorator and x@y");
    }

    #[test]
    fn tax_ids_go() {
        let s = Secrets::default();
        assert_eq!(scrub("GSTIN 27AAPFU0939F1ZV invalid", &s), "GSTIN <tax-id> invalid");
        assert_eq!(scrub("PAN AAPFU0939F", &s), "PAN <tax-id>");
        assert_eq!(scrub("TRN 100123456700003", &s), "TRN <number>");
        assert_eq!(scrub("VAT GB123456789 and DE123456789", &s), "VAT <tax-id> and <tax-id>");
        assert_eq!(scrub("NL123456789B01", &s), "<tax-id>");
        assert_eq!(scrub("RFC GODE561231GR8", &s), "RFC <tax-id>");
        assert_eq!(scrub("SSN 123-45-6789, EIN 12-3456789", &s), "SSN <number>, EIN <number>");
    }

    #[test]
    fn long_digit_runs_go_short_ones_stay() {
        let s = Secrets::default();
        assert_eq!(scrub("phone 9876543210", &s), "phone <number>");
        assert_eq!(scrub("phone 98765 43210", &s), "phone <number>");
        assert_eq!(scrub("acct ab12345678", &s), "acct <id>");
        assert_eq!(scrub("index 123456 out of range", &s), "index 123456 out of range");
        assert_eq!(scrub("src/api.rs:1203:17", &s), "src/api.rs:1203:17");
        assert_eq!(scrub("on 2026-10-21", &s), "on 2026-10-21");
    }

    #[test]
    fn amounts_go_versions_stay() {
        let s = Secrets::default();
        assert_eq!(scrub("total 12,500.00 does not tally with 99.5", &s), "total <amount> does not tally with <amount>");
        assert_eq!(scrub("₹1,23,456 Dr", &s), "₹<amount> Dr");
        assert_eq!(scrub("-1234567.89", &s), "-<amount>");
        assert_eq!(scrub("tauri 2.12.1 on 10.0.0.5", &s), "tauri 2.12.1 on 10.0.0.5");
        assert_eq!(scrub("x1.5 and v0.1", &s), "x1.5 and v0.1");
        assert_eq!(scrub("list 1,2,3", &s), "list 1,2,3");
    }

    #[test]
    fn quoted_values_go_messages_stay() {
        let s = Secrets::default();
        assert_eq!(scrub(r#"ledger "Rahul Traders" not found"#, &s), r#"ledger "…" not found"#);
        assert_eq!(scrub("Cannot read properties of undefined (reading 'length')", &s), "Cannot read properties of undefined (reading 'length')");
        assert_eq!(scrub(r#"Os { message: "No such file or directory" }"#, &s), r#"Os { message: "No such file or directory" }"#);
        assert_eq!(scrub("couldn't parse 'شركة'", &s), "couldn't parse '…'");
        assert_eq!(scrub("name “Mehta Bros”", &s), "name “…”");
    }

    #[test]
    fn scrub_is_stable() {
        let s = secrets();
        let once = scrub("open /Users/krunal/x.khata total 1,000.50 for a@b.in GSTIN 27AAPFU0939F1ZV", &s);
        assert_eq!(once, "open <company> total <amount> for <email> GSTIN <tax-id>");
        assert_eq!(scrub(&once, &s), once);
    }

    #[test]
    fn context_keeps_only_codes_and_class_names() {
        let c = Context {
            language: "ar\"><script>".into(),
            country: "in".into(),
            screens: vec!["HomeScreen".into(), "Ledger: Sharma".into(), "VoucherScreen".into()],
        }
        .clean();
        assert_eq!(c.language, "arscript");
        assert_eq!(c.country, "IN");
        assert_eq!(c.screens, ["HomeScreen", "VoucherScreen"]);
        let many = Context { screens: (0..30).map(|i| format!("S{i}")).collect(), ..Context::default() }.clean();
        assert_eq!(many.screens.len(), MAX_SCREENS);
        assert_eq!(many.screens.last().unwrap(), "S29");
    }

    fn sample(secrets: &Secrets) -> CrashRecord {
        let ctx = Context { language: "en".into(), country: "IN".into(), screens: vec!["HomeScreen".into(), "VoucherScreen".into()] };
        new_record(
            "panic",
            "called `Result::unwrap()` on an `Err` value: \"cannot post 12,500.00 to Sharma & Sons Pvt Ltd for accounts@sharma.co.in\"",
            "apps/desktop/src-tauri/src/api.rs:812:31",
            "thread 'main'",
            &ctx,
            secrets,
        )
    }

    #[test]
    fn report_is_scrubbed_and_fits() {
        let s = secrets();
        let rec = sample(&s);
        assert!(!rec.message.contains("Sharma") && !rec.message.contains("12,500") && !rec.message.contains('@'));
        let r = build_report("https://github.com/khataos/khataos/issues", Some(&rec), &Context::default(), &s);
        assert!(r.url.starts_with("https://github.com/khataos/khataos/issues/new?template=crash_report.yml&title=Crash%3A"));
        for field in ["&version=", "&os=", "&platform=desktop", "&country=IN", "&language=en", "&details="] {
            assert!(r.url.contains(field), "{field}");
        }
        assert!(r.details.contains("- Screens: HomeScreen > VoucherScreen"));
        assert!(r.details.contains("api.rs:812:31"));
        assert!(allowed_url(&r.url));
        assert!(!allowed_url("https://evil.example/new?x"));
        assert!(!allowed_url(&format!("{}\nx", r.url)));

        // A huge stack is cut, the URL stays under the limit.
        let mut big = rec.clone();
        big.stack = (0..2000).map(|i| format!("    at frame{i} (http://tauri.localhost/assets/index-abc.js:1:{i})")).collect::<Vec<_>>().join("\n");
        big.message = "x ".repeat(700);
        let r = build_report("https://github.com/khataos/khataos/issues", Some(&big), &Context::default(), &s);
        assert!(r.url.len() <= MAX_URL, "{}", r.url.len());
        assert!(r.details.contains("frame0\n") || r.details.contains("frame0 "));
        assert!(!r.details.contains("frame1999"));
        assert!(r.details.contains(&"x ".repeat(699)), "the message is kept whole while the stack can shrink");
        // A message too long even alone is cut after the stack is gone.
        big.message = "x ".repeat(5000);
        let r = build_report("https://github.com/khataos/khataos/issues", Some(&big), &Context::default(), &s);
        assert!(r.url.len() <= MAX_URL, "{}", r.url.len());
        assert!(!r.details.contains("frame0") && r.details.contains("x x x"));
    }

    #[test]
    fn blank_report_has_the_environment() {
        let ctx = Context { language: "ar".into(), country: "AE".into(), screens: vec!["StartupScreen".into()] };
        let r = build_report("https://github.com/khataos/khataos/issues/", None, &ctx, &Secrets::default());
        assert_eq!(r.title, "Problem: ");
        assert_eq!(r.version, version());
        assert_eq!(r.country, "AE");
        assert_eq!(r.language, "ar");
        assert!(r.url.starts_with("https://github.com/khataos/khataos/issues/new?template=crash_report.yml&title=Problem%3A%20&"));
        assert!(r.details.starts_with(ASK_PROBLEM));
        assert!(r.details.contains("StartupScreen"));
    }

    #[test]
    fn records_are_private_capped_and_deduplicated() {
        let dir = temp_dir("records");
        let s = secrets();
        let first = sample(&s);
        assert!(save(&dir, &first).unwrap());
        // The same crash again, still unreported: not written twice.
        let mut again = sample(&s);
        again.id = format!("{}-z", again.id);
        assert!(!save(&dir, &again).unwrap());
        assert_eq!(list(&dir).len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let f = dir.join(DIR).join(format!("{}.json", first.id));
            assert_eq!(std::fs::metadata(f).unwrap().permissions().mode() & 0o777, 0o600);
        }
        for i in 0..KEEP + 5 {
            let mut r = sample(&s);
            r.id = format!("2099{i:04}");
            r.signature = format!("sig{i}");
            save(&dir, &r).unwrap();
        }
        let all = list(&dir);
        assert_eq!(all.len(), KEEP);
        assert_eq!(all[0].id, format!("2099{:04}", KEEP + 4));

        // Dismissed ones stop being offered; a bad id is refused.
        let ids: Vec<String> = all.iter().map(|r| r.id.clone()).collect();
        dismiss(&dir, &ids).unwrap();
        assert!(pending(&dir).crashes.is_empty());
        assert!(load(&dir, "../../etc/passwd").is_err());

        assert!(ask_on_start(&dir));
        set_ask_on_start(&dir, false).unwrap();
        assert!(!ask_on_start(&dir));
        set_ask_on_start(&dir, true).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enc_is_query_safe() {
        assert_eq!(enc("a b&c=d/é\n"), "a%20b%26c%3Dd%2F%C3%A9%0A");
    }
}
