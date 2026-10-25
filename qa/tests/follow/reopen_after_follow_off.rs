//! Reopening a document whose follow a paste or an undo turned off:
//! `:follow on` and `tiza --follow` said «nothing to follow». The saved
//! `.tiza` held the code on screen embedded and nothing else, the same as
//! after a deliberate `:embed`, so the file it was following was lost on
//! quitting. The document now keeps it in `follows`.
//!
//! Acceptance criteria:
//! - AC1 Reopening a document whose follow was turned off by a paste or an
//!   undo: `:follow on` reloads the file it was following, and `tiza
//!   --follow` on the saved .tiza no longer answers «nothing to follow»;
//!   after a deliberate `:embed` the message says how to link it.
//! - AC5 This file; the document schema; CHANGELOG; the agent guide.
//!
//! Corner cases: an old file with no `follows` behaves as before; a
//! `scene.get` → `scene.put` round trip keeps `follows`, and a put of
//! another document takes that document's own; `:embed`, `:link` and a
//! reload back to the file clear it; `follows` beside a file reference is
//! not written back.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_core::Document;
use tiza_qa::{Live, bin, scratch};

const CODE: &str = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
const SEVEN: &str = "fn main() {\n    let x = 7;\n    println!(\"{x}\");\n}\n";
const PASTED: &str = "fn other() {\n    println!(\"hi\");\n}\n";
const NOTHING: &str =
    "nothing to follow: the code is embedded (use :link path to reference a file)";

const WAIT: Duration = Duration::from_secs(10);

/// The in-process tests start and drop file watchers; macOS makes a new one
/// wait while they do, so the pty test runs alone and the others share.
static WATCHERS: std::sync::RwLock<()> = std::sync::RwLock::new(());

fn shared() -> std::sync::RwLockReadGuard<'static, ()> {
    WATCHERS.read().unwrap_or_else(|e| e.into_inner())
}

/// `main.rs` and its sidecar `main.rs.tiza` in a scratch folder, opened as
/// `tiza main.rs` opens it (a relative reference), following.
fn followed(name: &str) -> (Live, PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, CODE).unwrap();
    let mut doc = Document::from_code(CODE, "rs", Some("main.rs".into()));
    doc.reference_file("main.rs", None);
    doc.step = 0;
    let tiza = dir.join("main.rs.tiza");
    let live = Live::open(doc, tiza.clone(), dir.join("live.sock"));
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    (live, src, tiza)
}

/// The saved `.tiza` opened again, as `tiza main.rs.tiza` opens it.
fn reopen(tiza: &Path, sock: &str) -> Live {
    let doc = tiza_core::fs::load_document(tiza).unwrap();
    Live::open(doc, tiza.to_path_buf(), tiza.with_file_name(sock))
}

fn paste(live: &Live) {
    live.with(|app| app.replace_code(PASTED).unwrap());
    assert!(message(live).contains("follow off"), "{}", message(live));
}

fn status(live: &Live) -> Value {
    live.api_ok("follow.status", json!({}))
}

fn code(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.code.clone())
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn message(live: &Live) -> String {
    live.message().unwrap_or_default().0
}

fn saved(tiza: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap()
}

/// Saves like an editor: a temp file renamed over the target.
fn save(path: &Path, content: &str) {
    let tmp = path.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

// ---------- AC1 ----------

#[test]
fn reopened_after_a_paste_follow_on_reloads_the_file() {
    let _w = shared();
    let (live, _src, tiza) = followed("paste");
    paste(&live);
    let v = saved(&tiza);
    assert_eq!(v["code"], json!(lines(PASTED)), "{v}");
    assert_eq!(v["follows"], json!({"file": "main.rs"}), "{v}");
    drop(live);

    let live = reopen(&tiza, "live2.sock");
    assert_eq!(code(&live), lines(PASTED));
    assert_eq!(
        status(&live)["path"]
            .as_str()
            .map(|p| p.ends_with("main.rs")),
        Some(true)
    );
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(status(&live)["enabled"], true);
    // Back to a reference: nothing to keep.
    let v = saved(&tiza);
    assert_eq!(v["code"], json!({"file": "main.rs"}), "{v}");
    assert!(v.get("follows").is_none(), "{v}");
    // One undo step gives the paste back, and the file stays known.
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(PASTED));
    assert_eq!(status(&live)["enabled"], false);
    assert_eq!(saved(&tiza)["follows"], json!({"file": "main.rs"}));
}

