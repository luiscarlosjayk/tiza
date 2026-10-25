//! `tiza check` panicked in a debug build on a label with zero-width
//! spaces: «step 1: the take at [174, 174] columns is not the one at 400»
//! (release gave the right width through its safety net). The width from
//! which the take no longer changes was bounded with `display_width`, which
//! counts U+200B as 0 columns, while the canvas draws it in a cell of its
//! own: a note of 100 columns with 25 of them takes 125.
//!
//! Acceptance criteria:
//! - AC1 A debug build never panics on that input: the bound measures a
//!   label as the canvas draws it.
//! - AC2 The same announced width as release gave (`zwsp-note.tiza`: 83),
//!   and the safety net still catches any other mismatch.
//! - AC3 Random documents with ZWSP, ZWJ sequences and combining marks;
//!   this file; CHANGELOG.
//!
//! Corner cases: ZWJ sequences, combining marks, NBSP, the ideographic
//! space, a word joiner and a lone combining mark in a label.

use serde_json::{Value, json};
use tiza_core::check::{Bound, last_step, min_clean_width, saturation_bounds, saturation_width};
use tiza_core::layout::{LayoutOptions, MAX_WIDTH, MIN_WIDTH, layout};
use tiza_qa::{bin, fixture};

fn lay(doc: &tiza_core::Document, width: usize, step: u32) -> tiza_core::layout::Layout {
    let opts = LayoutOptions {
        width,
        step,
        line_numbers: doc.numbers,
        margin_cap: doc.margin.map(usize::from),
        ..Default::default()
    };
    layout(doc, &tiza_core::Theme::dark(), &opts)
}

/// No step is searched from the widest width (the bound held), and the
/// take at the bound is the take at the widest width at every step.
fn bounded(doc: &tiza_core::Document) -> Result<(), String> {
    let theme = tiza_core::Theme::dark();
    let net: Vec<u32> = saturation_bounds(doc, &theme)
        .into_iter()
        .filter(|(_, b)| *b == Bound::Fallback)
        .map(|(s, _)| s)
        .collect();
    if !net.is_empty() {
        return Err(format!("steps {net:?}: the bound is wrong"));
    }
    let sat = saturation_width(doc, &theme);
    for step in 0..=last_step(doc) {
        let (a, b) = (lay(doc, sat, step), lay(doc, MAX_WIDTH, step));
        if a.clipped_labels != b.clipped_labels || a.rows != b.rows {
            return Err(format!(
                "step {step}: the take at {sat} is not the one at {MAX_WIDTH}"
            ));
        }
    }
    Ok(())
}

/// The narrowest clean width, the slow way: every width, every step.
fn clean_the_slow_way(doc: &tiza_core::Document) -> Option<usize> {
    let clips = |w: usize| (0..=last_step(doc)).any(|s| !lay(doc, w, s).clipped_labels.is_empty());
    match (MIN_WIDTH..=MAX_WIDTH).rev().find(|&w| clips(w)) {
        None => Some(MIN_WIDTH),
        Some(MAX_WIDTH) => None,
        Some(w) => Some(w + 1),
    }
}

fn doc_of(v: &Value) -> tiza_core::Document {
    tiza_core::fs::parse_document(&v.to_string(), None).unwrap()
}

#[test]
fn ac1_ac2_the_zwsp_note_checks_without_a_panic_and_says_83() {
    // Test step 1, with the debug build the tests run. (83 is what release
    // announced before; that the note is cut at the canvas edge from 172 to
    // 192 columns without a `…` is the layout's, for another card.)
    let out = std::process::Command::new(bin())
        .arg("check")
        .arg(fixture("t-208/zwsp-note.tiza"))
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{text}{err}");
    assert!(!err.contains("panicked"), "{err}");
    assert!(
        text.contains("no label is clipped at 83 or wider"),
        "{text}"
    );
    let doc = tiza_core::fs::load_document(&fixture("t-208/zwsp-note.tiza")).unwrap();
    bounded(&doc).unwrap();
    assert_eq!(min_clean_width(&doc, &tiza_core::Theme::dark()), Some(83));
}

/// A note, a highlight's label, a mark's label and a callout, each with
/// `text`, on a few lines of code.
fn labelled(text: &str) -> Value {
    json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1},
        "code": ["fn main() {", "    let a = 1;", "    let b = 2;", "    a + b", "}"],
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 1, "text": text},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 2, "start": 4, "end": 9}, "label": text},
            {"id": "m1", "step": 2, "type": "mark", "kind": "warn", "span": {"line": 3, "start": 4, "end": 9}, "label": text},
            {"id": "c1", "step": 2, "type": "callout", "from_line": 4, "to_line": 4, "text": text}
        ]
    })
}

#[test]
fn corner_other_zero_width_and_wide_spaces() {
    for (what, piece) in [
        ("zero-width space", "word\u{200b}"),
        ("ZWJ sequence", "👩\u{200d}💻"),
        ("combining mark", "e\u{301}"),
        ("lone combining mark", "\u{301}x"),
        ("word joiner", "join\u{2060}"),
        ("no-break space", "no\u{a0}"),
        ("ideographic space", "全\u{3000}"),
        ("byte order mark", "bom\u{feff}"),
    ] {
        for n in [5, 25, 60] {
            let text = piece.repeat(n);
            let doc = doc_of(&labelled(&text));
            bounded(&doc).unwrap_or_else(|e| panic!("{what} ×{n}: {e}"));
            assert_eq!(
                min_clean_width(&doc, &tiza_core::Theme::dark()),
                clean_the_slow_way(&doc),
                "{what} ×{n}"
            );
        }
    }
}

#[test]
fn rev_a_callout_with_a_character_wider_than_its_width() {
    // One grapheme 14 columns wide in a callout 8 wide: the block grows to
    // hold it, and the note beside it moves; the bound counts it.
    for text in [
        "\u{1100}".repeat(7),
        format!("\u{915}{}", "\u{93e}".repeat(13)),
    ] {
        let doc = doc_of(&json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1},
            "code": ["fn main() {", "    let a = 1;", "    let b = 2;", "    let c = 3;", "    a + b + c", "}"],
            "annotations": [
                {"id": "c1", "step": 1, "type": "callout", "from_line": 2, "to_line": 2, "text": text, "width": 8},
                {"id": "n1", "step": 1, "type": "note", "line": 2, "text": "the borrow checker moves a value"}
            ]
        }));
        bounded(&doc).unwrap_or_else(|e| panic!("{text:?}: {e}"));
        assert_eq!(
            min_clean_width(&doc, &tiza_core::Theme::dark()),
            clean_the_slow_way(&doc),
            "{text:?}"
        );
    }
}
