//! `--follow` and `follow.set` were refused at a step with moves
//! applied: a `.tiza` opens at its saved `step` (1 when it names none), so
//! with a move at step 1 `tiza --follow doc.tiza` came up with follow off and the bar saying
//! «cannot follow while moves are applied (step 1): go back to step 0
//! first» (Zed's «Tiza: current file, live» task on any document with
//! moves).
//!
//! Acceptance criteria:
//! - AC1 `tiza --follow DOC`, `follow.set {enabled: true}`, the MCP
//!   `set_follow` and `:follow on` are accepted at any step. At a step with
//!   moves applied follow starts waiting (the state a save at such a step already had): a save is
//!   deferred and loads as a normal reload when a step with no moves is
//!   reached; at step 0 it behaves as before.
//! - AC2 The reply and the bar say it is on and waiting in one short line
//!   (`following main.rs; step 0 loads a save`); `follow.status` gives `enabled: true` and says why it waits in
//!   `reason`.
//! - AC3 What is saved always agrees with a fresh load:
//!   `tiza check` on the autosave is clean and `render` equals the screen
//!   at every step, before and after a save.
//! - AC4 This file (pty, condition waits): the repro with `--follow`, with
//!   `follow.set`, with `:follow on`; a save while waiting, then `p` to step
//!   0; quitting while waiting; CHANGELOG (Fixed).
//!
//! Corner cases: step 0 never has a patch (`at_step` 0 is refused when the
//! document loads), so step 0 always loads; `--no-save` writes nothing; the
//! file missing when follow is turned on at step 1 waits for it to come
//! back, then for step 0; a change made while follow was off waits too.
//!
//! Review (`rev_*`): with the first move at step 3 the wait says `loads at
//! step 2 or earlier`; the wait over pasted code does not say the file
//! changed; `u` of that paste leaves nothing to reload; a file back as it
//! was is no change; pasted code numbered otherwise than the file's lines
//! is never saved as a reference with the paste's numbers; `already
//! following` says it waits; `follow.set` keeps an error of its own on the
//! bar.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, scratch};

/// QA 1's repro: a move at step 1, no `step`.
const OLD: &str = "fn main() {\n    let x = 1;\n}\n";
/// The editor's save: the move's `find` still matches.
const NEW: &str = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
const WAITING: &str = "paused; loads at step 0";
/// The reply of `follow on` at step 1, as the bar shows it.
const SAID: &str = "following main.rs; step 0 loads a save";

const WAIT: Duration = Duration::from_secs(10);

/// The in-process tests start and drop many file watchers; macOS makes a
/// new one wait while they do, so a TUI started with `--follow` alongside
/// can take seconds to draw (more than 20 s seen with the whole file
/// running). The pty tests that took it alone are in the watchers crate
/// now; the tests here still share it.
static WATCHERS: std::sync::RwLock<()> = std::sync::RwLock::new(());

fn shared() -> std::sync::RwLockReadGuard<'static, ()> {
    WATCHERS.read().unwrap_or_else(|e| e.into_inner())
}

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

/// The repro opened as the TUI opens it: at step 1, follow off.
fn opened(name: &str) -> (Live, PathBuf, PathBuf) {
    let (dir, src, tiza) = files(name, true);
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza.clone(), dir.join("live.sock"));
    assert_eq!(live.with(|app| app.doc.step), 1);
    (live, src, tiza)
}

fn status(live: &Live) -> Value {
    live.api_ok("follow.status", json!({}))
}

fn base(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.base().to_vec())
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn message(live: &Live) -> String {
    live.message().unwrap_or_default().0
}

