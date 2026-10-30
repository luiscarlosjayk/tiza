//! A request that panics goes back whole, on the real binary in a pty (the
//! hooks of the `test-hooks` build, two at once). Two things it changed
//! were not put back: the enter effect that a step ended, so the
//! auto-advance of the step never started, and the saved state, so with
//! autosave off `:q` said there were unsaved changes.
//!
//! Acceptance criteria:
//! - AC1 A dropped `step.set` that ended the effect of the step: the
//!   effect still plays, and when it ends (here, with a key) the step
//!   advances by itself after its `auto_advance_ms`.
//! - AC2 A dropped `step.set` with autosave off: `:q` quits, as before the
//!   request.

use serde_json::json;
use tiza_qa::{bin, scratch};

const CODE: &str = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";

/// A box with the `draw` effect at step 1, which advances by itself after
/// 200 ms, and a note at step 2.
fn document(dir: &std::path::Path) {
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "step": 0,
        "annotations": [
            {"id": "b1", "step": 1, "type": "box", "from_line": 1, "to_line": 2, "effect": "draw"},
            {"id": "n1", "step": 2, "type": "note", "line": 3, "text": "the end"}
        ],
        "steps": [{"step": 1, "auto_advance_ms": 200}]
    });
    std::fs::write(dir.join("main.rs.tiza"), doc.to_string()).unwrap();
}

/// Runs the TUI with `hooks`, then `steps` (Python); prints its lines.
fn run(name: &str, hooks: &str, steps: &str) -> String {
    let dir = scratch(name);
    document(&dir);
    let body = format!(
        r##"
import json, time
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
def api(method, params=None):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params or {{}})],
                       capture_output=True, text=True, timeout=10)
    return r.returncode, r.stdout + r.stderr
def step():
    return json.loads(api("status")[1])["step"]
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="{hooks}")
s = Pty([binary, "--socket", "s.sock", "main.rs.tiza"], 30, 120, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
{steps}
print("EXIT", s.finish(10))
"##
    );
    tiza_qa::pty_script(
        &dir,
        "rollback.py",
        &body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
    )
}

#[test]
fn pty_a_dropped_step_that_ends_an_effect_keeps_the_auto_advance() {
    let out = run(
        "rollback-effect",
        "effect:slow,request:step.set",
        r##"
    # Step 1: the effect of the box plays, 100 times slower.
    api("step.next")
    print("PLAYS", json.dumps(json.loads(api("status")[1]).get("effect")), flush=True)
    # A step.set to the step on screen ends the effect, then panics.
    print("DROPPED", json.dumps(api("step.set", {"step": 1})[0]), flush=True)
    print("STILL", json.dumps(json.loads(api("status")[1]).get("effect")), flush=True)
    # A key that is not a view key ends the effect (`z` does nothing else).
    s.send(b"z")
    end = time.time() + 10
    while time.time() < end and step() != 2:
        time.sleep(0.05)
    print("STEP", step(), flush=True)
    s.send(b":q!\r")
"##,
    );
    assert!(out.contains("PLAYS true"), "{out}");
    assert!(!out.contains("DROPPED 0"), "{out}");
    assert!(out.contains("STILL true"), "the effect is gone: {out}");
    assert!(
        out.contains("STEP 2"),
        "the step did not advance by itself: {out}"
    );
    assert!(out.contains("EXIT 0"), "{out}");
}

#[test]
fn pty_a_dropped_step_with_autosave_off_leaves_nothing_unsaved() {
    let out = run(
        "rollback-dirty",
        "request:step.set",
        r##"
    s.send(b":autosave off\r")
    s.wait_for(lambda out: b"autosave off" in out, 10, "autosave off")
    print("DROPPED", json.dumps(api("step.set", {"step": 1})[0]), flush=True)
    s.send(b":q\r")
"##,
    );
    assert!(!out.contains("DROPPED 0"), "{out}");
    assert!(out.contains("EXIT 0"), ":q did not quit: {out}");
}

/// A dropped request that changed the step (and so the revision): the
/// effect of the step it comes back to still plays, it does not jump to
/// its end.
#[test]
fn pty_a_dropped_step_to_another_step_keeps_the_effect_playing() {
    let out = run(
        "rollback-effect-step",
        "effect:slow,request:step.next",
        r##"
    api("step.set", {"step": 1})
    print("PLAYS", json.dumps(json.loads(api("status")[1]).get("effect")), flush=True)
    print("DROPPED", json.dumps(api("step.next")[0]), flush=True)
    # The loop ticks the effect after the request; this reply comes after.
    st = json.loads(api("status")[1])
    print("STILL", json.dumps([st["step"], st.get("effect")]), flush=True)
    s.send(b":q!\r")
"##,
    );
    assert!(out.contains("PLAYS true"), "{out}");
    assert!(!out.contains("DROPPED 0"), "{out}");
    assert!(out.contains("STILL [1, true]"), "{out}");
    assert!(out.contains("EXIT 0"), "{out}");
}
