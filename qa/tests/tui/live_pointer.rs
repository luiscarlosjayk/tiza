//! Live pointer: `f` toggles a temporary focus on the cursor line
//! (or the selection) that is never saved or exported; agents can point too.
//!
//! Acceptance criteria:
//! - AC1 `f` toggles the pointer. While it is on, the cursor line is drawn
//!   normally and everything else is dimmed with the same dimming a
//!   spotlight uses; with a selection (`v`/`V`), the selection is what stays
//!   undimmed. `f` again, or `Esc`, turns it off.
//! - AC2 The pointer follows the cursor (`j`/`k`, `g`/`G`, word motions, the
//!   mouse when `--mouse` is on). Annotations of the current step stay
//!   visible.
//! - AC3 Presentation mode (`P`): `f` shows the pointer on the last cursor
//!   line and `j`/`k` move it; the cursor itself stays hidden. Nothing else
//!   of the presentation UI appears.
//! - AC4 Never part of the document: no annotation is created, nothing is
//!   written to the .tiza, the undo stack and `unsaved` do not change,
//!   autosave is not triggered.
//! - AC5 Never exported: PNG, SVG, GIF, MP4, .cast, .tape, txt/md/ansi and
//!   `render_text` do not draw it (it is for the live take only).
//! - AC6 Agents can point: socket `pointer.set {line}` or `{line, find}` /
//!   `{from, to}` and `pointer.clear`; MCP tools `point_at` and
//!   `clear_pointer`; `status` reports the pointer (`pointer: {line…}` or
//!   null). An agent pointer and the presenter's `f` are the same pointer
//!   (the last one wins).
//! - AC7 Docs: README keys table, agent guide, CHANGELOG; schema
//!   regenerated; this file.
//!
//! Corner cases: the pointed line off screen (the view scrolls to it); a
//! step change keeps the pointer on its line (or where a move shifted it),
//! off when the line disappears; a key typed into the `:` line never toggles it; an agent pointing while
//! the command line is open shows after it closes.

use std::collections::BTreeSet;

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_core::fs;
use tiza_core::layout::{Layout, Row};
use tiza_qa::{Live, McpClient, scratch, tiza, tiza_ok};

/// Twenty lines, no annotations: every code line starts with text.
fn code20() -> String {
    (1..=20)
        .map(|i| format!("let v{i} = {i};\n"))
        .collect::<String>()
}

/// The lines whose code is drawn normally (not in the dimmed color) on the
/// presenter's own frame.
fn lit(live: &Live) -> BTreeSet<u32> {
    live.with(|app| {
        let l = app.screen_layout_with_pointer(100);
        lit_in(&l, app.theme.dimmed)
    })
}

fn lit_in(l: &Layout, dimmed: tiza_core::canvas::Rgb) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for (y, row) in l.rows.iter().enumerate() {
        let Row::Code(line) = row else { continue };
        let first = (l.code_x..l.canvas.width)
            .filter_map(|x| l.canvas.get(x, y))
            .find(|c| !c.sym.trim().is_empty());
        if let Some(c) = first
            && c.style.fg != Some(dimmed)
        {
            out.insert(*line);
        }
    }
    out
}

fn all(a: u32, b: u32) -> BTreeSet<u32> {
    (a..=b).collect()
}

fn pointer(live: &Live) -> Option<(u32, u32)> {
    live.with(|app| app.pointer.map(|p| (p.line, p.to)))
}

fn press(live: &Live, keys: &[KeyCode]) {
    live.keys(keys);
}

fn j(n: usize) -> Vec<KeyCode> {
    vec![KeyCode::Char('j'); n]
}

