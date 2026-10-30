//! Without a width, an export of the live instance draws at the width of
//! the code area of its screen, as `render.text` does: the terminal width
//! less the side padding of the document, from 20 to 400, or 100 while no
//! terminal size is known. `status` gives that width in `width`. Before,
//! `tiza render`, `:export` and MCP `export_image` took the terminal width
//! less 2, so with a padding other than the default they drew wider than
//! the screen.
//!
//! Each case compares the text of the four: `render.text`, `tiza render
//! --format txt` of the instance, `:export` to a `.txt`, and `export_image`
//! to a `.txt`.

use serde_json::json;
use tiza_qa::{Live, McpClient, tiza};

/// A long line: each width cuts it at another place, so the text of a take
/// tells its width.
fn code() -> String {
    format!(
        "fn main() {{\n    let a = 1;\n    let v = \"{}\";\n}}\n",
        "x".repeat(450)
    )
}

/// A live instance on a terminal `cols` × 30 (`(0, 0)`: no size yet),
/// with the `:` commands of `setup` (`pad 1 6`, `numbers off`…).
fn live(name: &str, size: (u16, u16), setup: &[&str]) -> Live {
    let live = Live::from_code(&code(), "rs", name);
    for c in setup {
        live.command(c);
    }
    live.with(move |a| a.size = size);
    live
}

/// The text of the four exports with no width, and the `width` of `status`.
/// Each one must be the take at that width.
fn exports(live: &Live, name: &str) -> (usize, [String; 4]) {
    let width = live.api_ok("status", json!({}))["width"]
        .as_u64()
        .expect("width") as usize;
    let text = live.api_ok("render.text", json!({}));
    assert_eq!(text["width"], width, "{text}");
    let sock = live.socket.to_str().unwrap();
    let render = tiza(&["--socket", sock, "render", "--format", "txt", "--out", "-"]);
    assert!(
        render.status.success(),
        "{}",
        String::from_utf8_lossy(&render.stderr)
    );
    let dir = live.socket.parent().unwrap();
    let exported = dir.join(format!("{name}-export.txt"));
    live.command(&format!("export {}", exported.display()));
    let image = dir.join(format!("{name}-image.txt"));
    let mut mcp = McpClient::start(&live.socket);
    let (err, said, _) = mcp.call("export_image", json!({"path": image.to_str().unwrap()}));
    assert!(!err, "{said}");
    let read = |p: &std::path::Path| {
        std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    };
    (
        width,
        [
            text["text"].as_str().unwrap().to_string(),
            String::from_utf8(render.stdout).unwrap(),
            read(&exported),
            read(&image),
        ],
    )
}

/// The four exports of `live` are the take at `want` columns.
fn assert_all_at(live: &Live, name: &str, want: usize) {
    let (width, texts) = exports(live, name);
    assert_eq!(width, want, "the width of status");
    // The take at that width, given: what each export must be.
    let given = live.api_ok("render.text", json!({"width": want}));
    let take = given["text"].as_str().unwrap();
    let names = ["render.text", "tiza render", ":export", "export_image"];
    for (n, t) in names.iter().zip(&texts) {
        assert_eq!(
            t.trim_end(),
            take.trim_end(),
            "{n} is not the take at {want} columns"
        );
    }
}

#[test]
fn the_side_padding_of_the_document_narrows_every_export() {
    // 120 columns, 6 on each side: 108.
    let l = live("live-width-pad", (120, 30), &["pad 1 6"]);
    assert_all_at(&l, "pad", 108);
}

#[test]
fn the_default_padding_and_no_padding() {
    // The default side padding is 1: 118.
    let l = live("live-width-default", (120, 30), &[]);
    assert_all_at(&l, "default", 118);
    let l = live("live-width-zero", (120, 30), &["pad 0 0"]);
    assert_all_at(&l, "zero", 120);
}

#[test]
fn a_padding_that_does_not_fit_keeps_the_code_as_the_screen_does() {
    // 80 columns: the code keeps 20 of them, so each side is 30, not 50.
    let l = live("live-width-wide-pad", (80, 30), &["pad 1 50"]);
    assert_all_at(&l, "wide-pad", 20);
}

#[test]
fn no_terminal_size_yet_and_a_very_wide_one() {
    let l = live("live-width-no-size", (0, 0), &["pad 1 6"]);
    assert_all_at(&l, "no-size", 100);
    let l = live("live-width-wide", (500, 30), &[]);
    assert_all_at(&l, "wide", 400);
}

#[test]
fn presentation_mode_numbers_and_margin_keep_the_width() {
    let l = live(
        "live-width-modes",
        (120, 30),
        &["pad 1 6", "numbers off", "margin 30"],
    );
    l.with(|a| a.set_present(true));
    assert_all_at(&l, "modes", 108);
}

#[test]
fn a_width_that_is_given_wins() {
    let l = live("live-width-given", (120, 30), &["pad 1 6"]);
    let sock = l.socket.to_str().unwrap();
    let render = tiza(&[
        "--socket", sock, "render", "--format", "txt", "--width", "60", "--out", "-",
    ]);
    let given = l.api_ok("render.text", json!({"width": 60}));
    assert_eq!(
        String::from_utf8(render.stdout).unwrap().trim_end(),
        given["text"].as_str().unwrap().trim_end()
    );
}

#[test]
fn a_terminal_narrower_than_20_columns_exports_at_20() {
    let l = live("live-width-narrow", (15, 30), &[]);
    assert_all_at(&l, "narrow", 20);
}

#[test]
fn the_padding_of_the_command_line_and_of_the_config_file() {
    // `--padding 1,8`: 120 less 8 on each side.
    let l = live("live-width-flag", (120, 30), &[]);
    l.with(|a| a.padding_override = Some((1, 8)));
    assert_all_at(&l, "flag", 104);
    // `pad = "1 5"` in config.toml.
    let l = live("live-width-config", (120, 30), &[]);
    l.with(|a| a.config_defaults.pad = Some((1, 5)));
    assert_all_at(&l, "config", 110);
}

#[test]
fn an_instance_with_no_width_in_its_status_gives_the_old_width() {
    // An instance older than `width`: the export takes the columns less 2,
    // as before.
    let l = live("live-width-old", (120, 30), &["pad 1 6"]);
    let mut v = l.api_ok("status", json!({}));
    v.as_object_mut().unwrap().remove("width");
    let status: tiza_core::protocol::Status = serde_json::from_value(v).unwrap();
    assert_eq!(status.width, 0);
    assert_eq!(status.take_width(), 118);
}

#[test]
fn a_tape_of_the_instance_takes_its_terminal_width() {
    // The TUI in the recording takes its own padding off: the window is
    // the terminal of the instance, 120 columns, not the take of 108.
    let l = live("live-width-tape", (120, 30), &["pad 1 6"]);
    let sock = l.socket.to_str().unwrap();
    let dir = l.socket.parent().unwrap();
    // The `Set Width` line of a tape written with `extra`.
    let tape = |name: &str, extra: &[&str]| {
        let out = dir.join(name);
        let mut args = vec!["--socket", sock, "render", "--out", out.to_str().unwrap()];
        args.extend_from_slice(extra);
        let run = tiza(&args);
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let text = std::fs::read_to_string(&out).unwrap();
        text.lines()
            .find(|l| l.starts_with("Set Width "))
            .unwrap_or_else(|| panic!("{text}"))
            .to_string()
    };
    let default = tape("default.tape", &[]);
    assert_eq!(default, tape("given.tape", &["--width", "120"]));
    assert_ne!(default, tape("take.tape", &["--width", "108"]));
}
