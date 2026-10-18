//! t-099 — Named steps (`steps: [{step, id, title, auto_advance_ms}]`),
//! `step.goto {id}` / MCP `goto_step`, `tiza next|prev|goto ID`,
//! auto-advance per step and `loop` (automation only, no timer).
//!
//! Acceptance criteria:
//! - AC1 `step.goto {id}` jumps; an unknown id is `not_found`.
//! - AC2 The status line shows `step 3/7 · borrow` when titles exist.
//! - AC3 `auto_advance_ms` advances without a key; `loop` wraps.
//! - AC4 `tiza next` works without quoting JSON.
//! - AC5 Each entry carries an explicit `step: N`; moving a step with `+`/`-`
//!   does not shift titles or `auto_advance_ms` (ADR 0007 §4). Entries may be
//!   sparse; saved sorted by `step`.
//! - AC6 `tiza next|prev|goto ID` talk to the running instance over the
//!   socket (session or `--socket`, like `tiza api`); with no instance they
//!   say so and exit non-zero.
//! - AC7 `goto` and `step.goto` land on the step exactly as `n` would after a
//!   full replay: recomputed from step 0 (moves, views, redacts).
//! - AC8 Auto-advance waits for the step to be ready: a typed move finishes
//!   typing and a `wait` elapses before the countdown starts; a key pressed
//!   meanwhile finishes the typing and is consumed.
//! - AC9 The title shows on the status line and in `status` /
//!   `list_annotations` over the socket and MCP; presentation mode keeps the
//!   bar's rules (no new text in a take).
//! - AC10 `scene.get`, the schema and `check-schema.py` cover `steps` and
//!   `loop`; docs: README, agent guide, CHANGELOG, ADR 0007 note.
//! - AC11 (Luis, 2026-10-18) The native GIF and MP4 use each step's
//!   `auto_advance_ms` as its duration, `--interval` for the rest; typed
//!   frames come first; the GIF summary line names the timed steps; `--help`
//!   and the README say so.
//!
//! Corner cases: duplicate ids and all-digit ids are rejected on load; the
//! auto-advance waits while the command line is open; `goto` to a typed step
//! lands with the text complete; a number means a step number; a redact
//! notice once per loop pass; an entry past the last step is kept and warned.
//!
//! Rework (2026-10-18, after QA 2): MCP `goto_step` takes a step number too
//! (`{"id": 4}`); ids starting with `-` are rejected on load and by
//! `scene.patch` (`tiza goto -1` would be read as an option); an unknown key
//! in a `steps` entry (the dropped `timer_min`) is kept and warned
//! (`status.warnings`, `render` stderr, `tiza check`); an open `v` selection
//! defers the auto-advance like the `:` line.

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_core::Document;
use tiza_core::fs;
use tiza_qa::{Live, McpClient, fixture, scratch, tiza};

