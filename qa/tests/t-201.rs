//! t-201 — Margin callouts and long labels: a fallen-back callout under its
//! line as a block, two callouts on one line never overlap, a callout block
//! never hides or displaces a label, a long under-line label stops at the
//! box rail, an empty label is no label.
//!
//! Acceptance criteria (Luis, 2026-10-22: 1A 2A 3A 4B 5A 6A):
//! - AC1 A callout that falls back from the margin to under its line is
//!   drawn as a block under the line, wrapped like the narrow layout already
//!   does, never as one row cut with «…». p1 at 100 columns shows the whole
//!   text.
//! - AC2 Two margin callouts on the same line never overlap: the second goes
//!   to the first free rows (or beside the first if the margin allows); if
//!   it does not fit it falls back under the line or is left out like any
//!   label, and `check` reports it. Width sweep 60–140: no silent loss, no
//!   overdrawn text.
//! - AC3 When a margin block leaves fewer than 14 columns beside it, the
//!   labels of the lines it covers go under their own line; a label is never
//!   drawn next to a line that is not its own (variant p3b). Covers point 6
//!   (callout on L1, note on L2, 98–115 columns).
//! - AC4 A label under a line inside a box is cut with «…» at the box rail
//!   and `check` counts it as clipped.
//! - AC5 An empty label `""` behaves exactly like no label (`null`) for
//!   notes, callouts, marks and boxes: no lone `·`, no dangling `warning:`,
//!   no gap in the border, and it does not count as a second label.
//! - AC6 Documents that hit none of these cases render byte-identical (txt,
//!   svg). Any visual change re-runs `scripts/readme-images.sh` and the herdr
//!   captures in the same commit.
//! - AC7 This file with the repro documents of QA 1 (`qa/fixtures/t-201/`,
//!   p1–p6 and their variants, as Luis decided on them) and a width sweep, `visible`/`clipped` in the `annotation.add`
//!   reply agreeing with `check` at every width; CHANGELOG (Fixed).
//!
//! Decided by Luis after the first QA round (2026-10-22), over AC2 as
//! written:
//! - A margin callout with no room for its block in the margin is always a
//!   block under its line, also alone on its line; never a `┆` label in the
//!   margin, cut with «…» or whole (p1b, p6 at 81–97).
//! - A margin block starts only on the rows of its own line, like a label
//!   (AC3): beside the blocks there when the margin holds it, else a block
//!   under its line (never on the rows of a later line, as on
//!   `consecutive-lines.tiza` at 130).
//! - The «…» of a label cut inside a box sits one cell before the rail, as
//!   the code does (`servi… │`).
//!
//! Decided by Luis for the second rework (2026-10-23), after t-216 found a
//! note under its line cut by a margin block with no «…» and no warning:
//! - A row under a line (a note, a label, an arrow's label) that passes a
//!   margin block ends before that block, with «…» one cell before its
//!   border, by the same rule as the box rail; `check` counts it as clipped
//!   and the reply's `visible`/`clipped` say the same
//!   (`under-beside-block.tiza`, `arrow-beside-block.tiza`). A callout
//!   block under its line never reaches a margin block (inside a box it
//!   stays inside the code column).
//!
//! Third rework (QA 1's final QA, 2026-10-23), each with a repro in
//! `qa/fixtures/t-201/`:
//! - A callout block under its line beside code of 11 columns or fewer
//!   lost the end of each row (its text wrapped at 8 cells, its block held
//!   6), with no «…» and no warning: its text now wraps at the block's
//!   width (inside a box, inside the code column; else up to 8 cells, one
//!   cell short of the margin).
//! - The continuation of a margin label took the rows of the next line when
//!   that line draws its labels under it, and wrote over them: those rows
//!   belong to that line's labels (a label of that line itself still
//!   continues on them where the margin is blank).
//! - A margin block started on the top row of the next line's box: a
//!   box's top row belongs to the line after it.

use serde_json::{Value, json};
use tiza_core::check::{CheckOptions, check_document};
use tiza_qa::{Live, fixture, scratch, tiza_ok};

fn path(name: &str) -> String {
    fixture(&format!("t-201/{name}"))
        .to_str()
        .unwrap()
        .to_string()
}

fn render_as(name: &str, width: usize, format: &str) -> String {
    render_step(name, 1, width, format)
}

fn render_step(name: &str, step: u32, width: usize, format: &str) -> String {
    let (s, w) = (step.to_string(), width.to_string());
    let p = path(name);
    tiza_ok(&[
        "render", &p, "--step", &s, "--width", &w, "--format", format, "--out", "-",
    ])
}

fn render(name: &str, width: usize) -> String {
    render_as(name, width, "txt")
}

