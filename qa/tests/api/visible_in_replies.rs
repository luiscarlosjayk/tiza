//! `visible` in the `annotation.add` / `annotation.update` replies said
//! `true` for a note the layout does not draw: `tiza check` said `a3 … has
//! no room and is not drawn` and the screen did not show it, so an agent
//! believed the audience saw a note that was not there. `visible` looked
//! at the step, the file and the window, not at the layout.
//!
//! Acceptance criteria:
//! - AC1 The replies (socket and MCP) say `visible: false` when the
//!   annotation is at the current step, file and window but the layout at
//!   the current terminal size does not draw it, with the reason in
//!   `hidden` (`it is not drawn: no room for its text at 100 columns (check
//!   at width 100 lists it)`).
//! - AC2 A label drawn but clipped with «…» stays `visible: true` and says
//!   so in `clipped`.
//! - AC3 The status-bar notice for agent changes covers it: `a3: note L2
//!   added by agent (not drawn: no room at 100 columns)`.
//! - AC4 The same answer as `tiza check` for that annotation at that width
//!   (the layout's own list of cut labels is the one source).
//! - AC5 This file; agent guide (what `visible` means); CHANGELOG.
//!
//! Decisions: only a note or a callout is not visible when its text is
//! left out (its text is all it draws); a box, an arrow, a highlight or a
//! mark is still drawn, so it stays visible and `clipped` says its label is
//! not drawn. The step is laid out as presentation mode, the exports and
//! `check` do (the take, once typed), at the screen's code area (the
//! terminal less its side padding, within the widths `check` takes), so
//! `check` at the width the reply names agrees.
//!
//! Review (`rev_*`): a later step with arrows (the take differs from the
//! authoring screen there), a note added while its step is typed, a
//! terminal narrower than `check` takes, a grouped bar notice, and `check`
//! keeping a label cut at one step apart from the same label left out at
//! another (an arrow label beside the code of its target line, which a
//! label that comes before it at a later step pushes out of the code
//! column).
//!
//! Corner cases: the reply is about the size at that moment (a resize
//! later changes nothing in it); presentation mode follows the same rule;
//! with no terminal size known the default width (100) is used and named.

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, scratch};

const ENV: &str = "DATABASE_URL=postgres://app:s3cr3t-pass@db.internal:5432/app\n\
                   API_TOKEN=tok_live_9f8e7d6c5b4a3f2e1d0c\n\
                   LOG_LEVEL=info\n\
                   PORT=8080\n";

/// Eight arrows from L1 to L4 take eight lanes left of the code, so a
/// label under a line starts 23 columns in: up to 24 columns it has no room
/// and is left out, from 25 it is cut with …, from 35 `secret token` is
/// whole under its line, and from 97 the labels go to the margin. (Until
/// the margin callouts the scenario was a callout's block on L1 that took the margin rows
/// of L2 and L3 at 100 columns; since then those labels go under their
/// line, so a label has no room only where the code itself leaves none.)
fn opened(name: &str) -> (Live, std::path::PathBuf) {
    opened_with(name, base_doc())
}

fn base_doc() -> Value {
    json!({
        "schema": 1,
        "source": {"path": "demo.env", "language": "env", "first_line": 1},
        "code": {"file": "demo.env"},
        "annotations": (1..=8)
            .map(|i| json!({
                "id": format!("r{i}"), "step": 1, "type": "arrow",
                "from": {"line": 1, "col": 0}, "to": {"line": 4, "col": 0}
            }))
            .collect::<Vec<_>>(),
        "step": 1
    })
}

fn opened_with(name: &str, doc: Value) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    std::fs::write(dir.join("demo.env"), ENV).unwrap();
    let tiza = dir.join("demo.env.tiza");
    std::fs::write(&tiza, doc.to_string()).unwrap();
    let loaded = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(loaded, tiza.clone(), dir.join("live.sock"));
    (live, tiza)
}

/// A terminal of `cols` columns: the code area is 2 narrower (the side
/// padding).
fn terminal(live: &Live, cols: u16) {
    live.with(move |app| app.size = (cols, 30));
}

fn add(live: &Live, params: Value) -> Value {
    live.api_ok("annotation.add", params)
}

