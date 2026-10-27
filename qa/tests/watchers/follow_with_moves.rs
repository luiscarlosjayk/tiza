//! The repro of follow with moves applied, on the real binary in a pty:
//! `--follow`, `follow.set` and `:follow on`, and quitting while the reload
//! waits.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{bin, scratch};

/// QA 1's repro: a move at step 1, no `step`.
const OLD: &str = "fn main() {\n    let x = 1;\n}\n";
/// The editor's save: the move's `find` still matches.
const NEW: &str = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
const WAITING: &str = "waiting: moves applied at step 1; loads at step 0";

/// The in-process tests start and drop many file watchers; macOS makes a
/// new one wait while they do, so a TUI started with `--follow` alongside
/// can take seconds to draw (more than 20 s seen with the whole file
/// running). These pty tests run one at a time, and no test of this crate
/// starts a watcher in the test process.
static WATCHERS: std::sync::RwLock<()> = std::sync::RwLock::new(());

fn repro(note: bool) -> Value {
    let mut doc = json!({
        "schema": 1,
        "source": {"language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [],
        "moves": [{
            "at_step": 1, "action": "code.patch", "op": "replace",
            "find": "x = 1", "content": "    let x = 2;"
        }]
    });
    if note {
        // Something drawn on the patched code, for the fresh-load checks.
        doc["annotations"] =
            json!([{"id": "a1", "step": 1, "type": "note", "line": 2, "text": "now 2"}]);
    }
    doc
}

/// `main.rs` (OLD) and `doc.tiza` (the repro) in a scratch folder.
fn files(name: &str, note: bool) -> (PathBuf, PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, OLD).unwrap();
    let tiza = dir.join("doc.tiza");
    std::fs::write(&tiza, repro(note).to_string()).unwrap();
    (dir, src, tiza)
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn check_file(path: &Path) -> (i32, Value) {
    let out = tiza_qa::command()
        .args(["check", "--json"])
        .arg(path)
        .output()
        .unwrap();
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)));
    (out.status.code().unwrap_or(-1), v)
}

fn saved_code(tiza: &Path) -> Value {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap();
    v["code"].clone()
}

// ---------- AC4: the real binary on a pty ----------

/// Runs `tiza --socket s.sock doc.tiza` (the repro, no note) on a pty and
/// turns follow on by `mode`: `flag` (`--follow`), `api` (`tiza api
/// follow.set`) or `cmd` (typing `:follow on`). Then: an editor save while
/// waiting, `p` to step 0 (or, with `quit`, `:q` while waiting). Every
/// state is checked against a fresh load of the autosave (AC3). Prints
/// `KEY json` lines; `FAIL …` on a mismatch.
fn pty_run(name: &str, mode: &str, quit: bool) -> (String, PathBuf) {
    let _alone = WATCHERS.write().unwrap_or_else(|e| e.into_inner());
    let (dir, _src, _tiza) = files(name, false);
    let body = r##"
import json, shutil
binary, cwd, mode, quit = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4] == "quit"
os.chdir(cwd)
NEW = 'fn main() {\n    let x = 1;\n    println!("{x}");\n}\n'
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
argv = [binary, "--socket", "s.sock", "doc.tiza"] + (["--follow"] if mode == "flag" else [])
s = Pty(argv, 30, 160, env=env)

def api(method, params=None):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params or {})],
                       capture_output=True, text=True, timeout=10)
    if r.returncode != 0:
        print("API_ERR", method, r.stderr.strip(), flush=True)
        return {}
    return json.loads(r.stdout)

def until(pred, what, timeout=10):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if pred():
            return True
        s.settle(0.05)
    print("WAIT_TIMEOUT", what, flush=True)
    return False

def code():
    st = api("scene.get")
    return st.get("document", st).get("code")

def saved_code():
    try:
        return json.load(open("doc.tiza"))["code"]
    except Exception:
        return None

def steps(args, out):
    shutil.rmtree(out, ignore_errors=True)
    r = subprocess.run([binary, "render"] + args + ["--all-steps", "--format", "txt",
                        "--width", "100", "--out", out + "/"], capture_output=True, text=True, timeout=30)
    if r.returncode != 0:
        print("FAIL render", args, r.stderr.strip(), flush=True)
        return None
    return [open(os.path.join(out, f)).read() for f in sorted(os.listdir(out))]

