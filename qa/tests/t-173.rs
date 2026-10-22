//! t-173 — At 80 columns the status bar loses the step hint.
//!
//! Acceptance criteria:
//! - AC1 When the message does not fit, the bar keeps the id and the tail
//!   first and cuts the middle: `a1 … (step 1, press n)`. If not even that
//!   fits, the tail wins over the id (`(step 1, press n)`). Never a cut tail.
//! - AC2 The same for t-155's file tail (`(src/queue.rs, not shown at step
//!   2)`) and for the grouped notice (`3 agent annotations not shown (step 1,
//!   press n)`). In those cases the file name is shortened first
//!   (`(queue.rs, not shown at step 2)`).
//! - AC3 Nothing changes when the message fits (100 columns or more).
//! - AC4 This test at 80 and 60 columns with the badge on; CHANGELOG.
//!
//! Corner cases: 60 columns and less; the pointer (t-030) at the same time;
//! wide characters (CJK, emoji) in a title or id near the cut; presentation
//! mode has no bar, but its message line uses the same `fit_message`: it
//! must not get worse.
//!
//! The «id» a cut keeps is the message's subject (`ui::fit_message`): the
//! ids before the first `: ` (`a17`, `a3, a4`, `accepted p1`), the count
//! and what it counts (`3 agent annotations`), else the first word (`a3` of
//! `a3 now appears at step 4`). The card's 16 cells at 80 columns come from
//! the left of the bar (`⚠ labels under the lines`, plus `•  unsaved (:w)`
//! under `--no-save`/`:autosave off`): when the tail alone does not fit in
//! what is left, the badge keeps only its sign `⚠` (the full warning stays
//! in `status.warnings`).

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, example, fixture, scratch};

const STEP_HINT: &str = "(step 1, press n)";

/// `t-173/FILE` at step 0 with an empty bar. `no_save` adds what QA 2 had
/// on the bar (`:autosave off`, then the agent's change makes it dirty):
/// at 80 columns the message had 16 cells.
fn open(file: &str, name: &str, no_save: bool) -> Live {
    let code = std::fs::read_to_string(fixture(&format!("t-173/{file}"))).unwrap();
    let live = Live::from_code(&code, "rs", name);
    live.command("step 0");
    if no_save {
        live.command("autosave off");
    }
    live.with(|a| a.message = None);
    live
}

fn bar(live: &Live, cols: u16) -> String {
    let screen = live.screen(cols, 30);
    screen.lines().last().unwrap_or_default().to_string()
}

fn info(live: &Live) -> String {
    match live.message() {
        Some((m, false)) => m,
        other => panic!("expected an info message, got {other:?}"),
    }
}

/// The badge is on: the full warning when there is room, its sign `⚠`
/// when the message's tail needed the room.
fn assert_badge(bar: &str) {
    assert!(bar.contains('⚠'), "no badge: {bar}");
}

/// AC1: a tail is shown whole or not at all, never cut.
fn assert_no_cut_tail(bar: &str, tail: &str) {
    if bar.ends_with(tail) {
        return;
    }
    let open = tail.split(',').next().unwrap();
    assert!(!bar.contains(open), "a cut tail: {bar}");
}

/// The agent's `annotation.add` / `.update` at the injected clock `at`, so
/// the 1 s grouping of t-155 is tested without sleeping.
fn ipc_at(live: &Live, method: &str, params: Value, at: Instant) -> Value {
    let method = method.to_string();
    live.with(move |a| {
        let (tx, rx) = mpsc::channel();
        let req = tiza_core::protocol::Request {
            id: json!(1),
            method,
            params,
        };
        a.handle_ipc_at(req, tx, at);
        let resp = rx.recv().unwrap();
        resp.result.unwrap_or_else(|| panic!("{:?}", resp.error))
    })
}

fn agent_box(live: &Live) {
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "box", "from_line": 3, "to_line": 5}),
    );
    assert_eq!(r["visible"], json!(false), "{r}");
    assert_eq!(info(live), "a1: box 3-5 added by agent (step 1, press n)");
}

// ---------- AC1 ----------

#[test]
fn ac1_agent_box_at_80_keeps_the_whole_hint() {
    // Test step 1: the badge on, the agent's box at step 0.
    let live = open("wide.rs", "t173-ac1-80", false);
    assert!(bar(&live, 80).contains("⚠ labels under the lines"));
    agent_box(&live);
    let b = bar(&live, 80);
    assert!(
        b.ends_with("⚠ labels under the lines a1: box 3-5 a… (step 1, press n)"),
        "{b}"
    );

    // The card's bar: 16 cells left for the message. The badge gives way to
    // its sign and the message keeps its id and its whole tail.
    let live = open("wide.rs", "t173-ac1-80-nosave", true);
    agent_box(&live);
    let b = bar(&live, 80);
    assert!(b.contains("unsaved (:w)"), "{b}");
    assert_badge(&b);
    assert!(
        b.ends_with("⚠ a1: box 3-5 added by… (step 1, press n)"),
        "{b}"
    );
}