#[test]
fn reopened_after_an_undo_follow_on_reloads_the_file() {
    let _w = shared();
    let (live, src, tiza) = followed("undo");
    // A highlight on the `1` of `x = 1`: the reload drops it, so it is an
    // undo step.
    live.command("hl 2:1 one");
    save(&src, SEVEN);
    wait_for("the reload", || code(&live) == lines(SEVEN));
    // `u` past the reload: the code from before it, which the file no
    // longer holds, stays on screen embedded and follow goes off.
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(status(&live)["enabled"], false);
    let v = saved(&tiza);
    assert_eq!(v["code"], json!(lines(CODE)), "{v}");
    assert_eq!(v["follows"], json!({"file": "main.rs"}), "{v}");
    drop(live);

    let live = reopen(&tiza, "live2.sock");
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    assert_eq!(code(&live), lines(SEVEN));
    assert_eq!(saved(&tiza)["code"], json!({"file": "main.rs"}));
}

#[test]
fn after_embed_the_message_says_how_to_link() {
    let _w = shared();
    for pasted_first in [false, true] {
        let (live, _src, tiza) = followed(&format!("embed-{pasted_first}"));
        if pasted_first {
            paste(&live);
        }
        live.command("embed");
        let v = saved(&tiza);
        assert!(v["code"].is_array(), "{v}");
        assert!(v.get("follows").is_none(), "{pasted_first}: {v}");
        drop(live);

        let live = reopen(&tiza, "live2.sock");
        let shown = code(&live);
        live.command("follow on");
        assert_eq!(live.message(), Some((NOTHING.to_string(), true)));
        assert_eq!(code(&live), shown, "nothing reloads");
        assert_eq!(status(&live)["enabled"], false);
    }
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

// ---------- Corner cases ----------

#[test]
fn an_old_file_with_no_follows_is_unchanged() {
    let _w = shared();
    let dir = scratch("old");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let tiza = dir.join("main.rs.tiza");
    let doc = json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": lines(PASTED),
        "step": 0
    });
    std::fs::write(&tiza, doc.to_string()).unwrap();
    let live = reopen(&tiza, "live.sock");
    live.command("follow on");
    assert_eq!(live.message(), Some((NOTHING.to_string(), true)));
    live.command("note 1 still embedded");
    let v = saved(&tiza);
    assert!(v["code"].is_array() && v.get("follows").is_none(), "{v}");
}

#[test]
fn follows_beside_a_file_reference_is_not_written_back() {
    let _w = shared();
    let dir = scratch("beside");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    std::fs::write(dir.join("other.rs"), SEVEN).unwrap();
    let tiza = dir.join("main.rs.tiza");
    let doc = json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "follows": {"file": "other.rs"},
        "step": 0
    });
    std::fs::write(&tiza, doc.to_string()).unwrap();
    let live = reopen(&tiza, "live.sock");
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    assert_eq!(code(&live), lines(CODE));
    live.command("note 1 a note");
    let v = saved(&tiza);
    assert_eq!(v["code"], json!({"file": "main.rs"}), "{v}");
    assert!(v.get("follows").is_none(), "{v}");
}

#[test]
fn link_and_a_reload_clear_it() {
    let _w = shared();
    let (live, _src, tiza) = followed("link");
    paste(&live);
    assert_eq!(saved(&tiza)["follows"], json!({"file": "main.rs"}));
    live.command("link");
    let v = saved(&tiza);
    assert_eq!(v["code"], json!({"file": "main.rs"}), "{v}");
    assert!(v.get("follows").is_none(), "{v}");
    // A reload back to the file: `:follow on` over a paste.
    live.command("follow on");
    paste(&live);
    assert_eq!(saved(&tiza)["follows"], json!({"file": "main.rs"}));
    live.command("follow on");
    let v = saved(&tiza);
    assert_eq!(v["code"], json!({"file": "main.rs"}), "{v}");
    assert!(v.get("follows").is_none(), "{v}");
}

fn get(live: &Live) -> Value {
    live.api_ok("scene.get", json!({}))["document"].clone()
}

fn put(live: &Live, document: &Value) -> Value {
    let r = live.api("scene.put", json!({ "document": document }));
    assert!(r.error.is_none(), "put refused: {:?}", r.error);
    r.result.unwrap_or(Value::Null)
}

#[test]
fn scene_get_put_round_trip_keeps_it() {
    let _w = shared();
    let (live, _src, tiza) = followed("put");
    paste(&live);
    let doc = get(&live);
    assert_eq!(doc["follows"], json!({"file": "main.rs"}), "{doc}");
    let revision = live.with(|app| app.revision);
    put(&live, &doc);
    assert_eq!(
        live.with(|app| app.revision),
        revision,
        "a put that changes nothing"
    );
    assert_eq!(saved(&tiza)["follows"], json!({"file": "main.rs"}));
    let mut edited = doc.clone();
    edited["title"] = json!("after a put");
    put(&live, &edited);
    let v = saved(&tiza);
    assert_eq!(v["title"], "after a put", "{v}");
    assert_eq!(v["follows"], json!({"file": "main.rs"}), "{v}");
    // Without `follows`, the open document keeps the file it knows.
    let mut bare = edited.clone();
    bare.as_object_mut().unwrap().remove("follows");
    bare["title"] = json!("no follows");
    put(&live, &bare);
    assert_eq!(saved(&tiza)["follows"], json!({"file": "main.rs"}));
    live.command("follow on");
    assert_eq!(code(&live), lines(CODE));
}

