//! t-203 — A selection that a `:` command did not use stayed stored after
//! Enter, and a later `:mark` landed on it.
//!
//! Rule (accepted by Luis, 2026-10-21): a `:` command that does not take a
//! selection clears it after it runs; one that takes it keeps it so ↑ can
//! re-run the last command on the same selection.
//!
//! Acceptance criteria:
//! - AC1 `v … n` → `:note text` + Enter: the note is made as today and the
//!   selection is gone afterwards; a later `:mark …` does not land on it.
//! - AC2 A command that takes the selection (`:hl`, `:mark`) keeps it after
//!   Enter, and ↑ + Enter re-runs it on the same selection. (`:box`,
//!   `:callout`, `:note` opened from a selection carry its lines in their
//!   own text, so ↑ re-runs them on the same lines as before.)
//! - AC3 A command that fails (bad arguments) leaves the selection as it
//!   was, so the author can fix the line and retry.
//! - AC4 Esc and Ctrl-C keep t-197's behaviour.
//! - AC5 The list of which commands take a selection is one place in the
//!   code (`selection_use` in app.rs); a test walks every `:` command name
//!   (`App::COMMANDS`) and checks it is classified.
//! - AC6 This file, in a pty; CHANGELOG (Fixed).
//!
//! Corner cases: a command with no selection at all is unchanged; `:w`
//! clears it and still saves.

use serde_json::{Value, json};
use tiza_qa::{fixture, scratch};
use tiza_tui::app::{App, SelectionUse};

#[test]
fn ac5_every_command_name_is_classified() {
    for name in App::COMMANDS {
        assert!(
            App::selection_use(name).is_some(),
            ":{name} is not in selection_use: does it take the selection?"
        );
    }
    assert_eq!(App::selection_use("hl"), Some(SelectionUse::Takes));
    assert_eq!(App::selection_use("mark"), Some(SelectionUse::Takes));
    assert_eq!(App::selection_use("m"), Some(SelectionUse::Takes));
    assert_eq!(App::selection_use("note"), Some(SelectionUse::Ignores));
    assert_eq!(App::selection_use("kinds"), Some(SelectionUse::Neutral));
    assert_eq!(App::selection_use("u"), Some(SelectionUse::Neutral));
    assert_eq!(App::selection_use("nonsense"), None);
}

/// Runs `tiza --no-save main.rs` (a copy of `qa/fixtures/ownership/ownership.rs`) on a
/// pty and plays `steps`, then quits with `:q!`. A step is `["k", keys,
/// what]` (sends `keys`, then waits until `what` is on the screen, when
/// given), `["anns", n]` (waits until the document has `n` annotations) or
/// `["dump"]` (prints the annotations as `ANNS <json>`). Returns stdout and
/// the directory.
fn run(name: &str, steps: Value) -> (String, std::path::PathBuf) {
    let dir = scratch(name);
    std::fs::copy(fixture("ownership/ownership.rs"), dir.join("main.rs")).unwrap();
    let body = r##"
import json
binary, cwd, steps = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--no-save", "--socket", "s.sock", "main.rs"], 30, 120, env=env)

def api(method, params):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params)],
                       capture_output=True, text=True, timeout=10)
    if r.returncode != 0:
        print("API_ERR", method, r.stderr.strip(), flush=True)
        return None
    return json.loads(r.stdout)

def anns():
    st = api("scene.get", {}) or {}
    return st.get("document", st).get("annotations", [])

def until(pred, what, timeout=10):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if pred():
            return True
        s.settle(0.05)
    print("WAIT_TIMEOUT", what, flush=True)
    return False

ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
for st in steps if ok else []:
    if st[0] == "k":
        mark = len(s.out)
        s.send(st[1].encode())
        if len(st) > 2 and st[2]:
            s.wait_for(lambda out, w=st[2]: w in plain(out[mark:]), 10, repr(st[2]))
        else:
            s.settle(0.15)
    elif st[0] == "anns":
        until(lambda: len(anns()) == st[1], f"{st[1]} annotations")
    elif st[0] == "dump":
        print("ANNS", json.dumps(anns()), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[
            tiza_qa::bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &steps.to_string(),
        ],
    );
    assert!(
        !out.contains("WAIT_TIMEOUT") && !out.contains("API_ERR"),
        "{out}"
    );
    (out, dir)
}

