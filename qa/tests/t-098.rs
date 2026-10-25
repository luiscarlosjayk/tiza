//! t-098 — Moves: `code.patch` (insert/replace/delete, typing modes) so the
//! code evolves between steps, with deterministic re-anchoring (ADR 0007).
//!
//! Acceptance criteria:
//! - AC1 Model: `moves: [{at_step, action: "code.patch", op, position |
//!   after/before | find | find_start/find_end, content, typing, speed_ms,
//!   rebind}]` in the `.tiza`; `at_step >= 1`; a text target must match
//!   exactly once in the code at that step (load-time error naming the
//!   move); `rebind` ids must exist.
//! - AC2 The document is a pure function of the step: reaching step N
//!   applies the moves `<= N` on the step-0 code; going back recomputes.
//!   Annotations are authored against the code at their own step and
//!   shifted deterministically through later patches (insert/delete shift,
//!   a replaced line keeps its offset, a range shrinks to its surviving
//!   lines, no mapping → hidden and `(unanchored)` in `scene.list`, never
//!   deleted).
//! - AC3 `n` onto a step with a typing move types the content in place
//!   (blank placeholder lines), `speed_ms` per unit; the step's annotations
//!   wait for the end; any key finishes it; `render --step N` is instant.
//! - AC4 Socket `move.add` / `move.delete` / `move.list`; MCP `add_move`,
//!   `delete_move`, `list_moves`; `scene.get` carries `moves`; the embedded
//!   code saved to disk is always the step-0 code.
//! - AC5 Paste and follow are refused while moves are applied; `:embed`
//!   embeds the step-0 code.
//! - AC6 `examples/ownership-moves.rs.tiza` renders every step.
//! - AC7 On a line a `replace` overwrites, line-level annotations (note, box,
//!   callout, spotlight) keep the same-offset line; column anchors
//!   (highlight, mark, arrow ends) go `(unanchored)` unless `rebind`.
//! - AC8 `occurrence: N` (1-based, over the whole code at that step) on
//!   `find`/`find_start`/`after`/`before` (`end_occurrence` for `find_end`)
//!   and on `rebind` finds; N beyond the matches is a load-time error naming
//!   the move; without it, several matches are the AC1 error.

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, example, scratch, tiza, tiza_ok};

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    println!(\"{a} {b}\");\n}\n";

fn live(name: &str) -> Live {
    Live::from_code(CODE, "rs", name)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Row index (in `text`) of the first line containing `needle`.
fn row_of(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} not in:\n{text}"))
}

/// The label `label` is drawn on the row of `code` or within two rows under it.
fn assert_near(text: &str, code: &str, label: &str) {
    let c = row_of(text, code);
    let l = row_of(text, label);
    assert!(
        l >= c && l - c <= 2,
        "{label:?} is not next to {code:?}:\n{text}"
    );
}

fn code_of(live: &Live) -> Vec<String> {
    live.with(|a| a.doc.code.clone())
}

fn insert_after_b(step: u32, typing: &str) -> Value {
    json!({
        "at_step": step, "action": "code.patch", "op": "insert",
        "after": "let b = 2;", "content": "    let c = 3;\n    let d = 4;",
        "typing": typing, "speed_ms": 1
    })
}

// ---------------------------------------------------------------- AC1 / AC4

#[test]
fn ac1_move_add_validates_against_the_code_at_its_step() {
    let live = live("t098-ac1");
    // at_step 0 is the untouched code.
    let r = live.api(
        "move.add",
        json!({"at_step": 0, "action": "code.patch", "op": "delete", "find": "let a"}),
    );
    let e = r.error.unwrap();
    assert_eq!(e.code, "invalid_argument");
    assert!(e.message.contains("at_step must be >= 1"), "{}", e.message);
    // A text target must match exactly once.
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "find": "let"}),
    );
    let e = r.error.unwrap();
    assert!(
        e.message.contains("matches 2 times (L2, L3)"),
        "{}",
        e.message
    );
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "find": "nothing here"}),
    );
    assert_eq!(r.error.unwrap().code, "text_not_found");
    // Insert needs exactly one of position/after/before, and content.
    let r = live.api("move.add", json!({"at_step": 1, "action": "code.patch", "op": "insert", "after": "let a", "before": "let b", "content": "x"}));
    assert!(
        r.error
            .unwrap()
            .message
            .contains("exactly one of position, after or before")
    );
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "after": "let a"}),
    );
    assert!(r.error.unwrap().message.contains("content is required"));
    // Unknown action.
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.rotate", "op": "insert"}),
    );
    assert!(r.error.is_some());
    // rebind must name existing annotations.
    let r = live.api("move.add", json!({"at_step": 1, "action": "code.patch", "op": "delete", "find": "let a", "rebind": {"zz": {"at": {"find": "let b"}}}}));
    assert!(r.error.unwrap().message.contains("unknown annotation «zz»"));
    // Nothing was added by the failures; a valid one lands at index 1.
    assert_eq!(
        live.api_ok("move.list", json!({}))["moves"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let r = live.api_ok("move.add", insert_after_b(2, "instant"));
    assert_eq!(r["index"], 1);
    assert_eq!(r["moves"], 1);
    assert_eq!(r["max_step"], 2, "moves count towards the last step");
    // The second move of the same step resolves on the code after the first.
    let r = live.api(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "delete", "find": "let d = 4;"}),
    );
    assert!(r.error.is_none(), "{:?}", r.error);
    let list = live.api_ok("move.list", json!({}));
    let moves = list["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 2);
    assert_eq!(moves[0]["index"], 1);
    assert_eq!(moves[0]["at_step"], 2);
    assert_eq!(moves[0]["op"], "insert");
    assert!(
        moves[0]["summary"]
            .as_str()
            .unwrap()
            .starts_with("#1 [step 2] code.patch insert after «let b = 2;» 2 lines"),
        "{}",
        moves[0]["summary"]
    );
    assert_eq!(
        moves[1]["summary"],
        "#2 [step 2] code.patch delete «let d = 4;»"
    );
    // A move that would break a later one is refused as a whole.
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "find": "let b = 2;"}),
    );
    let e = r.error.unwrap();
    assert_eq!(e.code, "text_not_found");
    assert!(
        e.message
            .starts_with("move #1, step 2: move #3 (step 1), the one being added, makes it fail (after «let b = 2;» matches nothing"),
        "{}",
        e.message
    );
    assert_eq!(
        live.api_ok("move.list", json!({}))["moves"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // move.delete by 1-based index; out of range is not_found.
    assert_eq!(
        live.api("move.delete", json!({"index": 3}))
            .error
            .unwrap()
            .code,
        "not_found"
    );
    assert_eq!(
        live.api("move.delete", json!({"index": 0}))
            .error
            .unwrap()
            .code,
        "not_found"
    );
    let r = live.api_ok("move.delete", json!({"index": 2}));
    assert_eq!(r["moves"], 1);
    // scene.get carries the moves, with the step-0 code embedded.
    live.api_ok("step.set", json!({"step": 2}));
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(scene["document"]["moves"].as_array().unwrap().len(), 1);
    assert_eq!(
        scene["document"]["code"].as_array().unwrap().len(),
        5,
        "step-0 code on the wire"
    );
    assert_eq!(
        code_of(&live).len(),
        7,
        "but the live code is the projection"
    );
}

// ---------------------------------------------------------------- AC2

#[test]
fn ac2_pure_function_of_the_step_and_deterministic_shift() {
    let live = live("t098-ac2");
    // Annotations of step 1 on the base code.
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 4, "text": "prints", "step": 1}),
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "box", "from_line": 2, "to_line": 4, "step": 1}),
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "mark", "kind": "read", "at": {"find": "let b"}, "step": 1}),
    );
    live.api_ok("move.add", insert_after_b(2, "instant"));
    // Step 3 deletes `let a` (the box loses its first line) and replaces the println.
    live.api_ok(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "delete", "find": "let a = 1;"}),
    );
    live.api_ok("move.add", json!({"at_step": 3, "action": "code.patch", "op": "replace", "find": "println!", "content": "    dbg!(a, b, c, d);\n    // done"}));
    // Step 4 deletes the (replaced) println line → a1 unanchored, rebind-less.
    live.api_ok(
        "move.add",
        json!({"at_step": 4, "action": "code.patch", "op": "delete", "find": "dbg!"}),
    );

    let text_at = |step: u32| {
        live.api_ok("step.set", json!({"step": step}));
        live.api_ok("render.text", json!({"width": 60}))["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let t1 = text_at(1);
    assert!(t1.contains("prints") && t1.contains("let a = 1;"), "{t1}");
    assert_eq!(code_of(&live).len(), 5);

    let t2 = text_at(2);
    assert_eq!(
        code_of(&live),
        [
            "fn main() {",
            "    let a = 1;",
            "    let b = 2;",
            "    let c = 3;",
            "    let d = 4;",
            "    println!(\"{a} {b}\");",
            "}"
        ]
    );
    // The note followed its line (4 → 6), the box grew with the insert (2..6).
    assert_near(&t2, "println!", "prints");
    let list = live.api_ok("scene.list", json!({}));
    let list: Vec<String> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    assert!(list.iter().any(|l| l.contains("box L2-6")), "{list:?}");
    assert!(list.iter().all(|l| !l.contains("(unanchored)")), "{list:?}");

    let t3 = text_at(3);
    assert_eq!(
        code_of(&live),
        [
            "fn main() {",
            "    let b = 2;",
            "    let c = 3;",
            "    let d = 4;",
            "    dbg!(a, b, c, d);",
            "    // done",
            "}"
        ]
    );
    let list: Vec<String> = live
        .api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    // The box shrank to its surviving lines (let b .. the replaced line).
    assert!(list.iter().any(|l| l.contains("box L2-5")), "{list:?}");
    // The replaced line keeps its offset: the note is now on the dbg! line.
    assert_near(&t3, "dbg!", "prints");
    assert!(
        t3.contains("let b = 2;") && !t3.contains("let a = 1;"),
        "{t3}"
    );

    let t4 = text_at(4);
    assert!(!t4.contains("prints"), "the note's line is gone:\n{t4}");
    let list: Vec<String> = live
        .api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    assert!(
        list.iter()
            .any(|l| l.starts_with("a1 ") && l.ends_with("(unanchored)")),
        "{list:?}"
    );
    assert_eq!(
        live.api_ok("move.list", json!({}))["unanchored"],
        json!(["a1"])
    );
    assert_eq!(
        live.with(|a| a.doc.annotations.len()),
        3,
        "hidden, never deleted"
    );

    // Back and forth: the document is identical to the start.
    for _ in 0..10 {
        for s in [4, 1, 3, 0, 2] {
            live.api_ok("step.set", json!({"step": s}));
        }
    }
    let t1b = text_at(1);
    assert_eq!(t1, t1b);
    assert_eq!(code_of(&live), CODE.lines().collect::<Vec<_>>());
    // Stored shapes never changed: the annotations still carry their authoring coordinates.
    assert_eq!(
        live.with(|a| a.doc.annotations[0].shape.line_ranges()),
        vec![(4, 4)]
    );
}

#[test]
fn ac2_annotations_are_authored_against_the_code_at_their_step() {
    let live = live("t098-ac2b");
    live.api_ok("move.add", insert_after_b(2, "instant"));
    // At step 2 the new lines exist: an annotation authored there points at them.
    live.api_ok("step.set", json!({"step": 2}));
    let r = live.api_ok(
        "annotation.add",
        json!({"id": "n", "type": "note", "line": 5, "text": "d", "step": 2}),
    );
    assert_eq!(r["id"], "n");
    let t = live.api_ok("render.text", json!({"width": 60}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_near(&t, "let d = 4;", "┆ d");
    // Step 1 shows the base code; the step-2 note is not drawn and stays anchored to L5 of step 2.
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(code_of(&live).len(), 5);
    // From step 1, adding a step-2 annotation is validated on the step-2 code (7 lines).
    let r = live.api(
        "annotation.add",
        json!({"type": "note", "line": 7, "text": "brace", "step": 2}),
    );
    assert!(r.error.is_none(), "{:?}", r.error);
    let r = live.api(
        "annotation.add",
        json!({"type": "note", "line": 7, "text": "x", "step": 1}),
    );
    assert_eq!(r.error.unwrap().code, "out_of_range", "step 1 has 5 lines");
    // Rebind: a patch that replaces the line of a mark says where it goes.
    live.api_ok(
        "annotation.add",
        json!({"id": "m", "type": "mark", "kind": "read", "at": {"find": "let b"}, "step": 1}),
    );
    live.api_ok("move.add", json!({"at_step": 3, "action": "code.patch", "op": "delete", "find": "let b = 2;", "rebind": {"m": {"at": {"find": "let d"}}}}));
    live.api_ok("step.set", json!({"step": 3}));
    let list: Vec<String> = live
        .api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    let m = list.iter().find(|l| l.starts_with("m ")).unwrap();
    // Code at step 3: fn main, let a, let c, let d, println, } → `let d` is L4.
    assert_eq!(code_of(&live)[3], "    let d = 4;");
    assert_eq!(
        m, "m [step 1] mark read L4 4..9",
        "rebound onto `let d`, not unanchored"
    );
    // The same list at step 2 shows it on `let b` (unshifted), as the pre-move code has it.
    live.api_ok("step.set", json!({"step": 2}));
    let list: Vec<String> = live
        .api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    assert_eq!(
        list.iter().find(|l| l.starts_with("m ")).unwrap(),
        "m [step 1] mark read L3 4..9"
    );
    live.api_ok("step.set", json!({"step": 3}));
    // A rebind target that does not exist is a load-time error naming the move and the id.
    let r = live.api("move.add", json!({"at_step": 4, "action": "code.patch", "op": "delete", "find": "let c = 3;", "rebind": {"m": {"at": {"find": "nowhere"}}}}));
    let e = r.error.unwrap();
    assert!(
        e.message
            .contains("move #3, step 4: rebind «m» «nowhere» matches 0 times"),
        "{}",
        e.message
    );
}

// ---------------------------------------------------------------- AC3

#[test]
fn ac3_typing_animation_in_the_tui() {
    let live = live("t098-ac3");
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 2, "text": "first", "step": 1}),
    );
    // A slow typing move, so we can observe the placeholder lines.
    live.api_ok(
        "move.add",
        json!({
            "at_step": 2, "action": "code.patch", "op": "insert",
            "after": "let b = 2;", "content": "    let c = 3;\n    let d = 4;",
            "typing": "line", "speed_ms": 400
        }),
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 5, "text": "fourth", "step": 2}),
    );
    live.api_ok("step.set", json!({"step": 1}));
    live.keys(&[KeyCode::Char('n')]);
    assert!(live.with(|a| a.is_typing()));
    let code = code_of(&live);
    assert_eq!(code.len(), 7, "placeholder lines keep the line count");
    assert_eq!(&code[3..5], ["", ""], "{code:?}");
    let screen = live.screen(60, 14);
    assert!(
        screen.contains("first"),
        "earlier annotations stay:\n{screen}"
    );
    assert!(
        !screen.contains("fourth"),
        "the step's annotations wait:\n{screen}"
    );
    // Any key finishes it (the key itself is consumed: still at step 2).
    live.keys(&[KeyCode::Char('j')]);
    assert!(!live.with(|a| a.is_typing()));
    assert_eq!(code_of(&live)[3], "    let c = 3;");
    assert_eq!(live.with(|a| a.doc.step), 2);
    let screen = live.screen(60, 14);
    assert!(screen.contains("fourth"), "{screen}");
    // Going back and forward again restarts the animation; `p` cancels it.
    live.keys(&[KeyCode::Char('p'), KeyCode::Char('n')]);
    assert!(live.with(|a| a.is_typing()));
    live.api_ok("step.prev", json!({}));
    assert!(!live.with(|a| a.is_typing()));
    assert_eq!(code_of(&live).len(), 5);
    // A fast one runs to completion on its own (ticks in the loop). The
    // step-2 note is on a line the move wrote, and deleting a move carries
    // the annotations back through it: refused, naming the note, until the
    // note goes.
    let r = live.api("move.delete", json!({"index": 1}));
    let e = r.error.unwrap().message;
    assert!(
        e.starts_with(
            "annotation a2, step 2: deleting move #1 (step 2) removes line 5, where it is"
        ),
        "{e}"
    );
    live.api_ok("annotation.delete", json!({"id": "a2"}));
    live.api_ok("move.delete", json!({"index": 1}));
    live.api_ok("move.add", insert_after_b(2, "char"));
    live.keys(&[KeyCode::Char('n')]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while live.with(|a| a.is_typing()) {
        assert!(
            std::time::Instant::now() < deadline,
            "typing never finished"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(code_of(&live)[4], "    let d = 4;");
    // Undo while typing cancels the animation first.
    live.keys(&[KeyCode::Char('p')]);
    live.api_ok("move.delete", json!({"index": 1}));
    live.api_ok(
        "move.add",
        json!({
            "at_step": 2, "action": "code.patch", "op": "insert",
            "after": "let b = 2;", "content": "    let c = 3;", "typing": "char", "speed_ms": 1000
        }),
    );
    live.keys(&[KeyCode::Char('n')]);
    assert!(live.with(|a| a.is_typing()));
    live.with(|a| a.undo());
    assert!(!live.with(|a| a.is_typing()));
}

#[test]
fn ac3_render_and_export_apply_moves_instantly() {
    let dir = scratch("t098-render");
    let path = dir.join("ex.tiza");
    std::fs::copy(example("ownership-moves.rs.tiza"), &path).unwrap();
    let p = path.to_str().unwrap();
    let s1 = tiza_ok(&["render", p, "--step", "1", "--out", "-", "--width", "80"]);
    assert!(!s1.contains("let t = String"), "{s1}");
    let s2 = tiza_ok(&["render", p, "--step", "2", "--out", "-", "--width", "80"]);
    assert!(
        s2.contains("let t = String::from(\"world\")") && s2.contains("fn len(s: &String)"),
        "{s2}"
    );
    let s4 = tiza_ok(&["render", p, "--step", "4", "--out", "-", "--width", "80"]);
    assert!(
        s4.contains("let n = t.len();") && !s4.contains("fn len("),
        "{s4}"
    );
    assert!(
        s4.contains("a shared borrow"),
        "the rebound mark follows:\n{s4}"
    );
    assert!(
        !s4.contains("len only reads"),
        "the deleted block's box is unanchored:\n{s4}"
    );
    // No stale-annotation warning: a5/a6 are valid at their own step.
    let out = tiza(&["render", p, "--step", "1", "--out", "-", "--width", "80"]);
    assert!(
        String::from_utf8_lossy(&out.stderr).trim().is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // SVG and the asciicast go through the same projection.
    let svg = tiza_ok(&[
        "render",
        p,
        "--step",
        "4",
        "--out",
        "-",
        "--format",
        "svg",
        "--no-embed-fonts",
        "--width",
        "80",
    ]);
    assert!(svg.contains("t.len()") && !svg.contains("fn len("), "svg");
    let cast = dir.join("ex.cast");
    tiza_ok(&["render", p, "--out", cast.to_str().unwrap()]);
    let frames: Vec<String> = std::fs::read_to_string(cast)
        .unwrap()
        .lines()
        .skip(1)
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|e| e[1] == "o")
        .map(|e| strip_ansi(e[2].as_str().unwrap()))
        .collect();
    assert_eq!(frames.len(), 6, "steps 0..=5");
    assert!(
        frames[2].contains("fn len(s: &String)") && !frames[1].contains("fn len("),
        "step 2 types the helper"
    );
    assert!(
        frames[4].contains("t.len()") && !frames[4].contains("fn len("),
        "step 4 replaces it"
    );
}

// ---------------------------------------------------------------- AC4 (MCP) / AC5

#[test]
fn ac4_mcp_tools_and_saved_file_keep_the_base_code() {
    let dir = scratch("t098-mcp");
    let doc = tiza_core::Document::from_code(CODE, "rs", None);
    let save = dir.join("doc.tiza");
    let live = Live::open(doc, save.clone(), dir.join("live.sock"));
    let mut mcp = McpClient::start(&live.socket);
    let names = mcp.tool_names();
    for t in ["add_move", "delete_move", "list_moves"] {
        assert!(names.iter().any(|n| n == t), "{t} missing: {names:?}");
    }
    let (err, text, _) = mcp.call("add_move", json!({"at_step": 2, "op": "insert", "after": "let b = 2;", "content": "    let c = 3;", "typing": "line"}));
    assert!(!err, "{text}");
    let (err, text, _) = mcp.call(
        "add_move",
        json!({"at_step": 2, "op": "delete", "find": "let"}),
    );
    assert!(err && text.contains("matches"), "{text}");
    let (err, text, _) = mcp.call("list_moves", json!({}));
    assert!(!err);
    assert!(
        text.contains("#1 [step 2] code.patch insert after «let b = 2;» 1 line typing=line 120ms"),
        "{text}"
    );
    // Saved file: embedded code is the step-0 code even while at step 2.
    live.api_ok("step.set", json!({"step": 2}));
    live.command("w");
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&save).unwrap()).unwrap();
    assert_eq!(saved["code"].as_array().unwrap().len(), 5);
    assert_eq!(saved["moves"][0]["at_step"], 2);
    assert_eq!(saved["step"], 2);
    // Reopening lands on the projection at the saved step.
    let reopened = tiza_core::fs::load_document(&save).unwrap();
    assert_eq!(reopened.code.len(), 6);
    assert_eq!(reopened.base().len(), 5);
    let (err, text, _) = mcp.call("delete_move", json!({"index": 1}));
    assert!(!err, "{text}");
    assert_eq!(code_of(&live).len(), 5);
    assert_eq!(
        live.with(|a| a.doc.step),
        1,
        "the step is clamped to what is left"
    );
}