#[test]
fn f_toggles_the_pointer_on_the_cursor_line_and_esc_turns_it_off() {
    let live = Live::from_code(&code20(), "rs", "ac1");
    assert_eq!(lit(&live), all(1, 20), "nothing dimmed before");
    press(&live, &j(6));
    press(&live, &[KeyCode::Char('f')]);
    assert_eq!(pointer(&live), Some((7, 7)));
    assert_eq!(lit(&live), all(7, 7), "line 7 normal, the rest dimmed");
    // The dimming is the spotlight's: the theme's dimmed color.
    press(&live, &[KeyCode::Char('j')]);
    assert_eq!(lit(&live), all(8, 8), "j moves it");
    press(&live, &[KeyCode::Char('f')]);
    assert_eq!(pointer(&live), None);
    assert_eq!(lit(&live), all(1, 20), "f again: back to normal");
    press(&live, &[KeyCode::Char('f')]);
    assert!(pointer(&live).is_some());
    press(&live, &[KeyCode::Esc]);
    assert_eq!(pointer(&live), None, "Esc turns it off");
    assert_eq!(lit(&live), all(1, 20));
    // The bar names it while it is on.
    press(&live, &[KeyCode::Char('f')]);
    assert!(live.screen(100, 30).contains("POINTER"));
    // In a selection, Esc closes the selection, not the pointer: the
    // message names only `f`, and the pointer stays.
    press(
        &live,
        &[KeyCode::Char('f'), KeyCode::Char('V'), KeyCode::Char('f')],
    );
    let said = live.message().unwrap_or_default().0;
    assert!(said.starts_with("pointer on (f turns it off;"), "{said}");
    press(&live, &[KeyCode::Esc]);
    assert!(pointer(&live).is_some(), "Esc closed only the selection");
}

#[test]
fn the_dimming_is_the_one_a_spotlight_uses() {
    let live = Live::from_code(&code20(), "rs", "spot");
    // A spotlight on 3-4 at step 1 vs the pointer on 3-4: same colors.
    live.command("spot 3-4");
    let spot = live.with(|app| {
        let l = app.screen_layout_with_pointer(100);
        (0..l.rows.len())
            .map(|y| l.canvas.get(l.code_x, y).map(|c| c.style.fg))
            .collect::<Vec<_>>()
    });
    live.command("del a1");
    live.api_ok("pointer.set", json!({"from": 3, "to": 4}));
    let ptr = live.with(|app| {
        let l = app.screen_layout_with_pointer(100);
        (0..l.rows.len())
            .map(|y| l.canvas.get(l.code_x, y).map(|c| c.style.fg))
            .collect::<Vec<_>>()
    });
    assert_eq!(spot, ptr);
}

#[test]
fn with_a_selection_the_selection_stays_lit() {
    let live = Live::from_code(&code20(), "rs", "sel");
    press(&live, &j(14));
    press(
        &live,
        &[KeyCode::Char('V'), KeyCode::Char('j'), KeyCode::Char('j')],
    );
    press(&live, &[KeyCode::Char('f')]);
    assert_eq!(pointer(&live), Some((15, 17)));
    assert_eq!(lit(&live), all(15, 17));
    // The selection grows: the pointer goes with it.
    press(&live, &[KeyCode::Char('j')]);
    assert_eq!(lit(&live), all(15, 18));
    // Esc leaves the selection first (the pointer is on the cursor line
    // then), a second Esc turns it off.
    press(&live, &[KeyCode::Esc]);
    assert_eq!(pointer(&live), Some((18, 18)));
    press(&live, &[KeyCode::Esc]);
    assert_eq!(pointer(&live), None);
    // `v` on one line: that line.
    press(
        &live,
        &[KeyCode::Char('f'), KeyCode::Char('v'), KeyCode::Char('l')],
    );
    assert_eq!(pointer(&live), Some((18, 18)));
}

#[test]
fn the_pointer_wins_over_a_focus_and_a_spotlight() {
    let live = Live::from_code(&code20(), "rs", "wins");
    live.command("focus 1-2");
    live.command("spot 1-2");
    press(&live, &j(9));
    press(&live, &[KeyCode::Char('f')]);
    assert_eq!(lit(&live), all(10, 10));
    press(&live, &[KeyCode::Char('f')]);
    assert_eq!(
        lit(&live),
        all(1, 2),
        "off: the focus and the spotlight again"
    );
}

