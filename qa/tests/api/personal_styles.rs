//! Personal styles over the socket: `style.list` (the MCP `list_kinds`)
//! shows them, and a mark of an agent, a `scene.put` or a `scene.patch`
//! that uses one copies it into the document. A document style of the
//! same name wins.

use serde_json::json;
use tiza_qa::Live;

#[test]
fn style_list_and_agent_marks_see_personal_styles() {
    let live = Live::from_code(
        "fn main() {\n    let body = fetch();\n}\n",
        "rs",
        "personal-styles-api",
    );
    live.with(|a| {
        a.personal_styles.insert(
            "io".into(),
            tiza_core::MarkStyle::new("⇄", "network I/O", "cyan"),
        );
        a.personal_styles.insert(
            "db".into(),
            tiza_core::MarkStyle::new("◫", "database", "purple"),
        );
    });
    let kinds = live.api_ok("style.list", json!({}));
    assert_eq!(kinds["personal"]["io"]["glyph"], json!("⇄"), "{kinds}");
    assert_eq!(kinds["document"], json!({}));
    // An agent's mark (the MCP add_mark) copies the style, only that one.
    live.api_ok(
        "annotation.add",
        json!({"type": "mark", "kind": "io", "at": {"line": 2, "find": "fetch"}}),
    );
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(
        scene["document"]["styles"]["io"]["legend"],
        json!("network I/O")
    );
    assert!(scene["document"]["styles"].get("db").is_none(), "{scene}");
    let kinds = live.api_ok("style.list", json!({}));
    assert!(
        kinds["personal"].get("io").is_none(),
        "now a document style"
    );
    // Undo takes the mark and its copied style away together.
    live.command("undo");
    assert!(live.with(|a| a.doc.styles.is_empty()));
    // A document style of the same name wins over the personal one.
    live.command("style db glyph=D legend=\"my db\" color=red");
    live.api_ok(
        "annotation.add",
        json!({"type": "mark", "kind": "db", "at": {"line": 2, "find": "fetch"}}),
    );
    assert_eq!(
        live.with(|a| a.doc.styles["db"].legend.clone()),
        Some("my db".into())
    );
    // `:mark` lists the personal names.
    live.command("mark");
    let (m, err) = live.message().unwrap();
    assert!(err && m.contains("io"), "{m}");
    // A mark that the document refuses (its id is taken) copies nothing:
    // the style it copied goes away with it.
    let taken = live.with(|a| a.doc.annotations[0].id.clone());
    let r = live.api(
        "annotation.add",
        json!({"id": taken, "type": "mark", "kind": "io", "at": {"line": 2, "find": "fetch"}}),
    );
    assert!(r.error.is_some(), "{r:?}");
    assert!(live.with(|a| !a.doc.styles.contains_key("io")));
    // A put of marks that use one copies it too.
    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
    doc["annotations"] = json!([{"id": "p1", "step": 1, "type": "mark", "kind": "io",
        "span": {"line": 2, "start": 15, "end": 20}}]);
    live.api_ok("scene.put", json!({"document": doc}));
    assert!(live.with(|a| a.doc.styles.contains_key("io")));
    // And a patch.
    live.command("style io");
    live.api_ok(
        "scene.patch",
        json!({"annotations": [{"id": "p1", "step": 1, "type": "mark", "kind": "io",
            "span": {"line": 2, "start": 15, "end": 20}}]}),
    );
    assert!(live.with(|a| a.doc.styles.contains_key("io")));
}
