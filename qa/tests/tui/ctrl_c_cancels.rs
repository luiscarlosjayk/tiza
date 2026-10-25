//! Ctrl-C cancels where there is something to cancel (command
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
//!   quits exactly as `:q!`: nothing written beyond what the mode
//!   already wrote, the stderr summaries (stale moves, unsaved changes), and
//!   the retry of a failed autosave.
//! - AC5 `:q!` typed in the command line is unchanged; Ctrl-U still clears
//!   the command line.
//! - AC6 This file (the real binary in a pty, one case per AC); README keys
//!   table and help («Ctrl-C cancels, or quits»); CHANGELOG (Fixed).
//!
//! Corner cases: text typed with the cursor moved is discarded whole;
//! `:` in presentation mode closes and leaves the frame clean; an OSC answer
//! arriving as key presses is still swallowed before Ctrl-C is
//! looked at; two Ctrl-C sent at once cancel, then quit; Ctrl-C over a
//! follow reload notice or a stale notice quits.
//!
//! Review: leaving a selection with Ctrl-C clears the bar's hint, as Esc
//! does; cancelling a command line that a selection opened (Esc, Ctrl-C, or
//! Backspace on the empty line) forgets that selection, so a later command
//! typed without an anchor does not land on it.
//!
//! Each case runs `tiza` on a pty with a socket (a relative path, short
//! enough for any temporary directory). After each key it waits, with a
//! deadline, for what that key changes: the screen (rebuilt from what Tiza
//! wrote, see `screen` in the script) or a field of `status`. Between two
//! Ctrl-C it checks the state with the API and with a key whose effect
//! tells the modes apart.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::scratch;

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

const CTRL_C: &str = "\x03";
/// An OSC 11 answer, as the terminal sends it.
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
    /// One entry per `screen` step: its rows.
    screens: Vec<Vec<String>>,
    exit: Option<i64>,
    /// What the terminal shows after Tiza left the alternate screen: its
    /// stderr on the way out.
    tail: String,
    out: String,
}

impl Run {
    fn quit_cleanly(&self) {
        assert!(!self.out.contains("WAIT_TIMEOUT"), "{}", self.out);
        assert_eq!(
            self.exit,
            Some(0),
            "Tiza did not quit by itself:\n{}",
            self.out
        );
    }

    fn api(&self, i: usize) -> &Value {
        &self.api[i]
    }
}

/// Runs `tiza ARGS FILE` on a 120×30 pty in `dir`, with `--socket s.sock`,
/// and plays `steps`:
/// - `["k", text, conds]` sends the keys, then waits for `conds` (below);
/// - `["wait", conds]` waits for `conds`;
/// - `["alive"]` records whether the child is still running (a short
///   settle first: it checks that a key did not make Tiza quit);
/// - `["api", method, params]` records the reply of the local API;
/// - `["bg_api", method, params]` calls it without waiting (a proposal
///   waits for the presenter);
/// - `["save", file, content]` saves a file as an editor does;
/// - `["chmod", file, mode]`;
/// - `["screen"]` records the screen.
///
/// A condition is `["bar", text]` / `["nobar", text]` (the last row has, or
/// lacks, `text`), `["has", text]` / `["lacks", text]` (anywhere on the
/// screen) or `["status", field, value]` (`status` gives that value). Each
/// is chosen false before its key, so it holds only once the key was
/// handled; one that never holds prints `WAIT_TIMEOUT`.
///
/// The last step is expected to make Tiza quit; a child still running
/// 10 s later is killed and the run says so (`exit` is not 0).
fn run(dir: &Path, name: &str, args: &[&str], file: &str, steps: Value) -> Run {
    let body = r##"
import json, unicodedata
binary, cwd = sys.argv[1], sys.argv[2]
args = json.loads(sys.argv[3])
file = sys.argv[4]
steps = json.loads(sys.argv[5])
os.chdir(cwd)
ROWS, COLS = 30, 120
SEQ = re.compile(r"\x1b\[([0-9;?<>=]*)([ -/]*)([@-~])|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[P^_][^\x1b]*\x1b\\|\x1b.|([^\x1b])", re.S)

def screen(raw):
    """The rows a terminal shows after `raw`: the cursor moves, erases and
    text that ratatui writes through crossterm; styles, modes and OSC
    strings change no cell."""
    grid = [[" "] * COLS for _ in range(ROWS)]
    r = c = 0
    for m in SEQ.finditer(raw.decode(errors="replace")):
        params, final, ch = m.group(1), m.group(3), m.group(4)
        if ch is not None:
            if ch == "\r":
                c = 0
            elif ch == "\n":
                r = min(r + 1, ROWS - 1)
            elif ch >= " ":
                w = 0 if unicodedata.combining(ch) else (2 if unicodedata.east_asian_width(ch) in "WF" else 1)
                if w and c < COLS:
                    grid[r][c] = ch
                    if w == 2 and c + 1 < COLS:
                        grid[r][c + 1] = ""
                c += w
        elif final is not None and not params.startswith("?"):
            n = [int(x) if x else 0 for x in params.split(";")] if params else []
            first = n[0] if n else 0
            if final in "Hf":
                r = min(max((n[0] if n else 1) - 1, 0), ROWS - 1)
                c = max((n[1] if len(n) > 1 else 1) - 1, 0)
            elif final == "J" and first == 2:
                grid = [[" "] * COLS for _ in range(ROWS)]
            elif final == "K":
                lo, hi = {1: (0, c + 1), 2: (0, COLS)}.get(first, (c, COLS))
                for x in range(lo, min(hi, COLS)):
                    grid[r][x] = " "
    return ["".join(row).rstrip() for row in grid]

def api(method, params):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params)],
                       capture_output=True, text=True, timeout=10)
    if r.returncode != 0:
        print("API_ERR", method, r.stderr.strip(), flush=True)
        return None
    return json.loads(r.stdout)

