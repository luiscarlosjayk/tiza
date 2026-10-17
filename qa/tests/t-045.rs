//! t-045 — Supervised mode: optional confirmation in the TUI before an
//! agent's change is applied.
//!
//! Acceptance criteria:
//! - AC1 `:supervise on` (or socket `supervise.set {enabled:true}`): every
//!   mutating request (annotation.*, scene.*, style.set, step.*, focus.set,
//!   theme.set, follow.set) is parked as a proposal; the socket reply waits.
//!   Read-only methods (status, code.get, scene.get, render.text…) and
//!   `supervise.*` answer immediately. `status` reports `supervised` and
//!   `pending`.
//! - AC2 The review overlay opens with the pending list (`▶ p1  add note L2
//!   «…»`), the status bar shows `⏸ N to review (y/n)`; `y`/Enter applies the
//!   first proposal and the agent receives the normal result; `n`/`x` rejects
//!   it and the agent receives error `rejected`; `a` accepts all, `d` rejects
//!   all; Esc hides the overlay, `:review` reopens it.
//! - AC3 A proposal nobody decides expires after 60 s with `rejected`; turning
//!   supervision off rejects what is pending (nothing sneaks in).
//! - AC4 In presentation mode the overlay does not pop up (the chip still
//!   counts); the MCP bridge surfaces the rejection text to the agent.

use std::thread;
use std::time::Duration;

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, McpClient};
use tiza_tui::app::Overlay;

/// Sends a request from another thread and returns a handle to its reply.
fn send_async(
    socket: &std::path::Path,
    method: &str,
    params: Value,
) -> thread::JoinHandle<tiza_core::protocol::Response> {
    let socket = socket.to_path_buf();
    let method = method.to_string();
    thread::spawn(move || tiza_tui::ipc::call(&socket, &method, params).expect("socket"))
}

fn wait_pending(live: &Live, n: usize) {
    let start = std::time::Instant::now();
    while live.with(|a| a.proposals.len()) < n {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "waiting for {n} proposals"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn ac1_mutations_wait_and_reads_do_not() {
    let live = Live::from_code("fn a() {}\nfn b() {}\n", "rs", "t045-park");
    live.command("supervise on");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("supervised mode on"), "{msg}");
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["supervised"], json!(true));
    assert!(st.get("pending").is_none(), "zero is omitted");

    let h = send_async(
        &live.socket,
        "annotation.add",
        json!({"type":"note","line":2,"text":"hi"}),
    );
    wait_pending(&live, 1);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0, "not applied yet");
    assert_eq!(live.api_ok("status", json!({}))["pending"], json!(1));
    // Reads are answered while the proposal waits.
    let code = live.api_ok("code.get", json!({}));
    assert!(code["text"].as_str().unwrap().contains("fn b()"));
    let sup = live.api_ok("supervise.status", json!({}));
    assert_eq!(sup["pending"][0]["summary"], json!("add note L2 «hi»"));
    assert_eq!(sup["pending"][0]["method"], json!("annotation.add"));
    assert!(!h.is_finished(), "the agent is still waiting");

    // Accept: the agent gets the normal result and the note exists.
    live.keys(&[KeyCode::Char('y')]);
    let resp = h.join().unwrap();
    assert!(resp.error.is_none(), "{:?}", resp.error);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    assert_eq!(
        live.api_ok("supervise.status", json!({}))["accepted"],
        json!(1)
    );
    let (msg, _) = live.message().unwrap();
    assert!(msg.starts_with("accepted p1: add note L2"), "{msg}");

    // Every mutating method is parked, including view changes.
    for (m, p) in [
        ("step.next", json!({})),
        ("theme.set", json!({"theme": "light"})),
        ("scene.clear", json!({})),
        (
            "style.set",
            json!({"name": "hot", "glyph": "🔥", "legend": "hot"}),
        ),
    ] {
        let h = send_async(&live.socket, m, p);
        wait_pending(&live, 1);
        assert!(!h.is_finished(), "{m} must wait");
        live.keys(&[KeyCode::Char('n')]);
        let r = h.join().unwrap();
        assert_eq!(
            r.error.as_ref().map(|e| e.code.as_str()),
            Some("rejected"),
            "{m}"
        );
    }
    assert_eq!(
        live.with(|a| a.doc.annotations.len()),
        1,
        "clear was rejected"
    );
    assert_eq!(live.with(|a| a.theme.name), tiza_core::ThemeName::Dark);
}

