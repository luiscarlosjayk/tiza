//! t-119 — Themes: auto light/dark from the host terminal, a `terminal`
//! theme on the ANSI palette, per-token overrides, Catppuccin Mocha as the
//! first override example.
//!
//! Acceptance criteria:
//! - AC1 On start (unless the document or `--theme` fixes one) Tiza asks the
//!   terminal (OSC 11/10), picks light or dark by luminance, re-asks on a
//!   resize, shows `·auto`; 100 ms timeout, silent fallback to dark.
//! - AC2 `:theme terminal` / `--theme terminal`: the host's palette; exports
//!   fall back to dark/light with a notice.
//! - AC3 Overrides: `[theme]` in the config file and `theme_overrides` in
//!   the document; built-in → config → document → `:theme`; faint text is
//!   pushed to 4.5:1 and reported.
//! - AC4 `examples/themes/catppuccin-mocha.toml` loads with `--theme-file`
//!   / `:theme file`, and passes the legibility gate.
//! - AC5 Documents that name dark/light and users without a config file:
//!   unchanged (t-053 snapshots); schema; README; CHANGELOG.

use serde_json::json;
use tiza_core::ThemeName;
use tiza_qa::{Live, example, scratch, tiza, tiza_ok};

const PTY: &str = r#"
import os, pty, subprocess, sys, time, fcntl, termios, struct, re, signal
binary, doc, first, second = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 120, 0, 0))
def ctty():
    os.setsid(); fcntl.ioctl(0, termios.TIOCSCTTY, 0)
env = dict(os.environ, TIZA_CONFIG="/nonexistent/config.toml")
p = subprocess.Popen([binary, doc, "--no-socket", "--no-save"], stdin=slave, stdout=slave,
                     stderr=slave, close_fds=True, preexec_fn=ctty, env=env)
os.close(slave)
def answer(bg):
    if bg != "none":
        os.write(master, f"\x1b]11;rgb:{bg}\x1b\\\x1b]10;rgb:8080/8080/8080\x1b\\".encode())
out = b""; t0 = time.time(); stage = 0; t_query = None
while time.time() - t0 < 10:
    try: chunk = os.read(master, 65536)
    except OSError: break
    if not chunk: break
    out += chunk
    if stage == 0 and b"\x1b]11;?" in out:
        t_query = time.time(); answer(first); stage = 1; time.sleep(0.8)
        if second == "skip":
            os.write(master, b":q\r"); stage = 3
        else:
            fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 118, 0, 0))
            os.killpg(p.pid, signal.SIGWINCH)
    elif stage == 1 and out.count(b"\x1b]11;?") >= 2:
        answer(second); stage = 2; time.sleep(0.8)
        os.write(master, b"jj:q\r"); stage = 3
try: p.wait(timeout=5)
except Exception: p.kill(); p.wait()
text = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", out.decode(errors="replace"))
print("QUERIES", out.count(b"\x1b]11;?"))
i, j = text.find("dark·auto"), text.find("light·auto")
print("DARK_AT", i, "LIGHT_AT", j)
print("STARTUP_MS", int((t_query - t0) * 1000) if t_query else -1)
print("EXIT", p.returncode)
"#;