#[test]
fn ac1_typed_box_at_80_keeps_the_whole_hint() {
    // Test step 2: the same with `:box 3-5` typed by the presenter.
    let live = open("wide.rs", "t173-ac1-typed", true);
    live.type_text(":box 3-5");
    live.keys(&[KeyCode::Enter]);
    assert_eq!(info(&live), "a1: box 3-5 (step 1, press n)");
    let b = bar(&live, 80);
    assert_badge(&b);
    // Room for all of it once the badge keeps only its sign.
    assert!(b.ends_with("a1: box 3-5 (step 1, press n)"), "{b}");
}

#[test]
fn ac1_at_60_the_badge_on_the_hint_stays_whole() {
    let live = open("narrow.rs", "t173-ac1-60", false);
    assert!(bar(&live, 60).contains("⚠ labels under the lines"));
    agent_box(&live);
    let b = bar(&live, 60);
    assert!(b.ends_with("⚠ a1: box 3-5 adde… (step 1, press n)"), "{b}");

    // With the card's left segments too: the tail wins over the id.
    let live = open("narrow.rs", "t173-ac1-60-nosave", true);
    agent_box(&live);
    let b = bar(&live, 60);
    assert_badge(&b);
    assert!(b.ends_with(&format!("⚠   {STEP_HINT}")), "{b}");
    assert!(!b.contains("a1"), "{b}");
}

#[test]
fn ac1_narrower_than_60_never_cuts_the_hint() {
    let live = open("wide.rs", "t173-ac1-narrow", false);
    agent_box(&live);
    for cols in (30..=79).rev() {
        let b = bar(&live, cols);
        assert_no_cut_tail(&b, STEP_HINT);
    }
    // Still room for it at 50; at 40 only the beginning fits, uncut tail.
    assert!(bar(&live, 50).ends_with("a1: bo… (step 1, press n)"));
    assert!(bar(&live, 40).ends_with("⚠ a1: box 3-5 ad…"));
}

// ---------- AC2 ----------

/// A scratch copy of `examples/stack-and-queue.tiza` and its two files.
fn open_demo(name: &str) -> Live {
    let dir = scratch(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for f in ["src/stack.rs", "src/queue.rs", "stack-and-queue.tiza"] {
        std::fs::copy(example(f), dir.join(f)).unwrap();
    }
    let path = dir.join("stack-and-queue.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    Live::open(doc, path, dir.join("live.sock"))
}

#[test]
fn ac2_file_tail_at_80_and_60() {
    // Test step 3: step 2 shows src/queue.rs; a1 belongs to src/stack.rs.
    let live = open_demo("t173-ac2-file");
    live.api_ok("step.set", json!({"step": 2}));
    live.with(|a| a.message = None);
    assert!(bar(&live, 60).contains("⚠ labels under the lines"));
    live.api_ok("annotation.update", json!({"id": "a1", "label": "LIFO"}));
    assert_eq!(
        info(&live),
        "a1: box 12-18 updated by agent (src/stack.rs, not shown at step 2)"
    );
    let b = bar(&live, 80);
    assert!(
        b.ends_with("a1: box… (stack.rs, not shown at step 2)"),
        "{b}"
    );
    let b = bar(&live, 60);
    assert_badge(&b);
    assert!(b.ends_with("⚠     (stack.rs, not shown at step 2)"), "{b}");
    for cols in 30..=130 {
        assert_no_cut_tail(&bar(&live, cols), "(stack.rs, not shown at step 2)");
    }
}

#[test]
fn ac2_grouped_notice_at_80_and_60() {
    for (no_save, at_80, at_60) in [
        (
            false,
            "⚠ labels under the lines 3 agent annot… (step 1, press n)",
            "⚠ 3 agent annotati… (step 1, press n)",
        ),
        (
            true,
            "⚠  3 agent annotations… (step 1, press n)",
            "⚠   (step 1, press n)",
        ),
    ] {
        let live = open("wide.rs", "t173-ac2-group", no_save);
        let t0 = Instant::now();
        for (i, line) in [2, 3, 4].into_iter().enumerate() {
            ipc_at(
                &live,
                "annotation.add",
                json!({"type": "note", "line": line, "text": "x"}),
                t0 + Duration::from_millis(300 * i as u64),
            );
        }
        assert_eq!(
            info(&live),
            "3 agent annotations not shown (step 1, press n)"
        );
        let b = bar(&live, 80);
        assert!(b.ends_with(at_80), "{b}");
        let b = bar(&live, 60);
        assert!(b.ends_with(at_60), "{b}");
    }
}

// ---------- AC3 ----------

#[test]
fn ac3_a_message_that_fits_is_unchanged() {
    // The bars main drew before this change, byte for byte.
    let live = open("wide.rs", "t173-ac3", false);
    agent_box(&live);
    assert_eq!(
        bar(&live, 120),
        " ╭▶ NORMAL  step 0/1  120×30  dark·auto  doc.tiza  L1  1 ann.              a1: box 3-5 added by agent (step 1, press n)"
    );
    assert_eq!(
        bar(&live, 100),
        " ╭▶ NORMAL  step 0/1  100×30  doc.tiza  L1  1 ann.     a1: box 3-5 added by agent (step 1, press n)"
    );
    // The badge keeps its words when the cut message keeps its tail anyway.
    assert!(
        bar(&live, 90)
            .contains("⚠ labels under the lines a1: box 3-5 added by ag… (step 1, press n)")
    );
}

// ---------- AC4 ----------

#[test]
fn ac4_changelog() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("(t-173)"));
}

