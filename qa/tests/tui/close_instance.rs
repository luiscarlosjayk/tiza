//! `tiza close` closes the live instance of a session, as `:q` does, and
//! returns only when it exited: a lost terminal can be closed by hand, and
//! the next `tiza --session NAME` starts.
//!
//! Acceptance criteria:
//! - AC1 The reply of `app.quit` gets to its client before the instance
//!   exits.
//! - AC2 `tiza close` exits with 0 only when the instance exited (its socket
//!   is gone and a new instance starts on it), in normal and presentation
//!   mode and with `--follow`. It says which instance it closed.
//! - AC3 With unsaved changes (a `scene.put` with `--no-save`) it refuses
//!   with code 1 and the instance stays; `--force` closes it.
//! - AC4 An instance that answers and does not exit: after 10 s, code 1 and
//!   what to do.
//!
//! A `tiza close` with no instance: `api/socket_and_runtime_folder.rs`.
//! The method itself: `api/quit_method.rs`.

use serde_json::{Value, json};
use tiza_qa::{bin, scratch};

const CODE: &str = "fn main() {\n    let a = 1;\n}\n";

/// Starts `tiza ARGS main.rs` on a pty in a new scratch folder (runtime
/// folder `run/`, `TIZA_TEST_HOOK` = `hook`), waits until it listens, runs
/// `steps` and prints what they gave, then waits for the instance to exit.
///
/// A step is `["close", [args]]` (prints `CLOSE code stdout stderr`),
/// `["api", method, params]` (prints `API code stdout`), `["hold"]` (makes
/// the file `exit-hold` of the runtime folder), `["unhold"]`, `["again"]`
/// (starts a second `tiza ARGS main.rs` and prints `AGAIN listens`, then
/// closes it with `:q!`) or `["socket"]` (prints `SOCKET exists`).
fn run(name: &str, args: &[&str], hook: &str, steps: Value) -> String {
    let dir = scratch(name);
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let body = r##"
import json
binary, cwd, args, hook, steps = sys.argv[1], sys.argv[2], json.loads(sys.argv[3]), sys.argv[4], json.loads(sys.argv[5])
os.chdir(cwd)
run = os.path.join(cwd, "run")
env = dict(os.environ, XDG_RUNTIME_DIR=run, TERM="xterm-256color", TIZA_NO_OSC="1")
for k in ("TIZA_SOCKET", "TIZA_SESSION", "TIZA_TEST_HOOK"):
    env.pop(k, None)
if hook:
    env["TIZA_TEST_HOOK"] = hook
socket = args[args.index("--socket") + 1] if "--socket" in args else os.path.join(run, "tiza", args[args.index("--session") + 1] + ".sock")
print("SOCK_PATH", socket, flush=True)
where = args[:2]
s = Pty([binary, *args, "main.rs"], 30, 100, env=env)
print("LISTENS", s.wait_listening(), flush=True)

def tiza(*cmd):
    r = subprocess.run([binary, *where, *cmd], capture_output=True, text=True, timeout=30, env=env)
    return r.returncode, r.stdout.strip(), r.stderr.strip()

for step in steps:
    op = step[0]
    if op == "close":
        code, out, err = tiza("close", *step[1])
        print("CLOSE", code, json.dumps(out), json.dumps(err), flush=True)
    elif op == "api":
        code, out, err = tiza("api", step[1], json.dumps(step[2]))
        print("API", code, json.dumps(out), json.dumps(err), flush=True)
    elif op == "hold":
        open(os.path.join(run, "tiza", "exit-hold"), "w").close()
    elif op == "unhold":
        os.remove(os.path.join(run, "tiza", "exit-hold"))
    elif op == "socket":
        print("SOCKET", os.path.exists(socket), flush=True)
    elif op == "again":
        t = Pty([binary, *args, "main.rs"], 30, 100, env=env)
        print("AGAIN", t.wait_listening(), flush=True)
        t.send(b":q!\r")
        print("AGAIN_EXIT", t.finish(10), flush=True)
print("EXIT", s.finish(10), flush=True)
out = s.out
left = out.rfind(b"\x1b[?1049l")
print("TAIL", plain(out[left + 8:] if left >= 0 else b"NOT_LEFT").replace("\n", " ").strip(), flush=True)
"##;
    tiza_qa::pty_script(
        &dir,
        name,
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &serde_json::to_string(args).unwrap(),
            hook,
            &steps.to_string(),
        ],
    )
}

