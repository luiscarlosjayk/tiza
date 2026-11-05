//! The message of an undo or a redo goes when a later edit comes in.
//!
//! An agent adds a note, `u` takes it away, and the agent adds it again:
//! the bar said `undone: …` with the note on the screen, also after
//! resizes. Now the next frame after a later edit has no such message. A
//! resize alone keeps the message: nothing made it old.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::Live;

const CODE: &str = "fn main() {\n    let s = String::new();\n    drop(s);\n}\n";

fn bar(live: &Live, cols: u16, rows: u16) -> String {
    live.screen(cols, rows).lines().last().unwrap().to_string()
}

fn note(live: &Live) {
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "id": "a7", "line": 2, "text": "owned here"}),
    );
}

#[test]
fn an_undo_message_goes_when_an_agent_edits_after_it() {
    let live = Live::from_code(CODE, "rs", "undo-message");
    note(&live);
    live.keys(&[KeyCode::Char('u')]);
    let (said, _) = live.message().unwrap();
    assert!(said.starts_with("undone: "), "{said}");
    // A resize does not make it old: it stays at each size.
    for (cols, rows) in [(100, 30), (80, 24), (120, 40)] {
        let b = bar(&live, cols, rows);
        assert!(b.contains("undone: "), "{cols}x{rows}: {b}");
    }
    // The agent adds the note again: the next frame drops the message.
    note(&live);
    for (cols, rows) in [(100, 30), (80, 24), (120, 40)] {
        let s = live.screen(cols, rows);
        let b = s.lines().last().unwrap();
        assert!(s.contains("owned here"), "{cols}x{rows}: no note\n{s}");
        assert!(!b.contains("undone"), "{cols}x{rows}: {b}");
    }
    assert_eq!(live.message(), None);
    // A redo says so, and a later delete by the agent makes it old too.
    live.keys(&[KeyCode::Char('u')]);
    live.keys(&[KeyCode::Char('U')]);
    let (said, _) = live.message().unwrap();
    assert!(said.starts_with("redone: "), "{said}");
    assert!(bar(&live, 100, 30).contains("redone: "));
    live.api_ok("annotation.delete", json!({"id": "a7"}));
    assert!(!bar(&live, 100, 30).contains("redone"));
    // Two undos in a row: the second message is the one on the bar.
    note(&live);
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "id": "a8", "line": 3, "text": "dropped"}),
    );
    live.keys(&[KeyCode::Char('u')]);
    live.keys(&[KeyCode::Char('u')]);
    let (said, _) = live.message().unwrap();
    assert!(said.starts_with("undone: "), "{said}");
    assert!(bar(&live, 100, 30).contains(&said), "{said}");
}