#[test]
fn ac5_paste_is_refused_and_follow_waits_while_moves_are_applied() {
    let dir = scratch("t098-ac5");
    std::fs::write(dir.join("src.rs"), CODE).unwrap();
    let doc = tiza_core::fs::open(dir.join("src.rs").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("src.rs.tiza"), dir.join("live.sock"));
    live.api_ok("move.add", insert_after_b(1, "instant"));
    assert_eq!(live.with(|a| a.doc.step), 1);
    let err = live.with(|a| a.replace_code("fn other() {}\n").unwrap_err().to_string());
    assert!(
        err.contains("cannot paste while moves are applied (step 1)"),
        "{err}"
    );
    // Follow turns on and waits for step 0 (it used to be refused).
    let r = live.api_ok("follow.set", json!({"enabled": true}));
    assert_eq!(r["enabled"], true, "{r}");
    assert_eq!(
        r["reason"], "waiting: moves applied at step 1; loads at step 0",
        "{r}"
    );
    live.api_ok("follow.set", json!({"enabled": false}));
    // At step 0 nothing is applied: a paste works on the step-0 code.
    live.api_ok("step.set", json!({"step": 0}));
    live.with(|a| {
        a.replace_code("fn main() {\n    let a = 1;\n    let b = 2;\n    let z = 0;\n}\n")
            .unwrap()
    });
    assert_eq!(code_of(&live).len(), 5);
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(
        code_of(&live),
        [
            "fn main() {",
            "    let a = 1;",
            "    let b = 2;",
            "    let c = 3;",
            "    let d = 4;",
            "    let z = 0;",
            "}"
        ]
    );
    // `:embed` freezes the step-0 code.
    live.command("embed");
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(scene["document"]["code"].as_array().unwrap().len(), 5);
    assert!(live.with(|a| a.doc.is_embedded()));
    // The :list overlay shows the moves and the unanchored marks.
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 4, "text": "cee", "step": 1}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "delete", "find": "let c = 3;"}),
    );
    live.api_ok("step.set", json!({"step": 2}));
    live.command("list");
    let screen = live.screen(80, 20);
    assert!(
        screen.contains("! unanchored") && screen.contains("#2  step 2  code.patch  delete"),
        "{screen}"
    );
}

