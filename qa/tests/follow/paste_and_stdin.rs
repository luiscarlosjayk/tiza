//! Paste and stdin input.
//!
//! Acceptance criteria:
//! - AC1 Pasting replaces the code; annotations are re-anchored by the text they
//!   pointed at (same line first, then anywhere), the others are dropped with a
//!   message; `u` restores everything. An empty paste is refused.
//! - AC2 CRLF is normalized; an empty paste is rejected.

use tiza_qa::Live;

#[test]
fn paste_reanchors_by_text_and_undo_restores() {
    let live = Live::from_code(
        "fn main() {\n    let s = 1;\n    let t = 2;\n}\n",
        "rs",
        "paste",
    );
    live.command("note 1 keep");
    live.command("note 3 gone");
    live.command("hl 2:s shared");
    // A comment is inserted on top and `let t` disappears: the note on
    // `fn main` moves to line 2, the highlight follows `s` to line 3, the
    // note on `let t` is dropped. CRLF is normalized.
    live.with(|app| {
        app.replace_code("// header\r\nfn main() {\r\n    let s = 1;\r\n}\r\n")
            .unwrap()
    });
    let (msg, err) = live.message().unwrap();
    assert!(
        !err && msg.contains("4 lines pasted") && msg.contains("1 annotation dropped"),
        "{msg}"
    );
    let list = live.api_ok("scene.list", serde_json::json!({}));
    let text = list
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("note L2 «keep»"), "{text}");
    assert!(text.contains("highlight L3 8..9"), "{text}");
    assert!(!text.contains("gone"), "{text}");
    assert_eq!(
        live.with(|app| app.doc.code.clone()),
        vec!["// header", "fn main() {", "    let s = 1;", "}"]
    );
    live.with(|app| app.undo());
    assert_eq!(live.with(|app| app.doc.annotations.len()), 3);
    assert_eq!(live.with(|app| app.doc.code.len()), 4);
    // Corner: an empty paste is refused and changes nothing.
    let r = live.with(|app| app.replace_code("  \n\n"));
    assert!(r.is_err());
    assert_eq!(live.with(|app| app.doc.annotations.len()), 3);
}
