//! t-034 — README «Terminal support»: verified in Zed's terminal and Herdr
//! on macOS; other truecolor terminals expected to work, with known caveats.
//!
//! Acceptance criteria:
//! - AC1 README section «Terminal support» (English): verified on macOS in
//!   Zed's integrated terminal and in Herdr; any terminal with truecolor and
//!   Unicode box drawing should work, others are not tested — reports
//!   welcome.
//! - AC2 Known caveats: Terminal.app has truecolor only since macOS 26; tmux
//!   and OSC queries, tmux `mouse on/off` with `--mouse`; Zed needs
//!   `line_height: standard` for box drawing; no essential shortcut depends
//!   on the kitty keyboard protocol.
//! - AC3 One line on how to report a terminal issue (terminal name, version,
//!   `TERM`, `COLORTERM`, a screenshot).
//!
//! The section describes what the code does, so besides the text these tests
//! pin the claims it makes about the code: a plain (not tmux-wrapped) OSC
//! 11/10 query, no `COLORTERM` check, bracketed paste on, mouse capture only
//! with `--mouse`, no kitty keyboard flags, and `--theme terminal` drawing
//! without a single 24-bit colour. If one of these changes, the README
//! section has to change with it.

use std::path::{Path, PathBuf};

use tiza_qa::{Live, tiza_ok};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The «Terminal support» section of the README, up to the next `## `.
fn section() -> String {
    let readme = read("README.md");
    let start = readme
        .find("\n## Terminal support\n")
        .expect("README has a «## Terminal support» section");
    let rest = &readme[start + 1..];
    let end = rest[3..].find("\n## ").map(|i| i + 3).unwrap_or(rest.len());
    rest[..end].to_string()
}

/// Every `.rs` file under `crates/`, as (path, contents).
fn crate_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push((p.clone(), std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("crates"), &mut out);
    assert!(!out.is_empty());
    out
}

#[test]
fn ac1_the_section_says_what_is_verified_and_what_is_not() {
    let s = section();
    for needle in [
        "Verified on macOS in Zed's integrated terminal and in [Herdr]",
        "truecolor",
        "Unicode box drawing",
        "**not tested**",
        "reports welcome",
    ] {
        assert!(s.contains(needle), "section lacks «{needle}»:\n{s}");
    }
    // Nothing beyond Zed and Herdr is called verified.
    assert_eq!(s.matches("erified").count(), 1, "{s}");
}

#[test]
fn ac2_the_caveats_are_there() {
    let s = section();
    for needle in [
        "Terminal.app",
        "macOS 26",
        "allow-passthrough",
        "set -g mouse on|off",
        "--mouse",
        "line_height: \"standard\"",
        "kitty keyboard protocol",
        "--theme terminal",
        "Bracketed paste",
    ] {
        assert!(s.contains(needle), "section lacks «{needle}»:\n{s}");
    }
}

#[test]
fn ac2_the_colour_query_is_plain_osc_not_wrapped_for_tmux() {
    // The README says `allow-passthrough` does not change the query: that
    // holds only while the query is not a `ESC P tmux;` passthrough.
    assert_eq!(
        tiza_tui::appearance::QUERY,
        "\x1b]11;?\x1b\\\x1b]10;?\x1b\\"
    );
    for (path, src) in crate_sources() {
        assert!(
            !src.contains("Ptmux;"),
            "{} wraps something for tmux passthrough: update the README's tmux caveat",
            path.display()
        );
    }
}

#[test]
fn ac2_no_colorterm_check_and_no_kitty_keyboard_flags() {
    for (path, src) in crate_sources() {
        assert!(
            !src.contains("\"COLORTERM\""),
            "{} reads COLORTERM: the README says there is no 256-colour fallback",
            path.display()
        );
        assert!(
            !src.contains("KeyboardEnhancementFlags"),
            "{} enables the kitty keyboard protocol: the README says Tiza does not",
            path.display()
        );
    }
}

#[test]
fn ac2_bracketed_paste_is_on_and_the_mouse_only_with_the_flag() {
    let tui = read("crates/tiza-tui/src/lib.rs");
    assert!(tui.contains("execute!(io::stdout(), EnableBracketedPaste)?;"));
    assert!(
        tui.contains("if app.mouse {\n        let _ = execute!(io::stdout(), EnableMouseCapture);"),
        "mouse capture at start must depend on --mouse"
    );
    assert_eq!(
        tui.matches("EnableMouseCapture").count(),
        3,
        "import, start, :mouse on"
    );
    let help = tiza_ok(&["--help"]);
    assert!(help.contains("--mouse"), "{help}");
    assert!(help.contains("Off by default"), "{help}");
}

#[test]
fn ac2_the_terminal_theme_sends_no_24_bit_colour() {
    let live = Live::from_code(
        "fn main() {\n    let s = String::from(\"hi\");\n    println!(\"{s}\");\n}\n",
        "rs",
        "t-034-terminal-theme",
    );
    live.api_ok(
        "annotation.add",
        serde_json::json!({"type": "note", "line": 2, "text": "s is born"}),
    );
    live.api_ok(
        "annotation.add",
        serde_json::json!({"type": "box", "from_line": 1, "to_line": 4, "label": "main"}),
    );
    live.command("theme terminal");
    let (symbols, styles) = live.snapshot(100, 20);
    assert!(symbols.contains("s is born"), "{symbols}");
    assert!(
        !styles.contains('#'),
        "`--theme terminal` drew a 24-bit colour:\n{styles}"
    );
    // The dark theme, for contrast, is all RGB.
    live.command("theme dark");
    let (_, dark) = live.snapshot(100, 20);
    assert!(dark.contains('#'), "{dark}");
}

#[test]
fn ac3_the_report_line_asks_for_what_we_need() {
    let s = section();
    let line = s
        .lines()
        .find(|l| l.starts_with("A terminal issue?"))
        .expect("report line");
    for needle in [
        "issues",
        "name and version",
        "$TERM $COLORTERM",
        "tmux or Herdr",
        "screenshot",
    ] {
        assert!(
            line.contains(needle),
            "report line lacks «{needle}»: {line}"
        );
    }
}