#[test]
fn ac1_load_time_errors_name_the_move() {
    let dir = scratch("t098-load");
    let bad = dir.join("bad.tiza");
    std::fs::write(
        &bad,
        json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1},
            "code": ["a", "b", "a"],
            "moves": [{"at_step": 1, "action": "code.patch", "op": "delete", "find": "a"}]
        })
        .to_string(),
    )
    .unwrap();
    let out = tiza(&["render", bad.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("move #1, step 1") && err.contains("matches 2 times"),
        "{err}"
    );
    // A move whose position runs past the code.
    std::fs::write(&bad, json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1},
        "code": ["a", "b"],
        "moves": [{"at_step": 1, "action": "code.patch", "op": "insert", "position": "9", "content": "x"}]
    }).to_string()).unwrap();
    let out = tiza(&["render", bad.to_str().unwrap(), "--out", "-"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("line 9 does not exist"), "{err}");
    // A range whose end marker comes before its start marker quotes the
    // end marker as written (moved here from t-167).
    std::fs::write(
        &bad,
        json!({
            "schema": 1, "source": {"language": "ini", "first_line": 1},
            "code": ["[api]", "token=abc123Q7valueXyz789", "region=eu-west-1", "port=8080"],
            "moves": [{"at_step": 2, "action": "code.patch", "op": "replace",
                       "find_start": "region", "find_end": "Q7valueXyz789", "content": "x"}]
        })
        .to_string(),
    )
    .unwrap();
    let out = tiza(&["render", bad.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("find_end «Q7valueXyz789»"), "{err}");
}

// ---------------------------------------------------------------- AC7 / AC8

#[test]
fn ac7_replaced_line_keeps_line_anchors_and_unanchors_column_anchors() {
    let live = live("t098-ac7");
    // Line 3 `    let b = 2;` carries a note (line) and a highlight (columns); line 4 a mark.
    live.api_ok(
        "annotation.add",
        json!({"id": "note", "type": "note", "line": 3, "text": "two", "step": 1}),
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "hl", "type": "highlight", "at": {"find": "b = 2"}, "step": 1}),
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "box", "type": "box", "from_line": 2, "to_line": 4, "step": 1}),
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "mk", "type": "mark", "kind": "read", "at": {"find": "{a} {b}"}, "step": 1}),
    );
    live.api_ok("annotation.add", json!({"id": "arrow", "type": "arrow", "from": {"find": "let b"}, "to": {"find": "{b}"}, "step": 1}));
    live.api_ok("move.add", json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "let b = 2;", "content": "    let b = a + 1;"}));
    live.api_ok("step.set", json!({"step": 2}));
    assert_eq!(code_of(&live)[2], "    let b = a + 1;");
    let list = live.api_ok("move.list", json!({}));
    assert_eq!(list["unanchored"], json!(["hl", "arrow"]), "{list}");
    let lines: Vec<String> = live
        .api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("note ") && l.contains("L3") && !l.contains("unanchored")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("box ") && l.contains("L2-4")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("mk ") && l.contains("L4") && !l.contains("unanchored")),
        "the mark is after the edit: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("hl ") && l.ends_with("(unanchored)")),
        "{lines:?}"
    );
    // Back on `p`.
    live.api_ok("step.prev", json!({}));
    assert_eq!(live.api_ok("move.list", json!({}))["unanchored"], json!([]));
    // With a rebind the highlight follows the new text.
    live.api_ok("move.delete", json!({"index": 1}));
    live.api_ok("move.add", json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "let b = 2;", "content": "    let b = a + 1;", "rebind": {"hl": {"at": {"find": "a + 1"}}}}));
    live.api_ok("step.set", json!({"step": 2}));
    assert_eq!(
        live.api_ok("move.list", json!({}))["unanchored"],
        json!(["arrow"])
    );
    let lines: Vec<String> = live
        .api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("hl ") && l.contains("L3 12..17")),
        "{lines:?}"
    );
    // The screen agrees: the highlight's label-less span is on `a + 1`, the note is drawn.
    let t = live.api_ok("render.text", json!({"width": 60}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(t.contains("two"), "{t}");
}

#[test]
fn ac8_occurrence_over_the_whole_code_at_that_step() {
    let live = Live::from_code(
        "fn main() {\n    let a = 1;\n    let a = a + 1;\n    let b = a;\n}\n",
        "rs",
        "t098-ac8",
    );
    // Without occurrence: the AC1 error, counting every match (L3 has two).
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "find": "a"}),
    );
    let e = r.error.unwrap();
    assert_eq!(e.code, "invalid_argument");
    assert!(
        e.message.contains("matches 5 times (L1, L2, L3, L3, L4)")
            && e.message.contains("occurrence: N"),
        "{}",
        e.message
    );
    // Reading order: `main` (1), line 2 (2), line 3 twice (3, 4), line 4 (5).
    let r = live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "find": "a", "occurrence": 5}),
    );
    assert_eq!(r["index"], 1);
    assert_eq!(
        code_of(&live),
        ["fn main() {", "    let a = 1;", "    let a = a + 1;", "}"]
    );
    // Beyond the matches: a load-time error naming the move.
    let r = live.api("move.add", json!({"at_step": 2, "action": "code.patch", "op": "delete", "find": "let", "occurrence": 3}));
    let e = r.error.unwrap();
    assert!(
        e.message.contains("move #2, step 2")
            && e.message.contains("occurrence 3 asked, only 2 matches"),
        "{}",
        e.message
    );
    // `after` with occurrence, and `end_occurrence` for find_end.
    live.api_ok("move.add", json!({"at_step": 2, "action": "code.patch", "op": "insert", "after": "let a", "occurrence": 2, "content": "    // second a"}));
    live.api_ok("step.set", json!({"step": 2}));
    assert_eq!(code_of(&live)[3], "    // second a");
    // find_start needs its own occurrence (`let a` is on two lines); `a` of `// second a` is the 5th match.
    let r = live.api("move.add", json!({"at_step": 3, "action": "code.patch", "op": "delete", "find_start": "let a", "find_end": "a", "end_occurrence": 5}));
    assert!(
        r.error
            .unwrap()
            .message
            .contains("find_start «let a» matches 2 times")
    );
    live.api_ok("move.add", json!({"at_step": 3, "action": "code.patch", "op": "delete", "find_start": "let a", "occurrence": 1, "find_end": "a", "end_occurrence": 5}));
    live.api_ok("step.set", json!({"step": 3}));
    assert_eq!(code_of(&live), ["fn main() {", "}"], "{:?}", code_of(&live));
}

