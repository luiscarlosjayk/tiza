//! t-188 — `scene.get` → `scene.put` is a faithful round trip.
//!
//! Acceptance criteria:
//! - AC1 A `scene.put` of exactly what `scene.get` returned works and leaves
//!   the same document: same moves, annotations, steps, styles, title and
//!   `render --all-steps` before and after, and nothing is written to disk
//!   when nothing changed.
//! - AC2 References stay references: when the code that comes back (and
//!   that of every linked file in `views`) equals what the referenced file
//!   holds NOW, the document keeps `"code": {"file": …}` (and its `code.link`
//!   paths) and is saved as a reference; only what the agent changed
//!   changes. Code the agent did change is shown, not autosaved over a
//!   different source, and the bar says why.
//! - AC3 Any mix of moves an agent can add (position, find, after/before,
//!   insert, replace, delete, typed or not, same step or different steps)
//!   survives get → put. An invalid put names the move (`move #N, step S:
//!   …`).
//! - AC4 Simple: no fingerprints and no per-request state, a plain
//!   comparison of the code sent with the file's current text; the rule is
//!   in the «Saving rule» of docs/agent-guide.md.
//! - AC5 This file: case A (reference kept on disk, a later edit of the code
//!   file shows), case A with a linked file, case B, and a fixed-seed loop of
//!   random moves added to the examples (examples/*.tiza), each followed by
//!   get → put → same document and same `render --all-steps`; a put that
//!   changes nothing writes nothing (content and mtime of the .tiza).
//!
//! Cause of B: `move.add` did not carry what a new `code.patch` moves.
//! Annotations are written against the code at their own step and a
//! move's `position` against the code where it runs (ADR 0007 §3d), so a
//! patch added at step S changed the code under every annotation of its
//! file at step S or later and under every later move, and left them on
//! the old numbers: the TUI drew them on the wrong text and the put, which
//! validates every annotation at its step, refused the document. A new
//! patch now carries them through its own effect (`tiza_core::carry`), and
//! `move.delete` carries them back.
//!
//! Corner cases: the code file changed on disk between the get and the put
//! (the reference is kept, the screen keeps the copy loaded, the bar says
//! so); a `code.link` window survives; a document from stdin stays
//! embedded; a put taken at another step than the TUI's; `:embed` stays
//! embedded after a put.
//!
//! The repros (QA's scripts on Luis's Mac) are rebuilt here on scratch
//! copies of the examples.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};
use tiza_qa::{Live, bin, example, scratch};

// ---------------------------------------------------------------- helpers

/// A scratch copy of `examples/` (the documents and the code they read).
fn examples_copy(name: &str) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            if p.is_dir() {
                copy(&p, &to.join(e.file_name()));
            } else {
                std::fs::copy(&p, to.join(e.file_name())).unwrap();
            }
        }
    }
    let dir = scratch(name);
    copy(&example(""), &dir);
    dir
}