def holds(cond):
    kind = cond[0]
    if kind == "status":
        st = api("status", {})
        return st is not None and st.get(cond[1]) == cond[2]
    rows = screen(s.out)
    text = "\n".join(rows)
    return {"bar": lambda: cond[1] in rows[-1], "nobar": lambda: cond[1] not in rows[-1],
            "has": lambda: cond[1] in text, "lacks": lambda: cond[1] not in text}[kind]()

def wait(conds, what):
    end = time.monotonic() + 10
    while not all(holds(c) for c in conds):
        if time.monotonic() > end or s.p.poll() is not None:
            print("WAIT_TIMEOUT", json.dumps(what), json.dumps(conds), "bar:", screen(s.out)[-1], flush=True)
            return False
        s.settle(0.05)
    return True

bg = []
s = Pty([binary, *args, "--socket", "s.sock", file], ROWS, COLS)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame") \
        and wait([["bar", " ann."]], "first frame"):
    for step in steps:
        op = step[0]
        if op == "k":
            s.send(step[1].encode())
            if len(step) > 2 and not wait(step[2], step[1]):
                break
        elif op == "wait":
            if not wait(step[1], "wait"):
                break
        elif op == "alive":
            s.settle(0.3)
            print("ALIVE", 1 if s.p.poll() is None else 0, flush=True)
        elif op == "api":
            print("API", json.dumps(api(step[1], step[2])), flush=True)
        elif op == "bg_api":
            bg.append(subprocess.Popen([binary, "--socket", "s.sock", "api", step[1], json.dumps(step[2])],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
        elif op == "save":
            tmp = step[1] + ".tmp"
            with open(tmp, "w") as f:
                f.write(step[2])
            os.rename(tmp, step[1])
        elif op == "chmod":
            os.chmod(step[1], step[2])
        elif op == "screen":
            print("SCREEN", json.dumps(screen(s.out)), flush=True)
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
fn ctrl_c_closes_the_command_line_runs_nothing_and_the_next_quits() {
    // Test step 1: `tiza --no-save main.rs`, `:note 1 hel`, Ctrl-C.
    let (dir, tiza) = project("ac1");
    let before = std::fs::read(&tiza).unwrap();
    let r = run(
        &dir,
        "ac1.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 hel", [["bar", ":note 1 hel"]]],
            ["k", CTRL_C, [["bar", "NORMAL"]]],
            ["alive"],
            ["api", "status", {}],
            ["screen"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true], "the first Ctrl-C only closes the line");
    assert_eq!(r.api(0)["annotations"], 0, "the line did not run");
    let bar = r.screens[0].last().unwrap();
    assert!(!bar.contains(":note"), "nothing typed is left: {bar}");
    assert!(!r.tail.contains("not saved"), "nothing unsaved: {}", r.tail);
    assert_eq!(std::fs::read(&tiza).unwrap(), before);
}

#[test]
fn ctrl_c_on_an_empty_command_line_closes_it() {
    let (dir, _) = project("ac1-empty");
    let r = run(
        &dir,
        "ac1e.py",
        &["--no-save"],
        "main.rs",
        // `f` turns the pointer on only in normal mode: in the command
        // line it would be text.
        json!([
            ["k", ":", [["nobar", "NORMAL"]]],
            ["k", CTRL_C, [["bar", "NORMAL"]]],
            ["alive"],
            ["k", "f", [["bar", "POINTER"]]],
            ["api", "status", {}],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true]);
    assert_eq!(pointer(r.api(0)), Some((1, 1)), "{}", r.api(0));
}

#[test]
fn the_whole_line_is_dropped_and_no_c_is_typed() {
    // Text typed, the cursor keys pressed in the middle of it: Ctrl-C drops
    // the line whole. A command typed after it runs alone, so nothing of
    // the dropped line (nor a `c`) was left behind.
    let (dir, _) = project("ac1-middle");
    let r = run(
        &dir,
        "ac1m.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 hello", [["bar", ":note 1 hello"]]],
            ["k", format!("\x1b[D\x1b[D{CTRL_C}"), [["bar", "NORMAL"]]],
            ["alive"],
            ["k", ":note 2 x\r", [["status", "annotations", 1]]],
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
fn ctrl_c_leaves_a_selection_without_an_annotation_and_the_next_quits() {
    // Test step 2: `v`, `j`, Ctrl-C → NORMAL, no annotation; Ctrl-C quits.
    for (key, mode) in [("v", "SELECT"), ("V", "V-LINE")] {
        let (dir, tiza) = project(&format!("ac2-{mode}"));
        let before = std::fs::read(&tiza).unwrap();
        let r = run(
            &dir,
            &format!("ac2-{mode}.py"),
            &["--no-save"],
            "main.rs",
            // `f` lights the cursor line alone in normal mode, the two
            // selected lines in selection mode.
            json!([
                ["k", key, [["bar", mode]]],
                ["k", format!("j{CTRL_C}"), [["bar", "NORMAL"]]],
                ["alive"],
                ["k", "f", [["bar", "POINTER"]]],
                ["api", "status", {}],
                ["k", CTRL_C],
            ]),
        );
        r.quit_cleanly();
        assert_eq!(r.alive, [true], "{key}");
        assert_eq!(pointer(r.api(0)), Some((2, 2)), "{key}: {}", r.api(0));
        assert_eq!(r.api(0)["annotations"], 0);
        assert!(!r.tail.contains("not saved"), "{}", r.tail);
        assert_eq!(std::fs::read(&tiza).unwrap(), before);
    }
}

#[test]
fn ctrl_c_cancels_an_arrow_being_drawn() {
    // `v a` chooses the source; Ctrl-C forgets it, so `v a` again chooses
    // a source (ARROW) instead of drawing the arrow. With the target
    // selected (`v a j v`), one Ctrl-C leaves the selection and the arrow,
    // like Esc.
    let (dir, _) = project("ac2-arrow");
    let r = run(
        &dir,
        "arrow.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", "v", [["bar", "SELECT"]]],
            ["k", "a", [["bar", "ARROW "]]],
            ["k", CTRL_C, [["bar", "NORMAL"], ["has", "arrow cancelled"]]],
            ["alive"],
            ["k", "v", [["bar", "SELECT"]]],
            ["k", "a", [["bar", "ARROW "]]],
            ["api", "status", {}],
            ["k", CTRL_C, [["bar", "NORMAL"]]],
            ["alive"],
            ["k", "v", [["bar", "SELECT"]]],
            ["k", "a", [["bar", "ARROW "]]],
            ["k", "jv", [["bar", "SELECT"]]],
            ["k", CTRL_C, [["bar", "NORMAL"], ["lacks", "ARROW"]]],
            ["alive"],
            ["k", "v", [["bar", "SELECT"]]],
            ["k", "a", [["bar", "ARROW "]]],
            ["api", "status", {}],
            ["k", CTRL_C, [["bar", "NORMAL"]]],
            ["alive"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true, true, true, true]);
    assert_eq!(r.api(0)["annotations"], 0, "no arrow drawn");
    assert_eq!(r.api(1)["annotations"], 0, "no arrow drawn");
    assert!(!r.tail.contains("not saved"), "{}", r.tail);
}

#[test]
fn ctrl_c_clears_the_hint_of_a_selection_as_esc_does() {
    // A mouse drag selects and puts its hint on the bar; Ctrl-C leaves the
    // selection and the hint goes with it.
    let (dir, _) = project("hint");
    let r = run(
        &dir,
        "hint.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":mouse on\r", [["has", "mouse on"]]],
            [
                "k",
                "\x1b[<0;10;3M\x1b[<32;14;3M\x1b[<0;14;3m",
                [["bar", "SELECT"], ["has", "selected: h highlight"]]
            ],
            ["k", CTRL_C, [["bar", "NORMAL"], ["lacks", "selected:"]]],
            ["alive"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true]);
}

#[test]
fn cancelling_a_command_line_a_selection_opened_forgets_the_selection() {
    // `j v l l m` opens `:mark ` on the selected text of line 2. Closing
    // the line without running it (Ctrl-C, Esc, or Backspace past its
    // start) cancels that selection too: `:mark warn` typed right after, on
    // the same line, has no anchor and is refused, instead of marking the
    // selected text. (No key between them: now any key in normal
    // mode but `:`, `u` and Ctrl-R drops a pending selection, and the test
    // would pass even if closing kept it.)
    let closers = [
        ("ctrl-c", CTRL_C.to_string()),
        ("esc", "\x1b".to_string()),
        ("backspace", "\x7f".repeat(6)),
    ];
    for (name, close) in closers {
        let (dir, _) = project(&format!("pending-{name}"));
        let r = run(
            &dir,
            &format!("pending-{name}.py"),
            &["--no-save"],
            "main.rs",
            json!([
                ["k", "j", [["bar", " L2 "]]],
                ["k", "v", [["bar", "SELECT"]]],
                ["k", "llm", [["bar", ":mark"]]],
                ["k", close, [["bar", "NORMAL"]]],
                ["k", ":mark warn\r", [["has", "usage: :mark"]]],
                ["api", "status", {}],
                ["k", CTRL_C],
            ]),
        );
        r.quit_cleanly();
        assert_eq!(r.api(0)["annotations"], 0, "{name}: {}", r.api(0));
        assert!(!r.tail.contains("not saved"), "{name}: {}", r.tail);
    }
}

// ---------------------------------------------------------------- AC3

#[test]
fn ctrl_c_closes_the_help_and_the_list_and_the_next_quits() {
    // Test step 3: `?`, Ctrl-C → help closes; Ctrl-C quits. Same with `L`.
    for (key, name) in [("?", "help"), ("L", "list")] {
        let open = "esc close";
        let (dir, _) = project(&format!("ac3-{name}"));
        let r = run(
            &dir,
            &format!("ac3-{name}.py"),
            &["--no-save"],
            "main.rs",
            // An overlay takes `f` for itself; without it the pointer
            // turns on.
            json!([
                ["k", key, [["has", open]]],
                ["k", CTRL_C, [["lacks", open]]],
                ["alive"],
                ["k", "f", [["bar", "POINTER"]]],
                ["api", "status", {}],
                ["k", CTRL_C],
            ]),
        );
        r.quit_cleanly();
        assert_eq!(r.alive, [true], "{name}");
        assert_eq!(pointer(r.api(0)), Some((1, 1)), "{name}: {}", r.api(0));
    }
}

#[test]
fn ctrl_c_closes_the_review_and_the_proposal_stays_pending() {
    let (dir, _) = project("ac3-review");
    let r = run(
        &dir,
        "review.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":supervise on\r", [["status", "supervised", true]]],
            ["bg_api", "annotation.add", {"type": "note", "line": 1, "text": "agent"}],
            ["wait", [["bar", "1 to review · :review"]]],
            ["k", ":review\r", [["has", "esc hide"], ["has", "agent"]]],
            ["k", CTRL_C, [["lacks", "esc hide"]]],
            ["alive"],
            ["k", "f", [["bar", "POINTER"]]],
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
fn ctrl_c_quits_from_normal_and_presentation_mode_as_q_bang() {
    // Test step 4: `P`, Ctrl-C → quits; under --no-save stderr names the
    // note, and the sidecar is untouched.
    let (dir, tiza) = project("ac4-present");
    let before = std::fs::read(&tiza).unwrap();
    let r = run(
        &dir,
        "present.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 hello\r", [["status", "annotations", 1]]],
            ["k", "P", [["lacks", "NORMAL"]]],
            ["k", CTRL_C],
        ]),
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
    let (dir, _) = project("ac4-normal");
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
fn the_command_line_in_presentation_mode_closes_and_leaves_the_frame_clean() {
    let (dir, _) = project("ac4-present-cmd");
    let r = run(
        &dir,
        "present-cmd.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", "P", [["lacks", "NORMAL"]]],
            ["k", ":note 1 x", [["has", ":note 1 x"]]],
            ["k", CTRL_C, [["lacks", ":note 1 x"]]],
            ["alive"],
            ["api", "status", {}],
            ["screen"],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(r.alive, [true]);
    assert_eq!(r.api(0)["annotations"], 0);
    let text = r.screens[0].join("\n");
    assert!(
        !text.contains(":note") && !text.contains("NORMAL"),
        "{text}"
    );
    assert!(text.contains("let a = 1;"), "{text}");
    assert!(!r.tail.contains("not saved"), "{}", r.tail);
}

#[test]
fn ctrl_c_quits_with_the_pointer_on_or_an_annotation_selected() {
    let (dir, _) = project("ac4-pointer");
    let r = run(
        &dir,
        "pointer.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", "f", [["bar", "POINTER"]]],
            ["api", "status", {}],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert_eq!(pointer(r.api(0)), Some((1, 1)));
    let (dir, _) = project("ac4-tab");
    let r = run(
        &dir,
        "tab.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 hello\r", [["status", "annotations", 1]]],
            ["k", "\t", [["bar", "▸ a1"]]],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    assert!(
        r.tail.contains("1 change not saved (note a1)"),
        "{}",
        r.tail
    );
}

#[test]
fn ctrl_c_quits_while_a_step_types() {
    // Test step 5: a `typing` move, `n` to its step, Ctrl-C while it types
    // (three lines at 3 s each) → quits, instead of only ending the
    // animation.
    let dir = scratch("ac4-typing");
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
        json!([
            ["k", "n", [["status", "step", 1]]],
            ["api", "code.get", {}],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    let text = r.api(0)["text"].as_str().unwrap();
    assert!(!text.contains("let e = 5;"), "still typing: {text}");
    assert!(!r.tail.contains("could not"), "{}", r.tail);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

// ---------------------------------------------------------------- corners

#[test]
fn an_osc_answer_is_swallowed_before_ctrl_c_is_looked_at() {
    // The answer arrives while the command line is open: none of it is
    // typed (the note says `a`), and a Ctrl-C right after it still closes
    // the line (the second note never runs) before the next one quits.
    let (dir, _) = project("osc");
    let r = run(
        &dir,
        "osc.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 a", [["bar", ":note 1 a"]]],
            [
                "k",
                format!("{OSC_ANSWER}\r"),
                [["status", "annotations", 1]]
            ],
            ["k", ":note 2 b", [["bar", ":note 2 b"]]],
            ["k", format!("{OSC_ANSWER}{CTRL_C}"), [["bar", "NORMAL"]]],
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
fn two_ctrl_c_sent_at_once_cancel_then_quit() {
    let (dir, _) = project("two");
    let r = run(
        &dir,
        "two.py",
        &["--no-save"],
        "main.rs",
        json!([
            ["k", ":note 1 hel", [["bar", ":note 1 hel"]]],
            ["k", format!("{CTRL_C}{CTRL_C}")],
        ]),
    );
    r.quit_cleanly();
    assert!(!r.tail.contains("not saved"), "nothing ran: {}", r.tail);
}

// ---------------------------------------------------------------- AC6

#[test]
fn the_help_says_ctrl_c_cancels_or_quits() {
    let dir = scratch("help");
    let doc = tiza_core::Document::from_code(CODE, "rs", None);
    let live = tiza_qa::Live::open(doc, dir.join("main.rs.tiza"), dir.join("s.sock"));
    live.command("help");
    live.with(|app| app.overlay_filter = "ctrl-c".into());
    let s = live.screen(200, 50);
    assert!(
        s.lines()
            .any(|l| l.contains("Ctrl-C") && l.contains("Close what is open, as Esc does.")),
        "{s}"
    );
}
