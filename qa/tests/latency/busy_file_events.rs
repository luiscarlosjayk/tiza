//! Follow froze the TUI while fseventsd was busy (a build running beside
//! it): starting the file watcher (`:follow on`, `:ref` to another file)
//! waited for the event stream to start, and stopping it (`:follow off`, a
//! `scene.put` of another document, a paste) waited for the stream's
//! thread, seconds each, up to 50 s measured; a socket request in the
//! meantime timed out.
//!
//! Acceptance criteria:
//! - AC1 Measured first: with the follow tests running in another process, a single
//!   watcher took 8.2 s to start and 22 s to drop; idle, 2 ms and 0.3 ms
//!   (card note).
//! - AC2 The watcher starts and stops on threads of its own, so starting
//!   or stopping follow never blocks the TUI or a socket request; a save
//!   made while it starts is reloaded once it runs; a start that fails
//!   turns follow off and says why (bar and `follow.status`); quitting
//!   waits for none of these threads and leaves no watcher.
//! - AC3 This file, with the `follow:slow` test hook (3 s on each side,
//!   only with the `test-hooks` feature) standing for a busy fseventsd;
//!   CHANGELOG (Fixed).
//!
//! Corner cases: `:follow on`/`off` many times quickly (the last one asked
//! wins, one watcher); quitting while a watcher starts.

use std::path::PathBuf;

use serde_json::Value;
use tiza_qa::{bin, scratch};

const CODE: &str = "fn main() {\n    let a = 1;\n}\n";

/// `main.rs` and its `.tiza` (a note on line 2) in a scratch folder.
fn setup(name: &str) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let doc = serde_json::json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 2, "text": "a is one"}],
        "step": 1
    });
    std::fs::write(dir.join("main.rs.tiza"), doc.to_string()).unwrap();
    dir
}

/// Python shared by the tests: the TUI with a test hook, `api()` timed,
/// `status()`, and `until()` that waits on a condition (10 s).
const PRELUDE: &str = r##"
import json
binary, cwd, hook = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK=hook)

def api(method, params=None):
    t0 = time.monotonic()
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params or {})],
                       capture_output=True, text=True, timeout=30)
    return r.returncode, r.stdout + r.stderr, time.monotonic() - t0

def status():
    return json.loads(api("follow.status")[1])

def screen():
    # What the screen shows (the TUI repaints only the cells that change).
    return json.loads(api("render.text")[1])["text"]

def until(pred, timeout=13.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if pred():
            return True
        time.sleep(0.05)
    return pred()

def start():
    s = Pty([binary, "--socket", "s.sock", "main.rs.tiza"], 30, 160, env=env)
    ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
    return s, ok

def quit(s):
    t0 = time.monotonic()
    s.send(b":q!\r")
    code = s.finish(10)
    print("EXIT", code, "%.2f" % (time.monotonic() - t0), flush=True)
"##;

fn run(dir: &std::path::Path, name: &str, hook: &str, body: &str) -> String {
    tiza_qa::pty_script(
        dir,
        name,
        &format!("{PRELUDE}\n{body}"),
        &[bin().to_str().unwrap(), dir.to_str().unwrap(), hook],
    )
}

/// The seconds a `NAME rc secs` line says, with its exit code.
fn timed(out: &str, name: &str) -> Vec<(i32, f64)> {
    out.lines()
        .filter_map(|l| l.strip_prefix(&format!("{name} ")))
        .map(|l| {
            let mut it = l.split_whitespace();
            (
                it.next().unwrap().parse().unwrap(),
                it.next().unwrap().parse().unwrap(),
            )
        })
        .collect()
}

fn line_json(out: &str, name: &str) -> Value {
    let l = out
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix(&format!("{name} ")))
        .unwrap_or_else(|| panic!("no {name}\n{out}"));
    serde_json::from_str(l).unwrap()
}

fn assert_quick(out: &str, name: &str) {
    let all = timed(out, name);
    assert!(!all.is_empty(), "no {name}\n{out}");
    for (rc, secs) in all {
        assert_eq!(rc, 0, "{name}\n{out}");
        assert!(secs < 2.0, "{name} took {secs} s\n{out}");
    }
}

