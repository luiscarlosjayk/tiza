//! Theme bugs that the check of the themes page found.
//!
//! Acceptance criteria:
//! - AC1 With `--no-save`, `--theme-file` does not write the `.tiza`
//!   document at start: no new file, and a file that is there keeps its
//!   bytes. Without `--no-save` it writes the tokens at start, as before.
//! - AC2 A `--theme-file` on the command line is not an edit that `u`
//!   takes back, as for `--theme`: after `u` the tokens stay.
//! - AC3 The `[dark]`, `[light]` or `[terminal]` subtable of the theme in
//!   use wins over the flat tokens, whatever the names of the keys.
//! - AC4 A text token that reads under 4.5:1 on the background moves toward
//!   black or white, whichever contrasts more, until it reads at 4.5:1:
//!   the notice that says so is always true.
//!
//! Corner cases: `:theme file PATH` keeps its undo entry; `--theme-file`
//! still says how many tokens it applied.

use std::path::Path;

use tiza_qa::{bin, scratch};

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

/// Runs `tiza --smoke ARGS hello.rs` (one frame, then exit) on a pty in
/// `dir` and returns the frame as text.
fn smoke(dir: &Path, name: &str, args: &[&str]) -> String {
    let body = r##"
import json
binary, cwd, args = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--smoke", "--no-socket", *args, "hello.rs"], 30, 200, env=env)
print("EXIT", s.finish(10))
print("SCREEN", json.dumps(plain(s.out)))
"##;
    let out = tiza_qa::pty_script(
        dir,
        name,
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &serde_json::to_string(args).unwrap(),
        ],
    );
    assert!(out.contains("EXIT 0"), "{out}");
    let screen = out.lines().find_map(|l| l.strip_prefix("SCREEN ")).unwrap();
    serde_json::from_str(screen).unwrap()
}

#[test]
fn ac1_with_no_save_the_theme_file_writes_no_document() {
    let dir = scratch("t256-no-save");
    std::fs::write(dir.join("hello.rs"), CODE).unwrap();
    std::fs::write(dir.join("m.toml"), "note = \"#303030\"\n").unwrap();
    let sidecar = dir.join("hello.rs.tiza");
    let applied = "theme file m.toml: 1 token(s) applied";
    // No document yet: none after the start.
    let screen = smoke(&dir, "none.py", &["--no-save", "--theme-file", "m.toml"]);
    assert!(screen.contains(applied), "{screen}");
    assert!(!sidecar.exists(), "--no-save wrote {}", sidecar.display());
    // Without --no-save the tokens are written at start, as before: the
    // test sees a write when there is one.
    smoke(&dir, "save.py", &["--theme-file", "m.toml"]);
    let saved = std::fs::read(&sidecar).unwrap();
    assert!(
        String::from_utf8_lossy(&saved).contains("#303030"),
        "no tokens in the document"
    );
    let mtime = std::fs::metadata(&sidecar).unwrap().modified().unwrap();
    // A document that is there keeps its bytes, also with other tokens.
    std::fs::write(dir.join("m.toml"), "note = \"#404040\"\n").unwrap();
    let screen = smoke(&dir, "kept.py", &["--no-save", "--theme-file", "m.toml"]);
    assert!(screen.contains(applied), "{screen}");
    assert_eq!(std::fs::read(&sidecar).unwrap(), saved);
    assert_eq!(
        std::fs::metadata(&sidecar).unwrap().modified().unwrap(),
        mtime
    );
}