/// `main.rs` and a `main.rs.tiza` that reads it by reference.
fn reference_demo(name: &str) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(
        dir.join("main.rs"),
        "fn main() {\n    let s = String::from(\"hi\");\n    println!(\"{s}\");\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("main.rs.tiza"),
        serde_json::to_string_pretty(&json!({
            "schema": 1,
            "source": {"language": "rs", "first_line": 1},
            "code": {"file": "main.rs"},
            "step": 1,
            "annotations": [
                {"id": "a1", "step": 1, "type": "mark", "kind": "move",
                 "span": {"line": 2, "start": 8, "end": 9}, "label": "s"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    dir
}

/// Opens `target` as `tiza target` would (a source file opens its sidecar).
fn open(target: &Path) -> Live {
    let o = tiza_core::fs::open(target.to_str().unwrap()).unwrap();
    let sock = target.parent().unwrap().join("s.sock");
    Live::open(o.document, o.save_path, sock)
}

fn sock(live: &Live) -> &str {
    live.socket.to_str().unwrap()
}

fn get(live: &Live) -> Value {
    live.api_ok("scene.get", json!({}))
}

fn put(live: &Live, document: &Value) -> Result<Value, (String, String)> {
    let r = live.api("scene.put", json!({ "document": document }));
    match r.error {
        Some(e) => Err((e.code, e.message)),
        None => Ok(r.result.unwrap_or(Value::Null)),
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// What is on disk: the bytes and the modification time.
fn on_disk(path: &Path) -> (Vec<u8>, SystemTime) {
    (
        std::fs::read(path).unwrap(),
        std::fs::metadata(path).unwrap().modified().unwrap(),
    )
}

/// `render --all-steps` of the live document, drawn in this process as the
/// CLI draws it (every step, all the same size, as text).
fn all_steps(live: &Live) -> Vec<String> {
    let doc = live.with(|app| app.doc.clone());
    let theme = tiza_core::Theme::from_name(tiza_core::ThemeName::Dark);
    let last = tiza_core::check::last_step(&doc);
    let mut canvases: Vec<_> = (0..=last)
        .map(|step| {
            tiza_core::layout::layout(
                &doc,
                &theme,
                &tiza_core::layout::LayoutOptions {
                    step,
                    ..Default::default()
                },
            )
            .canvas
        })
        .collect();
    tiza_core::anim::same_size(&mut canvases, &theme);
    canvases.iter().map(|c| c.to_plain_text()).collect()
}

/// `tiza render --socket … --all-steps --format txt`: one text per step.
fn all_steps_cli(live: &Live, out: &Path) -> Vec<String> {
    let _ = std::fs::remove_dir_all(out);
    let r = std::process::Command::new(bin())
        .args([
            "render",
            "--socket",
            sock(live),
            "--all-steps",
            "--format",
            "txt",
            "--out",
        ])
        .arg(format!("{}/", out.display()))
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "render --all-steps: {}",
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

/// `tiza render FILE --step N` (a fresh load from disk).
fn render_file(path: &Path, step: u32) -> String {
    let r = std::process::Command::new(bin())
        .args(["render"])
        .arg(path)
        .args(["--step", &step.to_string(), "--out", "-"])
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "render {}: {}",
        path.display(),
        String::from_utf8_lossy(&r.stderr)
    );
    String::from_utf8(r.stdout).unwrap()
}

fn message(live: &Live) -> String {
    live.message().map(|(m, _)| m).unwrap_or_default()
}

/// The fields AC1 compares, from a `scene.get` result.
fn fields(scene: &Value) -> Value {
    let d = &scene["document"];
    json!({
        "moves": d["moves"], "annotations": d["annotations"], "steps": d["steps"],
        "styles": d["styles"], "title": d["title"], "code": d["code"], "views": d["views"],
        "step": scene["step"],
    })
}

/// get → put of exactly what came back: accepted, the same document and
/// the same render, the revision unchanged and the `.tiza` untouched.
fn round_trip(live: &Live, tiza: Option<&Path>, what: &str) {
    // A typing animation in progress draws the step half typed and counts
    // revisions as it goes: end it, as a key press would.
    live.with(|app| app.finish_typing());
    let before = get(live);
    let render = all_steps(live);
    let disk = tiza.filter(|p| p.exists()).map(on_disk);
    let r = put(live, &before["document"]).unwrap_or_else(|e| panic!("{what}: put refused: {e:?}"));
    assert_eq!(r["revision"], before["revision"], "{what}: nothing changed");
    let after = get(live);
    assert_eq!(fields(&after), fields(&before), "{what}");
    assert_eq!(all_steps(live), render, "{what}: render --all-steps");
    if let (Some(p), Some(d)) = (tiza, disk) {
        assert_eq!(on_disk(p), d, "{what}: the put wrote {}", p.display());
    }
}

// ---------------------------------------------------------------- AC1 / AC2: case A

/// Test steps 1-3: `tiza main.rs`, `tiza api scene.get > s.json`, a note
/// added to s.json, `tiza scene put s.json`: the .tiza on disk still reads
/// main.rs, has the note, and a later edit of main.rs shows.
#[test]
fn ac2_case_a_reference_kept_on_disk_and_later_edits_show() {
    let dir = reference_demo("case-a");
    let live = open(&dir.join("main.rs"));
    let tiza = dir.join("main.rs.tiza");
    let out = std::process::Command::new(bin())
        .args(["api", "scene.get", "--socket", sock(&live)])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut scene: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        scene["document"]["code"].is_array(),
        "scene.get embeds the code: {scene}"
    );
    scene["document"]["annotations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "n1", "step": 1, "type": "note", "line": 3, "text": "printed here"}));
    let s_json = dir.join("s.json");
    std::fs::write(&s_json, serde_json::to_string(&scene).unwrap()).unwrap();
    let out = std::process::Command::new(bin())
        .args(["scene", "put"])
        .arg(&s_json)
        .args(["--socket", sock(&live)])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let saved = read_json(&tiza);
    assert_eq!(saved["code"], json!({"file": "main.rs"}), "{saved}");
    assert!(
        saved["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == "n1"),
        "{saved}"
    );
    assert!(saved.get("views").is_none(), "{saved}");
    let status = live.api_ok("status", json!({}));
    assert_eq!(status["autosave"], true, "{status}");
    assert!(!message(&live).contains("differs"), "{}", message(&live));
    // A later edit of main.rs reaches the render of the .tiza.
    std::fs::write(
        dir.join("main.rs"),
        "fn main() {\n    let s = String::from(\"edited later\");\n    println!(\"{s}\");\n}\n",
    )
    .unwrap();
    let shown = render_file(&tiza, 1);
    assert!(shown.contains("edited later"), "{shown}");
    assert!(shown.contains("printed here"), "{shown}");
}

/// AC2 with linked files: stack-and-queue reads src/stack.rs (lines 7-30)
/// and links src/queue.rs with windows; a get → put with a new note keeps
/// both as references, with their windows, and an edit of queue.rs shows.
#[test]
fn ac2_linked_file_stays_a_reference_with_its_window() {
    let dir = examples_copy("linked");
    let tiza = dir.join("stack-and-queue.tiza");
    let live = open(&tiza);
    let mut scene = get(&live);
    assert!(
        scene["document"]["views"]["src/queue.rs"].is_array(),
        "scene.get embeds the linked file: {}",
        scene["document"]
    );
    scene["document"]["annotations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "n1", "step": 2, "file": "src/queue.rs", "type": "note", "line": 20, "text": "new note"}));
    put(&live, &scene["document"]).unwrap();
    let saved = read_json(&tiza);
    assert_eq!(
        saved["code"],
        json!({"file": "src/stack.rs", "lines": "7-30"}),
        "{saved}"
    );
    assert!(saved.get("views").is_none(), "{saved}");
    let links: Vec<&Value> = saved["moves"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["action"] == "code.link")
        .collect();
    assert_eq!(links[0]["file"], "src/queue.rs");
    assert_eq!(links[0]["lines"], "10-36", "the window survives");
    assert_eq!(links[2]["lines"], "17-23", "the window survives");
    assert_eq!(live.api_ok("status", json!({}))["autosave"], true);
    // A later edit of queue.rs reaches the render of the .tiza.
    let q = dir.join("src/queue.rs");
    let text = std::fs::read_to_string(&q).unwrap();
    std::fs::write(&q, text.replace("pop_front()", "pop_front() /* edited */")).unwrap();
    let shown = render_file(&tiza, 2);
    assert!(shown.contains("/* edited */"), "{shown}");
    assert!(shown.contains("new note"), "{shown}");
}

/// AC2: code the agent changed is shown, not autosaved over the file it
/// came from, and the bar says why — for the main file and a linked one.
#[test]
fn ac2_edited_code_is_shown_not_saved_and_the_bar_says_why() {
    let dir = reference_demo("edited");
    let live = open(&dir.join("main.rs"));
    let tiza = dir.join("main.rs.tiza");
    let before = on_disk(&tiza);
    let mut scene = get(&live);
    scene["document"]["code"][2] = json!("    println!(\"{s} changed by the agent\");");
    put(&live, &scene["document"]).unwrap();
    assert_eq!(on_disk(&tiza), before, "nothing written over the reference");
    let text = live.api_ok("render.text", json!({}))["text"].clone();
    assert!(
        text.as_str().unwrap().contains("changed by the agent"),
        "{text}"
    );
    assert_eq!(live.api_ok("status", json!({}))["autosave"], false);
    let m = message(&live);
    assert!(
        m.contains("the code sent for main.rs differs from the file on disk")
            && m.contains("autosave off"),
        "{m}"
    );

    // A linked file edited in the put.
    let dir = examples_copy("edited-link");
    let tiza = dir.join("stack-and-queue.tiza");
    let live = open(&tiza);
    let before = on_disk(&tiza);
    let mut scene = get(&live);
    let q = &mut scene["document"]["views"]["src/queue.rs"];
    q[19] = json!("    // edited by the agent");
    put(&live, &scene["document"]).unwrap();
    assert_eq!(on_disk(&tiza), before);
    let m = message(&live);
    assert!(
        m.contains("the code sent for src/queue.rs differs from the file on disk"),
        "{m}"
    );
    assert_eq!(live.api_ok("status", json!({}))["autosave"], false);
}

/// Corner case: the code file changed on disk between the get and the put.
/// The code sent equals the copy the instance loaded, so the agent did not
/// change it: the document keeps the reference (never embedded in
/// silence), the agent's note is saved, the screen keeps the copy loaded and
/// the bar says so; the .tiza renders the new file.
#[test]
fn corner_file_changed_between_get_and_put_keeps_the_reference() {
    let dir = reference_demo("changed-between");
    let live = open(&dir.join("main.rs"));
    let tiza = dir.join("main.rs.tiza");
    let mut scene = get(&live);
    std::fs::write(
        dir.join("main.rs"),
        "fn main() {\n    let s = String::from(\"new on disk\");\n    println!(\"{s}\");\n}\n",
    )
    .unwrap();
    scene["document"]["annotations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "n1", "step": 1, "type": "note", "line": 3, "text": "agent note"}));
    put(&live, &scene["document"]).unwrap();
    let saved = read_json(&tiza);
    assert_eq!(saved["code"], json!({"file": "main.rs"}), "{saved}");
    assert!(saved.to_string().contains("agent note"), "{saved}");
    assert_eq!(live.api_ok("status", json!({}))["autosave"], true);
    let m = message(&live);
    assert!(
        m.contains("main.rs changed on disk since it was loaded") && m.contains("still reads it"),
        "{m}"
    );
    let text = live.api_ok("render.text", json!({}))["text"].clone();
    assert!(
        text.as_str().unwrap().contains("\"hi\""),
        "the copy loaded: {text}"
    );
    let shown = render_file(&tiza, 1);
    assert!(
        shown.contains("new on disk") && shown.contains("agent note"),
        "{shown}"
    );
}

/// AC1: a put that changes nothing writes nothing, on a reference document
/// and on an embedded one.
#[test]
fn ac1_unchanged_put_writes_nothing() {
    let dir = reference_demo("no-write");
    let tiza = dir.join("main.rs.tiza");
    let live = open(&dir.join("main.rs"));
    // Let the mtime tick: a write would show even on a coarse clock.
    let t0 = std::fs::metadata(&tiza).unwrap().modified().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while SystemTime::now().duration_since(t0).unwrap_or_default() < Duration::from_millis(20)
        && Instant::now() < deadline
    {
        std::hint::spin_loop();
    }
    round_trip(&live, Some(&tiza), "reference");
    let dir = examples_copy("no-write-embedded");
    let tiza = dir.join("ownership-moves.rs.tiza");
    let live = open(&tiza);
    for step in 0..=5 {
        live.api_ok("step.set", json!({ "step": step }));
        // The step change itself saves; the put after it must not.
        round_trip(&live, Some(&tiza), &format!("embedded, step {step}"));
    }
}

/// AC1 on every example as it is, at every step.
#[test]
fn ac1_every_example_round_trips_at_every_step() {
    let dir = examples_copy("examples");
    for name in tiza_files(&dir) {
        let tiza = dir.join(&name);
        let live = open(&tiza);
        let max = get(&live)["max_step"].as_u64().unwrap() as u32;
        for step in 0..=max {
            live.api_ok("step.set", json!({ "step": step }));
            round_trip(&live, Some(&tiza), &format!("{name}, step {step}"));
        }
        let saved = read_json(&tiza);
        let orig = read_json(&example(&name));
        assert_eq!(saved["code"], orig["code"], "{name}: code kept as it was");
    }
}

/// Corner case: a document from stdin (no file) stays embedded.
#[test]
fn corner_stdin_document_stays_embedded() {
    let dir = scratch("stdin");
    let o = tiza_core::fs::from_stdin_text("fn main() {}\nfn other() {}\n", Some("rs"));
    let path = dir.join("stdin.tiza");
    let live = Live::open(o.document, path.clone(), dir.join("s.sock"));
    let mut scene = get(&live);
    scene["document"]["annotations"] =
        json!([{"id": "n1", "step": 1, "type": "note", "line": 2, "text": "from stdin"}]);
    put(&live, &scene["document"]).unwrap();
    let saved = read_json(&path);
    assert_eq!(
        saved["code"],
        json!(["fn main() {}", "fn other() {}"]),
        "{saved}"
    );
    assert!(saved.to_string().contains("from stdin"));
    round_trip(&live, Some(&path), "stdin");
}

/// Corner case: `:embed` embeds on purpose, and a put after it does not
/// turn the code back into a reference.
#[test]
fn corner_embed_stays_embedded_after_a_put() {
    let dir = examples_copy("embed");
    let tiza = dir.join("stack-and-queue.tiza");
    let live = open(&tiza);
    live.command("embed");
    let saved = read_json(&tiza);
    assert!(
        saved["code"].is_array() && saved["views"].is_object(),
        "{saved}"
    );
    let mut scene = get(&live);
    scene["document"]["title"] = json!("embedded on purpose");
    put(&live, &scene["document"]).unwrap();
    let saved = read_json(&tiza);
    assert!(saved["code"].is_array(), "{saved}");
    assert!(saved["views"]["src/queue.rs"].is_array(), "{saved}");
    assert_eq!(saved["title"], "embedded on purpose");
    assert_eq!(live.api_ok("status", json!({}))["autosave"], true);
    round_trip(&live, Some(&tiza), "after :embed");
}

/// Corner case: a put taken at another step than the TUI's. The document
/// is the same except for the step it was taken at: the TUI goes there (the
/// put replaces the document, step included, as `u` brings it back), and
/// the rest is unchanged.
#[test]
fn corner_put_taken_at_another_step() {
    let dir = examples_copy("other-step");
    let tiza = dir.join("tally-tour.tiza");
    let live = open(&tiza);
    live.api_ok("step.set", json!({"step": 1}));
    let scene = get(&live);
    let render = all_steps(&live);
    live.api_ok("step.set", json!({"step": 3}));
    put(&live, &scene["document"]).unwrap();
    let after = get(&live);
    assert_eq!(after["step"], 1);
    assert_eq!(fields(&after), fields(&scene));
    assert_eq!(all_steps(&live), render);
    let saved = read_json(&tiza);
    assert_eq!(
        saved["code"],
        json!({"file": "tally/src/main.rs"}),
        "{saved}"
    );
    assert!(saved.get("views").is_none(), "{saved}");
}

// ---------------------------------------------------------------- AC3: case B

/// Test step 4: ownership-moves with `--no-save`; a move by `position` and
/// one by `find` added at step 1; get → put accepted, `render --all-steps`
/// the same before and after. The annotations and the later moves the new
/// patches moved are carried to the same text.
#[test]
fn ac3_case_b_position_and_find_moves_survive_get_put() {
    let dir = examples_copy("case-b");
    let tiza = dir.join("ownership-moves.rs.tiza");
    let live = open(&tiza);
    live.command("autosave off");
    let disk = on_disk(&tiza);
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "3",
               "content": "    // ownership starts here", "typing": "line", "speed_ms": 1}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "find": "fn takes(s: String) {",
               "content": "fn takes(s: String) { // takes ownership"}),
    );
    let scene = get(&live);
    let moves = scene["document"]["moves"].as_array().unwrap();
    // The step-2 insert and the step-4 delete by position follow the new line.
    assert_eq!(moves[1]["position"], "15", "{}", moves[1]);
    assert_eq!(moves[3]["position"], "15-18", "{}", moves[3]);
    // The arrow to `takes` follows it, its column kept on the rewritten line.
    let a2 = scene["document"]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "a2")
        .unwrap();
    assert_eq!(a2["to"], json!({"line": 8, "col": 9}), "{a2}");
    let out = dir.join("steps");
    let cli = all_steps_cli(&live, &out);
    round_trip(&live, None, "case B");
    assert_eq!(all_steps_cli(&live, &out), cli, "render --all-steps");
    // At step 2 `fn len` goes after `takes`, as before the new moves.
    let step2 = &cli[2];
    let takes = step2.find("fn takes").unwrap();
    let len = step2.find("fn len").unwrap();
    assert!(takes < len, "{step2}");
    // At step 4 the delete removes `fn len` again and nothing else.
    assert!(
        !cli[4].contains("fn len") && cli[4].contains("println!(\"{s}\");"),
        "{}",
        cli[4]
    );
    assert_eq!(on_disk(&tiza), disk, "--no-save: nothing written");
}

/// AC3: an invalid put names the move or the annotation (with the code of
/// the error inside).
#[test]
fn ac3_an_invalid_put_names_the_move_or_the_annotation() {
    let dir = examples_copy("invalid");
    let live = open(&dir.join("ownership-moves.rs.tiza"));
    let scene = get(&live);
    let mut doc = scene["document"].clone();
    doc["moves"][2]["find"] = json!("nowhere to be found");
    let (code, msg) = put(&live, &doc).unwrap_err();
    assert!(msg.starts_with("move #3, step 4: "), "{msg}");
    assert_eq!(code, "text_not_found");
    let mut doc = scene["document"].clone();
    doc["annotations"][1]["to"] = json!({"line": 6, "col": 9});
    let (code, msg) = put(&live, &doc).unwrap_err();
    assert!(msg.starts_with("annotation a2, step 1: "), "{msg}");
    assert_eq!(code, "out_of_range");
    assert_eq!(get(&live)["document"], scene["document"], "nothing changed");
}

/// AC3: a patch that removes the line an annotation of its step (or later)
/// is on is refused naming the annotation, and one that removes a later
/// move's target names that move; nothing changes. Deleting a move carries
/// back what adding it carried.
#[test]
fn ac3_refusals_name_what_would_lose_its_line_and_delete_carries_back() {
    let dir = examples_copy("refusals");
    let live = open(&dir.join("ownership-moves.rs.tiza"));
    live.command("autosave off");
    let before = get(&live);
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "position": "3"}),
    );
    let e = r.error.unwrap().message;
    assert!(
        e.starts_with("annotation a1, step 1: move #5 (step 1), the one being added, removes or rewrites line 3"),
        "{e}"
    );
    let r = live.api(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "delete", "position": "14-17"}),
    );
    let e = r.error.unwrap().message;
    assert!(
        e.contains("annotation a5, step 3") || e.contains("annotation a6, step 3"),
        "{e}"
    );
    assert_eq!(get(&live)["document"], before["document"]);
    // Add, then delete: the same document again.
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "before": "fn takes", "content": "// a\n// b"}),
    );
    let added = get(&live);
    assert_ne!(
        added["document"]["annotations"],
        before["document"]["annotations"]
    );
    live.api_ok("move.delete", json!({"index": 5}));
    assert_eq!(get(&live)["document"], before["document"]);
}

