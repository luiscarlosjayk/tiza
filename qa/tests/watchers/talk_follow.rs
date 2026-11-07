//! `--follow` in a talk, on the real binary in a pty, with watchers that
//! start and stop slowly (the `follow:slow` hook holds each 3 s, as a busy
//! fseventsd does).
//!
//! Acceptance criteria:
//! - AC1 A scene change does not wait for the watcher of either scene: the
//!   next scene is on screen before its watcher starts, and before the
//!   watcher of the scene before stops.
//! - AC2 The scene with a file follows it; the watcher of a scene stops
//!   when the talk leaves it. After a scene with no file, the next scene
//!   with a file follows again.

use std::path::Path;

use serde_json::Value;
use tiza_qa::{FOLLOW_START_BUDGET, PTY_DEADLINE_S, bin, fixture, scratch};

const BODY: &str = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="follow:slow")

def answer(method):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, "{}"],
                       capture_output=True, text=True, timeout=30)
    return json.loads(r.stdout) if r.returncode == 0 else {}

def follow():
    return answer("follow.status")

def position():
    return (follow().get("test_scene") or {}).get("position")

def until(pred, what, timeout=BUDGET):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if pred():
            return True
        s.settle(0.05)
    print("WAIT_TIMEOUT", what, json.dumps(follow()), flush=True)
    return False

def go(scene, position_after):
    s.send(b":scene " + scene + b"\r")
    ok = until(lambda: position() == position_after, "scene " + scene.decode(), 10)
    st = follow()
    return ok, {k: st.get(k) for k in ("enabled", "watching", "watchers")}

s = Pty([binary, "--socket", "s.sock", "--follow", "talk.tiza"], 30, 100, env=env)
ok = s.wait_listening() and until(lambda: position() == 1, "the first scene", 10)
if ok:
    print("INTRO", json.dumps({"enabled": follow().get("enabled")}), flush=True)
    ok, at = go(b"borrow", 2)
    print("ARRIVED", json.dumps(at), flush=True)
    ok = ok and until(lambda: follow().get("watching") == "running", "the watcher of borrow")
    tmp = "src/borrow.rs.tmp"
    open(tmp, "w").write(open("src/borrow.rs").read() + "// FOLLOWED\n")
    os.rename(tmp, "src/borrow.rs")
    ok = ok and until(lambda: "FOLLOWED" in (answer("code.get").get("text") or ""), "the reload")
    ok2, at = go(b"own", 3)
    print("LEFT", json.dumps(at), flush=True)
    ok = ok and ok2 and until(lambda: follow().get("watchers") == 0, "the watcher of borrow stops")
    ok2, at = go(b"borrow", 2)
    print("BACK", json.dumps(at), flush=True)
    ok = ok and ok2 and until(lambda: follow().get("watching") == "running", "the watcher of borrow again")
print("PATH", ok, flush=True)
s.send(b"q")
print("EXIT", s.finish(10), flush=True)
"##;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

fn line_json(out: &str, key: &str) -> Value {
    let l = out
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key} ")))
        .unwrap_or_else(|| panic!("no {key}\n{out}"));
    serde_json::from_str(l).unwrap()
}

#[test]
fn pty_a_scene_change_does_not_wait_for_a_slow_watcher_and_follow_goes_on() {
    let dir = scratch("talk-follow-slow");
    copy_dir(&fixture("shared/talk"), &dir);
    let budget = FOLLOW_START_BUDGET.as_secs();
    let waits = BODY.matches("until(lambda").count() as u64;
    let out = tiza_qa::pty_script_with_deadline(
        &dir,
        "talk-follow.py",
        &format!("BUDGET = {budget}\n{BODY}"),
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
        PTY_DEADLINE_S + waits * budget,
    );
    assert!(out.contains("PATH True"), "{out}");
    assert!(out.contains("EXIT 0"), "{out}");
    // The intro has no file to follow.
    assert_eq!(line_json(&out, "INTRO")["enabled"], false, "{out}");
    // On screen while the watcher of borrow still starts.
    let arrived = line_json(&out, "ARRIVED");
    assert_eq!(arrived["enabled"], true, "{out}");
    assert_eq!(arrived["watching"], "starting", "{out}");
    // On screen while the watcher of borrow still stops.
    let left = line_json(&out, "LEFT");
    assert_eq!(left["enabled"], false, "{out}");
    assert_eq!(left["watchers"], 1, "{out}");
    let back = line_json(&out, "BACK");
    assert_eq!(back["enabled"], true, "{out}");
    assert_eq!(back["watching"], "starting", "{out}");
}
