//! `[styles]` of the config file: personal mark styles. The first mark that
//! uses one copies it into the document, so the `.tiza` looks the same on a
//! computer without that file.

use serde_json::{Value, json};
use tiza_core::model::MarkStyle;
use tiza_qa::Live;

use crate::common::{field, setup, tui};

#[test]
fn a_personal_style_goes_into_the_document_with_its_first_mark() {
    let dir = setup(
        "config-style-copied",
        "shared/personal_config/personal-styles.toml",
    );
    let out = tui(
        &dir,
        &dir.join("cfg.toml"),
        120,
        &["file.rs"],
        "",
        &[
            ("mark io 2:fetch", "(style io from config.toml)"),
            ("w", "saved to"),
        ],
    );
    assert_eq!(field(&out, "CMD0"), "True", "{out}");
    assert_eq!(field(&out, "CMD1"), "True", "{out}");
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("file.rs.tiza")).unwrap()).unwrap();
    assert_eq!(
        saved["styles"]["io"],
        json!({"glyph": "⇄", "legend": "network I/O", "color": "cyan"}),
        "{saved}"
    );
    assert_eq!(saved["annotations"][0]["kind"], json!("io"));
    // The same look with an empty config file: the document has the style.
    std::fs::write(dir.join("empty.toml"), "").unwrap();
    let (code, without, err) = tiza_qa::tiza_with_config(
        &dir,
        "empty.toml",
        &["render", "file.rs.tiza", "--format", "ansi"],
    );
    assert_eq!(code, 0, "{err}");
    assert!(without.contains('⇄') && without.contains("network I/O"));
}

#[test]
fn style_on_a_personal_style_changes_a_copy_of_it() {
    let live = Live::from_code("fn main() {}\n", "rs", "config-style-change");
    live.with(|a| {
        a.personal_styles.insert(
            "io".into(),
            MarkStyle {
                glyph: Some("⇄".into()),
                legend: Some("network I/O".into()),
                color: Some("cyan".into()),
            },
        );
    });
    live.command("style io color=blue");
    let (m, err) = live.message().unwrap();
    assert!(!err, "{m}");
    let doc: Value = live.with(|a| serde_json::to_value(&a.doc).unwrap());
    assert_eq!(
        doc["styles"]["io"],
        json!({"glyph": "⇄", "legend": "network I/O", "color": "blue"}),
        "the glyph and legend of the personal style, the new color"
    );
    // The personal style itself does not change.
    let personal = live.with(|a| a.personal_styles["io"].color.clone());
    assert_eq!(personal.as_deref(), Some("cyan"));
    // `style.set` (the MCP tool `define_style`) does the same.
    live.with(|a| {
        a.personal_styles.insert(
            "db".into(),
            MarkStyle {
                glyph: Some("⛁".into()),
                legend: Some("database".into()),
                color: None,
            },
        );
    });
    let reply = live.api_ok("style.set", json!({"name": "db", "color": "red"}));
    assert_eq!(
        reply["style"],
        json!({"glyph": "⛁", "legend": "database", "color": "red"}),
        "style.set: {reply}"
    );
}
