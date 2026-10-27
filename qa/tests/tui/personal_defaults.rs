//! `[defaults]` of the config file: the values that each session starts
//! with. Built-in < config file < document < flags and `:` commands, and a
//! value of the config file never goes into the document.

use serde_json::{Value, json};
use tiza_qa::Live;

use crate::common::{field, json_field, setup, text_field, tui};

#[test]
fn config_turns_off_the_gutter_and_on_the_mouse_and_mouse_off_wins() {
    let dir = setup(
        "config-gutter-mouse",
        "tui/personal_defaults/no-gutter-mouse-on.toml",
    );
    let out = tui(
        &dir,
        &dir.join("cfg.toml"),
        120,
        &["--no-save", "file.rs"],
        "",
        &[("mouse off", "mouse off")],
    );
    assert_eq!(field(&out, "STARTED"), "True", "{out}");
    assert_eq!(
        json_field(&out, "STATUS0")["numbers"],
        json!(false),
        "{out}"
    );
    let text = text_field(&out, "TEXT0");
    let first = text.lines().find(|l| l.contains("fn main")).unwrap();
    assert!(!first.trim_start().starts_with('1'), "no gutter: {text:?}");
    assert_eq!(
        field(&out, "MOUSE_ON0"),
        "True",
        "the config turns the mouse on"
    );
    assert_eq!(field(&out, "CMD0"), "True", "{out}");
    assert_eq!(field(&out, "MOUSE_OFF0"), "True", ":mouse off wins");
    assert_eq!(field(&out, "EXIT"), "0", "{out}");
}

#[test]
fn every_default_of_the_config_reaches_the_session() {
    let dir = setup(
        "config-every-default",
        "shared/personal_config/every-default.toml",
    );
    let out = tui(
        &dir,
        &dir.join("cfg.toml"),
        120,
        &["file.rs"],
        "",
        &[
            ("pad", "padding 2 5 (from config.toml)"),
            ("layout", "layout stable"),
            ("numbers", "numbers off (from config.toml)"),
            ("autosave", "autosave off"),
        ],
    );
    let status = json_field(&out, "STATUS0");
    assert_eq!(status["numbers"], json!(false), "{out}");
    assert_eq!(status["autosave"], json!(false), "{out}");
    assert_eq!(field(&out, "MOUSE_ON0"), "True", "{out}");
    for i in 0..4 {
        assert_eq!(
            field(&out, &format!("CMD{i}")),
            "True",
            "command {i}: {out}"
        );
    }
    // autosave off: nothing was written next to the code, also at `:q!`.
    assert!(!dir.join("file.rs.tiza").exists());
}

#[test]
fn commands_win_over_the_config_and_are_saved_alone() {
    let live = Live::from_code("fn main() {}\n", "rs", "config-order");
    live.with(|a| {
        a.config_defaults.numbers = Some(false);
        a.config_defaults.pad = Some((3, 4));
    });
    assert!(!live.with(|a| a.numbers()), "the config over the built-in");
    assert_eq!(live.with(|a| a.effective_padding()), (3, 4));
    // `:numbers on` writes the document's own value, which then wins.
    live.command("numbers on");
    assert_eq!(live.with(|a| a.doc.numbers), Some(true));
    assert_eq!(live.api_ok("status", json!({}))["numbers"], json!(true));
    live.command("pad 1 1");
    assert_eq!(live.with(|a| a.effective_padding()), (1, 1));
    // `--padding` (a flag of the session) over the document's padding.
    live.with(|a| a.padding_override = Some((0, 0)));
    assert_eq!(live.with(|a| a.effective_padding()), (0, 0));
    // `:pad default` goes back to the config's, and says so.
    live.command("pad default");
    let (m, _) = live.message().unwrap();
    assert_eq!(m, "padding 3 4 (from config.toml)");
    // The document holds what was typed, never the config's values.
    live.command("numbers off");
    let saved: Value = live.with(|a| serde_json::to_value(&a.doc).unwrap());
    assert_eq!(saved["numbers"], json!(false));
    assert!(saved.get("padding").is_none(), "{saved}");
    // A document that sets `numbers` keeps it whatever the config says.
    live.with(|a| {
        a.doc.numbers = Some(true);
        a.config_defaults.numbers = Some(false);
    });
    assert!(live.with(|a| a.numbers()));
    assert_eq!(live.api_ok("status", json!({}))["numbers"], json!(true));
}

#[test]
fn numbers_on_or_off_writes_the_document_also_when_the_value_is_in_effect() {
    // (case, the config's numbers, the command)
    for (case, config, command) in [
        ("config off, :numbers off", Some(false), "numbers off"),
        ("no config, :numbers on", None, "numbers on"),
        ("config on, :numbers on", Some(true), "numbers on"),
    ] {
        let live = Live::from_code("fn main() {}\n", "rs", "config-numbers-write");
        live.with(move |a| a.config_defaults.numbers = config);
        let on = command == "numbers on";
        assert_eq!(live.with(|a| a.numbers()), on, "{case}: in effect before");
        live.command(command);
        let (m, _) = live.message().unwrap();
        assert!(!m.contains("already"), "{case}: {m}");
        assert_eq!(live.with(|a| a.doc.numbers), Some(on), "{case}: written");
        // The second time, the document has it.
        live.command(command);
        let (m, _) = live.message().unwrap();
        assert!(m.contains("already"), "{case}, again: {m}");
    }
}
