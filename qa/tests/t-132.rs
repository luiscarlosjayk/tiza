//! t-132 — Step 0: the confirmation of a new annotation says it lands on step 1.
//!
//! Acceptance criteria:
//! - AC1 Adding an annotation (`:` command, selection keys, socket/MCP reply
//!   message if any) while the view is at a step where it is not visible ends
//!   the confirmation with `(step N, press n)` or similar; nothing changes
//!   when it is visible.
//! - AC2 Step logic of t-079 unchanged (new annotations at step 0 still go to
//!   step 1, the view stays at 0).
//! - AC3 qa test; CHANGELOG.
//!
//! Corner cases: `:step ID N` past the view gets the same hint; messages cut
//! with `…` on narrow terminals keep the step hint visible when possible.
//! The socket/MCP reply carries no message (`{id, revision}`), so it is
//! unchanged here (t-155 later added `step`, `shown_step`, `visible`).

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::Live;
use tiza_tui::app::{step_hint, step_hint_start};
use tiza_tui::ui::fit_message;

const CODE: &str = "fn main() {\n    let s = String::from(\"hello\");\n    takes(s);\n}\n\nfn takes(s: String) {\n    println!(\"{s}\");\n}\n";

fn info(live: &Live) -> String {
    let (m, err) = live.message().expect("a message");
    assert!(!err, "unexpected error: {m}");
    m
}

/// A new document at step 0 (the code alone), no message on the bar.
fn at_step_0(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.command("step 0");
    live.with(|a| a.message = None);
    live
}

fn step(live: &Live) -> u32 {
    live.with(|a| a.doc.step)
}

#[test]
fn ac1_command_at_step_0_names_step_1_and_ac2_view_stays() {
    let live = at_step_0("t132-ac1-cmd");
    assert_eq!(step(&live), 0);
    live.command("box 2-3 hi");
    assert_eq!(info(&live), "a1: box 2-3 (step 1, press n)");
    // AC2: t-079 unchanged — the box is at step 1, the view stays at 0.
    assert_eq!(step(&live), 0);
    assert_eq!(live.with(|a| a.doc.annotations[0].step), 1);
    assert!(!live.screen(70, 14).contains("hi"));
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(step(&live), 1);
    assert!(live.screen(70, 14).contains("hi"));
}

#[test]
fn ac1_visible_annotation_keeps_the_plain_confirmation() {
    let live = at_step_0("t132-ac1-plain");
    live.command("box 2-3 hi");
    live.command("next");
    live.command("note 6 takes ownership");
    assert_eq!(info(&live), "a2: note at L6");
    live.command("hl 3:takes");
    assert!(!info(&live).contains("press n"));
}

#[test]
fn ac1_each_kind_of_command_gets_the_hint() {
    let live = at_step_0("t132-ac1-kinds");
    for cmd in [
        "note 6 takes ownership",
        "callout 2-3 moved",
        "arrow 2:s 3:takes moves",
        "hl 3:takes",
        "mark move 3:s here",
    ] {
        live.command(cmd);
        let m = info(&live);
        assert!(m.ends_with(" (step 1, press n)"), "{cmd}: {m}");
    }
    // The spotlight already names its step: only how to get there.
    live.command("spot 2-3");
    assert_eq!(
        info(&live),
        format!(
            "a{}: spotlight at step 1 (press n)",
            live.with(|a| a.doc.annotations.len())
        )
    );
    assert_eq!(step(&live), 0);
}

#[test]
fn ac1_selection_keys_get_the_hint() {
    let live = at_step_0("t132-ac1-keys");
    // `takes` on line 3, highlighted with `h` from a selection.
    live.with(|a| {
        a.cursor_line = 3;
        a.line_start();
        a.select_word();
    });
    live.keys(&[KeyCode::Char('h')]);
    let m = info(&live);
    assert!(
        m.starts_with("a1: highlight") && m.ends_with(" (step 1, press n)"),
        "{m}"
    );
    // `s` spotlights through `:spot`: the hint is added once, not twice.
    live.with(|a| {
        a.cursor_line = 2;
        a.start_select(true);
    });
    live.keys(&[KeyCode::Char('s')]);
    let m = info(&live);
    assert_eq!(m.matches("press n").count(), 1, "{m}");
    assert_eq!(step(&live), 0);
}

