//! t-011 — Follow mode: reload the code when the source file is saved.
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
//!   the sidecar `.codedraw` autosave in the same folder never triggers a reload;
//!   ranges `FILE:A-B` reload only their lines.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use codedraw_core::Document;
use codedraw_qa::{Live, McpClient, scratch};
use crossterm::event::KeyCode;
use serde_json::json;

const CODE: &str = "fn main() {\n    let s = String::new();\n    takes(s);\n    println!(\"done\");\n}\n\nfn takes(s: String) {}\n";

/// A source file next to its sidecar document, opened by reference.
fn followed(name: &str, code: &str) -> (Live, PathBuf) {
    let dir = scratch(name);
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, code).unwrap();
    let mut doc = Document::from_code(code, "rs", Some(src.display().to_string()));
    doc.reference_file(src.display().to_string(), None);
    let live = Live::open(doc, dir.join("main.rs.codedraw"), dir.join("live.sock"));
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

/// Waits (up to 3 s) until the reload counter reaches `n`.
fn wait_reloads(live: &Live, n: u64) {
    let start = Instant::now();
    while reloads(live) < n {
        assert!(
            start.elapsed() < Duration::from_secs(3),
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
                codedraw_core::model::Shape::Box {
                    from_line, to_line, ..
                } => Some((*from_line, *to_line)),
                _ => None,
            })
    })
}

#[test]
fn ac1_tmp_rename_save_reloads_once_after_debounce() {
    let (live, src) = followed("t011-ac1", CODE);
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
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(reloads(&live), 2, "burst must collapse into one reload");
    assert!(code(&live)[3].contains("v4"), "{:?}", code(&live));
}

#[test]
fn ac2_annotations_follow_their_text_and_u_undoes() {
    let (live, src) = followed("t011-ac2", CODE);
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
    assert_eq!(theme, codedraw_core::ThemeName::Light, "theme survives");
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
fn ac3_status_line_and_follow_command() {
    let (live, src) = followed("t011-ac3", CODE);
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
    std::thread::sleep(Duration::from_millis(400));
    assert!(code(&live)[3].contains("ok"), "no reload after :follow off");

    live.command("follow sideways");
    let (msg, is_err) = live.message().unwrap();
    assert!(is_err);
    assert!(msg.contains("usage: :follow [on|off]"), "{msg}");
}

#[test]
fn ac4_socket_and_mcp() {
    let (live, src) = followed("t011-ac4", CODE);
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
fn corner_embedded_code_has_nothing_to_follow() {
    let live = Live::from_code(CODE, "rs", "t011-embedded");
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
fn corner_removed_file_waits_and_recreation_reloads() {
    let (live, src) = followed("t011-removed", CODE);
    live.command("follow on");
    std::fs::remove_file(&src).unwrap();
    let start = Instant::now();
    loop {
        if let Some((m, true)) = live.message()
            && m == "main.rs removed; waiting for it to come back"
        {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "{:?}",
            live.message()
        );
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
fn corner_truncate_then_write_is_one_reload() {
    let (live, src) = followed("t011-truncate", CODE);
    live.command("box 2-3 keep");
    live.command("follow on");
    // An editor that truncates first and writes the content afterwards.
    std::fs::write(&src, "").unwrap();
    std::thread::sleep(Duration::from_millis(60));
    std::fs::write(&src, CODE.replace("done", "rewritten")).unwrap();
    wait_reloads(&live, 1);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(reloads(&live), 1);
    assert!(code(&live)[3].contains("rewritten"));
    assert_eq!(
        box_lines(&live, "a1"),
        Some((2, 3)),
        "never saw the empty file"
    );
}

#[test]
fn corner_identical_content_and_sidecar_autosave_do_not_reload() {
    let (live, src) = followed("t011-same", CODE);
    live.command("follow on");
    save_like_zed(&src, CODE);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(reloads(&live), 0, "same bytes: nothing to do");
    assert!(live.message().is_none_or(|(m, _)| !m.contains("reloaded")));

    // Annotating autosaves `main.rs.codedraw` in the watched folder.
    for i in 0..3 {
        live.command(&format!("box {} sidecar", i + 1));
    }
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(reloads(&live), 0, "the sidecar write is not the source");
    assert!(src.with_extension("rs.codedraw").exists());
}

#[test]
fn corner_non_utf8_keeps_previous_content() {
    let (live, src) = followed("t011-utf8", CODE);
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
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "{:?}",
            live.message()
        );
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
fn corner_line_range_reloads_only_its_lines() {
    let dir = scratch("t011-range");
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, CODE).unwrap();
    let all: Vec<&str> = CODE.lines().collect();
    let mut doc = Document::from_code(&all[1..4].join("\n"), "rs", Some(src.display().to_string()));
    doc.source.first_line = 2;
    doc.reference_file(src.display().to_string(), Some((2, 4)));
    let live = Live::open(doc, dir.join("main.rs.codedraw"), dir.join("live.sock"));
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
fn corner_follow_flag_with_stdin_reports_nothing_to_follow() {
    // The TUI needs a terminal, so the check is on the App: a document read
    // from stdin is embedded and `--follow` at start produces the message.
    let live = Live::from_code(CODE, "rs", "t011-stdin");
    let msg = live.with(|app| app.set_follow(true).map_err(|e| e.to_string()));
    assert!(msg.unwrap_err().starts_with("nothing to follow"));
}

#[test]
fn corner_editing_one_edge_line_of_a_box_keeps_it() {
    let (live, src) = followed("t011-edge", CODE);
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
