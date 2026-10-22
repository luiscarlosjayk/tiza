//! t-194 — Annotations whose file is not shown at their step are said, and
//! an insert at a window's first line stays visible.
//!
//! Acceptance criteria:
//! - AC1 `tiza check` (CLI and live) warns about every annotation whose
//!   file is not shown at its own step (`not_shown`: `a6, step 4, L20: its
//!   file src/stack.rs is not shown at step 4 (src/queue.rs is): kept, but
//!   not drawn at its step`). The examples and fixtures keep their findings
//!   (none of them has such an annotation).
//! - AC2 `move.add` of a `code.link` (and `move.delete`) answers with a
//!   `warning` naming the annotations it leaves unshown; it is accepted.
//! - AC3 The delete-a-link rule and the warning agree: the refusal is
//!   replaced by the warning wherever the document stays loadable (deleting
//!   a link that leaves a step showing another file is accepted, with the
//!   same `warning` as adding the link that does it). Deleting the only
//!   `code.link` to a file with annotations stays refused: they would
//!   belong to no view, so the saved `.tiza` would name a file it does not
//!   load (a fresh load refuses it) unless the save wrote that file's code
//!   into it, which the reference mode never writes. Adding a link never
//!   reaches that state.
//! - AC4 An insert at the first line of a window, at the link's own step,
//!   grows the window to start at the new lines (17-23 → 17-24); at other
//!   steps the window is carried as before; `move.delete` gives it back.
//! - AC5 The warning of a move names only what was drawn at a step before
//!   it and is not after it (a step it creates past the last one does not
//!   repeat what was already outside there; what that step showed of
//!   another file, or already outside the window, is not named); a file is
//!   named the same in the warning, the bar's message and `check` (live and
//!   on disk): a linked file by its path, the main file as `code.file`
//!   names it (its file name when the code is embedded).
//! - AC6 This file; agent guide and CHANGELOG.
//!
//! Corner cases: an annotation of a file shown at another step only (still
//! said); an annotation outside the window of its own file (`outside_window`
//! only, not both); the author's zoom stays silent in `check`; linking back
//! to the main file shows its annotations again and `check` forgets them.
//!
//! Every accepted change is checked against a fresh load of the autosaved
//! `.tiza`: the same `check` findings and the same `render --all-steps`.

use std::path::{Path, PathBuf};

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

fn open(target: &Path) -> Live {
    let o = tiza_core::fs::open(target.to_str().unwrap()).unwrap();
    let sock = target.parent().unwrap().join("s.sock");
    Live::open(o.document, o.save_path, sock)
}

fn get(live: &Live) -> Value {
    live.api_ok("scene.get", json!({}))
}

fn link(step: u32, file: &str, lines: Option<&str>) -> Value {
    match lines {
        Some(l) => json!({"at_step": step, "action": "code.link", "file": file, "lines": l}),
        None => json!({"at_step": step, "action": "code.link", "file": file}),
    }
}

fn add(live: &Live, m: Value) -> Value {
    live.api_ok("move.add", m)
}

fn delete(live: &Live, index: usize) -> Value {
    live.api_ok("move.delete", json!({ "index": index }))
}

fn refusal(live: &Live, method: &str, params: Value) -> String {
    let before = get(live)["document"].clone();
    let r = live.api(method, params);
    let e = r.error.expect("refused").message;
    assert_eq!(get(live)["document"], before, "a refusal changes nothing");
    e
}

fn findings(live: &Live) -> Vec<Value> {
    live.api_ok("check", json!({}))["findings"]
        .as_array()
        .unwrap()
        .clone()
}

fn of_code<'a>(f: &'a [Value], code: &str) -> Vec<&'a Value> {
    f.iter().filter(|f| f["code"] == code).collect()
}

/// `(id, step)` of the `not_shown` findings.
fn not_shown(f: &[Value]) -> Vec<(String, u64)> {
    of_code(f, "not_shown")
        .iter()
        .map(|f| {
            (
                f["id"].as_str().unwrap().to_string(),
                f["step"].as_u64().unwrap(),
            )
        })
        .collect()
}

fn check_file(path: &Path) -> Value {
    let out = std::process::Command::new(bin())
        .args(["check", "--json"])
        .arg(path)
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
}