/// The fixture (the ownership demo with moves, steps 0–5; 0–4 named),
/// opened live with a throwaway save path.
fn opened(name: &str) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    let doc = fs::open(fixture("t-099/steps.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("steps.tiza"), dir.join("live.sock"));
    (live, dir)
}

fn step(live: &Live) -> u32 {
    live.with(|a| a.doc.step)
}

/// Waits (10 s at most) until `cond` holds.
fn wait_until(what: &str, cond: impl Fn() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < Duration::from_secs(10), "waiting: {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The step 3 seen by walking there from step 0 with `n`: code and render.
fn walked(name: &str, to: u32) -> (String, String) {
    let (live, _) = opened(name);
    live.api_ok("step.set", json!({"step": 0}));
    for _ in 0..to {
        live.keys(&[KeyCode::Char('n')]);
        live.with(|a| a.finish_typing());
    }
    (
        live.api_ok("code.get", json!({}))["text"].to_string(),
        live.api_ok("render.text", json!({"width": 100}))["text"].to_string(),
    )
}

#[test]
fn ac1_ac7_goto_jumps_as_a_full_replay_and_an_unknown_id_is_not_found() {
    let (live, _) = opened("t099-ac1");
    // From step 5 back to "borrow" (3), and from 0 forward to it: both are
    // the step-3 document `n` reaches from step 0.
    live.api_ok("step.set", json!({"step": 5}));
    let r = live.api_ok("step.goto", json!({"id": "borrow"}));
    assert_eq!(r["step"], 3);
    assert_eq!(r["id"], "borrow");
    assert_eq!(r["title"], "Shared borrow");
    assert_eq!(r["label"], "step 3/5 · Shared borrow");
    let code = live.api_ok("code.get", json!({}))["text"].to_string();
    let text = live.api_ok("render.text", json!({"width": 100}))["text"].to_string();
    assert_eq!((code.clone(), text.clone()), walked("t099-ac1-walk", 3));
    live.api_ok("step.set", json!({"step": 0}));
    live.api_ok("step.goto", json!({"id": "borrow"}));
    assert_eq!(
        live.api_ok("code.get", json!({}))["text"].to_string(),
        code,
        "0 → goto"
    );
    // A number is a step number, as text or as a number.
    assert_eq!(live.api_ok("step.goto", json!({"id": "1"}))["step"], 1);
    assert_eq!(live.api_ok("step.goto", json!({"id": 4}))["step"], 4);
    // Unknown id: not_found, the named ones listed; the step stays.
    let r = live.api("step.goto", json!({"id": "nope"}));
    let e = r.error.expect("an error");
    assert_eq!(e.code, "not_found");
    assert!(
        e.message.contains("intro, move, grow, borrow, method"),
        "{}",
        e.message
    );
    assert_eq!(step(&live), 4);
    // `:goto` in the TUI does the same.
    live.command("goto move");
    assert_eq!(step(&live), 1);
    assert_eq!(live.message().unwrap().0, "step 1/5 · Ownership moves");
}

#[test]
fn corner_goto_to_a_typed_step_lands_with_the_text_complete() {
    let (live, _) = opened("t099-typed");
    live.api_ok("step.set", json!({"step": 1}));
    // Step 2 types eight lines; goto lands on it whole, no animation.
    live.api_ok("step.goto", json!({"id": "grow"}));
    assert!(
        !live.with(|a| a.is_typing()),
        "goto must not replay the typing"
    );
    let code = live.api_ok("code.get", json!({}))["text"].to_string();
    assert!(code.contains("fn len(s: &String)"), "{code}");
    assert_eq!(code, walked("t099-typed-walk", 2).0);
    // `n` from 1 to 2 still types (the presenter's own walk).
    live.api_ok("step.set", json!({"step": 1}));
    live.keys(&[KeyCode::Char('n')]);
    assert!(live.with(|a| a.is_typing()));
}

#[test]
fn ac2_ac9_the_title_is_on_the_bar_in_status_and_in_list_annotations() {
    let (live, _) = opened("t099-ac2");
    live.api_ok("step.goto", json!({"id": "borrow"}));
    let screen = live.screen(110, 30);
    let bar = screen.lines().last().unwrap();
    assert!(bar.contains("step 3/5 · Shared borrow"), "{bar}");
    // An id without a title shows the id; an unnamed step only its number.
    live.api_ok("step.set", json!({"step": 4}));
    assert!(live.screen(110, 30).contains("step 4/5 · method"));
    live.api_ok("step.set", json!({"step": 5}));
    let bar = live.screen(110, 30).lines().last().unwrap().to_string();
    assert!(
        bar.contains("step 5/5") && !bar.contains("step 5/5 ·"),
        "{bar}"
    );
    // Presentation mode: no bar, so no title on screen.
    live.api_ok("step.set", json!({"step": 3}));
    live.with(|a| a.set_present(true));
    assert!(!live.screen(110, 30).contains("Shared borrow"));
    live.with(|a| a.set_present(false));
    // status over the socket.
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["step_id"], "borrow");
    assert_eq!(st["step_title"], "Shared borrow");
    assert_eq!(st["step_label"], "step 3/5 · Shared borrow");
    // MCP: list_annotations starts with the step; goto_step jumps and
    // render_text shows that step.
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("list_annotations", json!({}));
    assert!(!err, "{text}");
    assert!(text.starts_with("# step 3/5 · Shared borrow\n"), "{text}");
    let (err, text, _) = mcp.call("goto_step", json!({"id": "move"}));
    assert!(
        !err && text.contains("\"label\": \"step 1/5 · Ownership moves\""),
        "{text}"
    );
    let (_, render, _) = mcp.call("render_text", json!({"width": 100}));
    assert!(render.contains("move: s moves into"), "{render}");
    assert!(
        !render.contains("a shared borrow"),
        "step 1 only:\n{render}"
    );
    let (err, text, _) = mcp.call("goto_step", json!({"id": "nope"}));
    assert!(err && text.contains("not_found"), "{text}");
    let (_, st, _) = mcp.call("status", json!({}));
    assert!(st.contains("\"step_title\": \"Ownership moves\""), "{st}");
}

#[test]
fn ac3_ac8_auto_advance_waits_for_the_typing_and_moves_on_alone() {
    let (live, _) = opened("t099-ac3");
    // Step 2 types 8 lines at 160 ms (1.28 s) and then waits 500 ms.
    live.api_ok(
        "scene.patch",
        json!({"steps": [
            {"step": 2, "id": "grow", "title": "The code grows", "auto_advance_ms": 500},
            {"step": 3, "id": "borrow", "title": "Shared borrow"}
        ]}),
    );
    let typing_ms = live.with(|a| a.doc.typing_ms_at(2));
    assert_eq!(typing_ms, 8 * 160);
    live.api_ok("step.set", json!({"step": 1}));
    let t0 = Instant::now();
    live.keys(&[KeyCode::Char('n')]);
    assert!(live.with(|a| a.is_typing()));
    wait_until("auto-advance to step 3", || step(&live) == 3);
    let took = t0.elapsed();
    assert!(
        took >= Duration::from_millis(typing_ms + 500 - 40),
        "advanced before the typing and the 500 ms: {took:?}"
    );
    // Step 3 has no auto_advance_ms: it appears alone and stays.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(step(&live), 3, "step 3 must stay");

    // A key during the typing finishes it and is consumed (still step 2);
    // the countdown starts from there.
    live.api_ok("step.set", json!({"step": 1}));
    live.keys(&[KeyCode::Char('n')]);
    assert!(live.with(|a| a.is_typing()));
    let t1 = Instant::now();
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(step(&live), 2, "the key only finished the typing");
    assert!(!live.with(|a| a.is_typing()));
    wait_until("auto-advance after the key", || step(&live) == 3);
    assert!(
        t1.elapsed() >= Duration::from_millis(460),
        "{:?}",
        t1.elapsed()
    );
}

#[test]
fn ac8_a_wait_move_elapses_before_the_countdown() {
    let live = Live::from_code("fn a() {}\nfn b() {}\n", "rs", "t099-wait");
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "one", "step": 1}),
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 2, "text": "two", "step": 2}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "wait", "ms": 400}),
    );
    live.api_ok(
        "scene.patch",
        json!({"steps": [{"step": 1, "id": "one", "auto_advance_ms": 300}]}),
    );
    live.api_ok("step.set", json!({"step": 0}));
    let t0 = Instant::now();
    live.keys(&[KeyCode::Char('n')]);
    wait_until("auto-advance past the wait", || step(&live) == 2);
    assert!(
        t0.elapsed() >= Duration::from_millis(660),
        "{:?}",
        t0.elapsed()
    );
}