// ---------------------------------------------------------------- AC5: random moves

/// xorshift64*: a fixed seed gives the same moves on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const SEED: u64 = 0x7188_5EED;
const MOVES_PER_DOCUMENT: usize = 30;

fn tiza_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tiza"))
        .collect();
    names.sort();
    names
}

/// A move an agent could add at `step`, on the code shown there.
fn random_move(rng: &mut Rng, live: &Live, step: u32, i: usize) -> Value {
    let (code, first) = live.with(move |app| {
        let key = app.doc.shown_view_at(step).file;
        app.doc.view_code_at(&key, step).unwrap()
    });
    let n = code.len();
    let at = rng.below(n);
    let line = first + at as u32;
    let contents = [
        format!("    // added {i}"),
        format!("    let x{i} = {i};"),
        format!("    first_{i}();\n    second_{i}();"),
        format!("{} // edited {i}", code[at]),
    ];
    let content = contents[rng.below(contents.len())].clone();
    let typing = ["instant", "line", "char"][rng.below(3)];
    let text = code[at].trim().to_string();
    // The occurrence of this line's text that is this line.
    let occurrence = tiza_core::moves::matches_of(&code, first, &text)
        .iter()
        .position(|l| *l == line)
        .map(|p| p as u32 + 1);
    let by_text = !text.is_empty() && occurrence.is_some();
    let mut m = json!({"at_step": step, "action": "code.patch", "typing": typing, "speed_ms": 1});
    let span = 1 + rng.below(3) as u32;
    let last = first + n as u32 - 1;
    let range = format!("{}-{}", line, (line + span - 1).min(last));
    match rng.below(7) {
        0 => {
            m["op"] = json!("insert");
            m["position"] = json!((line + rng.below(2) as u32).to_string());
        }
        1 | 2 if by_text => {
            m["op"] = json!("insert");
            m[if rng.below(2) == 0 { "after" } else { "before" }] = json!(text);
            if occurrence != Some(1) || rng.below(2) == 0 {
                m["occurrence"] = json!(occurrence);
            }
        }
        3 => {
            m["op"] = json!("replace");
            m["position"] = json!(range);
        }
        4 if by_text => {
            m["op"] = json!("replace");
            m["find"] = json!(text);
            m["occurrence"] = json!(occurrence);
        }
        5 => {
            m["op"] = json!("delete");
            m["position"] = json!(range);
        }
        _ if by_text => {
            m["op"] = json!("delete");
            m["find"] = json!(text);
            m["occurrence"] = json!(occurrence);
        }
        _ => {
            m["op"] = json!("insert");
            m["position"] = json!(line.to_string());
        }
    }
    if m["op"] != "delete" {
        m["content"] = json!(content);
    } else {
        m.as_object_mut().unwrap().remove("typing");
        m.as_object_mut().unwrap().remove("speed_ms");
    }
    m
}

