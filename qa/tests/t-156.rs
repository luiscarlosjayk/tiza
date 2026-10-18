//! t-156 — `status` over the socket/MCP says a redact notice is pending.
//!
//! Acceptance criteria:
//! - AC1 `status` carries `redact_alerts` (the pending notices, ids and
//!   lines only, never masked text); empty after the presenter presses a
//!   key.
//! - AC2 MCP status/resources say it in words.
//! - AC3 schema regenerated, docs/agent-guide.md, qa test, CHANGELOG.
//!
//! Corner cases: several notices pending at once; the no-leak check of
//! t-117 AC9 on the new field. `redact_alerts` mirrors the rows the TUI
//! draws above its bar (t-135): the t-117 reload/paste notices, the t-140
//! notices of a step whose move widened or dropped a mask (a copy of the
//! value in a move's content, t-157, included) and the t-157 notice of a
//! move retargeted by another.

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_core::fs;
use tiza_qa::{Live, McpClient, scratch, tiza};

const SECRET: &str = "sk_live_51Hx9QeLkT2mNpR8vWzYa";

fn config() -> String {
    format!(
        "# payments service\nAPP_ENV=production\nPORT=8080\nSTRIPE_KEY={SECRET}\nLOG_LEVEL=info\n"
    )
}

/// Every 6-character piece of `value` with a digit or a capital in it.
fn pieces(value: &str) -> Vec<String> {
    let c: Vec<char> = value.chars().collect();
    (0..=c.len().saturating_sub(6))
        .map(|i| c[i..(i + 6).min(c.len())].iter().collect::<String>())
        .filter(|p| {
            p.chars()
                .any(|ch| ch.is_ascii_digit() || ch.is_ascii_uppercase())
        })
        .collect()
}

/// t-117 AC9 on any text an agent reads: no piece of any value.
#[track_caller]
fn assert_no_leak(values: &[&str], what: &str, s: &str) {
    for v in values {
        assert!(!s.contains(v), "{what} shows «{v}»:\n{s}");
        for p in pieces(v) {
            assert!(!s.contains(&p), "{what} shows «{p}» of «{v}»:\n{s}");
        }
    }
}

/// `config.env` open with follow on and `:redact 4:STRIPE_KEY=`.
fn followed(name: &str) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    let src = dir.join("config.env");
    std::fs::write(&src, config()).unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("config.env.tiza"), dir.join("live.sock"));
    live.command("redact 4:STRIPE_KEY=");
    assert!(!live.message().unwrap().1);
    live.command("follow on");
    (live, src)
}