#[test]
fn ac3_loop_wraps_to_step_zero() {
    let (live, _) = opened("t099-loop");
    live.api_ok("step.set", json!({"step": 5}));
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(step(&live), 5, "no loop: the last step stays");
    live.api_ok("scene.patch", json!({"loop": true}));
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(step(&live), 0, "loop: n past the last step is step 0");
    assert_eq!(
        live.api_ok("code.get", json!({}))["text"].to_string(),
        walked("t099-loop-walk", 0).0,
        "step 0 is the untouched code"
    );
    // step.next does the same; the status says so.
    live.api_ok("step.set", json!({"step": 5}));
    assert_eq!(live.api_ok("step.next", json!({}))["step"], 0);
    assert_eq!(live.api_ok("status", json!({}))["loop"], true);
    // An auto-advance on the last step wraps too, and step 0's own one
    // starts the next pass.
    live.api_ok(
        "scene.patch",
        json!({"steps": [{"step": 0, "id": "intro", "auto_advance_ms": 50},
                         {"step": 5, "id": "end", "auto_advance_ms": 50}]}),
    );
    live.api_ok("step.set", json!({"step": 5}));
    wait_until("wrap and step 0 → 1", || step(&live) == 1);
}

#[test]
fn corner_the_command_line_defers_the_auto_advance_until_esc() {
    let (live, _) = opened("t099-cmdline");
    live.api_ok(
        "scene.patch",
        json!({"steps": [{"step": 3, "id": "borrow", "auto_advance_ms": 100}]}),
    );
    live.api_ok("step.set", json!({"step": 3}));
    live.keys(&[KeyCode::Char(':')]);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(step(&live), 3, "deferred while the command line is open");
    live.keys(&[KeyCode::Esc]);
    wait_until("advance once the command line closes", || step(&live) == 4);
}