/// `tiza check --json --width W` on the autosave: each annotation's
/// `clipped_label` findings, as (id, has no room).
fn check_cuts(tiza: &std::path::Path, width: usize) -> Vec<(String, bool)> {
    check_findings(tiza, width)
        .into_iter()
        .map(|(id, no_room, _)| (id, no_room))
        .collect()
}

/// [`check_cuts`] with the first step of each finding.
fn check_findings(tiza: &std::path::Path, width: usize) -> Vec<(String, bool, u64)> {
    let out = tiza_qa::command()
        .args(["check", "--json", "--width", &width.to_string()])
        .arg(tiza)
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "clipped_label")
        .map(|f| {
            (
                f["id"].as_str().unwrap_or_default().to_string(),
                f["message"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("has no room"),
                f["step"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn a_note_with_no_room_is_not_visible() {
    // Test step 1.
    let (live, tiza) = opened("no-room");
    terminal(&live, 26);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    assert_eq!(r["visible"], false, "{r}");
    assert_eq!(
        r["hidden"],
        "it is not drawn: no room for its text at 24 columns (check at width 24 lists it)"
    );
    assert!(r.get("clipped").is_none(), "{r}");
    // AC4: `tiza check` at that width says the same of it.
    let id = r["id"].as_str().unwrap().to_string();
    assert!(
        check_cuts(&tiza, 24).contains(&(id.clone(), true)),
        "{:?}",
        check_cuts(&tiza, 24)
    );
    // And the screen does not show it.
    assert!(!live.screen(26, 30).contains("secret token"));
}

#[test]
fn widened_until_drawn_the_update_says_visible() {
    // Test step 2.
    let (live, tiza) = opened("widen");
    terminal(&live, 26);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    assert_eq!(r["visible"], false, "{r}");
    let id = r["id"].as_str().unwrap().to_string();
    terminal(&live, 40);
    let r = live.api_ok("annotation.update", json!({"id": id, "color": "green"}));
    assert_eq!(r["visible"], true, "{r}");
    assert!(
        r.get("hidden").is_none() && r.get("clipped").is_none(),
        "{r}"
    );
    assert!(
        check_cuts(&tiza, 38).is_empty(),
        "{:?}",
        check_cuts(&tiza, 38)
    );
    let s = live.screen(40, 30);
    assert!(s.contains("secret token"), "{s}");
}

#[test]
fn a_clipped_label_stays_visible_and_says_so() {
    // Test step 3: a long note on L3 in the margin, cut with … at 120
    // columns.
    let (live, tiza) = opened("clipped");
    terminal(&live, 122);
    let r = add(
        &live,
        json!({"type": "note", "line": 3, "text": "the log level of the whole service, read once when it starts and never again"}),
    );
    assert_eq!(r["visible"], true, "{r}");
    assert_eq!(
        r["clipped"],
        "its label is cut with … at 120 columns (check at width 120 lists it)"
    );
    let id = r["id"].as_str().unwrap().to_string();
    assert!(check_cuts(&tiza, 120).contains(&(id, false)));
}

#[test]
fn a_highlight_whose_label_has_no_room_is_still_drawn() {
    // The highlight is drawn; only its label is not.
    let (live, tiza) = opened("highlight");
    terminal(&live, 26);
    let r = add(
        &live,
        json!({"type": "highlight", "at": {"line": 2, "find": "tok_live"}, "label": "secret token"}),
    );
    assert_eq!(r["visible"], true, "{r}");
    assert_eq!(
        r["clipped"],
        "its label is not drawn: no room at 24 columns (check at width 24 lists it)"
    );
    let id = r["id"].as_str().unwrap().to_string();
    assert!(check_cuts(&tiza, 24).contains(&(id, true)));
}

#[test]
fn the_mcp_reply_says_it() {
    let (live, _tiza) = opened("mcp");
    terminal(&live, 26);
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("add_note", json!({"line": 2, "text": "secret token"}));
    assert!(!err, "{text}");
    assert!(
        text.contains("not visible now")
            && text.contains("it is not drawn: no room for its text at 24 columns"),
        "{text}"
    );
}

#[test]
fn the_bar_says_it() {
    let (live, _tiza) = opened("bar");
    terminal(&live, 26);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    let id = r["id"].as_str().unwrap().to_string();
    assert_eq!(
        live.message(),
        Some((
            format!("{id}: note L2 added by agent (not drawn: no room at 24 columns)"),
            false
        ))
    );
}

#[test]
fn a_note_that_fits_is_visible_as_before() {
    let (live, _tiza) = opened("fits");
    terminal(&live, 122);
    let r = add(&live, json!({"type": "note", "line": 3, "text": "info"}));
    assert_eq!(r["visible"], true, "{r}");
    assert!(
        r.get("hidden").is_none() && r.get("clipped").is_none(),
        "{r}"
    );
    // Not on screen for another reason: that reason, as before.
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "later", "step": 2}),
    );
    assert_eq!(r["visible"], false);
    assert_eq!(r["hidden"], "it appears at step 2");
}

#[test]
fn presentation_mode_follows_the_same_rule() {
    let (live, _tiza) = opened("present");
    live.with(|app| {
        app.size = (26, 30);
        app.set_present(true);
    });
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    assert_eq!(r["visible"], false, "{r}");
    // 26 columns less the default side padding (1 each side).
    assert!(
        r["hidden"].as_str().unwrap().contains("at 24 columns"),
        "{r}"
    );
}

#[test]
fn no_terminal_size_uses_the_default_width_and_names_it() {
    let (live, _tiza) = opened("headless");
    live.with(|app| app.size = (0, 0));
    assert_eq!(live.with(|app| app.screen_width()), 100);
    // At 100 the labels are in the margin; this one, on the last line,
    // has no rows below to wrap into and is cut.
    let r = add(
        &live,
        json!({"type": "note", "line": 4, "text": "the secret token the API calls carry"}),
    );
    assert_eq!(r["visible"], true, "{r}");
    assert!(
        r["clipped"].as_str().unwrap().contains("at 100 columns"),
        "{r}"
    );
}

// ---------- review ----------

/// The reply's verdict on `id`, as check's words: `Some(true)` left out,
/// `Some(false)` cut with …, `None` drawn whole; and the width it names.
fn verdict(r: &Value) -> (Option<bool>, Option<usize>) {
    let text = r["hidden"].as_str().or(r["clipped"].as_str());
    let width = text.and_then(|t| {
        let at = t.find(" columns")?;
        t[..at].rsplit(' ').next()?.parse().ok()
    });
    let v = if r["visible"] == false {
        Some(true)
    } else {
        r["clipped"].as_str().map(|c| c.contains("not drawn"))
    };
    (v, width)
}

#[test]
fn a_later_step_with_arrows_the_reply_is_what_check_says() {
    // Arrows at step 2 make the take keep room for their rails at step 1
    // too, where the authoring screen does not: the reply follows the take.
    let mut doc = base_doc();
    doc["annotations"].as_array_mut().unwrap().extend([
        json!({"id": "s1", "step": 2, "type": "arrow", "from": {"line": 1, "col": 0}, "to": {"line": 4, "col": 0}}),
        json!({"id": "s2", "step": 2, "type": "arrow", "from": {"line": 2, "col": 0}, "to": {"line": 4, "col": 0}}),
        json!({"id": "s3", "step": 2, "type": "arrow", "from": {"line": 3, "col": 0}, "to": {"line": 1, "col": 0}}),
    ]);
    let (live, tiza) = opened_with("arrows", doc);
    terminal(&live, 102);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token", "step": 1}),
    );
    let id = r["id"].as_str().unwrap().to_string();
    let mut seen = std::collections::BTreeSet::new();
    for cols in 22..=60u16 {
        terminal(&live, cols);
        let r = live.api_ok("annotation.update", json!({"id": id, "color": "green"}));
        let (v, named) = verdict(&r);
        let width = usize::from(cols) - 2;
        if v.is_some() {
            assert_eq!(named, Some(width), "{cols}: {r}");
        }
        // The note first appears at step 1, the step on screen.
        let from_check = check_findings(&tiza, width)
            .into_iter()
            .find(|(i, _, step)| *i == id && *step == 1)
            .map(|(_, no_room, _)| no_room);
        assert_eq!(v, from_check, "{cols} columns: reply {r}");
        seen.insert(format!("{v:?}"));
    }
    // The sweep crosses both answers.
    assert!(seen.len() >= 2, "{seen:?}");
}

#[test]
fn a_note_added_while_its_step_is_typed() {
    // Right after `next_step` the step's typing runs: the reply is about
    // the step once typed, as the screen shows it then. One character an
    // hour: the typing does not end by its clock during the test.
    let mut doc = base_doc();
    doc["step"] = json!(0);
    doc["moves"] = json!([{
        "at_step": 1, "action": "code.patch", "op": "replace", "find": "PORT=8080",
        "content": "PORT=9090", "typing": "char", "speed_ms": 3_600_000
    }]);
    let (live, tiza) = opened_with("typing", doc);
    terminal(&live, 26);
    live.api_ok("step.next", json!({}));
    assert!(live.with(|app| app.typing.is_some()), "the step is typing");
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    assert!(live.with(|app| app.typing.is_some()), "still typing");
    assert_eq!(r["visible"], false, "{r}");
    let id = r["id"].as_str().unwrap().to_string();
    assert!(check_cuts(&tiza, 24).contains(&(id, true)));
}

#[test]
fn a_terminal_narrower_than_check_takes_names_a_width_it_takes() {
    let (live, tiza) = opened("tiny");
    terminal(&live, 12);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "the secret token the API calls carry"}),
    );
    let (v, named) = verdict(&r);
    assert_eq!(named, Some(20), "{r}");
    let id = r["id"].as_str().unwrap().to_string();
    let from_check = check_cuts(&tiza, 20)
        .into_iter()
        .find(|(i, _)| *i == id)
        .map(|(_, no_room)| no_room);
    assert_eq!(v, from_check, "{r}");
}

