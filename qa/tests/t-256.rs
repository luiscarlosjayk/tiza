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
//! - AC5 A value that is not a color string leaves out only that token, in
//!   the config file and in a theme file, with a note. The config file
//!   names a key other than `theme`. A document with such a value does not
//!   open, and the error says what a token must be. A theme file with no
//!   color token to apply (empty, or no value is a string) is refused and
//!   changes nothing: the tokens of the document stay.
//!   open, and the error says what a token must be.
//! - AC6 MCP `export_image` applies the config file, as `tiza render`
//!   does. A config file that does not parse is said in the reply, and the
//!   export goes on.
//! - AC7 `tiza render --theme auto` exports the theme of the document (else
//!   `dark`), and `--theme terminal` exports `dark` and says so, as the help
//!   says.
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
    let (ov, _) = parse_theme_file(
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
    let (ov, _) = parse_theme_file("bg = \"#808080\"\nnote = \"#7a7a7a\"\n").unwrap();
    let (th, notes) = Theme::resolve(ThemeName::Dark, true, None, Some(&ov));
    assert!(th.note.0 < 0x7a, "not toward black: {:?}", th.note);
    assert!(contrast(th.note, th.bg) >= MIN_CODE_CONTRAST, "{notes:?}");
    // Every gray background, a token of the same gray: the token always
    // ends at 4.5:1 or more, as the notice says.
    for g in (0..=255u8).step_by(5) {
        let hex = format!("#{g:02x}{g:02x}{g:02x}");
        let (ov, _) = parse_theme_file(&format!("bg = \"{hex}\"\nnote = \"{hex}\"\n")).unwrap();
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

/// `tiza render main.rs ARGS` in `dir`: (exit 0, stdout, stderr).
fn render(dir: &Path, args: &[&str], config: &Path) -> (bool, String, String) {
    let out = std::process::Command::new(bin())
        .args(["render", "main.rs"])
        .args(args)
        .current_dir(dir)
        .env("TIZA_CONFIG", config)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Whether one line of `text` holds every part.
fn line_with(text: &str, parts: &[&str]) -> bool {
    text.lines().any(|l| parts.iter().all(|p| l.contains(p)))
}

#[test]
fn ac5_a_value_that_is_not_a_color_leaves_out_only_that_token() {
    let dir = scratch("t256-values");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let svg = ["--format", "svg", "--no-embed-fonts"];
    // The config file: the good token applies, each problem has its line.
    let cfg = dir.join("config.toml");
    std::fs::write(
        &cfg,
        "[theme]\naccent = 3\nbg = \"#123456\"\n[theme.dark]\nfg = true\n[them]\nx = 1\n",
    )
    .unwrap();
    let (ok, out, err) = render(&dir, &svg, &cfg);
    assert!(ok, "{err}");
    assert!(
        out.contains("#123456"),
        "the good token did not apply\n{err}"
    );
    assert!(!err.contains("config ignored"), "{err}");
    assert!(
        line_with(&err, &["tiza: config:", "accent", "integer"]),
        "{err}"
    );
    assert!(
        line_with(&err, &["tiza: config:", "[dark] fg", "boolean"]),
        "{err}"
    );
    assert!(
        line_with(&err, &["tiza: config:", "unknown key them "]),
        "{err}"
    );
    // A theme file: the same rule, in `tiza render` and in `:theme file`.
    std::fs::write(dir.join("m.toml"), "accent = 3\nbg = \"#654321\"\n").unwrap();
    let none = dir.join("no-config.toml");
    let mut args = svg.to_vec();
    args.extend(["--theme-file", "m.toml"]);
    let (ok, out, err) = render(&dir, &args, &none);
    assert!(ok, "{err}");
    assert!(out.contains("#654321"), "{err}");
    assert!(line_with(&err, &["tiza:", "accent", "integer"]), "{err}");
    let live = tiza_qa::Live::from_code(CODE, "rs", "t256-values-live");
    live.command(&format!("theme file {}", dir.join("m.toml").display()));
    let (msg, error) = live.message().unwrap();
    assert!(!error, "{msg}");
    assert!(msg.contains("accent") && msg.contains("integer"), "{msg}");
    assert_eq!(
        live.with(|a| a.theme.bg),
        tiza_core::canvas::Rgb(0x65, 0x43, 0x21)
    );
}

#[test]
fn ac5_a_document_with_such_a_value_says_what_a_token_must_be() {
    let dir = scratch("t256-doc-value");
    let base = serde_json::to_value(tiza_core::Document::from_code(CODE, "rs", None)).unwrap();
    // At the top and inside a subtable.
    for (case, overrides) in [
        ("top", serde_json::json!({"accent": 3})),
        ("subtable", serde_json::json!({"dark": {"note": 1}})),
    ] {
        let mut doc = base.clone();
        doc["theme_overrides"] = overrides;
        let path = dir.join(format!("{case}.tiza"));
        std::fs::write(&path, doc.to_string()).unwrap();
        let out = std::process::Command::new(bin())
            .args(["check", path.to_str().unwrap()])
            .output()
            .unwrap();
        // `tiza check` reports on stdout.
        let err = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(1), "{case}: {err}");
        assert!(err.contains("expected a color string"), "{case}: {err}");
        assert!(!err.contains("untagged"), "{case}: {err}");
    }
}

#[test]
fn ac5_a_theme_file_with_no_token_to_apply_changes_nothing() {
    let dir = scratch("t256-no-token");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    std::fs::write(dir.join("good.toml"), "note = \"#303030\"\n").unwrap();
    std::fs::write(dir.join("bad.toml"), "accent = 3\n[dark]\nnote = true\n").unwrap();
    std::fs::write(dir.join("empty.toml"), "").unwrap();
    let live = tiza_qa::Live::from_code(CODE, "rs", "t256-no-token-live");
    live.command(&format!("theme file {}", dir.join("good.toml").display()));
    let path = live.with(|a| a.save_path.clone());
    let tokens = live.with(|a| a.doc.theme_overrides.clone());
    assert!(tokens.is_some());
    let bytes = std::fs::read(&path).unwrap();
    let time = std::fs::metadata(&path).unwrap().modified().unwrap();
    for (name, notes) in [("bad.toml", true), ("empty.toml", false)] {
        live.command(&format!("theme file {}", dir.join(name).display()));
        let (msg, error) = live.message().unwrap();
        assert!(
            error && msg.contains("no color token to apply"),
            "{name}: {msg}"
        );
        if notes {
            assert!(
                msg.contains("accent") && msg.contains("[dark] note"),
                "{name}: {msg}"
            );
        }
        assert_eq!(
            live.with(|a| a.doc.theme_overrides.clone()),
            tokens,
            "{name}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "{name}");
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            time,
            "{name}"
        );
    }
    let (ok, _, err) = render(
        &dir,
        &["--theme-file", "bad.toml"],
        &dir.join("no-config.toml"),
    );
    assert!(!ok && err.contains("no color token to apply"), "{err}");
}

#[test]
fn ac6_export_image_uses_the_config_file_and_a_broken_one_does_not_stop_it() {
    let dir = scratch("t256-export-config");
    let cfg = dir.join("config.toml");
    std::fs::write(&cfg, "[theme]\nbg = \"#123456\"\n").unwrap();
    let live = tiza_qa::Live::from_code(CODE, "rs", "t256-export-live");
    let mut mcp =
        tiza_qa::McpClient::start_with_env(&live.socket, &[("TIZA_CONFIG", cfg.to_str().unwrap())]);
    let export = |mcp: &mut tiza_qa::McpClient, name: &str| {
        let out = dir.join(name);
        let (err, text, _) = mcp.call(
            "export_image",
            serde_json::json!({"path": out.to_str().unwrap()}),
        );
        assert!(!err, "{text}");
        let reply: serde_json::Value = serde_json::from_str(&text).unwrap();
        (std::fs::read_to_string(&out).unwrap(), reply)
    };
    let (svg, reply) = export(&mut mcp, "a.svg");
    assert!(svg.contains("#123456"), "the config bg did not apply");
    assert!(reply.get("warning").is_none(), "{reply}");
    // A config that does not parse: the export goes on, and says so.
    std::fs::write(&cfg, "[theme\n").unwrap();
    let (svg, reply) = export(&mut mcp, "b.svg");
    assert!(!svg.contains("#123456"));
    let warning = reply["warning"].as_str().unwrap_or_default();
    assert!(warning.contains("config ignored"), "{reply}");
}

#[test]
fn ac7_render_with_theme_auto_exports_the_theme_of_the_document() {
    use tiza_core::{Theme, ThemeName};
    let dir = scratch("t256-render-auto");
    let mut doc = serde_json::to_value(tiza_core::Document::from_code(CODE, "rs", None)).unwrap();
    doc["theme"] = serde_json::json!("light");
    let path = dir.join("d.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let hex = |name| {
        let c = Theme::from_name(name).bg;
        format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
    };
    let (light, dark) = (hex(ThemeName::Light), hex(ThemeName::Dark));
    let out = std::process::Command::new(bin())
        .args([
            "render",
            path.to_str().unwrap(),
            "--format",
            "svg",
            "--no-embed-fonts",
        ])
        .args(["--theme", "auto"])
        .env("TIZA_CONFIG", dir.join("no-config.toml"))
        .output()
        .unwrap();
    let svg = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(svg.contains(&light), "not the light bg {light}");
    assert!(!svg.contains(&dark), "the dark bg {dark}");
    // terminal: dark, and the export says so.
    let out = std::process::Command::new(bin())
        .args([
            "render",
            path.to_str().unwrap(),
            "--format",
            "svg",
            "--no-embed-fonts",
        ])
        .args(["--theme", "terminal"])
        .env("TIZA_CONFIG", dir.join("no-config.toml"))
        .output()
        .unwrap();
    let (svg, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{err}");
    assert!(svg.contains(&dark), "not the dark bg {dark}");
    assert!(err.contains("exported with dark"), "{err}");
}