#[test]
fn ac8_rebind_find_must_match_once_or_carry_occurrence() {
    let live = Live::from_code("fn main() {\n    let b = 1;\n}\n", "rs", "t098-ac8b");
    live.api_ok(
        "annotation.add",
        json!({"id": "h", "type": "highlight", "at": {"find": "let b"}, "step": 1}),
    );
    let r = live.api("move.add", json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "let b = 1;", "content": "    let c = 1; let d = 1;", "rebind": {"h": {"at": {"find": "= 1;"}}}}));
    let e = r.error.unwrap();
    assert!(
        e.message
            .contains("move #1, step 2: rebind «h» «= 1;» matches 2 times"),
        "{}",
        e.message
    );
    let r = live.api("move.add", json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "let b = 1;", "content": "    let c = 1; let d = 1;", "rebind": {"h": {"at": {"find": "= 1;", "occurrence": 3}}}}));
    assert!(
        r.error
            .unwrap()
            .message
            .contains("occurrence 3 asked, only 2 matches")
    );
    live.api_ok("move.add", json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "let b = 1;", "content": "    let c = 1; let d = 1;", "rebind": {"h": {"at": {"find": "= 1;", "occurrence": 2}}}}));
    live.api_ok("step.set", json!({"step": 2}));
    let lines: Vec<String> = live
        .api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("h ") && l.contains("L2 21..25")),
        "{lines:?}"
    );
    assert_eq!(live.api_ok("move.list", json!({}))["unanchored"], json!([]));
    // The document carries `occurrence` on the wire and on disk.
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(
        scene["document"]["moves"][0]["rebind"]["h"]["at"]["occurrence"],
        2
    );
}

