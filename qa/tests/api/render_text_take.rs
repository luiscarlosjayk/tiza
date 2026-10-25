//! `render.text` with a width gave the presenter's own layout: while
//! authoring, the room the steps so far need, not the take. Sending back
//! the width the reply named (or any width) could give another picture than
//! the export at that width. With a width it is now the take too.
//!
//! Acceptance criteria:
//! - AC1 `render.text {width: N}` is the text `tiza render DOC --step S
//!   --width N --format txt` writes, while authoring, in presentation mode
//!   and during a typing animation.
//! - AC2 The agent guide, the MCP tool and its `width`, the CLI help and the
//!   API schema say so; CHANGELOG.
//! - AC3 This file, with the two-step document where the authoring layout
//!   and the take differ.

use serde_json::{Value, json};
use tiza_qa::{Live, fixture, scratch, tiza_ok};

const NOTE: &str = "the live token: rotate it right after the demo, never commit it again";

/// A note at step 1; arrows at step 2 take lanes on the left of the take,
/// which the authoring screen at step 1 does not have.
fn lanes() -> Value {
    json!({
        "schema": 1, "source": {"path": "four.env", "language": "env", "first_line": 1},
        "code": {"file": "four.env"}, "step": 1,
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 2, "text": NOTE},
            {"id": "r1", "step": 2, "type": "arrow", "from": {"line": 1, "col": 0}, "to": {"line": 4, "col": 0}},
            {"id": "r2", "step": 2, "type": "arrow", "from": {"line": 2, "col": 0}, "to": {"line": 4, "col": 1}},
            {"id": "r3", "step": 2, "type": "arrow", "from": {"line": 3, "col": 0}, "to": {"line": 1, "col": 1}}
        ]
    })
}

/// A step 1 that types its change.
fn typed() -> Value {
    let mut doc = lanes();
    doc["step"] = json!(0);
    doc["moves"] = json!([{"at_step": 1, "action": "code.patch", "op": "replace", "find": "PORT=8080",
                           "content": "PORT=9090", "typing": "char", "speed_ms": 1000}]);
    doc
}

fn opened(name: &str, doc: &Value) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    std::fs::copy(
        fixture("api/render_text_width/four.env"),
        dir.join("four.env"),
    )
    .unwrap();
    let path = dir.join("four.env.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let d = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    live.with(|a| a.size = (120, 30));
    (live, path)
}

fn exported(path: &std::path::Path, step: u32, width: usize) -> String {
    let out = tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--step",
        &step.to_string(),
        "--width",
        &width.to_string(),
        "--format",
        "txt",
    ]);
    out.trim_end().to_string()
}

fn live_text(live: &Live, width: usize) -> String {
    let r = live.api_ok("render.text", json!({"width": width}));
    assert_eq!(r["width"], json!(width), "{r}");
    r["text"].as_str().unwrap().trim_end().to_string()
}

#[test]
fn authoring_and_presenting_give_the_export_at_that_width() {
    for present in [false, true] {
        let (live, path) = opened(&present.to_string(), &lanes());
        if present {
            live.with(|a| a.set_present(true));
        }
        let mut own_differs = false;
        for width in [70usize, 90, 100, 117, 140] {
            let text = live_text(&live, width);
            assert_eq!(text, exported(&path, 1, width), "{present} {width}");
            // While authoring, the screen's own layout is another picture:
            // the case the change is about.
            let own = live.with(move |a| a.layout(width).canvas.to_plain_text());
            own_differs |= own.trim_end() != text;
        }
        assert_eq!(
            own_differs, !present,
            "authoring differs, presenting does not"
        );
    }
}

#[test]
fn during_a_typing_animation() {
    let (live, path) = opened("typing", &typed());
    live.api_ok("step.next", json!({}));
    assert!(live.with(|a| a.typing.is_some()), "the step is typing");
    for width in [90usize, 117] {
        let text = live_text(&live, width);
        assert!(live.with(|a| a.typing.is_some()), "{width}: still typing");
        assert!(text.contains("PORT=9090"), "{text}");
        assert_eq!(text, exported(&path, 1, width), "{width}");
    }
}
