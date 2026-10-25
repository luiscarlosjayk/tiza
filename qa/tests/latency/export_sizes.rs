//! The size of an export is bounded in the TUI and the CLI, and the largest
//! export that they allow is written in time.

use serde_json::json;
use tiza_qa::{Live, fixture, scratch, tiza};

const CODE: &str = "fn main() {\n    let s = String::from(\"hi\");\n    takes(s);\n    // println!(\"{s}\");\n}\n\nfn takes(s: String) {\n    println!(\"{s}\");\n}\n";

fn fresh(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.command("note 1 first");
    live.command("spot 7-9");
    live.command("step 2");
    live.command("note 8 later");
    live.command("step 1");
    live
}

fn count(live: &Live) -> usize {
    live.with(|a| a.doc.annotations.len())
}

/// Runs `cmd`; the message must be an error containing `expect`, and the
/// annotation count and the step must not change.
fn rejected(live: &Live, cmd: &str, expect: &str) {
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

#[test]
fn export_sizes_are_bounded_in_the_tui_and_the_cli() {
    let live = fresh("export");
    let dir = scratch("export-files");
    let png = dir.join("x.png");
    rejected(
        &live,
        &format!("export {} width=99999", png.display()),
        "width=99999: use a number between 20 and 400",
    );
    rejected(
        &live,
        &format!("export {} width=5", png.display()),
        "width=5: use a number between 20 and 400",
    );
    assert!(!png.exists());
    // The CLI: same message, non-zero exit, no allocator text.
    let ex = fixture("shared/ownership/ownership.rs.tiza");
    let out = tiza(&[
        "render",
        ex.to_str().unwrap(),
        "--width",
        "99999",
        "--out",
        png.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("width=99999: use a number between 20 and 400"),
        "{err}"
    );
    assert!(!err.contains("memory allocation"));
    let out = tiza(&[
        "render",
        ex.to_str().unwrap(),
        "--scale",
        "99",
        "--out",
        png.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("scale=99: use a number between 0.25 and 8"),
        "{err}"
    );
    // Inside the range but over the pixel budget: refused by the budget.
    let out = tiza(&[
        "render",
        ex.to_str().unwrap(),
        "--width",
        "400",
        "--scale",
        "8",
        "--out",
        png.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("over the limit (16384 px per side, 100000000 px in all)"),
        "{err}"
    );
    assert!(!png.exists());
    // The socket's render.text and the MCP's export_image get the same cap.
    let r = live.api("render.text", json!({"width": 99999}));
    assert!(
        r.error
            .unwrap()
            .message
            .contains("width=99999: use a number between 20 and 400")
    );
    // AC3: the largest allowed export is fast.
    let t = std::time::Instant::now();
    let out = tiza(&[
        "render",
        ex.to_str().unwrap(),
        "--width",
        "400",
        "--scale",
        "2",
        "--out",
        png.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // 5 s here in debug, ~0.5 s in release; the shared CI runners are two
    // to three times slower, so the debug budget is wide (the release
    // budget is the one the card means).
    let budget = if cfg!(debug_assertions) { 40 } else { 10 };
    assert!(t.elapsed().as_secs() < budget, "{:?}", t.elapsed());
}