fn pty_run(dir: &std::path::Path, doc: &std::path::Path, first: &str, second: &str) -> String {
    let script = dir.join("osc.py");
    std::fs::write(&script, PTY).unwrap();
    let out = std::process::Command::new("python3")
        .args([
            "-I",
            script.to_str().unwrap(),
            tiza_qa::bin().to_str().unwrap(),
            doc.to_str().unwrap(),
            first,
            second,
        ])
        .output()
        .expect("python3");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn plain_source(dir: &std::path::Path) -> std::path::PathBuf {
    let src = dir.join("plain.rs");
    std::fs::copy(example("ownership.rs"), &src).unwrap();
    src
}

#[test]
fn ac1_the_terminal_background_picks_light_or_dark_and_a_resize_asks_again() {
    let dir = scratch("t119-pty");
    let src = plain_source(&dir);
    // A light terminal: Tiza opens light and says `light·auto`.
    let out = pty_run(&dir, &src, "f7f7/f5f5/efef", "skip");
    assert!(out.contains("QUERIES 1") && out.contains("EXIT 0"), "{out}");
    assert!(
        out.contains("DARK_AT -1 LIGHT_AT") && !out.contains("LIGHT_AT -1"),
        "{out}"
    );
    // A dark one, then the terminal switches to light with a resize: the
    // second query is answered light and the theme follows.
    let out = pty_run(&dir, &src, "1515/1717/1b1b", "f7f7/f5f5/efef");
    assert!(out.contains("QUERIES 2") && out.contains("EXIT 0"), "{out}");
    let dark_at: i64 = out
        .lines()
        .find(|l| l.starts_with("DARK_AT"))
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap()
        .parse()
        .unwrap();
    let light_at: i64 = out
        .lines()
        .find(|l| l.starts_with("DARK_AT"))
        .and_then(|l| l.split_whitespace().nth(3))
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        dark_at >= 0 && light_at > dark_at,
        "dark first, then light: {out}"
    );
    // No answer at all (CI, a pipe): dark, and the start-up query cost
    // well under a second.
    let out = pty_run(&dir, &src, "none", "skip");
    assert!(
        !out.contains("DARK_AT -1") && out.contains("LIGHT_AT -1") && out.contains("EXIT 0"),
        "{out}"
    );
    let ms: i64 = out
        .lines()
        .find(|l| l.starts_with("STARTUP_MS"))
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap()
        .parse()
        .unwrap();
    assert!((0..1500).contains(&ms), "{out}");
}

