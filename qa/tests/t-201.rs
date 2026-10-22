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
//!   p1–p6 and their variants; `four.env` with demo values of the same
//!   length) and a width sweep, `visible`/`clipped` in the `annotation.add`
//!   reply agreeing with `check` at every width; CHANGELOG (Fixed).

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
    let w = width.to_string();
    let p = path(name);
    tiza_ok(&[
        "render", &p, "--step", "1", "--width", &w, "--format", format, "--out", "-",
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

/// The texts a fixture shows (note and callout texts, labels, titles), by
/// annotation id.
fn texts(name: &str) -> Vec<(String, String)> {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(path(name)).unwrap()).unwrap();
    let mut out = Vec::new();
    for a in doc["annotations"].as_array().unwrap() {
        for k in ["text", "label", "title"] {
            if let Some(t) = a[k].as_str()
                && !t.trim().is_empty()
            {
                out.push((a["id"].as_str().unwrap().to_string(), t.to_string()));
            }
        }
    }
    out
}

/// Every word of every text of `name` is on the render at `width`, unless
/// `check` lists that annotation: nothing is lost or overdrawn silently.
fn assert_nothing_lost(name: &str, width: usize) {
    let t = render(name, width);
    let listed: Vec<String> = check_cuts(name, width).into_iter().map(|c| c.0).collect();
    for (id, text) in texts(name) {
        if listed.contains(&id) {
            continue;
        }
        let missing: Vec<&str> = text.split_whitespace().filter(|w| !t.contains(w)).collect();
        assert!(
            missing.is_empty(),
            "{name} at {width}: {id} loses {missing:?} and check says nothing\n{t}"
        );
    }
}

/// The index of the row of code line `n` (`" 2 "` prefix) in a render.
fn row_of(text: &str, n: u32) -> usize {
    text.lines()
        .position(|r| r.starts_with(&format!("{n:>2} ")))
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
fn ac2_p2_at_100_both_callouts_are_drawn() {
    // The second does not fit beside the first nor below it: it falls back
    // to a label, beside the first block where 14 columns are left.
    let t = render("p2.tiza", 100);
    assert!(t.contains("│ rotate it every month │"), "{t}");
    assert!(t.contains("┆ never commit the real"), "{t}");
    assert!(check_cuts("p2.tiza", 100).is_empty());
}

#[test]
fn ac2_p2_and_p2b_never_lose_text_silently_from_60_to_140() {
    for name in ["p2.tiza", "p2b.tiza"] {
        for w in 60..=140 {
            assert_nothing_lost(name, w);
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
            assert_nothing_lost(name, w);
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
   │  ┆ sent as a bearer header to every servic…│
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
fn ac6_cases_outside_the_card_draw_as_before() {
    // As origin/main (b2016b5) drew them: a fallen-back callout alone on its
    // line stays in the margin (p1b), and p6 where the block does not fit
    // (90) or leaves room beside it (118).
    assert_eq!(
        render("p1b.tiza", 100),
        " 1   DB_URL=postgres://demo:demo@localhost/app
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info                             ┆ set it to debug locally to see every SQL query
 4   PORT=8080                                    and every request the server answers, with timin…
"
    );
    assert_eq!(
        render("p6.tiza", 90),
        " 1   DATABASE_URL=postgres://app:demo-passwd@db.internal:5432/app  ┆ user, password, hos…
 2   API_TOKEN=tok_demo_0000000000000000000a                       ┆ the API token
 3   LOG_LEVEL=info
 4   PORT=8080
"
    );
    assert_eq!(
        render("p6.tiza", 118),
        " 1   DATABASE_URL=postgres://app:demo-passwd@db.internal:5432/app  ╭────────────────────────────────╮
 2   API_TOKEN=tok_demo_0000000000000000000a                       │ user, password, host, port and │ ┆ the API token
 3   LOG_LEVEL=info                                                │ database in one value          │
 4   PORT=8080                                                     ╰────────────────────────────────╯
"
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
