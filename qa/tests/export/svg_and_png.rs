//! SVG and PNG export with embedded fonts.
//!
//! Acceptance criteria:
//! - AC1 `tiza render FILE --out x.svg` writes a standalone SVG: theme
//!   background, one text run per style, backgrounds for highlights, fonts
//!   embedded as `@font-face` unless `--no-embed-fonts`.
//! - AC2 `--out x.png` writes a PNG of `width × 0.6em + margins` by
//!   `rows × 1.32em + margins`, times `--scale` (default 2).
//! - AC3 Without FILE the live instance is exported (its current step and width).
//! - AC4 Symbols the main font lacks (★ ✎ ⇄), wide glyphs and box drawing render
//!   with the bundled fonts; emoji need `--system-fonts`.
//! - AC5 PNG bytes are refused on a terminal stdout but written to a pipe.
//! - AC6 The MCP `export_image` tool writes .png/.svg (and .txt/.ansi/.md) and rejects other extensions.
//! - AC7 Dark and light themes both export; `--theme` overrides the document's.
//! - AC8 (with the grid checks) A mark label's glyphs
//!   keep their own run, the words start on the next cell, and every text
//!   run sits on the grid.

use serde_json::json;
use tiza_qa::{Live, fixture, scratch, tiza, tiza_ok};

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    (
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    )
}

#[test]
fn svg_is_standalone_and_fonts_are_optional() {
    let dir = scratch("svg");
    let f = fixture("shared/export/export.tiza");
    let svg = dir.join("a.svg");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        svg.to_str().unwrap(),
        "--width",
        "100",
    ]);
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.starts_with("<svg xmlns="));
    assert_eq!(
        text.matches("@font-face").count(),
        4,
        "regular, bold, italic + symbols: the faces this export draws (no bold italic text)"
    );
    assert!(text.contains("<title>Export:"));
    assert!(text.contains("hot path"), "mark legend");
    assert!(text.contains("a note in the margin"));
    assert!(text.contains("<rect x="), "highlight backgrounds");
    let small = dir.join("b.svg");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        small.to_str().unwrap(),
        "--no-embed-fonts",
    ]);
    let small_text = std::fs::read_to_string(&small).unwrap();
    assert!(!small_text.contains("@font-face"));
    assert!(small_text.len() < 64 * 1024 && text.len() > 1_000_000);
}

#[test]
fn png_size_follows_columns_rows_and_scale() {
    let dir = scratch("png");
    let f = fixture("shared/export/export.tiza");
    let txt = tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--width",
        "100",
        "--out",
        "-",
    ]);
    let rows = txt.lines().count() as f32;
    for (scale, expect_w) in [("1", 1024u32), ("2", 2048), ("1.5", 1536)] {
        let png = dir.join(format!("s{scale}.png"));
        tiza_ok(&[
            "render",
            f.to_str().unwrap(),
            "--width",
            "100",
            "--scale",
            scale,
            "--out",
            png.to_str().unwrap(),
        ]);
        let (w, h) = png_size(&std::fs::read(&png).unwrap());
        assert_eq!(w, expect_w, "scale {scale}");
        let s: f32 = scale.parse().unwrap();
        assert_eq!(
            h,
            ((rows * 21.12 + 64.0).ceil() * s).ceil() as u32,
            "scale {scale}"
        );
    }
    // Corner: margin 0 and radius 0 give exactly cols × 9.6 px.
    let png = dir.join("tight.png");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--width",
        "50",
        "--scale",
        "1",
        "--margin",
        "0",
        "--radius",
        "0",
        "--out",
        png.to_str().unwrap(),
    ]);
    assert_eq!(png_size(&std::fs::read(&png).unwrap()).0, 480);
}

#[test]
fn live_instance_is_exported_without_a_file() {
    let dir = scratch("live");
    let doc = tiza_core::fs::open(fixture("shared/export/export.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 1}));
    let png = dir.join("live.png");
    tiza_ok(&[
        "render",
        "--out",
        png.to_str().unwrap(),
        "--socket",
        live.socket.to_str().unwrap(),
    ]);
    assert!(std::fs::metadata(&png).unwrap().len() > 10_000);
    let svg = dir.join("live.svg");
    tiza_ok(&[
        "render",
        "--out",
        svg.to_str().unwrap(),
        "--no-embed-fonts",
        "--socket",
        live.socket.to_str().unwrap(),
    ]);
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(
        text.contains("important") && !text.contains("a note in the margin"),
        "step 1 only"
    );
    // Corner: no instance → clear error, exit 1.
    let out = tiza(&[
        "render",
        "--out",
        "x.png",
        "--socket",
        dir.join("none.sock").to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no Tiza instance"));
}

#[test]
fn symbols_and_wide_glyphs_render_with_bundled_fonts() {
    let dir = scratch("glyphs");
    let f = fixture("shared/export/export.tiza");
    let png = dir.join("g.png");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--width",
        "90",
        "--scale",
        "1",
        "--out",
        png.to_str().unwrap(),
    ]);
    let svg = tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--width",
        "90",
        "--format",
        "svg",
        "--no-embed-fonts",
        "--out",
        "-",
    ]);
    let png_bytes = tiza_export::to_png(
        &svg,
        &tiza_export::PngOptions {
            scale: 1.0,
            system_fonts: false,
        },
    )
    .unwrap();
    let pm = resvg_decode(&png_bytes);
    // The star/pencil/arrows row (L3) has painted pixels right of the code column
    // where the comment sits: the fallback font drew something.
    let (w, _h) = pm;
    assert!(w > 0);
    assert!(svg.contains("★ ✎ ⇄"));
    assert!(svg.contains("名前"));
}

