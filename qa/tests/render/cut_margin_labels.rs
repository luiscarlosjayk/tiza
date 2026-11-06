//! A margin label that does not fit whole in the rows it gets there goes
//! under its line of code, whole, instead of being cut with `…`:
//! - the next line has a label of its own, or the rows end (the last line);
//! - a margin block cuts a label under its line: its words go on in the
//!   rows under it.
//!
//! Only under a line of code (a source file, or the inside of a ``` block
//! of Markdown): rows under a line of a drawing or of prose would break it,
//! so there the label stays cut and `check` warns. A label that is too long
//! for the row under its line too stays in the margin: no row is added for
//! a label that stays cut. A take decides once: a label that goes under its
//! line at one step is under it at every step it shows at.

use tiza_core::layout::{LayoutOptions, layout};
use tiza_qa::{Live, fixture, scratch, tiza_ok};

fn path(name: &str) -> String {
    fixture(&format!("render/cut_margin_labels/{name}"))
        .to_str()
        .unwrap()
        .to_string()
}

fn render(name: &str, width: usize, step: u32) -> String {
    let (w, s) = (width.to_string(), step.to_string());
    let p = path(name);
    tiza_ok(&[
        "render", &p, "--width", &w, "--step", &s, "--format", "txt", "--out", "-",
    ])
}

/// The ids `check --json` names as clipped at `width` columns of code (a
/// terminal of 2 more: its padding of 1 at each side).
fn cuts(name: &str, width: usize) -> Vec<String> {
    let w = (width + 2).to_string();
    let p = path(name);
    let out = tiza_ok(&["check", "--json", "--width", &w, &p]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "clipped_label")
        .map(|f| f["id"].as_str().unwrap().to_string())
        .collect()
}

const LONG: &str = "the token every call to the payments service carries";

/// The row after the one that holds `code`.
fn row_after<'a>(text: &'a str, code: &str) -> &'a str {
    let rows: Vec<&str> = text.lines().collect();
    let at = rows
        .iter()
        .position(|r| r.contains(code))
        .unwrap_or_else(|| panic!("no {code}:\n{text}"));
    rows[at + 1]
}

#[test]
fn a_label_of_code_goes_whole_under_its_line_when_the_next_line_gets_a_label() {
    // At step 2 L3 gets a note: the note of L2 has one margin row there.
    let s2 = render("next-line.tiza", 72, 2);
    assert_eq!(
        row_after(&s2, "API_TOKEN").trim(),
        format!("┆ {LONG}"),
        "{s2}"
    );
    assert!(
        s2.contains("LOG_LEVEL=info                             ┆ chatty"),
        "{s2}"
    );
    assert!(!s2.contains('…'), "{s2}");
    assert!(cuts("next-line.tiza", 72).is_empty());
}

#[test]
fn a_take_puts_the_label_under_its_line_from_the_step_it_shows_at() {
    // Step 1 alone has room for the note in the margin; it goes under its
    // line at step 2, so it is under it at step 1 too: the code does not go
    // down a row between the two steps.
    let s1 = render("next-line.tiza", 72, 1);
    let s2 = render("next-line.tiza", 72, 2);
    assert_eq!(
        row_after(&s1, "API_TOKEN").trim(),
        format!("┆ {LONG}"),
        "{s1}"
    );
    let row = |t: &str| t.lines().position(|r| r.contains("LOG_LEVEL")).unwrap();
    assert_eq!(row(&s1), row(&s2));
    // The authoring screen, with no take, grows as it goes: the note is in
    // the margin at step 1.
    let doc = tiza_core::fs::open(&path("next-line.tiza"))
        .unwrap()
        .document;
    let free = layout(
        &doc,
        &tiza_core::Theme::dark(),
        &LayoutOptions {
            width: 72,
            step: 1,
            stable: false,
            ..Default::default()
        },
    )
    .canvas
    .to_plain_text();
    assert!(
        free.contains("API_TOKEN=demo-token-123                   ┆ the token every call"),
        "{free}"
    );
}

#[test]
fn a_label_of_the_last_line_goes_under_it_when_the_rows_end() {
    let s = render("last-line.tiza", 72, 1);
    assert_eq!(
        row_after(&s, "WORKERS=4").trim(),
        format!("┆ {LONG}"),
        "{s}"
    );
    assert!(cuts("last-line.tiza", 72).is_empty());
}