#[test]
fn ac1_fixed_themes_do_not_follow_and_the_answers_parse() {
    // A document that names a theme is not asked about: no `·auto`, and a
    // host answer does not move it.
    let doc = tiza_core::fs::open(example("ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document;
    assert_eq!(doc.theme, Some(ThemeName::Dark));
    let dir = scratch("t119-fixed");
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    assert!(!live.with(|a| a.theme_auto));
    live.with(|a| {
        a.set_host(tiza_tui::appearance::Host {
            bg: Some(tiza_core::canvas::Rgb(0xF7, 0xF5, 0xEF)),
            fg: None,
        })
    });
    assert_eq!(live.with(|a| a.theme.name), ThemeName::Dark);
    assert!(
        live.screen(100, 30).contains("  dark  "),
        "{}",
        live.screen(100, 30)
    );
    // `:theme auto` follows again (the last answer was light), the document
    // stops naming a theme, and `t` fixes it again.
    live.command("theme auto");
    assert!(live.message().unwrap().0.contains("light now"));
    assert_eq!(
        live.with(|a| (a.theme.name, a.theme_auto, a.doc.theme)),
        (ThemeName::Light, true, None)
    );
    assert_eq!(live.with(|a| a.theme_status()), "light·auto");
    live.with(|a| a.message = None);
    assert!(
        live.screen(100, 30).contains("light·auto"),
        "{}",
        live.screen(100, 30)
    );
    live.keys(&[crossterm::event::KeyCode::Char('t')]);
    assert_eq!(
        live.with(|a| (a.theme.name, a.theme_auto, a.doc.theme)),
        (ThemeName::Dark, false, Some(ThemeName::Dark))
    );
    // A document with no theme follows from the start; saving keeps it so.
    let live = Live::from_code("fn main() {}\n", "rs", "t119-auto");
    assert!(live.with(|a| a.theme_auto));
    live.command("note 1 x");
    live.command("w");
    let saved = live.with(|a| std::fs::read_to_string(&a.save_path).unwrap());
    assert!(!saved.contains("\"theme\""), "{saved}");
    live.command("theme light");
    live.command("w");
    let saved = live.with(|a| std::fs::read_to_string(&a.save_path).unwrap());
    assert!(saved.contains("\"theme\": \"light\""), "{saved}");
}

#[test]
fn ac2_terminal_theme_maps_roles_to_the_host_palette_and_exports_fall_back() {
    let doc = tiza_core::fs::open(example("ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let dir = scratch("t119-terminal");
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 7}));
    live.command("theme terminal");
    assert!(live.message().unwrap().0.starts_with("theme terminal"));
    let th = live.with(|a| a.theme.clone());
    assert!(th.is_terminal());
    assert_eq!(
        tiza_core::theme::host_color(th.bg),
        Some(tiza_core::theme::HostColor::Default)
    );
    assert_eq!(th.named_color("blue"), Some(tiza_core::theme::ansi(4)));
    // The screen draws with Reset / Indexed colours, never a sentinel RGB.
    let (symbols, styles) = live.snapshot(100, 40);
    assert!(symbols.contains("terminal"), "{symbols}");
    assert!(
        !styles.contains("fea0") && !styles.contains("fea1"),
        "{styles}"
    );
    assert!(
        styles.contains("indexed(") && styles.contains("reset/reset"),
        "host colours expected: {styles}"
    );
    // Exports fall back to dark with a notice (the text formats too: the
    // ANSI one would otherwise carry the sentinels).
    let out = dir.join("x.ansi");
    live.command(&format!("export {} width=100", out.display()));
    let (m, err) = live.message().unwrap();
    assert!(!err && m.contains("exported with dark"), "{m}");
    let ansi = std::fs::read_to_string(&out).unwrap();
    assert!(
        !ansi.contains("254;160;") && !ansi.contains("254;161;"),
        "sentinels leaked"
    );
    assert!(ansi.contains("21;23;27"), "dark background");
    // `t` from terminal goes to the theme that matches the host.
    live.keys(&[crossterm::event::KeyCode::Char('t')]);
    assert_eq!(live.with(|a| a.theme.name), ThemeName::Dark);
    // The CLI: `render --theme terminal` says so and paints dark.
    let r = tiza(&[
        "render",
        example("ownership.rs.tiza").to_str().unwrap(),
        "--theme",
        "terminal",
        "--format",
        "svg",
        "--no-embed-fonts",
        "--width",
        "80",
    ]);
    assert!(r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("exported with dark"));
}

#[test]
fn ac3_overrides_built_in_then_config_then_document_then_command() {
    let dir = scratch("t119-overrides");
    let cfg = dir.join("config.toml");
    std::fs::write(
        &cfg,
        "[theme]\naccent = \"#a6e3a1\"\nmuted = \"#202020\"\n[theme.light]\naccent = \"#b7410e\"\n",
    )
    .unwrap();
    let ex = example("ownership.rs.tiza");
    let svg = |args: &[&str]| {
        let r = std::process::Command::new(tiza_qa::bin())
            .args([
                "render",
                ex.to_str().unwrap(),
                "--format",
                "svg",
                "--no-embed-fonts",
                "--width",
                "100",
                "--step",
                "7",
            ])
            .args(args)
            .env("TIZA_CONFIG", &cfg)
            .output()
            .unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        (
            String::from_utf8_lossy(&r.stdout).into_owned(),
            String::from_utf8_lossy(&r.stderr).into_owned(),
        )
    };
    // Config: the accent is green; a faint muted is pushed and reported.
    let (s, e) = svg(&[]);
    assert!(s.contains("#a6e3a1"), "config accent");
    assert!(e.contains("theme muted:") && e.contains("pushed"), "{e}");
    // The light subtable applies only on light.
    let (s, _) = svg(&["--theme", "light"]);
    assert!(s.contains("#b7410e") && !s.contains("#a6e3a1"));
    // The document's own table wins over the config's.
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&ex).unwrap()).unwrap();
    doc["theme_overrides"] = json!({"accent": "#89b4fa"});
    doc["code"] = json!({"file": example("ownership.rs").to_str().unwrap()});
    let path = dir.join("doc.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let r = std::process::Command::new(tiza_qa::bin())
        .args([
            "render",
            path.to_str().unwrap(),
            "--format",
            "svg",
            "--no-embed-fonts",
            "--width",
            "100",
            "--step",
            "7",
        ])
        .env("TIZA_CONFIG", &cfg)
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&r.stdout);
    assert!(
        s.contains("#89b4fa") && !s.contains("#a6e3a1"),
        "document wins"
    );
    // `--theme-file` replaces the document's table for one export.
    let (s, _) = {
        let r = std::process::Command::new(tiza_qa::bin())
            .args([
                "render",
                path.to_str().unwrap(),
                "--format",
                "svg",
                "--no-embed-fonts",
                "--width",
                "100",
                "--step",
                "7",
                "--theme-file",
                example("themes/catppuccin-mocha.toml").to_str().unwrap(),
            ])
            .env("TIZA_CONFIG", &cfg)
            .output()
            .unwrap();
        (String::from_utf8_lossy(&r.stdout).into_owned(), ())
    };
    assert!(s.contains("#1e1e2e") && s.contains("#fab387"), "Mocha");
    // A broken config is reported once and ignored, never fatal.
    std::fs::write(&cfg, "[theme\naccent = 1\n").unwrap();
    let r = std::process::Command::new(tiza_qa::bin())
        .args([
            "render",
            ex.to_str().unwrap(),
            "--format",
            "txt",
            "--width",
            "80",
        ])
        .env("TIZA_CONFIG", &cfg)
        .output()
        .unwrap();
    assert!(r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("config ignored"));
    // Live: scene.patch cannot set overrides, but the document field does
    // through scene.put, and `:theme` reports what it pushed.
    let live = Live::from_code("fn main() {}\n", "rs", "t119-live-ov");
    live.command("theme dark");
    live.with(|a| {
        a.doc.theme_overrides =
            Some(tiza_core::theme::parse_theme_file("note = \"#303030\"").unwrap());
        a.rebuild_theme();
    });
    live.command("theme dark");
    let (m, _) = live.message().unwrap();
    assert!(m.contains("theme note:") && m.contains("pushed"), "{m}");
    assert!(
        tiza_core::theme::contrast(live.with(|a| a.theme.note), live.with(|a| a.theme.bg)) >= 4.5
    );
}

