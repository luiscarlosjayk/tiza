//! Moves of a newer Tiza and the socket.
//!
//! Acceptance criteria:
//! - AC1 A file can hold a move with an action or a value that this
//!   version does not know: the file opens, and Tiza skips the move or uses
//!   the default (see `saving::format_compatibility`). The socket stays
//!   strict: `scene.put` and `move.add` refuse such a move. So does
//!   `tiza scene put FILE`, which reads a file but sends it to the socket,
//!   and it also refuses an effect that this version does not know.
//! - AC2 In a live instance, a message names a move by its place in
//!   `move.list`, also after Tiza skipped a move of the file.
//! - AC3 `status.warnings` tells an agent what the file holds that this
//!   version does not know, and that a save does not keep it, until the
//!   first save writes the file without it.
//! - AC4 `move.add` and `scene.put` refuse a key of a move that this
//!   version does not know, with the key that it is near or the action
//!   that has it; so does `tiza scene put FILE`. The keys that `move.list`
//!   adds are not refused. A file with such a key opens (see
//!   `check::preflight`).

use serde_json::json;
use tiza_qa::{Live, fixture, scratch};

/// A live instance of the compat file with moves of a newer Tiza: 3 of
/// its 10 moves are skipped.
fn newer(name: &str) -> Live {
    let dir = scratch(name);
    let text = std::fs::read_to_string(fixture("compat/v1-newer-unknown-move.tiza")).unwrap();
    let doc = tiza_core::fs::parse_document(&text, None).unwrap();
    Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"))
}

#[test]
fn the_socket_refuses_a_move_that_this_version_does_not_know() {
    let live = newer("newer-strict");
    let refused = |method: &str, params: serde_json::Value| {
        let r = live.api(method, params.clone());
        let e = r
            .error
            .unwrap_or_else(|| panic!("{method} {params} succeeded"));
        assert_eq!(e.code, "invalid_argument", "{method} {params}: {e:?}");
        e.message
    };
    for (mv, names) in [
        (
            json!({"at_step": 1, "action": "teleport"}),
            "`code.patch`, `code.link`, `cursor`, `select`, `wait`",
        ),
        (
            json!({"at_step": 1, "action": "scroll", "line": 1, "align": "middle"}),
            "`top`, `center`, `bottom`, `show`",
        ),
        (
            json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "x", "typing": "word"}),
            "`instant`, `line`, `char`",
        ),
        (
            json!({"at_step": 1, "action": "code.patch", "op": "swap", "find": "takes"}),
            "`insert`, `replace`, `delete`",
        ),
    ] {
        let m = refused("move.add", mv.clone());
        assert!(m.contains(names), "move.add {mv}: {m}");
        let mut doc: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(fixture("compat/v1-newer-unknown-move.tiza")).unwrap(),
        )
        .unwrap();
        doc["moves"] = json!([mv]);
        let m = refused("scene.put", json!({"document": doc}));
        assert!(m.contains(names), "scene.put {mv}: {m}");
    }
    // Nothing changed.
    let moves = live.api_ok("move.list", json!({}));
    assert_eq!(moves["moves"].as_array().map(Vec::len), Some(7), "{moves}");
}

#[test]
fn a_live_instance_names_a_move_by_its_place_in_move_list() {
    // The file has 10 moves and Tiza skips 3: the live list has 7, and the
    // move being added is #8, not its number if it were in the file.
    let live = newer("newer-live-numbers");
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "scroll", "line": 1, "find": "fn"}),
    );
    let e = r.error.expect("two targets");
    assert_eq!(
        e.message, "move #8, step 1: scroll: give exactly one of line, find or annotation",
        "{e:?}"
    );
}

#[test]
fn tiza_scene_put_refuses_what_this_version_does_not_know() {
    // A file opens without what this version does not know, but a put
    // would drop it in silence.
    let live = newer("newer-scene-put");
    let dir = scratch("newer-scene-put-files");
    let mut effect: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture("compat/v1-newer-unknown-effect.tiza")).unwrap(),
    )
    .unwrap();
    effect["moves"] = json!([]);
    let envelope = dir.join("envelope.json");
    std::fs::write(
        &envelope,
        json!({"step": 1, "document": effect}).to_string(),
    )
    .unwrap();
    let moves = fixture("compat/v1-newer-unknown-move.tiza");
    for (file, named) in [
        (moves.as_path(), "move #3 (step 2, action \"teleport\")"),
        (envelope.as_path(), "b1 (\"slide\")"),
    ] {
        let out = tiza_qa::command()
            .args(["scene", "put"])
            .arg(file)
            .args(["--socket", live.socket.to_str().unwrap()])
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{}: {err}", file.display());
        assert!(
            err.contains(named)
                && err.contains("scene put sends a document as it is, so it refuses this one"),
            "{}: {err}",
            file.display()
        );
    }
    let list = live.api_ok("move.list", json!({}));
    assert_eq!(list["moves"].as_array().map(Vec::len), Some(7), "{list}");
}

