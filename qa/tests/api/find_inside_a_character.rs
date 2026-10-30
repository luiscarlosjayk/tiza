//! A `find` whose text starts or ends inside a character (a grapheme: `d`
//! with a joiner, `e` with an accent, a family emoji). The span took the
//! graphemes that start before each end of the text, so the joiner alone
//! in `ad` U+200D `min` gave an empty span on the `m`.
//!
//! Acceptance criteria:
//! - AC1 Where the column counts (the `at` of a highlight or a mark, the
//!   `from` and `to` of an arrow, `annotation.update` with `move`, the
//!   points of a `select` move, the `rebind` of a span or an arrow end),
//!   such a `find` is refused with `invalid_argument`. The message names
//!   the character, its line and its column, and says to put the whole
//!   character in the find or to use `line` and `col`.
//! - AC2 Where only the line counts (the markers of a box or a callout, the
//!   `at` of a note in a `rebind`, the target of a `code.patch`), it works
//!   as before.
//! - AC3 A whole character, and `occurrence`, work as before: the count of
//!   `occurrence` takes every hit.

use serde_json::{Value, json};
use tiza_qa::Live;

const CODE: &str =
    "let ad\u{200D}min = 1;\nlet cafe\u{301} = \"e\";\nlet f = \"\u{1F468}\u{200D}\u{1F469}\";\n";

fn refused(live: &Live, method: &str, params: Value) -> String {
    let r = live.api(method, params.clone());
    let e = r
        .error
        .unwrap_or_else(|| panic!("{method} {params} succeeded"));
    assert_eq!(e.code, "invalid_argument", "{method} {params}: {e:?}");
    e.message
}

const JOINER: &str = "text \"\\u{200d}\" starts inside the character \"d\\u{200d}\" at line 1, col 5: \
     put the whole character in the find, or use line and col";

#[test]
fn a_find_inside_a_character_is_refused_where_the_column_counts() {
    let live = Live::from_code(CODE, "rs", "inside");
    live.api_ok(
        "annotation.add",
        json!({"id": "h1", "type": "highlight", "at": {"line": 1, "col": 0, "end": 3}}),
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "r1", "type": "arrow", "from": {"line": 1, "col": 0}, "to": {"line": 2, "col": 0}}),
    );
    let joiner = json!({"find": "\u{200D}"});
    let cases = [
        ("annotation.add", json!({"type": "highlight", "at": joiner})),
        (
            "annotation.add",
            json!({"type": "mark", "kind": "warn", "at": joiner}),
        ),
        (
            "annotation.add",
            json!({"type": "arrow", "from": joiner, "to": {"line": 2, "col": 0}}),
        ),
        (
            "annotation.add",
            json!({"type": "arrow", "from": {"line": 2, "col": 0}, "to": joiner}),
        ),
        (
            "annotation.update",
            json!({"id": "h1", "move": {"at": joiner}}),
        ),
        (
            "annotation.update",
            json!({"id": "r1", "move": {"from": joiner}}),
        ),
    ];
    for (method, params) in cases {
        assert_eq!(
            refused(&live, method, params.clone()),
            JOINER,
            "{method} {params}"
        );
    }
    // The end of the text inside a character, the accent alone, and one
    // person of a family emoji.
    for (find, edge, character, line, col) in [
        ("cafe", "ends", "e\\u{301}", 2, 7),
        ("\u{301}", "starts", "e\\u{301}", 2, 7),
        ("\u{1F468}", "ends", "\u{1F468}\\u{200d}\u{1F469}", 3, 9),
    ] {
        let message = refused(
            &live,
            "annotation.add",
            json!({"type": "highlight", "at": {"find": find}}),
        );
        assert!(
            message.contains(&format!(
                "{edge} inside the character \"{character}\" at line {line}, col {col}"
            )),
            "{find:?}: {message}"
        );
    }
    // Nothing changed.
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2);
}

#[test]
fn a_whole_character_and_occurrence_work_as_before() {
    let live = Live::from_code(CODE, "rs", "whole");
    let span = |params: Value| -> (u32, u32, u32) {
        let r = live.api_ok("annotation.add", params);
        let id = r["id"].as_str().unwrap().to_string();
        live.with(move |a| match &a.doc.find_annotation(&id).unwrap().shape {
            tiza_core::model::Shape::Highlight { span, .. } => (span.line, span.start, span.end),
            s => panic!("{s:?}"),
        })
    };
    assert_eq!(
        span(json!({"type": "highlight", "at": {"find": "d\u{200D}"}})),
        (1, 5, 6)
    );
    assert_eq!(
        span(json!({"type": "highlight", "at": {"find": "cafe\u{301}"}})),
        (2, 4, 8)
    );
    assert_eq!(
        span(json!({"type": "highlight", "at": {"find": "\u{1F468}\u{200D}\u{1F469}"}})),
        (3, 9, 10)
    );
    // On line 2, `e` is in `let`, inside `e` + accent, then in quotes: the
    // count takes all three, so the third is the one in quotes, and the
    // second is refused.
    assert_eq!(
        span(json!({"type": "highlight", "at": {"line": 2, "find": "e", "occurrence": 3}})),
        (2, 12, 13)
    );
    let message = refused(
        &live,
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 2, "find": "e", "occurrence": 2}}),
    );
    assert!(message.contains("at line 2, col 7"), "{message}");
}

