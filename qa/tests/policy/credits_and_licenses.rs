//! Credits: one form of Luis's name in Cargo.toml and LICENSE-MIT,
//! and the license text of the README.
//!
//! Acceptance criteria:
//! - AC1 and AC2 (Demo Time in the README «Inspired by» line, the author
//!   line at the end of the README) are manual checks: since the test rule
//!   of AGENTS.md, no test pins prose.
//! - AC3 `[workspace.package] authors` has one entry, the full name (Claude
//!   removed); every crate keeps `authors.workspace = true`.
//! - AC4 LICENSE-MIT: «Copyright (c) 2026 Luis Carlos Osorio Jayk and Tiza
//!   contributors».
//! - AC4b README «License» keeps «MIT or Apache-2.0, at your option.», links
//!   both license files and carries the Rust contribution clause verbatim.
//! - AC5 the name only appears in its full form, never without «Jayk»
//!   (CHANGELOG history may stay as written).
//! - AC6 Every font of the repository has its license next to it: the two
//!   that the binary embeds (JetBrains Mono, OFL; the DejaVu symbols,
//!   Bitstream Vera) and the brand serif (Libron, OFL, with its Reserved
//!   Font Name). Moved here from the SVG export tests, extended to the embedded
//!   fonts.
//! - AC7 The release archives (release.yml and scripts/release-local.sh)
//!   carry the license of every font that the binary embeds, and the
//!   license and patent grant of libwebp, which it links.

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
fn license_mit_copyright_line_has_the_full_name() {
    let mit = read("LICENSE-MIT");
    assert_eq!(
        mit.lines().nth(2),
        Some("Copyright (c) 2026 Luis Carlos Osorio Jayk and Tiza contributors")
    );
}

#[test]
fn license_section_keeps_the_dual_license_and_adds_the_clause() {
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
fn the_name_only_appears_in_its_full_form() {
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

/// AC6: each license file says its license, and its folder holds the fonts
/// that it covers.
#[test]
fn every_font_has_its_license_next_to_it() {
    let fonts: [(&str, &str, &[&str], &[&str]); 3] = [
        (
            "crates/tiza-export/assets/fonts",
            "LICENSE-JetBrainsMono-OFL.txt",
            &["SIL OPEN FONT LICENSE Version 1.1", "JetBrains Mono"],
            &[
                "JetBrainsMono-Regular.ttf",
                "JetBrainsMono-Bold.ttf",
                "JetBrainsMono-Italic.ttf",
                "JetBrainsMono-BoldItalic.ttf",
            ],
        ),
        (
            "crates/tiza-export/assets/fonts",
            "LICENSE-TizaSymbols-DejaVu.txt",
            &["License: bitstream-vera", "Permission is hereby granted"],
            &["TizaSymbols.ttf"],
        ),
        (
            "docs/brand/fonts",
            "LICENSE-TizaSerif-Libron-OFL.txt",
            &[
                "SIL OPEN FONT LICENSE Version 1.1",
                "Reserved Font Name Libron",
            ],
            &[
                "TizaSerif-Regular.ttf",
                "TizaSerif-Bold.ttf",
                "TizaSerif-Italic.ttf",
                "TizaSerif-BoldItalic.ttf",
            ],
        ),
    ];
    for (dir, license, phrases, covered) in fonts {
        let text = read(&format!("{dir}/{license}"));
        for p in phrases {
            assert!(text.contains(p), "{dir}/{license}: «{p}»");
        }
        for f in covered {
            assert!(root().join(dir).join(f).is_file(), "{dir}/{f}");
        }
    }
}

/// AC7: both ways to build a release archive copy each license of
/// `crates/tiza-export/assets/fonts` and of `crates/tiza-export` into it,
/// so a font or a library added there with its license is not shipped
/// without it. (`LICENSE-MIT` and `LICENSE-APACHE` of the crate are
/// Tiza's own, copied from the top of the repository.)
#[test]
fn the_release_archives_carry_the_licenses_of_what_the_binary_embeds() {
    let mut licenses = Vec::new();
    for dir in ["crates/tiza-export/assets/fonts", "crates/tiza-export"] {
        for e in std::fs::read_dir(root().join(dir)).unwrap() {
            let n = e.unwrap().file_name().to_string_lossy().into_owned();
            if n.starts_with("LICENSE-") && n != "LICENSE-MIT" && n != "LICENSE-APACHE" {
                licenses.push(format!("{dir}/{n}"));
            }
        }
    }
    licenses.sort();
    assert_eq!(licenses.len(), 3, "{licenses:?}");
    let webp = read("crates/tiza-export/LICENSE-libwebp.txt");
    for p in [
        "Copyright (c) 2010, Google Inc.",
        "Redistribution and use in source and binary forms",
        "Additional IP Rights Grant (Patents)",
    ] {
        assert!(webp.contains(p), "LICENSE-libwebp.txt: «{p}»");
    }
    for (script, into) in [
        (".github/workflows/release.yml", "\"dist/$name/\""),
        ("scripts/release-local.sh", "\"$stage/\""),
    ] {
        // The copy command, joined across its `\` line breaks, before the
        // `tar -czf` that packs the archive.
        let text = read(script).replace("\\\n", " ");
        let lines: Vec<&str> = text.lines().collect();
        let tar = lines
            .iter()
            .position(|l| l.contains("tar ") && l.contains("-czf"))
            .unwrap_or_else(|| panic!("{script} has no `tar -czf`"));
        for path in &licenses {
            let cp = lines
                .iter()
                .position(|l| {
                    l.trim_start().starts_with("cp ")
                        && l.contains(path.as_str())
                        && l.contains(into)
                })
                .unwrap_or_else(|| panic!("{script} does not copy {path} into the archive"));
            assert!(
                cp < tar,
                "{script} copies {path} after it packs the archive"
            );
        }
    }
}