/// `tiza check --json --width W`: each `clipped_label` finding as (id, has
/// no room).
fn check_cuts(name: &str, width: usize) -> Vec<(String, bool)> {
    let w = width.to_string();
    let p = path(name);
    let out = tiza_qa::tiza(&["check", "--json", "--width", &w, &p]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "clipped_label")
        .map(|f| {
            (
                f["id"].as_str().unwrap_or_default().to_string(),
                f["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("has no room"),
            )
        })
        .collect()
}

/// The lines an annotation belongs to: a callout's range, a note's line, a
/// span's line, an arrow's target line (where its label goes).
fn lines_of(a: &Value) -> Option<(u32, u32)> {
    let n = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
    match a["type"].as_str()? {
        "callout" => {
            let from = n(&a["from_line"])?;
            Some((from, n(&a["to_line"]).unwrap_or(from)))
        }
        "note" => n(&a["line"]).map(|l| (l, l)),
        "highlight" | "mark" => n(&a["span"]["line"]).map(|l| (l, l)),
        "arrow" => n(&a["to"]["line"]).map(|l| (l, l)),
        _ => None,
    }
}

/// The words drawn from column `col` of row `row` on: that row and the
/// rows under it at the same column (a wrapped label, a block's text), each
/// up to a border or the end of the row, until `want` words or an empty
/// piece.
fn words_from(rows: &[Vec<char>], row: usize, col: usize, want: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in &rows[row..] {
        let piece: String = r
            .iter()
            .skip(col)
            .take_while(|c| !"│┆╮╯╭╰".contains(**c))
            .collect();
        if piece.trim().is_empty() {
            break;
        }
        out.extend(piece.split_whitespace().map(str::to_string));
        if out.len() >= want {
            break;
        }
    }
    out
}

/// Every text of `name` at step 1 and `width` (note and callout texts,
/// labels, titles) is drawn whole and in its place, unless `check` lists
/// its annotation: it starts on the rows of its own line (a callout's text
/// on the row after its top border) and reads in order from there, so
/// nothing is lost, overdrawn or moved beside another line silently.
fn assert_in_place(name: &str, width: usize) {
    assert_in_place_at(name, 1, width);
}

/// [`assert_in_place`] at `step` (an annotation `check` lists at any step
/// is skipped).
fn assert_in_place_at(name: &str, step: u32, width: usize) {
    let t = render_step(name, step, width, "txt");
    let listed: Vec<String> = check_cuts(name, width).into_iter().map(|c| c.0).collect();
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path(name)).unwrap()).unwrap();
    let rows: Vec<Vec<char>> = t.lines().map(|r| r.chars().collect()).collect();
    for a in doc["annotations"].as_array().unwrap() {
        let id = a["id"].as_str().unwrap();
        if listed.iter().any(|l| l == id) || a["step"].as_u64().unwrap_or(0) > u64::from(step) {
            continue;
        }
        for k in ["text", "label", "title"] {
            let Some(text) = a[k].as_str().filter(|t| !t.trim().is_empty()) else {
                continue;
            };
            let words: Vec<&str> = text.split_whitespace().collect();
            let Some((first, last)) = lines_of(a) else {
                // A box's label is in its top border.
                assert!(t.contains(text), "{name} at {width}: {id} «{text}»\n{t}");
                continue;
            };
            let from = row_of(&t, first);
            let to = t
                .lines()
                .enumerate()
                .skip(row_of(&t, last) + 1)
                .find(|(_, r)| code_line(r).is_some())
                .map_or(rows.len(), |(i, _)| i);
            // A callout's text is on the row after its border, which may be
            // the next line's.
            let to = if k == "text" && a["type"] == "callout" {
                (to + 1).min(rows.len())
            } else {
                to
            };
            // From each place the first word starts at (a block's text
            // wraps after any word).
            let found = (from..to).any(|r| {
                let row: String = rows[r].iter().collect();
                row.match_indices(words[0]).any(|(at, _)| {
                    let col = row[..at].chars().count();
                    let starts = col == 0 || rows[r][col - 1] == ' ';
                    starts
                        && words_from(&rows, r, col, words.len())
                            .iter()
                            .take(words.len())
                            .eq(words.iter())
                })
            });
            assert!(
                found,
                "{name} at {width}: {id} «{text}» is not drawn whole from the rows of L{first} \
                 (rows {from}..{to}) and check says nothing\n{t}"
            );
        }
    }
}

/// The code line a row of a render draws (its line number first), if any.
fn code_line(row: &str) -> Option<u32> {
    row.split_whitespace().next()?.parse().ok()
}

/// The index of the row of code line `n` in a render.
fn row_of(text: &str, n: u32) -> usize {
    text.lines()
        .position(|r| code_line(r) == Some(n))
        .unwrap_or_else(|| panic!("no row for L{n}\n{text}"))
}

/// The row where `label` starts: on the row of its line `n` or on a row
/// under it, before line `n + 1`.
fn assert_label_on_its_line(text: &str, label: &str, n: u32) {
    let at = text
        .lines()
        .position(|r| r.contains(label))
        .unwrap_or_else(|| panic!("{label:?} not drawn\n{text}"));
    let own = row_of(text, n);
    let next = row_of(text, n + 1);
    assert!(
        own <= at && at < next,
        "{label:?} is on row {at}, L{n} spans rows {own}..{next}\n{text}"
    );
}