#[test]
fn follows_every_motion_and_the_mouse() {
    let live = Live::from_code(&code20(), "rs", "motions");
    press(&live, &[KeyCode::Char('f')]);
    press(&live, &[KeyCode::Char('G')]);
    assert_eq!(pointer(&live), Some((20, 20)));
    press(&live, &[KeyCode::Char('g')]);
    assert_eq!(pointer(&live), Some((1, 1)));
    press(&live, &[KeyCode::Down, KeyCode::Down, KeyCode::Up]);
    assert_eq!(pointer(&live), Some((2, 2)));
    // Word motions move the column: the pointer stays on the line.
    press(
        &live,
        &[KeyCode::Char('w'), KeyCode::Char('e'), KeyCode::Char('b')],
    );
    assert_eq!(pointer(&live), Some((2, 2)));
    press(&live, &[KeyCode::Char('J')]);
    let cur = live.with(|a| a.cursor_line);
    assert_eq!(pointer(&live), Some((cur, cur)));
    // The mouse (`--mouse`): a click on line 5 moves the cursor and the
    // pointer follows on the next frame.
    press(&live, &[KeyCode::Char('g')]);
    live.with(|a| a.mouse = true);
    live.screen(100, 30);
    live.with(|a| {
        let row = a.hit.rows.iter().position(|l| *l == Some(5));
        let y = row.expect("line 5 on screen") as u16 + a.hit.origin.1;
        let x = a.hit.code_x + 2;
        a.on_mouse(tiza_tui::app::MouseKind::Down, x, y);
        a.on_mouse(tiza_tui::app::MouseKind::Up, x, y);
    });
    live.screen(100, 30);
    assert_eq!(pointer(&live), Some((5, 5)));
}

#[test]
fn annotations_stay() {
    let code = "APP=prod\nPORT=8080\nKEY=demo-key-123\nLOG=info\n";
    let live = Live::from_code(code, "sh", "ann");
    live.command("note 1 the app name");
    live.command("step 1");
    for present in [false, true] {
        live.with(move |a| a.set_present(present));
        press(
            &live,
            &[KeyCode::Char('g'), KeyCode::Char('j'), KeyCode::Char('j')],
        );
        press(&live, &[KeyCode::Char('f')]);
        assert_eq!(pointer(&live), Some((3, 3)));
        let screen = live.screen(100, 30);
        assert!(screen.contains("the app name"), "the note stays:\n{screen}");
        // The note is not dimmed: its text keeps its color.
        let note_dim = live.with(|a| {
            let l = a.screen_layout_with_pointer(100);
            let txt = l.canvas.to_plain_text();
            let (y, row) = txt
                .lines()
                .enumerate()
                .find(|(_, r)| r.contains("the app name"))
                .unwrap();
            let x = row[..row.find("the app name").unwrap()].chars().count();
            l.canvas.get(x, y).unwrap().style.fg == Some(a.theme.dimmed)
        });
        assert!(!note_dim);
        press(&live, &[KeyCode::Esc]);
        // Esc in presentation mode: the pointer first, then the mode.
        assert_eq!(live.with(|a| a.present), present);
        live.with(|a| a.set_present(false));
    }
}

#[test]
fn presentation_mode_shows_only_the_pointer() {
    let live = Live::from_code(&code20(), "rs", "present");
    press(&live, &j(4));
    press(&live, &[KeyCode::Char('P'), KeyCode::Char('f')]);
    assert_eq!(pointer(&live), Some((5, 5)), "on the last cursor line");
    press(&live, &[KeyCode::Char('j')]);
    assert_eq!(pointer(&live), Some((6, 6)));
    press(&live, &[KeyCode::Char('k'), KeyCode::Char('k')]);
    assert_eq!(pointer(&live), Some((4, 4)));
    assert_eq!(lit(&live), all(4, 4));
    let (symbols, styles) = live.snapshot(100, 30);
    for word in ["POINTER", "NORMAL", "pointer on", "step "] {
        assert!(!symbols.contains(word), "«{word}» on screen:\n{symbols}");
    }
    assert!(!styles.contains("+r"), "no cursor cell:\n{styles}");
    // No selection tint on the pointed line: only the dimming changes.
    let plain = live.with(|a| a.screen_layout_with_pointer(98).canvas.to_plain_text());
    let off = live.with(|a| a.layout(98).canvas.to_plain_text());
    assert_eq!(plain, off, "the same text, only dimmed differently");
    press(&live, &[KeyCode::Esc]);
    assert_eq!(pointer(&live), None);
    assert!(live.with(|a| a.present), "the first Esc only turns it off");
}

