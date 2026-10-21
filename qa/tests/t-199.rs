//! t-199 — A `rebind` entry whose annotation is gone never stops a
//! document from opening.
//!
//! Acceptance criteria:
//! - AC1 A `.tiza` whose move has a `rebind` entry naming an annotation that
//!   does not exist opens: the TUI, `tiza render`, the exports and
//!   `scene.put` load it, and the move applies as if the entry were not there.
//! - AC2 `tiza check` reports each such entry as a warning naming the move
//!   and the id (`move #1, step 2: rebind names a1, which is not in the
//!   document; the entry is ignored`); exit 0, 1 with `--strict`. The live
//!   `check` (socket/MCP) reports the same.
//! - AC3 A paste or a follow reload that drops an annotation a `rebind`
//!   names says so in its own notice, once (`… dropped (…; u undoes); move
//!   #1's rebind of a1 is now ignored`); `u` brings the annotation back and
//!   the entry works again, with no notice.
//! - AC4 Saving keeps the entry as written, so undoing the delete later or
//!   adding an annotation with that id again restores the rebind.
//! - AC5 `move.add` and `scene.put` still refuse a `rebind` naming an unknown
//!   id that the caller writes; only what the document already holds loads.
//!
//! Corner cases: two entries, one ignored (the other works); an id a
//! `rebind` names is never given to a new annotation; an ignored entry on a
//! stale move (both reported); `scene.get` → `scene.put` with an ignored
//! entry; a `.tiza` an earlier version saved broken opens.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_core::Document;
use tiza_qa::{Live, bin, fixture, scratch};

/// `tiza render FILE --step N --format txt`, which must succeed.
fn tiza_ok_render(path: &Path, step: u32) -> String {
    let (code, out, err) = run(&[
        "render",
        path.to_str().unwrap(),
        "--step",
        &step.to_string(),
        "--format",
        "txt",
    ]);
    assert_eq!(code, 0, "{err}");
    out
}

/// a1 highlights `t.len()` at step 1; the step-2 patch rewrites line 3 and
/// its `rebind` puts a1 on `t.len()` after it.
const DOC: &str = r#"{
  "schema": 1,
  "source": { "language": "rs", "first_line": 1 },
  "code": ["fn f(t: &str) -> usize {", "    let n = t.len();", "    n + 1", "}"],
  "step": 0,
  "moves": [
    { "at_step": 2, "action": "code.patch", "op": "replace", "find": "n + 1", "content": "    n * 2",
      "rebind": { "a1": { "at": { "find": "t.len()" } } } }
  ],
  "annotations": [
    { "id": "a1", "step": 1, "type": "highlight", "span": { "line": 2, "start": 12, "end": 19 }, "label": "length" }
  ]
}"#;

/// The same code with `t.len()` gone: a1 cannot be re-anchored.
const PASTED: &str = "fn f(t: &str) -> usize {\n    let n = t.chars().count();\n    n + 1\n}\n";

const WARNING: &str =
    "move #1, step 2: rebind names a1, which is not in the document; the entry is ignored";
const IGNORED: &str = "move #1's rebind of a1 is now ignored";

const WAIT: Duration = Duration::from_secs(10);