#[test]
fn ac5_titles_are_bound_by_step_number_sparse_and_saved_sorted() {
    let (live, dir) = opened("t099-ac5");
    // Move an annotation of step 3 to step 4 with `+`: the titles stay.
    live.api_ok("step.set", json!({"step": 3}));
    let id = live.with(|a| {
        a.doc
            .annotations
            .iter()
            .find(|x| x.step == 3)
            .unwrap()
            .id
            .clone()
    });
    live.with(move |a| {
        a.selected = Some(id);
        a.selected_action('+');
    });
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["step_title"], "Shared borrow");
    let steps: Vec<(u64, String)> = live.with(|a| {
        a.doc
            .steps
            .iter()
            .map(|n| (n.step as u64, n.id.clone().unwrap()))
            .collect()
    });
    assert_eq!(steps[3], (3, "borrow".into()));
    // Out of order in a patch: kept sparse, saved sorted.
    live.api_ok(
        "scene.patch",
        json!({"steps": [{"step": 4, "id": "d"}, {"step": 1, "id": "b"}]}),
    );
    live.command("w");
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("steps.tiza")).unwrap()).unwrap();
    assert_eq!(
        saved["steps"],
        json!([{"step": 1, "id": "b"}, {"step": 4, "id": "d"}])
    );
    // scene.get carries them; `u` undoes the patch.
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(scene["document"]["steps"][0]["id"], "b");
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(live.with(|a| a.doc.steps.len()), 5);
}

#[test]
fn ac4_ac6_tiza_next_prev_goto_drive_the_instance_or_say_there_is_none() {
    let (live, dir) = opened("t099-cli");
    let sock = live.socket.to_str().unwrap().to_string();
    live.api_ok("step.set", json!({"step": 0}));
    let run = |args: &[&str]| {
        let mut a = args.to_vec();
        a.extend(["--socket", &sock]);
        let out = tiza(&a);
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    };
    assert_eq!(run(&["next"]).1, "step 1/5 · Ownership moves");
    assert_eq!(run(&["goto", "borrow"]).1, "step 3/5 · Shared borrow");
    assert_eq!(step(&live), 3);
    assert_eq!(run(&["prev"]).1, "step 2/5 · The code grows");
    assert_eq!(run(&["goto", "5"]).1, "step 5/5");
    let (ok, _, err) = run(&["goto", "nope"]);
    assert!(!ok && err.contains("not_found"), "{err}");
    // No instance: says so, exit non-zero.
    let none = dir.join("nobody.sock");
    for cmd in [vec!["next"], vec!["prev"], vec!["goto", "drop"]] {
        let mut a = cmd.clone();
        a.extend(["--socket", none.to_str().unwrap()]);
        let out = tiza(&a);
        assert!(!out.status.success(), "{cmd:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("no Tiza instance is listening"), "{err}");
    }
}

fn load(steps: serde_json::Value) -> Result<Document, String> {
    let dir = scratch("t099-load");
    let p = dir.join("d.tiza");
    std::fs::write(
        &p,
        json!({"schema": 1, "source": {"language": "rs"}, "code": ["fn a() {}", "fn b() {}"],
               "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 1, "text": "x"},
                               {"id": "n2", "step": 2, "type": "note", "line": 2, "text": "y"}],
               "steps": steps})
        .to_string(),
    )
    .unwrap();
    fs::open(p.to_str().unwrap())
        .map(|o| o.document)
        .map_err(|e| e.to_string())
}