fn line<'a>(out: &'a str, key: &str) -> &'a str {
    out.lines()
        .find_map(|l| l.strip_prefix(key))
        .unwrap_or_else(|| panic!("no {key}:\n{out}"))
}

// ---------------------------------------------------------------- AC1

#[test]
fn the_reply_of_app_quit_gets_to_its_client() {
    // The connection writes the reply late: the instance waits for it.
    let out = run(
        "quit-reply",
        &["--socket", "s.sock", "--no-save"],
        "ipc:slow-reply",
        json!([["api", "app.quit", {}]]),
    );
    assert_eq!(line(&out, "LISTENS "), "True", "{out}");
    assert_eq!(
        line(&out, "API "),
        r#"0 "{\n  \"closing\": true\n}" """#,
        "{out}"
    );
    assert_eq!(line(&out, "EXIT "), "0", "{out}");
}

// ---------------------------------------------------------------- AC2

#[test]
fn tiza_close_returns_when_the_instance_exited_and_the_next_one_starts() {
    for (name, args) in [
        ("close-normal", &["--session", "talk"][..]),
        ("close-present", &["--session", "talk", "--present"][..]),
        ("close-follow", &["--session", "talk", "--follow"][..]),
    ] {
        let out = run(
            name,
            args,
            "",
            json!([["close", []], ["socket"], ["again"], ["socket"]]),
        );
        assert_eq!(line(&out, "LISTENS "), "True", "{name}: {out}");
        assert_eq!(
            line(&out, "CLOSE "),
            r#"0 "closed the Tiza instance of the session talk" """#,
            "{name}: {out}"
        );
        // Gone at once: the socket too, and a new instance starts on it.
        assert!(out.contains("SOCKET False\nAGAIN True"), "{name}: {out}");
        assert_eq!(line(&out, "AGAIN_EXIT "), "0", "{name}: {out}");
        assert_eq!(line(&out, "EXIT "), "0", "{name}: {out}");
        assert_ne!(line(&out, "TAIL "), "NOT_LEFT", "{name}: {out}");
    }
    let out = run(
        "close-socket",
        &["--socket", "s.sock"],
        "",
        json!([["close", []]]),
    );
    assert_eq!(
        line(&out, "CLOSE "),
        r#"0 "closed the Tiza instance on s.sock" """#,
        "{out}"
    );
}

// ---------------------------------------------------------------- AC3

#[test]
fn tiza_close_refuses_unsaved_changes_and_force_closes() {
    let doc = json!({
        "schema": 1,
        "source": {"language": "rs"},
        "code": CODE.lines().collect::<Vec<_>>(),
        "step": 0,
        "annotations": [{"id": "a1", "step": 0, "type": "note", "line": 2, "text": "put"}],
    });
    let out = run(
        "close-unsaved",
        &["--session", "talk", "--no-save"],
        "",
        json!([
            ["api", "scene.put", {"document": doc}],
            ["close", []],
            ["api", "ping", {}],
            ["close", ["--force"]],
        ]),
    );
    let closes: Vec<&str> = out
        .lines()
        .filter_map(|l| l.strip_prefix("CLOSE "))
        .collect();
    assert_eq!(
        closes,
        [
            r#"1 "" "tiza: the instance has unsaved changes: tiza close --force closes it and loses them""#,
            r#"0 "closed the Tiza instance of the session talk" """#,
        ],
        "{out}"
    );
    // The refusal left the instance open.
    assert!(out.contains(r#"API 0 "\"pong\"""#), "{out}");
    assert_eq!(line(&out, "EXIT "), "0", "{out}");
    // What `--force` lost is said where the instance ran.
    assert!(
        line(&out, "TAIL ").contains("quit without saving main.rs.tiza"),
        "{out}"
    );
}

// ---------------------------------------------------------------- AC4

#[test]
fn tiza_close_says_so_when_the_instance_does_not_exit() {
    // The instance answers and then holds before it exits: `close` waits
    // for the exit, not for the answer.
    let out = run(
        "close-held",
        &["--session", "talk"],
        "exit:hold",
        json!([["hold"], ["close", []], ["socket"], ["unhold"]]),
    );
    assert_eq!(
        line(&out, "CLOSE "),
        format!(
            r#"1 "" "tiza: the Tiza instance on {} did not close in 10 s: look at its terminal, or end it with kill""#,
            line(&out, "SOCK_PATH ")
        ),
        "{out}"
    );
    assert_eq!(line(&out, "SOCKET "), "True", "{out}");
    assert_eq!(line(&out, "EXIT "), "0", "{out}");
}
