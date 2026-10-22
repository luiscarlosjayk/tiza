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
//! not drawn. The width is the screen's code area (the terminal less its
//! side padding), the one `check` must be given to agree.
//!
//! Corner cases: the reply is about the size at that moment (a resize
//! later changes nothing in it); presentation mode follows the same rule;
//! with no terminal size known the default width (100) is used and named.

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, bin, scratch};

const ENV: &str = "DATABASE_URL=postgres://app:s3cr3t-pass@db.internal:5432/app\n\
                   API_TOKEN=tok_live_9f8e7d6c5b4a3f2e1d0c\n\
                   LOG_LEVEL=info\n\
                   PORT=8080\n";

/// At 100 and 110 columns the callout's block on L1 takes the margin rows
/// of L2 and L3, so a label on L2 finds no row; from 116 columns the
/// callout falls back to a label and every label is drawn.
fn opened(name: &str) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    std::fs::write(dir.join("demo.env"), ENV).unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"path": "demo.env", "language": "env", "first_line": 1},
        "code": {"file": "demo.env"},
        "annotations": [{
            "id": "c1", "step": 1, "type": "callout", "from_line": 1, "to_line": 1,
            "text": "user, password, host, port and database in one value"
        }],
        "step": 1
    });
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
    let out = std::process::Command::new(bin())
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
            )
        })
        .collect()
}

#[test]
fn ac1_ac4_a_note_with_no_room_is_not_visible() {
    // Test step 1.
    let (live, tiza) = opened("t171-no-room");
    terminal(&live, 102);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    assert_eq!(r["visible"], false, "{r}");
    assert_eq!(
        r["hidden"],
        "it is not drawn: no room for its text at 100 columns (check at width 100 lists it)"
    );
    assert!(r.get("clipped").is_none(), "{r}");
    // AC4: `tiza check` at that width says the same of it.
    let id = r["id"].as_str().unwrap().to_string();
    assert!(
        check_cuts(&tiza, 100).contains(&(id.clone(), true)),
        "{:?}",
        check_cuts(&tiza, 100)
    );
    // And the screen does not show it.
    assert!(!live.screen(102, 30).contains("secret token"));
}

#[test]
fn ac1_widened_until_drawn_the_update_says_visible() {
    // Test step 2.
    let (live, tiza) = opened("t171-widen");
    terminal(&live, 102);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    assert_eq!(r["visible"], false, "{r}");
    let id = r["id"].as_str().unwrap().to_string();
    terminal(&live, 122);
    let r = live.api_ok("annotation.update", json!({"id": id, "color": "green"}));
    assert_eq!(r["visible"], true, "{r}");
    assert!(
        r.get("hidden").is_none() && r.get("clipped").is_none(),
        "{r}"
    );
    assert!(
        check_cuts(&tiza, 120).is_empty(),
        "{:?}",
        check_cuts(&tiza, 120)
    );
    let s = live.screen(122, 30);
    assert!(s.contains("secret token"), "{s}");
}

#[test]
fn ac2_a_clipped_label_stays_visible_and_says_so() {
    // Test step 3: a long note on L3 beside the callout's block, cut with
    // … at 120 columns.
    let (live, tiza) = opened("t171-clipped");
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
fn ac2_a_highlight_whose_label_has_no_room_is_still_drawn() {
    // The highlight is drawn; only its label is not.
    let (live, tiza) = opened("t171-highlight");
    terminal(&live, 102);
    let r = add(
        &live,
        json!({"type": "highlight", "at": {"line": 2, "find": "tok_live"}, "label": "secret token"}),
    );
    assert_eq!(r["visible"], true, "{r}");
    assert_eq!(
        r["clipped"],
        "its label is not drawn: no room at 100 columns (check at width 100 lists it)"
    );
    let id = r["id"].as_str().unwrap().to_string();
    assert!(check_cuts(&tiza, 100).contains(&(id, true)));
}

#[test]
fn ac1_the_mcp_reply_says_it() {
    let (live, _tiza) = opened("t171-mcp");
    terminal(&live, 102);
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("add_note", json!({"line": 2, "text": "secret token"}));
    assert!(!err, "{text}");
    assert!(
        text.contains("not visible now")
            && text.contains("it is not drawn: no room for its text at 100 columns"),
        "{text}"
    );
}

#[test]
fn ac3_the_bar_says_it() {
    let (live, _tiza) = opened("t171-bar");
    terminal(&live, 102);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    let id = r["id"].as_str().unwrap().to_string();
    assert_eq!(
        live.message(),
        Some((
            format!("{id}: note L2 added by agent (not drawn: no room at 100 columns)"),
            false
        ))
    );
}

#[test]
fn ac1_a_note_that_fits_is_visible_as_before() {
    let (live, _tiza) = opened("t171-fits");
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
fn corner_presentation_mode_follows_the_same_rule() {
    let (live, _tiza) = opened("t171-present");
    live.with(|app| {
        app.size = (102, 30);
        app.set_present(true);
    });
    let width = live.with(|app| app.screen_width());
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    assert_eq!(r["visible"], false, "{r}");
    assert!(
        r["hidden"]
            .as_str()
            .unwrap()
            .contains(&format!("at {width} columns")),
        "{r}"
    );
}

#[test]
fn corner_no_terminal_size_uses_the_default_width_and_names_it() {
    let (live, _tiza) = opened("t171-headless");
    live.with(|app| app.size = (0, 0));
    assert_eq!(live.with(|app| app.screen_width()), 100);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    assert_eq!(r["visible"], false, "{r}");
    assert!(
        r["hidden"].as_str().unwrap().contains("at 100 columns"),
        "{r}"
    );
}

#[test]
fn corner_the_reply_is_about_the_size_at_that_moment() {
    // A resize after the reply changes the screen, not the reply: the next
    // reply is about the new size.
    let (live, _tiza) = opened("t171-resize");
    terminal(&live, 102);
    let r = add(
        &live,
        json!({"type": "note", "line": 2, "text": "secret token"}),
    );
    assert_eq!(r["visible"], false, "{r}");
    terminal(&live, 122);
    assert!(live.screen(122, 30).contains("secret token"));
    let id = r["id"].as_str().unwrap();
    let r = live.api_ok("annotation.update", json!({"id": id, "color": "green"}));
    assert_eq!(r["visible"], true, "{r}");
}