/// Waits (10 s at most) until follow mode has applied `n` reloads.
fn wait_reloads(live: &Live, n: u64) {
    let start = std::time::Instant::now();
    while live.api_ok("follow.status", json!({}))["reloads"]
        .as_u64()
        .unwrap_or(0)
        < n
    {
        assert!(
            start.elapsed().as_secs() < 10,
            "follow did not reload #{n} in 10 s"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Atomic save, as editors do (tmp + rename).
fn save(src: &std::path::Path, text: &str) {
    let tmp = src.with_extension("env.tmp");
    std::fs::write(&tmp, text).unwrap();
    std::fs::rename(&tmp, src).unwrap();
}

/// `redact_alerts` of the socket `status`, as strings.
fn alerts(live: &Live) -> Vec<String> {
    let st = live.api_ok("status", json!({}));
    st["redact_alerts"]
        .as_array()
        .unwrap_or_else(|| panic!("status has no `redact_alerts` array: {st}"))
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

/// `tiza api status` with the real binary, parsed.
fn cli_status(live: &Live) -> Value {
    let out = tiza(&["--socket", live.socket.to_str().unwrap(), "api", "status"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert_no_leak(&[SECRET], "tiza api status", &text);
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

const WIDENED: &str = "redact a1 could not follow its text: masking the rest of L4 — fix it before recording (u undoes the reload)";

#[test]
fn ac1_test_steps_two_quick_saves_then_a_key() {
    let (live, src) = followed("t156-steps");
    assert_eq!(alerts(&live), Vec::<String>::new(), "nothing pending yet");
    // Rename the key and save: the redact fails closed, loudly.
    let renamed = config().replace("STRIPE_KEY=", "STRIPE_SECRET_KEY=");
    save(&src, &renamed);
    wait_reloads(&live, 1);
    // Save again (a harmless change): the bar moves on, the alert does not.
    save(&src, &renamed.replace("PORT=8080", "PORT=9090"));
    wait_reloads(&live, 2);
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("reloaded (#2)"), "{msg}");
    assert_eq!(alerts(&live), vec![WIDENED.to_string()]);
    // The same over the real CLI.
    let st = cli_status(&live);
    assert_eq!(st["redact_alerts"], json!([WIDENED]), "{st}");
    // The presenter presses a key: seen, gone, in the TUI and in `status`.
    live.keys(&[KeyCode::Char('j')]);
    assert!(live.with(|a| a.redact_alerts.is_empty()));
    assert_eq!(alerts(&live), Vec::<String>::new());
    let st = cli_status(&live);
    assert_eq!(st["redact_alerts"], json!([]), "always present: {st}");
}

#[test]
fn ac1_the_field_is_the_tuis_rows_and_an_overlay_key_does_not_clear_it() {
    let (live, src) = followed("t156-overlay");
    save(&src, &config().replace("STRIPE_KEY=", "STRIPE_SECRET_KEY="));
    wait_reloads(&live, 1);
    assert_eq!(alerts(&live), live.with(|a| a.redact_alerts.clone()));
    // `?` opens the help overlay over the alerts: the key that opens it
    // counts as seen (the alerts were on screen), as in the TUI (t-135).
    // A new alert under the overlay is not cleared by the key that closes it.
    live.keys(&[KeyCode::Char('?')]);
    let overlay_open = live.with(|a| a.overlay != tiza_tui::app::Overlay::None);
    assert!(overlay_open);
    assert!(alerts(&live).is_empty());
    save(
        &src,
        &config().replace(&format!("STRIPE_KEY={SECRET}\n"), ""),
    );
    wait_reloads(&live, 2);
    let pending = alerts(&live);
    assert_eq!(pending.len(), 1, "{pending:?}");
    live.keys(&[KeyCode::Esc]);
    assert_eq!(
        alerts(&live),
        pending,
        "closing an overlay is not seeing them"
    );
    live.keys(&[KeyCode::Char('j')]);
    assert!(alerts(&live).is_empty());
}

#[test]
fn corner_several_notices_pending_at_once_oldest_first() {
    let (live, src) = followed("t156-several");
    live.command("redact 2:APP_ENV=");
    let renamed = config().replace("STRIPE_KEY=", "STRIPE_SECRET_KEY=");
    save(&src, &renamed);
    wait_reloads(&live, 1);
    save(&src, &renamed.replace("APP_ENV=production\n", ""));
    wait_reloads(&live, 2);
    let (dropped, err) = live.message().unwrap();
    assert!(err && dropped.starts_with("redact a2 dropped"), "{dropped}");
    let got = alerts(&live);
    assert_eq!(got, vec![WIDENED.to_string(), dropped.clone()]);
    // The same notice twice is said once (the TUI's rule).
    live.with(|a| a.redact_alert(WIDENED.to_string()));
    assert_eq!(alerts(&live).len(), 2);
    for a in &got {
        assert_no_leak(&[SECRET, "production"], "redact_alerts", a);
    }
    live.keys(&[KeyCode::Char('k')]);
    assert!(alerts(&live).is_empty());
}

// ---------- moves (t-140, t-157) ----------

const FILE: &str = "# config\ntoken=abc123secret\nport=8080\n";
const VALUE: &str = "abc123secret";
const LONG: &str = "token=abc123secretLONGERvalue77";
const RETARGET: &str = "move #1 (step 2) now replaces L2 «# config» instead of «token=…»: check it";

fn normal(name: &str) -> Live {
    let dir = scratch(name);
    let src = dir.join("config.ini");
    std::fs::write(&src, FILE).unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("config.ini.tiza"), dir.join("live.sock"));
    live.command("redact 2@6..18");
    assert!(!live.message().unwrap().1);
    live
}

fn patch(step: u32, op: &str, position: &str, content: &str) -> Value {
    json!({"at_step": step, "action": "code.patch", "op": op, "position": position,
           "content": content})
}

#[test]
fn ac1_a_retargeted_move_is_listed_and_its_copy_masked_at_its_step() {
    let live = normal("t156-moves");
    live.api_ok("move.add", patch(2, "replace", "2", LONG));
    assert!(alerts(&live).is_empty());
    // An earlier insert retargets move #1 onto `# config` (t-157): the
    // presenter is told on the TUI's alert rows, and so is `status`.
    let r = live.api_ok("move.add", patch(1, "insert", "1", "# payments"));
    assert_eq!(r["notices"], json!([RETARGET]), "{r}");
    assert_eq!(alerts(&live), vec![RETARGET.to_string()]);
    assert_eq!(alerts(&live), live.with(|a| a.redact_alerts.clone()));
    // Reaching step 2 by key: the key clears the alert. The copy of the
    // value the move writes there is masked WIDENED (t-157) without a new
    // alert — the TUI does not draw one either; the retarget notice was
    // the warning — and nothing of it reaches `status`.
    live.keys(&[KeyCode::Char('n'), KeyCode::Char('n')]);
    live.with(|a| a.finish_typing());
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["step"], 2, "{st}");
    assert_eq!(st["redact_alerts"], json!([]), "{st}");
    assert_eq!(alerts(&live), live.with(|a| a.redact_alerts.clone()));
    assert_no_leak(&[VALUE, "LONGERvalue77"], "status", &st.to_string());
    let code = live.api_ok("code.get", json!({}))["text"].to_string();
    assert_no_leak(&[VALUE, "LONGERvalue77"], "code.get", &code);
}

#[test]
fn corner_no_leak_with_a_retarget_and_a_widened_copy_pending_together() {
    // t-117 AC9 on the new field: a retarget (t-157) and a redact a move
    // widened over a copy of its value (t-140/t-157), both pending, read
    // through the socket, the real CLI and the MCP.
    let live = normal("t156-noleak");
    live.api_ok("move.add", patch(2, "replace", "2", LONG));
    live.api_ok("move.add", patch(1, "insert", "1", "# payments"));
    // At step 3 the original token line (L3) goes: the redact is left on
    // the copy move #1 wrote, widened to the rest of its line.
    live.api_ok(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "delete", "position": "3"}),
    );
    // `goto` says the notices of every step on the way (t-099).
    live.api_ok("step.goto", json!({"id": 3}));
    live.with(|a| a.finish_typing());
    let got = alerts(&live);
    assert_eq!(
        got,
        vec![
            RETARGET.to_string(),
            "redact a1 could not follow its text through move #1: masking the rest of L2 — fix it before recording".to_string(),
        ]
    );
    let st = live.api_ok("status", json!({})).to_string();
    assert_no_leak(&[VALUE, LONG, "LONGERvalue77"], "status", &st);
    let cli = cli_status(&live);
    assert_eq!(cli["redact_alerts"], json!(got), "{cli}");
    assert_no_leak(
        &[VALUE, LONG, "LONGERvalue77"],
        "tiza api status",
        &cli.to_string(),
    );
    let mut mcp = McpClient::start(&live.socket);
    let (err, out, _) = mcp.call("status", json!({}));
    assert!(!err, "{out}");
    assert_no_leak(&[VALUE, LONG, "LONGERvalue77"], "MCP status", &out);
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["redact_alerts"], json!(got), "{out}");
    live.keys(&[KeyCode::Char('j')]);
    assert!(alerts(&live).is_empty());
}

// ---------- MCP ----------

#[test]
fn ac2_the_mcp_status_carries_it_and_says_it_in_words() {
    let (live, src) = followed("t156-mcp");
    let mut mcp = McpClient::start(&live.socket);
    let tools = mcp.tools();
    let desc = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "status")
        .unwrap()["description"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        desc.contains("redact_alerts")
            && desc.contains("never masked text")
            && desc.contains("presses a key"),
        "{desc}"
    );
    let (err, out, _) = mcp.call("status", json!({}));
    assert!(!err, "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["redact_alerts"], json!([]), "{out}");
    save(&src, &config().replace("STRIPE_KEY=", "STRIPE_SECRET_KEY="));
    wait_reloads(&live, 1);
    let (err, out, _) = mcp.call("status", json!({}));
    assert!(!err, "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["redact_alerts"], json!([WIDENED]), "{out}");
    assert_no_leak(&[SECRET], "MCP status", &out);
    live.keys(&[KeyCode::Char('j')]);
    let (_, out, _) = mcp.call("status", json!({}));
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["redact_alerts"], json!([]), "{out}");
}

// ---------- schema and docs ----------

#[test]
fn ac3_schema_guide_and_changelog() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let schema = std::fs::read_to_string(root.join("schema/api.schema.json")).unwrap();
    let v: Value = serde_json::from_str(&schema).unwrap();
    let live = serde_json::to_value(tiza_core::schema::api_schema()).unwrap();
    assert_eq!(v, live, "schema/api.schema.json is not regenerated");
    assert!(schema.contains("redact_alerts"));
    let guide = std::fs::read_to_string(root.join("docs/agent-guide.md")).unwrap();
    assert!(guide.contains("`redact_alerts`") && guide.contains("t-156"));
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(log.contains("`redact_alerts` (t-156)"));
}