#[test]
fn ac4_mocha_loads_live_and_passes_the_legibility_gate() {
    let file = example("themes/catppuccin-mocha.toml");
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "t119-mocha");
    live.command(&format!("theme file {}", file.display()));
    let (m, err) = live.message().unwrap();
    assert!(!err && m.contains("token(s) applied"), "{m}");
    assert_eq!(
        live.with(|a| a.theme.bg),
        tiza_core::canvas::Rgb::hex(0x1E1E2E)
    );
    assert_eq!(
        live.with(|a| a.theme.named_color("purple")),
        Some(tiza_core::canvas::Rgb::hex(0xCBA6F7))
    );
    // Saved with the document: the next session has it.
    live.command("w");
    let saved = live.with(|a| std::fs::read_to_string(&a.save_path).unwrap());
    assert!(
        saved.contains("\"theme_overrides\"") && saved.contains("#fab387"),
        "{saved}"
    );
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert!(live.message().unwrap().0.contains("undone: theme file"));
    assert_ne!(
        live.with(|a| a.theme.bg),
        tiza_core::canvas::Rgb::hex(0x1E1E2E)
    );
    live.command("theme file /nonexistent.toml");
    assert!(live.message().unwrap().1);
    // Every text role of the file reads at 4.5:1 on its background before
    // compression (the script measures after; it runs when ffmpeg is here).
    let ov = tiza_core::theme::load_theme_file(&file).unwrap();
    let (th, notes) = tiza_core::Theme::resolve(ThemeName::Dark, true, None, Some(&ov));
    assert!(notes.is_empty(), "nothing should be pushed: {notes:?}");
    for c in [th.fg, th.muted, th.accent, th.note, th.error] {
        assert!(tiza_core::theme::contrast(c, th.bg) >= 4.5, "{c:?}");
    }
    let have_ffmpeg = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if have_ffmpeg {
        let r = std::process::Command::new("python3")
            .args([
                "scripts/legibility.py",
                "examples/ownership.rs.tiza",
                "--step",
                "7",
                "--theme-file",
                "examples/themes/catppuccin-mocha.toml",
                "--bin",
                tiza_qa::bin().to_str().unwrap(),
            ])
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&r.stdout);
        assert!(
            r.status.success() && out.contains("OK"),
            "{out}\n{}",
            String::from_utf8_lossy(&r.stderr)
        );
    } else {
        eprintln!(
            "t-119: ffmpeg missing here; the Mocha legibility run is part of the local gate on a machine with it"
        );
    }
    // The CLI loads it too.
    let txt = tiza_ok(&[
        "render",
        example("ownership.rs.tiza").to_str().unwrap(),
        "--theme-file",
        file.to_str().unwrap(),
        "--format",
        "svg",
        "--no-embed-fonts",
        "--width",
        "80",
    ]);
    assert!(txt.contains("#1e1e2e"));
}

#[test]
fn ac5_nothing_changes_without_a_config_file_or_overrides() {
    let doc = tiza_core::fs::open(example("ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let (th, notes) =
        tiza_core::Theme::resolve(ThemeName::Dark, true, None, doc.theme_overrides.as_ref());
    assert!(notes.is_empty());
    let plain = tiza_core::Theme::dark();
    assert_eq!(
        (th.bg, th.fg, th.accent, th.note),
        (plain.bg, plain.fg, plain.accent, plain.note)
    );
    assert!(th.palette.is_empty());
}
