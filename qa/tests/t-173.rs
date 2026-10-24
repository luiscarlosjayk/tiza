//! t-173 — At 80 columns the status bar loses the step hint.
//!
//! Decisions by Luis (2026-10-21), after QA round 0. What gives way when the
//! bar is short, in this order: (1) the middle of the message; (2) the
//! directory of a file path, only if the short name is not repeated among
//! the document's files; (3) the warning badge, only while the message is
//! shown; (4) the subject (id). The tail is never cut.
//!
//! Acceptance criteria:
//! - AC1 When a message with a tail does not fit, the bar keeps the subject
//!   and the whole tail, and cuts the middle: `a1: box… (step 1, press n)`.
//!   If even subject + tail do not fit, the tail alone; if the tail alone
//!   does not fit, the beginning of the message with `…` and no tail. A tail
//!   is never shown cut, and a file name that contains ` (` is never taken
//!   for the start of the tail.
//! - AC2 File tails (t-155) and the grouped notice follow the same order.
//!   The directory is dropped only when that is what lets the subject and the
//!   tail fit after cutting the middle, and only when no other file of the
//!   document (`code.file` and every `code.link`, at any step) has the same
//!   short name; two `mod.rs` keep their directories.
//! - AC3 The `⚠ labels under the lines` badge gives way only when that is
//!   needed for subject + tail to fit: first to `⚠ labels`, then to `⚠`; it
//!   is back whole as soon as the message goes. `⚠` (U+26A0, East Asian
//!   Width N, not Ambiguous) is laid out as one cell, as Herdr, macOS
//!   Terminal and Zed draw it; a terminal that draws it two cells wide can
//!   still push the bar one cell past the edge (t-209, left for later). A
//!   more serious badge (`code clipped (N cols)`) never gives way.
//!   Since t-209 AC2 the bar writes the ASCII `!` instead of `⚠` in every
//!   form (`! labels under the lines` → `! labels` → `!`); the checks
//!   below read that sign.
//! - AC4 Monotonic: widening the terminal by one column never removes an
//!   element that was shown (subject, a directory once it is back, the tail,
//!   the whole badge). The middle may lose a few letters.
//! - AC5 Nothing changes when the message fits: byte for byte as before.
//! - AC6 This test at 80 and 60 columns with the badge on, a width sweep for
//!   AC4, two `mod.rs`, a file name with ` (`, ⚠ drawn 2 cells wide; tests
//!   check elements of the bar, not whole bars; CHANGELOG.
//!
//! The subject (`ui::fit_message`): the ids before the first `: ` (`a17`,
//! `a3, a4`, `accepted p1`), the count and what it counts (`3 agent
//! annotations`), else the first word (`a3` of `a3 now appears at step 4`).
//! The card's 16 cells at 80 columns come from the left of the bar: `⚠
//! labels under the lines` plus `•  unsaved (:w)` under `--no-save` /
//! `:autosave off` (`open(…, true)` here).

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, example, fixture, scratch};

const STEP_HINT: &str = "(step 1, press n)";
/// Since t-246 the bar warns only of what the step loses: a box label too
/// long for its border gives the badge `! 1 label cut` at every width.
const WHOLE_BADGE: &str = "! 1 label cut";
const LONG_LABEL: &str =
    "a box label far too long for the border of this box at any width the sweep tries";
const STEP2_HINT: &str = "(step 2, press n)";

// ---------- reading the bar by its elements ----------

/// The forms of the labels badge (`! 1 label cut`, then its sign), from
/// the least to the whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Badge {
    /// `code needs N cols`, which never gives way, or no badge.
    Other,
    Sign,
    Whole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tail {
    /// Not shown (the beginning of the message instead, or nothing).
    None,
    /// The file named alone (`(queue.rs, …)`).
    Short,
    Whole,
}

#[derive(Debug)]
struct Elements {
    badge: Badge,
    subject: bool,
    tail: Tail,
    /// What is on the right of the badge (the message, with blanks).
    right: String,
}

/// What a message looks like on the bar: its subject, its tail as written
/// and its tail with the file named alone (for a file tail).
struct Msg<'a> {
    subject: &'a str,
    tail: &'a str,
    short: Option<&'a str>,
}

const AGENT_BOX: Msg = Msg {
    subject: "a1",
    tail: STEP_HINT,
    short: None,
};

