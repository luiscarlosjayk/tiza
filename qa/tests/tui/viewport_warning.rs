//! Viewport warning in the status line and in `status`.
//!
//! Acceptance criteria, as the bar warnings left them (the size alone is no longer a
//! warning; only what the step on screen loses is):
//! - AC1 The status line shows cols×rows. Labels that go under the lines
//!   for lack of a margin are a fact in `status.warnings`, with no badge.
//! - AC2 When the widest code line does not fit, the badge says
//!   "code needs N cols" and `status.warnings` "code needs N columns at C
//!   columns". N is the terminal width from which every code line fits
//!   (line numbers, arrow lanes and padding included), not the width of
//!   the code.
//! - AC3 The badge disappears when the terminal grows; presentation mode
//!   never shows it.
//! - AC4 `status` over the socket returns `warnings: [...]`.
//! - Corner: a 0×0 frame produces no warning; padding reduces the usable
//!   width for the clipping check.
use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::Live;

fn status_line(screen: &str) -> String {
    screen.lines().last().unwrap_or("").to_string()
}

#[test]
fn small_terminal_badge_and_label_placement() {
    // A 60-col code line leaves no margin at 80 cols: labels go under the lines.
    let live = Live::from_code(
        "fn main() {\n    let s = String::from(\"a fairly long literal to fill the row\");\n}\n",
        "rs",
        "small",
    );
    live.command("note 2 a note");
    let s = live.screen(80, 24);
    let bar = status_line(&s);
    assert!(bar.contains("80×24"), "{bar}");
    // Nothing is lost: no badge, the fact goes to `status.warnings`.
    assert!(!bar.contains("  ! "), "{bar}");
    assert!(s.contains("┆ a note"), "note drawn under the line:\n{s}");
    let w = live.api_ok("status", json!({}))["warnings"].clone();
    assert_eq!(w, json!(["labels go under the lines at 80 columns"]));
    // Wide but short: labels fit in the margin, and the size is no warning.
    let bar = status_line(&live.screen(120, 20));
    assert!(!bar.contains("  ! "), "{bar}");
    assert!(live.api_ok("status", json!({})).get("warnings").is_none());
}

#[test]
fn clipped_code_is_reported_with_its_width() {
    let long = format!("fn main() {{ let x = \"{}\"; }}\n", "x".repeat(150));
    let live = Live::from_code(&long, "rs", "clip");
    // 175 cells of code; with the line numbers, the code column's margin
    // and the side padding, 183 columns show it whole.
    let bar = status_line(&live.screen(120, 40));
    assert!(bar.contains("! code needs 183 cols"), "{bar}");
    live.screen(80, 24);
    let w = live.api_ok("status", json!({}))["warnings"].clone();
    assert_eq!(w, json!(["code needs 183 columns at 80 columns"]));
    assert!(status_line(&live.screen(182, 24)).contains("! code needs 183 cols"));
    assert!(!status_line(&live.screen(183, 24)).contains("code needs"));
}

#[test]
fn badge_goes_away_when_the_terminal_grows_and_in_presentation_mode() {
    // A 90-cell line: clipped at 80 columns, whole at 120.
    let live = Live::from_code(&format!("{}\n", "z".repeat(90)), "rs", "grow");
    assert!(status_line(&live.screen(80, 24)).contains("  ! "));
    let big = live.screen(120, 40);
    assert!(!big.contains("  ! "), "{big}");
    live.keys(&[KeyCode::Char('P')]);
    let present = live.screen(80, 24);
    assert!(
        !present.contains("  ! ") && !present.contains("NORMAL"),
        "{present}"
    );
    live.keys(&[KeyCode::Char('P')]);
    assert!(status_line(&live.screen(80, 24)).contains("  ! "));
}

#[test]
fn status_warnings_match_and_corner_cases() {
    let live = Live::from_code("fn main() {}\n", "rs", "status");
    // The size alone, small or not, is no warning.
    for (c, r) in [(100, 30), (99, 30), (60, 12)] {
        live.screen(c, r);
        let st = live.api_ok("status", json!({}));
        assert!(st.get("warnings").is_none(), "{c}×{r}: {st}");
        assert_eq!(st["cols"], c);
    }
    // A 0×0 frame (terminal not sized yet) warns about nothing and does not panic.
    let s = live.screen(0, 0);
    assert!(s.is_empty());
    assert!(live.with(|app| app.viewport_warnings()).is_empty());
    // Padding eats width: a 100-col line fits at 120 cols with :pad 0 0 but not with :pad 0 20.
    let live = Live::from_code(&format!("{}\n", "y".repeat(100)), "rs", "pad");
    live.command("pad 0 0");
    assert!(!status_line(&live.screen(120, 40)).contains("code needs"));
    // The 40 columns of padding count: 40 more than with `:pad 0 0`.
    let bare = live.screen(100, 40);
    assert!(
        status_line(&bare).contains("! code needs 106 cols"),
        "{bare}"
    );
    live.command("pad 0 20");
    assert!(status_line(&live.screen(120, 40)).contains("! code needs 146 cols"));
}