#[test]
fn status_warns_an_agent_until_a_save_drops_them() {
    let live = newer("newer-status");
    let warnings = |live: &Live| -> Vec<String> {
        let st = live.api_ok("status", json!({}));
        st["warnings"]
            .as_array()
            .map(|w| w.iter().map(|x| x.as_str().unwrap().to_string()).collect())
            .unwrap_or_default()
    };
    let w = warnings(&live);
    assert!(
        w.iter().any(|x| x.contains(
            "Tiza skips 3 moves that this version does not know, and a save does not keep them: move #3 (step 2, action \"teleport\")"
        )),
        "{w:?}"
    );
    // An edit saves the document (autosave): the file holds only what this
    // version knows, and the warning goes.
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "step": 1, "line": 1, "text": "x"}),
    );
    let w = warnings(&live);
    assert!(!w.iter().any(|x| x.contains("not know")), "{w:?}");
    let saved = std::fs::read_to_string(scratch_file(&live)).unwrap();
    assert!(!saved.contains("teleport"), "{saved}");
}

/// The file that a live instance of [`newer`] saves to.
fn scratch_file(live: &Live) -> std::path::PathBuf {
    live.with(|a| a.save_path.clone())
}

#[test]
fn the_socket_refuses_a_key_of_a_move_that_this_version_does_not_know() {
    let live = newer("newer-keys");
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "scroll", "line": 1, "alig": "top"}),
    );
    let e = r.error.expect("refused");
    assert_eq!(e.code, "invalid_argument", "{e:?}");
    assert_eq!(
        e.message,
        "invalid parameters: unknown field `alig` of a scroll move, expected one of `at_step`, `action`, `line`, `find`, `occurrence`, `annotation`, `align`. Did you mean `align`?"
    );
    // A key of another action: most likely the `action` is missing.
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "line": 3}),
    );
    let e = r.error.expect("refused");
    assert!(
        e.message
            .ends_with("`line` is a key of a cursor or scroll move: send the `action` of the move"),
        "{e:?}"
    );
    // `scene.put`: the document of `scene.get`, with one key more.
    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
    assert_eq!(doc["moves"][2]["action"], "scroll", "{doc}");
    doc["moves"][2]["alig"] = json!("top");
    let r = live.api("scene.put", json!({"document": doc.clone()}));
    let e = r.error.expect("refused");
    assert!(
        e.message.starts_with(
            "invalid parameters: move #3: unknown field `alig` of a scroll move, expected one of"
        ),
        "{e:?}"
    );
    // Without it, the put changes nothing.
    doc["moves"][2].as_object_mut().unwrap().remove("alig");
    live.api_ok("scene.put", json!({"document": doc}));
    // An entry of `move.list` goes back to `move.add` with its own keys.
    let list = live.api_ok("move.list", json!({}));
    let entry = list["moves"][1].clone();
    assert_eq!(entry["action"], "cursor", "{entry}");
    assert!(
        entry["index"].is_number() && entry["summary"].is_string(),
        "{entry}"
    );
    live.api_ok("move.delete", json!({"index": 2}));
    live.api_ok("move.add", entry);
    // `tiza scene put` of a file with such a key.
    let dir = scratch("newer-keys-file");
    let file = dir.join("typo.tiza");
    std::fs::write(
        &file,
        json!({
            "schema": 1,
            "source": {"language": "rs", "first_line": 1},
            "code": ["fn a() {}"],
            "moves": [{"at_step": 1, "action": "cursor", "line": 1, "colum": 2}]
        })
        .to_string(),
    )
    .unwrap();
    let out = tiza_qa::command()
        .args(["scene", "put"])
        .arg(&file)
        .args(["--socket", live.socket.to_str().unwrap()])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(
        err.contains(
            "1 move has a key that this version of Tiza does not know: move #1 (step 1, key colum)"
        ),
        "{err}"
    );
    let list = live.api_ok("move.list", json!({}));
    assert_eq!(list["moves"].as_array().map(Vec::len), Some(7), "{list}");
}

#[test]
fn a_copy_keeps_the_warning_and_save_as_takes_it_away() {
    // `:w COPY` leaves the file of the document as it was, with the moves
    // that this version does not know. `:saveas` makes the new file, which
    // does not hold them, the file of the document.
    let live = newer("newer-saveas");
    live.with(|a| a.autosave = false);
    let dir = scratch("newer-saveas-files");
    let warned = |live: &Live| {
        let st = live.api_ok("status", json!({}));
        st["warnings"].as_array().is_some_and(|w| {
            w.iter()
                .any(|x| x.as_str().unwrap().contains("Tiza skips 3 moves"))
        })
    };
    assert!(warned(&live));
    live.command(&format!("w {}", dir.join("copy.tiza").display()));
    assert!(dir.join("copy.tiza").is_file());
    assert!(
        warned(&live),
        "a copy does not change the file of the document"
    );
    live.command(&format!("saveas {}", dir.join("new.tiza").display()));
    assert!(dir.join("new.tiza").is_file());
    assert!(
        !warned(&live),
        "the new file holds only what this version knows"
    );
}