#[test]
fn corner_bad_ids_are_rejected_on_load_and_a_step_past_the_end_is_kept_and_warned() {
    let e = load(json!([{"step": 1, "id": "a"}, {"step": 2, "id": "a"}])).unwrap_err();
    assert!(e.contains("used twice"), "{e}");
    let e = load(json!([{"step": 1, "id": "3"}])).unwrap_err();
    assert!(e.contains("all digits"), "{e}");
    let e = load(json!([{"step": 1, "id": "x"}, {"step": 1, "title": "y"}])).unwrap_err();
    assert!(e.contains("two entries name step 1"), "{e}");
    // Past the last step: kept, not used, said in status and by render.
    let d = load(json!([{"step": 9, "id": "later"}])).unwrap();
    assert_eq!(d.steps.len(), 1);
    let dir = scratch("t099-past");
    let live = Live::open(d, dir.join("d.tiza"), dir.join("live.sock"));
    let st = live.api_ok("status", json!({}));
    assert!(
        st["warnings"]
            .to_string()
            .contains("names step 9, beyond the last step (2)"),
        "{st}"
    );
    assert_eq!(
        live.api("step.goto", json!({"id": "later"}))
            .error
            .unwrap()
            .code,
        "invalid_argument"
    );
    live.command("w");
    let saved = std::fs::read_to_string(dir.join("d.tiza")).unwrap();
    assert!(saved.contains("\"later\""), "never dropped: {saved}");
    let out = tiza(&["render", dir.join("d.tiza").to_str().unwrap(), "--out", "-"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("names step 9"));
}

// ---- Rework of 2026-10-18 (QA 2's minor findings) ----

#[test]
fn rework1_mcp_goto_step_takes_a_step_number_too() {
    let (live, _) = opened("t099-rw1");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("goto_step", json!({"id": 4}));
    assert!(
        !err && text.contains("\"label\": \"step 4/5 · method\""),
        "{text}"
    );
    assert_eq!(step(&live), 4);
    let (err, text, _) = mcp.call("goto_step", json!({"id": "2"}));
    assert!(!err && text.contains("\"step\": 2"), "{text}");
    assert_eq!(step(&live), 2);
    let (err, text, _) = mcp.call("goto_step", json!({"id": 9}));
    assert!(err && text.contains("beyond the last step"), "{text}");
    assert_eq!(step(&live), 2, "a bad number moves nothing");
    // The tool's input schema says it takes a number.
    let tools = mcp.tools();
    let goto = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "goto_step")
        .expect("goto_step listed");
    // Flat, like every other tool: no `$ref`/`$defs`/`anyOf` for clients
    // that do not resolve references.
    let id = &goto["inputSchema"]["properties"]["id"];
    assert_eq!(id["type"], json!(["integer", "string"]), "{id}");
    let schema = goto["inputSchema"].to_string();
    for k in ["$ref", "$defs", "anyOf"] {
        assert!(!schema.contains(k), "{k} in {schema}");
    }
}

#[test]
fn rework1_a_value_that_names_no_step_gets_a_plain_error_on_the_socket_and_mcp() {
    let (live, _) = opened("t099-rw1-bad");
    live.api_ok("step.set", json!({"step": 2}));
    let mut mcp = McpClient::start(&live.socket);
    for bad in [json!(-1), json!(4.5), json!(null)] {
        let r = live.api("step.goto", json!({"id": bad}));
        let e = r.error.unwrap_or_else(|| panic!("{bad} accepted"));
        assert_eq!(e.code, "invalid_argument", "{bad}");
        assert!(
            e.message
                .contains("`id` must be a step id or a step number from 0 to 5"),
            "{bad}: {}",
            e.message
        );
        assert!(!e.message.contains("untagged"), "{}", e.message);
        let (err, text, _) = mcp.call("goto_step", json!({"id": bad}));
        assert!(err, "{bad}: {text}");
        assert!(
            text.contains("`id` must be a step id or a step number from 0 to 5"),
            "{bad}: {text}"
        );
        assert!(
            !text.contains("untagged") && !text.contains("deserialize"),
            "{text}"
        );
        assert_eq!(step(&live), 2, "{bad} moved nothing");
    }
}

