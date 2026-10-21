//! HMRC fraud-prevention headers for a desktop app that calls HMRC's APIs
//! itself (connection method `DESKTOP_APP_DIRECT`).
//!
//! Sending them is required by law for the VAT (MTD) API ("You are required
//! by law to submit header data for the VAT (MTD) ... APIs") [PRIMARY: HMRC
//! Developer Hub, *Send fraud prevention data*, version 3.3 issued 27 January
//! 2025, <https://developer.service.hmrc.gov.uk/guides/fraud-prevention/>].
//! The headers and their formats are from the *Desktop application direct*
//! page (<https://developer.service.hmrc.gov.uk/guides/fraud-prevention/connection-method/desktop-app-direct/>)
//! and the encoding rules from *Getting it right*
//! (<https://developer.service.hmrc.gov.uk/guides/fraud-prevention/getting-it-right/>):
//!
//! - header contents are US-ASCII, other characters percent-encoded;
//! - key-value structures are `<key-1>=<value-1>&<key-2>=<value-2>`, keys
//!   and values percent-encoded, separators not; "Whenever a key is
//!   applicable but has no applicable value, you can omit the key-value pair
//!   or include the key with an empty value" (this module omits it);
//! - lists are `<value-1>,<value-2>`, values percent-encoded and "must not
//!   be empty";
//! - a value that can't be collected "due to restrictions beyond your
//!   reasonable control": after telling HMRC, "you can omit the header or
//!   submit it with an empty value. You must not include a placeholder
//!   value, for example null or undefined". This module sends such headers
//!   with an empty value.
//!
//! This module only **formats** the values: gathering them from the OS and
//! the window is the desktop app's job (`apps/desktop/src-tauri/src/mtd_client.rs`).
//! Percent-encoding keeps the RFC 3986 unreserved characters (`A-Z a-z 0-9
//! - . _ ~`) and encodes every other byte of the UTF-8 text as `%XX`
//! (upper-case hex); HMRC's examples (`Dell%20Inc.`, `domain%5Calice`,
//! `ea%3A43…`, `2021-11-21T13%3A23Z`) are all consistent with that.

use std::fmt::Write as _;
use std::net::IpAddr;

use chrono::{DateTime, SecondsFormat, Utc};
use thiserror::Error;

/// `Gov-Client-Connection-Method` for a desktop app calling HMRC directly.
pub const CONNECTION_METHOD: &str = "DESKTOP_APP_DIRECT";

