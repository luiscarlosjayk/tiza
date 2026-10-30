//! Ctrl-C over a follow notice or a stale notice quits, on the real binary in
//! a pty.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::scratch;

const CTRL_C: &str = "\x03";

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
}

/// Runs `tiza ARGS FILE` on a 120×30 pty in `dir`, with `--socket s.sock`,
/// and plays `steps`:
/// - `["k", text, conds]` sends the keys, then waits for `conds` (below);
/// - `["wait", conds]` waits for `conds`;
/// - `["alive"]` records whether the child still answers a request. Put it
///   after a step that waits for the effect of its key: a key that made
///   Tiza quit ends the loop before that request;
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
def screen(raw):
    """The rows a terminal shows after `raw` (`terminal_rows` of the prelude)."""
    return terminal_rows(raw, ROWS, COLS)

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
            # The step before waited for the effect of its key: had that
            # key made Tiza quit, no request would get a reply now.
            print("ALIVE", 1 if api("status", {}) is not None and s.p.poll() is None else 0, flush=True)
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
fn ctrl_c_over_a_follow_or_stale_notice_quits() {
    // A reload notice on the bar is not something to cancel.
    let dir = followed("ac4-reload");
    let r = run(
        &dir,
        "reload.py",
        &["--no-save", "--follow"],
        "config.ini.tiza",
        json!([
            ["save", "config.ini", format!("# payments\n{INI}")],
            ["wait", [["has", "reloaded (#1)"]]],
            ["k", CTRL_C],
        ]),
    );
    r.quit_cleanly();
    // Nor a stale notice; stderr gives the summary of stale moves.
    let dir = followed("ac4-stale");
    let before = std::fs::read(dir.join("config.ini.tiza")).unwrap();
    let r = run(
        &dir,
        "stale.py",
        &["--no-save", "--follow"],
        "config.ini.tiza",
        json!([
            [
                "save",
                "config.ini",
                INI.replace("host = localhost", "host = db1")
            ],
            ["wait", [["has", "stale move skipped"]]],
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