/// What the loop did on one document.
#[derive(Default)]
struct Tally {
    tried: usize,
    accepted: usize,
    refused: usize,
    undone: usize,
}

/// The loop on one document, with its own instance and seed.
fn random_moves(dir: &Path, name: &str, seed: u64) -> Tally {
    let tiza = dir.join(name);
    let reference = read_json(&tiza)["code"].is_object();
    let o = tiza_core::fs::open(tiza.to_str().unwrap()).unwrap();
    let sock = dir.join(format!("{}.sock", name.trim_end_matches(".tiza")));
    let live = Live::open(o.document, o.save_path, sock);
    let mut rng = Rng(seed);
    let mut t = Tally::default();
    for i in 0..MOVES_PER_DOCUMENT {
        let max = get(&live)["max_step"].as_u64().unwrap() as u32;
        // Mostly the steps there are; now and then a new last one.
        let step = if rng.below(8) == 0 {
            max + 1
        } else {
            1 + rng.below(max.max(1) as usize) as u32
        };
        let m = random_move(&mut rng, &live, step, i);
        t.tried += 1;
        let before = get(&live);
        let r = live.api("move.add", m.clone());
        if let Some(e) = r.error {
            t.refused += 1;
            assert!(
                e.message.starts_with("move #") || e.message.starts_with("annotation "),
                "{name}: {m}: an unnamed refusal: {}",
                e.message
            );
            assert_eq!(get(&live)["document"], before["document"], "{name}: {m}");
            continue;
        }
        t.accepted += 1;
        let what = format!("{name}, move {m}");
        round_trip(&live, Some(&tiza), &what);
        if m["op"] == "insert" && rng.below(3) == 0 {
            let index = get(&live)["document"]["moves"].as_array().unwrap().len();
            live.api_ok("move.delete", json!({ "index": index }));
            t.undone += 1;
            let back = get(&live);
            assert_eq!(fields(&back)["moves"], fields(&before)["moves"], "{what}");
            assert_eq!(
                fields(&back)["annotations"],
                fields(&before)["annotations"],
                "{what}"
            );
        }
        if t.accepted % 4 == 0 {
            let mut scene = get(&live);
            let title = format!("take {}", t.accepted);
            scene["document"]["title"] = json!(title);
            put(&live, &scene["document"]).unwrap_or_else(|e| panic!("{what}: {e:?}"));
            let saved = read_json(&tiza);
            assert_eq!(saved["title"], json!(title), "{what}");
            assert_eq!(
                saved["code"].is_object(),
                reference,
                "{what}: {}",
                saved["code"]
            );
            assert!(
                saved.get("views").is_none(),
                "{what}: no linked file embedded"
            );
            assert_eq!(live.api_ok("status", json!({}))["autosave"], true, "{what}");
        }
    }
    t
}