#[test]
fn never_part_of_the_document() {
    let dir = scratch("doc");
    let src = dir.join("a.rs");
    std::fs::write(&src, code20()).unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let save = dir.join("a.rs.tiza");
    let live = Live::open(doc, save.clone(), dir.join("live.sock"));
    live.command("note 2 hello");
    live.command("w");
    let saved = std::fs::read(&save).unwrap();
    let before = live.with(|a| {
        (
            a.revision,
            a.dirty,
            a.doc.annotations.len(),
            a.history.can_undo(),
            a.history.can_redo(),
        )
    });
    let mtime = std::fs::metadata(&save).unwrap().modified().unwrap();
    press(&live, &[KeyCode::Char('f')]);
    press(&live, &j(3));
    live.api_ok("pointer.set", json!({"line": 9}));
    live.api_ok("pointer.clear", json!({}));
    press(&live, &[KeyCode::Char('f')]);
    let after = live.with(|a| {
        (
            a.revision,
            a.dirty,
            a.doc.annotations.len(),
            a.history.can_undo(),
            a.history.can_redo(),
        )
    });
    assert_eq!(
        before, after,
        "no revision, no unsaved mark, no annotation, no undo entry"
    );
    assert_eq!(std::fs::metadata(&save).unwrap().modified().unwrap(), mtime);
    assert_eq!(std::fs::read(&save).unwrap(), saved, "autosave never ran");
    // `u` undoes the note, not the pointer.
    press(&live, &[KeyCode::Char('u')]);
    assert!(pointer(&live).is_some(), "u does not undo the pointer");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
    live.command("w");
    let text = std::fs::read_to_string(&save).unwrap();
    assert!(!text.contains("pointer"), "{text}");
}

/// What every export path makes of the live document now.
fn exports(live: &Live, dir: &std::path::Path, tag: &str) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for ext in ["txt", "ansi", "md", "svg", "png"] {
        let p = dir.join(format!("{tag}.{ext}"));
        live.command(&format!("export {} step=1", p.display()));
        let (m, err) = live.message().unwrap();
        assert!(!err, "{ext}: {m}");
        out.push((format!(":export {ext}"), std::fs::read(&p).unwrap()));
    }
    let sock = live.socket.to_str().unwrap().to_string();
    for ext in ["png", "svg", "txt", "gif", "cast", "tape"] {
        let p = dir.join(format!("{tag}-render.{ext}"));
        let o = tiza(&["--socket", &sock, "render", "--out", p.to_str().unwrap()]);
        assert!(
            o.status.success(),
            "render {ext}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        let mut bytes = std::fs::read(&p).unwrap();
        if ext == "cast" {
            // The asciicast header carries the wall-clock time: compare the
            // frames, not the header.
            let nl = bytes.iter().position(|b| *b == b'\n').unwrap();
            bytes.drain(..=nl);
        }
        if ext == "tape" {
            // The VHS tape names its own path and its output.
            bytes = String::from_utf8(bytes)
                .unwrap()
                .replace(&format!("{tag}-render"), "X-render")
                .into_bytes();
        }
        out.push((format!("tiza render .{ext}"), bytes));
    }
    let text = live.api_ok("render.text", json!({"width": 100}));
    out.push(("render.text".into(), text.to_string().into_bytes()));
    out
}

#[test]
fn no_export_draws_it() {
    let dir = scratch("export");
    let live = Live::from_code(&code20(), "rs", "export");
    live.with(|a| {
        a.image_exporter = Some(Box::new(|canvas, theme, format, title| {
            let svg = tiza_export::to_svg(
                canvas,
                theme,
                &tiza_export::SvgOptions {
                    embed_fonts: format == tiza_tui::app::ImageFormat::Svg,
                    title: title.map(str::to_string),
                    ..Default::default()
                },
            );
            if format == tiza_tui::app::ImageFormat::Svg {
                Ok(svg.into_bytes())
            } else {
                tiza_export::to_png(&svg, &tiza_export::PngOptions::default())
                    .map_err(|e| e.to_string())
            }
        }));
    });
    live.command("note 3 hello");
    live.command("step 1");
    let off = exports(&live, &dir, "off");
    press(&live, &j(6));
    press(&live, &[KeyCode::Char('f')]);
    assert_eq!(lit(&live), all(7, 7), "the pointer is on screen");
    let on = exports(&live, &dir, "on");
    for ((what, a), (_, b)) in off.iter().zip(&on) {
        assert!(a == b, "{what} changed with the pointer on");
    }
    // An agent's pointer: the same.
    live.api_ok("pointer.set", json!({"from": 2, "to": 4}));
    let agent = exports(&live, &dir, "agent");
    for ((what, a), (_, b)) in off.iter().zip(&agent) {
        assert!(a == b, "{what} changed with an agent's pointer");
    }
    // MP4/WebM go through the same frames as the GIF (`tiza render`
    // pipes them to ffmpeg), so the GIF above covers them.
}

