//! Follow mode: reload the code when the source file is saved.
//!
//! Acceptance criteria:
//! - AC1 `--follow` / `:follow on` watches the *parent directory* of the
//!   referenced file (Zed saves with tmp + rename) and reloads the code after
//!   a debounce ≥ 100 ms; a burst of saves produces one reload.
//! - AC2 Annotations are re-anchored by text like a paste: moved lines keep
//!   their annotations, annotations whose text is gone are dropped with a
//!   message, and one `u` undoes the whole reload. Cursor, step, theme and
//!   scroll survive.
//! - AC3 The status line shows `⟳ following` and the time of the last reload;
//!   `:follow` reports the state; `:follow off` stops watching.
//! - AC4 Socket `follow.set {enabled}` / `follow.status`, MCP `set_follow`.
//! - AC5 Corner cases: embedded code → "nothing to follow"; file removed →
//!   "waiting" and the next write reloads; truncate-then-write is deferred;
//!   identical content → no reload; non-UTF-8 → message, previous content kept;
//!   the sidecar `.tiza` autosave in the same folder never triggers a reload;
//!   ranges `FILE:A-B` reload only their lines.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_core::Document;
use tiza_core::model::Shape;
use tiza_qa::{Live, McpClient, fixture, scratch};

const CODE: &str = "fn main() {\n    let s = String::new();\n    takes(s);\n    println!(\"done\");\n}\n\nfn takes(s: String) {}\n";

/// A source file next to its sidecar document, opened by reference.
fn followed(name: &str, code: &str) -> (Live, PathBuf) {
    let dir = scratch(name);
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, code).unwrap();
    let mut doc = Document::from_code(code, "rs", Some(src.display().to_string()));
    doc.reference_file(src.display().to_string(), None);
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    (live, src)
}

