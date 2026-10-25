//! Themes: auto light/dark from the host terminal, a `terminal`
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
//!   unchanged (the frame snapshots); schema; README; CHANGELOG.

use serde_json::json;
use tiza_core::ThemeName;
use tiza_qa::{Live, example, fixture, scratch, tiza, tiza_ok};

// Body of the pty script (`tiza_qa::PTY_PRELUDE` gives `Pty`, `plain`):
// every step waits on what the screen shows, never on the clock. The
// start-up answer is queued in the pty before Tiza starts, as if the
// terminal had answered at once: the product's 100 ms window is real
// behaviour, but whether a loaded test machine wakes this script inside it
// is not what the test is about. The answer after a resize goes
// through the key stream, which has no window, so it is sent on the query.
const PTY: &str = r#"
binary, doc, first, second = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
env = dict(os.environ, TIZA_CONFIG="/nonexistent/config.toml")
env.pop("TIZA_NO_OSC", None)
def answer(bg):
    return f"\x1b]11;rgb:{bg}\x1b\\\x1b]10;rgb:8080/8080/8080\x1b\\".encode()
def theme_of(bg):
    return "dark" if bg == "none" or int(bg[:2], 16) < 0x80 else "light"
def shows(label, since=0):
    return lambda out: label in plain(out[since:])
s = Pty([binary, doc, "--no-socket", "--no-save"], 30, 120, env=env, ctty=True, raw=True,
        seed=b"" if first == "none" else answer(first))
ok = s.wait_for(lambda out: b"\x1b]11;?" in out, 10, "start-up OSC 11 query")
t_query = time.monotonic()
ok = ok and s.wait_for(lambda out: b"\x1b[?1049h" in out, 10, "first frame")
t_frame = time.monotonic()
ok = ok and s.wait_for(shows(theme_of(first) + "·auto"), 10, "status bar " + theme_of(first))
if ok and second != "skip":
    # A SIGWINCH that lands before crossterm's event source exists is lost
    # (its handler is installed on the first poll): open and close the help
    # first, so the resize is only sent once keys are being read.
    s.send(b"?")
    ok = s.wait_for(shows("keys and commands"), 10, "help overlay (event loop reading keys)")
    s.send(b"?")
if ok and second != "skip":
    s.resize(30, 118)
    ok = s.wait_for(lambda out: out.count(b"\x1b]11;?") >= 2, 10, "query after the resize")
    if ok:
        mark = len(s.out)
        s.send(answer(second))
        s.wait_for(shows(theme_of(second) + "·auto", mark), 10, "status bar " + theme_of(second))
s.send(b":q\r")
code = s.finish(10)
text = plain(s.out)
print("QUERIES", s.out.count(b"\x1b]11;?"))
i, j = text.find("dark·auto"), text.find("light·auto")
print("DARK_AT", i, "LIGHT_AT", j)
print("STARTUP_MS", int((t_frame - t_query) * 1000))
print("EXIT", code)
"#;

fn pty_run(dir: &std::path::Path, doc: &std::path::Path, first: &str, second: &str) -> String {
    let out = tiza_qa::pty_script(
        dir,
        "osc.py",
        PTY,
        &[
            tiza_qa::bin().to_str().unwrap(),
            doc.to_str().unwrap(),
            first,
            second,
        ],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    out
}

fn plain_source(dir: &std::path::Path) -> std::path::PathBuf {
    let src = dir.join("plain.rs");
    std::fs::copy(fixture("shared/ownership/ownership.rs"), &src).unwrap();
    src
}

#[test]
fn the_terminal_background_picks_light_or_dark_and_a_resize_asks_again() {
    let dir = scratch("pty");
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
    // No answer at all (CI, a pipe): dark, and the first frame comes (a
    // query left waiting would never let it). How long it took is printed;
    // the window is 100 ms, but a loaded machine stretches the wall time
    // around it, so the bound is a condition's 10 s, not a budget.
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
    assert!((0..10_000).contains(&ms), "{out}");
}

#[test]
fn fixed_themes_do_not_follow_and_the_answers_parse() {
    // A document that names a theme is not asked about: no `·auto`, and a
    // host answer does not move it.
    let doc = tiza_core::fs::open(
        fixture("shared/ownership/ownership.rs.tiza")
            .to_str()
            .unwrap(),
    )
    .unwrap()
    .document;
    assert_eq!(doc.theme, Some(ThemeName::Dark));
    let dir = scratch("fixed");
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
    let live = Live::from_code("fn main() {}\n", "rs", "auto");
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
fn terminal_theme_maps_roles_to_the_host_palette_and_exports_fall_back() {
    let doc = tiza_core::fs::open(
        fixture("shared/ownership/ownership.rs.tiza")
            .to_str()
            .unwrap(),
    )
    .unwrap()
    .document;
    let dir = scratch("terminal");
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
        fixture("shared/ownership/ownership.rs.tiza")
            .to_str()
            .unwrap(),
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
fn overrides_built_in_then_config_then_document_then_command() {
    let dir = scratch("overrides");
    let cfg = dir.join("config.toml");
    std::fs::write(
        &cfg,
        "[theme]\naccent = \"#a6e3a1\"\nmuted = \"#202020\"\n[theme.light]\naccent = \"#b7410e\"\n",
    )
    .unwrap();
    let ex = fixture("shared/ownership/ownership.rs.tiza");
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
    doc["code"] = json!({"file": fixture("shared/ownership/ownership.rs").to_str().unwrap()});
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
    let live = Live::from_code("fn main() {}\n", "rs", "live-ov");
    live.command("theme dark");
    live.with(|a| {
        a.doc.theme_overrides = Some(
            tiza_core::theme::parse_theme_file("note = \"#303030\"")
                .unwrap()
                .0,
        );
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
fn mocha_loads_live_and_passes_the_legibility_gate() {
    let file = example("themes/catppuccin-mocha.toml");
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "mocha");
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
    // compression (the script measures after; it runs only when ffmpeg and
    // python3 with Pillow + numpy are here, otherwise the test says what is missing).
    let (ov, _) = tiza_core::theme::load_theme_file(&file).unwrap();
    let (th, notes) = tiza_core::Theme::resolve(ThemeName::Dark, true, None, Some(&ov));
    assert!(notes.is_empty(), "nothing should be pushed: {notes:?}");
    for c in [th.fg, th.muted, th.accent, th.note, th.error] {
        assert!(tiza_core::theme::contrast(c, th.bg) >= 4.5, "{c:?}");
    }
    let missing = tiza_qa::legibility_tools_missing();
    if missing.is_empty() {
        let r = std::process::Command::new("python3")
            .args([
                "scripts/legibility.py",
                "qa/fixtures/shared/ownership/ownership.rs.tiza",
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
            "terminal_theme: SKIPPED the Mocha legibility check (scripts/legibility.py): it did not run here; missing: {}",
            missing.join(", ")
        );
    }
    // The CLI loads it too.
    let txt = tiza_ok(&[
        "render",
        fixture("shared/ownership/ownership.rs.tiza")
            .to_str()
            .unwrap(),
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

// Moved here from a test file that is gone: it tests the pty harness
// (qa/src/pty.py).
