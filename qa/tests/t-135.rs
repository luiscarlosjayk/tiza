//! t-135 — The loud redact notices are never cut by a full status bar.
//!
//! Acceptance criteria:
//! - AC1 A redact notice is never cut: it gets its own rows above the bar
//!   (wrapped, never an ellipsis) until the next key, at any width ≥ the
//!   minimum supported terminal (checked down to 40 columns).
//! - AC2 A later reload or message does not silently replace a pending
//!   redact notice (it stays until a key is pressed, or both are shown).
//! - AC3 qa test at 80 and 130 columns with follow on and `⚠ small
//!   terminal`; CHANGELOG.
//!
//! The notices that exist today: `redact aN could not follow its text:
//! masking … — fix it before recording` (fail-closed, t-117 reload/paste and
//! t-140 moves) and `redact aN dropped: the line changed — check the screen
//! before recording` (t-117). The `kept by columns` notice of the card's
//! description was removed by t-117.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_core::fs;
use tiza_qa::{Live, scratch};
use tiza_tui::ui::alert_lines;

const SECRET: &str = "sk_live_51Hx9QeLkT2mNpR8vWzYa";

fn config() -> String {
    format!(
        "# payments service\nAPP_ENV=production\nPORT=8080\nSTRIPE_KEY={SECRET}\nLOG_LEVEL=info\n"
    )
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

/// The key renamed in the editor: the redact fails closed, loudly.
fn rename_key(live: &Live, src: &std::path::Path, reload: u64) -> String {
    save(src, &config().replace("STRIPE_KEY=", "STRIPE_SECRET_KEY="));
    wait_reloads(live, reload);
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.starts_with("redact a1 could not follow its text: masking the rest of L4"),
        "{msg}"
    );
    msg
}

/// The alerts drawn on screen, each one rejoined from its wrapped rows
/// (`⚠ ` first row, indented continuation rows).
fn alerts_on(screen: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut inside = false;
    for l in screen.lines() {
        if let Some(rest) = l.strip_prefix(" ⚠ redact") {
            out.push(format!("redact{rest}"));
            inside = true;
        } else if inside && l.starts_with("   ") && !l.trim().is_empty() {
            let last = out.last_mut().unwrap();
            last.push(' ');
            last.push_str(l.trim());
        } else {
            inside = false;
        }
    }
    out
}

fn bar(screen: &str) -> &str {
    screen.lines().last().unwrap()
}

#[test]
fn ac1_ac3_the_whole_notice_at_80_and_130_columns_with_follow_and_small_terminal() {
    let (live, src) = followed("t135-widths");
    let notice = rename_key(&live, &src, 1);
    for cols in [80u16, 130] {
        let s = live.screen(cols, 24);
        assert_eq!(alerts_on(&s), vec![notice.clone()], "{cols} cols:\n{s}");
        let b = bar(&s);
        assert!(
            b.contains("⟳ following") && b.contains("⚠ small terminal"),
            "{cols} cols, the bar keeps its segments:\n{s}"
        );
        // Not repeated (cut) on the bar, and no ellipsis anywhere near it.
        assert!(!b.contains("redact a1"), "{cols} cols:\n{s}");
        assert!(!s.contains('…'), "{cols} cols:\n{s}");
        assert!(!s.contains(SECRET) && !s.contains("51Hx9"), "{s}");
        // Its own rows: the code is not covered (5 lines, all visible).
        assert!(s.contains("LOG_LEVEL=info"), "{s}");
    }
    // 80 columns: two rows; 130: one.
    assert_eq!(alert_lines(std::slice::from_ref(&notice), 80).len(), 2);
    assert_eq!(alert_lines(std::slice::from_ref(&notice), 130).len(), 1);
}

#[test]
fn ac1_the_drop_notice_is_whole_too_and_any_key_hides_it() {
    let (live, src) = followed("t135-drop");
    save(
        &src,
        &config().replace(&format!("STRIPE_KEY={SECRET}\n"), ""),
    );
    wait_reloads(&live, 1);
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg
            == "redact a1 dropped: the line changed — check the screen before recording (u undoes the reload)",
        "{msg}"
    );
    let s = live.screen(80, 24);
    assert_eq!(alerts_on(&s), vec![msg.clone()], "{s}");
    // The next key: seen, gone; the bar is back to normal.
    live.keys(&[KeyCode::Char('j')]);
    let s = live.screen(80, 24);
    assert!(alerts_on(&s).is_empty(), "{s}");
    assert!(!s.contains("redact a1"), "{s}");
    assert!(live.with(|a| a.redact_alerts.is_empty()));
}

