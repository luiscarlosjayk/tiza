//! t-130 — The SVG export embeds only the font faces it actually uses.
//!
//! Acceptance criteria:
//! - AC1 The SVG writer collects which faces the frame actually draws
//!   (JetBrains Mono Regular/Bold/Italic/BoldItalic, Tiza Symbols) and emits
//!   an `@font-face` only for those; a face that no run uses is never
//!   embedded. Regular is always in.
//! - AC2 Same output on screen: the drawing (every line of the SVG but the
//!   `@font-face` blocks) is unchanged, the PNG does not depend on the
//!   embedded faces, and the t-050/t-068/t-134 tests still pass.
//! - AC3 Size: before/after bytes of the default step-7 SVG of
//!   `examples/ownership.rs.tiza` (1,677,197 bytes after t-134) and of a
//!   document with no bold/italic text, recorded in the card's note.
//! - AC4 (optional) Subsetting each face to the glyphs used: not done here
//!   (follow-up card); the font files themselves stay complete.
//! - AC5 This test (an unused face has no `@font-face`) and the CHANGELOG.
//!
//! Corner cases: `--no-embed-fonts` embeds nothing; the symbols face only
//! when a character the run's JetBrains Mono face lacks needs it; a box or
//! callout title brings the bold face.

use tiza_qa::{example, fixture, scratch, tiza_ok};

const REGULAR: &str =
    r#"@font-face{font-family:"JetBrains Mono";font-weight:400;font-style:normal"#;
const BOLD: &str = r#"@font-face{font-family:"JetBrains Mono";font-weight:700;font-style:normal"#;
const ITALIC: &str = r#"@font-face{font-family:"JetBrains Mono";font-weight:400;font-style:italic"#;
const BOLD_ITALIC: &str =
    r#"@font-face{font-family:"JetBrains Mono";font-weight:700;font-style:italic"#;
const SYMBOLS: &str = r#"@font-face{font-family:"Tiza Symbols";font-weight:400;font-style:normal"#;

/// Renders `file` to SVG (extra args appended) and returns the text.
fn svg(name: &str, file: &std::path::Path, extra: &[&str]) -> String {
    let dir = scratch(name);
    let out = dir.join("a.svg");
    let mut args = vec![
        "render",
        file.to_str().unwrap(),
        "--width",
        "100",
        "--out",
        out.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    tiza_ok(&args);
    std::fs::read_to_string(&out).unwrap()
}

/// The `@font-face` blocks of an SVG, without their data URIs.
fn faces(svg: &str) -> Vec<&str> {
    svg.lines()
        .filter(|l| l.starts_with("@font-face"))
        .map(|l| l.split(";src:").next().unwrap())
        .collect()
}

/// Every line but the `@font-face` blocks: what is drawn.
fn drawing(svg: &str) -> Vec<&str> {
    svg.lines()
        .filter(|l| !l.starts_with("@font-face"))
        .collect()
}

#[test]
fn ac1_a_document_with_no_bold_italic_or_symbols_embeds_regular_only() {
    let s = svg("t130-plain", &fixture("t-130/plain.tiza"), &[]);
    assert_eq!(faces(&s), vec![REGULAR]);
    assert!(!s.contains(BOLD) && !s.contains(SYMBOLS));
    // One face of ~0.37 MB in base64 instead of five.
    assert!(s.len() < 450_000, "{} bytes", s.len());
}

#[test]
fn ac1_a_box_title_brings_the_bold_face_and_nothing_else() {
    let s = svg("t130-box", &fixture("t-130/box.tiza"), &[]);
    assert!(s.contains(r#"class="b""#), "the box title is bold");
    assert_eq!(faces(&s), vec![REGULAR, BOLD]);
}

#[test]
fn ac1_the_symbols_face_comes_only_with_a_symbol_the_mono_lacks() {
    let s = svg("t130-sym", &fixture("t-130/symbols.tiza"), &[]);
    assert!(s.contains('★'));
    assert_eq!(faces(&s), vec![REGULAR, SYMBOLS]);
    // Box drawing is in JetBrains Mono: a box alone does not need the fallback.
    let b = svg("t130-sym-box", &fixture("t-130/box.tiza"), &[]);
    assert!(b.contains('│') || b.contains('─'));
    assert!(!b.contains(SYMBOLS));
}

#[test]
fn ac1_ac3_ownership_step_7_drops_the_unused_bold_italic() {
    let s = svg("t130-own7", &example("ownership.rs.tiza"), &["--step", "7"]);
    assert!(s.contains(r#"class="b""#) && s.contains(r#"class="i""#));
    assert!(!s.contains(r#"class="b i""#), "no bold italic text");
    assert_eq!(faces(&s), vec![REGULAR, BOLD, ITALIC, SYMBOLS]);
    assert!(!s.contains(BOLD_ITALIC));
    assert!(s.len() < 1_677_197, "{} bytes", s.len());
}

#[test]
fn ac2_the_drawing_is_the_same_with_or_without_the_faces() {
    for (name, f) in [
        ("plain", fixture("t-130/plain.tiza")),
        ("box", fixture("t-130/box.tiza")),
        ("symbols", fixture("t-130/symbols.tiza")),
        ("t050", fixture("t-050/export.tiza")),
    ] {
        let with = svg(&format!("t130-d-{name}"), &f, &[]);
        let without = svg(&format!("t130-n-{name}"), &f, &["--no-embed-fonts"]);
        assert_eq!(drawing(&with), drawing(&without), "{name}");
    }
}

#[test]
fn corner_no_embed_fonts_embeds_nothing() {
    let s = svg(
        "t130-none",
        &fixture("t-050/export.tiza"),
        &["--no-embed-fonts"],
    );
    assert!(faces(&s).is_empty());
    assert!(!s.contains("@font-face"));
}
