//! What the take draws with `"center": true`. The block of the take (the
//! gutter, the arrow lanes, the box rails and the widest code line of
//! every step, spaces included) goes to the middle of the width, with one
//! offset for every step. A block that does not fit stays at the left, as
//! without `center`.

use serde_json::{Value, json};
use tiza_core::Document;
use tiza_core::canvas::Cell;
use tiza_core::layout::{Layout, LayoutOptions, layout, layout_traced, stable_geometry};
use tiza_qa::scratch;

/// Writes a `.tiza` with `fields` over a document of `code` and opens it.
fn open(name: &str, code: &[&str], fields: Value) -> Document {
    let dir = scratch(name);
    let path = dir.join("doc.tiza");
    let mut doc = json!({"schema": 2, "source": {"language": "rs", "first_line": 1},
        "code": code, "annotations": []});
    for (k, v) in fields.as_object().unwrap() {
        doc[k] = v.clone();
    }
    std::fs::write(&path, doc.to_string()).unwrap();
    tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document
}

fn opts(width: usize, step: u32) -> LayoutOptions {
    LayoutOptions {
        width,
        step,
        stable: true,
        ..Default::default()
    }
}

/// The take of `doc` at `width` and `step`, with its `numbers` and its
/// `margin`, as `tiza render` lays it out.
fn take(doc: &Document, width: usize, step: u32) -> Layout {
    let o = LayoutOptions {
        line_numbers: doc.numbers_or(true),
        margin_cap: doc.margin.map(usize::from),
        ..opts(width, step)
    };
    layout(doc, &tiza_core::Theme::dark(), &o)
}

/// `doc` without `center`.
fn uncentered(doc: &Document) -> Document {
    let mut d = doc.clone();
    d.center = false;
    d
}

/// The cells of `l`, row by row.
fn cells(l: &Layout) -> Vec<Vec<Cell>> {
    (0..l.canvas.height)
        .map(|y| {
            (0..l.canvas.width)
                .map(|x| l.canvas.get(x, y).cloned().unwrap_or_default())
                .collect()
        })
        .collect()
}

/// Asserts that `centered` is `plain` moved `dx` columns to the right: the
/// `dx` columns on its left are blank, and every other cell is the cell of
/// `plain` `dx` columns to the left (or blank past its right side).
fn assert_moved(centered: &Layout, plain: &Layout, dx: usize, what: &str) {
    let blank = Cell {
        style: tiza_core::Theme::dark().base(),
        ..Default::default()
    };
    assert_eq!(centered.rows, plain.rows, "{what}: the rows");
    assert_eq!(centered.left, dx, "{what}: the left columns");
    assert_eq!(
        centered.code_x,
        plain.code_x + dx,
        "{what}: the code column"
    );
    let (c, p) = (cells(centered), cells(plain));
    for (y, row) in c.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let want = x
                .checked_sub(dx)
                .and_then(|px| p[y].get(px))
                .unwrap_or(&blank);
            assert_eq!(cell, want, "{what}: the cell at column {x}, row {y}");
        }
    }
}

/// Lines 1-3: the widest is line 2, 14 columns.
const FN: [&str; 3] = ["fn main() {", "    let x = 1;", "}"];

#[test]
fn a_centered_take_is_the_take_of_the_narrower_width_moved_right() {
    let doc = open(
        "center-moved",
        &FN,
        json!({"center": true, "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 2, "text": "x is one"},
            {"id": "b1", "step": 2, "type": "box", "from_line": 1, "to_line": 3}]}),
    );
    // The block, by hand: the gutter (one digit, a cell before it and a
    // blank after it), the rail of the box, two blank columns, the widest
    // line, a blank column, the right rail and a blank column.
    let block = 3 + 1 + 2 + 14 + 1 + 1 + 1;
    // 82 columns: 59 are left, and the block goes 29 columns in (the odd
    // column is on the right).
    let width = 82;
    let dx = (width - block) / 2;
    assert_eq!(dx, 29);
    for step in 0..=2 {
        let centered = take(&doc, width, step);
        let plain = take(&uncentered(&doc), width - dx, step);
        assert_moved(&centered, &plain, dx, &format!("step {step}"));
    }
    // Without `center`, the take at the same width starts at the left.
    let plain = take(&uncentered(&doc), width, 1);
    assert_eq!(plain.left, 0);
    assert_ne!(
        take(&doc, width, 1).canvas.row_text(0),
        plain.canvas.row_text(0)
    );
}

#[test]
fn the_widest_step_sets_one_offset_for_every_step() {
    // Step 2 makes line 2 30 columns wide: steps 0 and 1 keep its offset.
    let doc = open(
        "center-widest-step",
        &FN,
        json!({"center": true, "numbers": false, "moves": [
            {"at_step": 2, "action": "code.patch", "op": "replace",
             "find": "let x = 1;", "content": "let x = 1 + 2 + 3 + 4 + 5 + 6;"}]}),
    );
    // No gutter: two blank columns, 30 columns of code, two blank columns.
    let block = 2 + 30 + 2;
    let width = 100;
    let left = (width - block) / 2;
    for step in 0..=2 {
        assert_eq!(take(&doc, width, step).left, left, "step {step}");
    }
}

