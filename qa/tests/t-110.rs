//! t-110 — Autosave: annotating an example must not rewrite it on disk
//! (`--no-save`, `:autosave on|off`, a read-only notice once), and one
//! saving rule shared with `scene put` (t-039): a document from another
//! source is never autosaved over the file that was open.
//!
//! Acceptance criteria:
//! - AC1 `tiza FILE --no-save` never writes the sidecar on its own; the bar
//!   shows `no-save`; `:w` / `:w PATH` still save explicitly.
//! - AC2 `:autosave on|off` switches it live, `:autosave` alone shows it;
//!   `status` carries `autosave`.
//! - AC3 A sidecar that cannot be written warns once (`read-only: …`) and
//!   the TUI keeps working; no panic, no message per autosave.
//! - AC4 `:q` with unsaved changes while autosave is off says how to save or
//!   discard (`:w`, `:w PATH`, `:q!`).
//! - AC5 Without the flag everything is as before (autosave on).
//! - AC6 A copy of an example opened with `--no-save` and annotated over the
//!   socket is byte-identical afterwards.
//! - AC7 A `scene put` of a different document turns autosave off (the file
//!   that was open is never overwritten); `:w` saves on purpose,
//!   `:autosave on` resumes. (Also covered from the CLI in t-039.)

use serde_json::json;
use tiza_core::fs;
use tiza_qa::{Live, example, fixture, scratch};

fn opened(name: &str, no_save: bool) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    let path = dir.join("ownership.rs.tiza");
    std::fs::copy(example("ownership.rs.tiza"), &path).unwrap();
    std::fs::copy(example("ownership.rs"), dir.join("ownership.rs")).unwrap();
    let doc = fs::open(path.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    if no_save {
        // What `--no-save` does in `tiza_tui::run`.
        live.with(|a| a.autosave = false);
    }
    (live, path)
}

#[test]
fn ac1_ac6_no_save_leaves_the_example_byte_identical() {
    let (live, path) = opened("t110-nosave", true);
    let before = std::fs::read(&path).unwrap();
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 3, "text": "hola", "step": 1}),
    );
    live.command("note 2 again");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 18);
    assert_eq!(std::fs::read(&path).unwrap(), before, "bytes identical");
    let screen = live.screen(120, 20);
    assert!(screen.contains("unsaved (:w)"), "{screen}");
    // `:w PATH` saves explicitly, elsewhere; the example is still untouched.
    let copy = path.with_file_name("copy.tiza");
    live.command(&format!("w {}", copy.display()));
    assert!(copy.exists());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(
        !live.with(|a| a.autosave),
        "`:w` does not turn autosave back on"
    );
}

#[test]
fn ac2_autosave_command_and_status() {
    let (live, path) = opened("t110-command", false);
    let status = live.api_ok("status", json!({}));
    assert_eq!(status["autosave"], json!(true));
    live.command("autosave");
    assert_eq!(live.message().unwrap().0, "autosave on");
    live.command("autosave off");
    assert_eq!(
        live.message().unwrap().0,
        "autosave off (:w saves explicitly)"
    );
    assert_eq!(live.api_ok("status", json!({}))["autosave"], json!(false));
    let before = std::fs::read(&path).unwrap();
    live.command("note 3 x");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    live.command("autosave");
    assert_eq!(
        live.message().unwrap().0,
        "autosave off (unsaved changes: :w)"
    );
    let screen = live.screen(120, 20);
    assert!(screen.contains("unsaved (:w)"), "{screen}");
    // Turning it on writes what is pending.
    live.command("autosave on");
    let (msg, _) = live.message().unwrap();
    assert!(msg.starts_with("autosave on (saved to "), "{msg}");
    assert_ne!(std::fs::read(&path).unwrap(), before);
    let screen = live.screen(120, 20);
    assert!(
        !screen.contains("no-save") && !screen.contains("unsaved"),
        "{screen}"
    );
    live.command("autosave maybe");
    assert_eq!(live.message().unwrap().0, "usage: :autosave [on|off]");
}

#[test]
fn ac3_read_only_sidecar_warns_once_and_keeps_going() {
    let dir = scratch("t110-readonly");
    let ro = dir.join("ro");
    std::fs::create_dir_all(&ro).unwrap();
    let path = ro.join("doc.tiza");
    std::fs::copy(fixture("t-050/export.tiza"), &path).unwrap();
    let doc = fs::open(path.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    // A read-only directory: the sidecar cannot be replaced atomically.
    let mut perms = std::fs::metadata(&ro).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o555);
    std::fs::set_permissions(&ro, perms.clone()).unwrap();
    if std::fs::write(ro.join("probe"), "x").is_ok() {
        // Running as root (CI containers): the permission bit means nothing.
        std::fs::remove_file(ro.join("probe")).unwrap();
        return;
    }
    live.command("note 1 first");
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.starts_with("read-only: could not save"), "{msg}");
    live.command("note 2 second");
    let (msg, _) = live.message().unwrap();
    assert!(!msg.starts_with("read-only"), "only once: {msg}");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 10);
    // `:q` refuses, `:w PATH` elsewhere works, then `:q` quits.
    live.command("q");
    assert!(!live.with(|a| a.quit));
    let elsewhere = dir.join("elsewhere.tiza");
    live.command(&format!("w {}", elsewhere.display()));
    assert!(elsewhere.exists());
    live.command("q");
    assert!(live.with(|a| a.quit));
    perms.set_mode(0o755);
    std::fs::set_permissions(&ro, perms).unwrap();
}

#[test]
fn ac4_quit_with_autosave_off_says_how_to_save_or_discard() {
    let (live, _) = opened("t110-quit", true);
    live.command("q");
    assert!(live.with(|a| a.quit), "nothing changed: quits");
    live.with(|a| a.quit = false);
    live.command("note 3 x");
    live.command("q");
    assert!(!live.with(|a| a.quit));
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("unsaved changes (autosave off)")
            && msg.contains(":w PATH saves elsewhere")
            && msg.contains(":q! discards"),
        "{msg}"
    );
    live.command("q!");
    assert!(live.with(|a| a.quit));
}

#[test]
fn ac5_default_still_autosaves() {
    let (live, path) = opened("t110-default", false);
    let before = std::fs::read(&path).unwrap();
    live.command("note 3 x");
    assert_ne!(std::fs::read(&path).unwrap(), before);
    let screen = live.screen(120, 20);
    assert!(!screen.contains("no-save"), "{screen}");
}

#[test]
fn ac7_put_of_another_document_stops_autosave_until_asked() {
    let (live, path) = opened("t110-put", false);
    let before = std::fs::read(&path).unwrap();
    let other = fs::load_document(&fixture("t-050/export.tiza")).unwrap();
    live.api_ok("scene.put", json!({"document": other}));
    assert!(!live.with(|a| a.autosave));
    assert_eq!(live.with(|a| a.doc.code.len()), 6);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "the open file is untouched"
    );
    live.command("note 1 x");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // The same source again (an agent editing the open document): no change
    // of rule — but autosave is already off, so still nothing is written.
    live.with(|a| a.undo());
    live.with(|a| a.undo());
    assert_eq!(live.with(|a| a.doc.code.len()), 25);
    let mut same = live.with(|a| a.doc.clone());
    same.code_src = tiza_core::model::CodeSource::Embedded(same.code.clone());
    live.api_ok("scene.put", json!({"document": same}));
    assert!(!live.with(|a| a.autosave));
    live.command("autosave on");
    assert!(live.with(|a| a.autosave));
}