const P1_100: &str = " 1   DB_URL=postgres://demo:demo@localhost/app
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info
     ┆ how chatty the logs are
     ╭──────────────────────────────────────╮
     │ set it to debug locally to see every │
     │ SQL query and every request the      │
     │ server answers, with timings         │
     ╰──────────────────────────────────────╯
 4   PORT=8080
";

#[test]
fn ac1_p1_at_100_is_a_whole_block_under_its_line() {
    let t = render("p1.tiza", 100);
    assert_eq!(t, P1_100, "{t}");
    assert!(!t.contains('…'), "{t}");
    assert!(check_cuts("p1.tiza", 100).is_empty());
}

#[test]
fn ac1_p1_looks_the_same_from_61_columns_on() {
    // The narrow layout drew this already; every width up to the one where
    // the block fits in the margin draws the same block now.
    for w in [61, 62, 80, 100, 104, 140] {
        assert_eq!(render("p1.tiza", w), P1_100, "at {w}");
        assert!(check_cuts("p1.tiza", w).is_empty(), "at {w}");
    }
}

#[test]
fn ac1_a_fallen_back_callout_next_to_a_note_is_a_block_at_80() {
    // p3 at 80: the block does not fit in the margin and falls back; it
    // shares L2 with the note, so both go under L2, the callout as a block.
    let t = render("p3.tiza", 80);
    assert_eq!(
        t,
        " 1   DB_URL=postgres://demo:demo@localhost/app
 2   API_TOKEN=demo-token-123
     ┆ sent as a bearer header
     ╭────────────────────────────────────╮
     │ rotate it every month; the old one │
     │ stops working after an hour        │
     ╰────────────────────────────────────╯
 3   LOG_LEVEL=info
 4   PORT=8080
 5   WORKERS=4
",
        "{t}"
    );
}

/// p1b (p1 without the note) at `width`: the callout's block under L3.
const P1B_BLOCK: &str = " 1   DB_URL=postgres://demo:demo@localhost/app
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info
     ╭──────────────────────────────────────╮
     │ set it to debug locally to see every │
     │ SQL query and every request the      │
     │ server answers, with timings         │
     ╰──────────────────────────────────────╯
 4   PORT=8080
";

#[test]
fn ac1_a_callout_alone_on_its_line_with_no_room_in_the_margin_is_a_block_under_it() {
    // p1b: four lines, so the block never has the rows in the margin; it
    // was a `┆` label there, cut with … up to 100 columns, whole from 101.
    for w in [61, 62, 81, 90, 100, 101, 140, 200] {
        let t = render("p1b.tiza", w);
        assert_eq!(t, P1B_BLOCK, "at {w}\n{t}");
        assert!(check_cuts("p1b.tiza", w).is_empty(), "at {w}");
    }
    // p6 from 81 to 97: the block is wider than the margin; it was a `┆`
    // label cut with … beside L1.
    for w in 81..=97 {
        let t = render("p6.tiza", w);
        let r = row_of(&t, 1);
        let rows: Vec<&str> = t.lines().collect();
        assert!(
            !rows[r].contains('┆') && rows[r + 1].contains('╭'),
            "at {w}\n{t}"
        );
        assert!(
            rows[r + 2].contains("│ user, password, host, port"),
            "at {w}\n{t}"
        );
        assert_label_on_its_line(&t, "┆ the API", 2);
        assert!(check_cuts("p6.tiza", w).is_empty(), "at {w}\n{t}");
    }
}

#[test]
fn ac2_p2b_at_100_the_second_block_does_not_overlap_the_first() {
    let t = render("p2b.tiza", 100);
    assert_eq!(
        t,
        " 1   DB_URL=postgres://demo:demo@localhost/app
 2   API_TOKEN=demo-token-123                   ╭────────────────────────────────────╮
     ╭─────────────────╮                        │ rotate it every month: the old     │
     │ never commit it │                        │ token stops working one hour after │
     ╰─────────────────╯                        │ the new one is issued              │
 3   LOG_LEVEL=info                             ╰────────────────────────────────────╯
 4   PORT=8080
 5   WORKERS=4
 6   TIMEOUT=30
",
        "{t}"
    );
    assert!(check_cuts("p2b.tiza", 100).is_empty());
}

#[test]
fn ac2_p2_at_100_the_second_callout_is_a_block_under_its_line() {
    // The second does not fit beside the first, and the rows of L2 hold no
    // other spot: a block under L2, never a label beside the first block.
    let t = render("p2.tiza", 100);
    assert_eq!(
        t,
        " 1   DB_URL=postgres://demo:demo@localhost/app
 2   API_TOKEN=demo-token-123                   ╭───────────────────────╮
     ╭───────────────────────────╮              │ rotate it every month │
     │ never commit the real one │              ╰───────────────────────╯
     ╰───────────────────────────╯
 3   LOG_LEVEL=info
 4   PORT=8080
 5   WORKERS=4
 6   TIMEOUT=30
",
        "{t}"
    );
    assert!(check_cuts("p2.tiza", 100).is_empty());
}