#[test]
fn spaces_at_the_end_of_a_line_count() {
    // The second line is 12 columns wide because of its spaces: Tiza does
    // not trim them.
    let doc = open(
        "center-spaces",
        &["ab", "cd          "],
        json!({"center": true, "numbers": false}),
    );
    let block = 2 + 12 + 2;
    assert_eq!(take(&doc, 40, 1).left, (40 - block) / 2);
}

#[test]
fn a_short_word_is_centered_on_its_own_width() {
    let doc = open(
        "center-short",
        &["Fin"],
        json!({"center": true, "numbers": false}),
    );
    // The block is 2 + 3 + 2 columns; the rails of a box would be 10
    // columns apart, and they fit too.
    let l = take(&doc, 41, 1);
    assert_eq!(l.left, (41 - 7) / 2);
    // The word is in the middle: as many columns on its left as on its
    // right, give or take the odd one.
    let row = l.canvas.row_text(0);
    let before = row.len() - row.trim_start().len();
    let after = 41 - before - 3;
    assert!(before.abs_diff(after) <= 1, "{before} and {after}: {row:?}");
}

#[test]
fn content_that_does_not_fit_stays_as_without_center() {
    let doc = open(
        "center-too-wide",
        &FN,
        json!({"center": true, "annotations": [
            {"id": "b1", "step": 1, "type": "box", "from_line": 1, "to_line": 3}]}),
    );
    let block = 3 + 1 + 2 + 14 + 1 + 1 + 1;
    // One column narrower than the block, and the block itself: no room,
    // and the take is the one without `center`, cell for cell.
    for width in [block - 1, block] {
        let centered = take(&doc, width, 1);
        let plain = take(&uncentered(&doc), width, 1);
        assert_moved(&centered, &plain, 0, &format!("width {width}"));
    }
    // Two columns more: one on each side.
    assert_eq!(take(&doc, block + 2, 1).left, 1);
}

#[test]
fn the_gutter_and_the_arrow_lanes_go_with_the_code() {
    let doc = open(
        "center-gutter",
        &FN,
        json!({"center": true, "annotations": [
            {"id": "a1", "step": 1, "type": "arrow",
             "from": {"line": 1, "col": 3}, "to": {"line": 3, "col": 0}}]}),
    );
    let l = take(&doc, 80, 1);
    assert!(l.left > 0);
    // The number of line 1 is in the gutter, `left` columns in: not at the
    // left side of the take.
    let row = l.canvas.row_text(l.row_of_line[&1]);
    let number = row.find('1').unwrap();
    assert!(number >= l.left, "{row:?}");
    assert!(row[..l.left].trim().is_empty(), "{row:?}");
}

#[test]
fn a_linked_file_keeps_the_offset_of_the_widest_file() {
    let dir = scratch("center-link");
    std::fs::write(
        dir.join("wide.rs"),
        "fn wide() -> &'static str { \"a line of forty columns..\" }\n",
    )
    .unwrap();
    let path = dir.join("doc.tiza");
    let doc = json!({"schema": 2, "source": {"language": "rs", "first_line": 1},
        "code": FN, "annotations": [], "center": true, "numbers": false,
        "moves": [{"at_step": 2, "action": "code.link", "file": "wide.rs"}]});
    std::fs::write(&path, doc.to_string()).unwrap();
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let wide = "fn wide() -> &'static str { \"a line of forty columns..\" }"
        .chars()
        .count();
    let left = (100 - (2 + wide + 2)) / 2;
    assert_eq!(take(&doc, 100, 1).left, left, "the main file");
    assert_eq!(take(&doc, 100, 2).left, left, "the linked file");
}

