//! t-105 — Moves: `cursor`, `select`, `wait` and `code.link` (the non-patch
//! actions), with one view per FILE (ADR 0007 §3f as rewritten in this card).
//!
//! Acceptance criteria:
//! - AC1 Model: `cursor {line, col?}`, `select {from, to}` (each `find`
//!   exactly once at that step or with `occurrence`), `wait {ms}` and
//!   `code.link {file, lines?}` run when the presenter reaches `at_step`,
//!   are reverted by `p`; `render --step N`, `.cast`/`.tape` and the image
//!   exports apply them instantly (`wait` is a no-op in a still render;
//!   `cursor`/`select` are drawn).
//! - AC2 Views per file: a `code.link` switches the shown file; `lines` is
//!   only a window. Annotations get an optional `file` (auto-filled from the
//!   view shown at their step); lines are real file line numbers; one
//!   outside the window is not drawn, never `(unanchored)`; linking back
//!   shows the first file's annotations untouched.
//! - AC3 `scene.list` names the file of every annotation of a linked file,
//!   the status line shows the file and window, `code.get`/`status` report
//!   the view (`file`, `first_line`, `last_line`).
//! - AC4 `code.patch` patches the file shown at its step; targets are
//!   validated against that file.
//! - AC5 `:embed` stores one entry per linked file under `views`; the saved
//!   document reloads with every view; a missing linked file fails to load
//!   naming the move and the path.
//! - AC6 `:link path[:A-B]` at step N ≥ 1 records a `code.link`; socket
//!   `move.add` and MCP `add_move` accept the four actions.
//! - AC7 Follow and paste stay refused while moves are applied.
//! - AC8 ADR/README/agent guide/CHANGELOG and `examples/stack-and-queue.tiza`.
//! - AC9 This file.

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, example, scratch, tiza, tiza_ok};

/// A scratch copy of the two-file example (`examples/` is never written).
fn demo(name: &str) -> std::path::PathBuf {
    let dir = scratch(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for f in ["src/stack.rs", "src/queue.rs", "stack-and-queue.tiza"] {
        std::fs::copy(example(f), dir.join(f)).unwrap();
    }
    dir
}

fn open_demo(name: &str) -> (std::path::PathBuf, Live) {
    let dir = demo(name);
    let path = dir.join("stack-and-queue.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path, dir.join("live.sock"));
    (dir, live)
}

fn render(path: &std::path::Path, step: &str) -> String {
    tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--step",
        step,
        "--out",
        "-",
        "--width",
        "90",
    ])
}

fn list(live: &Live) -> Vec<String> {
    live.api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

// ---------------------------------------------------------------- AC1

#[test]
fn ac1_the_four_actions_load_apply_and_revert() {
    let (_dir, live) = open_demo("t105-ac1");
    let moves = live.api_ok("move.list", json!({}));
    let summaries: Vec<&str> = moves["moves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["summary"].as_str().unwrap())
        .collect();
    assert_eq!(
        summaries,
        [
            "#1 [step 2] code.link src/queue.rs:10-36",
            "#2 [step 3] select «push_back» → «pop_front»",
            "#3 [step 3] cursor L22:8",
            "#4 [step 3] wait 800ms",
            "#5 [step 4] code.link src/stack.rs",
            "#6 [step 5] code.link src/queue.rs:17-23",
        ]
    );
    // Reaching step 3 with `n`: the cursor jumps, the selection is drawn,
    // the wait holds the step's annotations (any key ends it).
    live.api_ok("step.set", json!({"step": 2}));
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(live.with(|a| a.doc.step), 3);
    assert_eq!(live.with(|a| (a.cursor_line, a.cursor_col)), (22, 8));
    assert!(live.with(|a| a.is_typing()), "the wait is an animation");
    live.keys(&[KeyCode::Esc]);
    assert!(!live.with(|a| a.is_typing()));
    let (sel, cur) = live.with(|a| {
        let v = a.doc.view_at(3);
        (v.selection, v.cursor)
    });
    let (from, to) = sel.expect("selection");
    assert_eq!((from.line, from.col), (18, 19), "start of «push_back»");
    assert_eq!((to.line, to.col), (22, 28), "end of «pop_front»");
    assert_eq!(cur.map(|c| (c.line, c.col)), Some((22, 8)));
    // `p` reverts: no selection or cursor at step 2, the cursor back on the window.
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!(live.with(|a| a.doc.step), 2);
    assert!(live.with(|a| a.doc.view_at(2).selection.is_none()));
    assert!(live.with(|a| a.doc.view_at(2).cursor.is_none()));
    // A still render applies them at once; the wait changes nothing there.
    let path = live.with(|a| a.save_path.clone());
    let s3 = tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--step",
        "3",
        "--out",
        "-",
        "--width",
        "90",
        "--format",
        "svg",
        "--no-embed-fonts",
    ]);
    assert!(
        s3.contains("push_back") && s3.contains("dequeue"),
        "the linked file is rendered:\n{s3}"
    );
    // Selection tint (accent at 0.35 over the box tint) and the reversed
    // cursor cell: both are colours the plain step 2 render lacks.
    let s2 = tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--step",
        "2",
        "--out",
        "-",
        "--width",
        "90",
        "--format",
        "svg",
        "--no-embed-fonts",
    ]);
    let fills = |s: &str| {
        s.split("fill=\"")
            .skip(1)
            .filter_map(|p| p.split('"').next().map(String::from))
            .collect::<std::collections::BTreeSet<String>>()
    };
    let extra: Vec<String> = fills(&s3).difference(&fills(&s2)).cloned().collect();
    assert!(
        extra.len() >= 2,
        "step 3 draws the selection and the cursor: {extra:?}"
    );
}