/// AC5: random moves (fixed seed) on every example; after each one that is
/// accepted, get → put gives the same document and the same render and
/// writes nothing. Every fourth accepted move, a put that changes the title
/// is saved, and the references stay references. An accepted insert is
/// sometimes deleted again: the document is exactly the one before it.
/// One thread per document (each its own instance and seed).
#[test]
fn ac5_random_moves_on_the_examples_survive_get_put() {
    let started = Instant::now();
    let dir = examples_copy("random");
    let names = tiza_files(&dir);
    let tallies: Vec<Tally> = std::thread::scope(|s| {
        let jobs: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(k, name)| {
                let dir = &dir;
                let seed = SEED.wrapping_add(k as u64 * 0x9E37_79B9);
                s.spawn(move || random_moves(dir, name, seed))
            })
            .collect();
        jobs.into_iter().map(|j| j.join().unwrap()).collect()
    });
    let sum = |f: fn(&Tally) -> usize| tallies.iter().map(f).sum::<usize>();
    let (tried, accepted) = (sum(|t| t.tried), sum(|t| t.accepted));
    eprintln!(
        "t-188 random moves: seed {SEED:#x}, {} documents, {tried} moves tried, {accepted} accepted, {} refused, {} deleted again, {:.1} s",
        names.len(),
        sum(|t| t.refused),
        sum(|t| t.undone),
        started.elapsed().as_secs_f64()
    );
    for (name, t) in names.iter().zip(&tallies) {
        assert!(
            t.accepted >= t.tried / 3,
            "{name}: {} of {} accepted: the loop tests too little",
            t.accepted,
            t.tried
        );
    }
}
