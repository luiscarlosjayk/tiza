//! Presentation with moves went over the frame budget: every frame with a
//! stable layout (presentation mode, `:layout stable`, each frame of a GIF
//! and of `--all-steps`) computed the take's geometry again, a clone and a
//! projection of the document per step where something changes (18 ms of
//! a 20 ms frame on a 2,000-line document with 10 moves); `status` laid the
//! document out to say whether the code is clipped; every socket request,
//! `status` included, redrew the screen; and highlighting used up to 8
//! threads, each keeping its own regex cache for the whole session.
//!
//! Acceptance criteria:
//! - AC1 The instance keeps the take's geometry by revision and every
//!   stable layout of it reuses it; any change of the document computes it
//!   again. `layout_with` borrows it, never clones it.
//! - AC2 `--all-steps`, the GIF and the cast compute it once before their
//!   loop over the steps.
//! - AC3 The output is the same, byte for byte, as before: the layout with
//!   the kept geometry equals the one that computes it, at every step, with
//!   and without moves, in the TUI and the exports.
//! - AC4 `status` builds its warnings from what the last frame laid out,
//!   without a layout; with no frame yet, or a change since, it lays out
//!   once and keeps that.
//! - AC5 Highlighting uses at most 4 threads.
//! - AC6 A «presentation + moves» scenario in the bench (`scripts/bench.sh`)
//!   and its line in `qa/bench/baseline.json`.
//! - AC7 This file; and a read-only request (`status`, `render.text`,
//!   `code.get`…) does not redraw the screen.
//!
//! Corner cases: the geometry does not depend on the width (one geometry,
//! every width); undo, redo, `scene.put`, a follow reload and a paste all
//! change the revision; a document without moves; a typing animation shows
//! other code at its step at every tick, and the geometry follows it (each
//! tick is a revision, as it was every frame); `status` before the first
//! frame and right after a change not drawn yet.

use std::time::{Duration, Instant};

use serde_json::json;
use tiza_core::layout::{LayoutOptions, layout};
use tiza_core::model::Document;
use tiza_qa::{Live, example, scratch};
use tiza_tui::app::App;

/// The card's document: 2,000 lines, 200 annotations over 20 steps, and
/// 10 moves (an insert every second step): 21 steps where something changes.
fn big_with_moves() -> Document {
    // One long line: a narrow terminal clips the code.
    let mut code = format!("// {}\n", "wide ".repeat(30));
    for i in 0..400 {
        code.push_str(&format!(
            "fn f{i}(s: String) -> usize {{\n    let n = s.len(); // {i}\n    println!(\"{{n}}\");\n    takes(s);\n}}\n"
        ));
    }
    let mut doc = Document::from_code(&code, "rs", None);
    let mut annotations = Vec::new();
    for i in 0..200u32 {
        let line = 1 + (i * 10) % 1990;
        let step = 1 + i % 20;
        annotations.push(match i % 4 {
            0 => json!({"id": format!("n{i}"), "step": step, "type": "note", "line": line, "text": format!("note {i}")}),
            1 => json!({"id": format!("b{i}"), "step": step, "type": "box", "from_line": line, "to_line": line + 3, "label": "box"}),
            2 => json!({"id": format!("a{i}"), "step": step, "type": "arrow", "from": {"line": line, "col": 4}, "to": {"line": line + 3, "col": 4}, "label": "mv"}),
            _ => json!({"id": format!("h{i}"), "step": step, "type": "highlight", "span": {"line": line, "start": 0, "end": 5}}),
        });
    }
    let moves: Vec<_> = (1..=10u32)
        .map(|k| {
            json!({
                "at_step": 2 * k, "action": "code.patch", "op": "insert",
                "after": format!("fn f{}(s: String) -> usize {{", 30 * k),
                "content": format!("    // inserted at step {}", 2 * k)
            })
        })
        .collect();
    let mut v = serde_json::to_value(&doc).unwrap();
    v["annotations"] = json!(annotations);
    v["moves"] = json!(moves);
    v["step"] = json!(10);
    doc = tiza_core::fs::parse_document(&v.to_string(), None).unwrap();
    doc
}