#[test]
fn agents_point_over_the_socket_and_status_reports_it() {
    let live = Live::from_code(&code20(), "rs", "socket");
    assert_eq!(live.api_ok("status", json!({}))["pointer"], Value::Null);
    let r = live.api_ok("pointer.set", json!({"line": 3}));
    assert_eq!(r["pointer"], json!({"line": 3, "to": 3, "by": "agent"}));
    assert_eq!(r["shown"], json!(true));
    assert_eq!(lit(&live), all(3, 3));
    assert_eq!(
        live.api_ok("status", json!({}))["pointer"],
        json!({"line": 3, "to": 3, "by": "agent"})
    );
    // `{line, find}` and `{find}` alone.
    let r = live.api_ok("pointer.set", json!({"line": 12, "find": "v12"}));
    assert_eq!(r["pointer"]["line"], json!(12));
    let r = live.api_ok("pointer.set", json!({"find": "= 15;"}));
    assert_eq!(r["pointer"]["line"], json!(15));
    // A range.
    let r = live.api_ok("pointer.set", json!({"from": 9, "to": 6}));
    assert_eq!(r["pointer"], json!({"line": 6, "to": 9, "by": "agent"}));
    assert_eq!(lit(&live), all(6, 9));
    // Errors.
    let code = |p: Value| live.api("pointer.set", p).error.unwrap().code;
    assert_eq!(code(json!({"line": 99})), "out_of_range");
    assert_eq!(code(json!({"line": 3, "find": "v12"})), "text_not_found");
    assert_eq!(code(json!({"find": "nowhere"})), "text_not_found");
    assert_eq!(
        code(json!({"find": "let"})),
        "invalid_argument",
        "on many lines"
    );
    assert_eq!(code(json!({})), "invalid_argument");
    assert_eq!(code(json!({"line": 2, "from": 3})), "invalid_argument");
    assert_eq!(code(json!({"to": 3})), "invalid_argument");
    assert_eq!(
        code(json!({"lines": 3})),
        "invalid_argument",
        "unknown field"
    );
    // The last one wins: the presenter's motion takes it back, and the
    // agent's pointer moves the cursor (j goes on from there).
    live.api_ok("pointer.set", json!({"line": 10}));
    press(&live, &[KeyCode::Char('j')]);
    assert_eq!(
        live.api_ok("status", json!({}))["pointer"],
        json!({"line": 11, "to": 11, "by": "presenter"})
    );
    press(&live, &[KeyCode::Char('f')]);
    assert_eq!(live.api_ok("status", json!({}))["pointer"], Value::Null);
    live.api_ok("pointer.set", json!({"line": 4}));
    let r = live.api_ok("pointer.clear", json!({}));
    assert_eq!(r["pointer"], Value::Null);
    assert_eq!(pointer(&live), None);
    // `f` after an agent cleared it: on again, on the cursor line (4).
    press(&live, &[KeyCode::Char('f')]);
    assert_eq!(pointer(&live), Some((4, 4)));
    // `tiza api` reaches it too.
    let sock = live.socket.to_str().unwrap().to_string();
    let out = tiza_ok(&["--socket", &sock, "api", "pointer.set", r#"{"line": 2}"#]);
    assert!(
        out.contains("\"line\": 2") || out.contains("\"line\":2"),
        "{out}"
    );
}

#[test]
fn mcp_point_at_and_clear_pointer() {
    let live = Live::from_code(&code20(), "rs", "mcp");
    let mut mcp = McpClient::start(&live.socket);
    let names = mcp.tool_names();
    assert!(names.contains(&"point_at".to_string()), "{names:?}");
    assert!(names.contains(&"clear_pointer".to_string()), "{names:?}");
    let (err, text, _) = mcp.call("point_at", json!({"line": 5}));
    assert!(!err, "{text}");
    assert!(text.contains("\"by\": \"agent\""), "{text}");
    assert_eq!(pointer(&live), Some((5, 5)));
    let (err, text, _) = mcp.call("point_at", json!({"find": "v8 ="}));
    assert!(!err, "{text}");
    assert_eq!(pointer(&live), Some((8, 8)));
    let (err, text, _) = mcp.call("point_at", json!({"from": 2, "to": 3}));
    assert!(!err, "{text}");
    assert_eq!(pointer(&live), Some((2, 3)));
    let (err, text, _) = mcp.call("status", json!({}));
    assert!(!err && text.contains("\"pointer\""), "{text}");
    // render_text does not draw it: no dimming in plain text anyway, and
    // the same text as with the pointer off.
    let (_, with, _) = mcp.call("render_text", json!({"width": 100}));
    let (err, text, _) = mcp.call("clear_pointer", json!({}));
    assert!(!err, "{text}");
    assert_eq!(pointer(&live), None);
    let (_, without, _) = mcp.call("render_text", json!({"width": 100}));
    assert_eq!(with, without);
    let (err, text, _) = mcp.call("point_at", json!({"line": 999}));
    assert!(err && text.contains("out_of_range"), "{text}");
}

#[test]
fn the_pointed_line_off_screen_scrolls_into_view() {
    let code: String = (1..=80).map(|i| format!("let v{i} = {i};\n")).collect();
    let live = Live::from_code(&code, "rs", "scroll");
    live.screen(100, 20);
    live.api_ok("pointer.set", json!({"line": 70}));
    let screen = live.screen(100, 20);
    assert!(screen.contains("let v70 = 70;"), "{screen}");
    assert!(!screen.contains("let v1 = 1;"), "{screen}");
}

#[test]
fn a_step_change_keeps_the_line_or_turns_it_off() {
    let code = "fn a() {\n    let x = 1;\n    let y = 2;\n    let z = 3;\n}\n";
    let live = Live::from_code(code, "rs", "steps");
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "insert", "after": "let x", "content": "    let w = 0;\n    let v = 0;"}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "delete", "find": "let z"}),
    );
    live.command("note 1 one");
    live.command("step 1");
    // Step 1: `let z` is line 4; the insert at step 2 shifts it to 6.
    live.api_ok("pointer.set", json!({"find": "let z"}));
    assert_eq!(pointer(&live), Some((4, 4)));
    press(&live, &[KeyCode::Char('n')]);
    live.with(|a| a.finish_typing());
    assert_eq!(live.with(|a| a.doc.step), 2);
    assert_eq!(pointer(&live), Some((6, 6)), "carried by the insert");
    assert_eq!(live.with(|a| a.cursor_line), 6, "the cursor goes with it");
    assert!(
        live.with(|a| a.doc.line_text(6).unwrap_or("").contains("let z")),
        "still the same text"
    );
    press(&live, &[KeyCode::Char('p')]);
    assert_eq!(pointer(&live), Some((4, 4)), "and back");
    // A line no move touches stays where it is.
    live.api_ok("pointer.set", json!({"line": 1}));
    press(&live, &[KeyCode::Char('n')]);
    live.with(|a| a.finish_typing());
    assert_eq!(pointer(&live), Some((1, 1)));
    // Step 3 deletes `let z`: the pointer on it turns off.
    live.api_ok("pointer.set", json!({"find": "let z"}));
    press(&live, &[KeyCode::Char('n')]);
    live.with(|a| a.finish_typing());
    assert_eq!(live.with(|a| a.doc.step), 3);
    assert_eq!(pointer(&live), None, "its line disappeared");
    // Without moves: the same line across steps.
    let live = Live::from_code(&code20(), "rs", "steps2");
    live.command("note 1 one");
    live.command("note 2 two");
    live.command("set a2 step=2");
    live.command("step 1");
    live.api_ok("pointer.set", json!({"line": 9}));
    press(&live, &[KeyCode::Char('n')]);
    assert_eq!(pointer(&live), Some((9, 9)));
}