/// The specification version these headers follow (HMRC's change log).
pub const SPEC_VERSION: &str = "3.3";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FraudError {
    #[error("the device ID must be a UUID, not {0:?}")]
    DeviceId(String),
    #[error("the signed-in OS user is not known, so Gov-Client-User-IDs can't be sent")]
    NoOsUser,
    #[error("no screen size was given; Gov-Client-Screens needs at least one screen")]
    NoScreens,
    #[error("{0} must be positive whole numbers of pixels")]
    Size(&'static str),
    #[error("the screen scaling factor must be a positive number")]
    Scaling,
    #[error("the product name is empty")]
    ProductName,
    #[error("no software version is given for Gov-Vendor-Version")]
    NoVersion,
}

/// Percent-encode everything except the RFC 3986 unreserved characters.
pub fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// `k=v&k=v`, keys and values encoded; pairs with an empty value are left out.
pub fn key_values<K: AsRef<str>, V: AsRef<str>>(pairs: &[(K, V)]) -> String {
    pairs
        .iter()
        .filter(|(_, v)| !v.as_ref().trim().is_empty())
        .map(|(k, v)| format!("{}={}", pct(k.as_ref()), pct(v.as_ref())))
        .collect::<Vec<_>>()
        .join("&")
}

/// `v,v`, values encoded; empty values and repeats are left out (list values
/// "must not be empty").
pub fn list<S: AsRef<str>>(values: &[S]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for v in values {
        let v = v.as_ref().trim();
        if !v.is_empty() && !seen.contains(&v) {
            seen.push(v);
        }
    }
    seen.into_iter().map(pct).collect::<Vec<_>>().join(",")
}

/// `UTC±hh:mm` from the local offset east of UTC, in seconds
/// (`Gov-Client-Timezone`: "UTC+00:00", "UTC+01:00", "UTC-01:15").
pub fn timezone(offset_seconds: i32) -> String {
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let minutes = offset_seconds.unsigned_abs() / 60;
    format!("UTC{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

/// `yyyy-MM-ddThh:mm:ss.sssZ` in UTC, milliseconds always shown
/// (`Gov-Client-Local-IPs-Timestamp`: "2020-09-21T14:30:05.123Z").
pub fn timestamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Local IP addresses, IPv6 ones percent-encoded (`Gov-Client-Local-IPs`:
/// "fc00%3A%3A,10.1.2.3"). Encoding an IPv4 address changes nothing.
pub fn local_ips(ips: &[IpAddr]) -> String {
    list(&ips.iter().map(IpAddr::to_string).collect::<Vec<_>>())
}

/// MAC addresses as lower-case colon-separated hex, each encoded
/// (`Gov-Client-MAC-Addresses`: "ea%3A43%3A1a%3A5d%3A21%3A45,…"). All-zero
/// addresses (interfaces without hardware) are left out.
pub fn mac_addresses(macs: &[[u8; 6]]) -> String {
    let text: Vec<String> = macs
        .iter()
        .filter(|m| m.iter().any(|b| *b != 0))
        .map(|m| m.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":"))
        .collect();
    list(&text)
}

/// One screen, as the window reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    /// Device pixels per CSS pixel (`window.devicePixelRatio`).
    pub scaling_factor: f64,
    /// Bits per pixel (`screen.colorDepth`).
    pub colour_depth: u32,
}

/// A scaling factor without trailing zeros, to at most 4 decimals: 1, 1.25,
/// 2. (KhataOS's choice: HMRC gives "2" and "1.25" as examples and no
/// precision.)
fn scaling(f: f64) -> Result<String, FraudError> {
    if !f.is_finite() || f <= 0.0 {
        return Err(FraudError::Scaling);
    }
    let s = format!("{f:.4}");
    Ok(s.trim_end_matches('0').trim_end_matches('.').to_owned())
}

/// `Gov-Client-Screens`: "width=1920&height=1080&scaling-factor=1&colour-depth=16,…".
pub fn screens(list_of: &[Screen]) -> Result<String, FraudError> {
    if list_of.is_empty() {
        return Err(FraudError::NoScreens);
    }
    let mut out = Vec::with_capacity(list_of.len());
    for s in list_of {
        if s.width == 0 || s.height == 0 {
            return Err(FraudError::Size("screen width and height"));
        }
        out.push(key_values(&[
            ("width", s.width.to_string()),
            ("height", s.height.to_string()),
            ("scaling-factor", scaling(s.scaling_factor)?),
            ("colour-depth", s.colour_depth.to_string()),
        ]));
    }
    Ok(out.join(","))
}

/// `Gov-Client-Window-Size`: "width=1256&height=803".
pub fn window_size(width: u32, height: u32) -> Result<String, FraudError> {
    if width == 0 || height == 0 {
        return Err(FraudError::Size("window width and height"));
    }
    Ok(key_values(&[("width", width.to_string()), ("height", height.to_string())]))
}

/// The operating system and device for `Gov-Client-User-Agent`. An empty
/// field (not found on this OS) is left out of the header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserAgent {
    /// "Windows", "MacOS", "Linux".
    pub os_family: String,
    pub os_version: String,
    pub device_manufacturer: String,
    pub device_model: String,
}

/// "os-family=Windows&os-version=Server%202012&device-manufacturer=Dell%20Inc.&device-model=OptiPlex%20980".
pub fn user_agent(ua: &UserAgent) -> String {
    key_values(&[
        ("os-family", ua.os_family.as_str()),
        ("os-version", ua.os_version.as_str()),
        ("device-manufacturer", ua.device_manufacturer.as_str()),
        ("device-model", ua.device_model.as_str()),
    ])
}

/// How a multi-factor sign-in was done (`Gov-Client-Multi-Factor` `type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MfaType {
    Totp,
    AuthCode,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiFactor {
    pub kind: MfaType,
    /// The last successful prompt for this factor.
    pub at: DateTime<Utc>,
    /// A consistent hash identifying the factor, never the secret itself.
    pub unique_reference: String,
}

/// "type=AUTH_CODE&timestamp=2021-11-21T13%3A23Z&unique-reference=…,type=TOTP&…".
/// Empty when the user signs in with a password only (KhataOS today).
pub fn multi_factor(factors: &[MultiFactor]) -> String {
    factors
        .iter()
        .map(|f| {
            let kind = match f.kind {
                MfaType::Totp => "TOTP",
                MfaType::AuthCode => "AUTH_CODE",
                MfaType::Other => "OTHER",
            };
            key_values(&[
                ("type", kind.to_owned()),
                ("timestamp", f.at.format("%Y-%m-%dT%H:%MZ").to_string()),
                ("unique-reference", f.unique_reference.clone()),
            ])
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Everything gathered on the device for one request.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopFacts {
    /// A UUID made once per install and kept on the device.
    pub device_id: String,
    pub local_ips: Vec<IpAddr>,
    /// When `local_ips` were read: immediately before the request.
    pub local_ips_at: DateTime<Utc>,
    pub mac_addresses: Vec<[u8; 6]>,
    pub multi_factor: Vec<MultiFactor>,
    pub screens: Vec<Screen>,
    /// The window's inner size in pixels.
    pub window: (u32, u32),
    /// The local offset east of UTC, in seconds.
    pub timezone_offset_seconds: i32,
    pub user_agent: UserAgent,
    /// The user signed in on the device (`os` key), e.g. "DOMAIN\alice".
    pub os_user: String,
    /// Accounts the user signs in to this application with, as
    /// (application key, user id): ("khataos", "alice").
    pub app_users: Vec<(String, String)>,
    /// Hashed licence keys, as (software, hash). Empty for free software.
    pub licence_ids: Vec<(String, String)>,
    pub product_name: String,
    /// (software, version) of the software handling the request.
    pub versions: Vec<(String, String)>,
}

/// The `DESKTOP_APP_DIRECT` headers, in the order of HMRC's page. Every
/// header is present ("You must submit data for all of these headers");
/// one with nothing to report (no licences, no multi-factor sign-in, no
/// network interfaces) has an empty value, never a placeholder.
pub fn desktop_app_direct(f: &DesktopFacts) -> Result<Vec<(&'static str, String)>, FraudError> {
    let device_id = f.device_id.trim().to_ascii_lowercase();
    if !is_uuid(&device_id) {
        return Err(FraudError::DeviceId(f.device_id.clone()));
    }
    if f.os_user.trim().is_empty() {
        return Err(FraudError::NoOsUser);
    }
    if f.product_name.trim().is_empty() {
        return Err(FraudError::ProductName);
    }
    if f.versions.iter().all(|(_, v)| v.trim().is_empty()) {
        return Err(FraudError::NoVersion);
    }
    let mut ids: Vec<(&str, &str)> = vec![("os", f.os_user.as_str())];
    ids.extend(f.app_users.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    Ok(vec![
        ("Gov-Client-Connection-Method", CONNECTION_METHOD.to_owned()),
        ("Gov-Client-Device-ID", device_id),
        ("Gov-Client-User-IDs", key_values(&ids)),
        ("Gov-Client-Timezone", timezone(f.timezone_offset_seconds)),
        ("Gov-Client-Local-IPs", local_ips(&f.local_ips)),
        ("Gov-Client-Local-IPs-Timestamp", timestamp(f.local_ips_at)),
        ("Gov-Client-MAC-Addresses", mac_addresses(&f.mac_addresses)),
        ("Gov-Client-Screens", screens(&f.screens)?),
        ("Gov-Client-Window-Size", window_size(f.window.0, f.window.1)?),
        ("Gov-Client-User-Agent", user_agent(&f.user_agent)),
        ("Gov-Client-Multi-Factor", multi_factor(&f.multi_factor)),
        ("Gov-Vendor-Version", key_values(&f.versions)),
        ("Gov-Vendor-License-IDs", key_values(&f.licence_ids)),
        ("Gov-Vendor-Product-Name", pct(f.product_name.trim())),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // Each expected string is HMRC's own example from the Desktop
    // application direct page (spec 3.3).

    #[test]
    fn local_ips_encode_ipv6_only() {
        let ips: Vec<IpAddr> = vec!["fc00::".parse().unwrap(), "10.1.2.3".parse().unwrap()];
        assert_eq!(local_ips(&ips), "fc00%3A%3A,10.1.2.3");
        let v4: Vec<IpAddr> = vec!["10.1.2.3".parse().unwrap(), "10.3.4.2".parse().unwrap()];
        assert_eq!(local_ips(&v4), "10.1.2.3,10.3.4.2");
        assert_eq!(local_ips(&[]), "");
    }

    #[test]
    fn timestamp_has_milliseconds_and_z() {
        let at = Utc.with_ymd_and_hms(2020, 9, 21, 14, 30, 5).unwrap() + chrono::Duration::milliseconds(123);
        assert_eq!(timestamp(at), "2020-09-21T14:30:05.123Z");
        let whole = Utc.with_ymd_and_hms(2020, 9, 21, 14, 30, 5).unwrap();
        assert_eq!(timestamp(whole), "2020-09-21T14:30:05.000Z", "trailing zeros are kept");
    }

    #[test]
    fn mac_addresses_are_lower_case_and_encoded() {
        let macs = [[0xea, 0x43, 0x1a, 0x5d, 0x21, 0x45], [0x10, 0x12, 0xcc, 0xfa, 0xaa, 0x32], [0; 6]];
        assert_eq!(mac_addresses(&macs), "ea%3A43%3A1a%3A5d%3A21%3A45,10%3A12%3Acc%3Afa%3Aaa%3A32");
    }

    #[test]
    fn multi_factor_example() {
        let at = |m| Utc.with_ymd_and_hms(2021, 11, 21, 13, m, 0).unwrap();
        let s = multi_factor(&[
            MultiFactor {
                kind: MfaType::AuthCode,
                at: at(23),
                unique_reference: "fc4b5fd6816f75a7c81fc8eaa9499d6a299bd803397166e8c4cf9280b801d62c".into(),
            },
            MultiFactor {
                kind: MfaType::Totp,
                at: at(20),
                unique_reference: "0283da60063abfb3a87f1aed845d17fe2d9ba8c780b478dc4ae048f5ee97a6d5".into(),
            },
        ]);
        assert_eq!(
            s,
            "type=AUTH_CODE&timestamp=2021-11-21T13%3A23Z&unique-reference=fc4b5fd6816f75a7c81fc8eaa9499d6a299bd803397166e8c4cf9280b801d62c,type=TOTP&timestamp=2021-11-21T13%3A20Z&unique-reference=0283da60063abfb3a87f1aed845d17fe2d9ba8c780b478dc4ae048f5ee97a6d5"
        );
        assert_eq!(multi_factor(&[]), "");
    }

    #[test]
    fn screens_and_window() {
        let s = screens(&[
            Screen { width: 1920, height: 1080, scaling_factor: 1.0, colour_depth: 16 },
            Screen { width: 3000, height: 2000, scaling_factor: 1.25, colour_depth: 16 },
        ])
        .unwrap();
        assert_eq!(s, "width=1920&height=1080&scaling-factor=1&colour-depth=16,width=3000&height=2000&scaling-factor=1.25&colour-depth=16");
        assert_eq!(window_size(1256, 803).unwrap(), "width=1256&height=803");
        assert!(window_size(0, 803).is_err());
        assert!(screens(&[]).is_err());
        let odd = screens(&[Screen { width: 1, height: 1, scaling_factor: 1.100000023841858, colour_depth: 24 }]).unwrap();
        assert!(odd.contains("scaling-factor=1.1&"), "{odd}");
        assert!(screens(&[Screen { width: 1, height: 1, scaling_factor: f64::NAN, colour_depth: 24 }]).is_err());
    }

    #[test]
    fn timezone_examples() {
        assert_eq!(timezone(0), "UTC+00:00");
        assert_eq!(timezone(3600), "UTC+01:00");
        assert_eq!(timezone(-75 * 60), "UTC-01:15");
        assert_eq!(timezone(5 * 3600 + 30 * 60), "UTC+05:30");
    }

    #[test]
    fn user_agent_example() {
        let ua = UserAgent {
            os_family: "Windows".into(),
            os_version: "Server 2012".into(),
            device_manufacturer: "Dell Inc.".into(),
            device_model: "OptiPlex 980".into(),
        };
        assert_eq!(user_agent(&ua), "os-family=Windows&os-version=Server%202012&device-manufacturer=Dell%20Inc.&device-model=OptiPlex%20980");
        let mac = UserAgent { os_family: "MacOS".into(), os_version: "10.15.7".into(), device_manufacturer: "Apple".into(), device_model: "MacBookPro15,2".into() };
        assert_eq!(user_agent(&mac), "os-family=MacOS&os-version=10.15.7&device-manufacturer=Apple&device-model=MacBookPro15%2C2");
        let partial = UserAgent { os_family: "Linux".into(), ..Default::default() };
        assert_eq!(user_agent(&partial), "os-family=Linux", "unknown fields are left out");
    }

    #[test]
    fn key_value_examples() {
        assert_eq!(key_values(&[("os", "domain\\alice")]), "os=domain%5Calice");
        assert_eq!(key_values(&[("os", "domain\\alice"), ("my-application", "alice123")]), "os=domain%5Calice&my-application=alice123");
        assert_eq!(key_values(&[("my-application", "1.6.0")]), "my-application=1.6.0");
        assert_eq!(pct("Product Name"), "Product%20Name");
        assert_eq!(key_values(&[("a", ""), ("b", "x")]), "b=x");
        assert_eq!(pct("café"), "caf%C3%A9", "non-ASCII is encoded as UTF-8 bytes");
    }

    fn facts() -> DesktopFacts {
        DesktopFacts {
            device_id: "BEEC798B-B366-47FA-B1F8-92CEDE14A1CE".into(),
            local_ips: vec!["10.1.2.3".parse().unwrap()],
            local_ips_at: Utc.with_ymd_and_hms(2026, 10, 20, 9, 0, 0).unwrap(),
            mac_addresses: vec![],
            multi_factor: vec![],
            screens: vec![Screen { width: 1920, height: 1080, scaling_factor: 2.0, colour_depth: 24 }],
            window: (1256, 803),
            timezone_offset_seconds: 3600,
            user_agent: UserAgent { os_family: "MacOS".into(), ..Default::default() },
            os_user: "alice".into(),
            app_users: vec![("khataos".into(), "Alice Smith".into())],
            licence_ids: vec![],
            product_name: "KhataOS".into(),
            versions: vec![("KhataOS".into(), "0.1.0".into())],
        }
    }

    #[test]
    fn every_header_is_sent_and_ascii() {
        let h = desktop_app_direct(&facts()).unwrap();
        let names: Vec<&str> = h.iter().map(|(n, _)| *n).collect();
        assert_eq!(names.len(), 14);
        for want in [
            "Gov-Client-Connection-Method",
            "Gov-Client-Device-ID",
            "Gov-Client-User-IDs",
            "Gov-Client-Timezone",
            "Gov-Client-Local-IPs",
            "Gov-Client-Local-IPs-Timestamp",
            "Gov-Client-MAC-Addresses",
            "Gov-Client-Screens",
            "Gov-Client-Window-Size",
            "Gov-Client-User-Agent",
            "Gov-Client-Multi-Factor",
            "Gov-Vendor-Version",
            "Gov-Vendor-License-IDs",
            "Gov-Vendor-Product-Name",
        ] {
            assert!(names.contains(&want), "{want}");
        }
        let get = |n: &str| h.iter().find(|(k, _)| *k == n).unwrap().1.as_str();
        assert_eq!(get("Gov-Client-Device-ID"), "beec798b-b366-47fa-b1f8-92cede14a1ce");
        assert_eq!(get("Gov-Client-User-IDs"), "os=alice&khataos=Alice%20Smith");
        assert_eq!(get("Gov-Vendor-License-IDs"), "", "no licences: an empty value, not a placeholder");
        assert_eq!(get("Gov-Client-Multi-Factor"), "");
        assert_eq!(get("Gov-Client-MAC-Addresses"), "");
        assert_eq!(get("Gov-Vendor-Version"), "KhataOS=0.1.0");
        assert!(h.iter().all(|(_, v)| v.is_ascii() && !v.contains(' ')));
    }

    #[test]
    fn bad_facts_are_refused() {
        let mut f = facts();
        f.device_id = "not-a-uuid".into();
        assert!(matches!(desktop_app_direct(&f), Err(FraudError::DeviceId(_))));
        let mut f = facts();
        f.os_user = " ".into();
        assert_eq!(desktop_app_direct(&f), Err(FraudError::NoOsUser));
        let mut f = facts();
        f.screens.clear();
        assert_eq!(desktop_app_direct(&f), Err(FraudError::NoScreens));
    }
}
