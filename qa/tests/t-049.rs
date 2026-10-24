//! t-049 — Export to plain text (box-drawing characters), Markdown and ANSI.
//!
//! Acceptance criteria:
//! - AC1 `tiza render FILE --out x.txt` writes exactly the screen as text:
//!   box-drawing glyphs kept, no ANSI, no trailing spaces, no trailing blank
//!   rows, one `\n` at the end. `--format txt` forces it for any path.
//! - AC2 `--out x.md` (or `--format md`) wraps the same text in a
//!   ```` ```text ```` fence with the document title in bold above (no title:
//!   just the fence), ready for a README, an issue or a chat message.
//! - AC3 `--out x.ansi` writes 24-bit SGR sequences, a reset at the end of
//!   every line, and the same visible text as `.txt` once the escapes are
//!   stripped; `--theme light` changes the colors.
//! - AC4 `:export [path] [step=N] [width=N] [theme=…]` in the TUI writes the
//!   same formats (plus `.svg`/`.png` through the binary) and defaults to
//!   `<doc>-stepN.txt` next to the document; unknown extensions and options
//!   are errors that name the valid ones.
//! - AC5 The MCP `export_image` tool accepts `.txt`, `.ansi` and `.md` and
//!   reports the format; other extensions are still rejected.

use serde_json::json;
use tiza_qa::{Live, McpClient, fixture, scratch, tiza, tiza_ok};

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // CSI … final byte in 0x40..=0x7E
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&d) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn render(args: &[&str]) -> String {
    let f = fixture("t-050/export.tiza");
    let mut full = vec!["render", f.to_str().unwrap(), "--width", "100"];
    full.extend_from_slice(args);
    tiza_ok(&full)
}

#[test]
fn ac1_txt_is_the_screen_without_styles_or_trailing_space() {
    let txt = render(&["--out", "-", "--format", "txt", "--step", "9"]);
    assert!(txt.contains("fn main() {"));
    assert!(
        txt.contains('╭') && txt.contains('│'),
        "box drawing kept:\n{txt}"
    );
    assert!(!txt.contains('\x1b'), "no ANSI in .txt");
    assert!(
        txt.ends_with('\n') && !txt.ends_with("\n\n"),
        "one newline, no blank tail"
    );
    for line in txt.lines() {
        assert_eq!(line, line.trim_end(), "trailing spaces on {line:?}");
    }
    // Wide glyphs are kept as text (no continuation-cell garbage).
    assert!(txt.contains("let 名前 = 1;"), "{txt}");
    // The extension alone picks the format: same bytes through a file.
    let dir = scratch("t049-txt");
    let path = dir.join("shot.txt");
    render(&["--out", path.to_str().unwrap(), "--step", "9"]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), txt);
}

#[test]
fn ac2_markdown_is_a_fenced_block_with_the_title() {
    let md = render(&["--out", "-", "--format", "md", "--step", "9"]);
    let txt = render(&["--out", "-", "--format", "txt", "--step", "9"]);
    assert!(
        md.starts_with(
            "**t-050 export: symbols, emoji, wide glyphs, every annotation kind**\n\n```text\n"
        ),
        "{md}"
    );
    assert!(md.ends_with("\n```\n"), "{md}");
    let inner = md
        .split("```text\n")
        .nth(1)
        .unwrap()
        .strip_suffix("```\n")
        .unwrap();
    assert_eq!(inner, txt, "the block is exactly the .txt export");
    // A document without a title: just the fence.
    let live = Live::from_code("fn a() {}\n", "rs", "t049-md-notitle");
    let dir = scratch("t049-md-notitle");
    let out = dir.join("x.md");
    live.command(&format!("export {}", out.display()));
    let md = std::fs::read_to_string(&out).unwrap();
    assert!(md.starts_with("```text\n"), "{md}");
    // `.markdown` is the same format.
    let out2 = dir.join("x.markdown");
    live.command(&format!("export {}", out2.display()));
    assert_eq!(std::fs::read_to_string(&out2).unwrap(), md);
}

#[test]
fn ac3_ansi_has_truecolor_resets_and_the_same_text() {
    let ansi = render(&["--out", "-", "--format", "ansi", "--step", "9"]);
    let txt = render(&["--out", "-", "--format", "txt", "--step", "9"]);
    assert!(ansi.contains("\x1b[38;2;"), "24-bit foreground");
    assert!(ansi.contains("\x1b[48;2;"), "24-bit background (highlight)");
    for line in ansi.lines() {
        assert!(line.ends_with("\x1b[0m"), "every line resets: {line:?}");
    }
    let plain: Vec<String> = strip_ansi(&ansi)
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect();
    let want: Vec<String> = txt.lines().map(str::to_string).collect();
    assert_eq!(&plain[..want.len()], &want[..], "same visible text");
    assert!(plain[want.len()..].iter().all(|l| l.is_empty()));
    let light = render(&[
        "--out", "-", "--format", "ansi", "--step", "9", "--theme", "light",
    ]);
    assert_ne!(ansi, light, "theme changes the colors");
    assert_eq!(strip_ansi(&light).trim_end(), strip_ansi(&ansi).trim_end());
}