// ---------------------------------------------------------------- AC9

#[test]
fn ac9_paste_at_step_0_reanchors_each_annotation_on_its_own_step() {
    let dir = scratch("t098-ac9");
    let path = dir.join("ex.tiza");
    std::fs::copy(example("ownership-moves.rs.tiza"), &path).unwrap();
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let base = doc.code.join("\n") + "\n";
    let ids = |live: &Live| -> Vec<String> {
        let mut v: Vec<String> =
            live.with(|a| a.doc.annotations.iter().map(|a| a.id.clone()).collect());
        v.sort();
        v
    };
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    let all = ids(&live);
    assert_eq!(all.len(), 8);

    // Pasting the identical step-0 code drops nothing: a5 (arrow onto the
    // `len` helper) and a6 (box around it) live on step-3 code that only
    // exists after the step-2 moves, and are matched there.
    live.with(move |a| a.replace_code(&base).unwrap());
    let (msg, err) = live.message().unwrap();
    assert!(
        !err && msg.contains("9 lines pasted (8 annotations kept)"),
        "{msg}"
    );
    assert_eq!(ids(&live), all);
    live.api_ok("step.set", json!({"step": 3}));
    assert!(live.with(|a| a.doc.unanchored().is_empty()));
    let screen = strip_ansi(&live.screen(100, 30));
    assert!(
        screen.contains("borrow") && screen.contains("len only reads"),
        "{screen}"
    );
    live.api_ok("step.set", json!({"step": 0}));

    // Rewriting the call so the `s` under the step-1 mark is gone drops
    // only a1; the step-3/4/5 annotations stay, re-anchored on their own step.
    let renamed = live.with(|a| {
        a.doc
            .code
            .join("\n")
            .replace("takes(s);", "give(String::new());")
            + "\n"
    });
    live.with(move |a| a.replace_code(&renamed).unwrap());
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("1 annotation dropped"), "{msg}");
    assert_eq!(ids(&live), ["a2", "a3", "a4", "a5", "a6", "a7", "a8"]);
    live.api_ok("step.set", json!({"step": 4}));
    let mut unanchored = live.with(|a| a.doc.unanchored());
    unanchored.sort();
    assert_eq!(
        unanchored,
        ["a5", "a6"],
        "a5/a6 unanchored at step 4 as before the paste"
    );
    // `u` brings a1 back.
    live.api_ok("step.set", json!({"step": 0}));
    live.with(|a| a.undo());
    assert_eq!(ids(&live), all);
}

