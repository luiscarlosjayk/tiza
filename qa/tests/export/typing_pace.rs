//! A GIF or WebP of a typed `code.patch` lasts as long as the typing on
//! the screen: the units × `speed_ms`. Before, each unit was a frame of
//! 20 ms or more (the shortest frame of a GIF), rounded up to 10 ms: at
//! `speed_ms: 1`, 854 characters took 17 s, not 0.85 s, and at 25 ms (the
//! default of `char`) the typing was 20 % slow. Now a frame shows every
//! unit typed in its time. The MP4 and WebM use the times of the GIF
//! (`video_last_step`).
//!
//! Each document has two steps: step 0, and step 1 that types the move.
//! With `--skip-zero --interval 1`, the take is the typed frames and then
//! step 1, which holds 2 s (the last step holds twice the interval).

use std::path::{Path, PathBuf};

use serde_json::json;
use tiza_qa::{Live, McpClient, scratch, tiza_ok};

/// What step 1 holds after the typing, in ms.
const LAST_STEP_MS: u64 = 2000;

/// A document whose step 1 types the first `chars` characters of 25 lines
/// of code (39 characters each, with its line break) with `typing` at
/// `speed_ms`, written to the scratch folder `dir` as `name`, and read
/// back as the CLI reads it.
fn typed(
    dir: &Path,
    name: &str,
    typing: &str,
    speed_ms: u64,
    chars: usize,
) -> (PathBuf, tiza_core::Document) {
    let content: String = (0..25)
        .map(|i| format!("\n    let value_{i:03} = compute_{i:03}(x, y);"))
        .collect();
    let doc = json!({
        "schema": 1,
        "source": {"language": "rs", "first_line": 1},
        "code": ["fn main() {", "}"],
        "step": 0,
        "moves": [{
            "at_step": 1, "action": "code.patch", "op": "insert", "after": "fn main() {",
            "content": &content[..chars], "typing": typing, "speed_ms": speed_ms
        }]
    });
    let path = dir.join(name);
    std::fs::write(&path, doc.to_string()).unwrap();
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    (path, doc)
}

/// How long the typing of step 1 lasts on the screen, in ms.
fn on_screen(doc: &tiza_core::Document) -> u64 {
    doc.typing_ms_at(1)
}

/// The time of each frame of a GIF, in ms.
fn gif_times(bytes: &[u8]) -> Vec<u64> {
    let mut d = gif::DecodeOptions::new()
        .read_info(std::io::Cursor::new(bytes))
        .unwrap();
    let mut times = Vec::new();
    while let Some(f) = d.read_next_frame().unwrap() {
        times.push(u64::from(f.delay) * 10);
    }
    times
}

/// The length of one loop of a GIF, in ms.
fn gif_ms(bytes: &[u8]) -> u64 {
    gif_times(bytes).iter().sum()
}

/// The typing of an export lasts as long as on the screen: up to 20 ms
/// more, since a frame ends on the 10 ms unit of a GIF and lasts 20 ms at
/// least.
fn assert_pace(what: &str, export_ms: u64, screen_ms: u64) {
    let typing = export_ms - LAST_STEP_MS;
    assert!(
        typing >= screen_ms && typing <= screen_ms + 20,
        "{what}: the typing lasts {typing} ms, and {screen_ms} ms on the screen"
    );
}

/// Exports `path` to `out` with the flags of the header.
fn export(path: &Path, out: &Path) -> Vec<u8> {
    tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--skip-zero",
        "--interval",
        "1",
        "--scale",
        "1",
    ]);
    std::fs::read(out).unwrap()
}

#[test]
fn a_gif_types_as_long_as_the_screen() {
    let dir = scratch("typing-pace-gif");
    // The bug shows best at 1 ms, with 854 characters. The slower speeds
    // type fewer, so that their GIF has fewer frames to encode.
    for (typing, speed, chars) in [
        ("char", 1, 854),
        ("char", 7, 200),
        ("char", 25, 120),
        ("line", 45, 854),
    ] {
        let name = format!("{typing}-{speed}.tiza");
        let (path, doc) = typed(&dir, &name, typing, speed, chars);
        let gif = export(&path, &dir.join(format!("{typing}-{speed}.gif")));
        let what = format!("GIF, {typing} at {speed} ms");
        assert_pace(&what, gif_ms(&gif), on_screen(&doc));
        // The typing is in frames, each until the next unit shows: not one
        // frame of the whole time.
        let times = gif_times(&gif);
        let longest = speed.max(20).div_ceil(10) * 10 + 10;
        let typed = &times[..times.len() - 1];
        assert!(
            typed.len() >= 20 && typed.iter().all(|t| *t <= longest),
            "{what}: typed frames of {typed:?} ms, each {longest} ms at most"
        );
    }
}

#[test]
fn a_webp_types_as_long_as_the_screen() {
    let dir = scratch("typing-pace-webp");
    let (path, doc) = typed(&dir, "char-1.tiza", "char", 1, 854);
    let webp = export(&path, &dir.join("char-1.webp"));
    let d = image_webp::WebPDecoder::new(std::io::Cursor::new(&webp[..])).unwrap();
    assert_pace("WebP, char at 1 ms", d.loop_duration(), on_screen(&doc));
}

#[test]
fn an_export_over_mcp_types_as_long_as_the_screen() {
    let dir = scratch("typing-pace-mcp");
    let (_, doc) = typed(&dir, "char-1.tiza", "char", 1, 854);
    let screen = on_screen(&doc);
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let mut mcp = McpClient::start(&live.socket);
    let path = dir.join("mcp.gif");
    let (err, text, _) = mcp.call(
        "export_image",
        json!({"path": path.to_str().unwrap(), "interval": 1, "skip_zero": true, "scale": 1}),
    );
    assert!(!err, "{text}");
    assert_pace(
        "GIF over MCP, char at 1 ms",
        gif_ms(&std::fs::read(&path).unwrap()),
        screen,
    );
}
