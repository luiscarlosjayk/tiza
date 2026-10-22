//! t-209 — The status bar overflowed when the terminal drew `⚠` two cells
//! wide, and gave up the `↪ file` view indicator before the middle of a
//! message.
//!
//! Acceptance criteria:
//! - AC1 Which real terminals draw U+26A0 two cells wide (card note):
//!   none of Luis's. Herdr 1 cell (⚠ with VS16: 2), macOS Terminal 488 and
//!   Zed 1 cell (⚠ with VS16: 1 too, so VS16 is no fix).
//! - AC2 The bar never writes past the last column, whatever width the
//!   terminal gives ⚠ (1 or 2 cells), with autowrap on and off. Left for
//!   later (below).
//! - AC3 Order of what gives way when a message with a tail does not fit:
//!   (1) the middle of the message; (2) the directory of a file path (only
//!   if the short name is unique among the document's files); (3) the
//!   `↪ file` view indicator, then the warning badge, before the subject;
//!   (4) the subject; the tail never.
//! - AC4 Monotonic: widening never removes an element.
//! - AC5 This file: the wide-sign case with autowrap on and off, read from
//!   the raw bytes; the `code.link` document at 125 columns keeps `↪
//!   src/b/mod.rs` and cuts the middle; a width sweep for AC4. CHANGELOG.
//!
//! Here: AC3 and AC4 (the `↪ file` order). AC2 is left for later (Luis,
//! 2026-10-21): no terminal of his draws `⚠` two cells wide, and every way
//! of reserving the cell moves the bar's right part on the ones that do
//! not; the work is kept on the local branch `local/t-209-wide-sign`.

use serde_json::json;
use tiza_qa::{Live, fixture, scratch};

// ---------- AC3, AC4: the `↪ file` view indicator ----------

/// A scratch copy of `qa/fixtures/t-173/mods`: `src/a/mod.rs`, and
/// `src/b/mod.rs` linked at step 2; at step 2 the agent updates `a1`,
/// which is not shown there.
fn mods_with_a_message(name: &str) -> Live {
    let dir = scratch(name);
    for f in ["src/a/mod.rs", "src/b/mod.rs", "doc.tiza"] {
        let to = dir.join(f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(fixture(&format!("t-173/mods/{f}")), to).unwrap();
    }
    let path = dir.join("doc.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path, dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 2}));
    live.api_ok("annotation.update", json!({"id": "a1", "label": "LIFO"}));
    assert_eq!(
        live.message().unwrap().0,
        "a1: box 3-5 updated by agent (src/a/mod.rs, not shown at step 2)"
    );
    live
}

const VIEW: &str = "↪ src/b/mod.rs";
const TAIL: &str = "(src/a/mod.rs, not shown at step 2)";

fn bar(live: &Live, cols: u16) -> String {
    live.screen(cols, 30)
        .lines()
        .last()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn ac3_where_the_message_first_needs_cutting_the_indicator_stays() {
    // Test step 3: at 125 columns (the card's width, with its badge) and
    // at the widest width where this message no longer fits whole, `↪
    // src/b/mod.rs` stays and the middle of the message is cut instead.
    let live = mods_with_a_message("t209-125");
    let b = bar(&live, 125);
    assert!(b.contains(VIEW) && b.ends_with(TAIL), "{b}");
    let (cols, b) = (40..=125u16)
        .rev()
        .map(|c| (c, bar(&live, c)))
        .find(|(_, b)| b.contains('…'))
        .expect("some width cuts the message");
    assert!(b.contains(VIEW), "{cols}: {b}");
    assert!(b.contains("a1") && b.ends_with(TAIL), "{cols}: {b}");
}

#[test]
fn ac3_ac4_sweep_the_indicator_goes_only_after_the_middle() {
    let live = mods_with_a_message("t209-sweep");
    let mut prev: Option<(u16, String)> = None;
    for cols in 30..=170u16 {
        let b = bar(&live, cols);
        let view = b.contains(VIEW);
        let tail = b.ends_with(TAIL);
        if tail
            && !view
            && let Some(at) = b.find("a1")
        {
            // (3) after (1): the indicator went only because the subject,
            // `…` and the tail would not fit beside it.
            let room = cols as usize - b[..at].chars().count();
            let least =
                "a1… ".chars().count() + TAIL.chars().count() + 1 + usize::from(b.contains('⚠'));
            let indicator = format!("  {VIEW}").chars().count();
            assert!(
                room < least + indicator,
                "{cols}: the indicator went with room to spare ({room} >= {least} + {indicator}): {b}"
            );
        }
        if let Some((pc, pb)) = &prev {
            let was = format!("{pc} → {cols}:\n{pb}\n{b}");
            // AC4: one column more never takes the indicator, the subject
            // or the tail away.
            assert!(!pb.contains(VIEW) || view, "the indicator went: {was}");
            assert!(!pb.ends_with(TAIL) || tail, "the tail went: {was}");
            assert!(
                !(pb.ends_with(TAIL) && pb.contains("a1")) || b.contains("a1"),
                "the subject went: {was}"
            );
        }
        prev = Some((cols, b));
    }
}