#[test]
fn rework2_ids_starting_with_a_dash_are_rejected_on_load_and_by_scene_patch() {
    for id in ["-1", "-x", "--socket"] {
        let e = load(json!([{"step": 1, "id": id}])).unwrap_err();
        assert!(e.contains("starts with `-`"), "{id}: {e}");
        assert!(e.contains("as an option"), "{id}: {e}");
    }
    // A dash inside an id is fine.
    let d = load(json!([{"step": 1, "id": "shared-borrow"}])).unwrap();
    assert_eq!(d.resolve_step("shared-borrow").unwrap(), 1);
    let (live, _) = opened("t099-rw2");
    let before = live.api_ok("scene.get", json!({}))["document"]["steps"].clone();
    let r = live.api("scene.patch", json!({"steps": [{"step": 1, "id": "-1"}]}));
    let e = r.error.expect("refused");
    assert_eq!(e.code, "invalid_argument");
    assert!(e.message.contains("starts with `-`"), "{}", e.message);
    assert_eq!(
        live.api_ok("scene.get", json!({}))["document"]["steps"],
        before,
        "nothing changed"
    );
    // `tiza check` reports it as an error.
    let dir = scratch("t099-rw2-check");
    let p = dir.join("d.tiza");
    std::fs::write(
        &p,
        json!({"schema": 1, "source": {"language": "rs"}, "code": ["a", "b"],
               "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 1, "text": "x"}],
               "steps": [{"step": 1, "id": "-1"}]})
        .to_string(),
    )
    .unwrap();
    let out = tiza(&["check", p.to_str().unwrap()]);
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "{all}");
    assert!(all.contains("starts with `-`"), "{all}");
}

#[test]
fn rework3_an_unknown_key_in_steps_is_warned_kept_and_breaks_nothing() {
    let d = load(json!([
        {"step": 1, "id": "one", "title": "One", "timer_min": 5},
        {"step": 2, "id": "two", "auto_advance_ms": 1000, "notes": "x", "colour": "red"}
    ]))
    .unwrap();
    // Everything known still works.
    assert_eq!(d.step_label(1), "step 1/2 · One");
    assert_eq!(d.auto_advance_ms(2), Some(1000));
    assert_eq!(d.resolve_step("two").unwrap(), 2);
    let w = d.step_warnings();
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(
        w[0].contains("«one» (step 1) has unknown key `timer_min`"),
        "{w:?}"
    );
    assert!(w[1].contains("unknown keys `colour`, `notes`"), "{w:?}");
    // status.warnings, and the keys are kept on save.
    let dir = scratch("t099-rw3");
    let live = Live::open(d, dir.join("d.tiza"), dir.join("live.sock"));
    let st = live.api_ok("status", json!({}));
    assert!(
        st["warnings"]
            .to_string()
            .contains("unknown key `timer_min`"),
        "{st}"
    );
    live.command("w");
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("d.tiza")).unwrap()).unwrap();
    assert_eq!(saved["steps"][0]["timer_min"], 5, "kept: {saved}");
    assert!(saved["steps"][0].get("unknown").is_none(), "{saved}");
    // render says it on stderr, `tiza check` lists it as a warning.
    let f = dir.join("d.tiza");
    let out = tiza(&["render", f.to_str().unwrap(), "--out", "-"]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown key `timer_min`"), "{err}");
    let out = tiza(&["check", f.to_str().unwrap()]);
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        all.contains("warning: steps: entry «one» (step 1) has unknown key `timer_min`"),
        "a warning, not an error: {all}"
    );
    assert!(all.contains("0 errors, 2 warnings"), "{all}");
    assert_eq!(out.status.code(), Some(0), "warnings only: exit 0\n{all}");
    let out = tiza(&["check", "--strict", f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "--strict fails on warnings");
    let out = tiza(&["check", "--json", f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["errors"], 0, "{report}");
    let finding = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["message"].as_str().unwrap().contains("`timer_min`"))
        .unwrap_or_else(|| panic!("no timer_min finding: {report}"));
    assert_eq!(finding["level"], "warning", "{finding}");
    assert_eq!(finding["code"], "steps", "{finding}");
    // A valid document has no such warning, and `null` known keys are fine.
    let ok = load(json!([{"step": 1, "id": "a", "title": null, "auto_advance_ms": null}])).unwrap();
    assert!(ok.step_warnings().is_empty(), "{:?}", ok.step_warnings());
    let (live, _) = opened("t099-rw3-ok");
    let st = live.api_ok("status", json!({}));
    assert!(!st["warnings"].to_string().contains("steps:"), "{st}");
    let out = tiza(&["check", fixture("t-099/steps.tiza").to_str().unwrap()]);
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!all.contains("steps:"), "{all}");
    // scene.patch with an unknown key: accepted, warned.
    live.api_ok(
        "scene.patch",
        json!({"steps": [{"step": 3, "id": "borrow", "timer_min": 2}]}),
    );
    let st = live.api_ok("status", json!({}));
    assert!(
        st["warnings"]
            .to_string()
            .contains("unknown key `timer_min`"),
        "{st}"
    );
}