#[test]
fn ac2_undo_after_start_keeps_the_theme_file_tokens() {
    let dir = scratch("t256-undo");
    std::fs::write(dir.join("hello.rs"), CODE).unwrap();
    std::fs::write(dir.join("m.toml"), "note = \"#303030\"\n").unwrap();
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--no-save", "--socket", "s.sock", "--theme-file", "m.toml", "hello.rs"], 30, 120, env=env)
def scene():
    r = subprocess.run([binary, "api", "scene.get", "--socket", "s.sock"], capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout)["document"] if r.returncode == 0 else {}
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    # `u`, then a command whose effect the API shows: once it shows, the
    # `u` before it was handled.
    s.send(b"u:margin 40\r")
    end = time.monotonic() + 10
    doc = scene()
    while doc.get("margin") != 40 and time.monotonic() < end:
        s.settle(0.05)
        doc = scene()
    print("MARGIN", doc.get("margin"))
    print("TOKENS", json.dumps(doc.get("theme_overrides")))
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "undo.py",
        body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(out.contains("MARGIN 40"), "{out}");
    let tokens = out
        .lines()
        .find_map(|l| l.strip_prefix("TOKENS "))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(tokens.contains("#303030"), "after u: {tokens}\n{out}");
}

#[test]
fn ac3_the_subtable_of_the_theme_in_use_wins_over_the_flat_tokens() {
    use tiza_core::ThemeName;
    use tiza_core::canvas::Rgb;
    use tiza_core::theme::{Theme, parse_theme_file};
    // `box_bg` sorts before `dark`, `highlight_bg` between `dark` and
    // `light`, `status_bg` after `light`. None of them is pushed for
    // contrast.
    let ov = parse_theme_file(
        "box_bg = \"#010101\"\nhighlight_bg = \"#020202\"\nstatus_bg = \"#030303\"\n\
         [dark]\nbox_bg = \"#110000\"\nhighlight_bg = \"#120000\"\nstatus_bg = \"#130000\"\n\
         [light]\nbox_bg = \"#001100\"\nhighlight_bg = \"#001200\"\nstatus_bg = \"#001300\"\n",
    )
    .unwrap();
    let tokens = |t: &Theme| (t.box_bg, t.highlight_bg, t.status_bg);
    let (dark, notes) = Theme::resolve(ThemeName::Dark, true, None, Some(&ov));
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(
        tokens(&dark),
        (Rgb(0x11, 0, 0), Rgb(0x12, 0, 0), Rgb(0x13, 0, 0))
    );
    let (light, _) = Theme::resolve(ThemeName::Light, false, None, Some(&ov));
    assert_eq!(
        tokens(&light),
        (Rgb(0, 0x11, 0), Rgb(0, 0x12, 0), Rgb(0, 0x13, 0))
    );
}

#[test]
fn ac4_a_faint_token_moves_toward_the_anchor_that_contrasts_more() {
    use tiza_core::ThemeName;
    use tiza_core::canvas::Rgb;
    use tiza_core::theme::{MIN_CODE_CONTRAST, Theme, contrast, parse_theme_file};
    // #808080: black reads at 5.3:1, white at 3.9:1.
    let ov = parse_theme_file("bg = \"#808080\"\nnote = \"#7a7a7a\"\n").unwrap();
    let (th, notes) = Theme::resolve(ThemeName::Dark, true, None, Some(&ov));
    assert!(th.note.0 < 0x7a, "not toward black: {:?}", th.note);
    assert!(contrast(th.note, th.bg) >= MIN_CODE_CONTRAST, "{notes:?}");
    // Every gray background, a token of the same gray: the token always
    // ends at 4.5:1 or more, as the notice says.
    for g in (0..=255u8).step_by(5) {
        let hex = format!("#{g:02x}{g:02x}{g:02x}");
        let ov = parse_theme_file(&format!("bg = \"{hex}\"\nnote = \"{hex}\"\n")).unwrap();
        for name in [ThemeName::Dark, ThemeName::Light] {
            let (th, notes) = Theme::resolve(name, true, None, Some(&ov));
            assert_eq!(th.bg, Rgb(g, g, g));
            let c = contrast(th.note, th.bg);
            assert!(c >= MIN_CODE_CONTRAST, "{hex}: {c:.2}:1 {notes:?}");
            assert!(
                notes.iter().any(|n| n.contains("for 4.5:1")),
                "{hex}: {notes:?}"
            );
        }
    }
}