#[test]
fn a_put_of_another_document_takes_its_own() {
    let _w = shared();
    for with in [false, true] {
        let (live, _src, _tiza) = followed(&format!("other-{with}"));
        paste(&live);
        let mut other = get(&live);
        other["source"]["path"] = json!("elsewhere.rs");
        if with {
            other["follows"] = json!({"file": "main.rs"});
        } else {
            other.as_object_mut().unwrap().remove("follows");
        }
        put(&live, &other);
        live.command("follow on");
        if with {
            assert_eq!(message(&live), "following main.rs");
            assert_eq!(code(&live), lines(CODE));
        } else {
            assert_eq!(live.message(), Some((NOTHING.to_string(), true)));
            assert_eq!(code(&live), lines(PASTED));
        }
    }
}

// ---------- Review: undo, a failed request, bad values ----------

#[test]
fn a_put_that_changes_only_follows_is_undone_with_u() {
    let _w = shared();
    let (live, src, tiza) = followed("rev-only");
    std::fs::write(src.with_file_name("other.rs"), SEVEN).unwrap();
    paste(&live);
    let mut doc = get(&live);
    doc["follows"] = json!({"file": "other.rs"});
    put(&live, &doc);
    assert_eq!(saved(&tiza)["follows"], json!({"file": "other.rs"}));
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(saved(&tiza)["follows"], json!({"file": "main.rs"}));
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    assert_eq!(code(&live), lines(CODE));
}

#[test]
fn u_back_to_the_first_paste_follows_the_file_it_replaced() {
    let _w = shared();
    // Paste over main.rs, `:ref other.rs`, paste again, `u`, `u`: the first
    // paste is back, and so is the file it replaced.
    let (live, src, tiza) = followed("rev-ref");
    std::fs::write(src.with_file_name("other.rs"), SEVEN).unwrap();
    paste(&live);
    live.command("ref other.rs");
    assert_eq!(code(&live), lines(SEVEN));
    live.with(|app| app.replace_code("fn second() {}\n").unwrap());
    assert_eq!(saved(&tiza)["follows"], json!({"file": "other.rs"}));
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(PASTED));
    assert_eq!(saved(&tiza)["follows"], json!({"file": "main.rs"}));
    assert!(
        status(&live)["reason"]
            .as_str()
            .is_some_and(|r| r.ends_with(":follow on reloads main.rs")),
        "{}",
        status(&live)
    );
    live.command("follow on");
    assert_eq!(code(&live), lines(CODE));
}

#[test]
fn u_of_a_put_of_another_document_brings_the_file_back() {
    let _w = shared();
    let (live, _src, tiza) = followed("rev-other-u");
    paste(&live);
    let mut other = get(&live);
    other["source"]["path"] = json!("elsewhere.rs");
    other.as_object_mut().unwrap().remove("follows");
    put(&live, &other);
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    live.command("autosave on");
    assert_eq!(saved(&tiza)["follows"], json!({"file": "main.rs"}));
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    assert_eq!(code(&live), lines(CODE));
}

#[test]
fn a_bad_follows_is_refused_on_load_and_on_put() {
    let _w = shared();
    let (live, _src, tiza) = followed("rev-bad");
    paste(&live);
    for (bad, says) in [
        (
            json!({"file": "main.rs", "lines": "abc"}),
            "invalid line range in follows.lines: abc",
        ),
        (json!({"file": ""}), "follows.file is empty"),
    ] {
        let mut doc = get(&live);
        doc["follows"] = bad.clone();
        let r = live.api("scene.put", json!({ "document": doc }));
        let err = format!("{:?}", r.error);
        assert!(err.contains(says), "{bad}: {err}");
        assert_eq!(saved(&tiza)["follows"], json!({"file": "main.rs"}), "{bad}");
        let file = tiza.with_file_name(format!("bad-{}.tiza", says.len()));
        let mut v = saved(&tiza);
        v["follows"] = bad.clone();
        std::fs::write(&file, v.to_string()).unwrap();
        let e = tiza_core::fs::load_document(&file).unwrap_err().to_string();
        assert!(e.contains(says), "{bad}: {e}");
    }
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
