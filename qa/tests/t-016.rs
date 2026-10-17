//! t-016 — Themes.
//!
//! Acceptance criteria:
//! - AC1 `t` toggles; `:theme dark|light|claro|oscuro` sets; `:theme` alone toggles.
//! - AC2 Unknown names are errors in the TUI and over the socket (`theme.set`).

use serde_json::json;
use tiza_qa::Live;

#[test]
fn ac1_ac2_theme_commands_and_errors() {
    let live = Live::from_code("fn a() {}\n", "rs", "t016");
    live.command("theme claro");
    assert_eq!(live.with(|app| app.theme.name), tiza_core::ThemeName::Light);
    live.command("theme");
    assert_eq!(live.with(|app| app.theme.name), tiza_core::ThemeName::Dark);
    live.command("theme bogus");
    assert!(
        matches!(live.message(), Some((m, true)) if m.contains("unknown theme")),
        "{:?}",
        live.message()
    );
    assert_eq!(
        live.with(|app| app.theme.name),
        tiza_core::ThemeName::Dark,
        "unchanged"
    );
    let r = live.api("theme.set", json!({"theme": "bogus"}));
    assert_eq!(r.error.unwrap().code, "invalid_argument");
    let r = live.api_ok("theme.set", json!({"theme": "light"}));
    assert_eq!(r["theme"], "light");
}
