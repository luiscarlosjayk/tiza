//! t-153 — Credits: Demo Time among the inspirations, Luis as the author in
//! the README, one form of his name in Cargo.toml and LICENSE-MIT.
//!
//! Acceptance criteria:
//! - AC1 README «Inspired by» line adds [Demo Time](https://demotime.show),
//!   keeping the existing four.
//! - AC2 README ends with the author line: «Made by
//!   [Luis Carlos Osorio Jayk](https://github.com/luiscarlosjayk) (LuisK) ·
//!   [LinkedIn](…) · [YouTube](…).» — exactly these three links.
//! - AC3 `[workspace.package] authors` has one entry, the full name (Claude
//!   removed); every crate keeps `authors.workspace = true`.
//! - AC4 LICENSE-MIT: «Copyright (c) 2026 Luis Carlos Osorio Jayk and Tiza
//!   contributors».
//! - AC4b README «License» keeps «MIT or Apache-2.0, at your option.», links
//!   both license files and carries the Rust contribution clause verbatim.
//! - AC5 the name only appears in its full form, never without «Jayk»
//!   (CHANGELOG history may stay as written).

use std::path::{Path, PathBuf};

const FULL_NAME: &str = "Luis Carlos Osorio Jayk";

const CONTRIBUTION_CLAUSE: &str = "Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Tiza by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The README's «License» section, up to the next `## ` or the end.
fn license_section() -> String {
    let readme = read("README.md");
    let start = readme
        .find("\n## License\n")
        .expect("README has a «## License» section");
    let rest = &readme[start + 1..];
    let end = rest[3..].find("\n## ").map(|i| i + 3).unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn ac3_workspace_authors_is_only_luis_with_the_full_name() {
    let cargo = read("Cargo.toml");
    let pkg_start = cargo
        .find("[workspace.package]")
        .expect("[workspace.package]");
    let pkg = &cargo[pkg_start..];
    let pkg = &pkg[..pkg[1..].find("\n[").map(|i| i + 1).unwrap_or(pkg.len())];
    let line = pkg
        .lines()
        .find(|l| l.trim_start().starts_with("authors"))
        .expect("[workspace.package] has authors");
    assert_eq!(
        line.trim(),
        r#"authors = ["Luis Carlos Osorio Jayk <luiscarlosjayk@gmail.com>"]"#
    );
    assert!(!cargo.contains("Claude <"), "Claude is not a Cargo author");

    // Every member inherits it, so cargo metadata shows only Luis.
    for member in [
        "crates/tiza-core",
        "crates/tiza-tui",
        "crates/tiza-mcp",
        "crates/tiza-cli",
        "crates/tiza-export",
        "qa",
    ] {
        let manifest = read(&format!("{member}/Cargo.toml"));
        assert!(
            manifest
                .lines()
                .any(|l| l.trim() == "authors.workspace = true"),
            "{member} keeps authors.workspace = true"
        );
        assert!(
            !manifest
                .lines()
                .any(|l| l.trim_start().starts_with("authors =")),
            "{member} does not set its own authors"
        );
    }
}

#[test]
fn ac4_license_mit_copyright_line_has_the_full_name() {
    let mit = read("LICENSE-MIT");
    assert_eq!(
        mit.lines().nth(2),
        Some("Copyright (c) 2026 Luis Carlos Osorio Jayk and Tiza contributors")
    );
}

#[test]
fn ac4b_license_section_keeps_the_dual_license_and_adds_the_clause() {
    let section = license_section();
    assert!(section.contains("MIT or Apache-2.0, at your option."));
    assert!(section.contains("[LICENSE-MIT](LICENSE-MIT)"));
    assert!(section.contains("[LICENSE-APACHE](LICENSE-APACHE)"));
    assert!(
        section.contains(CONTRIBUTION_CLAUSE),
        "the contribution clause is there, verbatim"
    );
    assert!(root().join("LICENSE-MIT").is_file());
    assert!(root().join("LICENSE-APACHE").is_file());
}

#[test]
fn ac5_the_name_only_appears_in_its_full_form() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if p.is_dir() {
                if name != "target" && name != ".git" {
                    walk(&p, out);
                }
            } else if name != "CHANGELOG.md" {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(&root(), &mut files);
    // Built from the full name, so this file never spells the short form.
    let short = FULL_NAME.strip_suffix(" Jayk").unwrap();
    let mut bad = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue; // binary files
        };
        for (i, _) in text.match_indices(short) {
            if !text[i..].starts_with(FULL_NAME) {
                bad.push(f.display().to_string());
            }
        }
    }
    assert!(bad.is_empty(), "short form of the name in: {bad:?}");
}
