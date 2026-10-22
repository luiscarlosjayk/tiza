//! t-209 — The status bar overflowed when the terminal drew `⚠` two cells
//! wide, and gave up the `↪ file` view indicator before the middle of a
//! message.
//!
//! Acceptance criteria:
//! - AC1 Which real terminals draw U+26A0 two cells wide (card note):
//!   none of Luis's. Herdr 1 cell (⚠ with VS16: 2), macOS Terminal 488 and
//!   Zed 1 cell (⚠ with VS16: 1 too, so VS16 is no fix).
//! - AC2 (Luis, 2026-10-22, option c) The bar's warning badge uses the
//!   ASCII `!` instead of `⚠`, so the bar never depends on how the terminal
//!   draws `⚠`: no cell is reserved and the terminal is not asked. Every
//!   place where Tiza writes `⚠` on its own (bar, help, messages) changes
//!   the same way; a `⚠` the user writes (step titles, labels) is theirs.
//!   Bars without the badge are byte for byte as before.
//! - AC3 Order of what gives way when a message with a tail does not fit:
//!   (1) the middle of the message; (2) the directory of a file path (only
//!   if the short name is unique among the document's files); (3) the
//!   `↪ file` view indicator, then the warning badge, before the subject;
//!   (4) the subject; the tail never.
//! - AC4 Monotonic: widening never removes an element (the middle may lose
//!   a few letters). Narrow widths keep t-173's order on purpose (t-214,
//!   won't do).
//! - AC5 This file: the badge's bytes are ASCII; the `code.link` document
//!   at 125 columns keeps `↪ src/b/mod.rs`; a width sweep for AC4.
//!   CHANGELOG.
//!
//! Where Tiza wrote `⚠` on its own before AC2: only the bar's badge
//! (`crates/tiza-tui/src/ui.rs`, `draw_status` and `fit_bar_message`: the
//! badge and its short forms) and the README's description of it. The
//! help, the messages, the presentation line and the agent guide never
//! had one.

use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use serde_json::json;
use tiza_qa::{Live, bin, fixture, scratch};

// ---------- AC3, AC4: the `↪ file` view indicator ----------