// ---------------------------------------------------------------- AC10

#[test]
fn ac10_tape_sleeps_through_the_typing() {
    let dir = scratch("t098-ac10");
    let path = dir.join("long.tiza");
    let content = (0..20)
        .map(|i| format!("    let x{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &path,
        json!({
            "schema": 1,
            "source": {"language": "rs"},
            "code": ["fn main() {", "    let a = 1;", "}"],
            "moves": [{
                "at_step": 2, "action": "code.patch", "op": "insert",
                "after": "let a = 1;", "content": content, "typing": "line", "speed_ms": 160
            }],
            "annotations": [
                {"id": "a1", "step": 1, "type": "note", "line": 1, "text": "one"},
                {"id": "a3", "step": 3, "type": "note", "line": 2, "text": "three"}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let tape = tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--format",
        "tape",
        "--out",
        "-",
    ]);
    let lines: Vec<&str> = tape.lines().collect();
    // Step 2 types 20 lines at 160 ms: the sleep covers 3.2 s plus a margin,
    // so the `n` of step 3 is not swallowed by the animation.
    let i = lines
        .iter()
        .position(|l| l.starts_with("# step 2"))
        .unwrap_or_else(|| panic!("{tape}"));
    assert_eq!(lines[i + 1], "Type \"n\"");
    let sleep = lines[i + 2].strip_prefix("Sleep ").unwrap();
    let ms: u64 = sleep.strip_suffix("ms").unwrap().parse().unwrap();
    assert!(ms >= 20 * 160 + 500, "{sleep}");
    // The other steps keep the plain interval.
    let j = lines.iter().position(|l| *l == "# step 3").unwrap();
    assert_eq!(lines[j + 2], "Sleep 2s");
    assert_eq!(tape.matches("Type \"n\"").count(), 3, "steps 1, 2 and 3");
}

// ---------------------------------------------------------------- rework minors

#[test]
fn rebind_with_no_target_or_an_unknown_key_is_a_load_error() {
    let dir = scratch("t098-rebind-keys");
    let doc = |rebind: Value| {
        json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1},
            "code": ["fn a() {", "    takes(s);", "}"],
            "annotations": [{"id": "m1", "step": 1, "type": "mark", "kind": "move",
                             "span": {"line": 2, "start": 10, "end": 11}}],
            "moves": [{"at_step": 2, "action": "code.patch", "op": "replace",
                       "find": "takes(s);", "content": "    give(s);", "rebind": rebind}]
        })
        .to_string()
    };
    // The ADR's old spelling, `find` without `at`: an unknown key.
    let g = dir.join("g.tiza");
    std::fs::write(&g, doc(json!({"m1": {"find": "give"}}))).unwrap();
    let out = tiza(&["render", g.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown field `find`"), "{err}");
    // An empty spec says nowhere to go.
    let e = dir.join("e.tiza");
    std::fs::write(&e, doc(json!({"m1": {}}))).unwrap();
    let out = tiza(&["render", e.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("move #1, step 2: rebind «m1» says nowhere to go"),
        "{err}"
    );
    // The right spelling loads and the mark follows.
    let h = dir.join("h.tiza");
    std::fs::write(&h, doc(json!({"m1": {"at": {"find": "s);"}}}))).unwrap();
    let s2 = tiza_ok(&["render", h.to_str().unwrap(), "--out", "-", "--step", "2"]);
    assert!(s2.contains("give(s);"), "{s2}");
}

#[test]
fn move_add_rejected_by_a_later_move_names_that_move_and_render_step_past_the_end_warns() {
    let live = live("t098-minors");
    // Move #1 at step 2 inserts at the end; move #2 at step 3 deletes line 9.
    live.api_ok("move.add", insert_after_b(2, "instant"));
    live.api_ok(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "delete", "position": "7"}),
    );
    // A new step-1 move that removes the line move #2 deletes leaves move #2
    // without its target: the error names move #2 and its step, not just
    // the line. A move that only shortens the code carries the later moves'
    // positions instead (qa/tests/t-188.rs).
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "position": "5"}),
    );
    let err = r.error.unwrap().message;
    assert!(
        err.contains("move #2, step 3") && err.contains("removes line 7, the line it changes"),
        "{err}"
    );
    let dir = scratch("t098-render-step");
    let path = dir.join("ex.tiza");
    std::fs::copy(example("ownership-moves.rs.tiza"), &path).unwrap();
    let out = tiza(&[
        "render",
        path.to_str().unwrap(),
        "--out",
        "-",
        "--step",
        "9",
    ]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("step 9 does not exist (the document has steps 0 to 5); showing step 5"),
        "{err}"
    );
    let shown = String::from_utf8_lossy(&out.stdout);
    assert!(shown.contains("The rule"), "step 5 is rendered:\n{shown}");
}