fn app_of(doc: Document, name: &str) -> App {
    let mut app = App::new(doc, scratch(name).join("doc.tiza"));
    app.autosave = false;
    app
}

/// A layout of the instance's document that computes the geometry itself.
fn fresh(app: &App, width: usize) -> String {
    let opts = LayoutOptions {
        width,
        step: app.doc.step,
        focus: app.focus,
        pointer: None,
        line_numbers: app.doc.numbers,
        tab_width: 4,
        typing: app.typing.is_some(),
        stable: app.present || app.layout_stable,
        margin_cap: app.doc.margin.map(usize::from),
    };
    layout(&app.doc, &app.theme, &opts).canvas.to_ansi()
}

fn kept(app: &App, width: usize) -> String {
    app.layout(width).canvas.to_ansi()
}

/// Every step and a few widths: the kept geometry draws what a fresh one
/// draws.
fn every_step_the_same(app: &mut App, what: &str) {
    let last = app.doc.max_step();
    for step in 0..=last {
        app.set_step(step);
        app.finish_typing();
        for width in [60, 100, 140] {
            assert_eq!(
                kept(app, width),
                fresh(app, width),
                "{what}: step {step} at {width}"
            );
        }
    }
}

// ---------- AC1, AC3 ----------

#[test]
fn ac1_ac3_presentation_reuses_one_geometry_and_draws_the_same() {
    let mut app = app_of(big_with_moves(), "t191-big");
    app.present = true;
    let before = app.geometries.get();
    every_step_the_same(&mut app, "presentation, 2,000 lines with moves");
    // One geometry for the whole take: steps and widths do not change it.
    assert_eq!(app.geometries.get() - before, 1);
}

#[test]
fn ac3_the_examples_draw_the_same_with_the_kept_geometry() {
    for (name, file) in [
        ("moves", "ownership-moves.rs.tiza"),
        ("links", "stack-and-queue.tiza"),
        ("tour", "tally-tour.tiza"),
        ("plain", "ownership.rs.tiza"),
    ] {
        let doc = tiza_core::fs::load_document(&example(file)).unwrap();
        for stable in [true, false] {
            let mut app = app_of(doc.clone(), &format!("t191-{name}"));
            app.layout_stable = stable;
            every_step_the_same(&mut app, &format!("{file}, stable {stable}"));
        }
    }
}

#[test]
fn ac1_every_change_computes_the_geometry_again() {
    let dir = scratch("t191-changes");
    let doc = tiza_core::fs::load_document(&example("ownership-moves.rs.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.with(|app| {
        app.present = true;
        app.autosave = false;
    });
    let same = |what: &str| {
        for width in [80, 120] {
            let (k, f) = live.with(move |app| {
                app.finish_typing();
                (kept(app, width), fresh(app, width))
            });
            assert_eq!(k, f, "{what} at {width}");
        }
    };
    same("opened");
    // A move added at step 1: the geometry is the new take's.
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1",
               "content": "// a much longer first line than any the example had before, to widen the code column"}),
    );
    same("move added");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    same("u");
    live.keys(&[crossterm::event::KeyCode::Char('U')]);
    same("U");
    let scene = live.api_ok("scene.get", json!({}));
    let mut put = scene["document"].clone();
    put["annotations"].as_array_mut().unwrap().push(json!({
        "id": "zz", "step": 2, "type": "arrow",
        "from": {"line": 2, "col": 4}, "to": {"line": 9, "col": 4}
    }));
    live.api_ok("scene.put", json!({"document": put}));
    same("scene.put");
    live.command("numbers off");
    same(":numbers off");
    live.command("step 0");
    live.with(|app| app.replace_code("fn main() {\n    let s = String::from(\"hi\");\n}\n"))
        .unwrap();
    same("paste");
}