#[test]
fn a_label_too_long_for_the_row_under_its_line_stays_cut_in_the_margin() {
    let s = render("too-long.tiza", 72, 1);
    assert!(
        s.trim_end()
            .ends_with("WORKERS=4                                  ┆ the token every call…"),
        "{s}"
    );
    assert_eq!(s.lines().count(), 5, "no row added:\n{s}");
    assert_eq!(cuts("too-long.tiza", 72), ["n1"]);
}

#[test]
fn a_label_of_prose_stays_cut_and_check_warns() {
    // The same document as plain text: nothing goes under a line.
    let s2 = render("next-line-prose.tiza", 72, 2);
    assert!(
        s2.contains("API_TOKEN=demo-token-123                   ┆ the token every call…"),
        "{s2}"
    );
    assert_eq!(s2.lines().count(), 5, "{s2}");
    assert_eq!(cuts("next-line-prose.tiza", 72), ["n1"]);
}

#[test]
fn in_markdown_only_a_label_inside_a_code_block_goes_under_its_line() {
    let s = render("markdown.tiza", 72, 1);
    // L4 is inside the ``` block: under its line, whole.
    assert_eq!(
        row_after(&s, "API_TOKEN").trim(),
        format!("┆ {LONG}"),
        "{s}"
    );
    // L8 is a drawing after the block: its note stays cut in the margin,
    // and no row splits the drawing.
    assert!(
        s.contains("┌──────────┬───────┐      ┆ the token every call to the payments…"),
        "{s}"
    );
    assert!(
        row_after(&s, "┌──────────┬───────┐").contains("│ token    │ demo  │"),
        "{s}"
    );
    assert_eq!(cuts("markdown.tiza", 72), ["d1"]);
}

#[test]
fn a_label_under_its_line_that_a_later_block_cuts_goes_on_below_from_the_start() {
    // At step 2 a margin callout of L2 comes beside the second note under
    // L2: the note goes on in the row under it, before the block. A take
    // decides once, so at step 1 it is already on those two rows.
    let s1 = render("block-later.tiza", 99, 1);
    let s2 = render("block-later.tiza", 99, 2);
    for s in [&s1, &s2] {
        assert!(
            s.contains("┆ a long note that does not fit beside the"),
            "{s}"
        );
        assert_eq!(
            row_after(s, "a long note").trim_start(),
            "callout of the next step",
            "{s}"
        );
        assert!(!s.contains('…'), "{s}");
    }
    assert!(s2.contains("╭───────────────────────╮"), "{s2}");
    let row = |t: &str| t.lines().position(|r| r.contains("LOG_LEVEL")).unwrap();
    assert_eq!(row(&s1), row(&s2));
    assert!(cuts("block-later.tiza", 99).is_empty());
}

#[test]
fn a_label_of_prose_under_its_line_that_a_block_cuts_stays_cut() {
    let s = render("under-beside-block-prose.tiza", 99, 1);
    assert!(
        s.contains("┆ the live token: rotate it right after the demo, never comm… │"),
        "{s}"
    );
    assert_eq!(cuts("under-beside-block-prose.tiza", 99), ["n2"]);
}

#[test]
fn presentation_mode_puts_the_label_under_its_line_from_its_step() {
    // The TUI lays the take out with its geometry, computed once: the same
    // rows as the exports, at step 1 and at step 2.
    let dir = scratch("cut-margin-present");
    let doc = tiza_core::fs::open(&path("next-line.tiza"))
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.with(|a| a.set_present(true));
    for step in [1, 2] {
        live.with(move |a| a.set_step(step));
        let s = live.screen(74, 20);
        assert_eq!(
            row_after(&s, "API_TOKEN").trim(),
            format!("┆ {LONG}"),
            "step {step}:\n{s}"
        );
        assert!(!s.contains('…'), "{s}");
    }
}

#[test]
fn a_margin_label_goes_on_beside_the_rows_under_its_own_line() {
    // The arrow label of L1 has one free row; the rows under L1 hold its two
    // notes, and they leave the margin blank: the label goes on there, in
    // the margin, and adds no row.
    let s = render("own-rows.tiza", 60, 1);
    assert_eq!(
        s.lines().take(4).collect::<Vec<_>>(),
        [
            " 1   let value = compute(a, b)",
            "         ╰───────▶              this arrow label wraps over",
            "     ┆ short one                two rows",
            "     ┆ short two",
        ],
        "{s}"
    );
    assert!(cuts("own-rows.tiza", 60).is_empty());
}