#[test]
fn a_grouped_notice_with_a_note_that_has_no_room_has_no_step_hint() {
    // `press n` would never show the note that has no room.
    let (live, _tiza) = opened("group");
    terminal(&live, 26);
    add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    add(
        &live,
        json!({"type": "box", "from_line": 3, "to_line": 4, "step": 2}),
    );
    let (msg, _) = live.message().unwrap();
    assert!(msg.starts_with("2 agent annotations not shown"), "{msg}");
    assert!(!msg.contains("press n"), "{msg}");
}

#[test]
fn check_keeps_a_label_cut_at_one_step_apart_from_it_left_out_at_another() {
    // At 88 columns the labels go beside the code of L4, one after the
    // other. At step 1 `a2` comes second and is cut with …; at step 2 `a0`
    // comes first and leaves `a2` no room. (Until the margin callouts a callout's block at
    // step 2 took the note's row; its labels now go under their line.)
    let mut doc = base_doc();
    doc["annotations"].as_array_mut().unwrap().extend([
        json!({"id": "a0", "step": 2, "type": "arrow", "from": {"line": 2, "col": 0},
            "to": {"line": 4, "col": 0}, "label": "the port every client of the service dials"}),
        json!({"id": "a1", "step": 1, "type": "arrow", "from": {"line": 3, "col": 0},
            "to": {"line": 4, "col": 0}, "label": "logs at info say which port"}),
        json!({"id": "a2", "step": 1, "type": "arrow", "from": {"line": 1, "col": 0},
            "to": {"line": 4, "col": 0}, "label": "the database answers on another port"}),
    ]);
    doc["step"] = json!(2);
    let (live, tiza) = opened_with("steps", doc);
    terminal(&live, 90);
    let mut findings: Vec<(bool, u64)> = check_findings(&tiza, 88)
        .into_iter()
        .filter(|(i, _, _)| i == "a2")
        .map(|(_, no_room, step)| (no_room, step))
        .collect();
    findings.sort();
    assert_eq!(findings, vec![(false, 1), (true, 2)]);
    // The reply says what each step draws.
    let r = live.api_ok("annotation.update", json!({"id": "a2", "color": "green"}));
    assert_eq!(verdict(&r).0, Some(true), "{r}");
    live.api_ok("step.set", json!({"step": 1}));
    let r = live.api_ok("annotation.update", json!({"id": "a2", "color": "blue"}));
    assert_eq!(verdict(&r).0, Some(false), "{r}");
}