#[test]
fn ac1_cast_and_tape_sleep_through_the_wait() {
    let dir = demo("t105-cast");
    let path = dir.join("stack-and-queue.tiza");
    let cast = dir.join("demo.cast");
    tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--out",
        cast.to_str().unwrap(),
    ]);
    let events: Vec<Value> = std::fs::read_to_string(&cast)
        .unwrap()
        .lines()
        .skip(1)
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|e| e[1] == "o")
        .collect();
    assert_eq!(events.len(), 6, "steps 0..=5");
    // The frame after step 3 waits the interval plus the 800 ms.
    assert_eq!(events[3][0].as_f64().unwrap(), 2.0);
    assert_eq!(events[4][0].as_f64().unwrap(), 2.8);
    let tape = tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--format",
        "tape",
        "--out",
        "-",
    ]);
    let lines: Vec<&str> = tape.lines().collect();
    let i = lines
        .iter()
        .position(|l| l.starts_with("# step 3"))
        .unwrap();
    assert_eq!(lines[i + 1], "Type \"n\"");
    assert_eq!(
        lines[i + 2],
        "Sleep 2s",
        "800 ms + margin is under the interval"
    );
    // With a short interval the wait (plus the typing margin) wins.
    let tape = tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--format",
        "tape",
        "--interval",
        "0.5",
        "--out",
        "-",
    ]);
    let lines: Vec<&str> = tape.lines().collect();
    let i = lines
        .iter()
        .position(|l| l.starts_with("# step 3"))
        .unwrap();
    let ms: u64 = lines[i + 2]
        .strip_prefix("Sleep ")
        .and_then(|s| s.strip_suffix("ms"))
        .unwrap_or_else(|| panic!("{}", lines[i + 2]))
        .parse()
        .unwrap();
    assert!(ms >= 800 + 500, "{ms}");
}

// ---------------------------------------------------------------- AC2 / AC3

