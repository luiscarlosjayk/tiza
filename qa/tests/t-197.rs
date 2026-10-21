//! t-197 — Ctrl-C cancels where there is something to cancel (command
//! line, selection, overlays) and quits everywhere else.
//!
//! It only quit from normal and presentation mode: in the command line it
//! typed a `c`, in a selection it did nothing, over an overlay it did not
//! quit. Rule (Luis, 2026-10-20, as in vim): Ctrl-C cancels what is open,
//! like Esc, and quits (as `:q!`) when nothing is.
//!
//! Acceptance criteria:
//! - AC1 Command line open (`:` typed, with or without text): Ctrl-C closes
//!   it like Esc, runs nothing and never inserts `c`; the next Ctrl-C quits.
//! - AC2 Selection mode (`v`, `V`, an arrow being drawn): Ctrl-C leaves it
//!   like Esc (no annotation created); the next Ctrl-C quits.
//! - AC3 An overlay (help `?`, list `L`, review): Ctrl-C closes it like
//!   Esc; the next Ctrl-C quits.
//! - AC4 Everywhere else (normal mode, presentation mode, the live pointer
//!   `f`, a typing animation in progress, a Tab-selected annotation) Ctrl-C
//!   quits exactly as `:q!` (t-195): nothing written beyond what the mode
//!   already wrote, the stderr summaries (stale moves, unsaved changes), and
//!   the retry of a failed autosave.
//! - AC5 `:q!` typed in the command line is unchanged; Ctrl-U still clears
//!   the command line.
//! - AC6 This file (the real binary in a pty, one case per AC); README keys
//!   table and help («Ctrl-C cancels, or quits»); CHANGELOG (Fixed).
//!
//! Corner cases: text typed with the cursor moved is discarded whole;
//! `:` in presentation mode closes and leaves the frame clean; an OSC answer
//! arriving as key presses (t-119) is still swallowed before Ctrl-C is
//! looked at; two Ctrl-C sent at once cancel, then quit; Ctrl-C over a
//! follow reload notice or a stale notice quits.
//!
//! Each case runs `tiza` on a pty with a socket (a relative path, short
//! enough for any temporary directory) and checks the state between two
//! Ctrl-C with the API and with a key whose effect tells the modes apart.
//! When python3 has `pyte`, the screen itself is checked too.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::scratch;

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

const CTRL_C: &str = "\x03";
/// An OSC 11 answer, as the terminal sends it (t-119).
const OSC_ANSWER: &str = "\x1b]11;rgb:f7f7/f5f5/efef\x1b\\";

/// `main.rs` with a sidecar, as Tiza itself wrote it.
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

/// What one run in the pty saw.
#[derive(Debug, Default)]
struct Run {
    /// One entry per `alive` step: the child had not exited.
    alive: Vec<bool>,
    /// One entry per `api` step: the reply (`null` if the call failed).
    api: Vec<Value>,
    /// One entry per `screen` step: the rows, or `None` without pyte.
    screens: Vec<Option<Vec<String>>>,
    exit: Option<i64>,
    /// What the terminal shows after Tiza left the alternate screen: its
    /// stderr on the way out.
    tail: String,
    out: String,
}

impl Run {
    fn quit_cleanly(&self) {
        assert_eq!(
            self.exit,
            Some(0),
            "Tiza did not quit by itself:\n{}",
            self.out
        );
        assert!(!self.out.contains("WAIT_TIMEOUT"), "{}", self.out);
    }

    fn api(&self, i: usize) -> &Value {
        &self.api[i]
    }

    /// The last row of screen `i` (the bar), when pyte is there.
    fn bar(&self, i: usize) -> Option<String> {
        self.screens[i]
            .as_ref()
            .map(|rows| rows.last().cloned().unwrap_or_default())
    }
}

