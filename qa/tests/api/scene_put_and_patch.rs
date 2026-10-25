//! `tiza scene put|patch` against the live instance.
//!
//! Acceptance criteria:
//! - AC1 `tiza scene put FILE` (or `-`) replaces the live document
//!   atomically: code, annotations, step, theme, styles, title, padding. A
//!   `.tiza` that references its code by a relative path is resolved
//!   relative to the file and travels embedded; `u` in the TUI restores the
//!   previous document in one step. The reply carries the new `revision`.
//! - AC2 `tiza scene patch JSON` (`-` = stdin, `@file`) changes only what
//!   it names: `step`, `theme`, `padding`, `title`, `styles` (merged by name)
//!   and `annotations` merged by id — same id replaces, new id adds,
//!   `{"id","deleted":true}` removes. Annotations use the document format
//!   (what `scene.get` returns: `span`, `from`/`to`…), not `annotation.add`'s
//!   `at`/`find` shorthand. All or nothing: one invalid annotation
//!   leaves the document untouched. Reply lists `added`/`replaced`/`deleted`.
//! - AC3 Unknown patch keys → `invalid_argument` naming the key; a document
//!   with a file reference sent to `scene.put` → `invalid_argument`.
//! - AC4 Both methods are in `api.methods` and `schema/api.schema.json`.

use serde_json::{Value, json};
use tiza_core::fs;
use tiza_qa::{Live, fixture, scratch, tiza, tiza_ok};

fn put_file(live: &Live, path: &str) -> Value {
    let out = tiza_ok(&[
        "--socket",
        live.socket.to_str().unwrap(),
        "scene",
        "put",
        path,
    ]);
    serde_json::from_str(&out).unwrap()
}

fn patch(live: &Live, json: &str) -> std::process::Output {
    tiza(&[
        "--socket",
        live.socket.to_str().unwrap(),
        "scene",
        "patch",
        json,
    ])
}