#[test]
fn ac2_p2_and_p2b_never_lose_text_silently_from_60_to_140() {
    for name in ["p2.tiza", "p2b.tiza"] {
        for w in 60..=140 {
            assert_in_place(name, w);
        }
    }
}

#[test]
fn ac2_two_blocks_side_by_side_when_the_margin_holds_both() {
    // p2 at 140: the second block goes beside the first, on L2.
    let t = render("p2.tiza", 140);
    let r = t.lines().nth(row_of(&t, 2)).unwrap();
    assert_eq!(r.matches('╭').count(), 2, "{t}");
    assert!(
        t.contains("│ rotate it every month │ │ never commit the real one │"),
        "{t}"
    );
}

#[test]
fn ac2_three_callouts_on_one_line_go_side_by_side_or_under_it() {
    // At 140 two blocks fit beside each other on L2 and the third goes
    // under L2; at 180 the three are side by side.
    let t = render("three-on-one-line.tiza", 140);
    let rows: Vec<&str> = t.lines().collect();
    let r = row_of(&t, 2);
    assert_eq!(rows[r].matches('╭').count(), 2, "{t}");
    assert!(
        rows[r + 1].contains("╭") && rows[r + 2].contains("│ the staging token"),
        "{t}"
    );
    let t = render("three-on-one-line.tiza", 180);
    assert_eq!(
        t.lines().nth(row_of(&t, 2)).unwrap().matches('╭').count(),
        3,
        "{t}"
    );
}

#[test]
fn ac2_a_block_never_starts_beside_a_later_line() {
    // consecutive-lines at 130: the callout of L4 has no room beside the
    // blocks of L2 and L3; it was drawn beside L6–L8, it is now a block
    // under L4.
    let t = render("consecutive-lines.tiza", 130);
    let rows: Vec<&str> = t.lines().collect();
    let r4 = row_of(&t, 4);
    assert!(rows[r4 + 1].starts_with("      ╭"), "{t}");
    assert!(
        rows[r4 + 2].contains("│ the port the server listens on │"),
        "{t}"
    );
    for n in 5..=10 {
        assert!(!rows[row_of(&t, n)].contains('│'), "L{n}\n{t}");
    }
    // later-steps at step 3, 100 columns: the second callout of L2 under
    // L2, not beside L5–L7.
    let p = path("later-steps.tiza");
    let t = tiza_ok(&[
        "render", &p, "--step", "3", "--width", "100", "--format", "txt", "--out", "-",
    ]);
    let rows: Vec<&str> = t.lines().collect();
    let r2 = row_of(&t, 2);
    assert!(
        rows[r2 + 2].contains("╭") && rows[r2 + 3].contains("│ never commit the real one │"),
        "{t}"
    );
    assert!(!rows[row_of(&t, 5)].contains('│'), "{t}");
}

#[test]
fn ac2_three_and_consecutive_never_lose_text_silently_from_60_to_140() {
    for name in ["three-on-one-line.tiza", "consecutive-lines.tiza"] {
        for w in 60..=140 {
            assert_in_place(name, w);
        }
    }
    for step in 1..=5 {
        for w in (60..=140).step_by(4) {
            assert_in_place_at("later-steps.tiza", step, w);
        }
    }
}

const P3_100: &str = " 1   DB_URL=postgres://demo:demo@localhost/app
 2   API_TOKEN=demo-token-123                   ╭────────────────────────────────────╮
     ┆ sent as a bearer header                  │ rotate it every month; the old one │
 3   LOG_LEVEL=info                             │ stops working after an hour        │
 4   PORT=8080                                  ╰────────────────────────────────────╯
 5   WORKERS=4
";

#[test]
fn ac3_p3_at_100_the_note_goes_under_its_line() {
    let t = render("p3.tiza", 100);
    assert_eq!(t, P3_100, "{t}");
    assert!(check_cuts("p3.tiza", 100).is_empty());
}

#[test]
fn ac3_p3b_at_100_the_note_stays_with_l2_not_l6() {
    let t = render("p3b.tiza", 100);
    assert_eq!(
        t,
        format!("{P3_100} 6   TIMEOUT=30\n 7   RETRIES=3\n"),
        "{t}"
    );
    let l6 = t.lines().nth(row_of(&t, 6)).unwrap();
    assert!(!l6.contains("bearer"), "{t}");
}

#[test]
fn ac3_p6_from_98_to_115_the_note_is_drawn_on_l2() {
    for w in 98..=115 {
        let t = render("p6.tiza", w);
        assert_label_on_its_line(&t, "┆ the API token", 2);
        assert!(check_cuts("p6.tiza", w).is_empty(), "at {w}\n{t}");
    }
}

