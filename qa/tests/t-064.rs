//! t-064 — Viewport warning in the status line and in `status`.
//!
//! Acceptance criteria:
//! - AC1 The status line shows cols×rows and, when cols < 100 or rows < 30, a
//!   short ⚠ badge ("small terminal" / "labels under the lines"); the full
//!   text "small terminal C×R < 100×30…" is in `status.warnings`.
//! - AC2 When the widest code line does not fit, the badge says "code clipped (N cols)".
//! - AC3 The badge disappears when the terminal grows; presentation mode never shows it.
//! - AC4 `status` over the socket returns `warnings: [...]` with the same texts.
//! - Corner: exactly 100×30 is not small; a 0×0 frame produces no warning; padding
//!   reduces the usable width for the clipping check.

use codedraw_qa::Live;
use crossterm::event::KeyCode;
use serde_json::json;

fn status_line(screen: &str) -> String {
    screen.lines().last().unwrap_or("").to_string()
}

#[test]
fn ac1_small_terminal_badge_and_label_placement() {
    // A 60-col code line leaves no margin at 80 cols: labels go under the lines.
    let live = Live::from_code(
        "fn main() {\n    let s = String::from(\"a fairly long literal to fill the row\");\n}\n",
        "rs",
        "t064-small",
    );
    live.command("note 2 a note");
    let s = live.screen(80, 24);
    let bar = status_line(&s);
    assert!(bar.contains("80×24"), "{bar}");
    assert!(bar.contains("⚠ labels under the lines"), "{bar}");
    assert!(s.contains("┆ a note"), "note drawn under the line:\n{s}");
    // Wide but short: labels fit in the margin, the badge only mentions the size.
    let bar = status_line(&live.screen(120, 20));
    assert!(bar.contains("⚠ small terminal"), "{bar}");
    assert!(!bar.contains("labels under"), "{bar}");
    let w = live.api_ok("status", json!({}))["warnings"][0]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        w.contains("120×20 < 100×30"),
        "full text over the socket: {w}"
    );
}

#[test]
fn ac2_clipped_code_is_reported_with_its_width() {
    let long = format!("fn main() {{ let x = \"{}\"; }}\n", "x".repeat(150));
    let live = Live::from_code(&long, "rs", "t064-clip");
    let bar = status_line(&live.screen(120, 40));
    assert!(bar.contains("⚠ code clipped (175 cols)"), "{bar}");
    // Both at once: clipping first, size second (status lists both).
    live.screen(80, 24);
    let w = live.api_ok("status", json!({}))["warnings"].clone();
    let w: Vec<String> = serde_json::from_value(w).unwrap();
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(
        w[0].starts_with("code clipped") && w[1].starts_with("small terminal"),
        "{w:?}"
    );
}

#[test]
fn ac3_badge_goes_away_when_the_terminal_grows_and_in_presentation_mode() {
    let live = Live::from_code("fn main() {}\n", "rs", "t064-grow");
    assert!(status_line(&live.screen(80, 24)).contains('⚠'));
    let big = live.screen(120, 40);
    assert!(!big.contains('⚠'), "{big}");
    live.keys(&[KeyCode::Char('P')]);
    let present = live.screen(80, 24);
    assert!(
        !present.contains('⚠') && !present.contains("NORMAL"),
        "{present}"
    );
    live.keys(&[KeyCode::Char('P')]);
    assert!(status_line(&live.screen(80, 24)).contains('⚠'));
}

#[test]
fn ac4_status_warnings_match_and_corner_cases() {
    let live = Live::from_code("fn main() {}\n", "rs", "t064-status");
    live.screen(100, 30);
    let st = live.api_ok("status", json!({}));
    assert!(st.get("warnings").is_none(), "exactly 100×30 is fine: {st}");
    live.screen(99, 30);
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["warnings"].as_array().unwrap().len(), 1);
    assert_eq!(st["cols"], 99);
    // A 0×0 frame (terminal not sized yet) warns about nothing and does not panic.
    let s = live.screen(0, 0);
    assert!(s.is_empty());
    assert!(live.with(|app| app.viewport_warnings()).is_empty());
    // Padding eats width: a 100-col line fits at 120 cols with :pad 0 0 but not with :pad 0 20.
    let live = Live::from_code(&format!("{}\n", "y".repeat(100)), "rs", "t064-pad");
    live.command("pad 0 0");
    assert!(!status_line(&live.screen(120, 40)).contains("clipped"));
    live.command("pad 0 20");
    assert!(status_line(&live.screen(120, 40)).contains("code clipped (100 cols)"));
}