/// Runs `tiza ARGS FILE` on a 120×30 pty in `dir`, with `--socket s.sock`,
/// and plays `steps`:
/// - `["k", text]` sends the keys and lets the screen settle;
/// - `["alive"]` settles and records whether the child is still running;
/// - `["api", method, params]` records the reply of the local API;
/// - `["bg_api", method, params]` calls it without waiting (a proposal
///   waits for the presenter);
/// - `["mark"]` / `["wait", text]` wait for `text` in what was drawn since
///   the mark;
/// - `["save", file, content]` saves a file as an editor does;
/// - `["chmod", file, mode]`;
/// - `["screen"]` records the screen (pyte), or `None`.
///
/// The last step is expected to make Tiza quit; a child still running
/// 10 s later is killed and the run says so (`exit` is not 0).
fn run(dir: &Path, name: &str, args: &[&str], file: &str, steps: Value) -> Run {
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
args = json.loads(sys.argv[3])
file = sys.argv[4]
steps = json.loads(sys.argv[5])
os.chdir(cwd)
try:
    import pyte
except ImportError:
    pyte = None
def api(method, params):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params)],
                       capture_output=True, text=True, timeout=10)
    if r.returncode != 0:
        print("API_ERR", method, r.stderr.strip(), flush=True)
        return None
    return json.loads(r.stdout)
bg = []
s = Pty([binary, *args, "--socket", "s.sock", file], 30, 120)
mark = 0
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    s.settle(0.5)
    for step in steps:
        op = step[0]
        if op == "k":
            s.send(step[1].encode())
            s.settle(0.3)
        elif op == "alive":
            s.settle(0.5)
            print("ALIVE", 1 if s.p.poll() is None else 0, flush=True)
        elif op == "api":
            print("API", json.dumps(api(step[1], step[2])), flush=True)
        elif op == "bg_api":
            bg.append(subprocess.Popen([binary, "--socket", "s.sock", "api", step[1], json.dumps(step[2])],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        elif op == "mark":
            mark = len(s.out)
        elif op == "wait":
            want = step[1]
            s.wait_for(lambda out: want in plain(out[mark:]), 10, want)
        elif op == "save":
            tmp = step[1] + ".tmp"
            with open(tmp, "w") as f:
                f.write(step[2])
            os.rename(tmp, step[1])
        elif op == "chmod":
            os.chmod(step[1], step[2])
        elif op == "screen":
            if pyte is None:
                print("SCREEN null", flush=True)
            else:
                sc = pyte.Screen(120, 30)
                pyte.ByteStream(sc).feed(s.out)
                print("SCREEN", json.dumps(sc.display), flush=True)
code = s.finish(10)
for p in bg:
    try:
        p.wait(timeout=10)
    except subprocess.TimeoutExpired:
        p.kill()
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
            file,
            &steps.to_string(),
        ],
    );
    let mut r = Run {
        out: out.clone(),
        ..Run::default()
    };
    for line in out.lines() {
        if let Some(v) = line.strip_prefix("ALIVE ") {
            r.alive.push(v == "1");
        } else if let Some(v) = line.strip_prefix("API ") {
            r.api.push(serde_json::from_str(v).unwrap());
        } else if let Some(v) = line.strip_prefix("SCREEN ") {
            r.screens.push(serde_json::from_str(v).unwrap());
        } else if let Some(v) = line.strip_prefix("EXIT ") {
            r.exit = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("TAIL ") {
            r.tail = v.to_string();
        }
    }
    r
}

