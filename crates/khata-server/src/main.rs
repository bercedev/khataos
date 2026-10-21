//! `khata-server` — run with:
//!
//! ```sh
//! KHATA_ADMIN_KEY=<long random secret> KHATA_DATA_DIR=./data khata-server
//! ```
//!
//! Put it behind a TLS-terminating reverse proxy; tokens are bearer secrets.
//! Running a relay for others: see `docs/deploy-relay.md`.
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `KHATA_ADMIN_KEY` | (required) | secret of 24+ characters for creating companies |
//! | `KHATA_DATA_DIR` | `./khata-data` | where `server.db` and company files live |
//! | `KHATA_BIND` | `127.0.0.1:8787` | address and port to listen on |
//! | `KHATA_OPEN_RELAY` | off | `1`: anyone may create end-to-end encrypted companies |
//! | `KHATA_RELAY_MAX_NEW_PER_IP_DAY` | 20 | new relay companies per client address a day (0 = no limit) |
//! | `KHATA_RELAY_MAX_BYTES` | 1073741824 (1 GiB) | bytes one relay company may store (0 = no limit) |
//! | `KHATA_RELAY_MAX_TOTAL_BYTES` | none | bytes all relay companies may store together |
//! | `KHATA_TRUST_PROXY` | off | `1`: take the client address from `X-Forwarded-For` |

use std::net::SocketAddr;

use khata_server::{AppState, Config, RelayLimits, router};

fn flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// A whole number from the environment; a bad value stops the server rather
/// than silently running without the limit the operator meant to set.
fn number(name: &str) -> Option<u64> {
    let v = std::env::var(name).ok()?;
    match v.trim().replace('_', "").parse() {
        Ok(n) => Some(n),
        Err(_) => {
            eprintln!("{name} must be a whole number, not {v:?}");
            std::process::exit(2);
        }
    }
}

#[tokio::main]
async fn main() {
    let admin_key = std::env::var("KHATA_ADMIN_KEY").unwrap_or_default();
    if admin_key.len() < 24 {
        eprintln!("KHATA_ADMIN_KEY must be set to a secret of at least 24 characters");
        std::process::exit(2);
    }
    let data_dir = std::env::var("KHATA_DATA_DIR").unwrap_or_else(|_| "./khata-data".into()).into();
    let bind = std::env::var("KHATA_BIND").unwrap_or_else(|_| "127.0.0.1:8787".into());

    let state = AppState::open(Config { data_dir, admin_key }).unwrap_or_else(|e| {
        eprintln!("cannot open data dir: {e}");
        std::process::exit(1);
    });
    let defaults = RelayLimits::default();
    let limits = RelayLimits {
        max_new_per_ip_day: number("KHATA_RELAY_MAX_NEW_PER_IP_DAY")
            .map_or(defaults.max_new_per_ip_day, |n| n.try_into().unwrap_or(u32::MAX)),
        max_bytes_per_company: number("KHATA_RELAY_MAX_BYTES").unwrap_or(defaults.max_bytes_per_company),
        max_total_bytes: number("KHATA_RELAY_MAX_TOTAL_BYTES").filter(|&n| n > 0),
        trust_proxy: flag("KHATA_TRUST_PROXY"),
    };
    state.set_relay_limits(limits);
    // A managed relay lets anyone create end-to-end encrypted companies.
    if flag("KHATA_OPEN_RELAY") {
        state.set_open_relay(true);
        eprintln!("open relay sign-up enabled (sealed companies only)");
        eprintln!(
            "relay limits: {} new companies per address a day, {} bytes per company, {} in all{}",
            limits.max_new_per_ip_day,
            limits.max_bytes_per_company,
            limits.max_total_bytes.map_or("no limit".into(), |n| format!("{n} bytes")),
            if limits.trust_proxy { ", client address from X-Forwarded-For" } else { "" },
        );
    }
    let listener = tokio::net::TcpListener::bind(&bind).await.unwrap_or_else(|e| {
        eprintln!("cannot bind {bind}: {e}");
        std::process::exit(1);
    });
    eprintln!("khata-server listening on {bind}");
    // The peer address feeds the per-address sign-up limit.
    axum::serve(listener, router(state).into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("server error");
}
