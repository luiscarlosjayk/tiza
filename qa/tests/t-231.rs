//! t-231 — `annotation.update` and `scene.patch` of an annotation that
//! already exists gave it the defaults of a new annotation.
//!
//! `Document::update` settled the file with the rule for a new annotation:
//! without a `file`, the file shown at its step. An update of `m5` (main
//! file, step 5, while step 5 shows `src/queue.rs`) that only changed its
//! label moved it to `src/queue.rs`. `scene.patch` re-added an annotation
//! it already had with `Document::add`: `n0` sent back as `scene.get` gives
//! it (`step: 0`) went to step 1.
//!
//! Acceptance criteria:
//! - AC1 An update of an existing annotation keeps its `step` and its
//!   `file`, unless the request sets them: an absent `file` keeps the one
//!   it has, and the step does not move. Over the socket
//!   (`annotation.update`), the MCP and every `:` command that edits one.
//! - AC2 A `scene.patch` of an id that exists keeps the `step` it is sent
//!   with (0 stays 0) and the `file` it is sent with, or else the one it had.
//!   A new id still gets today's defaults.
//! - AC3 Adding an annotation does not change: `annotation.add`, `:note`
//!   and the MCP `add_*` tools.
//! - AC4 A `file` or a `step` set in an update or a patch still works and is
//!   validated as before.
//! - AC5 This file: both repros, in `scene.get`, on screen and in the
//!   `.tiza` the autosave writes.
//!
//! Corner cases: an update that takes the range to a line only the linked
//! file has is validated against the annotation's own file; `file: null` in
//! an update is the main file.
//!
//! The fixture: `n0` at step 0 on main.rs L9, `q3` at step 3 on
//! src/queue.rs L2, `m5` (a note) and `s5` (a spotlight) at step 5 on
//! main.rs L3 while step 5 shows src/queue.rs (step 6 shows main.rs again).
//! main.rs has 10 lines, src/queue.rs 13.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, fixture, scratch};

const WAIT: Duration = Duration::from_secs(10);
const QUEUE: &str = "src/queue.rs";

/// (id, step, file, line) of an annotation.
type Place = (String, u64, Option<String>, u64);

fn place(id: &str, step: u64, file: Option<&str>, line: u64) -> Place {
    (id.into(), step, file.map(str::to_string), line)
}

fn places(annotations: &Value) -> Vec<Place> {
    let mut v: Vec<Place> = annotations
        .as_array()
        .expect("annotations")
        .iter()
        .map(|a| {
            (
                a["id"].as_str().unwrap().to_string(),
                a["step"].as_u64().unwrap_or(0),
                a.get("file").and_then(Value::as_str).map(str::to_string),
                a.get("line")
                    .or_else(|| a.get("from_line"))
                    .and_then(Value::as_u64)
                    .unwrap(),
            )
        })
        .collect();
    // A patch puts what it replaces last: compare by id.
    v.sort();
    v
}

/// The fixture as the author saved it.
fn authored() -> Vec<Place> {
    vec![
        place("m5", 5, None, 3),
        place("n0", 0, None, 9),
        place("q3", 3, Some(QUEUE), 2),
        place("s5", 5, None, 3),
    ]
}

/// `authored()` with the entry of `id` replaced.
fn with(p: Place) -> Vec<Place> {
    let mut v: Vec<Place> = authored().into_iter().filter(|a| a.0 != p.0).collect();
    v.push(p);
    v.sort();
    v
}

fn saved(tiza: &Path) -> Vec<Place> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap();
    places(&v["annotations"])
}

fn scene(live: &Live) -> Vec<Place> {
    places(&live.api_ok("scene.get", json!({}))["document"]["annotations"])
}

/// `scene.get` and the autosaved `.tiza` both say `expected`.
fn assert_places(live: &Live, tiza: &Path, expected: &[Place], what: &str) {
    assert_eq!(scene(live), expected, "{what}: scene.get");
    assert_eq!(saved(tiza), expected, "{what}: the saved .tiza");
}

