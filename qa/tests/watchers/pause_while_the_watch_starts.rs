//! Follow turned on at a step with a patch applied (paused) while the file
//! watcher starts slowly, on the real binary in a pty. The `follow:slow`
//! test hook holds the start 3 s, as a busy fseventsd does.
//!
//! A save made while the watcher starts has no event of its own: the start
//! reads the file again, so the save waits as a change (it is not lost),
//! with its notice. At step 0 it loads once: what the start read and what
//! the watcher sees later are one change, never two reloads.

use serde_json::Value;
use tiza_qa::{FOLLOW_START_BUDGET, PTY_DEADLINE_S, bin, scratch};

const OLD: &str = "fn main() {\n    let a = 1;\n}\n";

/// The script. `until()` waits on `follow.status` for at most `BUDGET` s
/// ([`FOLLOW_START_BUDGET`], set before it runs): each wait is for the
/// watcher, or for a save that it reads, and on a loaded Mac fseventsd
/// can take tens of seconds.
const BODY: &str = r##"
import hashlib, json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="follow:slow")

def api(method, params=None):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params or {})],
                       capture_output=True, text=True, timeout=30)
    return json.loads(r.stdout or "null")

def status():
    return api("follow.status") or {}

def until(pred, timeout=BUDGET):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if pred():
            return True
        time.sleep(0.05)
    return pred()

s = Pty([binary, "--socket", "s.sock", "main.rs.tiza"], 30, 160, env=env)
ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
if ok:
    print("SET", json.dumps(api("follow.set", {"enabled": True})), flush=True)
    # The watcher is still starting: this save has no event.
    tmp = "main.rs.tmp"
    open(tmp, "w").write("fn main() {\n    let a = 10;\n}\n")
    os.rename(tmp, "main.rs")
    print("STARTED", until(lambda: status().get("watchers") == 1), flush=True)
    print("WAITS", until(lambda: status().get("pending") is True), flush=True)
    print("PAUSED", json.dumps(status()), flush=True)
    s.send(b"p")
    print("LOADED", until(lambda: status().get("reloads") == 1), flush=True)
    # A late event of the watcher, if any, reads the same text: no reload.
    # A save of the same text: its read comes after that late event, so
    # once `reads` goes up the late event is read too.
    reads = status().get("reads") or 0
    open(tmp, "w").write("fn main() {\n    let a = 10;\n}\n")
    os.rename(tmp, "main.rs")
    print("READ", until(lambda: (status().get("reads") or 0) > reads), flush=True)
    after = status()
    print("AFTER", json.dumps(after), flush=True)
    print("HASH", after.get("hash") == hashlib.sha256(open("main.rs", "rb").read()).hexdigest(), flush=True)
    print("CODE", json.dumps(api("code.get")), flush=True)
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##;

fn line_json(out: &str, key: &str) -> Value {
    let l = out
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key} ")))
        .unwrap_or_else(|| panic!("no {key}\n{out}"));
    serde_json::from_str(l).unwrap()
}

#[test]
fn a_save_while_the_watch_starts_waits_and_loads_once() {
    let dir = scratch("pause-slow");
    std::fs::write(dir.join("main.rs"), OLD).unwrap();
    let doc = serde_json::json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [],
        "step": 1,
        "moves": [{"at_step": 1, "action": "code.patch", "op": "replace",
                   "find": "a = 1", "content": "    let a = 2;"}]
    });
    std::fs::write(dir.join("main.rs.tiza"), doc.to_string()).unwrap();
    // Each `until` can take all of `BUDGET`: the deadline of the script
    // adds one `BUDGET` for each.
    let budget = FOLLOW_START_BUDGET.as_secs();
    let waits = BODY.matches("until(lambda").count() as u64;
    let out = tiza_qa::pty_script_with_deadline(
        &dir,
        "slow.py",
        &format!("BUDGET = {budget}\n{BODY}"),
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
        PTY_DEADLINE_S + waits * budget,
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    let set = line_json(&out, "SET");
    assert_eq!(
        (set["paused"].clone(), set["pending"].clone()),
        (true.into(), false.into()),
        "{out}"
    );
    assert!(out.contains("STARTED True"), "{out}");
    // The save is not lost: it waits, with its notice on the bar.
    assert!(out.contains("WAITS True"), "{out}");
    let paused = line_json(&out, "PAUSED");
    assert_eq!(paused["reloads"], 0, "{out}");
    assert!(
        paused["bar"]
            .as_str()
            .is_some_and(|b| b.contains("step 0") && b.contains("main.rs")),
        "{out}"
    );
    // Step 0 loads it once, and the same text read again loads nothing.
    assert!(out.contains("LOADED True"), "{out}");
    assert!(out.contains("READ True"), "{out}");
    assert!(out.contains("HASH True"), "{out}");
    let after = line_json(&out, "AFTER");
    assert_eq!(after["reloads"], 1, "{out}");
    assert_eq!(after["paused"], false, "{out}");
    let code = line_json(&out, "CODE");
    assert!(
        code["text"]
            .as_str()
            .is_some_and(|t| t.contains("let a = 10;")),
        "{out}"
    );
    assert!(out.contains("EXIT 0"), "{out}");
}
