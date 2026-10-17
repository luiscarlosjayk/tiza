//! t-090 (follow-up from the QA agent) — `--padding` is a session override.
//!
//! - AC1 The override is shown but never saved; `:pad` sets the document and clears it.
//! - AC2 `:pad` reports clamping when the terminal is too small.

use tiza_qa::{Live, scratch};

#[test]
fn ac1_override_not_saved_and_cleared_by_pad() {
    let dir = scratch("t090b");
    let save = dir.join("doc.tiza");
    let live = Live::open(
        tiza_core::Document::from_code("fn main() {}\n", "rs", None),
        save.clone(),
        dir.join("s.sock"),
    );
    live.with(|app| app.padding_override = Some((3, 6)));
    live.command("note 1 x");
    assert!(
        !std::fs::read_to_string(&save).unwrap().contains("padding"),
        "override not persisted"
    );
    assert_eq!(live.with(|app| app.effective_padding()), (3, 6));
    live.command("pad");
    assert!(
        matches!(live.message(), Some((m, false)) if m.contains("not saved")),
        "{:?}",
        live.message()
    );
    live.command("pad 1 2");
    assert!(live.with(|app| app.padding_override.is_none()));
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&save).unwrap()).unwrap();
    assert_eq!(saved["padding"], serde_json::json!([1, 2]), "{saved}");
}

#[test]
fn ac2_pad_reports_clamping() {
    let live = Live::from_code("fn main() {}\n", "rs", "t090b-clamp");
    live.screen(100, 30);
    live.command("pad 30 60");
    let (m, err) = live.message().unwrap();
    assert!(!err && m.contains("shown as"), "{m}");
}
