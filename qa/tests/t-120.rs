//! t-120 — Native GIF export from Tiza's own frames; MP4 only through an
//! installed ffmpeg; the `.cast` stays.
//!
//! Acceptance criteria:
//! - AC1 `tiza render FILE --out demo.gif` writes an animated GIF: one frame
//!   per step from 0 (or `--skip-zero`), `--interval` seconds each, the
//!   typed frames of `code.patch` moves at their `speed_ms`, a final hold of
//!   2×interval, looping (`--loop N`), one palette per animation, sub-
//!   rectangle diffs; pure Rust.
//! - AC2 Every frame shares the take's geometry (t-111).
//! - AC3 `--out demo.mp4`: frames to a temp dir, `ffmpeg` from PATH or
//!   `$TIZA_FFMPEG` with a printed command; without ffmpeg a clear message,
//!   exit 2, no partial file.
//! - AC4 `.cast` and `.tape` unchanged.
//! - AC5 The live instance can be recorded (`tiza render --out demo.gif`
//!   without FILE) and MCP `export_image` accepts `.gif`.
//! - AC6 Frame count, durations, loop flag, size budget, stable x across
//!   frames, ffmpeg-missing message.

use serde_json::json;
use tiza_qa::{Live, McpClient, example, fixture, scratch, tiza, tiza_ok};

struct Gif {
    width: u16,
    height: u16,
    /// (left, top, width, height, delay in cs) per frame.
    frames: Vec<(u16, u16, u16, u16, u16)>,
    looping: bool,
}

fn decode(bytes: &[u8]) -> Gif {
    assert_eq!(&bytes[..6], b"GIF89a");
    let mut o = gif::DecodeOptions::new();
    o.set_color_output(gif::ColorOutput::Indexed);
    let mut d = o.read_info(std::io::Cursor::new(bytes)).unwrap();
    let (width, height) = (d.width(), d.height());
    let mut frames = Vec::new();
    while let Some(f) = d.read_next_frame().unwrap() {
        frames.push((f.left, f.top, f.width, f.height, f.delay));
    }
    Gif {
        width,
        height,
        frames,
        looping: bytes.windows(11).any(|w| w == b"NETSCAPE2.0"),
    }
}