#[test]
fn ac2_ac3_link_forward_and_back_keeps_each_files_annotations() {
    let (_dir, live) = open_demo("t105-ac2");
    let path = live.with(|a| a.save_path.clone());
    let s1 = render(&path, "1");
    assert!(
        s1.contains("LIFO: push and pop") && s1.contains("mutates: needs &mut"),
        "{s1}"
    );
    assert!(
        !s1.contains("FIFO"),
        "queue annotations are not on stack.rs"
    );
    let s2 = render(&path, "2");
    assert!(
        s2.contains("impl<T> Queue<T>")
            && s2.contains("FIFO: in at the back")
            && s2.contains("VecDeque, so both ends"),
        "{s2}"
    );
    assert!(
        !s2.contains("LIFO") && !s2.contains("Stack<T>"),
        "stack.rs and its annotations are not shown while queue.rs is up:\n{s2}"
    );
    // Back on stack.rs at step 4: the step-1 annotations exactly as before,
    // plus the step-4 note.
    let s4 = render(&path, "4");
    assert!(
        s4.contains("LIFO: push and pop")
            && s4.contains("mutates: needs &mut")
            && s4.contains("same signature in both files"),
        "{s4}"
    );
    assert!(!s4.contains("FIFO"), "{s4}");
    // Every annotation is listed, with its file; none is (unanchored).
    live.api_ok("step.set", json!({"step": 2}));
    let l = list(&live);
    assert_eq!(
        l[0],
        "a1 [step 1] box L12-18 «LIFO: push and pop at the same end»"
    );
    assert_eq!(
        l[2],
        "a3 [step 2] box L17-23 «FIFO: in at the back, out at the front» (src/queue.rs)"
    );
    assert_eq!(
        l[4],
        "a5 [step 2] note L4 «outside the window at steps 2-3: kept, not drawn» (src/queue.rs)"
    );
    assert!(l.iter().all(|x| !x.contains("unanchored")), "{l:?}");
    assert_eq!(live.api_ok("move.list", json!({}))["unanchored"], json!([]));
    // status and code.get report the view.
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["file"], "src/queue.rs");
    assert_eq!(st["view"], "src/queue.rs:10-36");
    assert_eq!(
        (st["first_line"].as_u64(), st["last_line"].as_u64()),
        (Some(10), Some(36))
    );
    let code = live.api_ok("code.get", json!({}));
    assert_eq!(code["first_line"], 10);
    assert_eq!(code["last_line"], 36);
    assert_eq!(code["file"], "src/queue.rs");
    assert!(
        code["text"]
            .as_str()
            .unwrap()
            .starts_with("  10  impl<T> Queue<T> {")
    );
    // The status line names the file and window.
    let screen = live.screen(100, 30);
    assert!(screen.contains("↪ src/queue.rs:10-36"), "{screen}");
    assert!(screen.contains("FIFO: in at the back"), "{screen}");
    // Back on the main file: the status line shows it again, nothing listed as unanchored.
    live.api_ok("step.set", json!({"step": 4}));
    let st = live.api_ok("status", json!({}));
    assert!(st["file"].as_str().unwrap().ends_with("stack.rs"));
    assert!(st.get("view").is_none(), "{st}");
    assert_eq!(
        (st["first_line"].as_u64(), st["last_line"].as_u64()),
        (Some(7), Some(30))
    );
    let screen = live.screen(100, 30);
    assert!(
        screen.contains("stack.rs") && !screen.contains("↪"),
        "{screen}"
    );
}

#[test]
fn ac2_window_crops_but_keeps_and_the_same_file_shares_annotations() {
    let (_dir, live) = open_demo("t105-window");
    let path = live.with(|a| a.save_path.clone());
    // a5 (queue.rs L4) is outside the 10-36 window: not drawn, not unanchored.
    let s2 = render(&path, "2");
    assert!(!s2.contains("outside the window"), "{s2}");
    assert!(!s2.contains("unanchored"));
    // Step 5 shows queue.rs:17-23: the step-2 box of the same file is
    // drawn (same view), the note at L13 is cropped away.
    let s5 = render(&path, "5");
    assert!(
        s5.contains("FIFO: in at the back") && s5.contains("the window crops"),
        "{s5}"
    );
    assert!(!s5.contains("VecDeque, so both ends"), "{s5}");
    assert!(!s5.contains(" 13 ") && s5.contains(" 17 "), "{s5}");
    // Widen the window at step 5 to the whole file: a5 appears.
    live.api_ok("move.delete", json!({"index": 6}));
    live.api_ok(
        "move.add",
        json!({"at_step": 5, "action": "code.link", "file": "src/queue.rs"}),
    );
    live.api_ok("step.set", json!({"step": 5}));
    let text = live.api_ok("render.text", json!({}))["text"].clone();
    let text = text.as_str().unwrap();
    assert!(
        text.contains("outside the window") && text.contains("VecDeque, so both ends"),
        "{text}"
    );
    assert_eq!(live.api_ok("status", json!({}))["view"], "src/queue.rs");
}

