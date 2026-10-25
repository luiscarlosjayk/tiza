//! Two socket requests could lose the work of the presenter: a
//! `scene.clear` whose `step` Tiza could not read cleared every
//! annotation, and a `scene.put` with a newer `schema` was saved in a file
//! that this version cannot open.
//!
//! Acceptance criteria:
//! - AC1 `scene.clear` refuses a `step` that is not a whole number, 0 or
//!   more (`"2"`, `-1`, `1.5`, `null`, 2^32 + 2, which `as u32` made step
//!   2), parameters that are not an object (`null`, `[]`) and an unknown
//!   key, with `invalid_argument`,
//!   and changes nothing. `{}` clears every annotation and `{"step": N}`
//!   the annotations of step N, as before. In
//!   supervised mode the refusal comes at once: the request is not parked.
//!   The MCP tool with no step still clears every annotation; it refuses
//!   an unknown key and a `null` step.
//! - AC2 `scene.put` refuses a `schema` newer than this version with
//!   `unsupported_schema`, before the document changes or is saved. An
//!   older `schema` is saved as the current one. A file and a `scene.put`
//!   give the same error, with the schema as it was written.
//! - AC3 In supervised mode, a method that does not exist is answered at
//!   once (`invalid_argument`), not parked as a change to review.
//!   Parking follows `METHODS`: each method there is one that the instance
//!   knows, and a name that is not there gets `unknown method`.
//! - AC4 A request that changes nothing (`scene.patch {}`, an
//!   `annotation.update` with only its `id`) leaves `revision` as it is. A
//!   request that changes something makes it grow (by 1 or more).
//! - AC5 The result schema of `scene.clear`, `code.get`, `style.set` and
//!   `theme.set` names every field of the reply.
//! - AC6 The help of `tiza scene patch` and the `scene.patch` description
//!   name every field that `scene.patch` takes.
//! - AC7 `tiza FILE:A-B` opens `FILE.A-B.tiza` from an earlier session, found
//!   under the clamped lines, and says so. Another range is another
//!   document. A `FILE.A-B.tiza` that does not load is an error, and the
//!   file stays as it is. `tiza render FILE:A-B` still shows only the
//!   lines, without that document.

use serde_json::json;
use tiza_qa::Live;

/// The number of annotations and the revision.
fn state(live: &Live) -> (u64, u64) {
    let s = live.api_ok("status", json!({}));
    (
        s["annotations"].as_u64().unwrap(),
        s["revision"].as_u64().unwrap(),
    )
}

#[test]
fn ac7_a_range_opens_its_document_from_an_earlier_session() {
    use tiza_core::fs;
    let dir = tiza_qa::scratch("t258-range");
    let file = dir.join("three.txt");
    std::fs::write(&file, "one\ntwo\nthree\n").unwrap();
    let spec = |r: &str| format!("{}:{r}", file.display());
    // Session 1: a note on lines 2-3, autosaved.
    let o = fs::open(&spec("2-3")).unwrap();
    assert!(!o.existing);
    let save = dir.join("three.txt.2-3.tiza");
    assert_eq!(o.save_path, save);
    {
        let live = Live::open(o.document, o.save_path, dir.join("a.sock"));
        live.api_ok(
            "annotation.add",
            json!({"type": "note", "line": 2, "text": "kept"}),
        );
    }
    // Session 2, the same range and one that clamps to it: the note is there.
    for (n, r) in ["2-3", "2..9"].iter().enumerate() {
        let o = fs::open(&spec(r)).unwrap();
        assert!(o.existing, "{r}");
        assert_eq!(o.save_path, save, "{r}");
        assert!(
            o.notice
                .as_deref()
                .unwrap_or("")
                .contains("opened the existing"),
            "{r}: {:?}",
            o.notice
        );
        let live = Live::open(o.document, o.save_path, dir.join(format!("b{n}.sock")));
        assert_eq!(state(&live).0, 1, "{r}");
    }
    // `tiza render` shows only the lines of a range, without its document.
    let out = std::process::Command::new(tiza_qa::bin())
        .args(["render", &spec("2-3"), "--format", "txt"])
        .output()
        .unwrap();
    let (text, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{err}");
    assert!(text.contains("two") && !text.contains("kept"), "{text}");
    assert!(!err.contains("opened the existing"), "{err}");
    // Another range is another document.
    let o = fs::open(&spec("1-2")).unwrap();
    assert!(!o.existing && o.document.annotations.is_empty());
    assert_eq!(o.save_path, dir.join("three.txt.1-2.tiza"));
    // A document there that does not load: an error, and the file stays.
    std::fs::write(&save, "not json").unwrap();
    assert!(fs::open(&spec("2-3")).is_err());
    assert_eq!(std::fs::read_to_string(&save).unwrap(), "not json");
}