fn run(args: &[&str]) -> (i32, String, String) {
    let o = Command::new(bin())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run tiza");
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

fn live_doc(text: &str, name: &str) -> (Live, PathBuf) {
    let dir = scratch(name);
    let doc = tiza_core::fs::parse_document(text, None).expect("the document loads");
    let save = dir.join("doc.tiza");
    (Live::open(doc, save.clone(), dir.join("live.sock")), save)
}

fn findings(live: &Live) -> Vec<Value> {
    live.api_ok("check", json!({}))["findings"]
        .as_array()
        .unwrap()
        .clone()
}

fn ignored_findings(f: &[Value]) -> Vec<Value> {
    f.iter()
        .filter(|x| x["code"] == "unknown_id")
        .cloned()
        .collect()
}

/// The status bar (last row) at `cols` columns.
fn bar(live: &Live, cols: u16) -> String {
    let screen = live.screen(cols, 30);
    screen.lines().last().unwrap_or_default().to_string()
}

/// The saved file opens and `tiza check` finds no error in it.
fn opens_clean(path: &Path) {
    tiza_core::fs::load_document(path).expect("the saved file opens");
    let (code, out, _) = run(&["check", path.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(" 0 errors"), "{out}");
}

fn stale_moves(live: &Live) -> usize {
    live.with(|app| tiza_core::stale::count(&app.doc))
}

/// What the saved file holds for move #1's `rebind`.
fn saved_rebind(path: &Path) -> Value {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["moves"][0]["rebind"].clone()
}

#[test]
fn ac1_ac2_a_file_saved_broken_opens_renders_and_checks() {
    // Written by Tiza before the fix after the paste of step 1: a1 dropped,
    // the rebind kept. It used to fail with «rebind names unknown
    // annotation».
    let file = fixture("t-199/saved-broken.tiza");
    let f = file.to_str().unwrap();
    let doc = tiza_core::fs::load_document(&file).expect("it opens");
    // The move applies: step 2 shows the patched line.
    let mut at2 = doc.clone();
    at2.step = 2;
    at2.sync_code_to_step();
    assert_eq!(at2.code[2], "    n * 2");

    let (code, out, _) = run(&["check", f]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(&format!("warning: {WARNING}")), "{out}");
    assert!(out.contains("0 errors"), "{out}");
    let (code, _, _) = run(&["check", "--strict", f]);
    assert_eq!(code, 1);
    let (_, json_out, _) = run(&["check", "--json", f]);
    let v: Value = serde_json::from_str(&json_out).unwrap();
    let w = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["code"] == "unknown_id")
        .unwrap()
        .clone();
    assert_eq!(
        (w["level"].clone(), w["id"].clone(), w["step"].clone()),
        (json!("warning"), json!("move #1"), json!(2))
    );

    // `render --all-steps` draws every step and names the entry once.
    let out_dir = scratch("t199-render").join("steps");
    let dir_arg = format!("{}/", out_dir.display());
    let (code, _, err) = run(&[
        "render",
        f,
        "--all-steps",
        "--format",
        "txt",
        "--out",
        &dir_arg,
    ]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(err.matches("rebind names a1").count(), 1, "{err}");
    assert!(err.contains(&format!("tiza: {WARNING}")), "{err}");
    let step2 = std::fs::read_to_string(out_dir.join("step-02.txt")).unwrap();
    assert!(step2.contains("n * 2"), "{step2}");

    // The TUI opens it and names it on the bar.
    let (live, _) = live_doc(&std::fs::read_to_string(&file).unwrap(), "t199-open");
    assert_eq!(stale_moves(&live), 0);
    let f = findings(&live);
    let w = ignored_findings(&f);
    assert_eq!(w.len(), 1, "{f:?}");
    assert_eq!(w[0]["level"], "warning");
    assert_eq!(w[0]["id"], "move #1");
}

#[test]
fn ac3_ac4_a_paste_that_drops_a1_says_so_once_and_u_restores_it() {
    let (live, save) = live_doc(DOC, "t199-paste");
    live.command("w");
    live.with(|app| app.replace_code(PASTED).unwrap());
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    assert_eq!(
        msg,
        format!(
            "1 annotation dropped (their text is gone; u undoes); {IGNORED} · 4 lines pasted (0 annotations kept)"
        )
    );
    // The bar gives it the room it needs: whole at 120 columns.
    assert!(bar(&live, 120).contains(IGNORED), "{}", bar(&live, 120));
    // The move still applies: its `find` on `t.len()` was a1's, so it no
    // longer counts.
    assert_eq!(stale_moves(&live), 0);
    // Saved as written, and the saved file opens.
    assert_eq!(saved_rebind(&save)["a1"]["at"]["find"], "t.len()");
    tiza_core::fs::load_document(&save).expect("the autosaved file opens");
    let (code, out, _) = run(&["check", save.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(WARNING), "{out}");
    let (code, _, _) = run(&["check", "--strict", save.to_str().unwrap()]);
    assert_eq!(code, 1);
    // The live check says the same.
    assert_eq!(ignored_findings(&findings(&live)).len(), 1);

    // Said once: a later paste that drops nothing else does not repeat it.
    live.with(|app| {
        app.replace_code(&PASTED.replace("n + 1", "n + 1 // one"))
            .unwrap()
    });
    let (msg, _) = live.message().unwrap();
    assert!(!msg.contains("rebind"), "{msg}");
    live.with(|app| app.undo());

    // `u` gives a1 back: the entry works again, check is clean, no notice.
    live.with(|app| app.undo());
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    let (msg, _) = live.message().unwrap();
    assert!(!msg.contains("rebind"), "{msg}");
    let f = findings(&live);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn ac3_a_follow_reload_that_drops_a1_says_so() {
    let dir = scratch("t199-follow");
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    let code = "fn f(t: &str) -> usize {\n    let n = t.len();\n    n + 1\n}\n";
    std::fs::write(&src, code).unwrap();
    let mut doc = Document::from_code(code, "rs", Some(src.display().to_string()));
    doc.reference_file(src.display().to_string(), None);
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    live.command("hl 2:t.len() length");
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "n + 1",
               "content": "    n * 2", "rebind": {"a1": {"at": {"find": "t.len()"}}}}),
    );
    live.command("follow on");
    std::fs::write(&src, PASTED).unwrap();
    let start = Instant::now();
    while live.api_ok("follow.status", json!({}))["reloads"].as_u64() != Some(1) {
        assert!(start.elapsed() < WAIT, "no reload");
        std::thread::sleep(Duration::from_millis(20));
    }
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    assert_eq!(
        msg,
        format!(
            "1 annotation dropped (text gone; u undoes); {IGNORED} · main.rs reloaded (#1): 0 annotations kept"
        )
    );
    let b = bar(&live, 120);
    assert!(b.contains("following") && b.contains(IGNORED), "{b}");
    assert_eq!(stale_moves(&live), 0);
    assert_eq!(ignored_findings(&findings(&live)).len(), 1);
    live.command("follow off");
    live.with(|app| app.undo());
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    assert!(ignored_findings(&findings(&live)).is_empty());
}

#[test]
fn ac5_move_add_and_scene_put_refuse_an_unknown_id_the_caller_writes() {
    let (live, _) = live_doc(DOC, "t199-typo");
    let r = live.api(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "replace", "find": "let n",
               "content": "    let m = t.len();", "rebind": {"zz": {"at": {"find": "t.len()"}}}}),
    );
    let e = r.error.expect("refused").message;
    assert!(e.contains("rebind names unknown annotation «zz»"), "{e}");

    // A hand-written document with that entry: refused too.
    let mut doc: Value = serde_json::from_str(DOC).unwrap();
    doc["moves"][0]["rebind"]["zz"] = json!({"at": {"find": "t.len()"}});
    let r = live.api("scene.put", json!({"document": doc}));
    let e = r.error.expect("refused").message;
    assert!(e.contains("rebind names unknown annotation «zz»"), "{e}");
    assert_eq!(live.with(|app| app.doc.moves[0].rebind.len()), 1);
}

