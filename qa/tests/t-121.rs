//! t-121 — Refresh the README images from the current renderer, with a
//! script so they never go stale again.
//!
//! Acceptance criteria:
//! - AC0 The native GIF uses `--scale 2` by default (PNG already does; MP4
//!   stays at 1): default, `--scale` help text, README, and the size
//!   assertion of `qa/tests/t-120.rs`.
//! - AC1 `scripts/readme-images.sh` regenerates every image the README shows
//!   with `tiza` alone (no vhs/ttyd/agg/ffmpeg).
//! - AC2 The images are regenerated at the current main and the README text
//!   around them matches what they show.
//! - AC3 GIF under ~1 MB, PNG under ~300 KB.
//! - AC4 examples/README.md and the example `.tiza` files only change if
//!   the new look makes them read worse.
//! - AC5 CHANGELOG line; AGENTS.md bullet (a visual renderer change re-runs
//!   the script in the same commit).
//! - AC6 Hero `docs/img/hero.png` (1200×630 at device scale 2) from
//!   `docs/img/src/hero.html` with the real `--step 1|4|7` frames; the back
//!   windows dimmed with an overlay, never with opacity; English alt text.
//! - AC7 The title in Libron (the brand font in docs/brand/fonts), the frames
//!   real `tiza render` exports used as produced. Since Luis chose the logo
//!   (2026-10-24), the title is `docs/brand/logo.svg`, whose glyphs are
//!   Libron outlines: its paths inline, without its slate background.
//!
//! The committed images are compared with fresh renders by size (pixels)
//! and the README text block byte for byte, so a renderer change that skips
//! the script fails here. PNG bytes are not compared: the rasteriser may
//! differ in the last bit between CPUs, and the hero depends on the system
//! sans of the machine that took the screenshot.

use tiza_qa::{example, scratch, tiza_ok};

const WIDTH: &str = "108";

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    let be = |i: usize| u32::from_be_bytes(bytes[i..i + 4].try_into().unwrap());
    (be(16), be(20))
}

fn gif_info(bytes: &[u8]) -> (u16, u16, usize) {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut d = opts.read_info(bytes).unwrap();
    let (w, h) = (d.width(), d.height());
    let mut n = 0;
    while d.read_next_frame().unwrap().is_some() {
        n += 1;
    }
    (w, h, n)
}

fn render(args: &[&str], out: &std::path::Path) {
    let ex = example("ownership.rs.tiza");
    let mut a = vec![
        "render",
        ex.to_str().unwrap(),
        "--width",
        WIDTH,
        "--out",
        out.to_str().unwrap(),
    ];
    a.extend_from_slice(args);
    tiza_ok(&a);
}

#[test]
fn ac0_the_gif_defaults_to_scale_2_and_video_stays_at_1() {
    let help = tiza_ok(&["render", "--help"]);
    assert!(
        help.contains("default 2 for PNG and GIF, 1 for MP4/WebM"),
        "{help}"
    );
    let dir = scratch("t121-scale");
    let (two, one) = (dir.join("two.gif"), dir.join("one.gif"));
    render(&[], &two);
    render(&["--scale", "1"], &one);
    let (w2, h2, _) = gif_info(&std::fs::read(&two).unwrap());
    let (w1, h1, _) = gif_info(&std::fs::read(&one).unwrap());
    assert!(
        w2.abs_diff(w1 * 2) <= 1 && h2.abs_diff(h1 * 2) <= 1,
        "{w2}×{h2} vs {w1}×{h1}"
    );
}

#[test]
fn ac1_the_script_uses_tiza_alone_and_writes_every_readme_image() {
    let path = root().join("scripts/readme-images.sh");
    let s = read("scripts/readme-images.sh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "not executable");
    }
    for out in [
        "docs/img/ownership.gif",
        "docs/img/ownership-step7.png",
        "docs/img/src/step$s.png",
        "docs/img/hero.png",
        "docs/img/src/hero.html",
        "readme-images:ownership-txt",
    ] {
        assert!(s.contains(out), "script does not write {out}");
    }
    // Recording tools never appear as commands.
    for line in s.lines().filter(|l| !l.trim_start().starts_with('#')) {
        for tool in ["vhs ", "ttyd", "agg ", "ffmpeg", "magick", "convert "] {
            assert!(!line.contains(tool), "{tool} in: {line}");
        }
    }
    // Every image the README and the exports page show is one the script
    // writes. The page is in docs/reference/, so its paths start at `../`.
    let mut shown = 0;
    for (page, prefix) in [("README.md", ""), ("docs/reference/exports.md", "../")] {
        for (alt, src) in images(&read(page)) {
            assert!(!alt.trim().is_empty(), "{page}: {src} has no alt text");
            let Some(rel) = src.strip_prefix(prefix) else {
                continue;
            };
            if rel.starts_with("img/") || rel.starts_with("docs/img/") {
                let rel = if rel.starts_with("img/") {
                    format!("docs/{rel}")
                } else {
                    rel.to_string()
                };
                assert!(
                    s.contains(&rel),
                    "{page}: {rel} is not regenerated by the script"
                );
                assert!(root().join(&rel).is_file(), "{page}: {rel} missing");
                shown += 1;
            }
        }
    }
    assert!(shown >= 3, "the hero, the GIF and the step-7 PNG: {shown}");
}

