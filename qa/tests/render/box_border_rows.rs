//! A border of a box goes on the row of the blank line beside the box, when
//! no annotation of the document (at any step) is on that line: that row
//! is there at every step, so a box that comes at a later step does not push
//! the code down. With no such line (code right beside the box, a line with
//! an annotation, or a second box on the line) the border has a row of its
//! own, as before.

use tiza_qa::{fixture, tiza_ok};

fn render(name: &str, step: u32) -> String {
    let p = if name == "later.rs.tiza" {
        fixture("shared/box-later.rs.tiza")
    } else {
        fixture(&format!("render/box_border_rows/{name}"))
    };
    let s = step.to_string();
    tiza_ok(&[
        "render",
        p.to_str().unwrap(),
        "--width",
        "50",
        "--step",
        &s,
        "--format",
        "txt",
        "--out",
        "-",
    ])
}

/// The row of the screen that starts with the number of `line`.
fn row_of(text: &str, line: u32) -> usize {
    let n = line.to_string();
    text.lines()
        .position(|r| r.split_whitespace().next() == Some(n.as_str()))
        .unwrap_or_else(|| panic!("no line {line}:\n{text}"))
}

fn row(text: &str, line: u32) -> &str {
    text.lines().nth(row_of(text, line)).unwrap()
}

#[test]
fn a_box_of_a_later_step_takes_the_blank_lines_beside_it_and_the_code_stays() {
    let s1 = render("later.rs.tiza", 1);
    let s2 = render("later.rs.tiza", 2);
    for line in 1..=13 {
        assert_eq!(row_of(&s1, line), row_of(&s2, line), "L{line}:\n{s1}\n{s2}");
    }
    assert!(row(&s2, 6).contains("╭─ takes owns s"), "{s2}");
    assert!(row(&s2, 10).contains("╰──"), "{s2}");
    assert!(!row(&s1, 6).contains('╭'), "{s1}");
}

#[test]
fn a_box_with_code_right_beside_it_has_rows_of_its_own() {
    let s1 = render("inside.rs.tiza", 1);
    let s2 = render("inside.rs.tiza", 2);
    assert_eq!(row_of(&s2, 3), row_of(&s1, 3) + 1, "{s2}");
    assert_eq!(row_of(&s2, 5), row_of(&s1, 5) + 2, "{s2}");
    let rows: Vec<&str> = s2.lines().collect();
    assert!(rows[row_of(&s2, 3) - 1].contains("╭─ the move"), "{s2}");
}

#[test]
fn a_blank_line_with_an_annotation_at_any_step_keeps_its_row() {
    // The note on L6 comes at step 2: at step 1 the top border already has
    // a row of its own, so the note does not move the box.
    let s1 = render("note-on-blank.rs.tiza", 1);
    assert!(!row(&s1, 6).contains('╭'), "{s1}");
    let rows: Vec<&str> = s1.lines().collect();
    assert!(rows[row_of(&s1, 6) + 1].contains("╭─ takes owns s"), "{s1}");
    // The blank line under the box has no annotation: it takes the bottom.
    assert!(row(&s1, 10).contains("╰──"), "{s1}");
}

#[test]
fn two_boxes_around_one_blank_line_use_it_once() {
    let s = render("two-boxes.rs.tiza", 1);
    assert!(row(&s, 4).contains("╰──"), "{s}");
    let rows: Vec<&str> = s.lines().collect();
    assert!(rows[row_of(&s, 4) + 1].contains("╭─ two"), "{s}");
    assert_eq!(row_of(&s, 5), row_of(&s, 4) + 2, "{s}");
}

#[test]
fn two_boxes_that_start_on_one_line_keep_their_rows() {
    let s = render("nested.rs.tiza", 1);
    assert_eq!(row(&s, 1).trim(), "1", "{s}");
    assert_eq!(row_of(&s, 2), row_of(&s, 1) + 3, "{s}");
    // One bottom on L4 (the outer box): it takes the blank line under it.
    assert!(row(&s, 5).contains("╰──"), "{s}");
}

#[test]
fn two_boxes_that_end_on_one_line_keep_their_rows() {
    let s = render("same-end.rs.tiza", 1);
    // One top on L2 (the outer box): it takes the blank line over it.
    assert!(row(&s, 1).contains("╭─ outer"), "{s}");
    assert_eq!(row(&s, 5).trim(), "5", "{s}");
    assert_eq!(row_of(&s, 5), row_of(&s, 4) + 3, "{s}");
}