fn dumps(out: &str) -> Vec<Vec<Value>> {
    out.lines()
        .filter_map(|l| l.strip_prefix("ANNS "))
        .map(|j| serde_json::from_str(j).unwrap())
        .collect()
}

/// `let` on line 2 selected with `v`: the cursor goes to line 2, `w` to
/// `let`, `v` starts the selection, `e` takes the word.
const SELECT_LET: &[&str] = &["j", "w", "v", "e"];

fn select_let() -> Vec<Value> {
    SELECT_LET.iter().map(|k| json!(["k", k])).collect()
}

fn steps(parts: Vec<Vec<Value>>) -> Value {
    Value::Array(parts.into_iter().flatten().collect())
}

#[test]
fn ac1_a_note_from_a_selection_lets_it_go() {
    // Test step 1: `v … n` → `:note 2 hello`; then `:mark drop` with no
    // anchor has no selection to land on: it says how to use it.
    let (out, _) = run(
        "t203-ac1",
        steps(vec![
            select_let(),
            vec![
                json!(["k", "n", ":note 2"]),
                json!(["k", "hello\r"]),
                json!(["anns", 1]),
                json!(["k", ":mark drop\r", "usage: :mark"]),
                json!(["dump"]),
            ],
        ]),
    );
    let d = dumps(&out);
    assert_eq!(d[0].len(), 1, "only the note: {out}");
    assert_eq!(d[0][0]["type"], "note");
}

#[test]
fn ac2_hl_and_mark_keep_the_selection_for_up_enter() {
    // Test step 2: `v … m` → `:mark drop` on the selection; `:` ↑ Enter
    // runs it again on the same text.
    let (out, _) = run(
        "t203-ac2",
        steps(vec![
            select_let(),
            vec![
                json!(["k", "m", ":mark"]),
                json!(["k", "drop\r"]),
                json!(["anns", 1]),
                json!(["k", ":"]),
                json!(["k", "\x1b[A"]),
                json!(["k", "\r"]),
                json!(["anns", 2]),
                json!(["dump"]),
            ],
        ]),
    );
    let d = &dumps(&out)[0];
    assert_eq!(d.len(), 2, "{out}");
    assert_eq!(d[0]["type"], "mark");
    assert_eq!(d[0]["span"], d[1]["span"], "the same selection: {out}");
    assert_eq!(d[0]["span"]["line"], 2, "{out}");
}

#[test]
fn ac3_a_failed_command_leaves_the_selection() {
    // Test step 3: `v … m` → a kind that does not exist → an error, which
    // closes the command line; `:` opens it again (empty) and the fixed
    // command → the mark is on the selection. (Ctrl-U, which clears an open
    // line, is in corner_w.)
    let (out, _) = run(
        "t203-ac3",
        steps(vec![
            select_let(),
            vec![
                json!(["k", "m", ":mark"]),
                json!(["k", "nonsense-kind\r", "unknown mark kind"]),
                json!(["k", ":"]),
                json!(["k", "mark drop\r"]),
                json!(["anns", 1]),
                json!(["dump"]),
            ],
        ]),
    );
    let d = &dumps(&out)[0];
    assert_eq!(d.len(), 1, "{out}");
    assert_eq!(d[0]["type"], "mark", "{out}");
    assert_eq!(d[0]["span"]["line"], 2, "on the selection: {out}");
}

