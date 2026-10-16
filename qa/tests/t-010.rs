//! t-010 — Paste and stdin input.
//!
//! Acceptance criteria:
//! - AC1 Pasting replaces the code; annotations whose lines still exist are
//!   kept, the others dropped with a message; `u` restores everything.
//! - AC2 CRLF is normalized; an empty paste is rejected.

use codedraw_qa::Live;

#[test]
fn ac1_paste_keeps_what_still_fits_and_undo_restores() {
    let live = Live::from_code(
        "fn main() {\n    let s = 1;\n    let t = 2;\n}\n",
        "rs",
        "t010-paste",
    );
    live.command("note 1 keep");
    live.command("note 4 gone");
    live.with(|app| app.replace_code("fn main() {}\r\nfn b() {}\r\n").unwrap());
    let (msg, err) = live.message().unwrap();
    assert!(
        !err && msg.contains("2 lines pasted") && msg.contains("1 annotation dropped"),
        "{msg}"
    );
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    assert_eq!(
        live.with(|app| app.doc.code.clone()),
        vec!["fn main() {}", "fn b() {}"]
    );
    live.with(|app| app.undo());
    assert_eq!(live.with(|app| app.doc.annotations.len()), 2);
    assert_eq!(live.with(|app| app.doc.code.len()), 4);
}