#[test]
fn corner_a_follow_reload_computes_the_geometry_again() {
    let dir = scratch("t191-follow");
    let src = dir.join("main.rs");
    std::fs::write(&src, "fn main() {\n    let a = 1;\n}\n").unwrap();
    let opened = tiza_core::fs::open(src.to_str().unwrap()).unwrap();
    let live = Live::open(
        opened.document,
        opened.save_path.clone(),
        dir.join("live.sock"),
    );
    live.with(|app| app.present = true);
    live.command("note 2 the binding");
    live.command("follow on");
    let geometries = live.with(|app| {
        let _ = app.layout(100);
        app.geometries.get()
    });
    let tmp = dir.join("main.rs.tmp");
    std::fs::write(
        &tmp,
        "fn main() {\n    let a = 1;\n    let a_much_longer_binding_name_than_before = 2;\n}\n",
    )
    .unwrap();
    std::fs::rename(&tmp, &src).unwrap();
    let start = Instant::now();
    while live.with(|app| app.doc.base().len()) != 4 {
        assert!(start.elapsed() < Duration::from_secs(10), "no reload");
        std::thread::sleep(Duration::from_millis(20));
    }
    let (k, f, now) = live.with(|app| (kept(app, 100), fresh(app, 100), app.geometries.get()));
    assert_eq!(k, f);
    assert_eq!(now, geometries + 1);
}

#[test]
fn corner_a_typing_tick_draws_the_code_typed_so_far() {
    // The take's geometry at the typed step comes from the code shown
    // there: each tick (a revision) sizes it again, as every frame did.
    let mut doc = tiza_core::fs::load_document(&example("ownership-moves.rs.tiza")).unwrap();
    doc.step = 0;
    let mut app = app_of(doc, "t191-typing");
    app.present = true;
    let typed = (1..=app.doc.max_step())
        .find(|s| tiza_core::moves::typing_plan(&app.doc, *s).is_some())
        .expect("the example types code");
    // Typing runs when the step is reached by going forward one.
    app.set_step(typed - 1);
    app.finish_typing();
    app.next_step();
    assert_eq!(app.doc.step, typed);
    assert!(app.is_typing());
    let start = Instant::now();
    let mut ticks = 0;
    while app.is_typing() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "typing never ended"
        );
        if app.typing_tick() {
            ticks += 1;
            assert_eq!(kept(&app, 100), fresh(&app, 100), "tick {ticks}");
        }
    }
    assert!(ticks > 1, "{ticks}");
    assert_eq!(kept(&app, 100), fresh(&app, 100), "typed");
}

// ---------- AC4 ----------

#[test]
fn ac4_status_reads_the_last_frame_and_lays_nothing_out() {
    let dir = scratch("t191-status");
    let live = Live::open(
        big_with_moves(),
        dir.join("doc.tiza"),
        dir.join("live.sock"),
    );
    live.with(|app| app.autosave = false);
    // A narrow terminal: the code is clipped, the warnings say so.
    let _ = live.screen(60, 30);
    let warnings = live.with(|app| app.viewport_warnings());
    assert!(
        warnings.iter().any(|w| w.starts_with("code needs")),
        "{warnings:?}"
    );
    let layouts = live.with(|app| app.layouts.get());
    for _ in 0..5 {
        let st = live.api_ok("status", json!({}));
        assert_eq!(st["warnings"], json!(warnings), "{st}");
    }
    assert_eq!(
        live.with(|app| app.layouts.get()),
        layouts,
        "status laid out"
    );
}

#[test]
fn ac4_a_change_not_drawn_yet_is_laid_out_once() {
    let dir = scratch("t191-status-change");
    let live = Live::open(
        big_with_moves(),
        dir.join("doc.tiza"),
        dir.join("live.sock"),
    );
    live.with(|app| app.autosave = false);
    let _ = live.screen(200, 40);
    let wide = live.api_ok("status", json!({}))["warnings"].clone();
    assert!(!wide.to_string().contains("code needs"), "{wide}");
    // Narrower and not drawn yet: the warnings are this size's.
    live.with(|app| app.size = (60, 30));
    let layouts = live.with(|app| app.layouts.get());
    let st = live.api_ok("status", json!({}));
    assert!(st["warnings"].to_string().contains("code needs"), "{st}");
    assert_eq!(live.with(|app| app.layouts.get()), layouts + 1);
    let again = live.api_ok("status", json!({}));
    assert_eq!(again["warnings"], st["warnings"]);
    assert_eq!(live.with(|app| app.layouts.get()), layouts + 1);
    // A change of the document: laid out again, once.
    live.command("numbers off");
    let _ = live.api_ok("status", json!({}));
    let _ = live.api_ok("status", json!({}));
    assert_eq!(live.with(|app| app.layouts.get()), layouts + 2);
}

