//! t-134 — Tiza Serif and `--brand-font` leave the binary; Libron stays as a
//! brand asset for t-069; JetBrains Mono stays complete.
//!
//! Acceptance criteria:
//! - AC1 The four TizaSerif-*.ttf are no longer embedded; `SvgOptions.brand_font`,
//!   the prose-run code path, the prose tracking/scale constants and the serif
//!   `@font-face` blocks are gone; every export uses the monospace exactly as
//!   the default did (same PNG/GIF bytes; the SVG only loses the four serif
//!   faces).
//! - AC2 `--brand-font` and `export_image {brand_font}` are removed; for one
//!   prerelease `--brand-font` (and the hidden `--mono-labels`) are accepted
//!   as no-ops that print one stderr line; the MCP tool list stays valid.
//!   t-136 (Luis, 2026-10-18: no grace period, the repository is private)
//!   removed both flags: clap now rejects them like any unknown argument.
//! - AC3 The Libron subset and its OFL license live in `docs/brand/fonts/`
//!   (not in any crate), with a line that it is for t-069 and the Reserved
//!   Font Name note.
//! - AC4 Docs: ADR 0006 amendment, README export section, CHANGELOG (Removed).
//! - AC5 Size: the binary shrinks (measured by hand, in the card's note);
//!   the JetBrains Mono files are untouched (same bytes).

