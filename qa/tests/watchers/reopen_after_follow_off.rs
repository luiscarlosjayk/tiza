//! `--follow` on a document that a paste or an undo left with follow off, and
//! a put that panics, on the real binary in a pty.

use std::path::PathBuf;

use serde_json::{Value, json};
use tiza_qa::{bin, scratch};

const CODE: &str = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
const SEVEN: &str = "fn main() {\n    let x = 7;\n    println!(\"{x}\");\n}\n";
const PASTED: &str = "fn other() {\n    println!(\"hi\");\n}\n";

/// The in-process tests start and drop file watchers; macOS makes a new one
/// wait while they do. These pty tests run one at a time, and no test of
/// this crate starts a watcher in the test process.
static WATCHERS: std::sync::RwLock<()> = std::sync::RwLock::new(());

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

/// `tiza --socket s.sock --follow main.rs.tiza` on a pty, over a sidecar
/// saved after a paste (`follows`) or as an old file wrote it (none).
/// Prints `KEY json` lines.
fn pty_follow_flag(name: &str, follows: bool) -> (String, PathBuf) {
    let _alone = WATCHERS.write().unwrap_or_else(|e| e.into_inner());
    let dir = scratch(name);
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let mut doc = json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": lines(PASTED),
        "step": 0
    });
    if follows {
        doc["follows"] = json!({"file": "main.rs"});
    }
    std::fs::write(dir.join("main.rs.tiza"), doc.to_string()).unwrap();
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--socket", "s.sock", "--follow", "main.rs.tiza"], 30, 160, env=env)

def api(method, params=None):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params or {})],
                       capture_output=True, text=True, timeout=10)
    if r.returncode != 0:
        print("API_ERR", method, r.stderr.strip(), flush=True)
        return {}
    return json.loads(r.stdout)

ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
if ok:
    s.wait_for(lambda out: b"following" in plain(out).encode() or b"nothing to follow" in plain(out).encode(),
               10, "the bar says")
    print("NOTHING", json.dumps("nothing to follow" in plain(s.out)), flush=True)
    print("STATUS", json.dumps(api("follow.status")), flush=True)
    print("CODE", json.dumps(api("scene.get")["document"]["code"]), flush=True)
    print("SAVED", json.dumps(json.load(open("main.rs.tiza"))), flush=True)
    s.send(b":q\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(!out.contains("API_ERR"), "{out}");
    assert!(out.contains("EXIT 0"), "{out}");
    (out, dir)
}

fn line_json(out: &str, key: &str) -> Value {
    out.lines()
        .find_map(|l| l.strip_prefix(&format!("{key} ")))
        .map(|j| serde_json::from_str(j).unwrap())
        .unwrap_or_else(|| panic!("no {key} in {out}"))
}

#[test]
fn pty_follow_flag_on_the_reopened_file() {
    let (out, _dir) = pty_follow_flag("pty", true);
    let st = line_json(&out, "STATUS");
    assert_eq!(st["enabled"], true, "{st}");
    assert_eq!(line_json(&out, "NOTHING"), false, "{out}");
    assert_eq!(line_json(&out, "CODE"), json!(lines(CODE)), "{out}");
    let v = line_json(&out, "SAVED");
    assert_eq!(v["code"], json!({"file": "main.rs"}), "{v}");
    assert!(v.get("follows").is_none(), "{v}");
    // An old file with no `follows`: as before.
    let (out, _dir) = pty_follow_flag("pty-old", false);
    assert_eq!(line_json(&out, "STATUS")["enabled"], false, "{out}");
    assert_eq!(line_json(&out, "CODE"), json!(lines(PASTED)), "{out}");
    assert_eq!(line_json(&out, "NOTHING"), true, "{out}");
}

#[test]
fn pty_a_put_that_panics_leaves_follows_as_it_was() {
    let _alone = WATCHERS.write().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("rev-panic");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    std::fs::write(dir.join("other.rs"), SEVEN).unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": lines(PASTED),
        "follows": {"file": "main.rs"},
        "step": 0
    });
    std::fs::write(dir.join("main.rs.tiza"), doc.to_string()).unwrap();
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="request:scene.put")
s = Pty([binary, "--socket", "s.sock", "main.rs.tiza"], 30, 160, env=env)

def api(method, params=None):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params or {})],
                       capture_output=True, text=True, timeout=10)
    return r.returncode, r.stdout + r.stderr

if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    doc = json.loads(api("scene.get")[1])["document"]
    same = dict(doc, follows={"file": "other.rs"}, title="kept?")
    print("PUT_SAME", json.dumps(api("scene.put", {"document": same})[0]), flush=True)
    print("SAVED", json.dumps(json.load(open("main.rs.tiza"))), flush=True)
    other = dict(doc, source=dict(doc["source"], path="elsewhere.rs"))
    other.pop("follows", None)
    print("PUT_OTHER", json.dumps(api("scene.put", {"document": other})[0]), flush=True)
    print("SAVED", json.dumps(json.load(open("main.rs.tiza"))), flush=True)
    print("STATUS", json.dumps(json.loads(api("follow.status")[1])), flush=True)
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "panic.py",
        body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(out.contains("EXIT 0"), "{out}");
    // Both puts failed (the hook), and nothing of them was kept.
    assert!(
        out.contains("PUT_SAME 1") && out.contains("PUT_OTHER 1"),
        "{out}"
    );
    for saved in out.lines().filter_map(|l| l.strip_prefix("SAVED ")) {
        let v: Value = serde_json::from_str(saved).unwrap();
        assert_eq!(v["follows"], json!({"file": "main.rs"}), "{out}");
        assert!(v.get("title").is_none(), "{out}");
    }
    let st = line_json(&out, "STATUS");
    assert!(
        st["path"].as_str().is_some_and(|p| p.ends_with("main.rs")),
        "{out}"
    );
}