/// Saves like Zed does: write a temp file, then rename over the target.
fn save_like_zed(path: &Path, content: &str) {
    let tmp = path.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn reloads(live: &Live) -> u64 {
    live.api_ok("follow.status", json!({}))["reloads"]
        .as_u64()
        .unwrap_or(0)
}

/// How long a positive wait may take: generous, because macOS FSEvents and
/// loaded CI runners can deliver an event well after the write.
const WAIT: Duration = Duration::from_secs(10);

/// Quiet time after which "nothing happened" is believed (several times the
/// debounce plus the worst event latency seen on CI).
fn settle() {
    std::thread::sleep(Duration::from_millis(700));
}

/// Waits (up to `WAIT`) until the reload counter reaches `n`.
fn wait_reloads(live: &Live, n: u64) {
    let start = Instant::now();
    while reloads(live) < n {
        assert!(
            start.elapsed() < WAIT,
            "timed out waiting for reload #{n} (at {})",
            reloads(live)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn code(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.code.clone())
}

fn box_lines(live: &Live, id: &str) -> Option<(u32, u32)> {
    let id = id.to_string();
    live.with(move |app| {
        app.doc
            .annotations
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| match &a.shape {
                tiza_core::model::Shape::Box {
                    from_line, to_line, ..
                } => Some((*from_line, *to_line)),
                _ => None,
            })
    })
}

#[test]
fn tmp_rename_save_reloads_once_after_debounce() {
    let (live, src) = followed("ac1", CODE);
    live.command("follow on");
    assert_eq!(live.message().unwrap().0, "following main.rs");
    assert_eq!(
        reloads(&live),
        0,
        "enabling with unchanged content is not a reload"
    );

    save_like_zed(&src, &CODE.replace("done", "finished"));
    wait_reloads(&live, 1);
    assert!(code(&live)[3].contains("finished"), "{:?}", code(&live));
    let (msg, is_err) = live.message().unwrap();
    assert!(!is_err);
    assert_eq!(msg, "main.rs reloaded (#1): 0 annotations kept");

    // A burst of writes within the debounce window is one reload.
    for i in 0..5 {
        std::fs::write(&src, CODE.replace("done", &format!("v{i}"))).unwrap();
        std::thread::sleep(Duration::from_millis(15));
    }
    wait_reloads(&live, 2);
    settle();
    assert_eq!(reloads(&live), 2, "burst must collapse into one reload");
    assert!(code(&live)[3].contains("v4"), "{:?}", code(&live));
}

#[test]
fn annotations_follow_their_text_and_u_undoes() {
    let (live, src) = followed("ac2", CODE);
    live.command("box 2-3 ownership");
    live.command("note 4 prints");
    live.command("set a1 step=2");
    live.command("step 2");
    live.command("theme light");
    live.keys(&[KeyCode::Char('j'), KeyCode::Char('j')]);
    let cursor_before = live.with(|app| app.cursor_line);
    live.command("follow on");

    // Two lines inserted at the top: the box moves down, the note's line is gone.
    let new = format!(
        "// header\n// more\n{}",
        CODE.replace("    println!(\"done\");\n", "")
    );
    save_like_zed(&src, &new);
    wait_reloads(&live, 1);

    assert_eq!(
        box_lines(&live, "a1"),
        Some((4, 5)),
        "box re-anchored to its text"
    );
    let ids: Vec<String> =
        live.with(|app| app.doc.annotations.iter().map(|a| a.id.clone()).collect());
    assert_eq!(ids, vec!["a1"], "the note whose text vanished is dropped");
    let (msg, _) = live.message().unwrap();
    assert_eq!(
        msg,
        "main.rs reloaded (#1): 1 annotation kept, 1 dropped (text gone; u undoes)"
    );
    let (step, theme, cursor) = live.with(|app| (app.doc.step, app.theme.name, app.cursor_line));
    assert_eq!(step, 2, "step survives");
    assert_eq!(theme, tiza_core::ThemeName::Light, "theme survives");
    assert_eq!(cursor, cursor_before, "cursor survives");

    // One undo brings back the previous code and both annotations.
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(
        code(&live),
        CODE.lines().map(str::to_string).collect::<Vec<_>>()
    );
    assert_eq!(live.with(|app| app.doc.annotations.len()), 2);
    assert_eq!(box_lines(&live, "a1"), Some((2, 3)));
}

#[test]
fn status_line_and_follow_command() {
    let (live, src) = followed("ac3", CODE);
    let bar = live.screen(120, 12).lines().last().unwrap().to_string();
    assert!(!bar.contains("following"), "{bar}");
    live.command("follow");
    assert_eq!(
        live.message().unwrap().0,
        "follow off (:follow on to reload when the file is saved)"
    );

    live.command("follow on");
    let bar = live.screen(120, 12).lines().last().unwrap().to_string();
    assert!(bar.contains("⟳ following"), "{bar}");
    assert!(!bar.contains("last"), "no reload yet: {bar}");

    save_like_zed(&src, &CODE.replace("done", "ok"));
    wait_reloads(&live, 1);
    let bar = live.screen(120, 12).lines().last().unwrap().to_string();
    assert!(bar.contains("⟳ following · last "), "{bar}");
    let at = live.api_ok("follow.status", json!({}))["last_reload"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(bar.contains(&at), "{bar} should show {at}");
    live.command("follow");
    assert!(
        live.message()
            .unwrap()
            .0
            .starts_with("following main.rs (1 reload"),
        "{:?}",
        live.message()
    );
    live.command("follow on");
    assert_eq!(live.message().unwrap().0, "already following main.rs");

    live.command("follow off");
    assert_eq!(live.message().unwrap().0, "follow off");
    let bar = live.screen(120, 12).lines().last().unwrap().to_string();
    assert!(!bar.contains("following"), "{bar}");
    save_like_zed(&src, &CODE.replace("done", "ignored"));
    settle();
    assert!(code(&live)[3].contains("ok"), "no reload after :follow off");

    live.command("follow sideways");
    let (msg, is_err) = live.message().unwrap();
    assert!(is_err);
    assert!(msg.contains("usage: :follow [on|off]"), "{msg}");
}

#[test]
fn socket_and_mcp() {
    let (live, src) = followed("ac4", CODE);
    let st = live.api_ok("follow.status", json!({}));
    assert_eq!(st["enabled"], false);
    assert_eq!(st["path"].as_str().unwrap(), src.display().to_string());

    let st = live.api_ok("follow.set", json!({"enabled": true}));
    assert_eq!(st["enabled"], true);
    assert_eq!(st["reloads"], 0);
    assert_eq!(st["message"].as_str().unwrap(), "following main.rs");

    let r = live.api("follow.set", json!({}));
    assert!(r.error.is_some(), "enabled is required");

    let mut mcp = McpClient::start(&live.socket);
    assert!(mcp.tool_names().iter().any(|n| n == "set_follow"));
    let (err, text, _) = mcp.call("set_follow", json!({"enabled": false}));
    assert!(!err, "{text}");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["enabled"], false);
    let (err, text, _) = mcp.call("set_follow", json!({"enabled": true}));
    assert!(!err, "{text}");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["enabled"], true);
    assert_eq!(v["path"].as_str().unwrap(), src.display().to_string());

    save_like_zed(&src, &CODE.replace("done", "via-mcp"));
    wait_reloads(&live, 1);
    let st = live.api_ok("follow.status", json!({}));
    assert_eq!(st["reloads"], 1);
    assert!(st["last_reload"].as_str().unwrap().ends_with('Z'));
    assert_eq!(st["missing"], false);
}

#[test]
fn embedded_code_has_nothing_to_follow() {
    let live = Live::from_code(CODE, "rs", "embedded");
    live.command("follow on");
    let (msg, is_err) = live.message().unwrap();
    assert!(is_err);
    assert!(
        msg.starts_with("nothing to follow: the code is embedded"),
        "{msg}"
    );
    let r = live.api("follow.set", json!({"enabled": true}));
    assert!(r.error.unwrap().message.contains("nothing to follow"));
    assert_eq!(
        live.api_ok("follow.status", json!({}))["path"],
        serde_json::Value::Null
    );
}

#[test]
fn removed_file_waits_and_recreation_reloads() {
    let (live, src) = followed("removed", CODE);
    live.command("follow on");
    std::fs::remove_file(&src).unwrap();
    let start = Instant::now();
    loop {
        if let Some((m, true)) = live.message()
            && m == "main.rs removed; waiting for it to come back"
        {
            break;
        }
        assert!(start.elapsed() < WAIT, "{:?}", live.message());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(live.api_ok("follow.status", json!({}))["missing"], true);
    assert_eq!(
        code(&live).len(),
        CODE.lines().count(),
        "previous content kept"
    );
    let bar = live.screen(120, 12).lines().last().unwrap().to_string();
    assert!(bar.contains("file removed; waiting"), "{bar}");

    std::fs::write(&src, CODE).unwrap();
    wait_reloads(&live, 1);
    assert_eq!(live.api_ok("follow.status", json!({}))["missing"], false);
    assert_eq!(
        live.message().unwrap().0,
        "main.rs reloaded (#1): 0 annotations kept"
    );
}

#[test]
fn truncate_then_write_is_one_reload() {
    let (live, src) = followed("truncate", CODE);
    live.command("box 2-3 keep");
    live.command("follow on");
    // An editor that truncates first and writes the content afterwards.
    std::fs::write(&src, "").unwrap();
    std::thread::sleep(Duration::from_millis(60));
    std::fs::write(&src, CODE.replace("done", "rewritten")).unwrap();
    wait_reloads(&live, 1);
    settle();
    assert_eq!(reloads(&live), 1);
    assert!(code(&live)[3].contains("rewritten"));
    assert_eq!(
        box_lines(&live, "a1"),
        Some((2, 3)),
        "never saw the empty file"
    );
}

#[test]
fn identical_content_and_sidecar_autosave_do_not_reload() {
    let (live, src) = followed("same", CODE);
    live.command("follow on");
    save_like_zed(&src, CODE);
    settle();
    assert_eq!(reloads(&live), 0, "same bytes: nothing to do");
    assert!(live.message().is_none_or(|(m, _)| !m.contains("reloaded")));

    // Annotating autosaves `main.rs.tiza` in the watched folder.
    for i in 0..3 {
        live.command(&format!("box {} sidecar", i + 1));
    }
    settle();
    assert_eq!(reloads(&live), 0, "the sidecar write is not the source");
    assert!(src.with_extension("rs.tiza").exists());
}

#[test]
fn non_utf8_keeps_previous_content() {
    let (live, src) = followed("utf8", CODE);
    live.command("follow on");
    std::fs::write(&src, [0xff, 0xfe, b'f', b'n']).unwrap();
    let start = Instant::now();
    loop {
        if let Some((m, true)) = live.message()
            && m.contains("not valid UTF-8")
        {
            assert_eq!(
                m,
                "cannot follow main.rs: not valid UTF-8 (keeping the previous content)"
            );
            break;
        }
        assert!(start.elapsed() < WAIT, "{:?}", live.message());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        code(&live),
        CODE.lines().map(str::to_string).collect::<Vec<_>>()
    );
    assert_eq!(reloads(&live), 0);
    // A valid write afterwards recovers.
    save_like_zed(&src, &CODE.replace("done", "back"));
    wait_reloads(&live, 1);
    assert!(code(&live)[3].contains("back"));
}

#[test]
fn line_range_reloads_only_its_lines() {
    let dir = scratch("range");
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, CODE).unwrap();
    let all: Vec<&str> = CODE.lines().collect();
    let mut doc = Document::from_code(&all[1..4].join("\n"), "rs", Some(src.display().to_string()));
    doc.source.first_line = 2;
    doc.reference_file(src.display().to_string(), Some((2, 4)));
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    live.command("box 2-3 inner");
    live.command("follow on");
    assert_eq!(code(&live).len(), 3);

    save_like_zed(&src, &CODE.replace("takes(s);", "takes(s.clone());"));
    wait_reloads(&live, 1);
    let c = code(&live);
    assert_eq!(c.len(), 3, "still the 3 lines of the range: {c:?}");
    assert!(c[1].contains("takes(s.clone())"), "{c:?}");
    assert_eq!(live.with(|app| app.doc.source.first_line), 2);
    // The box's bottom line was edited: it keeps its height from the top edge.
    assert_eq!(box_lines(&live, "a1"), Some((2, 3)), "{:?}", live.message());
    assert_eq!(
        live.message().unwrap().0,
        "main.rs reloaded (#1): 1 annotation kept"
    );
}

#[test]
fn editing_one_edge_line_of_a_box_keeps_it() {
    let (live, src) = followed("edge", CODE);
    live.command("box 2-3 ownership");
    live.command("callout 2-4 @below title=\"t\" explains");
    live.command("follow on");
    // Line 2 (top edge of both ranges) is rewritten; line 1 is new.
    let new = CODE
        .replace("fn main() {\n", "fn main() {\n    // setup\n")
        .replace("String::new()", "String::from(\"x\")");
    save_like_zed(&src, &new);
    wait_reloads(&live, 1);
    // `takes(s);` is now line 4: the box keeps its height ending there.
    assert_eq!(box_lines(&live, "a1"), Some((3, 4)), "{:?}", live.message());
    assert_eq!(
        live.with(|app| app.doc.annotations.len()),
        2,
        "{:?}",
        live.message()
    );
    // Both edges gone → dropped.
    save_like_zed(&src, "fn other() {}\n");
    wait_reloads(&live, 2);
    assert_eq!(live.with(|app| app.doc.annotations.len()), 0);
}

// ---------- repeated lines: the nearest copy, never the first ----------

fn repeated_fixture(name: &str) -> (Live, PathBuf, String) {
    let code = std::fs::read_to_string(fixture("follow/reload_on_save/repeated.rs")).unwrap();
    let (live, src) = followed(name, &code);
    (live, src, code)
}

/// Lines (1-based) of the annotation `id`, whatever its kind.
fn lines_of(live: &Live, id: &str) -> Option<(u32, u32)> {
    let id = id.to_string();
    live.with(move |app| {
        app.doc
            .annotations
            .iter()
            .find(|a| a.id == id)
            .map(|a| match &a.shape {
                Shape::Box {
                    from_line, to_line, ..
                } => (*from_line, *to_line),
                Shape::Note { line, .. } => (*line, *line),
                Shape::Highlight { span, .. } | Shape::Mark { span, .. } => (span.line, span.line),
                Shape::Arrow { from, to, .. } => (from.line, to.line),
                Shape::Callout {
                    from_line, to_line, ..
                } => (*from_line, to_line.unwrap_or(*from_line)),
                Shape::Spotlight {
                    from_line, to_line, ..
                } => (from_line.unwrap_or(0), to_line.unwrap_or(0)),
            })
    })
}

#[test]
fn second_brace_and_second_fn_new_stay_on_their_copy() {
    let (live, src, code) = repeated_fixture("repeated");
    // Line 9 is the second `}` (closing `impl Stack`); line 16 the second `fn new`.
    assert_eq!(code.lines().nth(8).unwrap(), "}");
    assert_eq!(code.lines().nth(15).unwrap(), "    fn new() -> Self {");
    live.command("note 9 end of impl Stack");
    live.command("hl 16:new second new");
    live.command("box 15-18 queue ctor");
    live.command("follow on");

    save_like_zed(&src, &format!("// 1\n// 2\n// 3\n// 4\n{code}"));
    wait_reloads(&live, 1);
    assert_eq!(
        lines_of(&live, "a1"),
        Some((13, 13)),
        "{:?}",
        live.message()
    );
    assert_eq!(
        lines_of(&live, "a2"),
        Some((20, 20)),
        "{:?}",
        live.message()
    );
    assert_eq!(
        lines_of(&live, "a3"),
        Some((19, 22)),
        "{:?}",
        live.message()
    );
    let (msg, _) = live.message().unwrap();
    assert_eq!(msg, "main.rs reloaded (#1): 3 annotations kept");
}

#[test]
fn deleting_a_neighbour_keeps_the_copy_deleting_the_copy_drops_it() {
    let (live, src, code) = repeated_fixture("repeated2");
    live.command("note 9 end of impl Stack");
    live.command("follow on");

    // Delete line 8 (the `}` closing `fn new`): the note is still on the brace
    // that closes `impl Stack`, now line 8 — not on line 3 or line 17.
    let mut lines: Vec<&str> = code.lines().collect();
    lines.remove(7);
    save_like_zed(&src, &(lines.join("\n") + "\n"));
    wait_reloads(&live, 1);
    assert_eq!(lines_of(&live, "a1"), Some((8, 8)), "{:?}", live.message());

    // Delete the annotated copy itself (the whole `impl Stack` block): dropped.
    let without: Vec<&str> = code
        .lines()
        .enumerate()
        .filter(|(i, _)| !(4..9).contains(i))
        .map(|(_, l)| l)
        .collect();
    save_like_zed(&src, &(without.join("\n") + "\n"));
    wait_reloads(&live, 2);
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("1 dropped"), "{msg}");
    assert_eq!(lines_of(&live, "a1"), None);
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(lines_of(&live, "a1"), Some((8, 8)), "u restores the note");
}