fn check_text(path: &Path) -> String {
    let out = std::process::Command::new(bin())
        .arg("check")
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `tiza render SOURCE --all-steps --format txt` (SOURCE: a file, or the
/// live instance with `--socket`): one text per step.
fn all_steps(args: &[&str], out: &Path) -> Vec<String> {
    let _ = std::fs::remove_dir_all(out);
    let r = std::process::Command::new(bin())
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

/// The live instance is what a fresh load of its autosaved `.tiza` gives:
/// the same `check` report and the same `render --all-steps`.
fn same_as_fresh(live: &Live, tiza: &Path, what: &str) {
    live.with(|app| app.finish_typing());
    let mut live_report = live.api_ok("check", json!({}));
    let mut disk_report = check_file(tiza);
    for r in [&mut live_report, &mut disk_report] {
        r.as_object_mut().unwrap().remove("narrower");
    }
    // The live reply also names the revision it checked (t-210).
    live_report.as_object_mut().unwrap().remove("revision");
    assert_eq!(live_report, disk_report, "{what}: live check vs fresh load");
    let dir = tiza.parent().unwrap();
    let sock = live.socket.to_str().unwrap().to_string();
    let live_steps = all_steps(&["--socket", &sock], &dir.join("render-live"));
    let disk_steps = all_steps(&[tiza.to_str().unwrap()], &dir.join("render-disk"));
    assert_eq!(live_steps, disk_steps, "{what}: render --all-steps");
}

/// get → put of what came back is accepted and changes nothing.
fn round_trip(live: &Live, what: &str) {
    live.with(|app| app.finish_typing());
    let before = get(live);
    let r = live.api("scene.put", json!({"document": before["document"]}));
    assert!(r.error.is_none(), "{what}: put refused: {:?}", r.error);
    assert_eq!(get(live)["document"], before["document"], "{what}");
}

fn window(scene: &Value, index: usize) -> Value {
    scene["document"]["moves"][index]["lines"].clone()
}

// ---------------------------------------------------------------- AC1 / AC2

/// Test step 1: tally-tour, a link to count.rs at step 4 hides k2, b4 and
/// n3 (report.rs, step 4): the reply and `check` name them; get → put is
/// accepted; deleting the link shows them again and says nothing.
#[test]
fn ac1_ac2_a_link_at_step_4_of_tally_tour_names_k2_b4_n3() {
    let dir = examples_copy("tally-link");
    let tiza = dir.join("tally-tour.tiza");
    let live = open(&tiza);
    let before = findings(&live);
    assert!(not_shown(&before).is_empty(), "{before:?}");
    let r = add(&live, link(4, "tally/src/count.rs", None));
    assert_eq!(
        r["warning"],
        "k2, b4, n3: their file tally/src/report.rs is not shown at step 4 (tally/src/count.rs is): kept, not drawn there"
    );
    let f = findings(&live);
    assert_eq!(
        not_shown(&f),
        [("k2".into(), 4), ("b4".into(), 4), ("n3".into(), 4)]
    );
    let text = check_text(&tiza);
    assert!(
        text.contains("warning: k2, step 4, tally/src/report.rs, L7: its file tally/src/report.rs is not shown at step 4 (tally/src/count.rs is): kept, but not drawn at its step"),
        "{text}"
    );
    same_as_fresh(&live, &tiza, "tally-tour with the count.rs link");
    round_trip(&live, "tally-tour with the count.rs link");
    let r = delete(&live, 6);
    assert!(r.get("warning").is_none(), "{r}");
    assert_eq!(findings(&live), before);
    same_as_fresh(&live, &tiza, "tally-tour after deleting it");
}

/// Test step 2: stack-and-queue, a link to queue.rs at step 4 hides a6
/// (stack.rs, step 4): named in the reply and in `check`; deleting that
/// link gives today's findings back. a6's file shows at steps 0-1: it is
/// still said, since it is never drawn at its own step.
#[test]
fn ac1_ac2_a_link_at_step_4_of_stack_and_queue_names_a6() {
    let dir = examples_copy("stack-link");
    let tiza = dir.join("stack-and-queue.tiza");
    let live = open(&tiza);
    let before = findings(&live);
    let r = add(&live, link(4, "src/queue.rs", None));
    assert_eq!(
        r["warning"],
        "a6: its file src/stack.rs is not shown at step 4 (src/queue.rs is): kept, not drawn there"
    );
    let f = findings(&live);
    assert_eq!(not_shown(&f), [("a6".into(), 4)]);
    let a6 = of_code(&f, "not_shown")[0];
    assert_eq!(
        a6["message"],
        "its file src/stack.rs is not shown at step 4 (src/queue.rs is): kept, but not drawn at its step"
    );
    assert_eq!(a6["line"], 20);
    assert!(a6.get("file").is_none(), "the main file: {a6}");
    // The window findings are untouched: a5 alone (the zoom is silent).
    let outside: Vec<&Value> = of_code(&f, "outside_window")
        .into_iter()
        .map(|f| &f["id"])
        .collect();
    assert_eq!(outside, [&json!("a5")]);
    same_as_fresh(&live, &tiza, "stack-and-queue with the queue.rs link");
    round_trip(&live, "stack-and-queue with the queue.rs link");
    delete(&live, 7);
    assert_eq!(findings(&live), before);
    same_as_fresh(&live, &tiza, "stack-and-queue after deleting it");
}

/// AC2 on the bar: `:link` at step 4 says the same warning before what it
/// added.
#[test]
fn ac2_the_bar_says_it_for_link() {
    let dir = examples_copy("bar-link");
    let live = open(&dir.join("stack-and-queue.tiza"));
    live.api_ok("step.set", json!({"step": 4}));
    live.command("link src/queue.rs");
    let (m, error) = live.message().expect("a message");
    assert!(!error, "{m}");
    assert!(
        m.starts_with("a6: its file src/stack.rs is not shown at step 4 (src/queue.rs is): kept, not drawn there · move #7 added: step 4 shows src/queue.rs"),
        "{m}"
    );
}

// ---------------------------------------------------------------- AC3

/// AC3, the step path: deleting the step-4 link back to stack.rs leaves
/// step 4 showing queue.rs, the same state a new queue.rs link makes. It is
/// accepted with the same warning, and `check` says the same.
#[test]
fn ac3_deleting_a_link_that_leaves_a_step_on_another_file_is_said_and_accepted() {
    let dir = examples_copy("delete-step");
    let tiza = dir.join("stack-and-queue.tiza");
    let live = open(&tiza);
    let before = get(&live);
    let added = add(&live, link(4, "src/queue.rs", None));
    let after_add = findings(&live);
    delete(&live, 7);
    assert_eq!(get(&live)["document"], before["document"]);
    // Step 4 now shows queue.rs through the step-2 link, with its window:
    // a5 is outside it there, but step 4 did not draw it before either
    // (it showed stack.rs), so only a6 is named, in the same words.
    let r = delete(&live, 5);
    assert_eq!(r["warning"], added["warning"], "{r}");
    assert_eq!(not_shown(&findings(&live)), not_shown(&after_add));
    same_as_fresh(&live, &tiza, "stack-and-queue without its step-4 link");
    round_trip(&live, "stack-and-queue without its step-4 link");
    // Corner case: linking back to the main file shows a6 again and
    // `check` forgets it.
    let r = add(&live, link(4, "src/stack.rs", None));
    assert!(r.get("warning").is_none(), "{r}");
    assert!(not_shown(&findings(&live)).is_empty());
    same_as_fresh(&live, &tiza, "stack-and-queue linked back");
}

/// AC3, the file path: deleting the only link to report.rs is still
/// refused (k2, b4, n3 would belong to no view: the saved `.tiza` would not
/// load them); with another link to report.rs at a later step, the same
/// deletion is accepted and says that step 4 no longer shows them.
#[test]
fn ac3_deleting_the_only_link_to_a_file_stays_refused_and_otherwise_warns() {
    let dir = examples_copy("delete-file");
    let tiza = dir.join("tally-tour.tiza");
    let live = open(&tiza);
    let e = refusal(&live, "move.delete", json!({"index": 3}));
    assert_eq!(
        e,
        "deleting move #3 (step 4): k2, b4, n3 belong to tally/src/report.rs, which no other code.link shows; delete them first"
    );
    // The same after a count.rs link hides them at step 4: what is refused
    // is the file leaving the document, not the step.
    add(&live, link(4, "tally/src/count.rs", None));
    let e = refusal(&live, "move.delete", json!({"index": 3}));
    assert!(e.contains("which no other code.link shows"), "{e}");
    delete(&live, 6);
    // Another link to report.rs, at step 6: deleting the step-4 one is
    // accepted, with the warning a new link would give.
    add(&live, link(6, "tally/src/report.rs", None));
    let r = delete(&live, 3);
    assert_eq!(
        r["warning"],
        "k2, b4, n3: their file tally/src/report.rs is not shown at step 4 (tally/src/count.rs is): kept, not drawn there"
    );
    assert_eq!(
        not_shown(&findings(&live)),
        [("k2".into(), 4), ("b4".into(), 4), ("n3".into(), 4)]
    );
    same_as_fresh(&live, &tiza, "tally-tour with report.rs at step 6 only");
    round_trip(&live, "tally-tour with report.rs at step 6 only");
}

// ---------------------------------------------------------------- AC4

/// Test step 3: stack-and-queue, an insert at line 17 at step 5 (the
/// window 17-23 is that step's link): the window grows to 17-24 and the new
/// line is drawn at step 5; deleting the insert gives 17-23 back. tally-tour
/// the same on its main file (window 9-28 at step 5).
#[test]
fn ac4_an_insert_at_a_window_first_line_at_the_link_step_grows_the_window() {
    let dir = examples_copy("grow");
    for (name, link_index, step, at, was, grown, line) in [
        (
            "stack-and-queue.tiza",
            5,
            5,
            "17",
            "17-23",
            "17-24",
            "    // seen at step 5",
        ),
        (
            "tally-tour.tiza",
            3,
            5,
            "9",
            "9-28",
            "9-29",
            "// seen at step 5",
        ),
    ] {
        let tiza = dir.join(name);
        let live = open(&tiza);
        let before = get(&live);
        assert_eq!(window(&before, link_index), was, "{name}");
        let r = add(
            &live,
            json!({"at_step": step, "action": "code.patch", "op": "insert", "position": at, "content": line}),
        );
        assert!(r.get("warning").is_none(), "{name}: {r}");
        let after = get(&live);
        assert_eq!(window(&after, link_index), grown, "{name}");
        let shown = live.with(move |a| a.doc.code_at(step).unwrap());
        assert_eq!(shown.0.first().map(String::as_str), Some(line), "{name}");
        assert_eq!(shown.1.to_string(), at, "{name}");
        same_as_fresh(&live, &tiza, name);
        round_trip(&live, name);
        let last = after["document"]["moves"].as_array().unwrap().len();
        let r = delete(&live, last);
        assert!(r.get("warning").is_none(), "{name}: {r}");
        assert_eq!(get(&live)["document"], before["document"], "{name}");
        same_as_fresh(&live, &tiza, name);
        drop(live);
    }
}

/// AC4 at other steps: an insert at line 17 at step 3 (before the link)
/// carries the window with the text (18-24), and one at step 6 (after it)
/// leaves the window as it is and pushes the step-2 box out, which the
/// reply says.
#[test]
fn ac4_at_other_steps_the_window_is_carried_as_before() {
    let dir = examples_copy("grow-other-steps");
    let tiza = dir.join("stack-and-queue.tiza");
    let live = open(&tiza);
    let before = get(&live);
    add(
        &live,
        json!({"at_step": 3, "action": "code.patch", "op": "insert", "position": "17", "content": "    // step 3"}),
    );
    assert_eq!(window(&get(&live), 5), "18-24");
    same_as_fresh(&live, &tiza, "insert at step 3");
    delete(&live, 7);
    assert_eq!(get(&live)["document"], before["document"]);
    let r = add(
        &live,
        json!({"at_step": 6, "action": "code.patch", "op": "insert", "position": "17", "content": "    // step 6"}),
    );
    assert_eq!(window(&get(&live), 5), "17-23");
    // AC5: only a3 is pushed out; a4 and a5 were already outside at step 5.
    assert_eq!(
        r["warning"],
        "a3: outside the window at step 6 (src/queue.rs:17-23): kept, not drawn there"
    );
    same_as_fresh(&live, &tiza, "insert at step 6");
    delete(&live, 7);
    assert_eq!(get(&live)["document"], before["document"]);
}

// ---------------------------------------------------------------- AC5

/// A move that creates step 6 and pushes nothing out says nothing (it used
/// to name a4 and a5, outside the step-5 zoom already).
#[test]
fn ac5_a_new_step_does_not_repeat_what_was_already_outside() {
    let dir = examples_copy("new-step");
    let tiza = dir.join("stack-and-queue.tiza");
    let live = open(&tiza);
    let r = add(
        &live,
        json!({"at_step": 6, "action": "code.patch", "op": "insert", "position": "30", "content": "    // later"}),
    );
    assert!(r.get("warning").is_none(), "{r}");
    assert_eq!(r["max_step"], 6);
    same_as_fresh(&live, &tiza, "a new step 6");
}

/// A link with a window at a step that showed another file: what that step
/// did not draw before (a5, of queue.rs, outside the zoom of step 5 only;
/// q2, a queue.rs note of a step that showed stack.rs) is not named, though
/// the new window leaves it outside. a6, drawn at step 4 before, is.
#[test]
fn ac5_a_link_names_only_what_its_step_drew_before() {
    let dir = examples_copy("drawn-before");
    let tiza = dir.join("stack-and-queue.tiza");
    let live = open(&tiza);
    live.api_ok(
        "annotation.add",
        json!({"id": "q2", "step": 4, "file": "src/queue.rs", "type": "note", "line": 4, "text": "outside 10-36"}),
    );
    assert_eq!(not_shown(&findings(&live)), [("q2".into(), 4)]);
    let r = add(&live, link(4, "src/queue.rs", Some("10-36")));
    assert_eq!(
        r["warning"],
        "a6: its file src/stack.rs is not shown at step 4 (src/queue.rs is): kept, not drawn there"
    );
    same_as_fresh(&live, &tiza, "a windowed queue.rs link at step 4");
}

/// The file label is one form in the reply, the bar's message and `check`,
/// live and on disk (the fixed status line keeps the file name): a window on the main file is `src/stack.rs:20-30` (as
/// `code.file` names it) and `tally/src/main.rs:4-9`, never `stack.rs` or
/// `main.rs`.
#[test]
fn ac5_one_file_label_in_the_warning_the_bar_and_check() {
    let dir = examples_copy("labels");
    for (name, file, lines, ids, label) in [
        (
            "stack-and-queue.tiza",
            "src/stack.rs",
            "20-30",
            "a1, a2",
            "src/stack.rs:20-30",
        ),
        (
            "tally-tour.tiza",
            "tally/src/main.rs",
            "4-9",
            "m1, m2, m3, n1, b1",
            "tally/src/main.rs:4-9",
        ),
    ] {
        let tiza = dir.join(name);
        let live = open(&tiza);
        live.api_ok("step.set", json!({"step": 1}));
        live.command(&format!("link {file}:{lines}"));
        let (m, error) = live.message().expect("a message");
        assert!(!error, "{name}: {m}");
        let said = format!("{ids}: outside the window at step 1 ({label}): kept, not drawn there");
        assert!(m.starts_with(&format!("{said} · ")), "{name}: {m}");
        let f = findings(&live);
        // The main file's (no `file`): a5, of queue.rs, has its own window.
        let outside: Vec<&Value> = of_code(&f, "outside_window")
            .into_iter()
            .filter(|f| f.get("file").is_none())
            .collect();
        assert_eq!(outside.len(), ids.split(", ").count(), "{name}: {f:?}");
        for o in &outside {
            let msg = o["message"].as_str().unwrap();
            assert!(msg.contains(&format!("({label})")), "{name}: {msg}");
        }
        same_as_fresh(&live, &tiza, name);
        drop(live);
    }
}

// ---------------------------------------------------------------- corner cases

/// An annotation of a linked file at a step before that file shows (it
/// shows later, never at its own step): still said. An annotation outside
/// the window of its own file is `outside_window` only.
#[test]
fn corner_a_file_shown_only_at_other_steps_and_no_double_warning() {
    let dir = examples_copy("corners");
    let tiza = dir.join("stack-and-queue.tiza");
    let live = open(&tiza);
    live.api_ok(
        "annotation.add",
        json!({"id": "q1", "step": 1, "file": "src/queue.rs", "type": "note", "line": 20, "text": "early"}),
    );
    let f = findings(&live);
    assert_eq!(not_shown(&f), [("q1".into(), 1)]);
    assert_eq!(
        of_code(&f, "not_shown")[0]["message"],
        "its file src/queue.rs is not shown at step 1 (src/stack.rs is): kept, but not drawn at its step"
    );
    assert!(
        of_code(&f, "outside_window")
            .iter()
            .all(|f| f["id"] == "a5"),
        "{f:?}"
    );
    same_as_fresh(&live, &tiza, "a queue.rs note at step 1");
}

/// Every example as shipped: no `not_shown` finding (AC1: their findings
/// stay as they were).
#[test]
fn ac1_the_examples_have_no_annotation_of_a_file_their_step_does_not_show() {
    for e in std::fs::read_dir(example("")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "tiza") {
            let r = check_file(&p);
            let f = r["findings"].as_array().unwrap();
            assert!(not_shown(f).is_empty(), "{}: {f:?}", p.display());
        }
    }
}
