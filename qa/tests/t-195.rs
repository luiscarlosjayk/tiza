//! t-195 — `:q!` and Ctrl-C saved unsaved changes on the way out (also
//! with `--no-save` / autosave off), although the bar said «:q! discards».
//!
//! Rule (Luis, 2026-10-20): no way out writes anything the chosen mode
//! would not have written already.
//!
//! Acceptance criteria:
//! - AC1 `:q!` quits without writing anything; the bar and the help say
//!   «:q! quits without saving».
//! - AC2 Ctrl-C behaves as `:q!`: nothing written, and stderr says what was
//!   not saved (after the summary of stale moves). With autosave on every
//!   change is on disk already, so there is nothing to write either. The
//!   rule is in the README.
//! - AC3 `:q` and `:wq` are unchanged (`:q` refuses over unsaved changes,
//!   `:wq` saves and quits, both still refuse over stale moves).
//! - AC4 This file; README and CHANGELOG.
//!
//! Corner cases: a read-only sidecar (autosave failed) writes nothing on
//! the way out either and stderr names what was not saved.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tiza_qa::scratch;

const CODE: &str = "fn main() {\n    let a = 1;\n}\n";

/// `main.rs` with an existing sidecar, as Tiza itself wrote it.
fn project(name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, CODE).unwrap();
    let mut doc = tiza_core::Document::from_code(CODE, "rs", Some(src.display().to_string()));
    doc.reference_file("main.rs", None);
    let tiza = dir.join("main.rs.tiza");
    tiza_core::fs::save_document(&doc, &tiza).unwrap();
    (dir, tiza)
}

fn stamp(p: &Path) -> (Vec<u8>, SystemTime) {
    (
        std::fs::read(p).unwrap(),
        std::fs::metadata(p).unwrap().modified().unwrap(),
    )
}

/// Runs `tiza ARGS main.rs` on a pty, types `keys` (each followed by a
/// short settle; `ready` waits for that text on the screen before the
/// last key), and returns what the terminal shows after Tiza left the
/// alternate screen: what it printed on stderr on the way out.
fn run(dir: &Path, name: &str, args: &[&str], keys: &[&str], before_last: Option<&str>) -> String {
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
args = json.loads(sys.argv[3])
keys = json.loads(sys.argv[4])
ready = sys.argv[5]
os.chdir(cwd)
s = Pty([binary, *args, "main.rs"], 30, 120)
if s.wait_for(lambda out: b"\x1b[?1049h" in out, 10, "first frame"):
    s.settle(0.5)
    for k in keys[:-1]:
        s.send(k.encode())
        s.settle(0.4)
    if ready:
        s.wait_for(lambda out: os.path.exists(ready) and b"hello" in open(ready, "rb").read(), 10, "autosave")
    s.send(keys[-1].encode())
code = s.finish(10)
out = s.out
left = out.rfind(b"\x1b[?1049l")
tail = out[left + 8:] if left >= 0 else b"NOT_LEFT"
print("EXIT", code)
print("TAIL", plain(tail).replace("\n", " ").strip())
"##;
    let out = tiza_qa::pty_script(
        dir,
        name,
        body,
        &[
            tiza_qa::bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &serde_json::to_string(args).unwrap(),
            &serde_json::to_string(keys).unwrap(),
            before_last.unwrap_or(""),
        ],
    );
    assert!(out.contains("EXIT 0"), "{out}");
    out.lines()
        .find_map(|l| l.strip_prefix("TAIL "))
        .unwrap_or("")
        .to_string()
}

#[test]
fn ac2_no_save_and_ctrl_c_writes_nothing_and_says_what_was_not_saved() {
    let (dir, tiza) = project("t195-ctrl-c");
    let before = stamp(&tiza);
    let tail = run(
        &dir,
        "c.py",
        &["--no-save"],
        &[":note 1 hello\r", "\x03"],
        None,
    );
    assert_eq!(
        stamp(&tiza),
        before,
        "the sidecar is untouched (bytes and mtime)"
    );
    assert!(
        tail.contains("tiza: quit without saving main.rs.tiza: 1 change not saved (note a1)"),
        "{tail}"
    );
}

#[test]
fn ac1_no_save_and_q_bang_writes_nothing() {
    let (dir, tiza) = project("t195-q-bang");
    let before = stamp(&tiza);
    let tail = run(
        &dir,
        "q.py",
        &["--no-save"],
        &[":note 1 hello\r", ":q!\r"],
        None,
    );
    assert_eq!(stamp(&tiza), before);
    assert!(tail.contains("1 change not saved (note a1)"), "{tail}");
}

#[test]
fn ac2_autosave_off_live_and_ctrl_c_writes_nothing() {
    let (dir, tiza) = project("t195-autosave-off");
    let before = stamp(&tiza);
    let tail = run(
        &dir,
        "off.py",
        &[],
        &[":autosave off\r", ":note 1 hello\r", "\x03"],
        None,
    );
    assert_eq!(stamp(&tiza), before);
    assert!(tail.contains("1 change not saved (note a1)"), "{tail}");
}

#[test]
fn ac2_with_autosave_on_the_note_is_on_disk_already_and_ctrl_c_adds_nothing() {
    let (dir, tiza) = project("t195-autosave-on");
    let tail = run(
        &dir,
        "on.py",
        &[],
        &[":note 1 hello\r", "\x03"],
        Some(tiza.to_str().unwrap()),
    );
    let text = std::fs::read_to_string(&tiza).unwrap();
    assert!(text.contains("hello"), "the autosave wrote it: {text}");
    assert!(!tail.contains("not saved"), "{tail}");
}

#[test]
fn corner_a_read_only_sidecar_writes_nothing_on_the_way_out() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, tiza) = project("t195-read-only");
    std::fs::set_permissions(&tiza, std::fs::Permissions::from_mode(0o444)).unwrap();
    let before = stamp(&tiza);
    let tail = run(&dir, "ro.py", &[], &[":note 1 hello\r", "\x03"], None);
    assert_eq!(stamp(&tiza), before);
    assert!(tail.contains("1 change not saved (note a1)"), "{tail}");
    std::fs::set_permissions(&tiza, std::fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
fn ac1_the_bar_and_the_help_say_q_bang_quits_without_saving() {
    let dir = scratch("t195-bar");
    let mut doc = tiza_core::Document::from_code(CODE, "rs", None);
    doc.reference_file("main.rs", None);
    let live = tiza_qa::Live::open(doc, dir.join("main.rs.tiza"), dir.join("s.sock"));
    live.command("autosave off");
    live.command("note 1 hello");
    live.command("q");
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains(":q! quits without saving"), "{msg}");
    assert!(!live.with(|app| app.quit));
    live.command("help");
    // The help is longer than the screen: look at every page of it.
    let mut pages = String::new();
    for page in 0..6 {
        live.with(move |app| app.overlay_scroll = page * 40);
        pages.push_str(&live.screen(200, 50));
    }
    assert!(pages.contains(":q! (quits without saving"), "{pages}");
    live.with(|app| app.overlay = tiza_tui::app::Overlay::None);
    // `:wq` saves and quits, as before.
    live.command("wq");
    assert!(live.with(|app| app.quit));
    assert!(
        std::fs::read_to_string(dir.join("main.rs.tiza"))
            .unwrap()
            .contains("hello")
    );
}