/// The request sent right after `follow off` (`NEXT`) answered in under 1 s.
fn assert_next_within_1s(out: &str) {
    let next = timed(out, "NEXT");
    assert!(!next.is_empty(), "no NEXT\n{out}");
    for (rc, secs) in next {
        assert_eq!(rc, 0, "{out}");
        assert!(
            secs < 1.0,
            "the request after follow off took {secs} s\n{out}"
        );
    }
}

#[test]
fn a_save_while_the_watch_starts_is_reloaded() {
    let _alone = crate::alone();
    let dir = setup("save");
    let out = run(
        &dir,
        "save.py",
        "follow:slow",
        r##"
s, ok = start()
if ok:
    rc, _, t = api("follow.set", {"enabled": True})
    print("SET", rc, "%.2f" % t, flush=True)
    with open("main.rs", "w") as f:
        f.write("fn main() {\n    let a = 10;\n}\n")
    print("RELOADED", until(lambda: "let a = 10" in screen()), flush=True)
    print("STATUS", json.dumps(status()), flush=True)
    # A running watcher stops at once too, and goes away on its own thread:
    # the next request answers at once while it does (the instance is not
    # seen as gone, `no Tiza instance is listening`).
    rc, _, t = api("follow.set", {"enabled": False})
    print("SET", rc, "%.2f" % t, flush=True)
    rc, _, t = api("status")
    print("NEXT", rc, "%.2f" % t, flush=True)
    print("GONE", until(lambda: status().get("watchers") == 0), flush=True)
    quit(s)
"##,
    );
    assert_quick(&out, "SET");
    assert_next_within_1s(&out);
    assert!(out.contains("RELOADED True"), "{out}");
    assert!(out.contains("GONE True"), "{out}");
    let st = line_json(&out, "STATUS");
    assert_eq!(st["enabled"], true, "{out}");
    assert_eq!(st["reloads"], 1, "{out}");
    assert!(timed(&out, "EXIT")[0].1 < 2.0, "{out}");
}

#[test]
fn follow_off_and_quit_while_the_watch_starts() {
    let _alone = crate::alone();
    let dir = setup("off");
    let out = run(
        &dir,
        "off.py",
        "follow:slow",
        r##"
s, ok = start()
if ok:
    for enabled in [True, False]:
        rc, _, t = api("follow.set", {"enabled": enabled})
        print("SET", rc, "%.2f" % t, flush=True)
    rc, _, t = api("status")
    print("NEXT", rc, "%.2f" % t, flush=True)
    # The watcher that was starting goes away once it has started.
    print("GONE", until(lambda: status().get("watchers") == 0), flush=True)
    with open("main.rs", "w") as f:
        f.write("fn main() {\n    let a = 10;\n}\n")
    s.settle(0.6)
    print("RELOADED", "let a = 10" in screen(), flush=True)
    # Quitting while a watcher starts waits for nothing.
    rc, _, t = api("follow.set", {"enabled": True})
    print("SET", rc, "%.2f" % t, flush=True)
    quit(s)
"##,
    );
    assert_quick(&out, "SET");
    assert_next_within_1s(&out);
    assert!(out.contains("GONE True"), "{out}");
    assert!(out.contains("RELOADED False"), "{out}");
    let (code, secs) = timed(&out, "EXIT")[0];
    assert_eq!(code, 0, "{out}");
    assert!(secs < 2.0, "quit took {secs} s\n{out}");
}

#[test]
fn many_on_off_quickly_end_in_the_last_one_with_one_watcher() {
    let _alone = crate::alone();
    let dir = setup("many");
    let out = run(
        &dir,
        "many.py",
        "follow:slow",
        r##"
s, ok = start()
if ok:
    for enabled in [True, False, True, False, True]:
        rc, _, t = api("follow.set", {"enabled": enabled})
        print("SET", rc, "%.2f" % t, flush=True)
    # Every start counts from its thread's first moment: once the two
    # started for nothing have ended, one watcher is left, the last one.
    print("ONE", until(lambda: status().get("watchers") == 1), flush=True)
    with open("main.rs", "w") as f:
        f.write("fn main() {\n    let a = 10;\n}\n")
    print("RELOADED", until(lambda: "let a = 10" in screen()), flush=True)
    print("STATUS", json.dumps(status()), flush=True)
    # A put of another document stops the running watcher at once.
    doc = json.loads(api("scene.get")[1])["document"]
    other = dict(doc, source=dict(doc["source"], path="elsewhere.rs"), code=["fn elsewhere() {}"], annotations=[])
    rc, _, t = api("scene.put", {"document": other})
    print("PUT", rc, "%.2f" % t, flush=True)
    print("GONE", until(lambda: status().get("watchers") == 0), flush=True)
    quit(s)
"##,
    );
    assert_quick(&out, "SET");
    assert_quick(&out, "PUT");
    assert!(out.contains("GONE True"), "{out}");
    assert!(out.contains("ONE True"), "{out}");
    assert!(out.contains("RELOADED True"), "{out}");
    let st = line_json(&out, "STATUS");
    assert_eq!(st["enabled"], true, "{out}");
    assert_eq!(st["reloads"], 1, "{out}");
}

