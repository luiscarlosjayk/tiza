//! A request that panics while follow is on, on the real binary in a pty
//! (the request hook of the `test-hooks` build). The instance goes back to
//! what it was, and that includes the follow pause and what a load of the
//! followed file wrote: a `step.set` to the resume step loaded the change
//! that waited and then panicked, and the change never loaded again.
//!
//! Acceptance criteria:
//! - AC1 After a dropped request at the resume step, the change still
//!   waits (`pending`), the screen and `reloads` are as before, and the
//!   next step to the resume step loads it.
//! - AC2 After a dropped `scene.put`, the reason of follow is the one of
//!   before (here, that the code is the paste again).

use std::path::Path;

use serde_json::{Value, json};
use tiza_qa::{bin, scratch};

const OLD: &str = "fn f(t: &str) -> usize {\n    let n = t.len();\n    n + 1\n}\n";
const NEW: &str = "fn f(t: &str) -> usize {\n    let n = t.chars().count();\n    n + 1\n}\n";

/// The pty tests of this crate run one at a time (see
/// `reopen_after_follow_off`).
static WATCHERS: std::sync::RwLock<()> = std::sync::RwLock::new(());

fn line_json(out: &str, key: &str) -> Value {
    out.lines()
        .find_map(|l| l.strip_prefix(&format!("{key} ")))
        .map(|j| serde_json::from_str(j).unwrap())
        .unwrap_or_else(|| panic!("no {key} in {out}"))
}

/// `main.rs` (OLD) and a sidecar that reads it: a cursor move at step 1,
/// a patch at step 2, so the resume step is 1.
fn files(dir: &Path) {
    std::fs::write(dir.join("main.rs"), OLD).unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "step": 0,
        "moves": [
            {"at_step": 1, "action": "cursor", "line": 1},
            {"at_step": 2, "action": "code.patch", "op": "replace", "find": "n + 1", "content": "    n * 2"}
        ]
    });
    std::fs::write(dir.join("main.rs.tiza"), doc.to_string()).unwrap();
}

/// The Python helpers of both tests: `api`, and `until`, which waits for a
/// condition on `follow.status` for at most 10 s.
const HELPERS: &str = r##"
import json, time
def api(method, params=None):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params or {})],
                       capture_output=True, text=True, timeout=10)
    return r.returncode, r.stdout + r.stderr
def status():
    return json.loads(api("follow.status")[1])
def until(what, ok):
    end = time.time() + 10
    while time.time() < end:
        st = status()
        if ok(st):
            return st
        time.sleep(0.05)
    print("TIMEOUT", what, json.dumps(status()), flush=True)
    return status()
def code():
    return json.loads(api("code.get")[1])["text"]
"##;

#[test]
fn pty_a_dropped_step_to_the_resume_step_keeps_the_change_waiting() {
    let _alone = WATCHERS.write().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("rollback-resume");
    files(&dir);
    let body = format!(
        r##"
binary, cwd, new = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(cwd)
{HELPERS}
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="request:step.set")
s = Pty([binary, "--socket", "s.sock", "--follow", "main.rs.tiza"], 30, 160, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    until("the watcher", lambda st: st.get("enabled") and st.get("reads", 0) >= 1)
    api("step.next")
    api("step.next")
    with open("main.rs.tmp", "w") as f:
        f.write(new)
    os.rename("main.rs.tmp", "main.rs")
    print("BEFORE", json.dumps(until("the save", lambda st: st.get("pending"))), flush=True)
    rc, text = api("step.set", {{"step": 1}})
    print("DROPPED", json.dumps([rc, text]), flush=True)
    print("AFTER", json.dumps(status()), flush=True)
    print("CODE", json.dumps(code()), flush=True)
    api("step.prev")
    print("LOADED", json.dumps(status()), flush=True)
    print("CODE2", json.dumps(code()), flush=True)
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##
    );
    let out = tiza_qa::pty_script(
        &dir,
        "resume.py",
        &body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap(), NEW],
    );
    assert!(out.contains("EXIT 0") && !out.contains("TIMEOUT"), "{out}");
    let before = line_json(&out, "BEFORE");
    assert_eq!(
        (&before["paused"], &before["pending"]),
        (&json!(true), &json!(true)),
        "{out}"
    );
    let dropped = line_json(&out, "DROPPED");
    assert!(
        dropped[0] != 0
            && dropped[1]
                .as_str()
                .unwrap()
                .contains("a bug in Tiza stopped this request"),
        "{out}"
    );
    // As before the request: the change waits, nothing was loaded.
    let after = line_json(&out, "AFTER");
    for key in ["paused", "pending", "resume_step", "reloads"] {
        assert_eq!(after[key], before[key], "{key}: {out}");
    }
    assert!(
        line_json(&out, "CODE")
            .as_str()
            .unwrap()
            .contains("t.len()"),
        "{out}"
    );
    // The next step to the resume step loads it, once.
    let loaded = line_json(&out, "LOADED");
    assert_eq!(loaded["pending"], json!(false), "{out}");
    assert_eq!(
        loaded["reloads"].as_u64().unwrap(),
        before["reloads"].as_u64().unwrap() + 1,
        "{out}"
    );
    assert!(
        line_json(&out, "CODE2")
            .as_str()
            .unwrap()
            .contains("t.chars().count()"),
        "{out}"
    );
}

