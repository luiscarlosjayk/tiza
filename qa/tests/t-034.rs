//! t-034 — Terminal support: the claims that docs/reference/terminals.md
//! makes about the code stay true.
//!
//! The text of the page (what is verified, the caveats, the report line) is
//! a manual check: since the test rule of AGENTS.md, no test pins prose.
//! These tests pin the claims the page makes about the code: a plain (not
//! tmux-wrapped) OSC 11/10 query, no `COLORTERM` check, bracketed paste on,
//! mouse capture only with `--mouse`, no kitty keyboard flags, and
//! `--theme terminal` drawing without a single 24-bit colour. If one of
//! these changes, docs/reference/terminals.md has to change with it.

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
fn ac2_the_colour_query_is_plain_osc_not_wrapped_for_tmux() {
    // docs/reference/terminals.md says `allow-passthrough` changes neither
    // tmux 3.3a nor 3.5a: that holds only while the query is not a
    // `ESC P tmux;` passthrough
    // (t-139: wrapped, 3.5a loses the answer).
    assert_eq!(
        tiza_tui::appearance::QUERY,
        "\x1b]11;?\x1b\\\x1b]10;?\x1b\\"
    );
    for (path, src) in crate_sources() {
        assert!(
            !src.contains("Ptmux;"),
            "{} wraps something for tmux passthrough: update the tmux caveat in docs/reference/terminals.md",
            path.display()
        );
    }
}

#[test]
fn ac2_no_colorterm_check_and_no_kitty_keyboard_flags() {
    for (path, src) in crate_sources() {
        assert!(
            !src.contains("\"COLORTERM\""),
            "{} reads COLORTERM: docs/reference/terminals.md says there is no 256-colour fallback",
            path.display()
        );
        assert!(
            !src.contains("KeyboardEnhancementFlags"),
            "{} enables the kitty keyboard protocol: docs/reference/terminals.md says Tiza does not",
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