#[test]
fn rework_optional_a_v_selection_defers_the_auto_advance() {
    let (live, _) = opened("t099-visual");
    live.api_ok(
        "scene.patch",
        json!({"steps": [{"step": 3, "id": "borrow", "auto_advance_ms": 100}]}),
    );
    live.api_ok("step.set", json!({"step": 3}));
    live.keys(&[KeyCode::Char('v'), KeyCode::Char('l')]);
    assert!(live.with(|a| a.selection().is_some()));
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(step(&live), 3, "deferred while the selection is open");
    assert!(live.with(|a| a.selection().is_some()), "selection kept");
    live.keys(&[KeyCode::Esc]);
    wait_until("advance once the selection closes", || step(&live) == 4);
}

#[test]
fn corner_a_redact_notice_once_per_loop_pass() {
    let dir = scratch("t099-redact");
    let src = dir.join("config.json");
    std::fs::write(&src, "{\"token\": \"abc123secret\", \"user\": \"ana\"}\n").unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("config.json.tiza"), dir.join("live.sock"));
    live.command("redact 1@11..23");
    // The key renamed: the redact cannot follow its text → widened, loud.
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "position": "1",
               "content": "{\"apiToken\": \"abc123secret\", \"user\": \"ana\"}"}),
    );
    live.api_ok("scene.patch", json!({"loop": true}));
    live.api_ok("step.set", json!({"step": 0}));
    for pass in 0..3 {
        live.keys(&[KeyCode::Char('n')]);
        let alerts = live.with(|a| a.redact_alerts.clone());
        assert_eq!(alerts.len(), 1, "pass {pass}: {alerts:?}");
        assert!(alerts[0].contains("could not follow its text"));
        // Ticks at that step say nothing more.
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(live.with(|a| a.redact_alerts.len()), 1);
        live.keys(&[KeyCode::Char('n')]);
        assert_eq!(step(&live), 0, "pass {pass}: wrapped");
        assert!(live.with(|a| a.redact_alerts.is_empty()));
    }
}