#[test]
fn same_word_twice_in_a_line_keeps_the_occurrence() {
    let (live, src) = followed("occurrence", "fn f(s: &str) {\n    takes(s, s.len());\n}\n");
    live.command("hl 2:s second s");
    // The second `s` is at column 13: `hl L:text` finds the first one, so move it.
    live.with(|app| {
        app.run_command("set a1 label=second");
        let a = app
            .doc
            .annotations
            .iter_mut()
            .find(|a| a.id == "a1")
            .unwrap();
        if let Shape::Highlight { span, .. } = &mut a.shape {
            span.start = 13;
            span.end = 14;
        }
    });
    live.command("follow on");
    // The line is edited in place; the second `s` is still there (col 21).
    save_like_zed(&src, "fn f(s: &str) {\n    takes(s.clone(), s.len());\n}\n");
    wait_reloads(&live, 1);
    let span = live.with(|app| match &app.doc.annotations[0].shape {
        Shape::Highlight { span, .. } => *span,
        _ => unreachable!(),
    });
    assert_eq!((span.line, span.start, span.end), (2, 21, 22));
    // The occurrence disappears: dropped, never moved to the remaining `s`.
    save_like_zed(&src, "fn f(s: &str) {\n    takes(s.clone());\n}\n");
    wait_reloads(&live, 2);
    assert_eq!(live.with(|app| app.doc.annotations.len()), 0);
    assert!(live.message().unwrap().0.contains("1 dropped"));
}

