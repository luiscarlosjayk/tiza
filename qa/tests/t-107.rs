//! t-107 — Presentation mode: show the command line and its messages while typing `:`.
//!
//! Acceptance criteria:
//! - AC1 In presentation mode, pressing `:` draws the command line on the
//!   last row (over the code padding or the last line, as `draw_status` does
//!   today) with the same text and cursor; Enter or Esc hides it again.
//! - AC2 The result of the command (`Ok` message or error) shows for ~2 s
//!   after Enter and then disappears; a command that changes nothing visible
//!   (`:foo`) therefore still tells the presenter it failed.
//! - AC3 Nothing else of the status bar appears: no mode, step counter or
//!   hints (the discreet step counter stays as it is).
//! - AC4 `render`, `--out` exports and the VHS tape are not affected (they
//!   use their own paths); with the command line hidden the frame is
//!   byte-identical to today's presentation frame (snapshot test).
//!
//! Corner cases: a 1-2 row terminal (the line takes the last row and the
//! command still runs), a supervised proposal while presenting (no overlay,
//! only the message), `:` while an overlay is open (unchanged).

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use ratatui::{Terminal, backend::TestBackend};
use serde_json::json;
use tiza_qa::{Live, fixture, scratch, tiza_ok};
use tiza_tui::app::{Mode, Overlay, PRESENT_MESSAGE_TTL};

const CODE: &str = "fn main() {\n    let s = String::from(\"hello\");\n    takes(s);\n}\n\nfn takes(s: String) {\n    println!(\"{s}\");\n}\n";

fn presenting(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.command("box 2-3 moved here");
    live.command("note 6 takes ownership");
    live.command("step a2 2");
    live.with(|a| {
        a.set_present(true);
        a.message = None;
    });
    live
}

fn last_row(live: &Live, cols: u16, rows: u16) -> String {
    live.screen(cols, rows)
        .lines()
        .last()
        .unwrap_or("")
        .to_string()
}

/// The terminal cursor after one frame, when the frame shows it.
fn cursor(live: &Live, cols: u16, rows: u16) -> Option<(u16, u16)> {
    live.with(move |app| {
        let mut t = Terminal::new(TestBackend::new(cols, rows)).unwrap();
        t.draw(|f| tiza_tui::ui::draw(f, app)).unwrap();
        let b = t.backend();
        b.cursor_visible()
            .then(|| (b.cursor_position().x, b.cursor_position().y))
    })
}

