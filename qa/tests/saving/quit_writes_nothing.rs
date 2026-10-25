//! `:q!` and Ctrl-C saved unsaved changes on the way out (also
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
    run_out(dir, name, args, keys, before_last)
        .lines()
        .find_map(|l| l.strip_prefix("TAIL "))
        .unwrap_or("")
        .to_string()
}

/// [`run`], all of the script's output. The key `BAR` (with `--socket
/// s.sock` in `args`) prints `BAR <the bar>` once the bar says `unsaved`.
fn run_out(
    dir: &Path,
    name: &str,
    args: &[&str],
    keys: &[&str],
    before_last: Option<&str>,
) -> String {
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
    mark = 0
    for k in keys[:-1]:
        if k == "BAR":
            end = time.monotonic() + 10
            said = None
            while time.monotonic() < end:
                r = subprocess.run([binary, "--socket", "s.sock", "api", "follow.status", "{}"],
                                   capture_output=True, text=True, timeout=10)
                said = json.loads(r.stdout).get("bar") if r.returncode == 0 else None
                if said and said.startswith("unsaved"):
                    break
                time.sleep(0.05)
            print("BAR", json.dumps(said), flush=True)
            continue
        if k == "CHMOD644":
            # The key before failed to autosave over the read-only file:
            # only then is it writable again (a condition, not the settle).
            s.wait_for(lambda out: "could not save" in plain(out[mark:]), 10, "the failed autosave")
            os.chmod("main.rs.tiza", 0o644)
            continue
        mark = len(s.out)
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
    out
}

#[test]
fn no_save_and_ctrl_c_writes_nothing_and_says_what_was_not_saved() {
    let (dir, tiza) = project("ctrl-c");
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
fn no_save_and_q_bang_writes_nothing() {
    let (dir, tiza) = project("q-bang");
    let before = stamp(&tiza);
    // `:q` with changes is refused and names both ways out (moved here from
    // the --no-save tests: the binary's `--no-save`, not a flag set in the test).
    let out = run_out(
        &dir,
        "q.py",
        &["--no-save", "--socket", "s.sock"],
        &[":note 1 hello\r", ":q\r", "BAR", ":q!\r"],
        None,
    );
    assert!(
        out.contains(
            r#"BAR "unsaved changes (--no-save): :saveas PATH saves them elsewhere, :q! discards them""#
        ),
        "{out}"
    );
    let tail = out
        .lines()
        .find_map(|l| l.strip_prefix("TAIL "))
        .unwrap_or("");
    assert_eq!(stamp(&tiza), before);
    assert!(tail.contains("1 change not saved (note a1)"), "{tail}");
}

#[test]
fn with_autosave_on_the_note_is_on_disk_already_and_ctrl_c_adds_nothing() {
    let (dir, tiza) = project("autosave-on");
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
fn a_read_only_sidecar_writes_nothing_on_the_way_out() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, tiza) = project("read-only");
    std::fs::set_permissions(&tiza, std::fs::Permissions::from_mode(0o444)).unwrap();
    let before = stamp(&tiza);
    let tail = run(&dir, "ro.py", &[], &[":note 1 hello\r", "\x03"], None);
    assert_eq!(stamp(&tiza), before);
    // Ctrl-C tried the failed autosave once more; it failed again.
    assert!(tail.contains("tiza: could not save "), "{tail}");
    assert!(tail.contains("1 change not saved (note a1)"), "{tail}");
    std::fs::set_permissions(&tiza, std::fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
fn ctrl_c_retries_a_failed_autosave_once_and_q_bang_never_writes() {
    use std::os::unix::fs::PermissionsExt;
    // Writable again by the time of the Ctrl-C: the note gets to disk. A
    // Ctrl-C that first closes a command line does not use up that one
    // retry (moved here from the Ctrl-C tests): the Ctrl-C that quits still
    // saves. That this first Ctrl-C closes the line and does not quit is
    // checked there (`alive`).
    let (dir, tiza) = project("retry");
    std::fs::set_permissions(&tiza, std::fs::Permissions::from_mode(0o444)).unwrap();
    let tail = run(
        &dir,
        "retry.py",
        &[],
        &[":note 1 hello\r", "CHMOD644", ":", "\x03", "\x03"],
        None,
    );
    assert!(std::fs::read_to_string(&tiza).unwrap().contains("hello"));
    assert!(
        !tail.contains("not saved") && !tail.contains("could not"),
        "{tail}"
    );
    // The same with `:q!`: it quits without saving, whatever the mode.
    let (dir, tiza) = project("retry-q");
    std::fs::set_permissions(&tiza, std::fs::Permissions::from_mode(0o444)).unwrap();
    let before = stamp(&tiza);
    let tail = run(
        &dir,
        "retry-q.py",
        &[],
        &[":note 1 hello\r", "CHMOD644", ":q!\r"],
        None,
    );
    assert_eq!(stamp(&tiza).0, before.0, "nothing written");
    assert!(tail.contains("1 change not saved (note a1)"), "{tail}");
}

/// A live instance on `main.rs` with autosave off.
fn unsaved(name: &str) -> (tiza_qa::Live, PathBuf) {
    let (dir, tiza) = project(name);
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = tiza_qa::Live::open(doc, tiza, dir.join("s.sock"));
    live.command("autosave off");
    (live, dir)
}

fn summary(live: &tiza_qa::Live) -> Option<String> {
    live.with(|app| app.unsaved_summary())
}

#[test]
fn the_summary_compares_with_the_sidecar_last_written() {
    // a, :w, b, u: the document is the one on disk.
    let (live, _dir) = unsaved("same");
    live.command("note 1 a");
    live.command("w");
    live.command("note 2 b");
    live.command("undo");
    assert_eq!(summary(&live), None);
    // a, b, :w, u, u: both notes undone since the save, named.
    let (live, _dir) = unsaved("undone");
    live.command("note 1 a");
    live.command("note 2 b");
    live.command("w");
    live.command("undo");
    live.command("undo");
    assert_eq!(
        summary(&live).unwrap(),
        format!(
            "quit without saving {}: 2 changes not saved (note a2 undone, note a1 undone)",
            live.with(|app| app.save_path.display().to_string())
        )
    );
}

#[test]
fn a_reload_of_code_read_by_reference_leaves_nothing_to_save() {
    let (live, dir) = unsaved("follow");
    live.command("follow on");
    std::fs::write(dir.join("main.rs"), CODE.replace("1", "2")).unwrap();
    let start = std::time::Instant::now();
    while live.with(|app| app.doc.code.join("\n")).contains("= 1") {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "no reload"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(summary(&live), None);
}

#[test]
fn the_bar_and_the_help_say_q_bang_quits_without_saving() {
    let dir = scratch("bar");
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
    // The help is longer than the screen: the filter finds the row.
    live.with(|app| app.overlay_filter = ":q!".into());
    let s = live.screen(200, 50);
    assert!(
        s.lines()
            .any(|l| l.contains(":q!") && l.contains("Quit without saving.")),
        "{s}"
    );
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