#[test]
fn ac2_a_later_reload_does_not_replace_the_notice() {
    let (live, src) = followed("t135-reloads");
    live.command("redact 2:APP_ENV=");
    let notice = rename_key(&live, &src, 1);
    // A second, harmless save: its message takes the bar, the notice stays.
    save(
        &src,
        &config()
            .replace("STRIPE_KEY=", "STRIPE_SECRET_KEY=")
            .replace("PORT=8080", "PORT=9090"),
    );
    wait_reloads(&live, 2);
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("reloaded (#2)"), "{msg}");
    let s = live.screen(130, 24);
    assert_eq!(alerts_on(&s), vec![notice.clone()], "{s}");
    assert!(bar(&s).contains("reloaded (#2)"), "{s}");
    // A third that drops another redact: both notices, whole.
    save(
        &src,
        &config()
            .replace("STRIPE_KEY=", "STRIPE_SECRET_KEY=")
            .replace("PORT=8080", "PORT=9090")
            .replace("APP_ENV=production\n", ""),
    );
    wait_reloads(&live, 3);
    let (dropped, _) = live.message().unwrap();
    assert!(dropped.starts_with("redact a2 dropped"), "{dropped}");
    for cols in [80u16, 130] {
        let s = live.screen(cols, 24);
        assert_eq!(
            alerts_on(&s),
            vec![notice.clone(), dropped.clone()],
            "{cols} cols:\n{s}"
        );
    }
    live.keys(&[KeyCode::Char('k')]);
    assert!(alerts_on(&live.screen(80, 24)).is_empty());
}

#[test]
fn corner_several_redacts_dropped_at_once() {
    let dir = scratch("t135-several");
    let src = dir.join("config.env");
    std::fs::write(&src, config()).unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("config.env.tiza"), dir.join("live.sock"));
    live.command("redact 2:APP_ENV=");
    live.command("redact 3:PORT=");
    live.command("redact 4:STRIPE_KEY=");
    live.command("follow on");
    save(&src, "# payments service\nLOG_LEVEL=info\n");
    wait_reloads(&live, 1);
    let (msg, _) = live.message().unwrap();
    assert!(msg.starts_with("redact a1, a2, a3 dropped:"), "{msg}");
    for cols in [80u16, 130, 40] {
        let s = live.screen(cols, 24);
        assert_eq!(alerts_on(&s), vec![msg.clone()], "{cols} cols:\n{s}");
    }
}

#[test]
fn corner_presentation_mode_shows_it_on_the_last_rows() {
    let (live, src) = followed("t135-present");
    live.keys(&[KeyCode::Char('P')]);
    let notice = rename_key(&live, &src, 1);
    let s = live.screen(80, 24);
    assert_eq!(alerts_on(&s), vec![notice.clone()], "{s}");
    // No bar in presentation mode: the notice ends on the last row.
    assert!(
        s.lines().last().unwrap().starts_with("   "),
        "the notice's last row is the last row:\n{s}"
    );
    // A `:` command line typed while it is pending: the notice sits above
    // it; the first key typed into it hides the notice (it was seen).
    live.with(|a| a.mode = tiza_tui::app::Mode::Command(String::new()));
    let s = live.screen(80, 24);
    assert_eq!(alerts_on(&s), vec![notice.clone()], "{s}");
    assert!(s.lines().last().unwrap().starts_with(':'), "{s}");
    live.keys(&[KeyCode::Esc]);
    assert!(alerts_on(&live.screen(80, 24)).is_empty());
}

#[test]
fn corner_an_overlay_does_not_eat_the_notice() {
    let (live, src) = followed("t135-overlay");
    live.keys(&[KeyCode::Char('?')]);
    let notice = rename_key(&live, &src, 1);
    // Esc only closes the help that was covering it.
    live.keys(&[KeyCode::Esc]);
    let s = live.screen(80, 24);
    assert_eq!(alerts_on(&s), vec![notice], "{s}");
    live.keys(&[KeyCode::Char('j')]);
    assert!(alerts_on(&live.screen(80, 24)).is_empty());
}

#[test]
fn ac1_minimum_width_and_wrapping() {
    let (live, src) = followed("t135-narrow");
    let notice = rename_key(&live, &src, 1);
    let s = live.screen(40, 12);
    assert_eq!(alerts_on(&s), vec![notice], "{s}");
    // Rows never wider than the terminal minus one cell at each side; a
    // word longer than a row is split, not dropped.
    let long = format!("redact a1 dropped: {} end", "x".repeat(50));
    let rows = alert_lines(std::slice::from_ref(&long), 30);
    assert!(rows.iter().all(|r| unicode_width(r) <= 28), "{rows:?}");
    let joined: String = rows
        .iter()
        .map(|r| r.trim_start_matches('⚠').trim())
        .collect();
    assert_eq!(joined.replace(' ', ""), long.replace(' ', ""));
    // Too narrow to say anything useful: nothing, never a panic.
    assert!(alert_lines(std::slice::from_ref(&long), 4).is_empty());
}

fn unicode_width(s: &str) -> usize {
    s.chars().count()
}