#[test]
fn with_a_margin_cap_the_block_is_in_the_middle_of_the_whole_width() {
    // 120 columns, a block of 30 (2 + 26 + 2) and a margin cap of 20: the
    // block goes 45 columns in, and the canvas keeps the 120 columns, so
    // an export has as many blank columns on the right of the block as on
    // its left. The cap still ends the margin: the long note wraps in 20
    // cells.
    let code = ["fn main() {", "    let total = 1 + 2 + 3;", "}"];
    let fields = |margin: Option<u16>| {
        let mut f = json!({"center": true, "numbers": false, "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 2,
             "text": "a note that is longer than twenty cells"}]});
        if let Some(m) = margin {
            f["margin"] = json!(m);
        }
        f
    };
    let free = open("center-margin-free", &code, fields(None));
    let capped = open("center-margin-cap", &code, fields(Some(20)));
    let (a, b) = (take(&free, 120, 1), take(&capped, 120, 1));
    assert_eq!(b.left, (120 - 30) / 2);
    assert_eq!(a.left, b.left);
    assert_eq!(b.canvas.width, 120);
    let right_of_block = 120 - (b.left + 30);
    assert_eq!(right_of_block, b.left);
    // The note ends at the cap: 20 cells after the block, never further.
    for y in 0..b.canvas.height {
        let row = b.canvas.row_text(y);
        let used = row.trim_end().chars().count();
        assert!(
            used <= b.left + 30 + 20,
            "row {y} goes past the cap: {row:?}"
        );
    }
    // Without the cap, the note has the whole margin: one row.
    assert!(
        b.rows.len() > a.rows.len(),
        "the cap does not wrap the note"
    );
    // Without `center`, a capped canvas still ends at the cap.
    let plain = take(&uncentered(&capped), 120, 1);
    assert_eq!(plain.canvas.width, 30 + 20);
}

#[test]
fn an_enter_effect_draws_in_the_moved_cells() {
    let doc = open(
        "center-effect",
        &FN,
        json!({"center": true, "annotations": [
            {"id": "b1", "step": 1, "type": "box", "from_line": 1, "to_line": 3,
             "effect": "draw"}]}),
    );
    let (l, trace) = layout_traced(&doc, &tiza_core::Theme::dark(), &opts(80, 1), None);
    assert!(l.left > 0);
    assert!(!trace.cells.is_empty());
    for i in trace.cells.keys() {
        let x = i % l.canvas.width;
        assert!(
            x >= l.left,
            "a cell of the effect at column {x}, left of {}",
            l.left
        );
    }
}

#[test]
fn a_step_that_types_its_tenth_line_has_the_wide_gutter_from_its_start() {
    // Nine lines; step 2 types a tenth. While it types, the code on screen
    // has nine lines, but the gutter has two digits from the first frame,
    // so the code (and the offset of `center`) do not move.
    let code: Vec<String> = (1..=9).map(|i| format!("let v{i} = {i};")).collect();
    let code: Vec<&str> = code.iter().map(String::as_str).collect();
    let mut doc = open(
        "center-typing-gutter",
        &code,
        json!({"center": true, "moves": [
            {"at_step": 2, "action": "code.patch", "op": "insert",
             "after": "let v9 = 9;", "content": "let v10 = 10;"}]}),
    );
    // The step on screen is 2, and its code is still the code of step 1:
    // the first frame of the typing.
    doc.step = 2;
    let typing = LayoutOptions {
        typing: true,
        ..opts(80, 2)
    };
    let g = stable_geometry(&doc, &typing);
    assert_eq!(g.num_w, 2 + 2, "two digits, a cell before them and a blank");
}

#[test]
fn a_label_that_goes_under_its_line_at_one_step_is_under_it_at_every_step() {
    // At step 2 the note on line 2 takes the margin row of line 2. Then the
    // long note on line 1 has one margin row only: in the margin that
    // `center` leaves, it needs two, so it goes under line 1. The take
    // decides once, from the margin with the offset: the note is under
    // line 1 at step 1 too, and line 2 stays on its row.
    let doc = open(
        "center-pinned",
        &FN,
        json!({"center": true, "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 1,
             "text": "the main function of the program starts here"},
            {"id": "n2", "step": 2, "type": "note", "line": 2, "text": "one"}]}),
    );
    let width = 90;
    let (one, two) = (take(&doc, width, 1), take(&doc, width, 2));
    assert!(one.left > 0);
    assert_eq!(
        one.row_of_line[&2], two.row_of_line[&2],
        "line 2 moved at step 2"
    );
    assert!(
        one.row_of_line[&2] > one.row_of_line[&1] + 1,
        "the note is not under line 1"
    );
    // And it is the take of the narrower width, moved.
    let dx = one.left;
    for step in 1..=2 {
        let plain = take(&uncentered(&doc), width - dx, step);
        assert_moved(
            &take(&doc, width, step),
            &plain,
            dx,
            &format!("step {step}"),
        );
    }
}

#[test]
fn a_note_goes_under_its_line_when_the_offset_leaves_too_little_margin() {
    // At 40 columns the block (2 + 14 + 2) goes 11 columns in. The margin
    // on its right is then 11 columns: too narrow for a label, so the note
    // goes under its line, as in the take of 29 columns without `center`.
    // Without the offset, the margin would be 22 columns wide.
    let doc = open(
        "center-narrow-margin",
        &FN,
        json!({"center": true, "numbers": false, "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 2, "text": "x is one"}]}),
    );
    let l = take(&doc, 40, 1);
    assert_eq!(l.left, 11);
    assert!(
        l.row_of_line[&3] > l.row_of_line[&2] + 1,
        "the note is not under line 2"
    );
    assert_moved(&l, &take(&uncentered(&doc), 40 - 11, 1), 11, "40 columns");
}