/// The annotations of the open document, from `scene.get`.
fn annotations(scene: &Value) -> Vec<Value> {
    scene["document"]["annotations"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn pointer(status: &Value) -> Option<(u64, u64)> {
    let p = &status["pointer"];
    p.is_object()
        .then(|| (p["line"].as_u64().unwrap(), p["to"].as_u64().unwrap()))
}

// ---------------------------------------------------------------- AC1

#[test]
fn ac1_ctrl_c_closes_the_command_line_runs_nothing_and_the_next_quits() {
    // Test step 1: `tiza --no-save main.rs`, `:note 1 hel`, Ctrl-C.
    let (dir, tiza) = project("t197-ac1");
    let before = std::fs::read(&tiza).unwrap();
    let r = run(
        &dir,
        "ac1.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 hel"],
            ["k", CTRL_C],
            ["alive"],
            ["api", "status", {}],
            ["screen"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true], "the first Ctrl-C only closes the line");
    assert_eq!(r.api(0)["annotations"], 0, "the line did not run");
    if let Some(bar) = r.bar(0) {
        assert!(bar.contains("NORMAL"), "{bar}");
        assert!(!bar.contains(":note"), "nothing typed is left: {bar}");
    }
    assert!(!r.tail.contains("not saved"), "nothing unsaved: {}", r.tail);
    assert_eq!(std::fs::read(&tiza).unwrap(), before);
}

#[test]
fn ac1_ctrl_c_on_an_empty_command_line_closes_it() {
    let (dir, _) = project("t197-ac1-empty");
    let r = run(
        &dir,
        "ac1e.py",
        &["--no-save"],
        "main.rs",
        // `f` turns the pointer on only in normal mode: in the command
        // line it would be text.
        json!([
            ["k", ":"],
            ["k", CTRL_C],
            ["alive"],
            ["k", "f"],
            ["api", "status", {}],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true]);
    assert_eq!(pointer(r.api(0)), Some((1, 1)), "{}", r.api(0));
}

#[test]
fn ac1_corner_the_whole_line_is_dropped_and_no_c_is_typed() {
    // Text typed, the cursor keys pressed in the middle of it: Ctrl-C drops
    // the line whole. A command typed after it runs alone, so nothing of
    // the dropped line (nor a `c`) was left behind.
    let (dir, _) = project("t197-ac1-middle");
    let r = run(
        &dir,
        "ac1m.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 hello"],
            ["k", "\x1b[D\x1b[D"],
            ["k", CTRL_C],
            ["alive"],
            ["k", ":note 2 x\r"],
            ["api", "scene.get", {}],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true]);
    let anns = annotations(r.api(0));
    assert_eq!(anns.len(), 1, "{anns:?}");
    assert_eq!(anns[0]["line"], 2, "{anns:?}");
    assert_eq!(anns[0]["text"], "x", "{anns:?}");
    assert!(
        r.tail.contains("1 change not saved (note a1)"),
        "{}",
        r.tail
    );
}

// ---------------------------------------------------------------- AC2

#[test]
fn ac2_ctrl_c_leaves_a_selection_without_an_annotation_and_the_next_quits() {
    // Test step 2: `v`, `j`, Ctrl-C → NORMAL, no annotation; Ctrl-C quits.
    for (linewise, key) in [(false, "v"), (true, "V")] {
        let (dir, tiza) = project(&format!("t197-ac2-{linewise}"));
        let before = std::fs::read(&tiza).unwrap();
        let r = run(
            &dir,
            &format!("ac2-{linewise}.py"),
            &["--no-save"],
            "main.rs",
            // `f` lights the cursor line alone in normal mode, the two
            // selected lines in selection mode.
            json!([
                ["k", key],
                ["k", "j"],
                ["k", CTRL_C],
                ["alive"],
                ["screen"],
                ["k", "f"],
                ["api", "status", {}],
                ["k", CTRL_C],
            ]),
        );
        r.quit_cleanly();
        assert_eq!(r.alive, [true], "{key}");
        assert_eq!(pointer(r.api(0)), Some((2, 2)), "{key}: {}", r.api(0));
        assert_eq!(r.api(0)["annotations"], 0);
        if let Some(bar) = r.bar(0) {
            assert!(bar.contains("NORMAL"), "{key}: {bar}");
        }
        assert!(!r.tail.contains("not saved"), "{}", r.tail);
        assert_eq!(std::fs::read(&tiza).unwrap(), before);
    }
}

#[test]
fn ac2_ctrl_c_cancels_an_arrow_being_drawn() {
    // `v a` chooses the source; Ctrl-C forgets it, so `v a` again chooses
    // a source instead of drawing the arrow. With the target selected
    // (`v a v`), one Ctrl-C leaves the selection and the arrow, like Esc.
    let (dir, _) = project("t197-ac2-arrow");
    let r = run(
        &dir,
        "arrow.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", "v"],
            ["k", "a"],
            ["k", CTRL_C],
            ["alive"],
            ["screen"],
            ["k", "v"],
            ["k", "a"],
            ["api", "status", {}],
            ["k", CTRL_C],
            ["alive"],
            ["k", "v"],
            ["k", "a"],
            ["k", "j"],
            ["k", "v"],
            ["k", CTRL_C],
            ["alive"],
            ["k", "v"],
            ["k", "a"],
            ["api", "status", {}],
            ["k", CTRL_C],
            ["alive"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true, true, true, true]);
    assert_eq!(r.api(0)["annotations"], 0, "no arrow drawn");
    assert_eq!(r.api(1)["annotations"], 0, "no arrow drawn");
    if let Some(bar) = r.bar(0) {
        assert!(bar.contains("NORMAL") && !bar.contains("ARROW"), "{bar}");
    }
    assert!(!r.tail.contains("not saved"), "{}", r.tail);
}