#[test]
fn far_copy_counts_as_gone() {
    // A `}` at the top and the annotated `}` 60 lines below; the second is
    // deleted. The first is beyond MAX_DISTANCE (50) → dropped, not moved.
    let filler: String = (0..60).map(|i| format!("// line {i}\n")).collect();
    let code = format!("}}\n{filler}}}\n// end\n");
    let (live, src) = followed("far", &code);
    live.command("note 62 closes");
    live.command("follow on");
    save_like_zed(&src, &format!("}}\n{filler}// end\n"));
    wait_reloads(&live, 1);
    assert_eq!(
        live.with(|app| app.doc.annotations.len()),
        0,
        "{:?}",
        live.message()
    );
    assert_eq!(tiza_tui::reanchor::MAX_DISTANCE, 50, "documented value");
}

#[test]
fn random_edits_above_keep_a_unique_line_and_never_panic() {
    use proptest::prelude::*;
    use tiza_tui::reanchor::Remap;

    let mut runner = proptest::test_runner::TestRunner::new(ProptestConfig {
        cases: 300,
        ..ProptestConfig::default()
    });
    let strategy = (
        proptest::collection::vec("[a-c }]{0,6}", 1..30),
        proptest::collection::vec(any::<bool>(), 0..30),
        1u32..30,
    );
    runner
        .run(&strategy, |(lines, ops, pick)| {
            let old = Document::from_code(&(lines.join("\n") + "\n"), "rs", None);
            let target = (pick % lines.len() as u32) + 1;
            // Random insertions/deletions strictly above the target line.
            let mut new_lines: Vec<String> = lines.clone();
            let mut shift: i64 = 0;
            for (i, op) in ops.iter().enumerate() {
                let at = i % (target as usize);
                if at >= (target as i64 + shift - 1).max(0) as usize {
                    continue;
                }
                if *op {
                    new_lines.insert(at, format!("// inserted {i}"));
                    shift += 1;
                } else if new_lines.len() > 1 && (new_lines.len() as i64) > target as i64 + shift {
                    new_lines.remove(at);
                    shift -= 1;
                }
            }
            let new = Document::from_code(&(new_lines.join("\n") + "\n"), "rs", None);
            let mut r = Remap::new(&old, &new);
            let got = r.line(target);
            let text = lines[target as usize - 1].trim();
            let unique = !text.is_empty() && lines.iter().filter(|l| l.trim() == text).count() == 1;
            if unique && shift.unsigned_abs() <= 50 {
                let expected = (target as i64 + shift) as u32;
                prop_assert_eq!(
                    got,
                    Some(expected),
                    "unique line follows the edits above it"
                );
            }
            if let Some(l) = got {
                prop_assert!(new.has_line(l));
            }
            Ok(())
        })
        .unwrap();
}