#[test]
fn ac4_status_before_the_first_frame() {
    let dir = scratch("t191-status-first");
    let live = Live::open(
        big_with_moves(),
        dir.join("doc.tiza"),
        dir.join("live.sock"),
    );
    live.with(|app| app.size = (60, 30));
    let st = live.api_ok("status", json!({}));
    // The oracle lays out without the instance's caches.
    let expected = live.with(|app| {
        let (_, pad) = app.effective_padding();
        let width = 60 - 2 * usize::from(pad);
        let opts = LayoutOptions {
            width,
            step: app.doc.step,
            focus: app.focus,
            pointer: None,
            line_numbers: app.doc.numbers,
            tab_width: 4,
            typing: app.typing.is_some(),
            stable: app.present || app.layout_stable,
            margin_cap: app.doc.margin.map(usize::from),
        };
        app.viewport_warnings_for(&layout(&app.doc, &app.theme, &opts))
    });
    assert_eq!(st["warnings"], json!(expected));
}

// ---------- AC5, AC7 ----------

#[test]
fn ac5_highlighting_uses_at_most_four_threads() {
    assert_eq!(tiza_core::highlight::MAX_THREADS, 4);
    assert!(tiza_core::highlight::default_threads(1_000_000) <= 4);
}

#[test]
fn ac7_a_read_only_request_does_not_redraw() {
    for m in tiza_tui::app::READ_ONLY_METHODS {
        assert!(!tiza_tui::request_redraws(m, false), "{m}");
        // Unless it changed something anyway (a message, a revision).
        assert!(tiza_tui::request_redraws(m, true), "{m}");
    }
    for m in [
        "annotation.add",
        "step.set",
        "scene.put",
        "follow.set",
        "theme.set",
    ] {
        assert!(tiza_tui::request_redraws(m, false), "{m}");
    }
}

// ---------- AC6 ----------

#[test]
fn ac6_presentation_with_moves_reuses_the_geometry_every_frame() {
    // What made a presentation frame slow is counted, not timed: every
    // frame lays the document out, none computes the take's geometry again
    // (that was ~200 ms of a debug frame, ~38 ms without it). A time budget
    // here failed on a loaded machine; the time is the bench's to judge.
    // `scripts/bench.sh` («presentation + moves») reads the median printed
    // below and compares it with `qa/bench/baseline.json`.
    let dir = scratch("t191-bench");
    let live = Live::open(
        big_with_moves(),
        dir.join("doc.tiza"),
        dir.join("live.sock"),
    );
    live.with(|app| {
        app.present = true;
        app.autosave = false;
    });
    let _ = live.screen(120, 40);
    let counts = || live.with(|app| (app.layouts.get(), app.geometries.get()));
    let (layouts, geometries) = counts();
    assert_eq!(
        geometries, 1,
        "the first frame computes the take's geometry"
    );
    let mut samples: Vec<Duration> = (0..21)
        .map(|_| {
            let t = Instant::now();
            let _ = live.screen(120, 40);
            t.elapsed()
        })
        .collect();
    let (layouts_after, geometries_after) = counts();
    samples.sort();
    eprintln!(
        "median presentation frame: {:?}",
        samples[samples.len() / 2]
    );
    assert!(
        layouts_after >= layouts + 21,
        "every frame lays out: {layouts} → {layouts_after}"
    );
    assert_eq!(
        geometries_after, geometries,
        "a frame computed the take's geometry again"
    );
}

// ---------- review: a typing ended by a request, a step patched ----------