#[test]
fn ac2_annotations_added_at_a_step_belong_to_the_file_shown_there() {
    let (_dir, live) = open_demo("t105-belong");
    // Added via the API at step 2 (queue shown), with no `file`: it is queue's.
    live.api_ok("step.set", json!({"step": 2}));
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 25, "text": "front, not last", "step": 2}),
    );
    let l = list(&live);
    assert_eq!(
        l.last().unwrap(),
        "a8 [step 2] note L25 «front, not last» (src/queue.rs)"
    );
    // Typed in the TUI while queue.rs is shown: same thing.
    live.command("note 29 len is the same in both");
    assert!(list(&live).last().unwrap().ends_with("(src/queue.rs)"));
    // A main-file annotation at step 2 by naming the file: validated on
    // stack.rs (L25 exists there too) and not drawn while queue.rs is up.
    live.api_ok("scene.patch", json!({"annotations": [{"id": "s1", "step": 2, "file": "src/stack.rs", "type": "note", "line": 25, "text": "stack only"}]}));
    let shown = live.api_ok("render.text", json!({}))["text"].clone();
    assert!(!shown.as_str().unwrap().contains("stack only"));
    live.api_ok("step.set", json!({"step": 4}));
    assert!(
        live.api_ok("render.text", json!({}))["text"]
            .as_str()
            .unwrap()
            .contains("stack only")
    );
    assert!(
        list(&live)
            .iter()
            .any(|x| x == "s1 [step 2] note L25 «stack only»"),
        "the main file carries no suffix: {:?}",
        list(&live)
    );
    // An unknown file is refused.
    let r = live.api(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "x", "file": "src/nope.rs"}),
    );
    let msg = r.error.unwrap().message;
    assert!(
        msg.contains("src/nope.rs") && msg.contains("not a view"),
        "{msg}"
    );
    // Corner: a link and an annotation at the same step: the annotation
    // belongs to the file shown AFTER the step's moves (rule 3d).
    live.api_ok("step.set", json!({"step": 0}));
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 11, "text": "new at 5", "step": 5}),
    );
    assert!(list(&live).last().unwrap().ends_with("(src/queue.rs)"));
}

// ---------------------------------------------------------------- AC4