#[test]
fn ac11_the_gif_holds_a_step_for_its_auto_advance_ms() {
    let dir = scratch("t099-gif");
    // The fixture without typing (steps 0..=5): one timed step, 3 s.
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("t-099/steps.tiza")).unwrap())
            .unwrap();
    v.as_object_mut().unwrap().remove("moves");
    v["annotations"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a["step"].as_u64().unwrap() <= 3);
    v["steps"] = json!([{"step": 2, "id": "two", "auto_advance_ms": 3000}]);
    let doc = dir.join("timed.tiza");
    std::fs::write(&doc, v.to_string()).unwrap();
    let out = dir.join("a.gif");
    let r = tiza(&[
        "render",
        doc.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--scale",
        "1",
        "--interval",
        "1",
    ]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let summary = String::from_utf8_lossy(&r.stderr);
    assert!(
        summary.contains("step 2 timed by its auto_advance_ms"),
        "{summary}"
    );
    let bytes = std::fs::read(&out).unwrap();
    let mut o = gif::DecodeOptions::new();
    o.set_color_output(gif::ColorOutput::Indexed);
    let mut d = o.read_info(std::io::Cursor::new(bytes)).unwrap();
    let mut delays = Vec::new();
    while let Some(f) = d.read_next_frame().unwrap() {
        delays.push(f.delay);
    }
    // Steps 0..=3: 1 s, 1 s, 3 s (its own), 2 s (the last: 2 × interval).
    assert_eq!(delays, vec![100, 100, 300, 200]);
    // --help says so.
    let help = String::from_utf8_lossy(&tiza(&["render", "--help"]).stdout).to_string();
    assert!(help.contains("auto_advance_ms"), "{help}");
}

#[test]
fn ac11_typed_frames_come_before_the_timed_hold_and_the_tape_sends_no_extra_n() {
    let mut doc = fs::open(fixture("t-099/steps.tiza").to_str().unwrap())
        .unwrap()
        .document;
    doc.steps[2].auto_advance_ms = Some(700);
    let theme = tiza_core::Theme::dark();
    let frames = tiza_core::anim::frames(&doc, &theme, &tiza_core::anim::AnimOptions::default());
    let at2: Vec<(bool, u64)> = frames
        .iter()
        .filter(|f| f.step == 2)
        .map(|f| (f.typing, f.delay_ms))
        .collect();
    assert!(at2.len() > 1 && at2[..at2.len() - 1].iter().all(|f| f.0));
    assert_eq!(*at2.last().unwrap(), (false, 700));
    // The .tape drives the real TUI, which advances from step 2 by itself:
    // no `n` for step 3, and step 2 held for its typing + 700 ms.
    let tape = tiza_core::cast::to_vhs_tape(
        &doc,
        &theme,
        "d.tiza",
        "d.gif",
        &tiza_core::cast::CastOptions::default(),
    );
    let lines: Vec<&str> = tape.lines().collect();
    let i = lines
        .iter()
        .position(|l| l.starts_with("# step 2"))
        .unwrap();
    assert_eq!(lines[i + 1], "Type \"n\"");
    assert_eq!(lines[i + 2], "Sleep 1980ms", "{tape}");
    assert_eq!(lines[i + 3], "# step 3");
    assert_eq!(lines[i + 4], "# (auto-advance from step 2)");
    assert!(lines[i + 5].starts_with("Sleep"), "{tape}");
    assert_eq!(tape.matches("Type \"n\"").count(), 4, "{tape}");
}

#[test]
fn ac10_schema_scene_get_and_docs_cover_steps_and_loop() {
    let schema = tiza_core::schema::document_schema().to_string();
    for k in ["\"steps\"", "\"loop\"", "\"auto_advance_ms\"", "NamedStep"] {
        assert!(schema.contains(k), "{k} missing from the document schema");
    }
    let api = serde_json::to_string(&tiza_core::schema::api_schema()).unwrap();
    assert!(api.contains("\"step.goto\""));
    // The fixture validates against the committed schema (check-schema.py
    // checks every qa/fixtures/*/*.tiza the same way).
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let committed: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("schema/tiza.schema.json")).unwrap(),
    )
    .unwrap();
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("t-099/steps.tiza")).unwrap())
            .unwrap();
    let v = jsonschema::validator_for(&committed).unwrap();
    assert!(v.is_valid(&doc));
    for (file, needle) in [
        ("README.md", "auto_advance_ms"),
        ("README.md", "tiza goto"),
        ("docs/agent-guide.md", "step.goto"),
        ("docs/agent-guide.md", "goto_step"),
        ("CHANGELOG.md", "t-099"),
        ("docs/adr/0007-moves-y-pasos-con-nombre.md", "t-099"),
    ] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        assert!(text.contains(needle), "{file} does not mention {needle}");
    }
}
