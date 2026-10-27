//! `tiza render` and the `[defaults]` of the config file: `numbers` when
//! the document does not set it, and `width` when no `--width` is given.
//! The document and the flags win.

use tiza_qa::{fixture, scratch, tiza_with_config};

/// A scratch folder with `file.rs`, the document `doc.tiza` that sets
/// `numbers`, and the config files `cfg.toml` (every default) and
/// `none.toml` (empty).
fn setup(name: &str) -> std::path::PathBuf {
    let dir = scratch(name);
    for (from, to) in [
        ("file.rs", "file.rs"),
        ("numbers-on.tiza", "doc.tiza"),
        ("every-default.toml", "cfg.toml"),
    ] {
        std::fs::copy(
            fixture(&format!("shared/personal_config/{from}")),
            dir.join(to),
        )
        .unwrap();
    }
    std::fs::write(dir.join("none.toml"), "").unwrap();
    dir
}

fn render(dir: &std::path::Path, config: &str, args: &[&str]) -> String {
    let (code, out, err) = tiza_with_config(dir, config, args);
    assert_eq!(code, 0, "{args:?}: {err}");
    out
}

#[test]
fn a_document_that_sets_numbers_keeps_it_and_a_flag_wins() {
    let dir = setup("config-render-numbers");
    let txt = ["--format", "txt"];
    let gutter = |out: &str| out.contains(" 1 ");
    // The config says no gutter, the document says yes.
    assert!(gutter(&render(
        &dir,
        "cfg.toml",
        &[&["render", "doc.tiza"], &txt[..]].concat()
    )));
    // Without the field, the config decides; a flag beats both.
    assert!(!gutter(&render(
        &dir,
        "cfg.toml",
        &[&["render", "file.rs"], &txt[..]].concat()
    )));
    assert!(gutter(&render(
        &dir,
        "cfg.toml",
        &[&["render", "file.rs", "--numbers"], &txt[..]].concat()
    )));
    assert!(!gutter(&render(
        &dir,
        "cfg.toml",
        &[&["render", "doc.tiza", "--no-numbers"], &txt[..]].concat()
    )));
    assert!(gutter(&render(
        &dir,
        "none.toml",
        &[&["render", "file.rs"], &txt[..]].concat()
    )));
}

#[test]
fn the_config_width_is_the_width_of_a_render_without_width() {
    let dir = setup("config-render-width");
    let svg = |config: &str, extra: &[&str]| {
        let out = render(
            &dir,
            config,
            &[&["render", "file.rs", "--format", "svg"], extra].concat(),
        );
        let at = out.find("width=\"").unwrap() + 7;
        out[at..at + out[at..].find('"').unwrap()].to_string()
    };
    // `cfg.toml` sets width 60 and no gutter.
    let from_config = svg("cfg.toml", &[]);
    assert_eq!(
        from_config,
        svg("none.toml", &["--width", "60", "--no-numbers"])
    );
    assert_ne!(from_config, svg("none.toml", &["--no-numbers"]));
    // The flag wins.
    assert_eq!(
        svg("cfg.toml", &["--width", "100"]),
        svg("none.toml", &["--width", "100", "--no-numbers"])
    );
}