#[test]
fn ac4_patches_target_the_file_shown_at_their_step() {
    let (_dir, live) = open_demo("t105-patch");
    // At step 2 queue.rs is shown: a find that exists only in stack.rs fails.
    let r = live.api(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "delete", "find": "self.items.pop()"}),
    );
    let msg = r.error.unwrap().message;
    assert!(
        msg.contains("self.items.pop()") && msg.contains("move #7"),
        "{msg}"
    );
    // One that exists in queue.rs patches queue.rs; stack.rs is untouched.
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "self.items.pop_front()", "content": "        self.items.pop_front() // FIFO"}),
    );
    live.api_ok("step.set", json!({"step": 2}));
    let text = live.api_ok("render.text", json!({}))["text"].clone();
    assert!(text.as_str().unwrap().contains("pop_front() // FIFO"));
    live.api_ok("step.set", json!({"step": 4}));
    let text = live.api_ok("render.text", json!({}))["text"].clone();
    assert!(
        text.as_str().unwrap().contains("self.items.pop()")
            && !text.as_str().unwrap().contains("FIFO"),
        "{text}"
    );
    // The patch is still there at step 5 (queue.rs again, another window)
    // and the step-2 box of queue.rs survived it (replace keeps the line).
    live.api_ok("step.set", json!({"step": 5}));
    let text = live.api_ok("render.text", json!({}))["text"].clone();
    assert!(
        text.as_str().unwrap().contains("pop_front() // FIFO")
            && text.as_str().unwrap().contains("FIFO: in at the back"),
        "{text}"
    );
    // A `select` whose text is gone at its step is a load-time error naming the move.
    let dir = scratch("t105-select-missing");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::copy(example("src/queue.rs"), dir.join("src/queue.rs")).unwrap();
    let bad = dir.join("bad.tiza");
    std::fs::write(&bad, json!({
        "schema": 1, "source": {"language": "rs"}, "code": ["fn a() {}"],
        "moves": [
            {"at_step": 1, "action": "code.link", "file": "src/queue.rs"},
            {"at_step": 2, "action": "select", "from": {"find": "nowhere"}, "to": {"find": "pop_front"}}
        ]
    }).to_string()).unwrap();
    let out = tiza(&["render", bad.to_str().unwrap(), "--out", "-"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("move #2") && err.contains("«nowhere»"),
        "{err}"
    );
    // A `find` that matches twice needs `occurrence`.
    std::fs::write(&bad, json!({
        "schema": 1, "source": {"language": "rs"}, "code": ["fn a() {}"],
        "moves": [
            {"at_step": 1, "action": "code.link", "file": "src/queue.rs"},
            {"at_step": 2, "action": "select", "from": {"find": "self.items"}, "to": {"find": "pop_front"}}
        ]
    }).to_string()).unwrap();
    let out = tiza(&["render", bad.to_str().unwrap(), "--out", "-"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("move #2") && err.contains("occurrence"),
        "{err}"
    );
    std::fs::write(&bad, json!({
        "schema": 1, "source": {"language": "rs"}, "code": ["fn a() {}"],
        "moves": [
            {"at_step": 1, "action": "code.link", "file": "src/queue.rs"},
            {"at_step": 2, "action": "select", "from": {"find": "self.items", "occurrence": 2}, "to": {"find": "pop_front"}}
        ]
    }).to_string()).unwrap();
    let out = tiza(&["render", bad.to_str().unwrap(), "--step", "2", "--out", "-"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---------------------------------------------------------------- AC5

#[test]
fn ac5_embed_stores_every_view_and_a_missing_file_names_the_move() {
    let (dir, live) = open_demo("t105-embed");
    live.api_ok("step.set", json!({"step": 3}));
    let before = render(&live.with(|a| a.save_path.clone()), "2");
    live.command("embed");
    live.command("w");
    let path = live.with(|a| a.save_path.clone());
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["code"].as_array().unwrap().len(), 24, "stack.rs 7-30");
    assert_eq!(
        saved["source"]["first_line"], 7,
        "the main file's first line, not the window's"
    );
    let views = saved["views"].as_object().unwrap();
    assert_eq!(views.keys().collect::<Vec<_>>(), ["src/queue.rs"]);
    assert_eq!(views["src/queue.rs"].as_array().unwrap().len(), 42);
    assert_eq!(
        saved["moves"].as_array().unwrap().len(),
        6,
        "the windows stay in the moves"
    );
    // Portable: the same demo from a directory with no sources.
    let alone = scratch("t105-embed-alone").join("demo.tiza");
    std::fs::copy(&path, &alone).unwrap();
    assert_eq!(render(&alone, "2"), before);
    assert!(render(&alone, "5").contains("the window crops"));
    // `scene.get` carries the views too.
    let scene = live.api_ok("scene.get", json!({}));
    assert!(scene["document"]["views"]["src/queue.rs"].is_array());
    assert_eq!(scene["document"]["source"]["first_line"], 7);
    // A link to a file that is not there: the load fails naming move and path.
    std::fs::remove_file(dir.join("src/queue.rs")).unwrap();
    let original = demo("t105-embed-missing");
    std::fs::remove_file(original.join("src/queue.rs")).unwrap();
    let out = tiza(&[
        "render",
        original.join("stack-and-queue.tiza").to_str().unwrap(),
        "--out",
        "-",
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("move #1 (step 2)")
            && err.contains("src/queue.rs")
            && err.contains("could not read"),
        "{err}"
    );
}

// ---------------------------------------------------------------- AC6 / AC7

#[test]
fn ac6_link_command_socket_and_mcp_record_the_moves() {
    let dir = scratch("t105-ac6");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::copy(example("src/stack.rs"), dir.join("src/stack.rs")).unwrap();
    std::fs::copy(example("src/queue.rs"), dir.join("src/queue.rs")).unwrap();
    let doc = tiza_core::fs::open(dir.join("src/stack.rs:7-30").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(
        doc,
        dir.join("src/stack.rs.7-30.tiza"),
        dir.join("live.sock"),
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 12, "text": "push", "step": 1}),
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 16, "text": "pop", "step": 2}),
    );
    // `:link path:A-B` at step 2 records a code.link move.
    live.api_ok("step.set", json!({"step": 2}));
    live.command("link queue.rs:17-23");
    let (msg, is_err) = live.message().unwrap();
    assert!(
        !is_err && msg.contains("move #1 added") && msg.contains("queue.rs:17-23"),
        "{msg}"
    );
    assert_eq!(live.api_ok("status", json!({}))["view"], "queue.rs:17-23");
    assert_eq!(
        live.api_ok("move.list", json!({}))["moves"][0]["summary"],
        "#1 [step 2] code.link queue.rs:17-23"
    );
    live.command("box 17-23 FIFO");
    assert!(list(&live).last().unwrap().ends_with("(queue.rs)"));
    // `:link` at step 0 is the one-shot reference (unchanged behaviour); `:ref` always is.
    live.api_ok("step.set", json!({"step": 0}));
    live.command("link stack.rs:7-30");
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("now references stack.rs"), "{msg}");
    assert_eq!(
        live.api_ok("move.list", json!({}))["moves"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    live.api_ok("step.set", json!({"step": 1}));
    live.command("ref stack.rs:7-30");
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("now references stack.rs"), "{msg}");
    // A bad path is an error, not a move.
    live.api_ok("step.set", json!({"step": 2}));
    live.command("link nope.rs");
    let (msg, is_err) = live.message().unwrap();
    assert!(
        is_err && msg.contains("nope.rs") && msg.contains("could not read"),
        "{msg}"
    );
    assert_eq!(
        live.api_ok("move.list", json!({}))["moves"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Socket: cursor, select and wait; MCP: add_move with `action`.
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "cursor", "line": 18}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "wait", "ms": 300}),
    );
    let r = live.api("move.add", json!({"at_step": 2, "action": "wait", "ms": 0}));
    assert!(r.error.unwrap().message.contains("ms must be >= 1"));
    let r = live.api(
        "move.add",
        json!({"at_step": 2, "action": "cursor", "line": 99}),
    );
    assert!(r.error.unwrap().message.contains("line 99 does not exist"));
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call(
        "add_move",
        json!({"at_step": 2, "action": "select", "from": {"find": "push_back"}, "to": {"find": "item);"}}),
    );
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call(
        "add_move",
        json!({"at_step": 3, "action": "code.link", "file": "stack.rs"}),
    );
    assert!(!err, "{text}");
    let (_, text, _) = mcp.call(
        "add_box",
        json!({"step": 3, "from_line": 8, "to_line": 10, "label": "new"}),
    );
    assert!(text.contains("a4"), "{text}");
    let (_, text, _) = mcp.call("list_annotations", json!({}));
    assert!(
        text.lines().any(|l| l == "a4 [step 3] box L8-10 «new»"),
        "the main file again, no suffix:\n{text}"
    );
    let (_, text, _) = mcp.call("list_moves", json!({}));
    assert!(
        text.contains("select «push_back» → «item);»")
            && text.contains("code.link stack.rs")
            && text.contains("wait 300ms"),
        "{text}"
    );
    let (_, text, _) = mcp.call("get_code", json!({}));
    assert!(text.starts_with("# queue.rs:17-23\n  17  "), "{text}");
    // The MCP tool schema documents the actions.
    let tools = mcp.tools();
    let add = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "add_move")
        .unwrap();
    let props = &add["inputSchema"]["properties"];
    for k in ["action", "file", "lines", "line", "col", "from", "to", "ms"] {
        assert!(props.get(k).is_some(), "{k} missing in add_move schema");
    }
}