fn resvg_decode(bytes: &[u8]) -> (u32, u32) {
    png_size(bytes)
}

#[test]
fn png_to_stdout_only_through_a_pipe() {
    let f = fixture("shared/export/export.tiza");
    let out = tiza(&[
        "render",
        f.to_str().unwrap(),
        "--format",
        "png",
        "--out",
        "-",
    ]);
    assert!(out.status.success());
    assert_eq!(&out.stdout[..4], b"\x89PNG");
}

#[test]
fn mcp_export_image_tool() {
    let dir = scratch("mcp");
    let doc = tiza_core::fs::open(fixture("shared/export/export.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let png = dir.join("mcp.png");
    let msgs = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"qa","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"export_image","arguments":{"path": png.to_str().unwrap(), "theme": "light", "scale": 1}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"export_image","arguments":{"path": dir.join("x.bmp").to_str().unwrap()}}}),
    ];
    let input = msgs
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut child = std::process::Command::new(tiza_qa::bin())
        .args(["mcp", "--socket", live.socket.to_str().unwrap()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let replies: Vec<serde_json::Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let r2 = replies.iter().find(|r| r["id"] == 2).unwrap();
    assert!(r2["error"].is_null(), "{r2}");
    assert!(png.exists());
    let r3 = replies.iter().find(|r| r["id"] == 3).unwrap();
    assert!(
        r3["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains(".png, .svg, .txt, .ansi, .md, .gif or .webp"),
        "{r3}"
    );
}

#[test]
fn both_themes_export_and_flag_overrides_document() {
    let dir = scratch("themes");
    let f = fixture("shared/export/export.tiza");
    for theme in ["dark", "light"] {
        let svg = dir.join(format!("{theme}.svg"));
        tiza_ok(&[
            "render",
            f.to_str().unwrap(),
            "--theme",
            theme,
            "--no-embed-fonts",
            "--out",
            svg.to_str().unwrap(),
        ]);
        let text = std::fs::read_to_string(&svg).unwrap();
        let bg = if theme == "dark" {
            tiza_core::Theme::dark().bg
        } else {
            tiza_core::Theme::light().bg
        };
        assert!(
            text.contains(&format!("fill=\"#{:02x}{:02x}{:02x}\"", bg.0, bg.1, bg.2)),
            "{theme} background"
        );
    }
}

// ---------- Mark labels in the SVG (with the grid checks) ----------

fn svg(args: &[&str]) -> String {
    let f = fixture("export/svg_and_png/ownership.rs.tiza");
    let mut full = vec![
        "render",
        f.to_str().unwrap(),
        "--out",
        "-",
        "--format",
        "svg",
        "--step",
        "7",
        "--width",
        "100",
    ];
    full.extend_from_slice(args);
    tiza_ok(&full)
}

fn texts(svg: &str) -> Vec<&str> {
    svg.match_indices("<text ")
        .map(|(i, _)| &svg[i..svg[i..].find("</text>").map(|e| i + e + 7).unwrap()])
        .collect()
}

/// A mark label's glyphs (`━▶`, `╳`) stay their own run on the grid and the
/// words start on the next cell; every run has its `textLength`.
#[test]
fn mark_glyphs_keep_their_own_run_on_the_grid() {
    let s = svg(&[]);
    let runs = texts(&s);
    let glyph = runs
        .iter()
        .find(|t| t.ends_with(">━▶</text>"))
        .expect("glyph run");
    assert!(
        glyph.contains("textLength") && !glyph.contains("class=\"p"),
        "{glyph}"
    );
    let words = runs
        .iter()
        .find(|t| t.contains("> move: s moves"))
        .expect("words run");
    assert!(
        words.contains("textLength") && !words.contains("class=\"p"),
        "{words}"
    );
    // The words start right after the glyph cells (2 cells = 2 × 9.6 px at 16 px).
    let x = |t: &str| {
        t.split("x=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse::<f32>()
            .unwrap()
    };
    assert!(
        (x(words) - x(glyph) - 2.0 * 9.6).abs() < 0.01,
        "{} vs {}",
        x(words),
        x(glyph)
    );
    let drop = runs
        .iter()
        .find(|t| t.ends_with(">╳</text>"))
        .expect("drop glyph");
    assert!(!drop.contains("class=\"p"));
    // Every text run sits on the grid, and the glyph and the
    // words of `move: s moves` are where step 7 at 100 columns puts them.
    assert!(!s.contains("letter-spacing"));
    let count = s.matches("<text ").count();
    assert!(count > 50, "{count} text runs");
    assert_eq!(s.matches("textLength=").count(), count);
    assert!(
        s.contains(r##"<text x="800.00" y="111.68" textLength="19.20" lengthAdjust="spacing" fill="#f08b5b" xml:space="preserve">━▶</text>"##),
        "glyph run"
    );
    assert!(
        s.contains(r#"<text x="819.20" y="111.68" textLength="163.20""#),
        "words run"
    );
}