#[test]
fn ac1_ac2_ac6_the_example_becomes_a_small_looping_gif_with_one_geometry() {
    let dir = scratch("t120-own");
    let out = dir.join("own.gif");
    let ex = fixture("ownership/ownership.rs.tiza");
    let err = tiza_ok(&[
        "render",
        ex.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    // tiza_ok returns stdout; the summary goes to stderr, checked below.
    let _ = err;
    let bytes = std::fs::read(&out).unwrap();
    let g = decode(&bytes);
    // Steps 0..=7, 2 s each, the last 4 s; loops forever; < 1 MB.
    assert_eq!(g.frames.len(), 8, "{:?}", g.frames);
    let delays: Vec<u16> = g.frames.iter().map(|f| f.4).collect();
    assert_eq!(delays, vec![200, 200, 200, 200, 200, 200, 200, 400]);
    assert!(g.looping);
    // Scale 2 by default (t-121 AC0, Luis's decision on the t-120 review):
    // the 100-column example is ~2048 px wide and still well under 1 MB
    // (~400 kB); `--scale 1` gives exactly half the pixels per side.
    assert!(bytes.len() < 1_000_000, "{} bytes", bytes.len());
    assert!(g.width >= 2000, "scale 2 by default: {} px wide", g.width);
    let one = dir.join("own-1x.gif");
    tiza_ok(&[
        "render",
        ex.to_str().unwrap(),
        "--out",
        one.to_str().unwrap(),
        "--scale",
        "1",
    ]);
    let g1 = decode(&std::fs::read(&one).unwrap());
    assert_eq!(
        (
            g1.width.abs_diff(g.width / 2) <= 1,
            g1.height.abs_diff(g.height / 2) <= 1
        ),
        (true, true),
        "--scale 1 {}×{} vs default {}×{}",
        g1.width,
        g1.height,
        g.width,
        g.height
    );
    // The first frame is whole; later ones are sub-rectangles inside it.
    assert_eq!(g.frames[0], (0, 0, g.width, g.height, 200));
    assert!(
        g.frames[1..]
            .iter()
            .any(|f| f.2 < g.width || f.3 < g.height)
    );
    assert!(
        g.frames
            .iter()
            .all(|f| f.0 + f.2 <= g.width && f.1 + f.3 <= g.height)
    );
    // Stable geometry: the same PNG path at every step puts the code at
    // one x (t-111), and the GIF is those pixels; the frame size is the
    // tallest step's and never changes.
    let out2 = tiza(&[
        "render",
        ex.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--loop",
        "3",
        "--interval",
        "1",
        "--skip-zero",
    ]);
    let summary = String::from_utf8_lossy(&out2.stderr);
    assert!(
        summary.contains("7 frames") && summary.contains("plays 3×"),
        "{summary}"
    );
    let g = decode(&std::fs::read(&out).unwrap());
    assert_eq!(g.frames.len(), 7);
    assert_eq!(
        g.frames.iter().map(|f| f.4).collect::<Vec<_>>(),
        vec![100; 6].into_iter().chain([200]).collect::<Vec<_>>()
    );
    assert!(
        g.looping,
        "a finite loop count still writes the NETSCAPE block"
    );
}

#[test]
fn ac1_typed_moves_animate_at_their_speed() {
    let dir = scratch("t120-moves");
    let out = dir.join("moves.gif");
    let ex = example("ownership-moves.rs.tiza");
    tiza_ok(&[
        "render",
        ex.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    let g = decode(&std::fs::read(&out).unwrap());
    let doc = tiza_core::fs::open(ex.to_str().unwrap()).unwrap().document;
    let steps = doc.max_step() as usize + 1;
    let typed: usize = (0..=doc.max_step())
        .filter_map(|s| tiza_core::moves::typing_plan(&doc, s))
        .map(|p| p.frames(doc.source.first_line).len() - 1)
        .sum();
    assert!(typed > 0, "the example has typed moves");
    assert_eq!(g.frames.len(), steps + typed, "{:?}", g.frames);
    // A typed frame lasts its speed (160 ms lines in the example: 16 cs),
    // never less than the format's 20 ms.
    let delays: Vec<u16> = g.frames.iter().map(|f| f.4).collect();
    assert!(delays.contains(&16), "{delays:?}");
    assert!(delays.iter().all(|d| *d >= 2), "{delays:?}");
    assert_eq!(*delays.last().unwrap(), 400);
}

#[test]
fn ac3_mp4_needs_ffmpeg_from_path_and_says_so_without_it() {
    let dir = scratch("t120-mp4");
    let out = dir.join("own.mp4");
    let ex = fixture("ownership/ownership.rs.tiza");
    let r = std::process::Command::new(tiza_qa::bin())
        .args([
            "render",
            ex.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ])
        .env("PATH", "/nonexistent")
        .env_remove("TIZA_FFMPEG")
        .output()
        .unwrap();
    assert_eq!(r.status.code(), Some(2));
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(err.contains("ffmpeg not found on PATH"), "{err}");
    assert!(err.contains("brew install ffmpeg") && err.contains("apt install ffmpeg"));
    assert!(err.contains("`.gif` and `.cast` need nothing"), "{err}");
    assert!(!out.exists(), "no partial file");
    assert!(
        std::fs::read_dir(&dir).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("tiza-tmp")),
        "no temp file left behind"
    );
    // With ffmpeg (when the machine has it): the command is printed and
    // the file plays.
    if let Some(ffmpeg) = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("ffmpeg"))
            .find(|p| p.is_file())
    }) {
        let r = std::process::Command::new(tiza_qa::bin())
            .args([
                "render",
                ex.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
                "--interval",
                "0.5",
            ])
            .env("TIZA_FFMPEG", &ffmpeg)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&r.stderr);
        assert!(r.status.success(), "{err}");
        assert!(
            err.contains("ffmpeg -y") && err.contains("yuv420p") && err.contains("fps=30"),
            "{err}"
        );
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.len() > 1000 && bytes[4..8] == *b"ftyp", "not an MP4");
    }
}

#[test]
fn ac5_the_live_instance_is_recorded_from_the_shell_and_over_mcp() {
    let dir = scratch("t120-live");
    let doc = tiza_core::fs::open(fixture("ownership/ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 3}));
    // `tiza render --out x.gif` without FILE: the live scene, every step.
    let out = dir.join("live.gif");
    let r = std::process::Command::new(tiza_qa::bin())
        .args(["render", "--out", out.to_str().unwrap(), "--width", "90"])
        .env("TIZA_SOCKET", &live.socket)
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(decode(&std::fs::read(&out).unwrap()).frames.len(), 8);
    // MCP export_image with .gif.
    let mut mcp = McpClient::start(&live.socket);
    let path = dir.join("mcp.gif");
    let (err, text, _) = mcp.call(
        "export_image",
        json!({"path": path.to_str().unwrap(), "width": 90, "interval": 1, "skip_zero": true}),
    );
    assert!(!err, "{text}");
    assert!(
        text.contains("\"format\": \"gif\"") && text.contains("\"frames\": 7"),
        "{text}"
    );
    let g = decode(&std::fs::read(&path).unwrap());
    assert_eq!(g.frames.len(), 7);
    assert_eq!(g.frames[0].4, 100);
    // Same default scale (2) as the CLI at the same width.
    assert_eq!(g.width, decode(&std::fs::read(&out).unwrap()).width);
    // In the TUI, `:export x.gif` points at the shell command.
    live.command(&format!("export {}", dir.join("tui.gif").display()));
    let (m, is_err) = live.message().unwrap();
    assert!(is_err && m.contains("tiza render --out"), "{m}");
}