#[test]
fn pty_a_dropped_put_keeps_the_reason_of_follow() {
    let _alone = WATCHERS.write().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("rollback-reason");
    files(&dir);
    let body = format!(
        r##"
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
{HELPERS}
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="request:scene.put")
s = Pty([binary, "--socket", "s.sock", "--follow", "main.rs.tiza"], 30, 160, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    until("the watcher", lambda st: st.get("enabled") and st.get("reads", 0) >= 1)
    # A paste turns follow off; an undo and a redo bring the paste back.
    s.send(b"\x1b[200~fn other() {{}}\n\x1b[201~")
    until("the paste", lambda st: not st.get("enabled"))
    s.send(b"u")
    until("the undo", lambda st: "pasted" not in (st.get("reason") or ""))
    s.send(b"U")
    print("BEFORE", json.dumps(until("the redo", lambda st: "paste again" in (st.get("reason") or ""))), flush=True)
    doc = json.loads(api("scene.get")[1])["document"]
    rc, text = api("scene.put", {{"document": dict(doc, title="dropped")}})
    print("DROPPED", json.dumps(rc), flush=True)
    print("AFTER", json.dumps(status()), flush=True)
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##
    );
    let out = tiza_qa::pty_script(
        &dir,
        "reason.py",
        &body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(out.contains("EXIT 0") && !out.contains("TIMEOUT"), "{out}");
    assert_ne!(line_json(&out, "DROPPED"), json!(0), "{out}");
    let before = line_json(&out, "BEFORE");
    assert!(
        before["reason"]
            .as_str()
            .unwrap()
            .contains("the code is the paste again"),
        "{out}"
    );
    assert_eq!(
        line_json(&out, "AFTER")["reason"],
        before["reason"],
        "{out}"
    );
}

/// Follow is off because its watcher could not start; a dropped
/// `follow.set` keeps that reason (two hooks at once).
#[test]
fn pty_a_dropped_follow_set_keeps_why_follow_is_off() {
    let _alone = WATCHERS.write().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("rollback-failed");
    files(&dir);
    let body = format!(
        r##"
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
{HELPERS}
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="follow:fail,request:follow.set")
s = Pty([binary, "--socket", "s.sock", "--follow", "main.rs.tiza"], 30, 160, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    print("BEFORE", json.dumps(until("the failed start", lambda st: "cannot watch" in (st.get("reason") or ""))), flush=True)
    print("DROPPED", json.dumps(api("follow.set", {{"enabled": False}})[0]), flush=True)
    print("AFTER", json.dumps(status()), flush=True)
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##
    );
    let out = tiza_qa::pty_script(
        &dir,
        "failed.py",
        &body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(out.contains("EXIT 0") && !out.contains("TIMEOUT"), "{out}");
    assert_ne!(line_json(&out, "DROPPED"), json!(0), "{out}");
    let before = line_json(&out, "BEFORE");
    assert_eq!(
        line_json(&out, "AFTER")["reason"],
        before["reason"],
        "{out}"
    );
}

/// A dropped `follow.set` that turned follow off during a pause with a
/// change: follow is on again with the same pause and counts, and the
/// change loads at the resume step.
#[test]
fn pty_a_dropped_follow_off_keeps_the_pause() {
    let _alone = WATCHERS.write().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("rollback-follow-off");
    files(&dir);
    let body = format!(
        r##"
binary, cwd, new = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(cwd)
{HELPERS}
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="request:follow.set")
s = Pty([binary, "--socket", "s.sock", "--follow", "main.rs.tiza"], 30, 160, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    until("the watcher", lambda st: st.get("enabled") and st.get("reads", 0) >= 1)
    # One load at step 0 first, so that a watcher started again with
    # counts of its own would show: `reloads` is 1.
    with open("main.rs.tmp", "w") as f:
        f.write(open("main.rs").read().replace("n + 1", "n + 1 "))
    os.rename("main.rs.tmp", "main.rs")
    until("the load", lambda st: st.get("reloads", 0) >= 1)
    api("step.next")
    api("step.next")
    with open("main.rs.tmp", "w") as f:
        f.write(new)
    os.rename("main.rs.tmp", "main.rs")
    print("BEFORE", json.dumps(until("the save", lambda st: st.get("pending"))), flush=True)
    print("DROPPED", json.dumps(api("follow.set", {{"enabled": False}})[0]), flush=True)
    print("AFTER", json.dumps(status()), flush=True)
    api("step.prev")
    print("LOADED", json.dumps(status()), flush=True)
    print("CODE", json.dumps(code()), flush=True)
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##
    );
    let out = tiza_qa::pty_script(
        &dir,
        "follow_off.py",
        &body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap(), NEW],
    );
    assert!(out.contains("EXIT 0") && !out.contains("TIMEOUT"), "{out}");
    assert_ne!(line_json(&out, "DROPPED"), json!(0), "{out}");
    let before = line_json(&out, "BEFORE");
    let after = line_json(&out, "AFTER");
    assert_eq!(before["reloads"], json!(1), "{out}");
    for key in ["enabled", "paused", "pending", "resume_step", "reloads"] {
        assert_eq!(after[key], before[key], "{key}: {out}");
    }
    let loaded = line_json(&out, "LOADED");
    assert_eq!(
        loaded["reloads"].as_u64().unwrap(),
        before["reloads"].as_u64().unwrap() + 1,
        "{out}"
    );
    assert!(
        line_json(&out, "CODE")
            .as_str()
            .unwrap()
            .contains("t.chars().count()"),
        "{out}"
    );
}

/// A dropped `scene.put` that ended the pause (no patch any more) and so
/// loaded the change inside the request: the pause, the change that waits
/// and the code are as before, and the next step to the resume step loads
/// it.
#[test]
fn pty_a_dropped_put_that_loaded_the_change_keeps_it_waiting() {
    let _alone = WATCHERS.write().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("rollback-put-load");
    files(&dir);
    let body = format!(
        r##"
binary, cwd, new = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(cwd)
{HELPERS}
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="request:scene.put")
s = Pty([binary, "--socket", "s.sock", "--follow", "main.rs.tiza"], 30, 160, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    until("the watcher", lambda st: st.get("enabled") and st.get("reads", 0) >= 1)
    api("step.next")
    api("step.next")
    with open("main.rs.tmp", "w") as f:
        f.write(new)
    os.rename("main.rs.tmp", "main.rs")
    print("BEFORE", json.dumps(until("the save", lambda st: st.get("pending"))), flush=True)
    doc = json.loads(api("scene.get")[1])["document"]
    doc["moves"] = [m for m in doc["moves"] if m["action"] != "code.patch"]
    print("DROPPED", json.dumps(api("scene.put", {{"document": doc}})[0]), flush=True)
    print("AFTER", json.dumps(status()), flush=True)
    print("CODE", json.dumps(code()), flush=True)
    api("step.prev")
    print("LOADED", json.dumps(status()), flush=True)
    print("CODE2", json.dumps(code()), flush=True)
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##
    );
    let out = tiza_qa::pty_script(
        &dir,
        "put_load.py",
        &body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap(), NEW],
    );
    assert!(out.contains("EXIT 0") && !out.contains("TIMEOUT"), "{out}");
    assert_ne!(line_json(&out, "DROPPED"), json!(0), "{out}");
    let before = line_json(&out, "BEFORE");
    let after = line_json(&out, "AFTER");
    for key in ["paused", "pending", "resume_step", "reloads"] {
        assert_eq!(after[key], before[key], "{key}: {out}");
    }
    assert!(
        line_json(&out, "CODE")
            .as_str()
            .unwrap()
            .contains("t.len()"),
        "{out}"
    );
    let loaded = line_json(&out, "LOADED");
    assert_eq!(
        loaded["reloads"].as_u64().unwrap(),
        before["reloads"].as_u64().unwrap() + 1,
        "{out}"
    );
    assert!(
        line_json(&out, "CODE2")
            .as_str()
            .unwrap()
            .contains("t.chars().count()"),
        "{out}"
    );
}