#[test]
fn ac4_export_command_formats_defaults_and_errors() {
    let dir = scratch("t049-cmd");
    let doc = tiza_core::fs::open(fixture("t-050/export.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("demo.tiza"), dir.join("live.sock"));
    live.with(|a| a.size = (100, 40));
    live.command("step 2");
    // Default path: <doc>-stepN.txt next to the document.
    live.command("export");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("exported step 2 to"), "{msg}");
    let default = dir.join("demo-step2.txt");
    let txt = std::fs::read_to_string(&default).unwrap();
    assert!(txt.contains("fn main() {") && !txt.contains('\x1b'));
    // Options: step, width, theme.
    let wide = dir.join("wide.txt");
    live.command(&format!(
        "export {} step=9 width=140 theme=light",
        wide.display()
    ));
    let (msg, err) = live.message().unwrap();
    // (A step past the last one exports the last, and says so, since t-227.)
    assert!(
        !err && msg
            .contains("step 9 does not exist (the document has steps 0 to 2); exported step 2"),
        "{msg}"
    );
    let wide_txt = std::fs::read_to_string(&wide).unwrap();
    let narrow = dir.join("narrow.txt");
    live.command(&format!("export {} step=9 width=60", narrow.display()));
    let narrow_txt = std::fs::read_to_string(&narrow).unwrap();
    let widest = |t: &str| t.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    assert!(
        widest(&wide_txt) > widest(&narrow_txt),
        "width= changes the canvas"
    );
    assert!(widest(&narrow_txt) <= 60, "{narrow_txt}");
    // ANSI and Markdown through the command.
    let ansi = dir.join("a.ansi");
    live.command(&format!("export {} step=9", ansi.display()));
    assert!(
        std::fs::read_to_string(&ansi)
            .unwrap()
            .contains("\x1b[38;2;")
    );
    let md = dir.join("a.md");
    live.command(&format!("export {}", md.display()));
    assert!(std::fs::read_to_string(&md).unwrap().contains("```text\n"));
    // Errors name the valid values.
    live.command(&format!("export {}", dir.join("a.bmp").display()));
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains(".txt .ansi .md .svg .png"), "{msg}");
    // A recording is made from the shell (t-120): the message says how.
    live.command(&format!("export {}", dir.join("a.gif").display()));
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("tiza render --out"), "{msg}");
    live.command(&format!("export {} width=5", dir.join("b.txt").display()));
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("width=5"), "{msg}");
    live.command(&format!(
        "export {} theme=sepia",
        dir.join("b.txt").display()
    ));
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("dark or light"), "{msg}");
    live.command(&format!("export {} foo=1", dir.join("b.txt").display()));
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("step, width, theme"), "{msg}");
    assert!(!dir.join("b.txt").exists(), "nothing written on error");
    // Headless (no image exporter installed): SVG says how to get one.
    live.command(&format!("export {}", dir.join("a.svg").display()));
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("tiza render"), "{msg}");
    // Unwritable path: an error, not a panic (since t-241 the missing
    // folder is created first, and the one that cannot be is named).
    live.command("export /nonexistent-dir/x.txt");
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("could not create the folder /nonexistent-dir"),
        "{msg}"
    );
}

#[test]
fn ac4_export_svg_and_png_through_the_binary() {
    // The headless harness has no image exporter; install the same closure
    // the binary installs and check both formats come out through `:export`.
    let dir = scratch("t049-img");
    let live = Live::from_code("fn a() {}\n", "rs", "t049-img");
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
    let svg = dir.join("x.svg");
    live.command(&format!("export {}", svg.display()));
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    assert!(std::fs::read_to_string(&svg).unwrap().starts_with("<svg"));
    let png = dir.join("x.png");
    live.command(&format!("export {}", png.display()));
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn ac5_mcp_export_image_writes_text_formats() {
    let dir = scratch("t049-mcp");
    let doc = tiza_core::fs::open(fixture("t-050/export.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let mut mcp = McpClient::start(&live.socket);
    for (ext, needle) in [
        ("txt", "fn main() {"),
        ("ansi", "\x1b[38;2;"),
        ("md", "```text\n"),
    ] {
        let path = dir.join(format!("out.{ext}"));
        let (err, text, _) = mcp.call(
            "export_image",
            json!({"path": path.to_str().unwrap(), "step": 9, "width": 100}),
        );
        assert!(!err, "{text}");
        assert!(text.contains(&format!("\"format\": \"{ext}\"")), "{text}");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(needle), "{ext}: {content}");
    }
    let (err, text, _) = mcp.call(
        "export_image",
        json!({"path": dir.join("out.bmp").to_str().unwrap()}),
    );
    assert!(err && text.contains(".txt, .ansi, .md or .gif"), "{text}");
}

#[test]
fn corner_empty_document_and_stdin_render_to_md() {
    // An empty document exports a gutter and nothing else, no panic, no blank tail.
    let live = Live::from_code("", "rs", "t049-empty");
    let dir = scratch("t049-empty");
    let out = dir.join("e.txt");
    live.command(&format!("export {}", out.display()));
    let txt = std::fs::read_to_string(&out).unwrap();
    assert!(!txt.ends_with("\n\n"), "{txt:?}");
    // `render -` from stdin straight to Markdown on stdout.
    use std::io::Write;
    let mut child = std::process::Command::new(tiza_qa::bin())
        .args([
            "render", "-", "--out", "-", "--format", "md", "--lang", "rs",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"fn main() {}\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let md = String::from_utf8_lossy(&out.stdout);
    assert!(
        md.starts_with("```text\n") && md.contains("fn main() {}"),
        "{md}"
    );
    assert!(tiza(&["--version"]).status.success());
}
