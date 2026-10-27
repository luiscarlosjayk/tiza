//! A follow reload in a document with a `code.link` moved two
//! annotations, and the autosave wrote them to the `.tiza`.
//!
//! The reload re-added each re-anchored annotation through `Document::add`,
//! which gives an annotation the defaults of a new one: `step: 0` became
//! `step: 1` (not drawn at step 0 any more), and an annotation of the main
//! file at a step that shows a linked file got `file:` that linked file
//! (`m5` at step 5 was drawn over the `}` of `src/queue.rs`).
//!
//! Acceptance criteria:
//! - AC1 A follow reload, at any step, deferred or not, keeps the `step` and
//!   the `file` of every annotation. Only the position changes, by the
//!   re-anchoring of lines. Step-0 annotations stay at step 0; annotations of
//!   the main file stay in it, whatever file their step shows.
//! - AC2 The same on every path that rebuilds the document from its
//!   annotations: paste, `:ref`, undo/redo of a reload, a `scene.put` round
//!   trip. None goes through the defaults of `Document::add`.
//! - AC3 A `.tiza` already saved with the moved annotations is not repaired:
//!   it is the author's data.
//! - AC4 This file: save at step 0 and at a step with the link; both
//!   properties in the saved `.tiza`, on screen and in `scene.get`.
//!
//! Corner cases: an annotation that cannot be re-anchored is dropped, as
//! before; a document without `code.link` does not change.
//!
//! The fixture (from the QA of the linked files): `n0` at step 0 on main.rs L9, `q3` at step 3
//! on src/queue.rs, `m5` at step 5 on main.rs L3 while step 5 shows
//! src/queue.rs (step 6 shows main.rs again). The editor save adds a line
//! at the top of main.rs, so the main-file annotations move down one line.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_qa::{Live, fixture, scratch};

const WAIT: Duration = Duration::from_secs(10);
const QUEUE: &str = "src/queue.rs";

/// (id, step, file, line) of each annotation, in document order.
type Place = (String, u64, Option<String>, u64);

fn place(id: &str, step: u64, file: Option<&str>, line: u64) -> Place {
    (id.into(), step, file.map(str::to_string), line)
}

/// The fixture as the author saved it.
fn authored() -> Vec<Place> {
    vec![
        place("n0", 0, None, 9),
        place("q3", 3, Some(QUEUE), 2),
        place("m5", 5, None, 3),
    ]
}

/// After the editor save: one line more at the top of main.rs.
fn reloaded() -> Vec<Place> {
    vec![
        place("n0", 0, None, 10),
        place("q3", 3, Some(QUEUE), 2),
        place("m5", 5, None, 4),
    ]
}

fn places(annotations: &Value) -> Vec<Place> {
    annotations
        .as_array()
        .expect("annotations")
        .iter()
        .map(|a| {
            (
                a["id"].as_str().unwrap().to_string(),
                a["step"].as_u64().unwrap_or(0),
                a.get("file").and_then(Value::as_str).map(str::to_string),
                a["line"].as_u64().unwrap(),
            )
        })
        .collect()
}

fn saved(tiza: &Path) -> Vec<Place> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap();
    places(&v["annotations"])
}

fn scene(live: &Live) -> Vec<Place> {
    places(&live.api_ok("scene.get", json!({}))["document"]["annotations"])
}