/// Waits (10 s at most) until `cond` holds on the app thread.
fn wait_until(live: &Live, what: &str, cond: impl Fn(&Live) -> bool) {
    let start = Instant::now();
    while !cond(live) {
        assert!(start.elapsed() < Duration::from_secs(10), "waiting: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn ac1_colon_shows_the_command_line_on_the_last_row() {
    let live = presenting("t107-ac1");
    let hidden = live.screen(70, 14);
    live.type_text(":box 2-3 hi");
    assert_eq!(
        live.with(|a| a.mode.clone()),
        Mode::Command("box 2-3 hi".into())
    );
    let s = live.screen(70, 14);
    assert_eq!(s.lines().last().unwrap(), ":box 2-3 hi", "{s}");
    // Everything above the last row is the presentation frame as it was.
    let above = |t: &str| t.lines().take(13).collect::<Vec<_>>().join("\n");
    assert_eq!(above(&s), above(&hidden));
    assert_eq!(
        cursor(&live, 70, 14),
        Some((11, 13)),
        "cursor after the input"
    );

    // Same text and cursor as the status bar's command line.
    live.with(|a| a.present = false);
    assert_eq!(last_row(&live, 70, 14), ":box 2-3 hi");
    assert_eq!(cursor(&live, 70, 14), Some((11, 13)));
    live.with(|a| a.present = true);

    // Enter runs it and the command line is gone (the result takes the row).
    live.keys(&[KeyCode::Enter]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 3, "the box exists");
    assert!(!last_row(&live, 70, 14).starts_with(':'));
    assert_eq!(
        cursor(&live, 70, 14),
        None,
        "no cursor in presentation mode"
    );
}

#[test]
fn ac1_esc_hides_it_at_once_and_the_frame_is_back() {
    let live = presenting("t107-esc");
    let before = live.snapshot(70, 14);
    live.type_text(":box 2-3 hi");
    live.keys(&[KeyCode::Esc]);
    assert_eq!(live.with(|a| a.mode.clone()), Mode::Normal);
    assert!(
        live.with(|a| a.present),
        "Esc leaves the command line, not P"
    );
    assert_eq!(live.snapshot(70, 14), before, "byte-identical after Esc");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2, "nothing ran");
    // Backspace on an empty line closes it too.
    live.keys(&[KeyCode::Char(':'), KeyCode::Backspace]);
    assert_eq!(live.snapshot(70, 14), before);
}

#[test]
fn ac2_the_result_shows_for_two_seconds_then_goes() {
    let live = presenting("t107-ac2");
    let before = live.snapshot(70, 14);
    live.type_text(":box 2-3 hi");
    live.keys(&[KeyCode::Enter]);
    let row = last_row(&live, 70, 14);
    assert!(row.starts_with(" a3: box 2-3"), "{row:?}");
    assert!(!row.contains("1/2"), "the row is the message's: {row:?}");

    // Deterministic: still there just before the deadline, gone at it.
    let until = live
        .with(|a| a.present_message_until)
        .expect("deadline set");
    assert!(until > Instant::now() + PRESENT_MESSAGE_TTL - Duration::from_millis(500));
    assert!(!live.with(move |a| a.present_message_tick(until - Duration::from_millis(1))));
    assert!(last_row(&live, 70, 14).starts_with(" a3: box"));
    assert!(live.with(move |a| a.present_message_tick(until)));
    assert_eq!(live.with(|a| a.message.clone()), None, "seen and gone");
    let (symbols, _) = live.snapshot(70, 14);
    assert_eq!(symbols.lines().last(), before.0.lines().last());
    assert!(symbols.contains("hi"), "the new box is drawn: {symbols}");
}

#[test]
fn ac2_unknown_command_tells_the_presenter_and_expires_by_itself() {
    let live = presenting("t107-foo");
    let before = live.snapshot(70, 14);
    let start = Instant::now();
    live.type_text(":foo");
    live.keys(&[KeyCode::Enter]);
    let row = last_row(&live, 70, 14);
    // The name typed is not repeated (t-167 rework).
    assert!(row.starts_with(" unknown command"), "{row:?}");
    assert!(!row.contains("foo"), "{row:?}");
    // Drawn as an error (the theme's error color, bold).
    let (_, styles) = live.snapshot(70, 14);
    let err = live.with(|a| a.theme.error);
    let hex = format!("#{:02x}{:02x}{:02x}", err.0, err.1, err.2);
    assert!(
        styles
            .lines()
            .last()
            .unwrap()
            .contains(&format!(" 1:{hex}/")),
        "{}",
        styles.lines().last().unwrap()
    );
    // The real loop's tick hides it after ~2 s (never before the TTL).
    wait_until(&live, "message hidden", |l| l.snapshot(70, 14) == before);
    assert!(start.elapsed() >= PRESENT_MESSAGE_TTL);
}

#[test]
fn ac2_a_long_message_is_cut_with_an_ellipsis() {
    let live = presenting("t107-long");
    live.type_text(":mark nope 2:s");
    live.keys(&[KeyCode::Enter]);
    let row = last_row(&live, 30, 10);
    assert!(row.ends_with('…'), "{row:?}");
    assert!(row.chars().count() <= 30);
}

#[test]
fn ac2_the_next_key_hides_it_and_p_does_not_bring_it_back() {
    let live = presenting("t107-next");
    live.type_text(":foo");
    live.keys(&[KeyCode::Enter]);
    assert!(live.with(|a| a.present_message_shown()));
    live.keys(&[KeyCode::Char('n')]);
    assert!(!live.with(|a| a.present_message_shown()));
    assert!(!last_row(&live, 70, 14).contains("unknown"));

    live.type_text(":foo");
    live.keys(&[KeyCode::Enter, KeyCode::Char('P')]);
    assert!(!live.with(|a| a.present));
    assert_eq!(live.with(|a| a.present_message_until), None);
}

#[test]
fn ac2_outside_presentation_mode_nothing_changes() {
    let live = Live::from_code(CODE, "rs", "t107-normal");
    live.type_text(":foo");
    live.keys(&[KeyCode::Enter]);
    assert_eq!(live.with(|a| a.present_message_until), None);
    let row = last_row(&live, 100, 14);
    assert!(
        row.contains("NORMAL") && row.contains("unknown command"),
        "{row}"
    );
}

#[test]
fn ac3_nothing_else_of_the_status_bar() {
    let live = presenting("t107-ac3");
    for (typed, enter) in [(":box 2-3 hi", false), (":foo", true)] {
        live.type_text(typed);
        if enter {
            live.keys(&[KeyCode::Enter]);
        }
        let s = live.screen(70, 14);
        for word in ["NORMAL", "step ", "?help", "ann.", "1/2", "dark", "L1"] {
            assert!(!s.contains(word), "{word} on {typed}: {s}");
        }
        live.keys(&[KeyCode::Esc]);
        live.with(|a| a.set_present(true));
    }
    // Hidden: the discreet step counter, as before.
    assert!(last_row(&live, 70, 14).ends_with("1/2"));
}

#[test]
fn ac4_hidden_command_line_frame_is_unchanged() {
    // Snapshot recorded before t-107 changed the drawing code.
    let live = presenting("t107-snap");
    let (symbols, styles) = live.snapshot(70, 14);
    insta::assert_snapshot!("t107-present-symbols", symbols);
    insta::assert_snapshot!("t107-present-styles", styles);
}

#[test]
fn ac4_exports_do_not_depend_on_the_command_line() {
    // `render` and `--out` never draw the TUI frame: same output with a
    // live instance in presentation mode typing a command or not.
    let out = scratch("t107-render");
    let doc = out.join("doc.tiza");
    std::fs::copy(fixture("t-107/present.rs.tiza"), &doc).unwrap();
    let a = tiza_ok(&["render", doc.to_str().unwrap(), "--format", "txt"]);
    let live = presenting("t107-render-live");
    live.type_text(":box 2-3 hi");
    let b = tiza_ok(&["render", doc.to_str().unwrap(), "--format", "txt"]);
    assert_eq!(a, b);
    let tape = out.join("a.tape");
    tiza_ok(&[
        "render",
        doc.to_str().unwrap(),
        "--out",
        tape.to_str().unwrap(),
    ]);
    let t = std::fs::read_to_string(&tape).unwrap();
    assert!(!t.contains("Type \":\""), "the tape types no command: {t}");
}

#[test]
fn corner_short_terminals() {
    for rows in [1u16, 2] {
        let live = presenting(&format!("t107-rows{rows}"));
        live.type_text(":box 2-3 hi");
        assert_eq!(last_row(&live, 40, rows), ":box 2-3 hi", "{rows} rows");
        live.keys(&[KeyCode::Enter]);
        assert_eq!(live.with(|a| a.doc.annotations.len()), 3, "it ran");
        assert!(last_row(&live, 40, rows).starts_with(" a3: box"));
    }
    // Narrow: input longer than the row, the cursor stays on it.
    let live = presenting("t107-narrow");
    live.type_text(":note 2 a long label that does not fit");
    assert_eq!(cursor(&live, 12, 6), Some((11, 5)));
    assert_eq!(cursor(&live, 1, 6), Some((0, 5)));
    let _ = live.screen(2, 1);
}

#[test]
fn corner_supervised_proposal_while_presenting() {
    let live = presenting("t107-supervised");
    live.command("supervise on");
    live.type_text(":box 2-3");
    let socket = live.socket.clone();
    let h = std::thread::spawn(move || {
        tiza_tui::ipc::call(
            &socket,
            "annotation.add",
            json!({"type": "note", "line": 2, "text": "agent"}),
        )
        .expect("socket")
    });
    wait_until(&live, "proposal parked", |l| {
        l.with(|a| a.proposals.len()) == 1
    });
    assert_eq!(live.with(|a| a.overlay), Overlay::None, "no popup");
    let row = last_row(&live, 100, 14);
    assert!(
        row.starts_with(":box 2-3") && row.contains("1 proposal waiting for review"),
        "{row}"
    );
    // Hidden again: the existing chip is the only sign.
    live.keys(&[KeyCode::Esc]);
    assert!(last_row(&live, 100, 14).contains("⏸ 1 to review"));
    live.with(|a| a.reject_proposal());
    let r = h.join().unwrap();
    assert!(r.error.is_some());
}

#[test]
fn corner_colon_with_an_overlay_open_is_unchanged() {
    let live = presenting("t107-overlay");
    live.keys(&[KeyCode::Char('?')]);
    assert_eq!(live.with(|a| a.overlay), Overlay::Help);
    live.keys(&[KeyCode::Char(':')]);
    assert_eq!(live.with(|a| a.mode.clone()), Mode::Normal);
    assert_eq!(live.with(|a| a.overlay), Overlay::Help);
}