/// The agent's box at step 2, beside the cut box of [`open_cut`].
const AGENT_BOX2: Msg = Msg {
    subject: "a2",
    tail: STEP2_HINT,
    short: None,
};

fn elements(bar: &str, msg: &Msg) -> Elements {
    let (badge, at) = if let Some(i) = bar.find(WHOLE_BADGE) {
        (Badge::Whole, i + WHOLE_BADGE.len())
    } else if let Some(i) = bar.find("! code needs") {
        (Badge::Other, i)
    } else if let Some(i) = bar.find("  ! ") {
        (Badge::Sign, i + "  !".len())
    } else {
        (Badge::Other, 0)
    };
    let right = bar[at..].to_string();
    let tail = if bar.ends_with(msg.tail) {
        Tail::Whole
    } else if msg.short.is_some_and(|s| bar.ends_with(s)) {
        Tail::Short
    } else {
        Tail::None
    };
    Elements {
        badge,
        subject: right.contains(msg.subject),
        tail,
        right,
    }
}

/// AC1: the bar never ends in a piece of the tail followed by `…` (main
/// drew `a1: box 3-5 (st…`).
fn assert_no_cut_tail(bar: &str, tail: &str) {
    let Some(before) = bar.strip_suffix('…') else {
        return;
    };
    for (k, _) in tail.char_indices().skip(1) {
        assert!(!before.ends_with(&tail[..k]), "a cut tail: {bar}");
    }
}

/// AC3: the bar fits, and its sign is the ASCII `!` (one cell in every
/// terminal; `Live::screen` gives one symbol per cell). A `⚠` the bar wrote
/// would take two cells where the terminal draws it wide.
fn assert_fits_with_a_wide_sign(bar: &str, cols: u16) {
    let cells = bar.chars().count() + bar.matches('⚠').count();
    assert!(cells <= cols as usize, "{cols}: {cells} cells: {bar}");
}

/// Every check of the card for one message on the bar, from `from` to `to`
/// columns: no cut tail, no overflow even with a wide `⚠`, and AC4 — one column
/// more never takes away the subject, the tail, the directory or a form of
/// the badge. Returns the bars, for more checks.
fn sweep(live: &Live, msg: &Msg, from: u16, to: u16) -> Vec<(u16, String)> {
    let mut bars: Vec<(u16, String)> = Vec::new();
    let mut prev: Option<Elements> = None;
    for cols in from..=to {
        let b = bar(live, cols);
        assert_no_cut_tail(&b, msg.tail);
        if let Some(s) = msg.short {
            assert_no_cut_tail(&b, s);
        }
        let e = elements(&b, msg);
        if e.tail != Tail::None {
            assert_fits_with_a_wide_sign(&b, cols);
        }
        if e.subject && e.tail != Tail::None {
            assert_badge_gave_way_only_if_needed(&b, &e, msg);
        }
        if let Some(p) = &prev
            && p.tail != Tail::None
        {
            let was = format!("{} → {cols}: {b}", cols - 1);
            assert_ne!(e.tail, Tail::None, "the tail went: {was}");
            assert!(!p.subject || e.subject, "the subject went: {was}");
            assert!(
                !(p.tail == Tail::Whole && e.tail == Tail::Short),
                "the directory went: {was}"
            );
            if p.badge > Badge::Other && e.badge > Badge::Other {
                assert!(e.badge >= p.badge, "the badge shrank: {was}");
            }
        }
        prev = Some(e);
        bars.push((cols, b));
    }
    bars
}

/// AC3: a badge that gave way (`! labels`, `!`) did it because its next
/// larger form would not leave room for the subject, `…` and the tail.
fn assert_badge_gave_way_only_if_needed(bar: &str, e: &Elements, msg: &Msg) {
    let grow = match e.badge {
        Badge::Sign => " 1 label cut".chars().count(),
        _ => return,
    };
    let tail = if e.tail == Tail::Short {
        msg.short.unwrap()
    } else {
        msg.tail
    };
    // The subject, `…`, a blank and the tail, after the blank that
    // separates the message and before its trailing blank.
    let least = msg.subject.chars().count() + 2 + tail.chars().count() + 2;
    let room = e.right.chars().count() + 1;
    assert!(
        room.saturating_sub(grow) < least,
        "the badge gave way with room to spare ({room} - {grow} >= {least}): {bar}"
    );
}