#[test]
fn corner_get_put_round_trip_keeps_the_ignored_entry() {
    let (live, _) = live_doc(DOC, "t199-roundtrip");
    live.with(|app| app.replace_code(PASTED).unwrap());
    // What `scene.get` gives out comes back: the entry is the document's.
    let scene = live.api_ok("scene.get", json!({}));
    let r = live.api("scene.put", json!({"document": scene["document"].clone()}));
    assert!(r.error.is_none(), "{:?}", r.error);
    assert_eq!(live.with(|app| app.doc.moves[0].rebind.len()), 1);
    // The same id with another target is a new entry: refused.
    let mut doc = scene["document"].clone();
    doc["moves"][0]["rebind"]["a1"] = json!({"at": {"find": "n + 1"}});
    let r = live.api("scene.put", json!({"document": doc}));
    let e = r.error.expect("refused").message;
    assert!(e.contains("rebind names unknown annotation «a1»"), "{e}");
}

#[test]
fn ac5_scene_put_keeps_only_the_entry_of_the_same_move() {
    let mut v: Value = serde_json::from_str(DOC).unwrap();
    v["moves"]
        .as_array_mut()
        .unwrap()
        .push(json!({"at_step": 3, "action": "code.patch",
        "op": "insert", "position": "1", "content": "// three"}));
    let (live, _) = live_doc(&v.to_string(), "t199-put-move");
    live.with(|app| app.replace_code(PASTED).unwrap());
    let doc = live.api_ok("scene.get", json!({}))["document"].clone();
    let entry = doc["moves"][0]["rebind"]["a1"].clone();
    let refused = |d: Value, what: &str| {
        let r = live.api("scene.put", json!({"document": d}));
        let e = r
            .error
            .unwrap_or_else(|| panic!("{what}: accepted"))
            .message;
        assert!(
            e.contains("rebind names unknown annotation «a1»"),
            "{what}: {e}"
        );
    };
    // Copied to move #2 as well.
    let mut copied = doc.clone();
    copied["moves"][1]["rebind"] = json!({"a1": entry.clone()});
    refused(copied, "copied");
    // Moved from move #1 to move #2.
    let mut moved = doc.clone();
    moved["moves"][0].as_object_mut().unwrap().remove("rebind");
    moved["moves"][1]["rebind"] = json!({"a1": entry.clone()});
    refused(moved, "moved");
    // Another document, with other code, that carries it.
    let mut other = doc.clone();
    other["code"] = json!([
        "fn g(t: &str) -> usize {",
        "    let n = 0;",
        "    n + 1",
        "}"
    ]);
    refused(other, "other code");
    assert_eq!(live.with(|app| app.doc.moves[1].rebind.len()), 0);
}