#[test]
fn a_watch_that_fails_turns_follow_off_and_says_why() {
    let _alone = crate::alone();
    let dir = setup("fail");
    let out = run(
        &dir,
        "fail.py",
        "follow:fail",
        r##"
s, ok = start()
if ok:
    rc, _, t = api("follow.set", {"enabled": True})
    print("SET", rc, "%.2f" % t, flush=True)
    print("OFF", until(lambda: status()["enabled"] is False), flush=True)
    print("STATUS", json.dumps(status()), flush=True)
    api("follow.set", {"enabled": False})
    print("CLEARED", json.dumps(status()), flush=True)
    quit(s)
"##,
    );
    assert!(out.contains("OFF True"), "{out}");
    assert!(
        line_json(&out, "STATUS")["bar"]
            .as_str()
            .is_some_and(|b| b.starts_with("follow off: cannot watch ")),
        "{out}"
    );
    assert!(line_json(&out, "CLEARED").get("reason").is_none(), "{out}");
    let st = line_json(&out, "STATUS");
    assert!(
        st["reason"]
            .as_str()
            .is_some_and(|r| r.contains("cannot watch") && r.contains("test hook")),
        "{out}"
    );
}

#[test]
fn a_start_over_a_paste_at_step_1_keeps_saying_it_waits() {
    let _alone = crate::alone();
    // The watcher's start counted as a save: over a paste at a step with
    // moves applied, the bar said the file changed on disk, and it did not.
    let dir = scratch("paste");
    std::fs::write(dir.join("main.rs"), "fn main() {\n    let x = 1;\n}\n").unwrap();
    let doc = serde_json::json!({
        "schema": 1,
        "source": {"language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [{"id": "a1", "step": 1, "type": "note", "line": 2, "text": "now 2"}],
        "moves": [{"at_step": 1, "action": "code.patch", "op": "replace",
                   "find": "x = 1", "content": "    let x = 2;"}]
    });
    std::fs::write(dir.join("main.rs.tiza"), doc.to_string()).unwrap();
    let out = run(
        &dir,
        "paste.py",
        "follow:slow",
        r##"
WAITING = "following main.rs (waiting: moves applied at step 1; loads at step 0)"
s, ok = start()
if ok:
    api("step.set", {"step": 0})
    api("follow.set", {"enabled": True})
    s.send(b"\x1b[200~fn main() {\n    let x = 1;\n    let y = 3;\n}\n\x1b[201~")
    print("PASTED", until(lambda: status()["enabled"] is False), flush=True)
    api("step.set", {"step": 1})
    rc, out, t = api("follow.set", {"enabled": True})
    print("SET", rc, "%.2f" % t, flush=True)
    # The watcher has started (the one of the first `follow on` is gone).
    print("ONE", until(lambda: status().get("watchers") == 1), flush=True)
    s.settle(0.6)
    print("STATUS", json.dumps(status()), flush=True)
    quit(s)
"##,
    );
    assert!(out.contains("PASTED True"), "{out}");
    assert_quick(&out, "SET");
    assert!(out.contains("ONE True"), "{out}");
    let st = line_json(&out, "STATUS");
    assert_eq!(
        st["bar"], "following main.rs (waiting: moves applied at step 1; loads at step 0)",
        "{out}"
    );
    assert_eq!(
        st["reason"], "waiting: moves applied at step 1; loads at step 0",
        "{out}"
    );
}
