//! `tiza check` measures as `tiza render` draws: the `width` of the config
//! file when no `--width` is given (the flag wins), and its `numbers` when
//! the document does not set the gutter.

use serde_json::{Value, json};
use tiza_qa::{fixture, scratch, tiza_with_config};

#[test]
fn check_takes_the_width_of_the_config_and_the_flag_wins() {
    let dir = scratch("config-check-width");
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
    let width = |args: &[&str]| {
        let (_, out, err) = tiza_with_config(&dir, "cfg.toml", args);
        let report: Value = serde_json::from_str(&out).unwrap_or_else(|_| panic!("{out}{err}"));
        report["width"].clone()
    };
    assert_eq!(
        width(&["check", "doc.tiza", "--json"]),
        json!(60),
        "the config's width"
    );
    assert_eq!(
        width(&["check", "doc.tiza", "--json", "--width", "80"]),
        json!(80),
        "the flag wins"
    );
}

#[test]
fn check_measures_without_the_gutter_that_the_config_turns_off() {
    let dir = scratch("config-check-gutter");
    std::fs::copy(
        fixture("shared/personal_config/file.rs"),
        dir.join("file.rs"),
    )
    .unwrap();
    std::fs::copy(
        fixture("check/config_width/note.tiza"),
        dir.join("note.tiza"),
    )
    .unwrap();
    std::fs::write(dir.join("off.toml"), "[defaults]\nnumbers = false\n").unwrap();
    std::fs::write(dir.join("none.toml"), "").unwrap();
    let clean = |config: &str, doc: &str| {
        let (_, out, err) = tiza_with_config(&dir, config, &["check", doc, "--json"]);
        let report: Value = serde_json::from_str(&out).unwrap_or_else(|_| panic!("{out}{err}"));
        report["min_clean_width"]
            .as_u64()
            .unwrap_or_else(|| panic!("{report}"))
    };
    let with_gutter = clean("none.toml", "note.tiza");
    assert!(
        clean("off.toml", "note.tiza") < with_gutter,
        "no gutter: the note is whole at a narrower width"
    );
    // A document that sets `numbers` keeps its gutter.
    let mut doc: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("note.tiza")).unwrap()).unwrap();
    doc["numbers"] = json!(true);
    std::fs::write(dir.join("numbers.tiza"), doc.to_string()).unwrap();
    assert_eq!(clean("off.toml", "numbers.tiza"), with_gutter);
}