/// The annotation `id` as `scene.get` gives it.
fn scene_annotation(live: &Live, id: &str) -> Value {
    live.api_ok("scene.get", json!({}))["document"]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap_or_else(|| panic!("{id} in scene.get"))
        .clone()
}

/// The fixture opened from a scratch copy, at step 0.
fn open(name: &str) -> (Live, PathBuf) {
    let dir = scratch(&format!("t-231-{name}"));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for f in ["main.rs", QUEUE, "demo.tiza"] {
        std::fs::copy(fixture(&format!("t-231/{f}")), dir.join(f)).unwrap();
    }
    let tiza = dir.join("demo.tiza");
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza.clone(), dir.join("live.sock"));
    assert_eq!(saved(&tiza), authored());
    (live, tiza)
}

/// The error of a call that must fail, as `code: message`.
fn refused(live: &Live, method: &str, params: Value) -> String {
    let r = live.api(method, params);
    let e = r
        .error
        .unwrap_or_else(|| panic!("{method} must fail: {:?}", r.result));
    format!("{}: {}", e.code, e.message)
}

fn goto(live: &Live, step: u64) {
    live.api_ok("step.set", json!({ "step": step }));
    let start = Instant::now();
    while live.api_ok("status", json!({}))["step"].as_u64() != Some(step) {
        assert!(start.elapsed() < WAIT, "timed out waiting for step {step}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The screen at `step`.
fn screen_at(live: &Live, step: u64) -> String {
    goto(live, step);
    live.screen(120, 30)
}

/// The screen row that draws the note `label`.
fn row_of(screen: &str, label: &str) -> Option<String> {
    screen
        .lines()
        .find(|l| l.contains(&format!("┆ {label}")))
        .map(str::to_string)
}

/// On screen, `m5` with `label`: not drawn over src/queue.rs at step 5,
/// drawn on its own main.rs line at step 6.
fn assert_m5_on_main(live: &Live, label: &str) {
    let s = screen_at(live, 5);
    assert!(
        s.contains("pub struct Queue"),
        "step 5 shows queue.rs:\n{s}"
    );
    assert!(row_of(&s, "the items").is_some(), "q3 at step 5:\n{s}");
    assert!(
        row_of(&s, label).is_none(),
        "m5 is of main.rs: not drawn over queue.rs:\n{s}"
    );
    let s = screen_at(live, 6);
    let m5 = row_of(&s, label).unwrap_or_else(|| panic!("m5 at step 6:\n{s}"));
    assert!(
        m5.contains("takes(s); // moved here"),
        "m5 on main.rs L3:\n{s}"
    );
    goto(live, 0);
}

/// On screen, `n0` with `label` at step 0 on `println!`.
fn assert_n0_at_step_0(live: &Live, label: &str) {
    let s = screen_at(live, 0);
    let n0 = row_of(&s, label).unwrap_or_else(|| panic!("n0 at step 0:\n{s}"));
    assert!(n0.contains("println!"), "n0 on L9:\n{s}");
}

// ---------- AC1: updates ----------

#[test]
fn ac1_socket_update_of_the_label_keeps_step_and_file() {
    let (live, tiza) = open("ac1-socket");
    let r = live.api_ok("annotation.update", json!({"id": "m5", "label": "moved!"}));
    assert_eq!(r["step"], 5, "{r}");
    assert_places(&live, &tiza, &authored(), "update of m5");
    assert!(scene_annotation(&live, "m5").get("file").is_none());
    assert_m5_on_main(&live, "moved!");
    live.api_ok(
        "annotation.update",
        json!({"id": "n0", "label": "prints it!"}),
    );
    assert_places(&live, &tiza, &authored(), "update of n0");
    assert_n0_at_step_0(&live, "prints it!");
}

#[test]
fn ac1_socket_update_of_the_step_keeps_the_file() {
    let (live, tiza) = open("ac1-step");
    // Step 3 shows src/queue.rs: m5 stays of main.rs all the same.
    let r = live.api_ok("annotation.update", json!({"id": "m5", "step": 3}));
    assert_eq!(r["step"], 3, "{r}");
    assert_places(&live, &tiza, &with(place("m5", 3, None, 3)), "step 3");
    let s = screen_at(&live, 3);
    assert!(row_of(&s, "moved").is_none(), "not over queue.rs:\n{s}");
    let s = screen_at(&live, 4);
    let m5 = row_of(&s, "moved").unwrap_or_else(|| panic!("m5 at step 4:\n{s}"));
    assert!(m5.contains("takes(s);"), "{s}");
}

#[test]
fn ac1_mcp_update_annotation_keeps_step_and_file() {
    let (live, tiza) = open("ac1-mcp");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("update_annotation", json!({"id": "m5", "label": "moved!"}));
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call(
        "update_annotation",
        json!({"id": "n0", "label": "prints it!"}),
    );
    assert!(!err, "{text}");
    assert_places(&live, &tiza, &authored(), "MCP label");
    assert_m5_on_main(&live, "moved!");
    assert_n0_at_step_0(&live, "prints it!");
    let (err, text, _) = mcp.call("update_annotation", json!({"id": "m5", "step": 3}));
    assert!(!err, "{text}");
    assert_places(&live, &tiza, &with(place("m5", 3, None, 3)), "MCP step");
}

#[test]
fn ac1_commands_keep_step_and_file() {
    let (live, tiza) = open("ac1-commands");
    for cmd in [
        "label m5 moved!",
        "color m5 blue",
        "set m5 label=moved!",
        "until s5 6",
        "label n0 prints it!",
        "color n0 green",
    ] {
        live.command(cmd);
        let (said, error) = live.message().unwrap();
        assert!(!error, ":{cmd}: {said}");
        assert_places(&live, &tiza, &authored(), &format!(":{cmd}"));
    }
    assert_eq!(scene_annotation(&live, "s5")["until"], 6);
    assert_eq!(scene_annotation(&live, "m5")["color"], "blue");
    assert_m5_on_main(&live, "moved!");
    assert_n0_at_step_0(&live, "prints it!");
    // `:step ID N` sets the step: the file stays.
    live.command("step m5 3");
    assert!(!live.message().unwrap().1, "{:?}", live.message());
    assert_places(&live, &tiza, &with(place("m5", 3, None, 3)), ":step m5 3");
}

// ---------- AC2: scene.patch ----------

#[test]
fn ac2_patch_of_an_existing_id_keeps_its_step_and_file() {
    let (live, tiza) = open("ac2-existing");
    // Sent back as scene.get gives them.
    let n0 = scene_annotation(&live, "n0");
    let m5 = scene_annotation(&live, "m5");
    assert_eq!(n0["step"], 0);
    assert!(m5.get("file").is_none());
    let r = live.api_ok("scene.patch", json!({"annotations": [n0, m5]}));
    assert_eq!(r["replaced"], json!(["n0", "m5"]), "{r}");
    assert_places(&live, &tiza, &authored(), "patch as scene.get gives it");
    assert_n0_at_step_0(&live, "prints it");
    assert_m5_on_main(&live, "moved");
    // Without a `file` key, q3 keeps src/queue.rs, at a step that shows
    // main.rs.
    live.api_ok(
        "scene.patch",
        json!({"annotations": [{"id": "q3", "step": 6, "type": "note", "line": 2, "text": "the items"}]}),
    );
    assert_places(
        &live,
        &tiza,
        &with(place("q3", 6, Some(QUEUE), 2)),
        "q3 without file",
    );
}

#[test]
fn ac2_patch_of_a_new_id_takes_the_defaults() {
    let (live, tiza) = open("ac2-new");
    let r = live.api_ok(
        "scene.patch",
        json!({"annotations": [
            {"id": "p0", "step": 0, "type": "note", "line": 2, "text": "new at 0"},
            {"id": "p5", "step": 5, "type": "note", "line": 2, "text": "new at 5"}
        ]}),
    );
    assert_eq!(r["added"], json!(["p0", "p5"]), "{r}");
    let mut expected = authored();
    expected.push(place("p0", 1, None, 2));
    expected.push(place("p5", 5, Some(QUEUE), 2));
    expected.sort();
    assert_places(&live, &tiza, &expected, "new ids");
}

// ---------- AC3: adding does not change ----------

#[test]
fn ac3_adding_takes_the_defaults_as_before() {
    let (live, tiza) = open("ac3");
    let mut expected = authored();
    let r = live.api_ok(
        "annotation.add",
        json!({"id": "a0", "step": 0, "type": "note", "line": 2, "text": "added at 0"}),
    );
    assert_eq!(r["step"], 1, "{r}");
    expected.push(place("a0", 1, None, 2));
    live.api_ok(
        "annotation.add",
        json!({"id": "a5", "step": 5, "type": "note", "line": 2, "text": "added at 5"}),
    );
    expected.push(place("a5", 5, Some(QUEUE), 2));
    // `:note` at step 0 and at step 5.
    live.command("note 2 noted at 0");
    let said = live.message().unwrap().0;
    let c0 = said.split(':').next().unwrap().to_string();
    expected.push(place(&c0, 1, None, 2));
    goto(&live, 5);
    live.command("note 2 noted at 5");
    let said = live.message().unwrap().0;
    let c5 = said.split(':').next().unwrap().to_string();
    expected.push(place(&c5, 5, Some(QUEUE), 2));
    goto(&live, 0);
    // The MCP add_* tools.
    let mut mcp = McpClient::start(&live.socket);
    for (id, step) in [("t0", 0), ("t5", 5)] {
        let (err, text, _) = mcp.call(
            "add_note",
            json!({"id": id, "step": step, "line": 2, "text": format!("tool at {step}")}),
        );
        assert!(!err, "{text}");
    }
    expected.push(place("t0", 1, None, 2));
    expected.push(place("t5", 5, Some(QUEUE), 2));
    expected.sort();
    assert_places(&live, &tiza, &expected, "added");
}

// ---------- AC4: an explicit file or step ----------

#[test]
fn ac4_update_with_an_explicit_file_or_step() {
    let (live, tiza) = open("ac4-update");
    live.api_ok("annotation.update", json!({"id": "m5", "file": QUEUE}));
    assert_places(
        &live,
        &tiza,
        &with(place("m5", 5, Some(QUEUE), 3)),
        "m5 to queue.rs",
    );
    let s = screen_at(&live, 5);
    assert!(row_of(&s, "moved").is_some(), "m5 over queue.rs now:\n{s}");
    goto(&live, 0);
    // The main file by its name: no `file`.
    live.api_ok("annotation.update", json!({"id": "m5", "file": "main.rs"}));
    assert_places(&live, &tiza, &authored(), "m5 back to main.rs");
    // A file that is no view of the document: refused, nothing changes.
    let e = refused(
        &live,
        "annotation.update",
        json!({"id": "m5", "label": "lost", "file": "src/nope.rs"}),
    );
    assert!(e.contains("src/nope.rs") && e.contains("not a view"), "{e}");
    assert_places(&live, &tiza, &authored(), "after the refused file");
    // An explicit step 0 goes to step 1, as before; the file stays.
    let r = live.api_ok("annotation.update", json!({"id": "q3", "step": 0}));
    assert_eq!(r["step"], 1, "{r}");
    assert_places(
        &live,
        &tiza,
        &with(place("q3", 1, Some(QUEUE), 2)),
        "q3 step 0",
    );
}

#[test]
fn ac4_patch_with_an_explicit_file() {
    let (live, tiza) = open("ac4-patch");
    let mut m5 = scene_annotation(&live, "m5");
    m5["file"] = json!(QUEUE);
    live.api_ok("scene.patch", json!({"annotations": [m5]}));
    assert_places(
        &live,
        &tiza,
        &with(place("m5", 5, Some(QUEUE), 3)),
        "m5 to queue.rs",
    );
    let mut q3 = scene_annotation(&live, "q3");
    q3["file"] = Value::Null;
    live.api_ok("scene.patch", json!({"annotations": [q3]}));
    let mut expected = with(place("m5", 5, Some(QUEUE), 3));
    expected.retain(|a| a.0 != "q3");
    expected.push(place("q3", 3, None, 2));
    expected.sort();
    assert_places(&live, &tiza, &expected, "q3 to main.rs with null");
    let mut n0 = scene_annotation(&live, "n0");
    n0["file"] = json!("src/nope.rs");
    let e = refused(&live, "scene.patch", json!({"annotations": [n0]}));
    assert!(e.contains("src/nope.rs") && e.contains("not a view"), "{e}");
    assert_places(&live, &tiza, &expected, "after the refused patch");
}

// ---------- corner cases ----------

#[test]
fn corner_a_line_only_the_linked_file_has_is_checked_on_the_annotations_file() {
    let (live, tiza) = open("corner-line");
    // L12 is in src/queue.rs (13 lines), not in main.rs (10 lines).
    let e = refused(
        &live,
        "annotation.update",
        json!({"id": "m5", "move": {"from_line": 12}}),
    );
    // Checked on main.rs, the annotation's file.
    assert!(
        e.contains("line 12 does not exist") && e.contains("1 to 10"),
        "{e}"
    );
    assert_places(&live, &tiza, &authored(), "m5 stays");
    live.api_ok(
        "annotation.update",
        json!({"id": "q3", "move": {"from_line": 12}}),
    );
    assert_places(
        &live,
        &tiza,
        &with(place("q3", 3, Some(QUEUE), 12)),
        "q3 to L12",
    );
    let s = screen_at(&live, 3);
    let q3 = row_of(&s, "the items").unwrap_or_else(|| panic!("q3 at step 3:\n{s}"));
    assert!(q3.trim_start().starts_with("12 "), "q3 on L12:\n{s}");
}

#[test]
fn corner_file_null_in_an_update_is_the_main_file() {
    let (live, tiza) = open("corner-null");
    goto(&live, 3);
    let r = live.api_ok("annotation.update", json!({"id": "q3", "file": null}));
    assert_places(
        &live,
        &tiza,
        &with(place("q3", 3, None, 2)),
        "q3 to main.rs",
    );
    assert_eq!(r["visible"], false, "{r}");
    // Its step shows src/queue.rs, so the reply says why it is not drawn.
    assert!(
        r["hidden"].as_str().unwrap_or_default().contains("main.rs"),
        "{r}"
    );
    let s = screen_at(&live, 4);
    let q3 = row_of(&s, "the items").unwrap_or_else(|| panic!("q3 at step 4:\n{s}"));
    assert!(q3.contains("let s"), "q3 on main.rs L2:\n{s}");
    goto(&live, 0);
    // The line is checked on the main file: L12 is only in src/queue.rs.
    live.api_ok("annotation.update", json!({"id": "q3", "file": QUEUE}));
    live.api_ok(
        "annotation.update",
        json!({"id": "q3", "move": {"from_line": 12}}),
    );
    let e = refused(
        &live,
        "annotation.update",
        json!({"id": "q3", "file": null}),
    );
    // Checked on main.rs, the annotation's file.
    assert!(
        e.contains("line 12 does not exist") && e.contains("1 to 10"),
        "{e}"
    );
    assert_places(
        &live,
        &tiza,
        &with(place("q3", 3, Some(QUEUE), 12)),
        "q3 stays",
    );
}