use serde_json::json;
use tiza_qa::{Live, McpClient, example, fixture, scratch, tiza, tiza_ok};

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn render(extra: &[&str], out: &str) -> std::process::Output {
    let f = example("ownership.rs.tiza");
    let mut args = vec![
        "render",
        f.to_str().unwrap(),
        "--step",
        "7",
        "--width",
        "100",
        "--out",
        out,
    ];
    args.extend_from_slice(extra);
    let o = tiza(&args);
    assert!(
        o.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    o
}

#[test]
fn ac1_no_serif_in_the_default_svg_and_one_text_path() {
    let dir = scratch("t134-svg");
    let out = dir.join("a.svg");
    render(&[], out.to_str().unwrap());
    let s = std::fs::read_to_string(&out).unwrap();
    assert_eq!(s.matches("Tiza Serif").count(), 0);
    assert_eq!(
        s.matches("@font-face").count(),
        4,
        "regular, bold, italic + symbols: the faces step 7 draws (t-130; no bold italic text)"
    );
    assert!(!s.contains(".p{") && !s.contains("class=\"p"));
    assert!(!s.contains("letter-spacing"));
    // Every text run sits on the grid.
    let texts = s.matches("<text ").count();
    assert!(texts > 50);
    assert_eq!(s.matches("textLength=").count(), texts);
    // A mark's glyphs keep their own run, the words start one cell later.
    assert!(
        s.contains(r##"<text x="800.00" y="111.68" textLength="19.20" lengthAdjust="spacing" fill="#f08b5b" xml:space="preserve">━▶</text>"##),
        "glyph run"
    );
    assert!(
        s.contains(r#"<text x="819.20" y="111.68" textLength="163.20""#),
        "words run"
    );
    // The serif is not compiled in.
    let fonts_rs = std::fs::read_to_string(root().join("crates/tiza-export/src/fonts.rs")).unwrap();
    assert!(!fonts_rs.contains("TizaSerif") && !fonts_rs.contains("PROSE_"));
    let svg_rs = std::fs::read_to_string(root().join("crates/tiza-export/src/svg.rs")).unwrap();
    assert!(!svg_rs.contains("brand_font"));
}

/// t-136: both flags are gone; clap rejects each one (exit 2, naming it)
/// and writes nothing, and the help does not list them.
#[test]
fn ac2_t136_removed_flags_are_rejected() {
    let dir = scratch("t136-flag");
    let f = example("ownership.rs.tiza");
    for flag in ["--brand-font", "--mono-labels"] {
        let out = dir.join("a.png");
        let r = tiza(&[
            "render",
            f.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            flag,
        ]);
        assert_eq!(r.status.code(), Some(2), "{flag}");
        let err = String::from_utf8_lossy(&r.stderr);
        assert!(
            err.contains(&format!("unexpected argument '{flag}'")),
            "{flag}: {err}"
        );
        assert!(!out.exists(), "{flag}: nothing written");
    }
    // Without them the same export still succeeds (ac1 compares the bytes).
    render(&[], dir.join("b.png").to_str().unwrap());
    let help = tiza_ok(&["render", "--help"]);
    assert!(
        !help.contains("--brand-font")
            && !help.contains("--mono-labels")
            && !help.contains("Serif"),
        "{help}"
    );
}

#[test]
fn ac2_mcp_export_image_has_no_brand_font_and_ignores_it() {
    let dir = scratch("t134-mcp");
    let doc = tiza_core::fs::open(fixture("t-050/export.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let mut mcp = McpClient::start(&live.socket);
    let tools = mcp.tools();
    let export = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "export_image")
        .expect("export_image");
    assert!(
        export["inputSchema"]["properties"]
            .get("brand_font")
            .is_none()
    );
    assert!(!export.to_string().contains("Serif"));
    for t in tools.as_array().unwrap() {
        jsonschema::validator_for(&t["inputSchema"])
            .unwrap_or_else(|e| panic!("{}: {e}", t["name"]));
    }
    // An old client that still sends it gets the monospace export.
    let a = dir.join("a.png");
    let b = dir.join("b.png");
    let (err, text, _) = mcp.call(
        "export_image",
        json!({"path": a.to_str().unwrap(), "step": 9, "width": 100}),
    );
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call(
        "export_image",
        json!({"path": b.to_str().unwrap(), "step": 9, "width": 100, "brand_font": true}),
    );
    assert!(!err, "{text}");
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
}

#[test]
fn ac3_libron_is_brand_material_outside_the_crates() {
    let r = root();
    let brand = r.join("docs/brand/fonts");
    for face in ["Regular", "Bold", "Italic", "BoldItalic"] {
        assert!(
            brand.join(format!("TizaSerif-{face}.ttf")).exists(),
            "{face}"
        );
        assert!(
            !r.join(format!(
                "crates/tiza-export/assets/fonts/TizaSerif-{face}.ttf"
            ))
            .exists(),
            "{face} left the crate"
        );
    }
    let license = std::fs::read_to_string(brand.join("LICENSE-TizaSerif-Libron-OFL.txt")).unwrap();
    assert!(license.contains("Reserved Font Name Libron") && license.contains("OPEN FONT LICENSE"));
    assert!(
        !r.join("crates/tiza-export/assets/fonts/LICENSE-TizaSerif-Libron-OFL.txt")
            .exists()
    );
    let readme = std::fs::read_to_string(brand.join("README.md")).unwrap();
    assert!(
        readme.contains("for the brand work — the README wordmark, thumbnails and video titles")
            && readme.contains("Reserved Font Name")
    );
    assert!(readme.contains("not compiled"));
    let fonts =
        std::fs::read_to_string(r.join("crates/tiza-export/assets/fonts/README.md")).unwrap();
    assert!(fonts.contains("docs/brand/fonts"));
}

#[test]
fn ac4_docs_record_the_removal() {
    let r = root();
    let adr = std::fs::read_to_string(r.join("docs/adr/0006-exportacion-svg-png.md")).unwrap();
    assert!(
        adr.contains(
            "Superseded on 2026-10-18 (Luis's decision): the serif is no longer in the binary."
        ) && adr.contains("docs/brand/fonts")
    );
    let changelog = std::fs::read_to_string(r.join("CHANGELOG.md")).unwrap();
    let unreleased = &changelog[..changelog.find("\n## [0").unwrap_or(changelog.len())];
    let removed = &unreleased[unreleased.find("### Removed").expect("Removed section")..];
    // (The CHANGELOG of 0.1.0 was condensed in t-217; `--brand-font` came
    // and went after 0.0.1, so only `--mono-labels` is a removal.)
    assert!(
        removed.contains("The binary no longer contains the brand serif")
            && removed.contains("--mono-labels")
    );
    let readme = std::fs::read_to_string(r.join("README.md")).unwrap();
    assert!(
        !readme.contains("`--brand-font` (notes"),
        "no longer an option"
    );
    // Rework: no public doc may still promise serif notes in the exports.
    assert!(
        !readme.contains("serif notes"),
        "README: exports are all JetBrains Mono"
    );
    assert!(
        !unreleased.contains("the brand serif is opt-in"),
        "CHANGELOG: t-068's entry must not contradict t-134's"
    );
}

/// FNV-1a, enough to tell that a file did not change.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn ac5_jetbrains_mono_stays_complete() {
    let dir = root().join("crates/tiza-export/assets/fonts");
    for (face, size, hash) in [
        ("Regular", 273_900, 0x4a1b_35e9_fc69_20d2_u64),
        ("Bold", 277_828, 0x6ab8_99db_bae4_1e07),
        ("Italic", 276_840, 0x328e_eb84_de91_fb86),
        ("BoldItalic", 279_832, 0x2643_2855_1691_33bf),
    ] {
        let b = std::fs::read(dir.join(format!("JetBrainsMono-{face}.ttf"))).unwrap();
        assert_eq!((b.len(), fnv(&b)), (size, hash), "{face}");
    }
}
