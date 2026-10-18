//! t-137 — The SVG export subsets each embedded face to the glyphs the frame uses.
//!
//! Acceptance criteria:
//! - AC1 Each embedded face is subset to the characters its runs draw (plus
//!   what GSUB/GPOS reach from them) at export time; the font files in the
//!   binary are untouched.
//! - AC2 Same output on screen: Chromium screenshots of the old and new SVG
//!   identical (the t-130 method), PNG/GIF bytes unchanged.
//! - AC3 Sizes before/after in the note: step-7 SVG of
//!   `examples/ownership.rs.tiza` (1,303,959 after t-130) and a plain
//!   document (367,968 for `qa/fixtures/t-130/plain.tiza` at 100 columns).
//! - AC4 Any new crate follows ADR 0004 (registry row) and passes cargo-deny;
//!   this test; CHANGELOG.
//!
//! Corner cases: ligatures and contextual alternates (the subset keeps the
//! glyphs GSUB substitutes); characters only in the symbols face (★ ✎ ⇄) and
//! combining marks (e + U+0301 is drawn with the font's `é`);
//! `--no-embed-fonts` unchanged.
//!
//! The pixel comparison in Chromium (AC2) is a manual step on the card: it
//! needs a browser, and the old SVG comes from the previous binary.

use std::collections::BTreeMap;

use base64::Engine as _;
use skrifa::MetadataProvider as _;
use skrifa::raw::TableProvider as _;
use tiza_qa::{example, fixture, scratch, tiza_ok};

/// Renders `file` (extra args appended) to `name.EXT` and returns the bytes.
fn render(name: &str, file: &std::path::Path, ext: &str, extra: &[&str]) -> Vec<u8> {
    let dir = scratch(name);
    let out = dir.join(format!("a.{ext}"));
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
    std::fs::read(&out).unwrap()
}

fn svg(name: &str, file: &std::path::Path, extra: &[&str]) -> String {
    String::from_utf8(render(name, file, "svg", extra)).unwrap()
}

/// `(weight, style)` → the embedded font bytes, per `@font-face` block.
fn faces(svg: &str) -> BTreeMap<(String, u16, bool), Vec<u8>> {
    let mut out = BTreeMap::new();
    for l in svg.lines().filter(|l| l.starts_with("@font-face")) {
        let family = l
            .split("font-family:\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let weight: u16 = l.split("font-weight:").nth(1).unwrap()[..3]
            .parse()
            .unwrap();
        let italic = l.contains("font-style:italic");
        let b64 = l
            .split("base64,")
            .nth(1)
            .unwrap()
            .split(')')
            .next()
            .unwrap();
        let data = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        out.insert((family.to_string(), weight, italic), data);
    }
    out
}

fn mono(weight: u16, italic: bool) -> (String, u16, bool) {
    ("JetBrains Mono".to_string(), weight, italic)
}

fn symbols() -> (String, u16, bool) {
    ("Tiza Symbols".to_string(), 400, false)
}

/// The whole face shipped in the binary for a key of [`faces`].
fn whole(key: &(String, u16, bool)) -> &'static [u8] {
    tiza_export::fonts::FACES
        .iter()
        .find(|f| f.family == key.0 && f.weight == key.1 && f.italic == key.2)
        .unwrap()
        .data
}

fn glyph_count(data: &[u8]) -> u16 {
    skrifa::FontRef::new(data)
        .unwrap()
        .maxp()
        .unwrap()
        .num_glyphs()
}

fn maps(data: &[u8], ch: char) -> bool {
    skrifa::FontRef::new(data)
        .unwrap()
        .charmap()
        .map(ch)
        .is_some_and(|g| g.to_u32() != 0)
}

/// The text of every `<text>` run with its face, unescaped.
fn runs(svg: &str) -> Vec<((String, u16, bool), String)> {
    svg.lines()
        .filter(|l| l.starts_with("<text "))
        .map(|l| {
            let class = l
                .split(" class=\"")
                .nth(1)
                .map(|c| c.split('"').next().unwrap())
                .unwrap_or("");
            let bold = class.split(' ').any(|c| c == "b");
            let italic = class.split(' ').any(|c| c == "i");
            let text = l
                .split("xml:space=\"preserve\">")
                .nth(1)
                .unwrap()
                .trim_end_matches("</text>")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&amp;", "&");
            (mono(if bold { 700 } else { 400 }, italic), text)
        })
        .collect()
}

/// Every character a run draws that its whole face (or, failing it, the
/// whole symbols face) has is in the embedded subset of that face.
fn assert_runs_covered(svg: &str) {
    let f = faces(svg);
    for (key, text) in runs(svg) {
        for ch in text.chars() {
            if maps(whole(&key), ch) {
                assert!(maps(&f[&key], ch), "{ch:?} missing from {key:?}");
            } else if maps(whole(&symbols()), ch) {
                assert!(
                    maps(&f[&symbols()], ch),
                    "{ch:?} missing from the symbols subset"
                );
            }
        }
    }
}

