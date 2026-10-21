//! Keeps `docs/guide/keyboard.md` in step with the key map.
//!
//! If `guide_key_reference_is_up_to_date` fails, run `scripts/keymap-doc`
//! (or `cargo run -p khata-keymap --example keymap-doc -- --write`).

#[allow(dead_code)]
#[path = "../examples/keymap-doc.rs"]
mod keymap_doc;

use std::path::Path;

use keymap_doc::{Status, describe, render, splice};
use khata_keymap::Keymap;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn guide_key_reference_is_up_to_date() {
    // A Windows checkout may turn the page's line endings into CRLF.
    let page = read(keymap_doc::GUIDE).replace("\r\n", "\n");
    let fresh = splice(&page).expect("docs/guide/keyboard.md has the keymap-doc markers");
    assert!(
        fresh == page,
        "docs/guide/keyboard.md is out of date with crates/khata-keymap/src/tally.rs. Run scripts/keymap-doc."
    );
}

#[test]
fn every_binding_is_listed() {
    let table = render();
    for b in Keymap::tally_prime().bindings() {
        assert!(
            table.contains(&format!("| `{}` |", b.chord)),
            "{} missing",
            b.chord
        );
    }
}

/// The quoted names inside `const NAME = new Set([ ... ]);` in shell.ts.
fn shell_set(shell: &str, name: &str) -> Vec<String> {
    let head = format!("const {name} = new Set([");
    let start = shell
        .find(&head)
        .unwrap_or_else(|| panic!("{name} not found in shell.ts"))
        + head.len();
    let body = &shell[start..start + shell[start..].find("]);").expect("set ends")];
    body.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn keys_the_app_says_are_not_built_are_not_claimed_to_work() {
    let shell = read("../../apps/desktop/src/shell.ts");
    let not_built = shell_set(&shell, "NOT_BUILT");
    assert!(!not_built.is_empty());
    for b in Keymap::tally_prime().bindings() {
        let name = format!("{:?}", b.action);
        if not_built.contains(&name) {
            let (_, status) = describe(b.context, b.action);
            // NOT_BUILT is the shell's fallback: a screen may still handle the
            // action (Filter on the Desk), so "Partly" is fine, "Yes" is not.
            assert_ne!(
                status,
                Status::Yes,
                "{} ({name}) is in shell.ts NOT_BUILT but the guide says it works everywhere",
                b.chord
            );
        }
    }
}