// ---------- QA round 2 (Luis, real Zed): ranges, formatter reflow, notes, undo ----------

/// `examples/src/stack.rs` as Luis tested it (37 lines; `fn pop` 16-18,
/// `fn peek` 20-22, `fn is_empty` 28-30).
fn stack_fixture(name: &str) -> (Live, PathBuf, Vec<String>) {
    let code = std::fs::read_to_string(fixture("follow/reload_on_save/stack.rs")).unwrap();
    let (live, src) = followed(name, &code);
    (live, src, code.lines().map(String::from).collect())
}

/// Moves the lines `from..=to` (1-based; pass the trailing blank line as
/// `to`) to just after line `after`, like cut and paste in the editor.
fn move_block(lines: &[String], from: usize, to: usize, after: usize) -> String {
    let block: Vec<String> = lines[from - 1..to].to_vec();
    let mut rest: Vec<String> = lines[..from - 1].to_vec();
    rest.extend_from_slice(&lines[to..]);
    let at = if after > to {
        after - block.len()
    } else {
        after
    };
    let mut out = rest[..at].to_vec();
    out.extend(block);
    out.extend_from_slice(&rest[at..]);
    out.join("\n") + "\n"
}

fn count(live: &Live) -> usize {
    live.with(|app| app.doc.annotations.len())
}

