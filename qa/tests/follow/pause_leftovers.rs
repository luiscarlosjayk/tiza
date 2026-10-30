//! Two leftovers of a follow pause with a change that waits.
//!
//! - The file went missing or could not be read, and came back with other
//!   code: the bar still said `removed` or `not valid UTF-8`, because a
//!   pause gives its notice once.
//! - With autosave off, the code that the pause embedded is not saved, but
//!   the list of what quitting leaves unsaved did not name it once another
//!   change was in it.
//!
//! Acceptance criteria:
//! - AC1 When the file comes back, or can be read again, and a change
//!   still waits, the notice says again which step loads it. A pause that
//!   gave no notice yet, a file back as the screen has it, and a file back
//!   at a step with no pause do what they did.
//! - AC2 What quitting leaves unsaved names, first, the code that the
//!   pause embedded while it is on screen and not saved (a change waits,
//!   the file went missing or cannot be read, or follow is off), alone or
//!   with other changes; not when the sidecar holds it (after `:w`, or
//!   with autosave on) or when no pause embedded code.
//!
//! The saves are read with `App::read_save` (what the watcher does after
//! its debounce) on the TUI thread.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, scratch};

const OLD: &str = "fn f(t: &str) -> usize {\n    let n = t.len();\n    n + 1\n}\n";
const NEW: &str = "fn f(t: &str) -> usize {\n    let n = t.chars().count();\n    n + 1\n}\n";
const OTHER: &str = "fn f(t: &str) -> usize {\n    let n = t.bytes().len();\n    n + 1\n}\n";
const NOT_UTF8: &[u8] = b"fn f() {\n    \xff\xfe\n}\n";
const NOTICE: &str = "step 1 loads the change to main.rs";

/// A cursor move at step 1 and the patch of `n + 1` at step 2: the resume
/// step is 1.
fn document() -> Value {
    json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "step": 0,
        "moves": [
            {"at_step": 1, "action": "cursor", "line": 1},
            {"at_step": 2, "action": "code.patch", "op": "replace", "find": "n + 1", "content": "    n * 2"}
        ],
    })
}

/// `main.rs` (OLD) and its sidecar, following, at `step`.
fn following_at(name: &str, step: u32) -> (Live, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, OLD).unwrap();
    let tiza = dir.join("main.rs.tiza");
    std::fs::write(&tiza, document().to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza, dir.join("live.sock"));
    live.command("follow on");
    live.api_ok("step.set", json!({"step": step}));
    (live, src)
}

/// Saves like an editor, then reads it as the watcher does after a save.
fn save_and_read(live: &Live, src: &Path, content: &[u8]) {
    let tmp = src.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, src).unwrap();
    live.with(|app| app.read_save());
}

fn remove_and_read(live: &Live, src: &Path) {
    std::fs::remove_file(src).unwrap();
    live.with(|app| app.read_save());
}

fn message(live: &Live) -> String {
    live.message().unwrap_or_default().0
}

fn pending(live: &Live) -> Value {
    live.api_ok("follow.status", json!({}))["pending"].clone()
}

#[test]
fn the_notice_comes_back_with_the_file() {
    // The file removed, and back with other code.
    let (live, src) = following_at("leftover-removed", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(message(&live), NOTICE);
    remove_and_read(&live, &src);
    assert_eq!(
        message(&live),
        "main.rs removed; waiting for it to come back"
    );
    save_and_read(&live, &src, OTHER.as_bytes());
    assert_eq!(message(&live), NOTICE);
    assert_eq!(pending(&live), json!(true));
    // Not valid UTF-8, and valid again with other code.
    let (live, src) = following_at("leftover-utf8", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    save_and_read(&live, &src, NOT_UTF8);
    assert!(
        message(&live).contains("not valid UTF-8"),
        "{}",
        message(&live)
    );
    save_and_read(&live, &src, OTHER.as_bytes());
    assert_eq!(message(&live), NOTICE);
    assert_eq!(pending(&live), json!(true));
    // Unreadable once more in the same pause: its error shows again.
    save_and_read(&live, &src, NOT_UTF8);
    assert!(
        message(&live).contains("not valid UTF-8"),
        "{}",
        message(&live)
    );
    save_and_read(&live, &src, OTHER.as_bytes());
    assert_eq!(message(&live), NOTICE);
    // The change loads once at the resume step.
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(
        live.with(|app| app.doc.base().to_vec()),
        OTHER.lines().map(str::to_string).collect::<Vec<_>>()
    );
}

#[test]
fn the_notice_comes_back_after_an_empty_first_write_and_a_read_error() {
    // An editor that truncates first: the empty read waits for the second
    // write, which is the one that finds the file back.
    let (live, src) = following_at("leftover-empty", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    remove_and_read(&live, &src);
    save_and_read(&live, &src, b"");
    save_and_read(&live, &src, OTHER.as_bytes());
    assert_eq!(message(&live), NOTICE);
    // A file that cannot be read for another reason (its permissions).
    use std::os::unix::fs::PermissionsExt;
    let (live, src) = following_at("leftover-perms", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o000)).unwrap();
    live.with(|app| app.read_save());
    assert!(
        message(&live).contains("cannot reload main.rs"),
        "{}",
        message(&live)
    );
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o644)).unwrap();
    save_and_read(&live, &src, OTHER.as_bytes());
    assert_eq!(message(&live), NOTICE);
    // Readable again, as the screen has it: the bar says so.
    let (live, src) = following_at("leftover-readable", 2);
    save_and_read(&live, &src, NOT_UTF8);
    save_and_read(&live, &src, OLD.as_bytes());
    assert_eq!(message(&live), "main.rs can be read again; following it");
}

