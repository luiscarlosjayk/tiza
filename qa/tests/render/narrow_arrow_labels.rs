//! Narrow mode: arrow labels attached to the arrowhead instead of
//! orphaned under the line.
//!
//! Acceptance criteria:
//! - AC1 With labels under the lines (narrow), an arrow's label sits on the
//!   arrowhead's row right after the code when it fits; otherwise on the row
//!   below behind the arrow's own glyph (`─▶ move`, its colour), never a
//!   bare `·`.
//! - AC2 Marks keep glyph + legend, notes keep `┆`; only arrow labels change.
//! - AC3 A label inside a box range stays tied to its arrow and never touches
//!   the box border.
//! - AC4 Wide mode was unchanged then. Since a later decision of Luis, the
//!   label of an arrow over several lines also goes beside the code of its
//!   target line in wide mode, after the arrow's glyph, when the code column
//!   holds it; else in the margin.
//! - AC5 This file, with the ownership example at 80 and 60 columns (its
//!   copy from then in `qa/fixtures/render/narrow_arrow_labels/`: the example changed later).

use serde_json::json;
use tiza_core::layout::{LayoutOptions, layout};
use tiza_qa::{Live, fixture};

fn ownership() -> tiza_core::Document {
    tiza_core::fs::open(
        fixture("render/narrow_arrow_labels/ownership.rs.tiza")
            .to_str()
            .unwrap(),
    )
    .unwrap()
    .document
}

fn text(doc: &tiza_core::Document, width: usize, step: u32) -> String {
    layout(
        doc,
        &tiza_core::Theme::dark(),
        &LayoutOptions {
            width,
            step,
            ..Default::default()
        },
    )
    .canvas
    .to_plain_text()
}

#[test]
fn labels_sit_beside_the_arrowhead_at_80_and_60_columns() {
    let doc = ownership();
    for width in [80usize, 60] {
        let t = text(&doc, width, 7);
        // The three arrows into functions: label on the target line, after
        // the code, with the arrow's own glyph (solid, dashed, thick).
        for (line, glyph) in [
            ("fn takes(s: String) {  ─▶ move", "─▶"),
            ("fn len(s: &String) -> usize {  ╌▶ borrow &", "╌▶"),
            ("fn push(s: &mut String) {  ━▶ &mut", "━▶"),
        ] {
            assert!(t.contains(line), "width {width}: missing «{line}»\n{t}");
            let _ = glyph;
        }
        // No orphaned `· move` / `· borrow &` / `· &mut` rows any more.
        for orphan in ["· move", "· borrow &", "· &mut"] {
            assert!(
                !t.contains(orphan),
                "width {width}: «{orphan}» still there\n{t}"
            );
        }
        // AC3: inside the `takes` box the label stays left of the border.
        let row = t
            .lines()
            .find(|l| l.contains("fn takes(s: String)"))
            .unwrap();
        let border = row.rfind('│').unwrap();
        let label_end = row.find("─▶ move").unwrap() + "─▶ move".len();
        assert!(label_end < border, "{row}");
        // AC2: marks and notes are as before.
        assert!(t.contains("· ╳ drop: s dies here"), "{t}");
        assert!(t.contains("┆ t is still valid"), "{t}");
        assert!(
            t.contains("· ━▶ move: s moves into takes"),
            "same-line arrow: unchanged"
        );
    }
}

#[test]
fn a_label_that_does_not_fit_goes_below_with_the_glyph() {
    let mut doc = ownership();
    let id = doc
        .annotations
        .iter()
        .find(|a| matches!(&a.shape, tiza_core::model::Shape::Arrow { to, label: Some(l), .. } if to.line == 15 && l == "move"))
        .map(|a| a.id.clone())
        .unwrap();
    let long = "the String is moved into takes and freed there, at the end";
    if let Some(a) = doc.annotations.iter_mut().find(|a| a.id == id)
        && let tiza_core::model::Shape::Arrow { label, .. } = &mut a.shape
    {
        *label = Some(long.into());
    }
    let t = text(&doc, 80, 7);
    let row = t
        .lines()
        .find(|l| l.contains("fn takes(s: String)"))
        .unwrap();
    assert!(!row.contains("─▶"), "too long for the row: {row}");
    let below = t
        .lines()
        .skip_while(|l| !l.contains("fn takes(s: String)"))
        .nth(1)
        .unwrap();
    assert!(below.contains("─▶ the String is moved"), "{below}");
    assert!(!below.contains("· the String"), "{below}");
    // Still left of the box border (clipped with … if need be).
    assert!(below.rfind('│').unwrap() > below.find("─▶").unwrap());
}

#[test]
fn two_arrows_into_one_line_follow_each_other() {
    // A long line keeps the margin under 14 columns (narrow mode) while
    // the short target line leaves room beside its code.
    let mut doc = tiza_core::Document::from_code(
        "fn a() {}\nfn b() {}\nlet long_line = String::from(\"0123456789\");\nfn target() {}\n",
        "rs",
        None,
    );
    for (i, (from, label)) in [(1u32, "one"), (2, "two")].iter().enumerate() {
        doc.add(tiza_core::model::Annotation {
            id: format!("a{i}"),
            step: 1,
            color: None,
            effect: None,
            markers: None,
            file: None,
            shape: tiza_core::model::Shape::Arrow {
                from: tiza_core::model::Anchor {
                    line: *from,
                    col: 0,
                },
                to: tiza_core::model::Anchor { line: 4, col: 3 },
                label: Some((*label).into()),
                line: Default::default(),
            },
        })
        .unwrap();
    }
    let t = text(&doc, 56, 1);
    let row = t.lines().find(|l| l.contains("fn target()")).unwrap();
    assert!(row.contains("─▶ one  ─▶ two"), "{t}");
}

#[test]
fn wide_mode_and_the_tui_agree() {
    let doc = ownership();
    // Wide mode attaches the label to its arrowhead too, beside the code of
    // the target line, and not in the margin.
    let wide = text(&doc, 120, 7);
    let row = wide
        .lines()
        .find(|l| l.contains("fn takes(s: String)"))
        .unwrap();
    assert!(row.contains("fn takes(s: String) {  ─▶ move"), "{wide}");
    assert_eq!(row.matches("move").count(), 1, "{wide}");
    // The TUI at 80 columns shows the same attached label (presentation and
    // normal mode).
    let dir = tiza_qa::scratch("live");
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 7}));
    let s = live.screen(80, 40);
    assert!(s.contains("fn takes(s: String) {  ─▶ move"), "{s}");
}