#[test]
fn bottom_edge_leaves_with_a_moved_block_all_three_kinds_keep_their_height() {
    let (live, src, lines) = stack_fixture("step13");
    assert_eq!(lines[15], "    pub fn pop(&mut self) -> Option<T> {");
    live.command("box 12-18 push and pop");
    live.command("spot 12-18");
    live.command("callout 12-18 @below both mutate");
    live.command("follow on");
    // The pop block (16-18 + blank) goes below is_empty: the box's bottom
    // edge (`}` of pop) is gone from there.
    save_like_zed(&src, &move_block(&lines, 16, 19, 30));
    wait_reloads(&live, 1);
    let (msg, _) = live.message().unwrap();
    assert_eq!(msg, "main.rs reloaded (#1): 3 annotations kept", "{msg}");
    for id in ["a1", "a2", "a3"] {
        assert_eq!(
            lines_of(&live, id),
            Some((12, 18)),
            "{id}: height from the top edge"
        );
    }
}

#[test]
fn unique_bottom_edge_moves_away_the_range_never_stretches() {
    let (live, src, lines) = stack_fixture("step14");
    assert_eq!(lines[19], "    pub fn peek(&self) -> Option<&T> {");
    live.command("box 14-20 x");
    live.command("spot 14-20");
    live.command("callout 14-20 @below y");
    live.command("follow on");
    // The peek block moves below is_empty: `pub fn peek(` is unique and now
    // at line 27, but the range must stay 14-20, never 14-27.
    save_like_zed(&src, &move_block(&lines, 20, 23, 30));
    wait_reloads(&live, 1);
    assert_eq!(code(&live)[26], "    pub fn peek(&self) -> Option<&T> {");
    for id in ["a1", "a2", "a3"] {
        assert_eq!(
            lines_of(&live, id),
            Some((14, 20)),
            "{id}: {:?}",
            live.message()
        );
    }
    assert_eq!(count(&live), 3);
}

