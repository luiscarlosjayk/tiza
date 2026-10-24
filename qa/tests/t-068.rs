//! t-068 — Brand typography in SVG/PNG exports. Shipped Tiza Serif (a Libron
//! subset, OFL) as an opt-in for notes, labels and legends; the serif left the
//! binary in t-134 (2026-10-18), so the tests that needed it (AC1 serif faces
//! and prose runs, AC4 PNG with the serif) are retired and live on, inverted,
//! in `t-134.rs`. What still holds:
//!
//! - AC2 A mark label's glyphs (`━▶`, `╳`, `✎▶`) stay their own run on the
//!   grid and the words start on the next cell.
//! - AC3 (flipped on Luis's review, 2026-10-17) the monospace is the
//!   export font; no prose runs; the text exports are unaffected.
//! - AC5 Fonts README and ADR 0006 record the subset, the OFL renaming
//!   ("Libron" is a Reserved Font Name) — now in `docs/brand/fonts/`.

use tiza_qa::{fixture, tiza, tiza_ok};

fn svg(args: &[&str]) -> String {
    let f = fixture("t-068/ownership.rs.tiza");
    let mut full = vec![
        "render",
        f.to_str().unwrap(),
        "--out",
        "-",
        "--format",
        "svg",
        "--step",
        "7",
        "--width",
        "100",
    ];
    full.extend_from_slice(args);
    tiza_ok(&full)
}

fn texts(svg: &str) -> Vec<&str> {
    svg.match_indices("<text ")
        .map(|(i, _)| &svg[i..svg[i..].find("</text>").map(|e| i + e + 7).unwrap()])
        .collect()
}

#[test]
fn ac2_mark_glyphs_keep_their_own_run() {
    let s = svg(&[]);
    let runs = texts(&s);
    let glyph = runs
        .iter()
        .find(|t| t.ends_with(">━▶</text>"))
        .expect("glyph run");
    assert!(
        glyph.contains("textLength") && !glyph.contains("class=\"p"),
        "{glyph}"
    );
    let words = runs
        .iter()
        .find(|t| t.contains("> move: s moves"))
        .expect("words run");
    assert!(
        words.contains("textLength") && !words.contains("class=\"p"),
        "{words}"
    );
    // The words start right after the glyph cells (2 cells = 2 × 9.6 px at 16 px).
    let x = |t: &str| {
        t.split("x=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse::<f32>()
            .unwrap()
    };
    assert!(
        (x(words) - x(glyph) - 2.0 * 9.6).abs() < 0.01,
        "{} vs {}",
        x(words),
        x(glyph)
    );
    let drop = runs
        .iter()
        .find(|t| t.ends_with(">╳</text>"))
        .expect("drop glyph");
    assert!(!drop.contains("class=\"p"));
}

#[test]
fn ac3_monospace_is_the_export_font() {
    let s = svg(&[]);
    assert!(!s.contains("class=\"p"), "no prose runs");
    assert!(!s.contains(".p{"), "no prose style");
    assert!(
        !s.contains("Tiza Serif"),
        "the serif is not embedded any more (t-134)"
    );
    let runs = texts(&s);
    let born = runs.iter().find(|t| t.contains(">s is born<")).unwrap();
    assert!(born.contains("textLength"), "on the grid: {born}");
    // The removed flag is rejected (t-134 made it a no-op, t-136 removed it).
    let f = fixture("t-068/ownership.rs.tiza");
    let out = tiza(&["render", f.to_str().unwrap(), "--out", "-", "--brand-font"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unexpected argument '--brand-font'"));
}

#[test]
fn ac5_docs_record_license_and_pairing() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let fonts = std::fs::read_to_string(root.join("docs/brand/fonts/README.md")).unwrap();
    assert!(
        fonts.contains("Tiza Serif")
            && fonts.contains("Libron")
            && fonts.contains("Reserved Font Name")
            && fonts.contains("pyftsubset")
    );
    assert!(
        !root.join("crates/tiza-export/fonts").exists(),
        "the duplicate font directory is gone"
    );
    assert!(
        root.join("docs/brand/fonts/LICENSE-TizaSerif-Libron-OFL.txt")
            .exists()
    );
    // ADR 0006 predates the rename to Tiza (ADR 0008): it records the font
    // under its first name and carries the rename note at the top.
    let adr = std::fs::read_to_string(root.join("docs/adr/0006-exportacion-svg-png.md")).unwrap();
    assert!(adr.contains("CodeDraw Serif") && adr.contains("ADR 0008"));
    assert!(!adr.contains("is not used yet"));
}