/// A copy of the fixture files in a scratch folder, `name` opened from it.
fn copy(dir_name: &str, name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(dir_name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for f in ["main.rs", QUEUE, name] {
        std::fs::copy(
            fixture(&format!("follow/reload_with_code_link/{f}")),
            dir.join(f),
        )
        .unwrap();
    }
    let tiza = dir.join(name);
    (dir, tiza)
}

fn open(dir: &Path, tiza: &Path) -> Live {
    let doc = tiza_core::fs::load_document(tiza).unwrap();
    Live::open(doc, tiza.to_path_buf(), dir.join("live.sock"))
}

/// `demo.tiza` (or `name`) opened at step 0 with follow on.
fn followed(dir_name: &str, name: &str) -> (Live, PathBuf, PathBuf) {
    let (dir, tiza) = copy(dir_name, name);
    let live = open(&dir, &tiza);
    live.command("follow on");
    assert_eq!(live.message().unwrap().0, "following main.rs");
    (live, dir.join("main.rs"), tiza)
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn reloads(live: &Live) -> u64 {
    live.api_ok("follow.status", json!({}))["reloads"]
        .as_u64()
        .unwrap_or(0)
}

/// The editor's save: a temp file renamed over main.rs, with one line more
/// at the top.
fn edit(src: &Path) {
    let text = std::fs::read_to_string(src).unwrap();
    let tmp = src.with_extension("rs.tmp");
    std::fs::write(&tmp, format!("// v2\n{text}")).unwrap();
    std::fs::rename(&tmp, src).unwrap();
}

fn goto(live: &Live, step: u64) {
    live.api_ok("step.set", json!({ "step": step }));
    wait_for(&format!("step {step}"), || {
        live.api_ok("status", json!({}))["step"].as_u64() == Some(step)
    });
}

fn screen(live: &Live) -> String {
    live.screen(120, 30)
}

/// The screen row that draws `label`.
fn row_of(screen: &str, label: &str) -> Option<String> {
    screen
        .lines()
        .find(|l| l.contains(&format!("┆ {label}")))
        .map(str::to_string)
}

/// On screen, after the reload: `n0` at step 0 on `println!` (L10), `m5`
/// not drawn at step 5 (src/queue.rs is shown), and drawn at step 6 on its
/// own main.rs line, the one the step-5 patch wrote.
fn assert_screen(live: &Live) {
    goto(live, 0);
    let s = screen(live);
    assert!(s.contains("// v2"), "the reload is on screen:\n{s}");
    let n0 = row_of(&s, "prints it").unwrap_or_else(|| panic!("n0 at step 0:\n{s}"));
    assert!(n0.contains("println!"), "n0 on its line:\n{s}");
    assert!(n0.trim_start().starts_with("10 "), "n0 at L10:\n{s}");
    goto(live, 5);
    let s = screen(live);
    assert!(
        s.contains("pub struct Queue"),
        "step 5 shows queue.rs:\n{s}"
    );
    assert!(row_of(&s, "the items").is_some(), "q3 at step 5:\n{s}");
    assert!(
        row_of(&s, "moved").is_none(),
        "m5 is of main.rs: not drawn over queue.rs:\n{s}"
    );
    goto(live, 6);
    let s = screen(live);
    let m5 = row_of(&s, "moved").unwrap_or_else(|| panic!("m5 at step 6:\n{s}"));
    assert!(
        m5.contains("takes(s); // moved here"),
        "m5 on main.rs L4:\n{s}"
    );
    goto(live, 0);
}

/// Saves main.rs at `step`, then goes to step 0 when the reload has not
/// happened there (it waits while a patch is applied).
fn save_at(step: u64) {
    let (live, src, tiza) = followed(&format!("save-{step}"), "demo.tiza");
    assert_eq!(saved(&tiza), authored());
    goto(&live, step);
    edit(&src);
    goto(&live, 0);
    wait_for("the reload", || reloads(&live) >= 1);
    assert_eq!(saved(&tiza), reloaded(), "the saved .tiza");
    assert_eq!(scene(&live), reloaded(), "scene.get");
    assert_screen(&live);
    // A fresh load of the autosave keeps them too.
    let again = tiza_core::fs::load_document(&tiza).unwrap();
    let again = serde_json::to_value(&again.annotations).unwrap();
    assert_eq!(places(&again), reloaded(), "a fresh load");
}

#[test]
fn save_at_step_0_keeps_step_and_file() {
    save_at(0);
}

#[test]
fn save_at_step_5_where_queue_rs_is_shown() {
    save_at(5);
}

#[test]
fn save_at_step_3_on_the_link() {
    save_at(3);
}

#[test]
fn undo_and_redo_of_the_reload() {
    let (live, src, tiza) = followed("undo", "demo.tiza");
    edit(&src);
    wait_for("the reload", || reloads(&live) >= 1);
    assert_eq!(saved(&tiza), reloaded());
    live.with(|app| app.undo());
    assert_eq!(scene(&live), authored(), "undo");
    assert_eq!(saved(&tiza), authored(), "undo, saved");
    live.with(|app| app.redo());
    assert_eq!(scene(&live), reloaded(), "redo");
    assert_eq!(saved(&tiza), reloaded(), "redo, saved");
    assert_screen(&live);
}

#[test]
fn paste_at_step_0() {
    let (dir, tiza) = copy("paste", "demo.tiza");
    let live = open(&dir, &tiza);
    let text = format!(
        "// v2\n{}",
        std::fs::read_to_string(dir.join("main.rs")).unwrap()
    );
    live.with(move |app| app.replace_code(&text)).unwrap();
    assert_eq!(scene(&live), reloaded());
    assert_eq!(saved(&tiza), reloaded());
    assert_screen(&live);
}

#[test]
fn ref_of_the_changed_file() {
    let (dir, tiza) = copy("ref", "demo.tiza");
    let live = open(&dir, &tiza);
    edit(&dir.join("main.rs"));
    live.command("ref main.rs");
    let said = live.message().unwrap().0;
    assert!(said.contains("references main.rs"), "{said}");
    assert_eq!(scene(&live), reloaded());
    assert_eq!(saved(&tiza), reloaded());
    assert_screen(&live);
}

#[test]
fn scene_put_round_trip() {
    let (live, src, tiza) = followed("put", "demo.tiza");
    edit(&src);
    wait_for("the reload", || reloads(&live) >= 1);
    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
    doc["title"] = json!("Put back");
    live.api_ok("scene.put", json!({ "document": doc }));
    assert_eq!(scene(&live), reloaded());
    assert_eq!(saved(&tiza), reloaded());
    assert_screen(&live);
    live.with(|app| app.undo());
    assert_eq!(scene(&live), reloaded(), "undo of the put");
}

#[test]
fn a_file_saved_with_moved_annotations_stays_as_saved() {
    let moved = vec![
        place("n0", 1, None, 9),
        place("q3", 3, Some(QUEUE), 2),
        place("m5", 5, Some(QUEUE), 3),
    ];
    let (live, src, tiza) = followed("moved", "moved.tiza");
    assert_eq!(saved(&tiza), moved);
    edit(&src);
    wait_for("the reload", || reloads(&live) >= 1);
    let after = vec![
        place("n0", 1, None, 10),
        place("q3", 3, Some(QUEUE), 2),
        place("m5", 5, Some(QUEUE), 3),
    ];
    assert_eq!(saved(&tiza), after);
    assert_eq!(scene(&live), after);
}

#[test]
fn an_annotation_without_its_text_is_dropped() {
    let (live, src, tiza) = followed("dropped", "demo.tiza");
    // L9 (`println!`) goes away: n0 has no text to go to.
    let text = std::fs::read_to_string(&src)
        .unwrap()
        .replace("    println!(\"{s}\");\n", "");
    let tmp = src.with_extension("rs.tmp");
    std::fs::write(&tmp, text).unwrap();
    std::fs::rename(&tmp, &src).unwrap();
    wait_for("the reload", || reloads(&live) >= 1);
    let left = vec![place("q3", 3, Some(QUEUE), 2), place("m5", 5, None, 3)];
    assert_eq!(scene(&live), left);
    assert_eq!(saved(&tiza), left);
}

#[test]
fn a_document_without_code_link_does_not_change() {
    let (live, src, tiza) = followed("plain", "plain.tiza");
    edit(&src);
    wait_for("the reload", || reloads(&live) >= 1);
    let after = vec![place("n0", 0, None, 10), place("n2", 2, None, 4)];
    assert_eq!(saved(&tiza), after);
    assert_eq!(scene(&live), after);
    let s = screen(&live);
    let n0 = row_of(&s, "prints it").unwrap_or_else(|| panic!("n0 at step 0:\n{s}"));
    assert!(n0.contains("println!"), "{s}");
}

/// A save at a step where only moves that do not edit the text apply (a
/// cursor, a selection, the link to queue.rs, the link back to main.rs)
/// loads at once, as at step 0. The view of the step stays up, and step 0
/// shows the new main.rs from line 1.
#[test]
fn a_save_at_a_step_with_no_patch_loads_at_once() {
    for step in 1..=4u64 {
        let (live, src, tiza) = followed(&format!("now-{step}"), "demo.tiza");
        goto(&live, step);
        let before = live.api_ok("status", json!({}));
        edit(&src);
        wait_for("the reload", || reloads(&live) >= 1);
        let after = live.api_ok("status", json!({}));
        assert_eq!(after["step"], step, "loaded at step {step}");
        assert_eq!(after["view"], before["view"], "step {step}: the view");
        assert_eq!(after["file"], before["file"], "step {step}: the file");
        assert_eq!(live.with(|app| app.doc.base_first()), 1, "step {step}");
        assert_eq!(saved(&tiza), reloaded(), "step {step}: the saved .tiza");
        assert_eq!(scene(&live), reloaded(), "step {step}: scene.get");
        if step == 3 {
            let s = screen(&live);
            assert!(s.contains("pub struct Queue"), "queue.rs stays:\n{s}");
        }
        assert_screen(&live);
    }
}

/// A window of main.rs (lines 7-9) shown at step 1 numbers the code on
/// screen from line 7. A save there keeps the numbers of the file: step 0
/// starts at line 1, and the window shows lines 7-9 of the new text.
#[test]
fn a_save_while_a_window_of_main_rs_is_shown() {
    let dir = scratch("window");
    std::fs::copy(
        fixture("follow/reload_with_code_link/main.rs"),
        dir.join("main.rs"),
    )
    .unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [],
        "step": 0,
        "moves": [{"at_step": 1, "action": "code.link", "file": "main.rs", "lines": "7-9"}],
    });
    let tiza = dir.join("window.tiza");
    std::fs::write(&tiza, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let live = open(&dir, &tiza);
    live.command("follow on");
    goto(&live, 1);
    assert_eq!(live.api_ok("status", json!({}))["first_line"], 7);
    edit(&dir.join("main.rs"));
    wait_for("the reload", || reloads(&live) >= 1);
    let st = live.api_ok("status", json!({}));
    assert_eq!(
        (st["step"].clone(), st["first_line"].clone()),
        (json!(1), json!(7))
    );
    let code = live.api_ok("code.get", json!({}));
    // The new line at the top moved `fn takes` from L8 to L9.
    let text = code["text"].as_str().unwrap_or_default().to_string();
    assert!(text.contains("   9  fn takes(s: String) {"), "{code}");
    goto(&live, 0);
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["first_line"], 1, "{st}");
    let s = screen(&live);
    let first = s.lines().find(|l| l.contains("// v2")).unwrap_or_default();
    assert!(
        first.trim_start().starts_with("1 "),
        "step 0 from line 1:\n{s}"
    );
}
