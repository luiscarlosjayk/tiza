//! The packages carry the license texts, and `tiza` leaves out its tests.

use std::path::PathBuf;
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

// --- crates.io pages -----------------------------------------------------

/// The files `cargo package` would put in the crate.
fn packaged(name: &str) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args([
            "package",
            "--list",
            "--offline",
            "--allow-dirty",
            "-p",
            name,
        ])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// QA P2-2: every crate carries the licence texts (links in each crate
/// directory to the repository's, packaged as files), and QA P3-6: `tiza`
/// leaves out its `tests/`, which read `../../examples/`.
#[test]
fn packages_carry_the_licences_and_tiza_not_its_tests() {
    for (name, dir) in [
        ("tiza", "crates/tiza-cli"),
        ("tiza-core", "crates/tiza-core"),
        ("tiza-export", "crates/tiza-export"),
        ("tiza-tui", "crates/tiza-tui"),
        ("tiza-mcp", "crates/tiza-mcp"),
    ] {
        let files = packaged(name);
        for licence in ["LICENSE-MIT", "LICENSE-APACHE"] {
            assert!(files.iter().any(|f| f == licence), "{name}: {licence}");
            assert_eq!(
                read(&format!("{dir}/{licence}")),
                read(licence),
                "{name}: the repository's {licence}"
            );
        }
        // The fonts and the C library that the binary embeds go with their
        // licenses.
        if name == "tiza-export" {
            for licence in [
                "assets/fonts/LICENSE-JetBrainsMono-OFL.txt",
                "assets/fonts/LICENSE-TizaSymbols-DejaVu.txt",
                "LICENSE-libwebp.txt",
            ] {
                assert!(files.iter().any(|f| f == licence), "{name}: {licence}");
            }
        }
        if name == "tiza" {
            assert!(
                !files.iter().any(|f| f.starts_with("tests/")),
                "tiza packages {files:?}"
            );
            assert!(files.iter().any(|f| f == "src/main.rs"));
        }
    }
}
