//! Legibility at 1080p: render → YouTube-like compression → measure
//! the contrast of both themes.
//!
//! Acceptance criteria:
//! - AC1 Theme level (always checked, no tools needed): every text role of
//!   both themes (fg, muted, accent, rail, box line, arrow, note, error,
//!   status fg on its bar, the syntax palette after `Theme::legible`) has a
//!   WCAG contrast ≥ 4.5:1 against its background before compression
//!   (`rail` is a decorative line, not text: excluded).
//! - AC2 `scripts/legibility.py DOC --step N [--theme]` renders a PNG,
//!   scales it to 1920×1080, round-trips it through H.264 at a YouTube-like
//!   bitrate (8 Mbit/s), and for every text color of the export measures the
//!   contrast of the stroke cores against the background that came back;
//!   exit 1 when any role falls under `--min` (4.5). Reports JSON with
//!   `--json`; keeps the frames with `--keep`.
//! - AC3 The ownership example, step 7, passes in both themes (worst ≈ 4.9
//!   dark, ≈ 5.0 light). CI runs the script on ubuntu and uploads the
//!   report and the compressed frames as an artifact.
//! - AC4 Here the test runs the script when python3 + Pillow + numpy +
//!   ffmpeg are present and otherwise only reports that it skipped.

use tiza_core::theme::{Theme, contrast};
use tiza_core::{ThemeName, canvas::Rgb};
use tiza_qa::{fixture, scratch};

fn roles(t: &Theme) -> Vec<(&'static str, Rgb, Rgb)> {
    vec![
        ("fg", t.fg, t.bg),
        ("muted (line numbers)", t.muted, t.bg),
        ("accent", t.accent, t.bg),
        ("box line", t.box_line, t.bg),
        ("arrow", t.arrow, t.bg),
        ("note", t.note, t.bg),
        ("error", t.error, t.bg),
        ("status fg", t.status_fg, t.status_bg),
    ]
}

#[test]
fn every_theme_role_passes_before_compression() {
    for name in [ThemeName::Dark, ThemeName::Light] {
        let t = Theme::from_name(name);
        for (role, fg, bg) in roles(&t) {
            let c = contrast(fg, bg);
            assert!(c >= 4.5, "{name:?} {role}: {c:.2} < 4.5");
        }
        for color in t.auto_palette() {
            let c = contrast(color, t.bg);
            assert!(c >= 4.5, "{name:?} palette {color:?}: {c:.2}");
        }
        // The syntax colors that reach the canvas went through `legible`.
        let code = ["// a comment\n", "let x = 1;\n", "fn f() {}\n", "\"str\"\n"].concat();
        let doc = tiza_core::Document::from_code(&code, "rs", None);
        let l = tiza_core::layout::layout(
            &doc,
            &t,
            &tiza_core::layout::LayoutOptions {
                width: 60,
                step: 1,
                focus: None,
                pointer: None,
                line_numbers: true,
                tab_width: 4,
                typing: false,
                stable: true,
                margin_cap: None,
            },
        );
        for (x, y, cell) in l.canvas.cells() {
            if cell.sym.trim().is_empty() || cell.style.dim {
                continue;
            }
            let fg = cell.style.fg.unwrap_or(t.fg);
            let bg = cell.style.bg.unwrap_or(t.bg);
            assert!(
                contrast(fg, bg) >= 4.5,
                "{name:?} cell ({x},{y}) {:?}: {:.2}",
                cell.sym,
                contrast(fg, bg)
            );
        }
    }
}

fn tools_available() -> bool {
    tiza_qa::legibility_tools_missing().is_empty()
}

#[test]
fn pipeline_passes_both_themes() {
    if !tools_available() {
        eprintln!(
            "video_legibility: ffmpeg or python3+Pillow+numpy missing here; the pipeline runs in CI (job `legibility`)"
        );
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let dir = scratch("video_legibility");
    for theme in ["dark", "light"] {
        let json = dir.join(format!("{theme}.json"));
        let out = std::process::Command::new("python3")
            // The binary that it starts reads the config of the tests.
            .envs(tiza_qa::isolated_config_env())
            .args([
                "-I",
                root.join("scripts/legibility.py").to_str().unwrap(),
                fixture("shared/ownership/ownership.rs.tiza")
                    .to_str()
                    .unwrap(),
                "--step",
                "7",
                "--theme",
                theme,
                "--bin",
                tiza_qa::bin().to_str().unwrap(),
                "--json",
                json.to_str().unwrap(),
                "--keep",
                dir.join(theme).to_str().unwrap(),
            ])
            .output()
            .expect("python3");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{theme}: {stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        assert_eq!(report["pass"], serde_json::json!(true), "{stdout}");
        assert!(report["worst"].as_f64().unwrap() >= 4.5, "{stdout}");
        let roles = report["roles"].as_array().unwrap();
        assert!(roles.len() >= 8, "enough roles measured: {stdout}");
        assert!(dir.join(theme).join("after.png").exists());
        // A role that fails must be reported with exit 1: force it with an absurd minimum.
        let out = std::process::Command::new("python3")
            // The binary that it starts reads the config of the tests.
            .envs(tiza_qa::isolated_config_env())
            .args([
                "-I",
                root.join("scripts/legibility.py").to_str().unwrap(),
                fixture("shared/ownership/ownership.rs.tiza")
                    .to_str()
                    .unwrap(),
                "--step",
                "7",
                "--theme",
                theme,
                "--min",
                "40",
                "--bin",
                tiza_qa::bin().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stdout).contains("<-- under"));
    }
}
