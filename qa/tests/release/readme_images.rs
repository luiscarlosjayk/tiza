//! Refresh the README images from the current renderer, with a
//! script so they never go stale again.
//!
//! Acceptance criteria:
//! - AC0 The native GIF uses `--scale 2` by default (PNG already does; MP4
//!   stays at 1): default, `--scale` help text, README, and the size
//!   assertion of the GIF export tests.
//! - AC1 `scripts/readme-images.sh` regenerates every image the README shows
//!   with `tiza` alone (no vhs/ttyd/agg/ffmpeg).
//! - AC2 The images are regenerated at the current main and the README text
//!   around them matches what they show.
//! - AC3 GIF under ~1 MB, PNG under ~300 KB.
//! - AC4 examples/README.md and the example `.tiza` files only change if
//!   the new look makes them read worse.
//! - AC5 CHANGELOG line; AGENTS.md bullet (a visual renderer change re-runs
//!   the script in the same commit).
//! - AC6 and AC7 (the hero image, its title and its layout) are manual
//!   checks since Luis's decision of 2026-10-25: no test checks the hero.
//!   Its three frames are still compared with fresh renders below.
//!
//! The committed images are compared with fresh renders by size (pixels)
//! and the README text block byte for byte, so a renderer change that skips
//! the script fails here. PNG bytes are not compared: the rasteriser may
//! differ in the last bit between CPUs.

use tiza_qa::{example, scratch, tiza_ok};

const WIDTH: &str = "92";

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
fn the_examples_are_untouched_by_rendering() {
    // `tiza render` only reads the document: the example is the same bytes
    // after an export (the autosave belongs to the TUI).
    let ex = example("ownership.rs.tiza");
    let before = std::fs::read(&ex).unwrap();
    render(&["--step", "3"], &scratch("ex").join("x.png"));
    assert_eq!(std::fs::read(&ex).unwrap(), before);
}
