//! Where a label goes: the cases that Luis saw in a tour and decided
//! (2026-10-30), in `qa/fixtures/render/label_places/tour.tiza`.
//!
//! - A label of a highlight or a mark that goes under its line starts under
//!   its span when it fits there. It sat at the start of the code, far from
//!   its span.
//! - The label of an arrow over several lines goes beside the code of its
//!   target line, after the arrow's own glyph (`─▶ limit`), when the code
//!   column holds it; else in the margin. It sat in the margin, past the
//!   border of a box.
//! - Brackets on a span with no space on one side go under the span as a
//!   pair, `⌊` and `⌋`. Before, `⌈` was beside the span and `⌋` under it.

use tiza_qa::{fixture, tiza_ok};

/// `tiza render tour.tiza --step STEP --width WIDTH --format txt`.
fn render(step: u32, width: u32) -> String {
    let path = fixture("render/label_places/tour.tiza");
    let (step, width) = (step.to_string(), width.to_string());
    tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--step",
        &step,
        "--width",
        &width,
        "--format",
        "txt",
    ])
}

/// The row of code line `n`.
fn row_of(text: &str, n: u32) -> usize {
    let start = format!("{n:>3} ");
    text.lines()
        .position(|r| r.starts_with(&start))
        .unwrap_or_else(|| panic!("no row for L{n}\n{text}"))
}

/// The column, in characters, where `pat` starts in `row`.
fn col(row: &str, pat: &str) -> usize {
    let at = row
        .find(pat)
        .unwrap_or_else(|| panic!("no «{pat}» in «{row}»"));
    row[..at].chars().count()
}

#[test]
fn a_label_under_its_line_starts_under_its_span() {
    // At 80 columns the block of the callout takes the margin beside L14,
    // so the label of the underline goes under L14, after the row of the
    // underline.
    let t = render(4, 80);
    let rows: Vec<&str> = t.lines().collect();
    let l14 = row_of(&t, 14);
    let underline = rows[l14 + 1];
    let label = rows[l14 + 2];
    assert!(label.contains("· full?"), "{t}");
    assert_eq!(col(label, "·"), col(underline, "‾"), "{t}");
}

#[test]
fn an_arrow_label_goes_beside_the_code_of_its_target_line() {
    for width in [80, 120] {
        let t = render(3, width);
        let rows: Vec<&str> = t.lines().collect();
        let l5 = rows[row_of(&t, 5)];
        assert!(l5.contains("limit: usize,  ─▶ limit"), "at {width}:\n{t}");
        // Not in the margin too: the code and the label only.
        assert_eq!(l5.matches("limit").count(), 2, "at {width}:\n{t}");
    }
}

#[test]
fn brackets_with_no_space_on_one_side_go_under_the_span_as_a_pair() {
    // `self.items.pop_front()` has a space before it and the `;` glued
    // after it.
    let t = render(5, 80);
    let rows: Vec<&str> = t.lines().collect();
    let l15 = row_of(&t, 15);
    let code = rows[l15];
    assert!(!code.contains('⌈') && !code.contains('⌉'), "{t}");
    let span = "self.items.pop_front()";
    let first = col(code, span);
    let last = first + span.chars().count() - 1;
    let under: Vec<char> = rows[l15 + 1].chars().collect();
    assert_eq!((under[first], under[last]), ('⌊', '⌋'), "{t}");
}