#[test]
fn corner_two_entries_one_ignored_the_other_still_rebinds() {
    // a2 highlights `n + 1`, which the patch rewrites: only its rebind keeps
    // it anchored. a1 is gone.
    let mut v: Value = serde_json::from_str(DOC).unwrap();
    v["annotations"] = json!([
        { "id": "a2", "step": 1, "type": "highlight", "span": { "line": 3, "start": 4, "end": 9 } }
    ]);
    v["moves"][0]["rebind"]["a2"] = json!({"at": {"find": "n * 2"}});
    let doc = tiza_core::fs::parse_document(&v.to_string(), None).expect("it opens");
    let p = tiza_core::moves::project(&doc, &doc.moves, 2).unwrap();
    let shapes = doc.shapes_at(&p, 2);
    match &shapes[0] {
        Some(tiza_core::model::Shape::Highlight { span, .. }) => {
            assert_eq!((span.line, span.start, span.end), (3, 4, 9));
        }
        other => panic!("a2 should be rebound onto `n * 2`: {other:?}"),
    }
    assert_eq!(tiza_core::moves::ignored_rebinds(&doc).len(), 1);
}

#[test]
fn corner_an_id_a_rebind_names_is_not_given_out_and_adding_it_back_restores() {
    let text = std::fs::read_to_string(fixture("t-199/saved-broken.tiza")).unwrap();
    let (live, save) = live_doc(&text, "t199-reuse");
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "new"}),
    );
    assert_eq!(r["id"], "a2", "a1 is the rebind's: {r}");
    assert_eq!(ignored_findings(&findings(&live)).len(), 1);
    // That entry's `find` (`t.len()`) matches nothing after the patch: an
    // annotation a1 would make the file fail to load, so it is refused.
    let r = live.api(
        "annotation.add",
        json!({"id": "a1", "type": "highlight", "at": {"line": 2, "find": "count"}, "step": 1}),
    );
    let e = r.error.expect("refused").message;
    assert!(
        e.contains("move #1 (step 2) has a rebind for a1 that would not work with it back")
            && e.contains("«t.len()» matches 0 times after the patch")
            && e.contains("use another id"),
        "{e}"
    );
    live.command("w");
    opens_clean(&save);

    // Where the patch writes `t.len()`, a1 added on purpose picks the entry up.
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["moves"][0]["content"] = json!("    t.len() * 2");
    let (live, save) = live_doc(&v.to_string(), "t199-reuse-ok");
    live.api_ok(
        "annotation.add",
        json!({"id": "a1", "type": "highlight", "at": {"line": 2, "find": "count"}, "step": 1}),
    );
    let f = findings(&live);
    assert!(
        !f.iter().any(|x| x["level"] == "error") && ignored_findings(&f).is_empty(),
        "{f:?}"
    );
    live.command("w");
    opens_clean(&save);
    let doc = tiza_core::fs::load_document(&save).unwrap();
    let p = tiza_core::moves::project(&doc, &doc.moves, 2).unwrap();
    match &doc.shapes_at(&p, 2)[0] {
        Some(tiza_core::model::Shape::Highlight { span, .. }) => {
            assert_eq!((span.line, span.start, span.end), (3, 4, 11));
        }
        other => panic!("a1 should be rebound onto `t.len()`: {other:?}"),
    }
}