fn at(bars: &[(u16, String)], cols: u16) -> &str {
    &bars.iter().find(|(c, _)| *c == cols).unwrap().1
}

// ---------- documents ----------

/// `t-173/FILE` at step 1 with a box whose label is always cut, so the bar
/// carries `! 1 label cut` (t-246); the agent's box then goes to step 2
/// ([`agent_box2`]). `no_save` as in [`open`].
fn open_cut(file: &str, name: &str, no_save: bool) -> Live {
    let code = std::fs::read_to_string(fixture(&format!("t-173/{file}"))).unwrap();
    let live = Live::from_code(&code, "rs", name);
    live.command(&format!("box 1-2 {LONG_LABEL}"));
    if no_save {
        live.command("autosave off");
    }
    live.with(|a| a.message = None);
    live
}

/// `t-173/FILE` at step 0 with an empty bar. `no_save` adds what QA 2 had
/// on the bar (`:autosave off`, then the agent's change makes it dirty).
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

/// A scratch copy of `examples/stack-and-queue.tiza`, with `stack.rs`
/// renamed to `stack_name` (`stack (v2).rs` for AC1).
fn open_demo(name: &str, stack_name: &str, no_save: bool) -> Live {
    let dir = scratch(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::copy(example("src/queue.rs"), dir.join("src/queue.rs")).unwrap();
    std::fs::copy(example("src/stack.rs"), dir.join("src").join(stack_name)).unwrap();
    let doc = std::fs::read_to_string(example("stack-and-queue.tiza"))
        .unwrap()
        .replace("src/stack.rs", &format!("src/{stack_name}"));
    let path = dir.join("stack-and-queue.tiza");
    std::fs::write(&path, doc).unwrap();
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path, dir.join("live.sock"));
    if no_save {
        live.command("autosave off");
    }
    live
}