// ---------------------------------------------------------------- AC3

#[test]
fn ac3_ctrl_c_closes_the_help_and_the_list_and_the_next_quits() {
    // Test step 3: `?`, Ctrl-C → help closes; Ctrl-C quits. Same with `L`.
    for key in ["?", "L"] {
        let (dir, _) = project(&format!("t197-ac3-{}", key == "?"));
        let r = run(
            &dir,
            &format!("ac3-{}.py", key == "?"),
            &["--no-save"],
            "main.rs",
            // An overlay takes `f` for itself; without it the pointer
            // turns on.
            json!([
                ["k", key],
                ["k", CTRL_C],
                ["alive"],
                ["screen"],
                ["k", "f"],
                ["api", "status", {}],
                ["k", CTRL_C],
            ]),
        );
        r.quit_cleanly();
        assert_eq!(r.alive, [true], "{key}");
        assert_eq!(pointer(r.api(0)), Some((1, 1)), "{key}: {}", r.api(0));
        if let Some(rows) = &r.screens[0] {
            assert!(
                !rows.iter().any(|l| l.contains("Esc closes")),
                "{key}: the overlay is gone: {rows:#?}"
            );
        }
    }
}

#[test]
fn ac3_ctrl_c_closes_the_review_and_the_proposal_stays_pending() {
    let (dir, _) = project("t197-ac3-review");
    let r = run(
        &dir,
        "review.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":supervise on\r"],
            ["mark"],
            ["bg_api", "annotation.add", {"type": "note", "line": 1, "text": "agent"}],
            ["wait", "waiting for review"],
            ["k", CTRL_C],
            ["alive"],
            ["k", "f"],
            ["api", "status", {}],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true]);
    assert_eq!(r.api(0)["pending"], 1, "still pending: {}", r.api(0));
    assert_eq!(r.api(0)["annotations"], 0, "{}", r.api(0));
    assert_eq!(pointer(r.api(0)), Some((1, 1)), "the review is closed");
}

// ---------------------------------------------------------------- AC4

#[test]
fn ac4_ctrl_c_quits_from_normal_and_presentation_mode_as_q_bang() {
    // Test step 4: `P`, Ctrl-C → quits; under --no-save stderr names the
    // note (t-195), and the sidecar is untouched.
    let (dir, tiza) = project("t197-ac4-present");
    let before = std::fs::read(&tiza).unwrap();
    let r = run(
        &dir,
        "present.py",
        &["--no-save"],
        "main.rs",
        json!([["k", ":note 1 hello\r"], ["k", "P"], ["k", CTRL_C]]),
    );
    r.quit_cleanly();
    assert!(
        r.tail
            .contains("tiza: quit without saving main.rs.tiza: 1 change not saved (note a1)"),
        "{}",
        r.tail
    );
    assert_eq!(std::fs::read(&tiza).unwrap(), before);
    // Normal mode, nothing changed: quits, says nothing.
    let (dir, _) = project("t197-ac4-normal");
    let r = run(
        &dir,
        "normal.py",
        &["--no-save"],
        "main.rs",
        json!([["k", CTRL_C]]),
    );
    r.quit_cleanly();
    assert_eq!(r.tail, "");
}

#[test]
fn ac4_corner_the_command_line_in_presentation_mode_closes_and_leaves_the_frame_clean() {
    let (dir, _) = project("t197-ac4-present-cmd");
    let r = run(
        &dir,
        "present-cmd.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", "P"],
            ["k", ":note 1 x"],
            ["k", CTRL_C],
            ["alive"],
            ["api", "status", {}],
            ["screen"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true]);
    assert_eq!(r.api(0)["annotations"], 0);
    if let Some(rows) = &r.screens[0] {
        let text = rows.join("\n");
        assert!(
            !text.contains(":note") && !text.contains("NORMAL"),
            "{text}"
        );
        assert!(text.contains("let a = 1;"), "{text}");
    }
    assert!(!r.tail.contains("not saved"), "{}", r.tail);
}