#[test]
fn corner_an_annotation_that_comes_back_without_its_rebind_stays_dropped() {
    // x (step 3) is on `val` of `let v = val(1);`, a line the step-2 patch
    // moves down; the rebind's `find` (`KEY`) is gone after the paste, and
    // so is x's text at step 3 while the patch is skipped over that `find`.
    // Without x's entry the patch applies and x's text would be found again:
    // the live instance keeps x dropped and the patch applied, as a fresh
    // load of the saved file has them.
    let text = r#"{
      "schema": 1,
      "source": { "language": "rs", "first_line": 1 },
      "code": ["fn KEY() {", "    val(0);", "    let v = val(1);", "}"],
      "step": 0,
      "moves": [
        { "at_step": 2, "action": "code.patch", "op": "replace", "find": "val(0)",
          "content": "    U1;\n    U2;", "rebind": { "x": { "at": { "find": "KEY" } } } }
      ],
      "annotations": [
        { "id": "x", "step": 3, "type": "highlight", "span": { "line": 4, "start": 12, "end": 15 } }
      ]
    }"#;
    let (live, save) = live_doc(text, "t199-comes-back");
    live.with(|app| {
        app.replace_code("fn K2() {\n    val(0);\n    let w = val(1);\n}\n")
            .unwrap()
    });
    let (msg, err) = live.message().unwrap();
    assert!(
        !err && msg.contains("move #1's rebind of x is now ignored"),
        "{msg}"
    );
    assert!(!msg.contains("stale"), "{msg}");
    assert_eq!(stale_moves(&live), 0);
    assert!(live.with(|app| app.doc.find_annotation("x").is_none()));
    let f = findings(&live);
    assert!(!f.iter().any(|x| x["level"] == "error"), "{f:?}");
    live.with(|app| app.set_step(2));
    assert_eq!(
        live.with(|app| app.doc.code.clone()),
        vec![
            "fn K2() {",
            "    U1;",
            "    U2;",
            "    let w = val(1);",
            "}"
        ]
    );
    // The saved file says the same.
    opens_clean(&save);
    let out = tiza_ok_render(&save, 2);
    assert!(out.contains("U1;") && out.contains("U2;"), "{out}");
}

#[test]
fn corner_an_ignored_entry_by_line_does_not_hold_back_a_move_add() {
    let mut v: Value = serde_json::from_str(DOC).unwrap();
    v["moves"][0]["rebind"] = json!({"a1": {"at": {"line": 3, "col": 4}}});
    v["annotations"] = json!([]);
    let (live, save) = live_doc(&v.to_string(), "t199-by-line");
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "// hi"}),
    );
    assert!(r.error.is_none(), "{:?}", r.error);
    live.command("w");
    opens_clean(&save);
}

#[test]
fn corner_an_ignored_entry_on_a_stale_move_reports_both() {
    let (live, save) = live_doc(DOC, "t199-stale");
    // `t.len()` and the move's target `n + 1` both gone.
    live.with(|app| {
        app.replace_code("fn f(t: &str) -> usize {\n    t.chars().count()\n}\n")
            .unwrap()
    });
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains(IGNORED), "{msg}");
    assert_eq!(stale_moves(&live), 1);
    let f = findings(&live);
    assert!(
        f.iter()
            .any(|x| x["code"] == "stale" && x["id"] == "move #1"),
        "{f:?}"
    );
    assert_eq!(ignored_findings(&f).len(), 1, "{f:?}");
    // On disk the entry is kept as written.
    assert_eq!(saved_rebind(&save)["a1"]["at"]["find"], "t.len()");
}