#[test]
fn where_only_the_line_counts_a_find_inside_a_character_works() {
    let live = Live::from_code(CODE, "rs", "lines");
    // A box and a callout from the joiner to the accent: lines 1 to 2, as
    // with whole characters.
    let boxed = live.api_ok(
        "annotation.add",
        json!({"type": "box", "find_start": "\u{200D}", "find_end": "\u{301}"}),
    );
    let called = live.api_ok(
        "annotation.add",
        json!({"type": "callout", "find_start": "\u{200D}", "find_end": "\u{301}", "text": "here"}),
    );
    let lines = |id: &str| {
        let id = id.to_string();
        live.with(move |a| match &a.doc.find_annotation(&id).unwrap().shape {
            tiza_core::model::Shape::Box {
                from_line, to_line, ..
            } => (*from_line, *to_line),
            tiza_core::model::Shape::Callout {
                from_line, to_line, ..
            } => (*from_line, to_line.unwrap_or(*from_line)),
            s => panic!("{s:?}"),
        })
    };
    assert_eq!(lines(boxed["id"].as_str().unwrap()), (1, 2));
    assert_eq!(lines(called["id"].as_str().unwrap()), (1, 2));
    // The same frame, byte for byte, as with the lines given.
    let other = Live::from_code(CODE, "rs", "lines-given");
    other.api_ok(
        "annotation.add",
        json!({"type": "box", "from_line": 1, "to_line": 2}),
    );
    other.api_ok(
        "annotation.add",
        json!({"type": "callout", "from_line": 1, "to_line": 2, "text": "here"}),
    );
    let frame = |l: &Live| l.api_ok("render.text", json!({"width": 60}))["text"].clone();
    assert_eq!(frame(&live), frame(&other));
    // A code.patch target picks a line: the joiner finds line 1.
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "\u{200D}", "occurrence": 1, "content": "let admin = 1;"}),
    );
    live.api_ok("step.set", json!({"step": 2}));
    let code = live.api_ok("code.get", json!({}));
    assert!(
        code["text"]
            .as_str()
            .unwrap()
            .starts_with("   1  let admin = 1;\n"),
        "{code}"
    );
}

#[test]
fn a_move_with_a_find_inside_a_character_is_refused_where_the_column_counts() {
    let live = Live::from_code(CODE, "rs", "moves");
    live.api_ok(
        "annotation.add",
        json!({"id": "h1", "step": 1, "type": "highlight", "at": {"line": 1, "col": 0, "end": 3}}),
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "n1", "step": 1, "type": "note", "line": 1, "text": "a note"}),
    );
    // The points of a select. (The joiner is also in the family on line 3,
    // so a move target names its first hit.)
    let message = refused(
        &live,
        "move.add",
        json!({"at_step": 2, "action": "select", "from": {"find": "\u{200D}", "occurrence": 1}, "to": {"line": 2, "col": 3}}),
    );
    assert_eq!(message, format!("move #1, step 2: select from: {JOINER}"));
    // The rebind of a span.
    let patch = |rebind: Value| {
        json!({"at_step": 2, "action": "code.patch", "op": "insert", "position": "1",
               "content": "// new", "rebind": rebind})
    };
    let message = refused(
        &live,
        "move.add",
        patch(json!({"h1": {"at": {"find": "\u{200D}", "occurrence": 1}}})),
    );
    // After the insert at line 1, the joiner is on line 2.
    assert_eq!(
        message,
        format!(
            "move #1, step 2: rebind «h1»: {}",
            JOINER.replace("line 1,", "line 2,")
        )
    );
    assert_eq!(live.with(|a| a.doc.moves.len()), 0);
    // The rebind of a note takes only the line: it works. The second
    // joiner is inside the family, on line 4 after the insert.
    live.api_ok(
        "move.add",
        patch(json!({"n1": {"at": {"find": "\u{200D}", "occurrence": 2}}})),
    );
    // At step 2 the note is on the line of the family (a shift alone would
    // put it on line 2).
    let line = live.with(|a| {
        let view = a.doc.view_at(2);
        match &view.doc.find_annotation("n1").unwrap().shape {
            tiza_core::model::Shape::Note { line, .. } => *line,
            s => panic!("{s:?}"),
        }
    });
    assert_eq!(line, 4);
}