#[test]
fn ac4_ctrl_c_quits_with_the_pointer_on_or_an_annotation_selected() {
    let (dir, _) = project("t197-ac4-pointer");
    let r = run(
        &dir,
        "pointer.py",
        &["--no-save"],
        "main.rs",
        json!([["k", "f"], ["api", "status", {}], ["k", CTRL_C]]),
    );
    r.quit_cleanly();
    assert_eq!(pointer(r.api(0)), Some((1, 1)));
    let (dir, _) = project("t197-ac4-tab");
    let r = run(
        &dir,
        "tab.py",
        &["--no-save"],
        "main.rs",
        json!([["k", ":note 1 hello\r"], ["k", "\t"], ["k", CTRL_C]]),
    );
    r.quit_cleanly();
    assert!(
        r.tail.contains("1 change not saved (note a1)"),
        "{}",
        r.tail
    );
}

#[test]
fn ac4_ctrl_c_quits_while_a_step_types() {
    // Test step 5: a `typing` move, `n` to its step, Ctrl-C while it types
    // (three lines at 3 s each) → quits, instead of only ending the
    // animation.
    let dir = scratch("t197-ac4-typing");
    let doc = json!({
        "schema": 1,
        "source": {"language": "rs"},
        "code": CODE.lines().collect::<Vec<_>>(),
        "step": 0,
        "annotations": [{"id": "a1", "step": 1, "type": "note", "line": 2, "text": "typed"}],
        "moves": [{
            "at_step": 1, "action": "code.patch", "op": "insert", "after": "let b = 2;",
            "content": "    let c = 3;\n    let d = 4;\n    let e = 5;",
            "typing": "line", "speed_ms": 3000
        }],
    });
    let path = dir.join("typed.tiza");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let before = std::fs::read(&path).unwrap();
    let r = run(
        &dir,
        "typing.py",
        &["--no-save"],
        "typed.tiza",
        json!([["k", "n"], ["api", "code.get", {}], ["k", CTRL_C]]),
    );
    r.quit_cleanly();
    let text = r.api(0)["text"].as_str().unwrap();
    assert!(!text.contains("let e = 5;"), "still typing: {text}");
    assert!(!r.tail.contains("could not"), "{}", r.tail);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

const INI: &str = "[server]\nhost = localhost\nport = 8080\n[db]\nuser = admin\n";

/// `config.ini` and its sidecar with a `replace position "2"` at step 2.
fn followed(name: &str) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(dir.join("config.ini"), INI).unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"language": "ini"},
        "code": {"file": "config.ini"},
        "step": 0,
        "moves": [{"at_step": 2, "action": "code.patch", "op": "replace", "position": "2", "content": "host = example.com"}],
        "annotations": [],
    });
    std::fs::write(
        dir.join("config.ini.tiza"),
        serde_json::to_string_pretty(&doc).unwrap(),
    )
    .unwrap();
    dir
}

