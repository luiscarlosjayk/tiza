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