fn bar(live: &Live) -> String {
    let screen = live.screen(160, 30);
    screen.lines().last().unwrap_or_default().to_string()
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Saves like an editor: a temp file renamed over the target.
fn save(path: &Path, content: &str) {
    let tmp = path.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

/// At step 1 with follow on, an editor save of `content`: it waits.
fn save_while_waiting(live: &Live, src: &Path, content: &str) {
    live.with(|app| app.message = None);
    save(src, content);
    wait_for("the save to wait for step 0", || {
        status(live)["pending"] == true
    });
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

/// `tiza render SOURCE --all-steps --format txt`: one text per step.
fn all_steps(args: &[&str], out: &Path) -> Vec<String> {
    let _ = std::fs::remove_dir_all(out);
    let r = tiza_qa::command()
        .arg("render")
        .args(args)
        .args(["--all-steps", "--format", "txt", "--width", "100", "--out"])
        .arg(format!("{}/", out.display()))
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "render {args:?}: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let mut files: Vec<PathBuf> = std::fs::read_dir(out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files
        .iter()
        .map(|f| std::fs::read_to_string(f).unwrap())
        .collect()
}

/// AC3: the live instance is what a fresh load of its autosave gives (the
/// same `check` report, `tiza check` exits 0, the same `render
/// --all-steps`), so quitting now leaves exactly this.
fn same_as_fresh(live: &Live, tiza: &Path, what: &str) {
    live.with(|app| app.finish_typing());
    assert!(
        live.with(|app| !app.dirty),
        "{what}: the autosave did not run"
    );
    let mut live_report = live.api_ok("check", json!({}));
    let (rc, mut disk_report) = check_file(tiza);
    assert_eq!(rc, 0, "{what}: tiza check on the autosave: {disk_report}");
    for r in [&mut live_report, &mut disk_report] {
        r.as_object_mut().unwrap().remove("narrower");
        r.as_object_mut().unwrap().remove("revision");
    }
    assert_eq!(live_report, disk_report, "{what}: live check vs fresh load");
    let dir = tiza.parent().unwrap();
    let sock = live.socket.to_str().unwrap().to_string();
    let live_steps = all_steps(&["--socket", &sock], &dir.join("render-live"));
    let disk_steps = all_steps(&[tiza.to_str().unwrap()], &dir.join("render-disk"));
    assert_eq!(live_steps, disk_steps, "{what}: render --all-steps");
}

fn saved_code(tiza: &Path) -> Value {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap();
    v["code"].clone()
}

/// Test step 2 after follow went on at step 1: the save waits (the screen
/// keeps the code, the autosave embeds it), `step 0` reloads it as one
/// reload, and every state is what a fresh load gives.
fn save_then_step_0(live: &Live, src: &Path, tiza: &Path) {
    same_as_fresh(live, tiza, "follow on at step 1");
    save_while_waiting(live, src, NEW);
    assert_eq!(base(live), lines(OLD), "the screen keeps the code");
    assert!(saved_code(tiza).is_array(), "{}", saved_code(tiza));
    assert_eq!(status(live)["enabled"], true);
    assert_eq!(status(live)["reason"], WAITING);
    same_as_fresh(live, tiza, "a save waits at step 1");
    live.command("step 0");
    assert_eq!(base(live), lines(NEW));
    let st = status(live);
    assert_eq!(st["reloads"], 1, "{st}");
    assert!(st.get("reason").is_none(), "{st}");
    assert!(
        message(live).starts_with("main.rs reloaded (#1)"),
        "{}",
        message(live)
    );
    assert_eq!(saved_code(tiza), json!({"file": "main.rs"}));
    same_as_fresh(live, tiza, "back at step 0");
    // The move applies on the new code.
    live.command("step 1");
    assert_eq!(
        live.with(|app| app.doc.code.clone()),
        lines(&NEW.replace("let x = 1;", "let x = 2;"))
    );
    same_as_fresh(live, tiza, "step 1 on the new code");
}

// ---------- AC1, AC2: every way of turning it on, at step 1 ----------

#[test]
fn follow_on_at_step_1_waits_and_says_so() {
    let _watchers = shared();
    let (live, src, tiza) = opened("cmd");
    live.command("follow on");
    let said = SAID.to_string();
    assert_eq!(message(&live), said);
    assert!(bar(&live).contains(&said), "{}", bar(&live));
    assert!(bar(&live).contains("⟳ paused"), "{}", bar(&live));
    let st = status(&live);
    assert_eq!(st["enabled"], true, "{st}");
    assert_eq!(st["reason"], WAITING, "{st}");
    live.command("follow");
    assert_eq!(
        message(&live),
        format!("following main.rs (0 reloads; {WAITING})")
    );
    save_then_step_0(&live, &src, &tiza);
}

#[test]
fn follow_set_at_step_1_waits_and_says_so() {
    let _watchers = shared();
    let (live, src, tiza) = opened("api");
    let r = live.api_ok("follow.set", json!({"enabled": true}));
    let said = SAID.to_string();
    assert_eq!(r["message"], said, "{r}");
    assert_eq!(r["enabled"], true, "{r}");
    assert_eq!(r["reason"], WAITING, "{r}");
    // The presenter is told too: a save would otherwise seem lost.
    assert_eq!(message(&live), said);
    save_then_step_0(&live, &src, &tiza);
}

#[test]
fn mcp_set_follow_at_step_1_waits() {
    let _watchers = shared();
    let (live, src, tiza) = opened("mcp");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("set_follow", json!({"enabled": true}));
    assert!(!err, "{text}");
    assert!(text.contains(WAITING), "{text}");
    assert_eq!(status(&live)["enabled"], true);
    save_then_step_0(&live, &src, &tiza);
}

#[test]
fn at_step_0_as_before() {
    let _watchers = shared();
    let (live, src, tiza) = opened("step0");
    live.command("step 0");
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    assert!(status(&live).get("reason").is_none());
    save(&src, NEW);
    wait_for("the reload", || base(&live) == lines(NEW));
    assert_eq!(status(&live)["reloads"], 1);
    // Stepping to 1 now: waiting, said by `follow.status`.
    live.command("step 1");
    assert_eq!(status(&live)["reason"], WAITING);
    same_as_fresh(&live, &tiza, "reloaded at step 0, then step 1");
}

#[test]
fn a_change_made_while_follow_was_off_waits_too() {
    let _watchers = shared();
    // The file changed while follow was off: turning it on at step 1 does
    // not load it there; it waits like a save, and step 0 loads it.
    let (live, src, tiza) = opened("off-change");
    save(&src, NEW);
    let r = live.api_ok("follow.set", json!({"enabled": true}));
    assert_eq!(r["message"], SAID, "{r}");
    assert_eq!(base(&live), lines(OLD));
    assert!(saved_code(&tiza).is_array(), "{}", saved_code(&tiza));
    same_as_fresh(&live, &tiza, "the change waits");
    live.command("step 0");
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(status(&live)["reloads"], 1);
    same_as_fresh(&live, &tiza, "loaded at step 0");
    // One undo step gives the code from before back.
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(base(&live), lines(OLD));
    same_as_fresh(&live, &tiza, "u after the reload");
}

// ---------- corner cases ----------

#[test]
fn step_0_never_has_a_patch() {
    let _watchers = shared();
    // So a save always loads at step 0: a move at step 0 does not load.
    let (_, _, tiza) = files("step0-patch", false);
    let mut doc = repro(false);
    doc["moves"][0]["at_step"] = json!(0);
    std::fs::write(&tiza, doc.to_string()).unwrap();
    let err = tiza_core::fs::load_document(&tiza).unwrap_err().to_string();
    assert!(err.contains("at_step must be >= 1"), "{err}");
}

#[test]
fn no_save_writes_nothing() {
    let _watchers = shared();
    let (live, src, tiza) = opened("no-save");
    live.with(|app| app.autosave = false);
    let before = std::fs::read(&tiza).unwrap();
    live.command("follow on");
    save_while_waiting(&live, &src, NEW);
    assert_eq!(base(&live), lines(OLD));
    live.command("step 0");
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(std::fs::read(&tiza).unwrap(), before);
}

#[test]
fn the_file_missing_at_step_1() {
    let _watchers = shared();
    let (live, src, tiza) = opened("missing");
    std::fs::remove_file(&src).unwrap();
    live.command("follow on");
    assert_eq!(message(&live), SAID);
    let st = status(&live);
    assert_eq!(
        (st["enabled"].clone(), st["missing"].clone()),
        (json!(true), json!(true))
    );
    assert!(
        bar(&live).contains("⟳ paused (file removed)"),
        "{}",
        bar(&live)
    );
    // Back with other code: it waits for step 0.
    save_while_waiting(&live, &src, NEW);
    assert_eq!(base(&live), lines(OLD));
    same_as_fresh(&live, &tiza, "back at step 1");
    live.command("step 0");
    assert_eq!(base(&live), lines(NEW));
    same_as_fresh(&live, &tiza, "loaded at step 0");
}

#[test]
fn pasted_code_then_step_1_then_follow_on() {
    let _watchers = shared();
    // Pasted over the file at step 0 (follow goes off), then step 1: `:follow
    // on` there waits, and step 0 loads the file over the paste as one undo.
    let (live, _src, tiza) = opened("paste");
    live.command("step 0");
    live.command("follow on");
    let pasted = "fn main() {\n    let x = 1;\n    let y = 3;\n}\n";
    live.with(move |app| app.replace_code(pasted)).unwrap();
    assert_eq!(status(&live)["enabled"], false);
    live.command("step 1");
    live.command("follow on");
    assert_eq!(status(&live)["enabled"], true);
    assert_eq!(base(&live), lines(pasted));
    same_as_fresh(&live, &tiza, "follow on over the paste at step 1");
    live.command("step 0");
    assert_eq!(base(&live), lines(OLD));
    same_as_fresh(&live, &tiza, "the file over the paste at step 0");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(base(&live), lines(pasted));
    same_as_fresh(&live, &tiza, "u gives the paste back");
}

// ---------- review ----------

/// A scratch folder with `main.rs` = `code` and `doc.tiza` = `doc`, opened.
fn opened_doc(name: &str, doc: Value, code: &str) -> (Live, PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, code).unwrap();
    let tiza = dir.join("doc.tiza");
    std::fs::write(&tiza, doc.to_string()).unwrap();
    let loaded = tiza_core::fs::load_document(&tiza).unwrap();
    (
        Live::open(loaded, tiza.clone(), dir.join("live.sock")),
        src,
        tiza,
    )
}

/// The first line of the first annotation.
fn a1_line(live: &Live) -> u32 {
    live.with(|app| app.doc.annotations[0].shape.line_ranges()[0].0)
}

#[test]
fn a_first_move_at_step_3_loads_at_step_2() {
    let _watchers = shared();
    let mut doc = repro(false);
    doc["moves"][0]["at_step"] = json!(3);
    doc["step"] = json!(3);
    doc["annotations"] = json!([
        {"id": "a1", "step": 1, "type": "note", "line": 1, "text": "one"},
        {"id": "a2", "step": 2, "type": "note", "line": 1, "text": "two"}
    ]);
    let (live, src, tiza) = opened_doc("step3", doc, OLD);
    live.command("follow on");
    let waits = "paused; loads at step 2 or earlier";
    assert_eq!(message(&live), "following main.rs; step 2 loads a save");
    assert_eq!(status(&live)["reason"], waits);
    live.with(|app| app.message = None);
    save(&src, NEW);
    wait_for("the save to wait", || status(&live)["pending"] == true);
    same_as_fresh(&live, &tiza, "waiting at step 3");
    live.command("step 2");
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(status(&live)["reloads"], 1);
    same_as_fresh(&live, &tiza, "loaded at step 2");
}

/// QA's repro with `a1`, pasted over at step 0 (follow goes off), at step 1.
fn pasted_at_step_1(name: &str) -> (Live, PathBuf) {
    let (live, _src, tiza) = opened(name);
    live.command("step 0");
    live.command("follow on");
    let pasted = "fn main() {\n    let x = 1;\n    let y = 3;\n}\n";
    live.with(move |app| app.replace_code(pasted)).unwrap();
    assert_eq!(status(&live)["enabled"], false);
    live.command("step 1");
    (live, tiza)
}

#[test]
fn over_a_paste_the_wait_does_not_say_the_file_changed() {
    let _watchers = shared();
    let (live, tiza) = pasted_at_step_1("paste-words");
    live.with(|app| app.message = None);
    live.command("follow on");
    assert_eq!(message(&live), SAID);
    assert_eq!(status(&live)["reason"], WAITING);
    same_as_fresh(&live, &tiza, ":follow on over the paste");
    live.command("follow off");
    live.with(|app| app.message = None);
    let r = live.api_ok("follow.set", json!({"enabled": true}));
    assert_eq!(r["message"], SAID, "{r}");
    assert_eq!(message(&live), SAID);
}

#[test]
fn u_of_the_paste_while_waiting_leaves_nothing_to_reload() {
    let _watchers = shared();
    // The paste undone at step 1: the file's code again, a reference, so
    // step 0 has nothing to load (no reload, no undo step that does nothing).
    let (live, tiza) = pasted_at_step_1("paste-u");
    live.command("follow on");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(base(&live), lines(OLD));
    assert!(live.with(|app| !app.doc.is_embedded()));
    assert_eq!(status(&live)["enabled"], true);
    same_as_fresh(&live, &tiza, "u of the paste at step 1");
    live.command("step 0");
    assert_eq!(status(&live)["reloads"], 0, "{}", message(&live));
    assert_eq!(base(&live), lines(OLD));
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    same_as_fresh(&live, &tiza, "step 0 after the u");
}

#[test]
fn the_file_back_unchanged_at_step_1_is_no_change() {
    let _watchers = shared();
    let (live, src, tiza) = opened("back-same");
    std::fs::remove_file(&src).unwrap();
    live.api_ok("follow.set", json!({"enabled": true}));
    assert_eq!(status(&live)["missing"], true);
    save(&src, OLD);
    wait_for("the file back", || status(&live)["missing"] == false);
    assert_eq!(message(&live), "main.rs is back; following it");
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    same_as_fresh(&live, &tiza, "back unchanged at step 1");
    live.command("step 0");
    assert_eq!(status(&live)["reloads"], 0);
    same_as_fresh(&live, &tiza, "step 0");
}

/// A range of the file (`lines 2-5`) pasted over at step 0 with a changed
/// line, numbered from 1 (follow goes off); the file then edited so that
/// lines 2-5 hold the paste.
fn ranged_paste(name: &str) -> (Live, PathBuf) {
    let six = "// head\nfn main() {\n    let x = 1;\n    let y = 2;\n}\n// tail\n";
    let mut doc = repro(false);
    doc["source"]["first_line"] = json!(2);
    doc["code"] = json!({"file": "main.rs", "lines": "2-5"});
    doc["annotations"] = json!([{"id": "a1", "step": 1, "type": "note", "line": 3, "text": "x"}]);
    let (live, src, tiza) = opened_doc(name, doc, six);
    live.command("step 0");
    live.command("follow on");
    let pasted = "fn main() {\n    let x = 1;\n    let y = 3;\n}\n";
    live.with(move |app| app.replace_code(pasted)).unwrap();
    assert_eq!(live.with(|app| app.doc.base_first()), 1);
    // a1 went with its text: the paste's second line.
    assert_eq!(a1_line(&live), 2);
    std::fs::write(
        &src,
        "// head\nfn main() {\n    let x = 1;\n    let y = 3;\n}\n// tail\n",
    )
    .unwrap();
    (live, tiza)
}

#[test]
fn a_paste_the_file_holds_on_other_lines_loads_with_the_files_numbers() {
    let _watchers = shared();
    // At step 1 it waits (the paste stays, embedded with its numbers); step
    // 0 loads the file's lines, numbered from 2, and a1 stays on its text.
    let (live, tiza) = ranged_paste("range-1");
    live.command("step 1");
    live.command("follow on");
    assert_eq!(status(&live)["reason"], WAITING);
    assert!(live.with(|app| app.doc.is_embedded()));
    same_as_fresh(&live, &tiza, "follow on over the paste at step 1");
    live.command("step 0");
    assert_eq!(live.with(|app| app.doc.base_first()), 2);
    assert_eq!(a1_line(&live), 3);
    assert_eq!(
        saved_code(&tiza),
        json!({"file": "main.rs", "lines": "2-5"})
    );
    same_as_fresh(&live, &tiza, "loaded at step 0");
}

#[test]
fn a_paste_the_file_holds_on_other_lines_at_step_0() {
    let _watchers = shared();
    let (live, tiza) = ranged_paste("range-0");
    live.command("follow on");
    assert_eq!(live.with(|app| app.doc.base_first()), 2);
    assert_eq!(a1_line(&live), 3);
    assert_eq!(
        saved_code(&tiza),
        json!({"file": "main.rs", "lines": "2-5"})
    );
    same_as_fresh(&live, &tiza, "follow on over the paste at step 0");
}

#[test]
fn follow_set_keeps_an_error_of_its_own_on_the_bar() {
    let _watchers = shared();
    let (live, src, _tiza) = opened("utf8");
    std::fs::write(&src, b"fn main() {\n    \xff\xfe\n}\n").unwrap();
    let r = live.api_ok("follow.set", json!({"enabled": true}));
    assert_eq!(r["enabled"], true, "{r}");
    let (msg, loud) = live.message().unwrap();
    assert!(loud && msg.contains("not valid UTF-8"), "{msg}");
}