#[test]
fn ac4_corner_ctrl_c_over_a_follow_or_stale_notice_quits() {
    // A reload notice on the bar is not something to cancel.
    let dir = followed("t197-ac4-reload");
    let r = run(
        &dir,
        "reload.py",
        &["--no-save", "--follow"],
        "config.ini.tiza",
        json!([
            ["mark"],
            ["save", "config.ini", format!("# payments\n{INI}")],
            ["wait", "reloaded (#1)"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    // Nor a stale notice; stderr gives the summary of stale moves (t-164).
    let dir = followed("t197-ac4-stale");
    let before = std::fs::read(dir.join("config.ini.tiza")).unwrap();
    let r = run(
        &dir,
        "stale.py",
        &["--no-save", "--follow"],
        "config.ini.tiza",
        json!([
            ["mark"],
            [
                "save",
                "config.ini",
                INI.replace("host = localhost", "host = db1")
            ],
            ["wait", "stale move skipped"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert!(
        r.tail
            .contains("tiza: 1 stale move (#1): next time it applies on line 2 as written"),
        "{}",
        r.tail
    );
    assert_eq!(std::fs::read(dir.join("config.ini.tiza")).unwrap(), before);
}

#[test]
fn ac4_a_cancel_first_still_leaves_the_retry_of_a_failed_autosave_to_the_quit() {
    use std::os::unix::fs::PermissionsExt;
    // Autosave on, the sidecar read-only (the autosave fails), writable
    // again later: Ctrl-C closes the command line (nothing written), and
    // the Ctrl-C that quits tries the save once more (t-195).
    let (dir, tiza) = project("t197-ac4-retry");
    std::fs::set_permissions(&tiza, std::fs::Permissions::from_mode(0o444)).unwrap();
    let r = run(
        &dir,
        "retry.py",
        &[],
        "main.rs",
        json!([
            ["k", ":note 1 hello\r"],
            ["chmod", "main.rs.tiza", 0o644],
            ["k", ":"],
            ["k", CTRL_C],
            ["alive"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true]);
    assert!(std::fs::read_to_string(&tiza).unwrap().contains("hello"));
    assert!(
        !r.tail.contains("not saved") && !r.tail.contains("could not"),
        "{}",
        r.tail
    );
}

// ---------------------------------------------------------------- AC5

#[test]
fn ac5_q_bang_and_ctrl_u_are_unchanged() {
    let (dir, tiza) = project("t197-ac5");
    let before = std::fs::read(&tiza).unwrap();
    let r = run(
        &dir,
        "ac5.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 hel"],
            ["k", "\x15"],
            ["k", "note 2 x\r"],
            ["api", "scene.get", {}],
            ["k", ":q!\r"],
        ]),
    );
    r.quit_cleanly();
    let anns = annotations(r.api(0));
    assert_eq!(anns.len(), 1, "{anns:?}");
    assert_eq!(
        (&anns[0]["line"], &anns[0]["text"]),
        (&json!(2), &json!("x"))
    );
    assert!(
        r.tail.contains("1 change not saved (note a1)"),
        "{}",
        r.tail
    );
    assert_eq!(std::fs::read(&tiza).unwrap(), before);
}

// ---------------------------------------------------------------- corners

#[test]
fn corner_an_osc_answer_is_swallowed_before_ctrl_c_is_looked_at() {
    // The answer arrives while the command line is open: none of it is
    // typed (the note says `a`), and a Ctrl-C right after it still closes
    // the line (the second note never runs) before the next one quits.
    let (dir, _) = project("t197-osc");
    let r = run(
        &dir,
        "osc.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 a"],
            ["k", OSC_ANSWER],
            ["k", "\r"],
            ["k", ":note 2 b"],
            ["k", format!("{OSC_ANSWER}{CTRL_C}")],
            ["alive"],
            ["api", "scene.get", {}],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true]);
    let anns = annotations(r.api(0));
    assert_eq!(anns.len(), 1, "{anns:?}");
    assert_eq!(anns[0]["text"], "a", "{anns:?}");
}

#[test]
fn corner_two_ctrl_c_sent_at_once_cancel_then_quit() {
    let (dir, _) = project("t197-two");
    let r = run(
        &dir,
        "two.py",
        &["--no-save"],
        "main.rs",
        json!([["k", ":note 1 hel"], ["k", format!("{CTRL_C}{CTRL_C}")]]),
    );
    r.quit_cleanly();
    assert!(!r.tail.contains("not saved"), "nothing ran: {}", r.tail);
}

// ---------------------------------------------------------------- AC6

#[test]
fn ac6_the_readme_and_the_help_say_ctrl_c_cancels_or_quits() {
    let readme = include_str!("../../README.md");
    assert!(
        readme.contains("| `Ctrl-C` | cancels, or quits"),
        "the keys table has a Ctrl-C row"
    );
    let dir = scratch("t197-help");
    let doc = tiza_core::Document::from_code(CODE, "rs", None);
    let live = tiza_qa::Live::open(doc, dir.join("main.rs.tiza"), dir.join("s.sock"));
    live.command("help");
    let mut pages = String::new();
    for page in 0..6 {
        live.with(move |app| app.overlay_scroll = page * 40);
        pages.push_str(&live.screen(200, 50));
    }
    assert!(pages.contains("Ctrl-C   cancels, or quits"), "{pages}");
}