#[test]
fn reordering_methods_the_callout_travels_with_its_block() {
    let (live, src, lines) = stack_fixture("reorder");
    live.command("callout 20-22 @below peek borrows");
    live.command("follow on");
    // peek (20-22 + blank) is cut and pasted above push (line 12).
    save_like_zed(&src, &move_block(&lines, 20, 23, 11));
    wait_reloads(&live, 1);
    assert_eq!(code(&live)[11], "    pub fn peek(&self) -> Option<&T> {");
    assert_eq!(
        lines_of(&live, "a1"),
        Some((12, 14)),
        "{:?}",
        live.message()
    );
}

#[test]
fn rustfmt_split_and_join_follow_the_span() {
    let (live, src, lines) = stack_fixture("step15");
    assert_eq!(lines[8], "        Self { items: Vec::new() }");
    live.command("hl 9:Vec::new x");
    live.command("follow on");
    let mut split = lines.clone();
    split.splice(
        8..9,
        [
            "        Self {",
            "            items: Vec::new(),",
            "        }",
        ]
        .map(String::from),
    );
    save_like_zed(&src, &(split.join("\n") + "\n"));
    wait_reloads(&live, 1);
    let (msg, _) = live.message().unwrap();
    assert_eq!(msg, "main.rs reloaded (#1): 1 annotation kept", "{msg}");
    let span = live.with(|app| match &app.doc.annotations[0].shape {
        Shape::Highlight { span, .. } => (span.line, span.start, span.end),
        _ => unreachable!(),
    });
    assert_eq!(span, (10, 19, 27), "on `Vec::new` of the new line 10");
    // Join the three lines back into one: it returns to line 9.
    save_like_zed(&src, &(lines.join("\n") + "\n"));
    wait_reloads(&live, 2);
    let span = live.with(|app| match &app.doc.annotations[0].shape {
        Shape::Highlight { span, .. } => (span.line, span.start, span.end),
        _ => unreachable!(),
    });
    assert_eq!(span, (9, 22, 30));
}