#[test]
fn the_command_line_never_toggles_it_and_hides_an_agents_pointer() {
    let live = Live::from_code(&code20(), "rs", "cmd");
    press(&live, &[KeyCode::Char(':')]);
    live.type_text("ff");
    assert_eq!(pointer(&live), None, "f typed into the command line");
    // An agent points while the command line is open: shown after it closes.
    let r = live.api_ok("pointer.set", json!({"line": 5}));
    assert_eq!(r["shown"], json!(false));
    assert_eq!(lit(&live), all(1, 20), "not drawn while typing");
    press(&live, &[KeyCode::Esc]);
    assert_eq!(lit(&live), all(5, 5), "drawn once it closes");
    assert!(live.with(|a| matches!(a.mode, tiza_tui::app::Mode::Normal)));
}

/// A document with a spotlight on L1-2 at step 2 and, outside it, a mark
/// on L3, a highlight on L6, a box over L9-11, an arrow from L5 to L10 and
/// a note on L12, each with a label.
fn spotlight_doc(name: &str) -> Live {
    let dir = tiza_qa::scratch(name);
    let path = tiza_qa::fixture("tui/live_pointer/spotlight.tiza");
    let doc = fs::load_document(&path).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("p.sock"));
    live.with(|a| a.autosave = false);
    live.command("step 2");
    live
}