/// A scratch copy of `qa/fixtures/t-173/mods`: `src/a/mod.rs`, and
/// `src/b/mod.rs` linked at step 2.
fn open_mods(name: &str) -> Live {
    let dir = scratch(name);
    for f in ["src/a/mod.rs", "src/b/mod.rs", "doc.tiza"] {
        let to = dir.join(f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(fixture(&format!("t-173/mods/{f}")), to).unwrap();
    }
    let path = dir.join("doc.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    Live::open(doc, path, dir.join("live.sock"))
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

fn agent_box2(live: &Live) {
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "box", "from_line": 3, "to_line": 5, "step": 2}),
    );
    assert_eq!(r["visible"], json!(false), "{r}");
    assert_eq!(info(live), "a2: box 3-5 added by agent (step 2, press n)");
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
fn ac1_agent_box_at_80_and_60_keeps_subject_and_tail() {
    // Test step 1, as QA 2 saw it (16 cells left at 80) and without
    // `--no-save`.
    for no_save in [true, false] {
        let live = open_cut("wide.rs", "t173-ac1", no_save);
        assert!(bar(&live, 80).contains(WHOLE_BADGE));
        agent_box2(&live);
        let bars = sweep(&live, &AGENT_BOX2, 30, 130);
        let e = elements(at(&bars, 80), &AGENT_BOX2);
        assert!(e.subject && e.tail == Tail::Whole, "{no_save}: {e:?}");
        assert!(e.right.contains("a2: box"), "{no_save}: {e:?}");
        // The middle is cut, not the beginning.
        assert!(e.right.contains("… (step 2, press n)"), "{e:?}");
        if !no_save {
            // Room for subject + tail with the whole badge: it stays (AC3).
            assert_eq!(e.badge, Badge::Whole, "{e:?}");
        }
    }
    // 60 columns with the badge on: the tail whole.
    for no_save in [true, false] {
        let live = open_cut("narrow.rs", "t173-ac1-60", no_save);
        assert!(bar(&live, 60).contains(WHOLE_BADGE));
        agent_box2(&live);
        let bars = sweep(&live, &AGENT_BOX2, 30, 130);
        let e = elements(at(&bars, 60), &AGENT_BOX2);
        assert_eq!(e.tail, Tail::Whole, "{no_save}: {e:?}");
    }
}

#[test]
fn ac1_typed_box_at_80_keeps_the_whole_hint() {
    // Test step 2: `:box 3-5` typed by the presenter.
    let live = open("wide.rs", "t173-ac1-typed", true);
    live.type_text(":box 3-5");
    live.keys(&[KeyCode::Enter]);
    assert_eq!(info(&live), "a1: box 3-5 (step 1, press n)");
    let bars = sweep(&live, &AGENT_BOX, 30, 130);
    let e = elements(at(&bars, 80), &AGENT_BOX);
    assert!(e.subject && e.tail == Tail::Whole, "{e:?}");
}

#[test]
fn ac1_far_step_hint_keeps_subject_and_tail() {
    // `:set a1 step=3` (QA's 88-92 columns).
    let live = open("wide.rs", "t173-ac1-far", true);
    live.command("box 3-5");
    live.command("set a1 step=3");
    assert_eq!(info(&live), "a1: updated (step 3, :step 3 shows it)");
    let msg = Msg {
        subject: "a1",
        tail: "(step 3, :step 3 shows it)",
        short: None,
    };
    let bars = sweep(&live, &msg, 30, 130);
    for cols in [80, 88, 90, 92] {
        let e = elements(at(&bars, cols), &msg);
        assert!(e.subject && e.tail == Tail::Whole, "{cols}: {e:?}");
    }
}

#[test]
fn ac1_the_tail_wins_and_then_the_beginning() {
    let live = open("narrow.rs", "t173-ac1-narrow", true);
    agent_box(&live);
    let bars = sweep(&live, &AGENT_BOX, 20, 130);
    // At some width the tail is alone, and narrower only the beginning.
    let alone = bars.iter().find(|(_, b)| {
        let e = elements(b, &AGENT_BOX);
        e.tail == Tail::Whole && !e.subject
    });
    assert!(alone.is_some(), "never the tail alone");
    // Narrower than that, never the subject with the tail.
    let alone = alone.unwrap().0;
    for (cols, b) in bars.iter().filter(|(c, _)| *c < alone) {
        let e = elements(b, &AGENT_BOX);
        assert!(!(e.subject && e.tail == Tail::Whole), "{cols}: {b}");
    }
}

#[test]
fn ac1_a_file_name_with_a_parenthesis_is_not_the_tail() {
    let m = "a1: box 12-18 updated by agent (src/stack (v2).rs, not shown at step 2)";
    assert_eq!(
        tiza_tui::app::step_hint_start(m),
        m.find(" (src/stack"),
        "{m}"
    );
    // The t-155 tails keep their start.
    for m in [
        "a9: note L17 added by agent (src/queue.rs, not shown at step 2)",
        "a1: box 3-5 (step 1, press n)",
    ] {
        assert_eq!(tiza_tui::app::step_hint_start(m), m.rfind(" ("), "{m}");
    }

    let live = open_demo("t173-paren", "stack (v2).rs", true);
    live.api_ok("step.set", json!({"step": 2}));
    live.api_ok("annotation.update", json!({"id": "a1", "label": "LIFO"}));
    assert_eq!(info(&live), m);
    let msg = Msg {
        subject: "a1",
        tail: "(src/stack (v2).rs, not shown at step 2)",
        short: Some("(stack (v2).rs, not shown at step 2)"),
    };
    let bars = sweep(&live, &msg, 30, 130);
    for (cols, b) in &bars {
        // Never the false tail `(v2).rs, not shown…` after a cut.
        let e = elements(b, &msg);
        if b.contains("(v2).rs, not shown") {
            assert_ne!(e.tail, Tail::None, "{cols}: {b}");
        }
    }
    let e = elements(at(&bars, 80), &msg);
    assert_ne!(e.tail, Tail::None, "{e:?}");
}

// ---------- AC2 ----------

#[test]
fn ac2_file_tail_drops_the_directory_only_when_needed() {
    // Test step 3: step 2 shows src/queue.rs; a1 belongs to src/stack.rs.
    let msg = Msg {
        subject: "a1",
        tail: "(src/stack.rs, not shown at step 2)",
        short: Some("(stack.rs, not shown at step 2)"),
    };
    for no_save in [true, false] {
        let live = open_demo("t173-ac2-file", "stack.rs", no_save);
        live.api_ok("step.set", json!({"step": 2}));
        // A cut label on the step shown, so the bar has its badge.
        live.command(&format!("box 12-13 {LONG_LABEL}"));
        live.api_ok("annotation.update", json!({"id": "a1", "label": "LIFO"}));
        assert_eq!(
            info(&live),
            "a1: box 12-18 updated by agent (src/stack.rs, not shown at step 2)"
        );
        let bars = sweep(&live, &msg, 30, 130);
        let e = elements(at(&bars, 80), &msg);
        assert!(e.subject && e.tail != Tail::None, "{no_save}: {e:?}");
        // Wide: the directory is back (the middle is cut first). With the
        // badge on the bar that is from 120 columns.
        for cols in [120, 130] {
            let e = elements(at(&bars, cols), &msg);
            assert_eq!(e.tail, Tail::Whole, "{no_save} {cols}: {e:?}");
        }
        // The directory goes only where the subject would not fit with it
        // (or, once the subject has given way too, the tail alone with it).
        for (cols, b) in &bars {
            let e = elements(b, &msg);
            if e.tail == Tail::Short && !e.subject {
                let room = e.right.chars().count().saturating_sub(1);
                assert!(room < msg.tail.chars().count(), "{cols}: {b}");
            }
        }
    }
}

#[test]
fn ac2_two_mod_rs_keep_their_directories() {
    let live = open_mods("t173-ac2-mods");
    live.api_ok("step.set", json!({"step": 2}));
    live.api_ok("annotation.update", json!({"id": "a1", "label": "LIFO"}));
    assert_eq!(
        info(&live),
        "a1: box 3-5 updated by agent (src/a/mod.rs, not shown at step 2)"
    );
    let msg = Msg {
        subject: "a1",
        tail: "(src/a/mod.rs, not shown at step 2)",
        short: Some("(mod.rs, not shown at step 2)"),
    };
    let bars = sweep(&live, &msg, 30, 130);
    for (cols, b) in &bars {
        assert_ne!(elements(b, &msg).tail, Tail::Short, "{cols}: {b}");
    }
    assert_eq!(elements(at(&bars, 80), &msg).tail, Tail::Whole);
}

#[test]
fn ac2_grouped_notice_keeps_what_it_counts() {
    let msg = Msg {
        subject: "3 agent annotations",
        tail: STEP_HINT,
        short: None,
    };
    for no_save in [true, false] {
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
        let bars = sweep(&live, &msg, 30, 130);
        for cols in [80, 90] {
            let e = elements(at(&bars, cols), &msg);
            assert!(
                e.subject && e.tail == Tail::Whole,
                "{no_save} {cols}: {e:?}"
            );
        }
    }
    // 60 columns with `! labels under the lines` (wide.rs is clipped there).
    let live = open("narrow.rs", "t173-ac2-group-60", true);
    let t0 = Instant::now();
    for (i, line) in [2, 3, 4].into_iter().enumerate() {
        ipc_at(
            &live,
            "annotation.add",
            json!({"type": "note", "line": line, "text": "x"}),
            t0 + Duration::from_millis(300 * i as u64),
        );
    }
    assert!(live.with(|a| a.message.clone()).is_some());
    let bars = sweep(&live, &msg, 30, 130);
    assert_eq!(elements(at(&bars, 60), &msg).tail, Tail::Whole);
}

// ---------- AC3 ----------

#[test]
fn ac3_the_badge_gives_way_in_steps_and_comes_back() {
    let live = open_cut("wide.rs", "t173-ac3", true);
    agent_box2(&live);
    let bars = sweep(&live, &AGENT_BOX2, 30, 130);
    let forms: Vec<Badge> = bars
        .iter()
        .map(|(_, b)| elements(b, &AGENT_BOX2).badge)
        .collect();
    // Each form shows at some width, in order (the sweep checks the order).
    for f in [Badge::Sign, Badge::Whole] {
        assert!(forms.contains(&f), "{f:?} never shows");
    }
    // `sweep` checked that each smaller form shows only where the larger one
    // would leave no room for the subject and the tail.
    // The message goes: the badge is whole again.
    live.keys(&[KeyCode::Esc]);
    live.with(|a| a.message = None);
    assert!(bar(&live, 80).contains(WHOLE_BADGE), "{}", bar(&live, 80));
}

#[test]
fn ac3_code_clipped_never_gives_way() {
    let code = std::fs::read_to_string(fixture("t-173/wide.rs")).unwrap();
    let live = Live::from_code(&code, "rs", "t173-ac3-clipped");
    live.command("step 0");
    live.command("autosave off");
    agent_box(&live);
    let whole = |b: &str| b.contains("  ! code needs ") && b.contains(" cols");
    let mut seen = false;
    for cols in 55..=79 {
        let b = bar(&live, cols);
        // The bar without the message: the room the badge has.
        let said = live.with(|a| a.message.take());
        let quiet = bar(&live, cols);
        live.with(|a| a.message = said);
        if whole(&quiet) {
            seen = true;
            // Whatever the message keeps, the clipped warning keeps its words.
            assert!(whole(&b), "{cols}: {b}");
            assert!(
                !b.contains("  ! a1") && !b.contains("  ! (step"),
                "{cols}: {b}"
            );
        }
        // Where the bar cannot hold it whole, the badge keeps its sign: it
        // is never cut in a word.
        assert!(!b.contains("! code") || whole(&b), "{cols}: {b}");
        assert_no_cut_tail(&b, STEP_HINT);
    }
    assert!(seen, "no `code needs N cols` badge in 55-79");
}

// ---------- AC5 ----------

#[test]
fn ac5_a_message_that_fits_is_unchanged() {
    let live = open("wide.rs", "t173-ac5", false);
    agent_box(&live);
    // The bars main drew (b56fd7e's AC3 test, checked against main's ui.rs):
    // the whole message at the end, no badge, nothing cut.
    for cols in [100, 120] {
        let b = bar(&live, cols);
        assert!(
            b.ends_with("  a1: box 3-5 added by agent (step 1, press n)"),
            "{b}"
        );
        assert!(!b.contains("  ! ") && !b.contains('…'), "{b}");
    }
}

// ---------- AC6 ----------

#[test]
fn ac6_changelog() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    // (The CHANGELOG of 0.1.0 was condensed in t-217.)
    assert!(
        changelog.contains("A message about an annotation on a later step tells how to see it")
    );
}

