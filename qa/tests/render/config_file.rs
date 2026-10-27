//! The config file as a whole: `tiza config` shows the file in use, each
//! value with its source and the warnings; a bad entry or a line that does
//! not parse is a warning with its line and key, and the rest of the file
//! applies; Tiza always starts, and `tiza config` exits with 1 only when it
//! cannot read the file.

use tiza_qa::{fixture, scratch, tiza_with_config};

/// A scratch folder with `file.rs` and `cfg.toml` holding `text`.
fn setup(name: &str, text: &[u8]) -> std::path::PathBuf {
    let dir = scratch(name);
    std::fs::copy(
        fixture("shared/personal_config/file.rs"),
        dir.join("file.rs"),
    )
    .unwrap();
    std::fs::write(dir.join("cfg.toml"), text).unwrap();
    dir
}

#[test]
fn a_broken_line_warns_and_the_rest_of_the_file_applies() {
    // (case, file, the warning that names its line, the row of `tiza
    // config` that shows that the rest applies)
    let cases: &[(&str, &[u8], &str, &str)] = &[
        (
            "a value that is missing",
            b"[defaults]\nmouse = true\nnumbers = \nwidth = 70\n",
            "cfg.toml:3: [defaults] numbers:",
            "  mouse     on       config",
        ),
        (
            "a byte that is not UTF-8",
            b"# caf\xe9 au lait\n[defaults]\nnumbers = false\nmouse = \"o\xffn\"\n",
            "cfg.toml:4: this line is not valid UTF-8",
            "  numbers   off      config",
        ),
        (
            "a key that is in the file two times",
            b"[defaults]\nnumbers = false\nnumbers = true\n[defaults]\nmouse = true\n",
            "cfg.toml:3: [defaults] numbers: duplicate key, the first value is used",
            "  numbers   off      config",
        ),
        (
            "a table header that is not closed",
            b"[theme]\naccent = \"#ffffff\"\n[defaults\nnumbers = false\n",
            "cfg.toml:3: unclosed table, expected `]`: the table header has an error, so Tiza ignores lines 3-4",
            "theme (config): accent = #ffffff\n",
        ),
    ];
    for (case, text, warning, applies) in cases {
        let dir = setup("config-broken-line", text);
        let (code, out, err) = tiza_with_config(&dir, "cfg.toml", &["config"]);
        assert_eq!(code, 0, "{case}: the file can be read: {err}");
        assert!(out.contains(warning), "{case}: no «{warning}» in:\n{out}");
        assert!(out.contains(applies), "{case}: no «{applies}» in:\n{out}");
        let (code, _, err) =
            tiza_with_config(&dir, "cfg.toml", &["render", "file.rs", "--format", "txt"]);
        assert_eq!(code, 0, "{case}: {err}");
        assert!(
            err.contains(&format!("tiza: config: {warning}")),
            "{case}: render says it too:\n{err}"
        );
    }
    // Under a broken header, no entry goes to the table above it: the
    // `numbers` that follows `[defaults` is not a token of `[theme]`.
    let dir = setup(
        "config-broken-header",
        b"[theme]\naccent = \"#ffffff\"\n[defaults\nnumbers = false\n",
    );
    let (_, out, _) = tiza_with_config(&dir, "cfg.toml", &["config"]);
    assert!(out.contains("  numbers   on       built-in"), "{out}");
    assert!(!out.contains("numbers = false"), "{out}");
}