#[test]
fn ac7_follow_and_paste_stay_refused_while_a_link_is_applied() {
    let (_dir, live) = open_demo("t105-ac7");
    live.api_ok("step.set", json!({"step": 2}));
    let err = live.with(|a| a.replace_code("fn other() {}\n").unwrap_err().to_string());
    assert!(
        err.contains("cannot paste while moves are applied (step 2)"),
        "{err}"
    );
    let r = live.api("follow.set", json!({"enabled": true}));
    assert!(
        r.error
            .unwrap()
            .message
            .contains("cannot follow while moves are applied")
    );
    // Undo of a link move brings the previous view back.
    live.api_ok("step.set", json!({"step": 1}));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.link", "file": "src/queue.rs"}),
    );
    assert_eq!(live.api_ok("status", json!({}))["view"], "src/queue.rs");
    live.keys(&[KeyCode::Char('u')]);
    let st = live.api_ok("status", json!({}));
    assert!(st.get("view").is_none(), "{st}");
    assert_eq!(st["first_line"], 7);
}

// ---------------------------------------------------------------- corner cases

#[test]
fn corner_linking_the_main_file_is_a_view_change_and_relative_paths_resolve_from_the_document() {
    let dir = scratch("t105-main");
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::copy(example("src/stack.rs"), dir.join("src/stack.rs")).unwrap();
    std::fs::copy(example("src/queue.rs"), dir.join("src/queue.rs")).unwrap();
    let path = dir.join("docs/demo.tiza");
    std::fs::write(&path, json!({
        "schema": 1, "source": {"language": "rs"},
        "code": {"file": "../src/stack.rs", "lines": "7-30"},
        "moves": [
            {"at_step": 2, "action": "code.link", "file": "../src/queue.rs", "lines": "17-23"},
            {"at_step": 3, "action": "code.link", "file": "./../src/stack.rs", "lines": "12-18"}
        ],
        "annotations": [
            {"id": "a1", "step": 1, "type": "note", "line": 12, "text": "push"},
            {"id": "a2", "step": 2, "file": "./../src/queue.rs", "type": "note", "line": 18, "text": "back"},
            {"id": "a3", "step": 3, "file": "../src/stack.rs", "type": "note", "line": 16, "text": "pop"}
        ]
    }).to_string()).unwrap();
    let d = tiza_core::fs::load_document(&path).unwrap();
    assert_eq!(d.annotations[1].file.as_deref(), Some("../src/queue.rs"));
    assert_eq!(
        d.annotations[2].file, None,
        "a link to the main file is view 0"
    );
    assert_eq!(d.link_files(), ["../src/queue.rs"]);
    let s3 = render(&path, "3");
    assert!(
        s3.contains("push") && s3.contains("pop") && s3.contains(" 12 ") && !s3.contains(" 20 "),
        "{s3}"
    );
    let s2 = render(&path, "2");
    assert!(s2.contains("┆ back") && !s2.contains("┆ push"), "{s2}");
}