#[test]
fn corner_w_clears_it_and_still_saves() {
    let (out, dir) = run(
        "t203-w",
        steps(vec![
            select_let(),
            vec![
                json!(["k", "n", ":note 2"]),
                json!(["k", "\x15w\r", "saved to"]),
                json!(["k", ":mark drop\r", "usage: :mark"]),
                json!(["dump"]),
            ],
        ]),
    );
    assert!(dumps(&out)[0].is_empty(), "{out}");
    assert!(dir.join("main.rs.tiza").exists());
}

#[test]
fn ac3_a_failed_command_that_does_not_take_it_leaves_it_too() {
    // `v … n` → Enter on `:note 2 ` with no text: an error; the selection is
    // still there for the `:mark` that follows.
    let (out, _) = run(
        "t203-ac3-note",
        steps(vec![
            select_let(),
            vec![
                json!(["k", "n", ":note 2"]),
                json!(["k", "\r", "the note is empty"]),
                json!(["k", ":mark drop\r"]),
                json!(["anns", 1]),
                json!(["dump"]),
            ],
        ]),
    );
    let d = &dumps(&out)[0];
    assert_eq!(d[0]["span"]["line"], 2, "on the selection: {out}");
}

#[test]
fn rev_a_kept_selection_goes_with_the_next_key() {
    // `:mark drop` kept the selection for ↑; a motion lets it go, so a
    // later `:mark read` with no anchor does not land on the old text.
    let (out, _) = run(
        "t203-rev-motion",
        steps(vec![
            select_let(),
            vec![
                json!(["k", "m", ":mark"]),
                json!(["k", "drop\r"]),
                json!(["anns", 1]),
                json!(["k", "j"]),
                json!(["k", ":mark read\r", "usage: :mark"]),
                json!(["dump"]),
            ],
        ]),
    );
    assert_eq!(dumps(&out)[0].len(), 1, "{out}");
}

#[test]
fn rev_a_mark_with_its_own_anchor_lets_the_selection_go() {
    // `:mark` with a typed anchor did not use the selection: it is not kept.
    let (out, _) = run(
        "t203-rev-anchor",
        steps(vec![
            select_let(),
            vec![
                json!(["k", "m", ":mark"]),
                json!(["k", "\x15mark drop 7:len\r"]),
                json!(["anns", 1]),
                json!(["k", ":mark read\r", "usage: :mark"]),
                json!(["dump"]),
            ],
        ]),
    );
    let d = &dumps(&out)[0];
    assert_eq!(d.len(), 1, "{out}");
    assert_eq!(d[0]["span"]["line"], 7, "{out}");
}

#[test]
fn rev_enter_on_an_empty_line_closes_it_as_esc() {
    let (out, _) = run(
        "t203-rev-empty",
        steps(vec![
            select_let(),
            vec![
                json!(["k", "n", ":note 2"]),
                json!(["k", "\x15\r", "NORMAL"]),
                json!(["k", ":mark drop\r", "usage: :mark"]),
                json!(["dump"]),
            ],
        ]),
    );
    assert!(dumps(&out)[0].is_empty(), "{out}");
}

#[test]
fn rev_undo_keeps_it_for_the_correction() {
    // `:mark drop`, undone with `:u` or `u`, then `:` ↑ … Enter: the same
    // command on the same selection.
    for (name, undo, ups) in [
        ("t203-rev-undo-cmd", ":u\r", "\x1b[A\x1b[A"),
        ("t203-rev-undo-key", "u", "\x1b[A"),
    ] {
        let (out, _) = run(
            name,
            steps(vec![
                select_let(),
                vec![
                    json!(["k", "m", ":mark"]),
                    json!(["k", "drop\r"]),
                    json!(["anns", 1]),
                    json!(["k", undo]),
                    json!(["anns", 0]),
                    json!(["k", ":"]),
                    json!(["k", ups]),
                    json!(["k", "\r"]),
                    json!(["anns", 1]),
                    json!(["dump"]),
                ],
            ]),
        );
        let d = &dumps(&out)[0];
        assert_eq!(d[0]["span"]["line"], 2, "{name}: {out}");
    }
}