#[test]
fn ac2_overlay_bar_and_keys() {
    let live = Live::from_code("fn a() {}\nfn b() {}\nfn c() {}\n", "rs", "t045-keys");
    live.api_ok("supervise.set", json!({"enabled": true}));
    let h1 = send_async(
        &live.socket,
        "annotation.add",
        json!({"type":"box","from_line":1,"to_line":2,"label":"first"}),
    );
    wait_pending(&live, 1);
    let h2 = send_async(
        &live.socket,
        "annotation.add",
        json!({"type":"note","line":3,"text":"second"}),
    );
    wait_pending(&live, 2);
    let h3 = send_async(&live.socket, "annotation.delete", json!({"id":"nope"}));
    wait_pending(&live, 3);

    assert_eq!(live.with(|a| a.overlay), Overlay::Review, "opens by itself");
    let screen = live.screen(120, 30);
    assert!(screen.contains("Agent proposals"), "{screen}");
    assert!(screen.contains("▶ p1  add box L1-2 «first»"), "{screen}");
    assert!(screen.contains("p2  add note L3 «second»"), "{screen}");
    assert!(screen.contains("p3  delete nope"), "{screen}");
    assert!(screen.contains("y accept"), "{screen}");
    // Esc hides, the chip stays, :review reopens.
    live.keys(&[KeyCode::Esc]);
    assert_eq!(live.with(|a| a.overlay), Overlay::None);
    let bar = live.screen(120, 30).lines().last().unwrap().to_string();
    assert!(bar.contains("⏸ 3 to review (y/n)"), "{bar}");
    live.command("review");
    assert_eq!(live.with(|a| a.overlay), Overlay::Review);

    // x rejects the first; y accepts the next; the third fails on apply
    // (unknown id) and the agent gets that error, not `rejected`.
    live.keys(&[KeyCode::Char('x')]);
    assert_eq!(h1.join().unwrap().error.unwrap().code, "rejected");
    live.keys(&[KeyCode::Enter]);
    assert!(h2.join().unwrap().error.is_none());
    live.keys(&[KeyCode::Char('y')]);
    let r3 = h3.join().unwrap();
    assert_eq!(r3.error.unwrap().code, "not_found");
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("p3 accepted but failed"), "{msg}");
    assert_eq!(live.with(|a| a.overlay), Overlay::None, "closes when empty");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);

    // a / d on batches.
    let hs: Vec<_> = (0..3)
        .map(|i| {
            send_async(
                &live.socket,
                "annotation.add",
                json!({"type":"note","line":1,"text":format!("n{i}")}),
            )
        })
        .collect();
    wait_pending(&live, 3);
    live.keys(&[KeyCode::Char('a')]);
    for h in hs {
        assert!(h.join().unwrap().error.is_none());
    }
    assert_eq!(live.with(|a| a.doc.annotations.len()), 4);
    let hs: Vec<_> = (0..2)
        .map(|_| send_async(&live.socket, "step.next", json!({})))
        .collect();
    wait_pending(&live, 2);
    live.keys(&[KeyCode::Char('d')]);
    for h in hs {
        assert_eq!(h.join().unwrap().error.unwrap().code, "rejected");
    }
    let sup = live.api_ok("supervise.status", json!({}));
    assert_eq!(sup["accepted"], json!(5));
    assert_eq!(sup["rejected"], json!(3));
    // The bar without pending proposals still says supervised.
    let bar = live.screen(120, 30).lines().last().unwrap().to_string();
    assert!(bar.contains("⏸ supervised"), "{bar}");
}

#[test]
fn ac3_turning_off_rejects_pending_and_timeout_is_published() {
    let live = Live::from_code("fn a() {}\n", "rs", "t045-off");
    live.command("supervise on");
    let h = send_async(
        &live.socket,
        "annotation.add",
        json!({"type":"note","line":1,"text":"late"}),
    );
    wait_pending(&live, 1);
    live.command("supervise off");
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("1 pending proposal rejected"), "{msg}");
    let r = h.join().unwrap();
    assert_eq!(r.error.unwrap().code, "rejected");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
    // Off: the next add applies at once.
    let r = live.api_ok(
        "annotation.add",
        json!({"type":"note","line":1,"text":"now"}),
    );
    assert!(r["id"].is_string());
    let sup = live.api_ok("supervise.status", json!({}));
    assert_eq!(sup["supervised"], json!(false));
    assert_eq!(sup["timeout_secs"], json!(60));
    assert_eq!(tiza_tui::app::PROPOSAL_TIMEOUT, Duration::from_secs(60));
    // The expiry path, without waiting a minute: age the proposal by hand.
    live.command("supervise on");
    let h = send_async(
        &live.socket,
        "annotation.add",
        json!({"type":"note","line":1,"text":"old"}),
    );
    wait_pending(&live, 1);
    live.with(|a| a.proposals[0].since -= Duration::from_secs(61));
    let r = h.join().unwrap();
    let e = r.error.unwrap();
    assert_eq!(e.code, "rejected");
    assert!(e.message.contains("expired after 60s"), "{}", e.message);
    assert_eq!(live.with(|a| a.proposals.len()), 0);
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("1 proposal expired"), "{msg}");
    // :supervise with a bad argument.
    live.command("supervise maybe");
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("usage: :supervise [on|off]"), "{msg}");
}

#[test]
fn ac4_presentation_mode_and_mcp_bridge() {
    let live = Live::from_code("fn a() {}\n", "rs", "t045-mcp");
    live.command("supervise on");
    live.with(|a| a.present = true);
    let mut mcp = McpClient::start(&live.socket);
    let socket = live.socket.clone();
    let agent = thread::spawn(move || {
        let mut mcp = McpClient::start(&socket);
        mcp.call("add_note", json!({"line": 1, "text": "from claude"}))
    });
    wait_pending(&live, 1);
    assert_eq!(
        live.with(|a| a.overlay),
        Overlay::None,
        "no popup while presenting"
    );
    assert_eq!(live.with(|a| a.proposals.len()), 1);
    // The presenter still sees it in `status` through the bridge.
    let (err, text, _) = mcp.call("status", json!({}));
    assert!(!err);
    assert!(
        text.contains("\"supervised\": true") && text.contains("\"pending\": 1"),
        "{text}"
    );
    live.command("review");
    assert_eq!(live.with(|a| a.overlay), Overlay::Review);
    live.keys(&[KeyCode::Char('n')]);
    let (err, text, _) = agent.join().unwrap();
    assert!(err, "{text}");
    assert!(text.contains("presenter rejected"), "{text}");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
}