#[test]
fn corner_rebind_only_on_patches_and_bad_windows_name_the_move() {
    let dir = scratch("t105-corner");
    let bad = dir.join("bad.tiza");
    std::fs::write(
        &bad,
        json!({
            "schema": 1, "source": {"language": "rs"}, "code": ["a", "b"],
            "moves": [{"at_step": 1, "action": "wait", "ms": 10, "rebind": {"x": {"from_line": 1}}}]
        })
        .to_string(),
    )
    .unwrap();
    let out = tiza(&["render", bad.to_str().unwrap(), "--out", "-"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("rebind only applies to code.patch"), "{err}");
    std::fs::write(
        &bad,
        json!({
            "schema": 1, "source": {"language": "rs"}, "code": ["a", "b"],
            "views": {"other.rs": ["x", "y", "z"]},
            "moves": [{"at_step": 1, "action": "code.link", "file": "other.rs", "lines": "3-1"}]
        })
        .to_string(),
    )
    .unwrap();
    let out = tiza(&["render", bad.to_str().unwrap(), "--out", "-"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("move #1 (step 1)") && err.contains("A-B"),
        "{err}"
    );
    // Embedded views load without any file on disk; a window past the end is clamped.
    std::fs::write(&bad, json!({
        "schema": 1, "source": {"language": "rs"}, "code": ["a", "b"],
        "views": {"other.rs": ["x", "y", "z"]},
        "moves": [{"at_step": 1, "action": "code.link", "file": "other.rs", "lines": "2-9"}],
        "annotations": [{"id": "n", "step": 1, "file": "./other.rs", "type": "note", "line": 3, "text": "zed"}]
    }).to_string()).unwrap();
    let s = render(&bad, "1");
    assert!(
        s.contains("2   y") && s.contains("zed") && !s.contains("1   x"),
        "{s}"
    );
    let d = tiza_core::fs::load_document(&bad).unwrap();
    assert_eq!(d.annotations[0].file.as_deref(), Some("other.rs"));
    assert_eq!(d.schema, 1);
}