#[test]
fn replaced_line_keeps_its_note_not_a_span_whose_text_is_gone() {
    let (live, src, lines) = stack_fixture("step16");
    assert_eq!(lines[3], "    items: Vec<T>,");
    live.command("note 4 owned");
    live.command("hl 4:Vec<T> y");
    live.command("follow on");
    let mut edited = lines.clone();
    edited[3] = "    items: VecDeque<T>,".into();
    save_like_zed(&src, &(edited.join("\n") + "\n"));
    wait_reloads(&live, 1);
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("1 annotation kept, 1 dropped"), "{msg}");
    assert_eq!(
        lines_of(&live, "a1"),
        Some((4, 4)),
        "the note stays on its line"
    );
    assert_eq!(lines_of(&live, "a2"), None, "the span's text is gone");
    // Completely different text: the note still has its line.
    edited[3] = "    return;".into();
    save_like_zed(&src, &(edited.join("\n") + "\n"));
    wait_reloads(&live, 2);
    assert_eq!(lines_of(&live, "a1"), Some((4, 4)));
}

#[test]
fn one_u_after_the_editor_undo_brings_the_dropped_annotation_back() {
    let (live, src, lines) = stack_fixture("step17");
    live.command("hl 9:Vec::new x");
    live.command("follow on");
    let mut edited = lines.clone();
    edited[8] = "        Self { items: VecDeque::new() }".into();
    save_like_zed(&src, &(edited.join("\n") + "\n"));
    wait_reloads(&live, 1);
    assert_eq!(count(&live), 0, "{:?}", live.message());
    // Cmd+Z in the editor, save: the text is back, nothing to re-anchor.
    save_like_zed(&src, &(lines.join("\n") + "\n"));
    wait_reloads(&live, 2);
    assert_eq!(count(&live), 0);
    assert!(code(&live)[8].contains("Vec::new()"));
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(
        count(&live),
        1,
        "one `u` reaches the reload that dropped it"
    );
    assert!(
        code(&live)[8].contains("Vec::new()"),
        "and the code is the restored one"
    );
    // Editor undo twice in a row (edit, save, undo, save, undo, save) is
    // still one `u` away from the state before the first drop.
    live.command("hl 13:push z");
    let mut e2 = lines.clone();
    e2[12] = "        self.items.insert(0, item);".into();
    save_like_zed(&src, &(e2.join("\n") + "\n"));
    wait_reloads(&live, 3);
    assert_eq!(count(&live), 1, "{:?}", live.message());
    save_like_zed(&src, &(lines.join("\n") + "\n"));
    wait_reloads(&live, 4);
    let mut e3 = lines.clone();
    e3[0] = "//! A tiny stack.".into();
    save_like_zed(&src, &(e3.join("\n") + "\n"));
    wait_reloads(&live, 5);
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(count(&live), 2, "{:?}", live.message());
}

#[test]
fn n_to_n_hunk_keeps_the_mark_and_the_box_in_place() {
    let (live, src, lines) = stack_fixture("step18");
    live.command("mark write 13:push moved");
    live.command("box 12-14 push");
    assert_eq!(count(&live), 2, "{:?}", live.message());
    live.command("follow on");
    // `item` → `value` on lines 12 AND 13: a 2↔2 hunk, paired in place.
    let mut e = lines.clone();
    e[11] = "    pub fn push(&mut self, value: T) {".into();
    e[12] = "        self.items.push(value);".into();
    save_like_zed(&src, &(e.join("\n") + "\n"));
    wait_reloads(&live, 1);
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("2 annotations kept"), "{msg}");
    assert_eq!(
        lines_of(&live, "a1"),
        Some((13, 13)),
        "the mark stays on `push`"
    );
    assert_eq!(lines_of(&live, "a2"), Some((12, 14)));
    // Lines 12, 13 and 14 edited (3↔3): the same.
    let mut e3 = e.clone();
    e3[13] = "    } // push".into();
    save_like_zed(&src, &(e3.join("\n") + "\n"));
    wait_reloads(&live, 2);
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("2 annotations kept"), "{msg}");
    assert_eq!(lines_of(&live, "a1"), Some((13, 13)));
    assert_eq!(lines_of(&live, "a2"), Some((12, 14)));
    // Lines 13 and 14 edited: still both.
    let mut e2 = lines.clone();
    e2[12] = "        self.items.push(item); // owned".into();
    e2[13] = "    } // push".into();
    save_like_zed(&src, &(e2.join("\n") + "\n"));
    wait_reloads(&live, 3);
    assert_eq!(count(&live), 2, "{:?}", live.message());
    assert_eq!(lines_of(&live, "a1"), Some((13, 13)));
}