#[test]
fn ac3_every_label_starts_on_its_own_line_from_60_to_140() {
    for w in 60..=140 {
        for (name, label, line) in [
            ("p3.tiza", "┆ sent as a", 2),
            ("p3b.tiza", "┆ sent as a", 2),
            ("p6.tiza", "┆ the API", 2),
        ] {
            let t = render(name, w);
            assert_label_on_its_line(&t, label, line);
            assert_in_place(name, w);
        }
    }
}

#[test]
fn ac4_p4_the_note_is_cut_at_the_box_rail() {
    for w in [61, 100] {
        let t = render("p4.tiza", w);
        assert_eq!(
            t,
            "   ╭─ secrets ──────────────────────────────────╮
 1 │  DB_URL=postgres://demo:demo@localhost/app │
 2 │  API_TOKEN=demo-token-123                  │
   │  · demo token                              │
   │  ┆ sent as a bearer header to every servi… │
 3 │  LOG_LEVEL=info                            │
   ╰────────────────────────────────────────────╯
 4    PORT=8080
",
            "at {w}\n{t}"
        );
        assert_eq!(check_cuts("p4.tiza", w), vec![("n2".to_string(), false)]);
    }
}

#[test]
fn ac5_an_empty_highlight_label_is_no_label() {
    // p5 ("") and p5b (null) are the same picture, in text and in SVG.
    for w in [61, 100] {
        assert_eq!(render("p5.tiza", w), render("p5b.tiza", w), "at {w}");
        // The documents' titles differ, and the SVG names its document.
        let svg = |name: &str| {
            render_as(name, w, "svg")
                .lines()
                .filter(|l| !l.starts_with("<title>"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(svg("p5.tiza"), svg("p5b.tiza"), "at {w}");
    }
    // Not a second label: the note of L1 stays in the margin at 100.
    let t = render("p5.tiza", 100);
    assert_eq!(
        t.lines().next().unwrap().trim_end(),
        " 1   DB_URL=postgres://demo:demo@localhost/app  ┆ DB connection string"
    );
    assert!(!t.lines().any(|r| r.trim() == "·"), "{t}");
}

#[test]
fn ac5_labels_of_only_spaces_are_no_label_for_every_kind() {
    // blank-spaces has `"  "` as the label of a highlight, a mark, an arrow
    // and a box and as the title of two callouts; blank-null has none.
    for w in [61, 80, 100, 140] {
        for format in ["txt", "svg"] {
            let a = render_as("blank-spaces.tiza", w, format);
            let b = render_as("blank-null.tiza", w, format);
            assert_eq!(a, b, "{format} at {w}\n{a}");
        }
        assert_eq!(
            check_cuts("blank-spaces.tiza", w),
            check_cuts("blank-null.tiza", w)
        );
    }
    let t = render("blank-spaces.tiza", 100);
    assert!(!t.lines().any(|r| r.trim().ends_with('·')), "{t}");
    assert!(!t.contains("warning:"), "{t}");
}

#[test]
fn ac5_an_empty_mark_label_and_box_label_are_no_label() {
    let t = render("p5c.tiza", 100);
    assert!(!t.contains("warning:") && t.contains("! warning"), "{t}");
    assert!(
        t.contains("╭────────────────────────────────────────────╮"),
        "no gap in the box border\n{t}"
    );
    // The same document with `null` labels draws the same text and SVG.
    let mut doc: Value =
        serde_json::from_str(&std::fs::read_to_string(path("p5c.tiza")).unwrap()).unwrap();
    for a in doc["annotations"].as_array_mut().unwrap() {
        a["label"] = Value::Null;
    }
    let dir = scratch("t201-p5c-null");
    let null = dir.join("p5c-null.tiza");
    std::fs::write(&null, doc.to_string()).unwrap();
    let n = null.to_str().unwrap();
    for format in ["txt", "svg"] {
        let a = render_as("p5c.tiza", 100, format);
        let b = tiza_ok(&[
            "render", n, "--step", "1", "--width", "100", "--format", format, "--out", "-",
        ]);
        assert_eq!(a, b, "{format}");
    }
}

#[test]
fn ac5_an_empty_callout_title_is_no_title() {
    let dir = scratch("t201-title");
    let doc = |title: Value| {
        json!({"schema": 1, "source": {"language": "sh", "first_line": 1},
            "code": ["A=1", "B=2", "C=3", "D=4", "E=5"],
            "annotations": [{"id": "c1", "step": 1, "type": "callout", "from_line": 1,
                "text": "a short callout", "title": title}]})
    };
    let mut out = Vec::new();
    for (name, title) in [("empty", json!("")), ("null", Value::Null)] {
        let p = dir.join(format!("{name}.tiza"));
        std::fs::write(&p, doc(title).to_string()).unwrap();
        let p = p.to_str().unwrap().to_string();
        out.push(tiza_ok(&[
            "render", &p, "--step", "1", "--width", "100", "--format", "txt", "--out", "-",
        ]));
    }
    assert_eq!(out[0], out[1], "{}", out[0]);
}

#[test]
fn ac6_p6_with_room_beside_the_block_draws_as_before() {
    // As origin/main drew it: the block fits in the margin at 118 and the
    // note of L2 is beside it. p6's first two code lines are the fixture's
    // (`four.env`), not repeated here.
    let env = std::fs::read_to_string(fixture("t-201/four.env")).unwrap();
    let code: Vec<&str> = env.lines().collect();
    let fill = |t: &str| t.replace("{L1}", code[0]).replace("{L2}", code[1]);
    assert_eq!(
        render("p6.tiza", 118),
        fill(
            " 1   {L1}  ╭────────────────────────────────╮
 2   {L2}                       │ user, password, host, port and │ ┆ the API token
 3   LOG_LEVEL=info                                                │ database in one value          │
 4   PORT=8080                                                     ╰────────────────────────────────╯
"
        )
    );
}

/// An annotation of a document as `annotation.add` takes it: a span is
/// given as `at`.
fn add_params(ann: &Value) -> Value {
    let mut p = ann.clone();
    if let Some(span) = p.as_object_mut().unwrap().remove("span") {
        p["at"] = json!({"line": span["line"], "col": span["start"], "end": span["end"]});
    }
    p
}

/// Each annotation of `name`, taken out and added back with
/// `annotation.add` on a live instance at every width from 60 to 140: the
/// reply's `visible` and `clipped` say what `check` says of it at that
/// width.
fn reply_agrees_with_check(name: &str) {
    let full = tiza_core::fs::open(&path(name)).unwrap().document;
    let raw: Value = serde_json::from_str(&std::fs::read_to_string(path(name)).unwrap()).unwrap();
    let base_dir = fixture("t-201");
    for (k, ann) in raw["annotations"].as_array().unwrap().iter().enumerate() {
        let id = ann["id"].as_str().unwrap().to_string();
        let mut doc = full.clone();
        doc.annotations.retain(|a| a.id != id);
        doc.step = 1;
        let dir = scratch(&format!("t201-{name}-{k}"));
        let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
        for w in 60..=140usize {
            let cols = u16::try_from(w + 2).unwrap();
            let shown = live.with(move |app| {
                app.size = (cols, 30);
                app.screen_width()
            });
            assert_eq!(shown, w);
            let r = live.api_ok("annotation.add", add_params(ann));
            let now = live.with(|app| app.doc.clone());
            let report = check_document(
                &now,
                Some(base_dir.as_path()),
                &CheckOptions { width: Some(w) },
            );
            let mine: Vec<bool> = report
                .findings
                .iter()
                .filter(|f| f.code == "clipped_label" && f.id.as_deref() == Some(id.as_str()))
                .map(|f| f.message.contains("has no room"))
                .collect();
            let note_or_callout = matches!(ann["type"].as_str(), Some("note" | "callout"));
            let dropped = mine.iter().any(|d| *d);
            let at = format!("{name} {id} at {w}: reply {r}, check {mine:?}");
            if dropped && note_or_callout {
                assert_eq!(r["visible"], false, "{at}");
                assert!(r["hidden"].as_str().unwrap().contains("no room"), "{at}");
            } else {
                assert_eq!(r["visible"], true, "{at}");
                assert_eq!(r.get("clipped").is_some(), !mine.is_empty(), "{at}");
            }
            live.api_ok("annotation.delete", json!({"id": id}));
        }
    }
}

#[test]
fn ac7_reply_agrees_with_check_p1_p1b_p2() {
    for name in ["p1.tiza", "p1b.tiza", "p2.tiza"] {
        reply_agrees_with_check(name);
    }
}

#[test]
fn ac7_reply_agrees_with_check_p2b_p3_p3b() {
    for name in ["p2b.tiza", "p3.tiza", "p3b.tiza"] {
        reply_agrees_with_check(name);
    }
}

#[test]
fn ac7_reply_agrees_with_check_p4_p5_p5b() {
    for name in ["p4.tiza", "p5.tiza", "p5b.tiza"] {
        reply_agrees_with_check(name);
    }
}

#[test]
fn ac7_reply_agrees_with_check_p5c_p6() {
    for name in ["p5c.tiza", "p6.tiza"] {
        reply_agrees_with_check(name);
    }
}

#[test]
fn ac7_reply_agrees_with_check_three_and_consecutive() {
    for name in ["three-on-one-line.tiza", "consecutive-lines.tiza"] {
        reply_agrees_with_check(name);
    }
}

// ---------- Second rework: rows under a line beside a margin block ----------

/// No text runs into a callout block: in every row the block covers, the
/// cell left of its border is blank (a «…» stops one cell before it).
fn assert_clear_of_blocks(at: &str, text: &str) {
    let rows: Vec<Vec<char>> = text.lines().map(|r| r.chars().collect()).collect();
    for (top, row) in rows.iter().enumerate() {
        for x in 1..row.len() {
            if row[x] != '╭' || row.get(x + 1) != Some(&'─') {
                continue;
            }
            let Some(bottom) = (top + 1..rows.len()).find(|&r| rows[r].get(x) == Some(&'╰'))
            else {
                continue;
            };
            for (y, r) in rows.iter().enumerate().take(bottom + 1).skip(top) {
                assert_eq!(
                    r.get(x - 1),
                    Some(&' '),
                    "{at}: row {y} touches the block at column {x}\n{text}"
                );
            }
        }
    }
}

#[test]
fn r2_a_note_under_its_line_stops_one_cell_before_the_margin_block() {
    // The note of L2 goes under its line (the block leaves fewer than 14
    // columns beside it) and the block covers that row: before, the row ran
    // under the block, cut with no «…» and no warning.
    let env = std::fs::read_to_string(fixture("t-201/four.env")).unwrap();
    let code: Vec<&str> = env.lines().collect();
    let fill = |t: &str| t.replace("{L1}", code[0]).replace("{L2}", code[1]);
    assert_eq!(
        render("under-beside-block.tiza", 99),
        fill(
            " 1   {L1}  ╭────────────────────────────╮
 2   {L2}                       │ user, password, host, port │
     ┆ the live token: rotate it right after the demo, never comm… │ and database in one value  │
 3   LOG_LEVEL=info                                                ╰────────────────────────────╯
 4   PORT=8080
"
        )
    );
    assert_eq!(
        check_cuts("under-beside-block.tiza", 99),
        vec![("n2".to_string(), false)]
    );
}

#[test]
fn r2_arrow_labels_under_their_line_stop_before_the_margin_block() {
    // The label of an arrow that ends on L2 and the label of an arrow along
    // L2, both under L2 beside the block of L1.
    let env = std::fs::read_to_string(fixture("t-201/four.env")).unwrap();
    let code: Vec<&str> = env.lines().collect();
    let fill = |t: &str| t.replace("{L1}", code[0]).replace("{L2}", code[1]);
    assert_eq!(
        render("arrow-beside-block.tiza", 99),
        fill(
            " 1     {L1}  ╭──────────────────────────╮
 2 ╭─▶ {L2}                       │ user, password, host,    │
   │   ╰─────────▶                                                   │ port and database in one │
   │   · the name and the value of the token are split at the equal… │ value                    │
   │   ─▶ the port and the token go to the same service, so keep th… ╰──────────────────────────╯
 3 │   LOG_LEVEL=info
 4 ╰── PORT=8080
"
        )
    );
    let mut cuts = check_cuts("arrow-beside-block.tiza", 99);
    cuts.sort();
    assert_eq!(
        cuts,
        vec![("a2".to_string(), false), ("a3".to_string(), false)]
    );
}

#[test]
fn r2_rows_under_a_line_never_run_into_a_margin_block_from_60_to_140() {
    // (p3, p3b and p6 are swept for their labels' place above.)
    for name in [
        "under-beside-block.tiza",
        "arrow-beside-block.tiza",
        "p6.tiza",
    ] {
        for w in 60..=140 {
            assert_clear_of_blocks(&format!("{name} at {w}"), &render(name, w));
            assert_in_place(name, w);
        }
    }
}

#[test]
fn r2_reply_agrees_with_check_under_and_arrow_beside_block() {
    for name in ["under-beside-block.tiza", "arrow-beside-block.tiza"] {
        reply_agrees_with_check(name);
    }
}

// ---------- Third rework ----------

/// The small letters drawn on the rows after line `n`, in order.
fn letters_after(text: &str, n: u32) -> String {
    text.lines()
        .skip(row_of(text, n) + 1)
        .flat_map(|r| r.chars())
        .filter(char::is_ascii_lowercase)
        .collect()
}

#[test]
fn r3_a_block_under_its_line_beside_narrow_code_loses_no_text() {
    // Code of 3 columns: the code column is 10 cells. The text wrapped at
    // 8 and the block held 6, at every width.
    for w in [20, 30, 40, 61, 80, 100, 140, 200] {
        let t = render("narrow-block.tiza", w);
        assert!(check_cuts("narrow-block.tiza", w).is_empty(), "at {w}");
        assert_eq!(
            letters_after(&t, 2),
            "abcdefghijklmnopqrstuvwxyztail",
            "at {w}\n{t}"
        );
    }
}

/// The column right after the border of the block under line `n`, on each
/// of its rows: nothing is drawn there.
fn assert_gap_after_block(at: &str, t: &str, n: u32) {
    let rows: Vec<Vec<char>> = t.lines().map(|r| r.chars().collect()).collect();
    let top = row_of(t, n) + 1;
    let x1 = rows[top]
        .iter()
        .position(|c| *c == '╮')
        .unwrap_or_else(|| panic!("{at}: no block under L{n}\n{t}"));
    for (r, row) in rows.iter().enumerate().skip(top) {
        assert!(
            row.get(x1 + 1).is_none_or(|c| *c == ' '),
            "{at}: row {r} touches the block\n{t}"
        );
        if row.get(x1) == Some(&'╯') {
            return;
        }
    }
    panic!("{at}: no bottom\n{t}");
}

#[test]
fn r3_a_block_under_its_line_keeps_a_cell_from_the_margin() {
    // At 12 cells the block's border touched a margin block or a label's
    // continuation; it is one cell narrower beside code of 10 columns.
    for w in 30..=60 {
        for name in ["block-gap.tiza", "block-gap-label.tiza"] {
            let t = render(name, w);
            assert_gap_after_block(&format!("{name} at {w}"), &t, 2);
            assert!(
                check_cuts(name, w).iter().all(|c| c.0 != "c1"),
                "{name} at {w}"
            );
        }
    }
}

#[test]
fn r3_a_label_continues_on_the_rows_of_its_own_line() {
    // L1 draws its notes under itself; the label of the arrow along L1
    // continues on those rows where the notes leave the margin blank.
    for w in (40..=140).step_by(5) {
        assert_in_place("own-under.tiza", w);
    }
    assert!(check_cuts("own-under.tiza", 60).is_empty());
    let t = render("own-under.tiza", 60);
    assert!(t.contains("rows of the margin"), "{t}");
}

#[test]
fn r3_inside_a_box_the_block_wraps_its_text_at_the_box_rail() {
    // The block must not cross the box's right rail: its text wraps at the
    // code column's width, splitting long words, and loses nothing.
    for w in [20, 30, 40, 61, 80, 100, 140, 200] {
        let t = render("narrow-block-boxed.tiza", w);
        assert!(
            check_cuts("narrow-block-boxed.tiza", w).is_empty(),
            "at {w}"
        );
        assert_eq!(
            letters_after(&t, 2),
            "abcdefghijklmnopqrstuvwxyztail",
            "at {w}\n{t}"
        );
        // Every row of the box, from its top border to its bottom border,
        // ends on the rail's column.
        let rows: Vec<Vec<char>> = t.lines().map(|r| r.chars().collect()).collect();
        let top = rows
            .iter()
            .position(|r| r.iter().collect::<String>().contains("╭─ env"))
            .unwrap();
        let rail = rows[top].len() - 1;
        assert_eq!(rows[top][rail], '╮', "at {w}\n{t}");
        let bottom = (top + 1..rows.len())
            .find(|&r| rows[r].get(rail) == Some(&'╯'))
            .unwrap_or_else(|| panic!("at {w}: no bottom on the rail\n{t}"));
        for (r, row) in rows.iter().enumerate().take(bottom).skip(top + 1) {
            assert_eq!(row.get(rail), Some(&'│'), "at {w}, row {r}\n{t}");
            assert!(
                row.len() == rail + 1,
                "at {w}, row {r}: something past the rail\n{t}"
            );
        }
    }
}

#[test]
fn r3_a_note_continues_only_on_rows_no_other_line_owns() {
    // L2 draws its two notes under itself: the rows of L2 belong to them,
    // and the note of L1 never writes over them nor sits beside L2.
    for name in ["under-rows.tiza", "under-rows-long.tiza"] {
        for w in (40..=140).step_by(5) {
            let t = render(name, w);
            assert_in_place(name, w);
            let rows: Vec<&str> = t.lines().collect();
            let l2 = row_of(&t, 2);
            if rows[l2 + 1].trim_start().starts_with("┆ second") {
                assert!(
                    rows[l2].trim_end().ends_with("B=2"),
                    "{name} at {w}: text beside L2\n{t}"
                );
            }
        }
    }
    let t = render("under-rows.tiza", 70);
    assert!(
        t.contains("┆ second line first note, fits the row whole"),
        "{t}"
    );
    assert_eq!(
        check_cuts("under-rows.tiza", 70),
        vec![("n1".to_string(), false)],
        "{t}"
    );
}

#[test]
fn r3_a_block_never_starts_on_the_top_row_of_the_next_lines_box() {
    for w in (40..=140).step_by(2) {
        let t = render("block-before-box.tiza", w);
        let rows: Vec<&str> = t.lines().collect();
        let box_top = rows.iter().position(|r| r.contains("╭─ box")).unwrap();
        let Some(text) = rows.iter().position(|r| r.contains("hhhh")) else {
            assert!(
                check_cuts("block-before-box.tiza", w)
                    .iter()
                    .any(|c| c.0 == "c1"),
                "at {w}: c1 not drawn, check says nothing\n{t}"
            );
            continue;
        };
        assert!(
            text - 1 < box_top,
            "at {w}: the block of L3 starts on row {}, the box's top row is {box_top}\n{t}",
            text - 1
        );
        assert_in_place("block-before-box.tiza", w);
    }
}

#[test]
fn r3_reply_agrees_with_check() {
    for name in [
        "narrow-block.tiza",
        "narrow-block-boxed.tiza",
        "under-rows.tiza",
        "under-rows-long.tiza",
        "block-before-box.tiza",
        "own-under.tiza",
        "block-gap.tiza",
    ] {
        reply_agrees_with_check(name);
    }
}