fn images(md: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = md;
    while let Some(i) = rest.find("![") {
        rest = &rest[i + 2..];
        let Some(j) = rest.find("](") else { break };
        let alt = rest[..j].to_string();
        let Some(k) = rest[j + 2..].find(')') else {
            break;
        };
        out.push((alt, rest[j + 2..j + 2 + k].to_string()));
        rest = &rest[j + 2 + k..];
    }
    out
}

#[test]
fn ac2_ac3_the_committed_images_match_the_current_renderer_and_the_budget() {
    let dir = scratch("t121-fresh");
    // GIF: same frames and size as a fresh render; < 1 MB.
    let gif_path = dir.join("own.gif");
    render(&[], &gif_path);
    let committed = std::fs::read(root().join("docs/img/ownership.gif")).unwrap();
    assert!(committed.len() < 1_000_000, "GIF {} bytes", committed.len());
    assert_eq!(
        gif_info(&committed),
        gif_info(&std::fs::read(&gif_path).unwrap()),
        "docs/img/ownership.gif is stale: run scripts/readme-images.sh"
    );
    // PNGs: step 7 (< 300 KB) and the hero's three frames.
    for (rel, step) in [
        ("docs/img/ownership-step7.png", "7"),
        ("docs/img/src/step1.png", "1"),
        ("docs/img/src/step4.png", "4"),
        ("docs/img/src/step7.png", "7"),
    ] {
        let fresh = dir.join(format!("step{step}.png"));
        render(&["--step", step], &fresh);
        let c = std::fs::read(root().join(rel)).unwrap();
        assert_eq!(
            png_size(&c),
            png_size(&std::fs::read(&fresh).unwrap()),
            "{rel} is stale: run scripts/readme-images.sh"
        );
        if rel == "docs/img/ownership-step7.png" {
            assert!(c.len() < 300_000, "{rel} {} bytes", c.len());
        }
    }
    // The text block under the GIF is the step-7 text export, rows 1..=17
    // plus the box's bottom border.
    let txt = tiza_ok(&[
        "render",
        example("ownership.rs.tiza").to_str().unwrap(),
        "--width",
        WIDTH,
        "--step",
        "7",
        "--out",
        "-",
        "--format",
        "txt",
    ]);
    let mut want = Vec::new();
    let mut after17 = false;
    for line in txt.lines() {
        want.push(line);
        if after17 && line.contains('╰') {
            break;
        }
        if line.split_whitespace().next() == Some("17") {
            after17 = true;
        }
    }
    let readme = read("README.md");
    let block = format!(
        "<!-- readme-images:ownership-txt -->\n```\n{}\n```\n<!-- /readme-images:ownership-txt -->",
        want.join("\n")
    );
    assert!(
        readme.contains(&block),
        "README text block is stale: run scripts/readme-images.sh\n{block}"
    );
}

#[test]
fn ac4_the_examples_are_untouched_by_rendering() {
    // `tiza render` only reads the document: the example is the same bytes
    // after an export (the autosave belongs to the TUI).
    let ex = example("ownership.rs.tiza");
    let before = std::fs::read(&ex).unwrap();
    render(&["--step", "3"], &scratch("t121-ex").join("x.png"));
    assert_eq!(std::fs::read(&ex).unwrap(), before);
}

#[test]
fn ac5_changelog_and_agents_rule() {
    // The condensed CHANGELOG of 0.1.0 (t-217) lists what changes for a
    // user: the README images script is the maintainers'.
    let agents = read("AGENTS.md");
    assert!(
        agents.contains("any visual change to the renderer re-runs `scripts/readme-images.sh` in the same commit"),
        "AGENTS.md bullet missing"
    );
}

#[test]
fn ac6_ac7_the_hero_is_built_from_real_frames_with_the_logo_for_the_title() {
    let hero = std::fs::read(root().join("docs/img/hero.png")).unwrap();
    assert_eq!(png_size(&hero), (2400, 1260), "1200×630 at device scale 2");
    assert!(hero.len() < 1_000_000, "hero {} bytes", hero.len());
    let html = read("docs/img/src/hero.html");
    // The logo of docs/brand for the title: the same paths, inline, less
    // its slate background (the page has its own). One mark, not two.
    let logo = read("docs/brand/logo.svg");
    let background = "<rect width=\"1200\" height=\"400\" rx=\"28\" fill=\"#22272b\"/>";
    let paths = logo
        .split_once(background)
        .expect("the logo's background")
        .1
        .trim_end()
        .trim_end_matches("</svg>");
    assert!(
        html.contains(paths),
        "the hero's logo differs from docs/brand/logo.svg"
    );
    assert!(!html.contains(background));
    assert!(html.contains("aria-label=\"Tiza\""));
    assert!(!html.contains("<h1>TIZA</h1>") && !html.contains("@font-face"));
    // The frames are the committed renders, with English alt text, and the
    // back windows dimmed by an overlay, not by opacity.
    for s in ["1", "4", "7"] {
        assert!(
            html.contains(&format!(
                "<img src=\"step{s}.png\" alt=\"Tiza frame, step {s}"
            )),
            "step {s}"
        );
    }
    assert!(html.contains(".dim::after") && html.contains("rgba(13, 17, 23, .55)"));
    assert!(
        !html.contains("opacity"),
        "dim with an overlay, never opacity"
    );
    assert_eq!(html.matches("class=\"win z").count(), 3);
    assert_eq!(html.matches(" dim\"").count(), 2);
    // No image inside a frame is anything but a `tiza render` export: the
    // template only references the three step files.
    assert_eq!(html.matches("<img ").count(), 3);
}