/// Four lines, a note at step 1, and a move at step 2 that types a line
/// wider than every other, one line every 5 s: the take's geometry at step
/// 2 is sized from what is typed so far.
fn typed_wide_line() -> Document {
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1},
        "code": ["fn main() {", "    let a = 1;", "    a", "}"],
        "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 2, "text": "the binding a"}],
        "moves": [{
            "at_step": 2, "action": "code.patch", "op": "insert", "after": "let a = 1;",
            "content": format!("    // {}", "x".repeat(70)), "typing": "line", "speed_ms": 5000
        }],
        "step": 1
    });
    tiza_core::fs::parse_document(&doc.to_string(), None).unwrap()
}

/// Presenting `typed_wide_line`, `n` starts the typing and a frame is
/// drawn halfway; then `end` ends it some other way than a key.
fn typed_then(name: &str, end: impl FnOnce(&Live)) {
    let dir = scratch(name);
    let live = Live::open(
        typed_wide_line(),
        dir.join("doc.tiza"),
        dir.join("live.sock"),
    );
    live.with(|app| {
        app.present = true;
        app.autosave = false;
    });
    live.keys(&[crossterm::event::KeyCode::Char('n')]);
    assert!(live.with(|app| app.is_typing()), "typing");
    let _ = live.screen(140, 20);
    end(&live);
    let (step, kept, fresh) = live.with(|app| {
        app.finish_typing();
        (app.doc.step, kept(app, 138), fresh(app, 138))
    });
    assert_eq!(kept, fresh, "step {step}");
}

/// The instance's layout at `width` and a fresh one, from the app thread.
fn both(live: &Live, width: usize) -> (String, String) {
    live.with(move |app| (kept(app, width), fresh(app, width)))
}

#[test]
fn rev_a_typing_ended_by_step_prev_over_the_socket() {
    typed_then("t191-typed-prev", |live| {
        live.api_ok("step.prev", json!({}));
    });
}

#[test]
fn rev_a_typing_ended_by_step_next_at_the_last_step_then_p() {
    typed_then("t191-typed-next", |live| {
        live.api_ok("step.next", json!({}));
        live.keys(&[crossterm::event::KeyCode::Char('p')]);
    });
}

#[test]
fn rev_a_typing_ended_by_a_refused_move_delete_then_p() {
    typed_then("t191-typed-refused", |live| {
        assert!(
            live.api("move.delete", json!({"index": 99}))
                .error
                .is_some()
        );
        live.keys(&[crossterm::event::KeyCode::Char('p')]);
    });
}

#[test]
fn rev_a_typing_ended_by_a_step_set_in_the_app() {
    typed_then("t191-typed-set", |live| {
        live.with(|app| app.set_step(1));
    });
}

#[test]
fn rev_scene_patch_to_a_step_shows_that_steps_code() {
    // A line wider than the rest exists only at step 3.
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1},
        "code": ["fn main() {", "    let a = 1;", "    a", "}"],
        "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 2, "text": "the binding a"}],
        "moves": [
            {"at_step": 3, "action": "code.patch", "op": "insert", "after": "let a = 1;",
             "content": format!("    // {}", "y".repeat(70))},
            {"at_step": 4, "action": "code.patch", "op": "delete", "find": "// yyy"}
        ],
        "step": 1
    });
    let dir = scratch("t191-patch-step");
    let live = Live::open(
        tiza_core::fs::parse_document(&doc.to_string(), None).unwrap(),
        dir.join("doc.tiza"),
        dir.join("live.sock"),
    );
    live.with(|app| {
        app.present = true;
        app.autosave = false;
    });
    live.api_ok("scene.patch", json!({"step": 3}));
    let code = live.api_ok("code.get", json!({}));
    assert!(code.to_string().contains("yyyy"), "{code}");
    let (k, f) = both(&live, 138);
    assert_eq!(k, f, "step 3");
    live.api_ok("step.set", json!({"step": 2}));
    let (k, f) = both(&live, 138);
    assert_eq!(k, f, "step 2");
}