#[test]
fn put_replaces_the_document_and_undo_restores() {
    let live = Live::from_code("fn old() {}\n", "rs", "put");
    live.command("note 1 old note");
    let before_rev = live.api_ok("status", json!({}))["revision"]
        .as_u64()
        .unwrap();

    // The export fixture embeds its code; `script.ref.tiza` of language detection
    // references `script` relatively: both must arrive whole.
    let r = put_file(&live, fixture("t-050/export.tiza").to_str().unwrap());
    assert!(r["revision"].as_u64().unwrap() > before_rev, "{r}");
    assert_eq!(r["annotations"], json!(8));
    let doc = live.with(|a| a.doc.clone());
    assert_eq!(
        doc.title.as_deref(),
        Some("t-050 export: symbols, emoji, wide glyphs, every annotation kind")
    );
    assert_eq!(doc.code.len(), 6);
    assert!(doc.styles.contains_key("hot"));
    assert!(
        doc.annotations
            .iter()
            .all(|a| a.id != "a1" || a.shape.line_ranges()[0].0 == 2),
        "old note gone"
    );

    let r = put_file(
        &live,
        fixture("render/language_detection/script.ref.tiza")
            .to_str()
            .unwrap(),
    );
    assert_eq!(r["lines"], json!(7), "{r}");
    assert_eq!(r["annotations"], json!(1));
    let (lang, embedded) = live.with(|a| {
        (
            a.doc.source.language.clone(),
            matches!(a.doc.code_src, tiza_core::model::CodeSource::Embedded(_)),
        )
    });
    assert_eq!(lang, "py", "language detected from the referenced file");
    assert!(embedded, "code travels embedded on the wire");

    // Undo: back to the export document, then to the original one.
    live.with(|a| a.undo());
    assert_eq!(live.with(|a| a.doc.code.len()), 6);
    live.with(|a| a.undo());
    assert_eq!(live.with(|a| a.doc.code.clone()), vec!["fn old() {}"]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);

    // From stdin.
    use std::io::Write;
    let mut child = std::process::Command::new(tiza_qa::bin())
        .args([
            "--socket",
            live.socket.to_str().unwrap(),
            "scene",
            "put",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"schema":1,"source":{"language":"rs"},"code":["fn piped() {}"],"theme":"light","annotations":[{"id":"p","step":1,"type":"note","line":1,"text":"hi"}]}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(live.with(|a| a.doc.code.clone()), vec!["fn piped() {}"]);
    assert_eq!(
        live.with(|a| a.theme.name),
        tiza_core::ThemeName::Light,
        "theme follows the document"
    );
}

#[test]
fn patch_merges_by_id_and_changes_only_what_it_names() {
    let live = Live::from_code("fn a() {}\nfn b() {}\nfn c() {}\n", "rs", "patch");
    live.command("note 1 first");
    live.command("note 2 second");
    live.command("title Old title");
    let ids = live.with(|a| {
        a.doc
            .annotations
            .iter()
            .map(|x| x.id.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(ids.len(), 2);

    let out = patch(
        &live,
        &json!({
            "theme": "light",
            "step": 2,
            "padding": [2, 3],
            "title": "New title",
            "styles": {"hot": {"glyph": "🔥", "legend": "hot path", "color": "red"}},
            "annotations": [
                {"id": ids[0], "step": 1, "type": "note", "line": 3, "text": "moved"},
                {"id": ids[1], "deleted": true},
                {"id": "n3", "step": 2, "type": "mark", "kind": "hot", "span": {"line": 2, "start": 3, "end": 4}}
            ]
        })
        .to_string(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let r: Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(r["replaced"], json!([ids[0]]));
    assert_eq!(r["deleted"], json!([ids[1]]));
    assert_eq!(r["added"], json!(["n3"]));
    assert_eq!(r["annotations"], json!(2));
    assert_eq!(r["step"], json!(2));
    let doc = live.with(|a| a.doc.clone());
    assert_eq!(doc.title.as_deref(), Some("New title"));
    assert_eq!(doc.padding, Some((2, 3)));
    assert_eq!(doc.theme, Some(tiza_core::ThemeName::Light));
    assert_eq!(live.with(|a| a.theme.name), tiza_core::ThemeName::Light);
    assert!(doc.styles.contains_key("hot"));
    let moved = doc.find_annotation(&ids[0]).unwrap();
    assert_eq!(moved.shape.line_ranges(), vec![(3, 3)]);
    assert!(doc.find_annotation(&ids[1]).is_none());
    assert!(doc.find_annotation("n3").is_some());
    assert_eq!(doc.code.len(), 3, "code untouched");

    // A patch that names nothing else leaves everything alone; `null` clears.
    let out = patch(&live, r#"{"title": null, "padding": null}"#);
    assert!(out.status.success());
    let doc = live.with(|a| a.doc.clone());
    assert_eq!(doc.title, None);
    assert_eq!(doc.padding, None);
    assert_eq!(doc.annotations.len(), 2);
    // One undo per patch.
    live.with(|a| a.undo());
    assert_eq!(
        live.with(|a| a.doc.title.clone()).as_deref(),
        Some("New title")
    );

    // `@file` and `-` are accepted.
    let dir = scratch("patchfile");
    let f = dir.join("p.json");
    std::fs::write(&f, r#"{"step": 1}"#).unwrap();
    let out = patch(&live, &format!("@{}", f.display()));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(live.with(|a| a.doc.step), 1);
}

#[test]
fn errors_leave_the_document_untouched() {
    let live = Live::from_code("fn a() {}\nfn b() {}\n", "rs", "errors");
    live.command("note 1 keep");
    let snapshot = live.with(|a| a.doc.clone());

    let out = patch(&live, r#"{"stepp": 2}"#);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        // The unknown field is named, and the valid ones.
        err.contains("unknown field `stepp`, expected one of `step`")
            && err.contains("invalid_argument"),
        "{err}"
    );

    // All or nothing: the valid note is not added because the second is invalid.
    let out = patch(
        &live,
        r#"{"annotations": [{"id": "ok", "step": 1, "type": "note", "line": 2, "text": "x"}, {"id": "bad", "step": 1, "type": "note", "line": 99, "text": "y"}]}"#,
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("99"),
        "names the bad line"
    );
    assert_eq!(live.with(|a| a.doc.clone()), snapshot);

    let out = patch(
        &live,
        r#"{"annotations": [{"step": 1, "type": "note", "line": 1, "text": "no id"}]}"#,
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("`id`"));
    let out = patch(
        &live,
        r#"{"annotations": [{"id": "ghost", "deleted": true}]}"#,
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("ghost"));
    let out = patch(&live, "not json");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not valid JSON"));
    assert_eq!(live.with(|a| a.doc.clone()), snapshot);

    // scene.put with a file reference straight on the socket: rejected.
    let r = live.api(
        "scene.put",
        json!({"document": {"schema": 1, "source": {"language": "rs"}, "code": {"file": "src/main.rs"}}}),
    );
    let e = r.error.expect("error");
    assert_eq!(e.code, "invalid_argument");
    assert!(e.message.contains("embedded"), "{}", e.message);
    // And a document whose annotations are out of range.
    let r = live.api(
        "scene.put",
        json!({"document": {"schema": 1, "source": {"language": "rs"}, "code": ["x"], "annotations": [{"id": "a", "step": 1, "type": "note", "line": 5, "text": "t"}]}}),
    );
    assert!(r.error.is_some());
    assert_eq!(live.with(|a| a.doc.clone()), snapshot);
    // The CLI reports a missing file without touching the instance.
    let out = tiza(&[
        "--socket",
        live.socket.to_str().unwrap(),
        "scene",
        "put",
        "/nope/none.tiza",
    ]);
    assert!(!out.status.success());
    assert_eq!(live.with(|a| a.doc.clone()), snapshot);
}

#[test]
fn methods_are_published() {
    let live = Live::from_code("fn a() {}\n", "rs", "schema");
    let methods = live.api_ok("api.methods", json!({}));
    let names: Vec<&str> = methods
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["method"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"scene.put") && names.contains(&"scene.patch"),
        "{names:?}"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("schema/api.schema.json")).unwrap(),
    )
    .unwrap();
    let in_schema: Vec<&str> = schema["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["method"].as_str().unwrap())
        .collect();
    assert!(in_schema.contains(&"scene.put") && in_schema.contains(&"scene.patch"));
    // The patch schema rejects unknown keys (additionalProperties: false).
    let patch_schema = schema["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["method"] == "scene.patch")
        .unwrap();
    assert_eq!(
        patch_schema["params"]["additionalProperties"],
        json!(false),
        "{patch_schema}"
    );
    // A round trip: scene.get → scene.put is a no-op on the content, and a
    // put that changes nothing is no change at all (same revision, no save;
    // the scene round trip tests).
    let before = live.api_ok("scene.get", json!({}));
    let r = live.api_ok("scene.put", json!({"document": before["document"]}));
    assert_eq!(r["revision"], before["revision"]);
    let after = live.api_ok("scene.get", json!({}));
    let strip = |mut d: Value| {
        // Saving records the active theme; the content is what must match.
        d.as_object_mut().unwrap().remove("theme");
        d
    };
    assert_eq!(
        strip(after["document"].clone()),
        strip(before["document"].clone())
    );
    let _ = fs::open; // keep the import meaningful for readers of the ACs
}

// ------------------------------------------------------------ rework (owner QA)

#[test]
fn undo_after_put_restores_the_step_too() {
    let live = Live::from_code("fn old() {}\nfn more() {}\n", "rs", "undo-step");
    live.command("note 1 one");
    live.command("note 2 two step=3");
    live.api_ok("step.set", json!({"step": 0}));
    assert_eq!(live.with(|a| a.doc.step), 0);
    put_file(&live, fixture("t-050/export.tiza").to_str().unwrap());
    assert_eq!(live.with(|a| a.doc.step), 2, "the put document's step");
    live.with(|a| a.undo());
    assert_eq!(live.with(|a| a.doc.code.len()), 2);
    assert_eq!(
        live.with(|a| a.doc.step),
        0,
        "the whole previous document comes back, step included"
    );
    assert_eq!(live.message().unwrap().0, "undone: scene.put");
    // And forward again.
    live.with(|a| a.redo());
    assert_eq!(live.with(|a| a.doc.step), 2);
    // A patch that moves the step is undone the same way.
    live.with(|a| a.undo());
    let out = patch(&live, r#"{"step":1,"theme":"light"}"#);
    assert!(out.status.success());
    assert_eq!(live.with(|a| a.doc.step), 1);
    live.with(|a| a.undo());
    assert_eq!(live.with(|a| a.doc.step), 0);
    assert_eq!(live.message().unwrap().0, "undone: scene.patch");
}

#[test]
fn cli_put_accepts_the_get_envelope_and_refuses_a_stray_file() {
    let dir = scratch("envelope");
    let src = dir.join("stack.rs");
    std::fs::write(&src, "fn a() {}\nfn b() {}\n").unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("stack.rs.tiza"), dir.join("live.sock"));
    live.command("note 2 bee");
    live.api_ok("step.set", json!({"step": 1}));
    let socket = live.socket.to_str().unwrap().to_string();
    // `scene get` prints the envelope; `scene put` of that file is a no-op.
    let envelope = tiza_ok(&["--socket", &socket, "scene", "get"]);
    let s_json = dir.join("s.json");
    std::fs::write(&s_json, &envelope).unwrap();
    let before = live.with(|a| (a.doc.code.clone(), a.doc.annotations.clone(), a.doc.step));
    let r = put_file(&live, s_json.to_str().unwrap());
    assert_eq!(r["lines"], json!(2), "{r}");
    let after = live.with(|a| (a.doc.code.clone(), a.doc.annotations.clone(), a.doc.step));
    assert_eq!(before, after, "get → put is a content no-op from the CLI");
    assert!(
        live.with(|a| a.autosave),
        "the same source keeps autosaving"
    );
    // A file that is neither a document nor an envelope is refused…
    let notes = dir.join("notes.json");
    std::fs::write(&notes, r#"{"hello": "world"}"#).unwrap();
    let out = tiza(&["--socket", &socket, "scene", "put", notes.to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("neither a .tiza document nor a `scene get` envelope")
            && err.contains("--as-code"),
        "{err}"
    );
    assert_eq!(live.with(|a| a.doc.code.len()), 2, "nothing was sent");
    // …unless the user asks for it as code.
    tiza_ok(&[
        "--socket",
        &socket,
        "scene",
        "put",
        notes.to_str().unwrap(),
        "--as-code",
    ]);
    assert_eq!(
        live.with(|a| a.doc.code.clone()),
        vec![r#"{"hello": "world"}"#]
    );
    assert_eq!(live.with(|a| a.doc.source.language.clone()), "json");
}

#[test]
fn put_of_another_document_never_autosaves_over_the_open_file() {
    let dir = scratch("autosave");
    let src = dir.join("stack.rs");
    std::fs::write(&src, "fn a() {}\nfn b() {}\n").unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let sidecar = dir.join("stack.rs.tiza");
    let live = Live::open(doc, sidecar.clone(), dir.join("live.sock"));
    live.command("note 2 bee");
    let saved = std::fs::read(&sidecar).unwrap();
    assert!(live.with(|a| a.autosave));
    // Another document: shown, but the sidecar on disk is left alone.
    put_file(&live, fixture("t-050/export.tiza").to_str().unwrap());
    assert_eq!(live.with(|a| a.doc.code.len()), 6);
    assert!(!live.with(|a| a.autosave), "autosave stops");
    let (msg, _) = live.message().unwrap();
    assert!(
        msg.contains("another document loaded: autosave off"),
        "{msg}"
    );
    live.command("note 1 extra");
    assert_eq!(std::fs::read(&sidecar).unwrap(), saved, "bytes untouched");
    let screen = live.screen(120, 20);
    assert!(screen.contains("unsaved (:w)"), "{screen}");
    // `:q` refuses while there are unsaved changes; `:w PATH` saves them elsewhere.
    live.command("q");
    assert!(!live.with(|a| a.quit));
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("unsaved changes (autosave off)"),
        "{msg}"
    );
    let copy = dir.join("copy.tiza");
    live.command(&format!("w {}", copy.display()));
    assert!(copy.exists());
    assert_eq!(std::fs::read(&sidecar).unwrap(), saved);
    // Back to the original with `u`: autosave stays off until asked.
    live.with(|a| a.undo());
    live.with(|a| a.undo());
    assert_eq!(live.with(|a| a.doc.code.len()), 2);
    assert!(!live.with(|a| a.autosave));
    live.command("autosave on");
    assert!(live.with(|a| a.autosave));
}
