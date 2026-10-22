//! t-211 — Dependabot's PR #1 (skrifa 0.44 → 0.48) would have put a second
//! skrifa and read-fonts in the tree: usvg (resvg) stays on skrifa 0.44.
//!
//! Acceptance criteria:
//! - AC2 `.github/dependabot.yml` ignores skrifa and read-fonts as direct
//!   updates (they move with resvg/usvg, in the same commit), and its
//!   grouped PR no longer takes 0.x minor bumps blindly.
//! - AC3 ADR 0004's row and Cargo.toml's comment: «version tied to usvg's
//!   (resvg); never bumped alone».
//! - AC4 A duplicate cannot slip in: `deny.toml` fails on a second version
//!   of each crate that is direct only because it was already in the tree,
//!   and this test fails the gate on it without cargo-deny (it reads
//!   Cargo.lock).
//!
//! Corner case: the other crates direct only because they were already in
//! the tree (rustix and signal-hook, through crossterm) get the same guard.

use std::path::PathBuf;

/// Direct dependencies of ours that some other crate of the tree pulls
/// too, and the crate whose version they follow.
const TIED: &[(&str, &str)] = &[
    ("skrifa", "usvg"),
    ("read-fonts", "usvg"),
    ("rustix", "crossterm"),
    ("signal-hook", "crossterm"),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap()
}

/// The versions of `name` in Cargo.lock.
fn locked_versions(lock: &str, name: &str) -> Vec<String> {
    let header = format!("name = \"{name}\"");
    let mut out = Vec::new();
    let mut lines = lock.lines();
    while let Some(l) = lines.next() {
        if l == header
            && let Some(v) = lines.next().and_then(|v| v.strip_prefix("version = "))
        {
            out.push(v.trim_matches('"').to_string());
        }
    }
    out
}

#[test]
fn ac4_one_version_of_each_tied_crate_in_the_lock() {
    let lock = read("Cargo.lock");
    for (name, follows) in TIED {
        let versions = locked_versions(&lock, name);
        assert_eq!(
            versions.len(),
            1,
            "{name}: {versions:?} in Cargo.lock — it follows {follows}'s version and is never bumped alone (t-211, ADR 0004)"
        );
    }
}

#[test]
fn ac4_deny_toml_fails_on_a_second_version() {
    let deny = read("deny.toml");
    for (name, _) in TIED {
        let entry = format!("{{ crate = \"{name}\", deny-multiple-versions = true");
        assert!(deny.contains(&entry), "deny.toml lacks {entry}");
    }
}

#[test]
fn ac2_dependabot_never_bumps_them_alone_nor_groups_minor() {
    let cfg = read(".github/dependabot.yml");
    for name in ["skrifa", "read-fonts", "rustix", "signal-hook"] {
        assert!(
            cfg.contains(&format!("dependency-name: {name}")),
            "dependabot.yml does not ignore {name}"
        );
    }
    assert!(
        !cfg.contains("update-types: [minor, patch]"),
        "the group still takes minor bumps (breaking for 0.x crates)"
    );
}

#[test]
fn ac3_the_adr_row_and_cargo_toml_say_it() {
    let adr = read("docs/adr/0004-dependencias.md");
    let row = adr
        .lines()
        .find(|l| l.starts_with("| skrifa / read-fonts"))
        .unwrap();
    assert!(
        row.contains("version tied to usvg's (resvg); never bumped alone"),
        "{row}"
    );
    let cargo = read("Cargo.toml");
    assert!(
        cargo.contains("version tied to usvg's (resvg); never bumped alone"),
        "Cargo.toml's comment on skrifa"
    );
}
