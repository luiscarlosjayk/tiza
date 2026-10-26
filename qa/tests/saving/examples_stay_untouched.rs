//! Autosave: annotating an example must not rewrite it on disk
//! (`--no-save`, `:autosave on|off`, a read-only notice once), and one
//! saving rule shared with `scene put`: a document from another
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
//!   discard (`:w`, `:saveas PATH`, `:q!`).
//! - AC5 Without the flag everything is as before (autosave on).
//! - AC6 A copy of an example opened with `--no-save` and annotated over the
//!   socket is byte-identical afterwards.
//! - AC7 A `scene put` of a different document turns autosave off (the file
//!   that was open is never overwritten); `:w` saves on purpose,
//!   `:autosave on` resumes. (Also covered from the CLI in the scene put tests.)

use serde_json::json;
use tiza_core::fs;
use tiza_qa::{Live, fixture, scratch};

fn opened(name: &str, no_save: bool) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    let path = dir.join("ownership.rs.tiza");
    std::fs::copy(fixture("shared/ownership/ownership.rs.tiza"), &path).unwrap();
    std::fs::copy(
        fixture("shared/ownership/ownership.rs"),
        dir.join("ownership.rs"),
    )
    .unwrap();
    let doc = fs::open(path.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    if no_save {
        // What `--no-save` does in `tiza_tui::run`.
        live.with(|a| a.autosave = false);
    }
    (live, path)
}

#[test]
fn no_save_leaves_the_example_byte_identical() {
    let (live, path) = opened("nosave", true);
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
fn autosave_command_and_status() {
    let (live, path) = opened("command", false);
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
fn read_only_sidecar_warns_once_and_keeps_going() {
    let dir = scratch("readonly");
    let path = dir.join("doc.tiza");
    std::fs::copy(fixture("shared/export/export.tiza"), &path).unwrap();
    let doc = fs::open(path.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    // `chmod a-w doc.tiza`: the atomic replace would walk over it, so the
    // read-only bit itself is honoured (also when running as root).
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms.clone()).unwrap();
    let before = std::fs::read(&path).unwrap();
    // (1) ONE notice on the first failed autosave, in the agreed words.
    live.command("note 1 first");
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.starts_with("read-only: could not save")
            && msg.ends_with("; :saveas PATH saves elsewhere"),
        "{msg}"
    );
    // (2) Then silence: the second annotation's own confirmation shows,
    // and the bar carries the persistent indicator instead.
    live.command("note 2 second");
    let (msg, err) = live.message().unwrap();
    assert!(
        !err && msg.contains("note") && msg.contains("L2"),
        "own message expected: {msg}"
    );
    assert_eq!(live.with(|a| a.doc.annotations.len()), 10);
    assert_eq!(std::fs::read(&path).unwrap(), before, "untouched");
    let bar = live.screen(140, 20).lines().last().unwrap().to_string();
    assert!(bar.contains("read-only • unsaved (:w)"), "{bar}");
    assert!(
        !bar.contains("could not save"),
        "no repeated error on the bar: {bar}"
    );
    assert_eq!(
        live.api_ok("status", serde_json::json!({}))["autosave"],
        true
    );
    // (3) `:q` refuses while there are unsaved changes, `:q!` would discard.
    live.command("q");
    assert!(!live.with(|a| a.quit));
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.starts_with("unsaved (read-only): :q! quits without saving"),
        "{msg}"
    );
    // (4) Writable again: the next autosave succeeds, says so once, and the
    // indicator goes away.
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o644);
    std::fs::set_permissions(&path, perms).unwrap();
    live.command("note 3 third");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.starts_with("saved to "), "{msg}");
    assert_ne!(std::fs::read(&path).unwrap(), before, "written now");
    let bar = live.screen(140, 20).lines().last().unwrap().to_string();
    assert!(
        !bar.contains("read-only") && !bar.contains("unsaved"),
        "{bar}"
    );
    live.command("note 4 fourth");
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("L4"), "back to normal: {msg}");
    live.command("q");
    assert!(live.with(|a| a.quit));
}

#[test]
fn read_only_then_saveas_elsewhere_clears_the_state() {
    let dir = scratch("readonly-w");
    let path = dir.join("doc.tiza");
    std::fs::copy(fixture("shared/export/export.tiza"), &path).unwrap();
    let doc = fs::open(path.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms.clone()).unwrap();
    live.command("note 1 first");
    assert!(live.message().unwrap().1);
    // (`:w PATH` only writes a copy, as in Vim.)
    let elsewhere = dir.join("elsewhere.tiza");
    live.command(&format!("saveas {}", elsewhere.display()));
    assert!(elsewhere.exists());
    let bar = live.screen(140, 20).lines().last().unwrap().to_string();
    assert!(!bar.contains("read-only"), "{bar}");
    live.command("q");
    assert!(live.with(|a| a.quit));
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o644);
    std::fs::set_permissions(&path, perms).unwrap();
}

#[test]
fn quit_with_autosave_off_says_how_to_save_or_discard() {
    let (live, _) = opened("quit", true);
    live.command("q");
    assert!(live.with(|a| a.quit), "nothing changed: quits");
    live.with(|a| a.quit = false);
    live.command("note 3 x");
    live.command("q");
    assert!(!live.with(|a| a.quit));
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("unsaved changes (autosave off)")
            && msg.contains(":saveas PATH saves elsewhere")
            && msg.contains(":q! quits without saving"),
        "{msg}"
    );
    live.command("q!");
    assert!(live.with(|a| a.quit));
}

#[test]
fn put_of_another_document_stops_autosave_until_asked() {
    let (live, path) = opened("put", false);
    let before = std::fs::read(&path).unwrap();
    let other = fs::load_document(&fixture("shared/export/export.tiza")).unwrap();
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
