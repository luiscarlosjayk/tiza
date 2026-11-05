//! Helpers of the tests that type `:` commands in a live app and read its
//! status bar.

use crate::Live;

/// The code of [`fresh`].
pub const CODE: &str = "fn main() {\n    let s = String::from(\"hi\");\n    takes(s);\n    // println!(\"{s}\");\n}\n\nfn takes(s: String) {\n    println!(\"{s}\");\n}\n";

/// The number of annotations.
pub fn count(live: &Live) -> usize {
    live.with(|a| a.doc.annotations.len())
}

/// A live app on [`CODE`]: a note and a spotlight at step 1, a note at
/// step 2, and step 1 on screen.
pub fn fresh(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.command("note 1 first");
    live.command("spot 7-9");
    live.command("step 2");
    live.command("note 8 later");
    live.command("step 1");
    live
}

/// Runs `cmd`; the message must be an error containing `expect`, and the
/// annotation count and the step must not change.
pub fn rejected(live: &Live, cmd: &str, expect: &str) {
    let before = (
        count(live),
        live.with(|a| a.doc.step),
        live.with(|a| a.doc.max_step()),
    );
    live.command(cmd);
    let (msg, err) = live.message().unwrap_or_default();
    assert!(err, "`:{cmd}` was not rejected: {msg}");
    assert!(
        msg.contains(expect),
        "`:{cmd}`: expected «{expect}», got: {msg}"
    );
    let after = (
        count(live),
        live.with(|a| a.doc.step),
        live.with(|a| a.doc.max_step()),
    );
    assert_eq!(before, after, "`:{cmd}` changed the document: {msg}");
}

/// The step on screen.
pub fn step(live: &Live) -> u32 {
    live.with(|a| a.doc.step)
}

/// The message of the status bar, which must be an info, not an error.
pub fn info(live: &Live) -> String {
    match live.message() {
        Some((m, false)) => m,
        other => panic!("expected an info message, got {other:?}"),
    }
}