/// The labels of the document drawn in the dimmed color on the
/// presenter's own frame.
fn dimmed_labels(live: &Live) -> BTreeSet<&'static str> {
    live.with(|app| {
        let l = app.screen_layout_with_pointer(120);
        let txt = l.canvas.to_plain_text();
        ["s moves", "the length", "the callee", "goes in", "far away"]
            .into_iter()
            .filter(|label| {
                let (y, row) = txt
                    .lines()
                    .enumerate()
                    .find(|(_, r)| r.contains(label))
                    .unwrap_or_else(|| panic!("«{label}» is drawn:\n{txt}"));
                let x = row[..row.find(label).unwrap()].chars().count();
                l.canvas.get(x, y).unwrap().style.fg == Some(app.theme.dimmed)
            })
            .collect()
    })
}

fn set(labels: &[&'static str]) -> BTreeSet<&'static str> {
    labels.iter().copied().collect()
}

#[test]
fn the_pointer_lights_the_annotations_that_touch_its_lines() {
    let live = spotlight_doc("ptr-spot");
    let every = set(&["s moves", "the length", "the callee", "goes in", "far away"]);
    assert_eq!(
        dimmed_labels(&live),
        every,
        "no pointer: the spotlight dims all"
    );
    for present in [false, true] {
        live.with(move |a| a.set_present(present));
        // On the line of the mark: the mark and its label, only. In
        // presentation mode the keys move the pointer once it is on.
        press(&live, &[KeyCode::Char('f')]);
        press(
            &live,
            &[KeyCode::Char('g'), KeyCode::Char('j'), KeyCode::Char('j')],
        );
        assert_eq!(pointer(&live), Some((3, 3)));
        let mut lit = every.clone();
        lit.remove("s moves");
        assert_eq!(
            dimmed_labels(&live),
            lit,
            "pointer on L3 (present: {present})"
        );
        // On L6: the highlight of L6 and the arrow from L5 to L10, which
        // goes through L6.
        press(&live, &j(3));
        assert_eq!(pointer(&live), Some((6, 6)));
        assert_eq!(
            dimmed_labels(&live),
            set(&["s moves", "the callee", "far away"]),
            "pointer on L6 (present: {present})"
        );
        // On L10: the box over L9-11 and the arrow from L5 to L10, whole.
        press(&live, &j(4));
        assert_eq!(pointer(&live), Some((10, 10)));
        assert_eq!(
            dimmed_labels(&live),
            set(&["s moves", "the length", "far away"]),
            "pointer on L10 (present: {present})"
        );
        // Inside the spotlight: as without the pointer.
        press(&live, &[KeyCode::Char('g')]);
        assert_eq!(
            dimmed_labels(&live),
            every,
            "pointer on L1 (present: {present})"
        );
        press(&live, &[KeyCode::Esc]);
        assert_eq!(pointer(&live), None);
        assert_eq!(
            dimmed_labels(&live),
            every,
            "pointer off (present: {present})"
        );
        live.with(|a| a.set_present(false));
    }
    // A group pointer (`V` + `f`) over L11-12: the box and the note.
    press(
        &live,
        &[
            KeyCode::Char('G'),
            KeyCode::Char('V'),
            KeyCode::Char('k'),
            KeyCode::Char('f'),
        ],
    );
    assert_eq!(pointer(&live), Some((11, 12)));
    assert_eq!(
        dimmed_labels(&live),
        set(&["s moves", "the length", "goes in"]),
        "group pointer on L11-12"
    );
}