def same(what):
    live = api("check")
    r = subprocess.run([binary, "check", "--json", "doc.tiza"], capture_output=True, text=True, timeout=30)
    disk = json.loads(r.stdout or "{}")
    for x in (live, disk):
        x.pop("narrower", None)
        x.pop("revision", None)
    if r.returncode != 0 or live != disk:
        print("FAIL check", what, r.returncode, json.dumps(live), json.dumps(disk), flush=True)
    if steps(["--socket", "s.sock"], "render-live") != steps(["doc.tiza"], "render-disk"):
        print("FAIL render", what, flush=True)

ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
if ok:
    if mode == "api":
        print("SET", json.dumps(api("follow.set", {"enabled": True})), flush=True)
    elif mode == "cmd":
        s.send(b":follow on\r")
    else:
        # The first frame draws the bar whole.
        s.wait_for(lambda out: b"(waiting: moves applied at step 1; loads at step 0)" in plain(out).encode(),
                   10, "the bar says it waits")
    until(lambda: api("follow.status").get("enabled") is True, "follow on")
    print("STATUS", json.dumps(api("follow.status")), flush=True)
    same("follow on at step 1")
    tmp = "main.rs.tmp"
    open(tmp, "w").write(NEW)
    os.rename(tmp, "main.rs")
    until(lambda: isinstance(saved_code(), list), "the code embedded while the save waits")
    print("WAITING", json.dumps(api("follow.status")), flush=True)
    print("CODE", json.dumps(code()), flush=True)
    same("a save waits at step 1")
    if quit:
        s.send(b":q\r")
    else:
        s.send(b"p")
        until(lambda: saved_code() == {"file": "main.rs"}, "the reload at step 0")
        print("STEP0", json.dumps(api("follow.status")), flush=True)
        print("CODE", json.dumps(code()), flush=True)
        same("back at step 0")
        s.send(b":q\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            mode,
            if quit { "quit" } else { "p" },
        ],
    );
    for bad in ["WAIT_TIMEOUT", "API_ERR", "FAIL"] {
        assert!(!out.contains(bad), "{mode}: {out}");
    }
    assert!(out.contains("EXIT 0"), "{mode}: {out}");
    (out, dir)
}

fn line_json(out: &str, key: &str) -> Vec<Value> {
    out.lines()
        .filter_map(|l| l.strip_prefix(&format!("{key} ")))
        .map(|j| serde_json::from_str(j).unwrap())
        .collect()
}

fn pty_repro(name: &str, mode: &str) {
    let (out, dir) = pty_run(name, mode, false);
    let st = &line_json(&out, "STATUS")[0];
    assert_eq!(st["enabled"], true, "{st}");
    assert_eq!(st["reason"], WAITING, "{st}");
    let waiting = &line_json(&out, "WAITING")[0];
    assert_eq!(waiting["reason"], WAITING, "{waiting}");
    let step0 = &line_json(&out, "STEP0")[0];
    assert_eq!(step0["reloads"], 1, "{step0}");
    assert!(step0.get("reason").is_none(), "{step0}");
    let codes = line_json(&out, "CODE");
    assert_eq!(codes[0], json!(lines(OLD)), "the screen kept the code");
    assert_eq!(codes[1], json!(lines(NEW)), "step 0 loaded the save");
    if mode == "api" {
        let set = &line_json(&out, "SET")[0];
        assert_eq!(set["message"], format!("following main.rs ({WAITING})"));
    }
    // Quitting at step 0 leaves the file's reference.
    assert_eq!(
        saved_code(&dir.join("doc.tiza")),
        json!({"file": "main.rs"})
    );
}

#[test]
fn pty_the_repro_with_follow() {
    // Test step 1: `tiza --follow doc.tiza` at the last step.
    pty_repro("pty-flag", "flag");
}

#[test]
fn pty_quitting_while_waiting() {
    // `:q` while the save waits: the `.tiza` opens as the screen was (step
    // 1, the old code with the move applied), and the file keeps the save.
    let (_, dir) = pty_run("pty-quit", "flag", true);
    let tiza = dir.join("doc.tiza");
    let (rc, report) = check_file(&tiza);
    assert_eq!(rc, 0, "{report}");
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    assert_eq!(doc.step, 1);
    assert_eq!(doc.base(), lines(OLD).as_slice());
    assert_eq!(doc.code, lines(&OLD.replace("let x = 1;", "let x = 2;")));
    assert_eq!(std::fs::read_to_string(dir.join("main.rs")).unwrap(), NEW);
}