// ---------- Corner cases ----------

#[test]
fn corner_pointer_row_at_the_same_time() {
    let live = open("wide.rs", "t173-pointer", true);
    live.keys(&[KeyCode::Char('f')]);
    assert!(live.with(|a| a.pointer.is_some()));
    agent_box(&live);
    let bars = sweep(&live, &AGENT_BOX, 30, 130);
    let e = elements(at(&bars, 80), &AGENT_BOX);
    assert!(e.subject && e.tail == Tail::Whole, "{e:?}");
    assert!(at(&bars, 80).contains("POINTER"));
    assert!(live.with(|a| a.pointer.is_some()));
    // 60 columns with the badge on.
    let live = open("narrow.rs", "t173-pointer-60", true);
    live.keys(&[KeyCode::Char('f')]);
    agent_box(&live);
    let bars = sweep(&live, &AGENT_BOX, 30, 130);
    assert!(at(&bars, 60).contains("POINTER"));
    assert_eq!(elements(at(&bars, 60), &AGENT_BOX).tail, Tail::Whole);
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
    // `Live::screen` reads a wide character's second cell as a blank.
    let msg = Msg {
        subject: "注 釈 1",
        tail: STEP_HINT,
        short: None,
    };
    let bars = sweep(&live, &msg, 30, 130);
    assert!(
        at(&bars, 80).contains("コ ー ド だ け"),
        "{}",
        at(&bars, 80)
    );
    let e = elements(at(&bars, 90), &msg);
    assert!(e.subject && e.tail == Tail::Whole, "{e:?}");
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
    let mut prev_subject = false;
    for cols in 18..=40 {
        let l = last(cols);
        assert_no_cut_tail(&l, STEP_HINT);
        let e = elements(&l, &AGENT_BOX);
        if e.tail == Tail::Whole {
            assert!(!prev_subject || e.subject, "{cols}: {l}");
            prev_subject = e.subject;
        }
    }
    // As main: the subject and the tail down to 23 columns (QA's P2-1).
    assert_eq!(last(23), "a1… (step 1, press n)");
    assert_eq!(last(26), "a1: b… (step 1, press n)");
    assert_eq!(last(80), "a1: box 3-5 (step 1, press n)");
    // Narrower, the tail alone, then the beginning (main cut the tail).
    assert_eq!(last(20), STEP_HINT);
    assert_eq!(last(18), "a1: box 3-5…");
}