#[test]
fn tiza_config_shows_the_file_the_values_and_the_warnings() {
    let mut text = std::fs::read(fixture("shared/personal_config/bad-key-and-value.toml")).unwrap();
    text.extend(std::fs::read(fixture("shared/personal_config/personal-styles.toml")).unwrap());
    let dir = setup("config-report", &text);
    let (code, out, err) = tiza_with_config(&dir, "cfg.toml", &["config"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.starts_with("file: cfg.toml\n"), "{out}");
    for row in [
        "  pad       0 1      built-in",
        "  numbers   off      config",
        "  mouse     off      built-in",
        "  autosave  on       built-in",
        "  layout    dynamic  built-in",
        "  width     100      built-in",
        "  io ⇄  network I/O  @cyan",
    ] {
        assert!(out.contains(&format!("{row}\n")), "{row:?} in:\n{out}");
    }
    assert!(out.contains("warnings (3):"), "{out}");
    for warning in [
        "  cfg.toml:3: [defaults] colour: unknown key",
        "  cfg.toml:4: [defaults] width: \"wide\" is not valid",
        "[styles] warn: a built-in kind has this name",
    ] {
        assert!(out.contains(warning), "{warning} in:\n{out}");
    }
    assert!(err.is_empty(), "the report is on stdout: {err}");
}

#[test]
fn tiza_config_lists_the_theme_warnings_that_render_prints() {
    let dir = setup(
        "config-theme-notes",
        b"[theme]\nbogus = \"#ffffff\"\naccent = \"notacolor\"\n",
    );
    let (_, out, _) = tiza_with_config(&dir, "cfg.toml", &["config"]);
    let (_, _, err) = tiza_with_config(&dir, "cfg.toml", &["render", "file.rs", "--format", "txt"]);
    for what in ["unknown token `bogus`", "theme accent: notacolor"] {
        assert!(out.contains(what), "{what} in tiza config:\n{out}");
        assert!(err.contains(what), "{what} from render:\n{err}");
    }
    assert!(
        out.contains("[theme] with the dark and light themes: "),
        "{out}"
    );
    // A key with a newline stays on one line.
    std::fs::write(dir.join("cfg.toml"), "[defaults]\n\"a\\nb\" = 1\n").unwrap();
    let (_, out, _) = tiza_with_config(&dir, "cfg.toml", &["config"]);
    assert!(out.contains("[defaults] a␊b: unknown key"), "{out}");
}

#[test]
fn no_config_file_and_a_missing_tiza_config() {
    // No file at the usual place: every value is built-in, no warning.
    let dir = setup("config-none", b"");
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let out = tiza_qa::command()
        .arg("config")
        .env_remove("TIZA_CONFIG")
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", &home)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "no file: {out:?}");
    assert!(
        text.starts_with("file: none (no file at ") && text.contains("every value is built-in"),
        "no file: {text}"
    );
    assert!(text.contains("warnings: none"), "no file: {text}");
    assert!(!text.contains(" config\n"), "no file: {text}");
    // `$TIZA_CONFIG` that names a missing file: a warning, not an error.
    let (code, out, _) = tiza_with_config(&dir, "missing.toml", &["config"]);
    assert_eq!(code, 0, "missing $TIZA_CONFIG");
    assert!(
        out.contains("missing.toml: TIZA_CONFIG names this file, but it does not exist"),
        "missing $TIZA_CONFIG: {out}"
    );
    let (code, _, err) = tiza_with_config(
        &dir,
        "missing.toml",
        &["render", "file.rs", "--format", "txt"],
    );
    assert_eq!(code, 0, "missing $TIZA_CONFIG, render: {err}");
    assert!(
        err.contains("tiza: config: missing.toml: TIZA_CONFIG names this file"),
        "missing $TIZA_CONFIG, render: {err}"
    );
}

#[test]
fn a_config_that_cannot_be_read_is_exit_1_and_render_goes_on() {
    let dir = setup("config-unreadable", b"");
    std::fs::create_dir_all(dir.join("folder.toml")).unwrap();
    let (code, _, err) = tiza_with_config(&dir, "folder.toml", &["config"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("tiza: config: folder.toml:"), "{err}");
    // Everything else starts anyway, and says that it ignored the file.
    let (code, _, err) = tiza_with_config(
        &dir,
        "folder.toml",
        &["render", "file.rs", "--format", "txt"],
    );
    assert_eq!(code, 0, "{err}");
    assert!(err.contains("tiza: config ignored: folder.toml:"), "{err}");
}