#[test]
fn corner_step_id_past_the_view_gets_the_hint() {
    let live = at_step_0("t132-step-id");
    live.command("box 2-3 hi");
    live.command("next");
    live.command("note 6 takes ownership");
    live.command("step a2 2");
    assert_eq!(info(&live), "a2 now appears at step 2 (press n)");
    // Two steps ahead: `n` once is not enough, name the command.
    live.command("prev");
    live.command("step a2 3");
    assert_eq!(info(&live), "a2 now appears at step 3 (:step 3 shows it)");
    live.command("step a2 2");
    // `:set ID step=N` and `+` on the selected annotation too.
    live.command("step 1");
    live.command("set a1 step=2");
    assert_eq!(info(&live), "a1: updated (step 2, press n)");
    live.command("step 2");
    live.with(|a| a.selected = Some("a2".into()));
    live.keys(&[KeyCode::Char('+')]);
    assert_eq!(info(&live), "a2 → step 3 (press n)");
    // Back to a visible step: plain.
    live.keys(&[KeyCode::Char('-')]);
    assert_eq!(info(&live), "a2 → step 2");
}

#[test]
fn corner_errors_undo_and_unchanged_messages_get_no_hint() {
    let live = at_step_0("t132-none");
    live.command("box 99-100 nope");
    let (m, err) = live.message().unwrap();
    assert!(err && !m.contains("press n"), "{m}");
    live.command("box 2-3 hi");
    live.command("del a1");
    live.command("undo");
    assert!(!info(&live).contains("press n"), "{}", info(&live));
    live.command("step 0");
    assert!(!info(&live).contains("press n"));
}

#[test]
fn corner_socket_reply_is_unchanged_and_ac2_holds() {
    let live = at_step_0("t132-socket");
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "box", "from_line": 2, "to_line": 3, "label": "hi"}),
    );
    // No message on the wire; t-155 added where it stands against the view.
    let keys: Vec<&String> = r.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        ["hidden", "id", "revision", "shown_step", "step", "visible"]
    );
    assert_eq!(r["visible"], json!(false));
    assert_eq!(step(&live), 0);
    assert_eq!(live.with(|a| a.doc.annotations[0].step), 1);
}

#[test]
fn fit_message_and_hint_parsing() {
    assert_eq!(step_hint("a1: box 3-5", 1, 0), " (step 1, press n)");
    assert_eq!(
        step_hint("a1: box 3-5", 3, 0),
        " (step 3, :step 3 shows it)"
    );
    assert_eq!(step_hint("a3 → step 2", 2, 1), " (press n)");
    // `step 12` does not name step 1.
    assert_eq!(step_hint("moved to step 12", 1, 0), " (step 1, press n)");
    for m in [
        "a1: box 3-5 (step 1, press n)",
        "a1: box 3-5 (step 3, :step 3 shows it)",
        "a3 → step 2 (press n)",
        "a3 now appears at step 4 (:step 4 shows it)",
    ] {
        assert_eq!(step_hint_start(m), m.rfind(" ("), "{m}");
    }
    for m in ["a1: box 3-5", "a1: note (hello)", "x (step one, press n)"] {
        assert_eq!(step_hint_start(m), None, "{m}");
    }
    let m = "a17: box 3-5 a long label (step 1, press n)";
    assert_eq!(fit_message(m, 100), m);
    assert_eq!(fit_message(m, 27), "a17: box… (step 1, press n)");
    // Too narrow for the hint: the beginning, as before.
    assert_eq!(fit_message(m, 10), "a17: box …");
    // No hint: unchanged behaviour.
    assert_eq!(fit_message("abcdefghij", 5), "abcd…");
}