// ---------- Corner cases ----------

#[test]
fn corner_pointer_row_at_the_same_time() {
    let live = open("wide.rs", "t173-pointer", true);
    live.keys(&[KeyCode::Char('f')]);
    assert!(live.with(|a| a.pointer.is_some()));
    agent_box(&live);
    let b = bar(&live, 80);
    assert!(b.contains("POINTER"), "{b}");
    assert!(
        b.ends_with("⚠ a1: box 3-5 added b… (step 1, press n)"),
        "{b}"
    );
    let b = bar(&live, 60);
    assert!(b.contains("POINTER"), "{b}");
    assert!(b.ends_with(STEP_HINT), "{b}");
    assert!(live.with(|a| a.pointer.is_some()));
}

#[test]
fn corner_wide_characters_near_the_cut() {
    let live = open("wide.rs", "t173-wide", true);
    // A named step 0 with CJK and an emoji on the left of the bar.
    live.with(|a| {
        a.doc.steps =
            vec![serde_json::from_value(json!({"step": 0, "title": "コードだけ 🦀"})).unwrap()];
    });
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "box", "from_line": 3, "to_line": 5, "id": "注釈1"}),
    );
    assert_eq!(r["id"], json!("注釈1"), "{r}");
    assert_eq!(
        info(&live),
        "注釈1: box 3-5 added by agent (step 1, press n)"
    );
    let b = wide_text(&bar(&live, 80));
    assert!(b.contains("step 0/1 · コードだけ 🦀"), "{b}");
    assert!(b.ends_with(STEP_HINT), "{b}");
    for cols in 30..=110 {
        let b = wide_text(&bar(&live, cols));
        assert_no_cut_tail(&b, STEP_HINT);
        // With the tail, the id is kept whole or left out (with no room for
        // the tail the beginning is cut as before: `注…`).
        if b.ends_with(STEP_HINT) && b.contains('注') {
            assert!(b.contains("注釈1"), "{cols}: {b}");
        }
    }
    let b = wide_text(&bar(&live, 90));
    assert!(b.ends_with("⚠ 注釈1: box 3-5… (step 1, press n)"), "{b}");
}

/// The bar's text as written: `Live::screen` reads a wide character's
/// second cell as a blank (`コ ー ド`), which is dropped here.
fn wide_text(row: &str) -> String {
    let wide = |c: char| ('\u{2E80}'..='\u{9FFF}').contains(&c) || c >= '\u{1F000}';
    let mut out = String::new();
    let mut prev_wide = false;
    for c in row.chars() {
        if !(prev_wide && c == ' ') {
            out.push(c);
        }
        prev_wide = wide(c) && !(prev_wide && c == ' ');
    }
    out
}

#[test]
fn corner_presentation_line_is_not_worse() {
    let live = open("wide.rs", "t173-present", false);
    live.with(|a| a.set_present(true));
    live.type_text(":box 3-5 a long label that will not fit");
    live.keys(&[KeyCode::Enter]);
    let last = |cols: u16| {
        live.screen(cols, 14)
            .lines()
            .last()
            .unwrap()
            .trim()
            .to_string()
    };
    // As before (t-132).
    assert_eq!(last(80), "a1: box 3-5 (step 1, press n)");
    assert_eq!(last(26), "a1: b… (step 1, press n)");
    // Narrower, the tail wins instead of being cut (`a1: box 3-5 (st…`).
    assert_eq!(last(24), "a1 … (step 1, press n)");
    assert_eq!(last(20), STEP_HINT);
    assert_eq!(last(19), STEP_HINT);
    // No room for the tail: the beginning, without a cut tail (it was
    // `a1: box 3-5 (st…`).
    assert_eq!(last(18), "a1: box 3-5…");
}