#[test]
fn the_other_returns_of_the_file_do_what_they_did() {
    // No notice yet in the pause: the save after the return gives it.
    let (live, src) = following_at("leftover-first", 2);
    remove_and_read(&live, &src);
    save_and_read(&live, &src, OTHER.as_bytes());
    assert_eq!(message(&live), NOTICE);
    // Back as the screen has it: nothing waits.
    let (live, src) = following_at("leftover-same", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    remove_and_read(&live, &src);
    save_and_read(&live, &src, OLD.as_bytes());
    assert_eq!(pending(&live), json!(false));
    assert_ne!(message(&live), NOTICE);
    // No pause (step 0): the file loads.
    let (live, src) = following_at("leftover-step0", 0);
    remove_and_read(&live, &src);
    save_and_read(&live, &src, OTHER.as_bytes());
    assert!(message(&live).contains("reloaded"), "{}", message(&live));
}

fn unsaved(live: &Live) -> Option<String> {
    live.with(|app| app.unsaved_summary())
}

const CODE: &str = "the code on screen in place of main.rs";

fn note(live: &Live, id: &str) {
    live.api_ok(
        "annotation.add",
        json!({"id": id, "type": "note", "line": 3, "text": "a note"}),
    );
}

#[test]
fn quitting_names_the_code_that_the_pause_embedded() {
    let (live, src) = following_at("leftover-unsaved", 2);
    live.command("autosave off");
    save_and_read(&live, &src, NEW.as_bytes());
    let alone = unsaved(&live).expect("unsaved");
    assert!(
        alone.ends_with(&format!("1 change not saved ({CODE})")),
        "{alone}"
    );
    // First in the list, with the other changes.
    note(&live, "n9");
    let both = unsaved(&live).expect("unsaved");
    assert!(
        both.contains(&format!("2 changes not saved ({CODE}, ")),
        "{both}"
    );
    // `u` of the note leaves the code alone again.
    live.command("undo");
    let undone = unsaved(&live).expect("unsaved");
    assert!(
        undone.ends_with(&format!("1 change not saved ({CODE})")),
        "{undone}"
    );
    live.command("redo");
    // `:w` writes the embedded code: nothing is left.
    live.command("w");
    assert_eq!(unsaved(&live), None);
    // The sidecar holds that code now: another note alone is listed.
    note(&live, "n8");
    let after = unsaved(&live).expect("unsaved");
    assert!(
        after.contains("1 change not saved (") && !after.contains(CODE),
        "{after}"
    );
}

#[test]
fn quitting_names_the_embedded_code_also_when_nothing_waits() {
    // A change waited, then the file could not be read: the pause dropped
    // the change, and the code it embedded is still on screen, not saved.
    let (live, src) = following_at("leftover-unsaved-dropped", 2);
    live.command("autosave off");
    save_and_read(&live, &src, NEW.as_bytes());
    save_and_read(&live, &src, NOT_UTF8);
    assert_eq!(pending(&live), json!(false));
    let list = unsaved(&live).expect("unsaved");
    assert!(
        list.ends_with(&format!("1 change not saved ({CODE})")),
        "{list}"
    );
    // Follow off: still on screen, still named.
    live.command("follow off");
    let list = unsaved(&live).expect("unsaved");
    assert!(
        list.ends_with(&format!("1 change not saved ({CODE})")),
        "{list}"
    );
}

#[test]
fn quitting_does_not_name_code_that_no_pause_embedded_or_that_is_saved() {
    // No pause at all.
    let (live, _src) = following_at("leftover-unsaved-none", 0);
    live.command("autosave off");
    note(&live, "n9");
    let list = unsaved(&live).expect("unsaved");
    assert!(
        list.contains("1 change not saved (") && !list.contains(CODE),
        "{list}"
    );
    // The pause embedded code, then it loaded, then a paste: the paste is
    // listed as a paste, not as the code of the pause.
    let (live, src) = following_at("leftover-unsaved-paste", 2);
    live.command("autosave off");
    save_and_read(&live, &src, NEW.as_bytes());
    live.api_ok("step.set", json!({"step": 0}));
    live.with(|app| app.paste("fn g() {}\n"));
    let list = unsaved(&live).expect("unsaved");
    assert!(!list.contains(CODE), "{list}");
    // Autosave on: the sidecar holds the embedded code at once.
    let (live, src) = following_at("leftover-unsaved-autosave", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(unsaved(&live), None);
}