#[test]
fn ac1_ac3_ownership_step_7_embeds_subsets_of_the_same_faces() {
    let s = svg("t137-own7", &example("ownership.rs.tiza"), &["--step", "7"]);
    let f = faces(&s);
    // The same faces as t-130 chose, each one cut down.
    assert_eq!(
        f.keys().cloned().collect::<Vec<_>>(),
        vec![
            mono(400, false),
            mono(400, true),
            mono(700, false),
            symbols()
        ]
    );
    for (key, data) in &f {
        let (n, all) = (glyph_count(data), glyph_count(whole(key)));
        assert!(n < all / 2, "{key:?}: {n} of {all} glyphs");
        assert!(
            data.len() < whole(key).len() / 2,
            "{key:?}: {} bytes",
            data.len()
        );
    }
    assert_runs_covered(&s);
    // 1,303,959 bytes after t-130.
    assert!(s.len() < 300_000, "{} bytes", s.len());
}

#[test]
fn ac1_ac3_a_plain_document_is_tens_of_kb() {
    let s = svg("t137-plain", &fixture("t-130/plain.tiza"), &[]);
    assert_eq!(faces(&s).len(), 1, "regular only (t-130)");
    assert_runs_covered(&s);
    // 367,968 bytes after t-130.
    assert!(s.len() < 60_000, "{} bytes", s.len());
}

#[test]
fn ac1_font_files_in_the_binary_stay_complete() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/tiza-export/assets/fonts");
    for (face, file) in tiza_export::fonts::FACES.iter().zip([
        "JetBrainsMono-Regular.ttf",
        "JetBrainsMono-Bold.ttf",
        "JetBrainsMono-Italic.ttf",
        "JetBrainsMono-BoldItalic.ttf",
        "TizaSymbols.ttf",
    ]) {
        assert_eq!(face.data, std::fs::read(dir.join(file)).unwrap(), "{file}");
    }
    assert_eq!(glyph_count(tiza_export::fonts::FACES[0].data), 1743);
}

#[test]
fn corner_ligatures_keep_the_glyphs_gsub_substitutes() {
    let s = svg("t137-liga", &fixture("t-137/ligatures.tiza"), &[]);
    assert!(s.contains("-&gt;") && s.contains("=&gt;") && s.contains("!="));
    let f = faces(&s);
    let regular = &f[&mono(400, false)];
    let font = skrifa::FontRef::new(regular).unwrap();
    assert!(font.gsub().is_ok(), "GSUB kept");
    // Glyphs no character maps to: the `calt` ligature pieces of `->`, `=>`,
    // `!=`, `<=`, `::`… that the browser substitutes inside a run.
    let mapped: std::collections::BTreeSet<u32> =
        font.charmap().mappings().map(|(_, g)| g.to_u32()).collect();
    let unmapped = u32::from(glyph_count(regular)) - mapped.len() as u32 - 1; // .notdef
    assert!(
        unmapped >= 10,
        "only {unmapped} glyphs reached through GSUB"
    );
    assert_runs_covered(&s);
}

#[test]
fn corner_combining_marks_bring_the_composed_letter() {
    let s = svg("t137-marks", &fixture("t-137/ligatures.tiza"), &[]);
    // The italic note has `e` + U+0301 and no precomposed `é`: the shaper
    // draws the font's `é`, so the italic subset must have it.
    let note: String = runs(&s)
        .into_iter()
        .filter(|(k, _)| *k == mono(400, true))
        .map(|(_, t)| t)
        .collect();
    assert!(note.contains("e\u{301}") && !note.contains('é'), "{note}");
    let italic = &faces(&s)[&mono(400, true)];
    assert!(maps(italic, 'é') && maps(italic, '\u{301}') && maps(italic, 'e'));
}

#[test]
fn corner_symbols_only_in_the_fallback_face() {
    let s = svg("t137-sym", &fixture("t-137/ligatures.tiza"), &[]);
    let f = faces(&s);
    let sym = &f[&symbols()];
    for ch in ['★', '✎', '⇄'] {
        assert!(maps(sym, ch), "{ch}");
        assert!(
            !maps(&f[&mono(400, false)], ch),
            "{ch} is not JetBrains Mono's"
        );
    }
    assert!(glyph_count(sym) < 20, "{} glyphs", glyph_count(sym));
}

#[test]
fn ac2_drawing_and_png_do_not_depend_on_the_subsets() {
    let f = fixture("t-137/ligatures.tiza");
    let with = svg("t137-d-with", &f, &[]);
    let without = svg("t137-d-without", &f, &["--no-embed-fonts"]);
    let drawing = |s: &str| {
        s.lines()
            .filter(|l| !l.starts_with("@font-face"))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(drawing(&with), drawing(&without));
    // The PNG rasterizer loads the whole faces: embedding changes nothing.
    assert_eq!(
        render("t137-p-with", &f, "png", &[]),
        render("t137-p-without", &f, "png", &["--no-embed-fonts"])
    );
}

#[test]
fn corner_no_embed_fonts_unchanged() {
    let s = svg(
        "t137-none",
        &fixture("t-137/ligatures.tiza"),
        &["--no-embed-fonts"],
    );
    assert!(!s.contains("@font-face"));
    assert!(s.len() < 20_000, "{} bytes", s.len());
}