/// The edges that give a column: an arrow end and the start of a selection
/// use the start of the text, the end of a selection its end. `cafe` starts
/// on the edge of `c` and ends inside `e` + accent.
#[test]
fn only_the_edges_that_give_a_column_count() {
    let live = Live::from_code(CODE, "rs", "edges");
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "arrow", "from": {"find": "cafe"}, "to": {"line": 1, "col": 0}}),
    );
    let id = r["id"].as_str().unwrap().to_string();
    let from = live.with(move |a| match &a.doc.find_annotation(&id).unwrap().shape {
        tiza_core::model::Shape::Arrow { from, .. } => (from.line, from.col),
        s => panic!("{s:?}"),
    });
    assert_eq!(from, (2, 4));
    // A select from `cafe` to the accent alone: both edges are on the
    // edges of characters.
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "select", "from": {"find": "cafe"}, "to": {"find": "\u{301}"}}),
    );
    live.api_ok("move.delete", json!({"index": 1}));
    // The end of a selection inside `e` + accent, and the start of an arrow
    // there.
    let message = refused(
        &live,
        "move.add",
        json!({"at_step": 2, "action": "select", "from": {"line": 1, "col": 0}, "to": {"find": "cafe"}}),
    );
    assert!(
        message.starts_with("move #1, step 2: select to: text \"cafe\" ends inside"),
        "{message}"
    );
    let message = refused(
        &live,
        "annotation.add",
        json!({"type": "arrow", "from": {"find": "\u{301}"}, "to": {"line": 1, "col": 0}}),
    );
    assert!(message.contains("starts inside the character"), "{message}");
}

#[test]
fn rebinds_and_updates_follow_the_shape() {
    let live = Live::from_code(CODE, "rs", "rebinds");
    live.api_ok(
        "annotation.add",
        json!({"id": "r1", "step": 1, "type": "arrow", "from": {"line": 1, "col": 0}, "to": {"line": 2, "col": 0}}),
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "n1", "step": 1, "type": "note", "line": 1, "text": "a note"}),
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "h1", "step": 1, "type": "highlight", "at": {"line": 1, "col": 0, "end": 3}}),
    );
    let patch = |rebind: Value| {
        json!({"at_step": 2, "action": "code.patch", "op": "insert", "position": "1",
               "content": "// new", "rebind": rebind})
    };
    // An arrow end uses the start of the text: the accent alone starts
    // inside `e` + accent; `cafe` starts on the edge of `c`.
    let message = refused(
        &live,
        "move.add",
        patch(json!({"r1": {"from": {"find": "\u{301}"}}})),
    );
    assert!(
        message.starts_with("move #1, step 2: rebind «r1»: text \"\\u{301}\" starts inside"),
        "{message}"
    );
    // With `line`, the search starts on that line: the joiner of the
    // family on line 4 after the insert.
    let message = refused(
        &live,
        "move.add",
        patch(json!({"h1": {"at": {"line": 4, "find": "\u{200D}"}}})),
    );
    assert!(message.contains("at line 4, col 9"), "{message}");
    live.api_ok(
        "move.add",
        patch(json!({"r1": {"from": {"find": "cafe"}}, "n1": {"at": {"find": "\u{200D}", "occurrence": 1}}})),
    );
    // `annotation.update` with `move`: a note takes only the line.
    live.api_ok(
        "annotation.update",
        json!({"id": "n1", "move": {"at": {"find": "\u{200D}", "occurrence": 1}}}),
    );
    // A note whose rebind is fine for a line cannot become a highlight:
    // its rebind would start inside `d` + joiner, and the file would not
    // open again.
    let message = refused(
        &live,
        "annotation.update",
        json!({"id": "n1", "shape": {"type": "highlight", "at": {"line": 1, "col": 0, "end": 3}}}),
    );
    assert!(
        message.starts_with("annotation n1: move #1 (step 2) has a rebind for n1 that would not work with this change (rebind «n1»: text \"\\u{200d}\" starts inside"),
        "{message}"
    );
    let kind = live.with(|a| match &a.doc.find_annotation("n1").unwrap().shape {
        tiza_core::model::Shape::Note { .. } => "note",
        _ => "other",
    });
    assert_eq!(kind, "note");
}

#[test]
fn scene_put_and_the_pointer() {
    let live = Live::from_code(CODE, "rs", "put");
    let document = |find: &str| {
        json!({"document": {
            "schema": 1, "source": {"language": "rs"},
            "code": ["let ad\u{200D}min = 1;", "fn b() {}"],
            "moves": [{"at_step": 1, "action": "select", "from": {"find": find}, "to": {"find": "b()"}}]
        }})
    };
    let message = refused(&live, "scene.put", document("\u{200D}"));
    assert!(
        message.contains("select from: text \"\\u{200d}\" starts inside"),
        "{message}"
    );
    live.api_ok("scene.put", document("d\u{200D}"));
    // The pointer takes only lines.
    live.api_ok("pointer.set", json!({"line": 1, "find": "\u{200D}"}));
}