/// A scratch copy of `qa/fixtures/t-173/mods`: `src/a/mod.rs`, and
/// `src/b/mod.rs` linked at step 2; at step 2 the agent updates `a1`,
/// which is not shown there. `no_save`: `:autosave off` first, so the
/// change leaves `•  unsaved (:w)` on the bar too.
fn mods_with_a_message(name: &str, no_save: bool) -> Live {
    let dir = scratch(name);
    for f in ["src/a/mod.rs", "src/b/mod.rs", "doc.tiza"] {
        let to = dir.join(f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(fixture(&format!("t-173/mods/{f}")), to).unwrap();
    }
    let path = dir.join("doc.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path, dir.join("live.sock"));
    if no_save {
        live.command("autosave off");
    }
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
/// The badge's sign with the blanks around it, in every form of the badge
/// (`  ! labels under the lines`, `  ! labels`, `  !` and the message's
/// blank after it, `  ! code clipped (N cols)`).
const SIGN: &str = "  ! ";

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
    let live = mods_with_a_message("t209-125", false);
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
    let live = mods_with_a_message("t209-sweep", false);
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
                "a1… ".chars().count() + TAIL.chars().count() + 1 + usize::from(b.contains(SIGN));
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

// ---------- AC2, AC5: the badge is ASCII ----------

/// The last row as drawn, and the text of its badge: the cells in the
/// badge's style (the theme's error color, bold), which no other segment
/// of these bars uses. Empty when there is no badge.
fn bar_and_badge(live: &Live, cols: u16, rows: u16) -> (String, String) {
    live.with(move |app| {
        let mut t = Terminal::new(TestBackend::new(cols, rows)).unwrap();
        t.draw(|f| tiza_tui::ui::draw(f, app)).unwrap();
        let buf = t.backend().buffer();
        let e = app.theme.error;
        let mut bar = String::new();
        let mut badge = String::new();
        for x in 0..cols {
            let c = &buf[(x, rows - 1)];
            bar.push_str(c.symbol());
            if c.fg == Color::Rgb(e.0, e.1, e.2) && c.modifier.contains(Modifier::BOLD) {
                badge.push_str(c.symbol());
            }
        }
        (bar.trim_end().to_string(), badge)
    })
}

/// `qa/fixtures/t-173/wide.rs` at step 0 with autosave off and the agent's
/// box: at 80 columns the labels go under the lines and the message ends in
/// a step hint, so the badge goes through all its forms in a sweep.
fn wide_with_a_message(name: &str) -> Live {
    let code = std::fs::read_to_string(fixture("t-173/wide.rs")).unwrap();
    let live = Live::from_code(&code, "rs", name);
    live.command("step 0");
    live.command("autosave off");
    live.with(|a| a.message = None);
    live.api_ok(
        "annotation.add",
        json!({"type": "box", "from_line": 3, "to_line": 5}),
    );
    assert_eq!(
        live.message().unwrap().0,
        "a1: box 3-5 added by agent (step 1, press n)"
    );
    live
}

#[test]
fn ac2_ac5_every_form_of_the_badge_is_ascii() {
    let mut seen: Vec<String> = Vec::new();
    let mut check = |live: &Live, cols: u16| {
        let (bar, badge) = bar_and_badge(live, cols, 30);
        assert!(!bar.contains('⚠'), "{cols}: {bar}");
        if !badge.is_empty() {
            assert!(badge.is_ascii(), "{cols}: {badge:?} in {bar}");
            // At the narrowest widths the edge may cut the badge to its
            // blanks (narrow bars are another card's).
            assert!(
                badge.trim().is_empty() || badge.starts_with("  !"),
                "{cols}: {badge:?} in {bar}"
            );
            if !seen.contains(&badge) {
                seen.push(badge);
            }
        }
    };
    // `labels under the lines` and its short forms, with a message.
    let live = wide_with_a_message("t209-ascii-labels");
    for cols in 30..=130 {
        check(&live, cols);
    }
    // The same without a message, `small terminal` and `code clipped`.
    live.with(|a| a.message = None);
    check(&live, 80);
    let small = Live::from_code("fn main() {}\n", "rs", "t209-ascii-small");
    check(&small, 120);
    let long = format!("fn main() {{ let x = \"{}\"; }}\n", "x".repeat(150));
    let clipped = Live::from_code(&long, "rs", "t209-ascii-clipped");
    check(&clipped, 120);
    for form in [
        "  ! labels under the lines",
        "  ! labels",
        "  !",
        "  ! small terminal",
        "  ! code clipped (175 cols)",
    ] {
        assert!(seen.iter().any(|b| b == form), "{form:?} not in {seen:?}");
    }
}

/// The bytes the real binary writes to its terminal: the badge is there as
/// ASCII and no `⚠` (E2 9A A0) is ever written, with the message on the bar
/// and without it, at 80 columns.
#[test]
fn ac2_ac5_the_binary_writes_the_badge_in_ascii() {
    let dir = scratch("t209-raw");
    let doc = dir.join("wide.rs");
    std::fs::copy(fixture("t-173/wide.rs"), &doc).unwrap();
    let body = r##"
binary, doc = sys.argv[1], sys.argv[2]
os.chdir(os.path.dirname(doc))
sock = "r.sock"
def api(*args):
    return subprocess.run([binary, "api", *args, "--socket", sock], capture_output=True, text=True, timeout=60)
s = Pty([binary, "--socket", sock, "--no-save", doc], 30, 80)
if s.wait_for(lambda out: b"labels under the lines" in out and os.path.exists(sock), 20, "first frame"):
    print("FIRST_BADGE", b"! labels under the lines" in s.out, flush=True)
    api("step.set", '{"step": 0}')
    api("annotation.add", '{"type": "box", "from_line": 3, "to_line": 5}')
    if s.wait_for(lambda out: b"(step 1, press n)" in out, 20, "the message"):
        print("MESSAGE", True, flush=True)
    print("WIDE_SIGN", "\u26a0".encode() in s.out, flush=True)
s.send(b"\x1b:q!\r")
s.finish(10)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "raw.py",
        body,
        &[bin().to_str().unwrap(), doc.to_str().unwrap()],
    );
    assert!(out.contains("FIRST_BADGE True"), "{out}");
    assert!(out.contains("MESSAGE True"), "{out}");
    assert!(out.contains("WIDE_SIGN False"), "{out}");
}

#[test]
fn ac5_at_125_columns_the_indicator_stays_beside_the_ascii_badge() {
    // Test step 3: the two-file document at 125 columns with the agent's
    // long message, with `unsaved (:w)` on the bar. With 24 rows the bar
    // carries `! small terminal` too, so the message no longer fits: `↪
    // src/b/mod.rs` and the badge stay, and the middle of the message is
    // cut.
    let live = mods_with_a_message("t209-125-ascii", true);
    let (b, badge) = bar_and_badge(&live, 125, 24);
    assert_eq!(badge, "  ! small terminal", "{b}");
    assert!(b.contains(VIEW) && b.ends_with(TAIL), "{b}");
    assert!(b.contains("a1") && b.contains('…'), "{b}");
    assert!(!b.contains('⚠'), "{b}");
    // With 30 rows there is no badge and the message fits whole.
    let (b, badge) = bar_and_badge(&live, 125, 30);
    assert!(badge.is_empty() && b.contains(VIEW), "{b}");
    assert!(
        b.ends_with("a1: box 3-5 updated by agent (src/a/mod.rs, not shown at step 2)"),
        "{b}"
    );
}

/// AC4 over the bar with the badge: from 30 to 170 columns, one column more
/// never takes away the subject, the tail or the badge, and while the
/// message keeps its tail the badge never goes to a shorter form. The badge
/// may go only where the terminal no longer warns at all (`status` has no
/// `warnings` at that size: big enough, as t-064 says).
#[test]
fn ac4_sweep_with_the_badge_is_monotonic() {
    let live = wide_with_a_message("t209-sweep-badge");
    let rank = |badge: &str| match badge.trim() {
        "! labels under the lines" | "! small terminal" => 3,
        "! labels" | "! small" => 2,
        "!" => 1,
        _ => 0,
    };
    let mut prev: Option<(u16, String, String)> = None;
    let mut went = 0;
    for cols in 30..=170u16 {
        let (b, badge) = bar_and_badge(&live, cols, 30);
        let warnings = live.api_ok("status", json!({}))["warnings"].clone();
        let warned = warnings.as_array().is_some_and(|w| !w.is_empty());
        if let Some((pc, pb, pbadge)) = &prev {
            let was = format!("{pc} → {cols}:\n{pb}\n{b}");
            assert!(
                !pb.ends_with(STEP_HINT) || b.ends_with(STEP_HINT),
                "the tail went: {was}"
            );
            assert!(
                !(pb.ends_with(STEP_HINT) && pb.contains("a1")) || b.contains("a1"),
                "the subject went: {was}"
            );
            if !pbadge.is_empty() && badge.is_empty() {
                assert!(!warned, "the badge went with {warnings} still on: {was}");
                went += 1;
            }
            // Its form: below the tail's widths the edge cuts the bar, a
            // narrow bar that is another card's.
            if pb.ends_with(STEP_HINT) && !pbadge.is_empty() && !badge.is_empty() {
                assert!(rank(&badge) >= rank(pbadge), "the badge shrank: {was}");
            }
        }
        prev = Some((cols, b, badge));
    }
    // It goes once, at the width where the warnings end, and stays gone.
    assert_eq!(went, 1, "the badge went {went} times");
}

const STEP_HINT: &str = "(step 1, press n)";

#[test]
fn ac5_changelog_names_the_sign() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(
        changelog.contains("- The status bar's warning badge uses an ASCII `!` instead of `⚠`")
    );
}
