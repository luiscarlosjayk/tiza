//! t-210 — The live `check` freezes the TUI while it runs (16 s on a
//! 3,000-line tutorial since t-200's width search).
//!
//! The live `check` (socket `check`, MCP `check`) ran on the TUI thread, so
//! the screen, the keys and every other socket request waited until it
//! ended. `tiza check` on the command line is not affected (no TUI).
//!
//! Acceptance criteria:
//! - AC1 While a live `check` runs, the TUI keeps drawing and taking keys,
//!   and other socket requests are answered (`status`, `render_text`,
//!   annotation edits); a step change or a key during the check is not
//!   delayed by it.
//! - AC2 The check runs on a snapshot of the document taken when it was
//!   asked (no lock held on the live document while it runs); if the
//!   document changes before it ends, the reply says which revision it
//!   checked (or the check is redone on the latest), never a mix.
//! - AC3 Concurrent `check` requests do not stack: a new one while one runs
//!   either waits for it and shares its result, or cancels and replaces
//!   it; at most one runs at a time.
//! - AC4 Same result as today, field for field, for a document that does
//!   not change during the check (the CLI `tiza check` and the live one
//!   agree).
//! - AC5 Quitting (`:q`, Ctrl-C, terminal gone per t-205) while a check
//!   runs exits at once; the check thread does not keep the process alive.
//! - AC6 This file; CHANGELOG (Fixed); agent guide line for the reply's
//!   new `revision` field.
//!
//! Corner cases: MCP `check` with a client timeout shorter than the check
//! (the server still answers when done); `--smoke` and `render` unchanged;
//! a panic in the check thread answers `internal` and leaves the TUI
//! running (t-166).
//!
//! The document is generated (300 lines, 16 annotations over 4 steps, as
//! the card measured), big enough for a check that takes seconds in a
//! debug build and most of a second in a release one. Nothing waits on a
//! fixed sleep: a request is answered within a deadline or the test
//! fails, and "while the check runs" means before its reply arrived.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_core::protocol::Response;
use tiza_qa::{Live, McpClient, scratch, tiza};

/// How long a request answered while a check runs may take.
const QUICK: Duration = Duration::from_secs(2);
/// How long a check may take at all.
const CHECK: Duration = Duration::from_secs(120);

/// `lines` lines of Rust with 16 labelled annotations, four per step
/// (boxes, notes, highlights and arrows on function headers).
fn big_doc(lines: usize) -> Value {
    let code: Vec<String> = (0..lines)
        .map(|i| match i % 6 {
            0 => format!("fn step_{i}(queue: &mut Vec<u64>, limit: usize) -> Option<u64> {{"),
            1 => format!(
                "    let value = queue.iter().copied().filter(|v| v % {} == 0).sum::<u64>();",
                i + 3
            ),
            2 => "    if queue.len() > limit { queue.truncate(limit); }".into(),
            3 => format!("    queue.push(value + {i});"),
            4 => "    queue.last().copied()".into(),
            _ => "}".into(),
        })
        .collect();
    let annotations: Vec<Value> = (0..16)
        .map(|j| {
            let step = j / 4 + 1;
            let line = 1 + 6 * ((j * (lines - 10)) / 16 / 6);
            match j % 4 {
                0 => json!({"id": format!("b{j}"), "step": step, "type": "box",
                    "from_line": line, "to_line": line + 4,
                    "label": format!("function number {j}: filters, truncates and pushes")}),
                1 => json!({"id": format!("n{j}"), "step": step, "type": "note", "line": line,
                    "text": format!("note {j}: the sum only keeps multiples of the line number")}),
                2 => json!({"id": format!("h{j}"), "step": step, "type": "highlight",
                    "span": {"line": line, "start": 4, "end": 20},
                    "label": format!("highlight {j} on the signature")}),
                _ => json!({"id": format!("a{j}"), "step": step, "type": "arrow",
                    "from": {"line": line, "col": 4}, "to": {"line": line + 3, "col": 4},
                    "label": format!("arrow {j}: value flows into the push")}),
            }
        })
        .collect();
    json!({
        "schema": 1,
        "title": format!("Big tutorial ({lines} lines, t-210)"),
        "source": {"language": "rust", "first_line": 1},
        "code": code,
        "annotations": annotations,
    })
}

/// The big document written as `big.tiza` in a scratch directory.
fn big_file(name: &str) -> PathBuf {
    let dir = scratch(name);
    let file = dir.join("big.tiza");
    std::fs::write(&file, serde_json::to_string_pretty(&big_doc(300)).unwrap()).unwrap();
    file
}

/// A live instance holding the big document (saved next to it).
fn big_live(name: &str) -> (PathBuf, Live) {
    let file = big_file(name);
    let doc = tiza_core::fs::open(file.to_str().unwrap())
        .unwrap()
        .document;
    let dir = file.parent().unwrap();
    let live = Live::open(doc, dir.join("held.tiza"), dir.join("live.sock"));
    (file, live)
}

/// A socket call on a thread of its own: its reply and when it came.
fn call_in_background(
    socket: &Path,
    method: &str,
    params: Value,
) -> mpsc::Receiver<(Instant, Response)> {
    let (tx, rx) = mpsc::channel();
    let socket = socket.to_path_buf();
    let method = method.to_string();
    std::thread::spawn(move || {
        let r = tiza_tui::ipc::call(&socket, &method, params).expect("socket call");
        let _ = tx.send((Instant::now(), r));
    });
    rx
}

/// A socket call that must be answered within `QUICK`: its result.
fn quick(socket: &Path, method: &str, params: Value) -> Value {
    let rx = call_in_background(socket, method, params);
    let (_, r) = rx
        .recv_timeout(QUICK)
        .unwrap_or_else(|_| panic!("{method} was not answered within {QUICK:?}"));
    assert!(r.error.is_none(), "{method}: {:?}", r.error);
    r.result.unwrap_or(Value::Null)
}

/// A `check` handed to the instance as its event loop hands a socket
/// request (`App::handle_ipc`), on the TUI thread: once this returns, the
/// check was asked, so what the test does next happens after it (a socket
/// client of its own could arrive after a later request). Its reply and
/// when it came.
fn ask_check(live: &Live, params: Value) -> mpsc::Receiver<(Instant, Response)> {
    let (tx, rx) = mpsc::channel();
    live.with(move |app| {
        let req = tiza_core::protocol::Request {
            id: json!(1),
            method: "check".into(),
            params,
        };
        app.handle_ipc(req, tx)
    });
    let (stamped_tx, stamped) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok(r) = rx.recv() {
            let _ = stamped_tx.send((Instant::now(), r));
        }
    });
    stamped
}

/// The reply of a check started with [`ask_check`].
fn check_reply(rx: &mpsc::Receiver<(Instant, Response)>) -> (Instant, Value) {
    let (at, r) = rx.recv_timeout(CHECK).expect("the check never answered");
    assert!(r.error.is_none(), "check: {:?}", r.error);
    (at, r.result.unwrap())
}

/// Step 1 / AC1: a check in flight; meanwhile `status`, `render.text` and
/// an annotation edit over the socket and `n` on the keyboard are answered
/// within `QUICK`, again and again, before the check's reply arrives.
#[test]
fn ac1_status_a_key_and_edits_are_answered_while_a_check_runs() {
    let (_, live) = big_live("t210-ac1");
    let sock = live.socket.clone();
    let step = quick(&sock, "status", json!({}))["step"].as_u64().unwrap();
    let check = ask_check(&live, json!({}));
    let mut answered = 0;
    let mut key_pressed = false;
    loop {
        let s = quick(&sock, "status", json!({}));
        if check.try_recv().is_ok() {
            break;
        }
        answered += 1;
        if answered == 2 {
            // `n`: the key goes through the TUI thread; it must not wait
            // for the check either.
            let (done_tx, done_rx) = mpsc::channel();
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    live.keys(&[KeyCode::Char('n')]);
                    let _ = done_tx.send(());
                });
                assert!(
                    done_rx.recv_timeout(QUICK).is_ok(),
                    "the key `n` was not handled within {QUICK:?}"
                );
            });
            let s = quick(&sock, "status", json!({}));
            assert_eq!(s["step"], step + 1, "{s}");
            assert!(
                matches!(check.try_recv(), Err(mpsc::TryRecvError::Empty)),
                "the check answered before the key was handled: it did not run long enough to tell"
            );
            key_pressed = true;
        }
        if answered == 3 {
            let text = quick(&sock, "render.text", json!({"width": 100}));
            assert!(text["text"].as_str().unwrap().contains("step_0"), "{text}");
            let r = quick(
                &sock,
                "annotation.add",
                json!({"id": "x1", "step": 1, "type": "note", "line": 2, "text": "added meanwhile"}),
            );
            assert_eq!(r["id"], "x1", "{r}");
        }
        assert!(s["revision"].is_u64(), "{s}");
        if answered > 10_000 {
            panic!("the check never answered");
        }
    }
    assert!(
        key_pressed,
        "the check answered after {answered} status calls"
    );
    assert!(
        answered >= 3,
        "only {answered} status calls were answered while the check ran"
    );
}

/// Step 2 / AC2: an annotation added while the check runs is not in its
/// report, and the reply names the revision it checked: the one before
/// the change. A check asked after the change sees it, with its revision.
#[test]
fn ac2_a_change_during_the_check_is_not_in_it_and_the_reply_names_its_revision() {
    let (_, live) = big_live("t210-ac2");
    let sock = live.socket.clone();
    let before = quick(&sock, "status", json!({}))["revision"]
        .as_u64()
        .unwrap();
    let check = ask_check(&live, json!({}));
    // A note at step 5: a report that saw it would say `max_step: 5`.
    let added = quick(
        &sock,
        "annotation.add",
        json!({"id": "late", "step": 5, "type": "note", "line": 2, "text": "added during the check"}),
    );
    let after = added["revision"].as_u64().unwrap();
    assert!(after > before, "{added}");
    assert!(
        matches!(check.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "the check answered before the edit: it did not run long enough to tell"
    );
    let (_, v) = check_reply(&check);
    let checked = v["revision"].as_u64().expect("revision");
    assert_eq!(
        checked, before,
        "asked at revision {before}, changed at {after}: {v}"
    );
    assert_eq!(v["max_step"], 4, "the report saw the later note: {v}");
    // Asked now: the latest document, with the note and its revision.
    let v = live.api_ok("check", json!({}));
    assert_eq!(v["max_step"], 5, "{v}");
    assert!(v["revision"].as_u64().unwrap() >= after, "{v}");
}

/// Step 3 / AC3: two checks at once share one computation: two replies,
/// the same report and the same revision, the second one no later than
/// the first one's computation. A third at another width waits for them
/// and is answered with its own width.
#[test]
fn ac3_two_checks_at_once_share_one_computation() {
    let (_, live) = big_live("t210-ac3");
    let sock = live.socket.clone();
    let t0 = Instant::now();
    let first = ask_check(&live, json!({}));
    quick(&sock, "status", json!({}));
    let second = ask_check(&live, json!({}));
    let narrow = ask_check(&live, json!({"width": 60}));
    quick(&sock, "status", json!({}));
    let (t1, a) = check_reply(&first);
    let (t2, b) = check_reply(&second);
    assert_eq!(a, b, "two checks at once, two different reports");
    // Shared: the second came with the first, not one more check later
    // (a second computation would take as long as the first).
    let took = t1 - t0;
    let gap = if t2 > t1 { t2 - t1 } else { t1 - t2 };
    assert!(
        gap < took / 2,
        "the second reply came {gap:?} after the first (one check takes {took:?})"
    );
    // The one at width 60 ran after them, not beside them: it ends about
    // one more check later.
    let (t3, c) = check_reply(&narrow);
    assert_eq!(c["width"], 60, "{c}");
    assert_eq!(a["width"], 100, "{a}");
    assert!(
        t3 > t1 + took / 4,
        "the check at width 60 ended {:?} after the first one (one check takes {took:?}): it ran beside it",
        t3.saturating_duration_since(t1)
    );
    assert_eq!(c["revision"], a["revision"], "{c}\n{a}");
}

/// AC3, replace: while a check runs, one at another width waits; one more
/// at a third width takes its place, and the replaced request is told at
/// once (`busy`, naming the width that replaced it) instead of waiting
/// for a computation. The running check and the newest one answer with
/// their reports.
#[test]
fn ac3_another_width_while_one_waits_replaces_it_and_it_is_told_at_once() {
    let (_, live) = big_live("t210-ac3-replace");
    let running = ask_check(&live, json!({}));
    let replaced = ask_check(&live, json!({"width": 60}));
    let newest = ask_check(&live, json!({"width": 70}));
    let (_, r) = replaced
        .recv_timeout(QUICK)
        .unwrap_or_else(|_| panic!("the replaced check was not answered within {QUICK:?}"));
    let err = r.error.expect("the replaced check got a report");
    assert_eq!(err.code, "busy", "{err:?}");
    assert!(
        err.message.contains("width 70") && err.message.contains("ask again"),
        "{err:?}"
    );
    assert!(
        matches!(running.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "the running check answered before the replaced one was told: it did not run long enough to tell"
    );
    let (_, a) = check_reply(&running);
    assert_eq!(a["width"], 100, "{a}");
    let (_, c) = check_reply(&newest);
    assert_eq!(c["width"], 70, "{c}");
    assert_eq!(c["revision"], a["revision"], "{c}\n{a}");
}

/// AC3, the effective width: `{}` and `{"width": 100}` (the default) are
/// the same check, so the second joins the first instead of waiting for it
/// and running again.
#[test]
fn ac3_the_default_width_and_the_same_width_given_share_one_computation() {
    let (_, live) = big_live("t210-ac3-default");
    let t0 = Instant::now();
    let implicit = ask_check(&live, json!({}));
    let explicit = ask_check(&live, json!({"width": tiza_core::check::DEFAULT_WIDTH}));
    let (t1, a) = check_reply(&implicit);
    let (t2, b) = check_reply(&explicit);
    assert_eq!(a, b, "{a}\n{b}");
    let took = t1 - t0;
    let gap = if t2 > t1 { t2 - t1 } else { t1 - t2 };
    assert!(
        gap < took / 2,
        "`{{\"width\": 100}}` answered {gap:?} after `{{}}` (one check takes {took:?}): it ran again"
    );
}

/// AC3, a burst: while a check runs, many checks at other widths at once
/// leave at most one waiting. Counted with the test hook (one line in
/// `panics.log` per computation): all of them but one are told `busy`
/// while the first still runs, and only two computations happen in all —
/// the first and the one left waiting — so no copy of the document piles
/// up behind them.
#[test]
fn ac3_a_burst_of_widths_leaves_one_check_waiting() {
    let file = big_file("t210-ac3-burst");
    let dir = file.parent().unwrap().to_path_buf();
    let runtime = dir.join("run");
    std::fs::create_dir_all(&runtime).unwrap();
    let body = r##"
import json
binary, doc, runtime = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(os.path.dirname(doc))
sock = "b.sock"
env = dict(os.environ, TIZA_TEST_HOOK="request:check", XDG_RUNTIME_DIR=runtime)
def check(params):
    return subprocess.Popen([binary, "api", "check", json.dumps(params), "--socket", sock], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
s = Pty([binary, "--socket", sock, "--no-save", doc], 30, 120, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 20, "first frame"):
    first = check({})
    r = subprocess.run([binary, "api", "status", "--socket", sock], capture_output=True, text=True, timeout=5)
    print("STATUS", r.returncode, flush=True)
    burst = [check({"width": w}) for w in range(30, 50)]
    # Every one but one is told it was replaced; the last waits.
    end = time.monotonic() + 30
    while sum(p.poll() is not None for p in burst) < len(burst) - 1 and time.monotonic() < end:
        s.settle(0.05)
    told = [p for p in burst if p.poll() is not None]
    print("TOLD", len(told), flush=True)
    print("FIRST_IN_FLIGHT", first.poll() is None, flush=True)
    print("BUSY", sum('busy' in p.communicate(timeout=5)[0] for p in told), flush=True)
    print("FIRST", json.dumps(first.communicate(timeout=120)[0]), flush=True)
    for p in burst:
        if p not in told:
            print("LAST", json.dumps(p.communicate(timeout=120)[0]), flush=True)
s.send(b":q\r")
s.finish(10)
"##;
    let out = tiza_qa::pty_script_with_deadline(
        &dir,
        "burst.py",
        body,
        &[
            tiza_qa::bin().to_str().unwrap(),
            file.to_str().unwrap(),
            runtime.to_str().unwrap(),
        ],
        150,
    );
    assert!(out.contains("STATUS 0"), "{out}");
    assert!(out.contains("TOLD 19"), "{out}");
    assert!(
        out.contains("FIRST_IN_FLIGHT True"),
        "the first check ended before the burst was told: it did not run long enough to tell\n{out}"
    );
    assert!(out.contains("BUSY 19"), "{out}");
    for who in ["FIRST", "LAST"] {
        let line = out
            .lines()
            .find(|l| l.starts_with(&format!("{who} ")))
            .unwrap_or_else(|| panic!("{who}:\n{out}"));
        assert!(
            line.contains("a bug in Tiza stopped this request"),
            "{line}"
        );
    }
    let log = std::fs::read_to_string(runtime.join("tiza/panics.log"))
        .unwrap_or_else(|e| panic!("{e}: {out}"));
    assert_eq!(log.lines().count(), 2, "{log}");
}

/// Step 3 / AC3, counted: with the test hook every check computation
/// panics once it is done (one line in `panics.log` each). Two checks at
/// once leave one line and two `internal` replies, and the TUI keeps
/// answering (the panic on the check thread neither closes it nor
/// restores its terminal).
#[test]
fn ac3_two_checks_at_once_are_one_computation_and_a_panic_in_it_is_caught() {
    let file = big_file("t210-ac3-hook");
    let dir = file.parent().unwrap().to_path_buf();
    let runtime = dir.join("run");
    std::fs::create_dir_all(&runtime).unwrap();
    let body = r##"
import json
binary, doc, runtime = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(os.path.dirname(doc))
sock = "h.sock"
env = dict(os.environ, TIZA_TEST_HOOK="request:check", XDG_RUNTIME_DIR=runtime)
def api(*args, timeout=120):
    r = subprocess.run([binary, "api", *args, "--socket", sock], capture_output=True, text=True, timeout=timeout)
    return r.returncode, r.stdout + r.stderr
s = Pty([binary, "--socket", sock, "--no-save", doc], 30, 120, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 20, "first frame"):
    a = subprocess.Popen([binary, "api", "check", "{}", "--socket", sock], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    api("status", timeout=2)
    b = subprocess.Popen([binary, "api", "check", "{}", "--socket", sock], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    code, text = api("status", timeout=2)
    print("STATUS", code, flush=True)
    print("A", json.dumps(a.communicate(timeout=120)[0]), flush=True)
    print("B", json.dumps(b.communicate(timeout=120)[0]), flush=True)
    code, text = api("status", timeout=10)
    print("AFTER", code, flush=True)
    print("CRASHED", "tiza crashed" in plain(s.out), flush=True)
s.send(b":q\r")
s.finish(10)
"##;
    let out = tiza_qa::pty_script_with_deadline(
        &dir,
        "hook.py",
        body,
        &[
            tiza_qa::bin().to_str().unwrap(),
            file.to_str().unwrap(),
            runtime.to_str().unwrap(),
        ],
        150,
    );
    assert!(out.contains("STATUS 0"), "{out}");
    for who in ["A", "B"] {
        let line = out
            .lines()
            .find(|l| l.starts_with(&format!("{who} ")))
            .unwrap_or_else(|| panic!("{out}"));
        assert!(
            line.contains("a bug in Tiza stopped this request") && line.contains("internal"),
            "{line}"
        );
    }
    assert!(out.contains("AFTER 0"), "{out}");
    assert!(out.contains("CRASHED False"), "{out}");
    let log = std::fs::read_to_string(runtime.join("tiza/panics.log"))
        .unwrap_or_else(|e| panic!("{e}: {out}"));
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(
        log.contains("check panicked at crates/tiza-tui/src/checks.rs:"),
        "{log}"
    );
}

/// AC4: for a document that does not change, the live check is the CLI's
/// `tiza check --json`, field for field, plus the revision it checked —
/// at the default width and at one that clips labels.
#[test]
fn ac4_the_live_check_is_the_cli_check() {
    let (file, live) = big_live("t210-ac4");
    for (params, args) in [
        (json!({}), vec![]),
        (json!({"width": 60}), vec!["--width", "60"]),
    ] {
        let mut v = live.api_ok("check", params.clone());
        let revision = v.as_object_mut().unwrap().remove("revision");
        assert!(revision.as_ref().is_some_and(Value::is_u64), "{v}");
        let mut cli_args = vec!["check", file.to_str().unwrap(), "--json"];
        cli_args.extend(args);
        let out = tiza(&cli_args);
        let cli: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v, cli, "{params}: live\n{v}\nCLI\n{cli}");
    }
    let v = live.api_ok("check", json!({"width": 60}));
    assert!(v["warnings"].as_u64().unwrap() > 0, "{v}");
}

/// Step 4 / AC5: `:q`, Ctrl-C, SIGHUP and the terminal going away without
/// a SIGHUP (the pty closed from the master side, t-205) while a check
/// runs inside the instance. That it runs there is shown by the instance
/// itself: of three checks at three widths (the default, 60 and 70), one is
/// told `busy` — replaced while it waited, which only happens while another
/// check runs — and the other two get no reply before the quit. Then Tiza
/// exits within a second with status 0, leaves the alternate screen with
/// the line discipline restored (canonical mode and echo on), removes its
/// socket, and no check's client gets a report.
#[test]
fn ac5_quitting_while_a_check_runs_exits_at_once() {
    let file = big_file("t210-ac5");
    let dir = file.parent().unwrap().to_path_buf();
    let body = r##"
import json
binary, doc = sys.argv[1], sys.argv[2]
os.chdir(os.path.dirname(doc))
for how in ["q", "ctrl-c", "hup", "close"]:
    sock = how + ".sock"
    s = Pty([binary, "--socket", sock, "--no-save", doc], 30, 120)
    if not s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 20, "first frame"):
        break
    checks = [subprocess.Popen([binary, "api", "check", json.dumps(p), "--socket", sock], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
              for p in [{}, {"width": 60}, {"width": 70}]]
    try:
        r = subprocess.run([binary, "api", "status", "--socket", sock], capture_output=True, text=True, timeout=2)
        print(how, "STATUS", r.returncode, flush=True)
    except subprocess.TimeoutExpired:
        print(how, "STATUS_TIMEOUT", flush=True)
    end = time.monotonic() + 5
    while not any(c.poll() is not None for c in checks) and time.monotonic() < end:
        s.settle(0.02)
    done = [c for c in checks if c.poll() is not None]
    print(how, "TOLD", len(done), all("busy" in c.communicate(timeout=5)[0] for c in done), flush=True)
    t0 = time.monotonic()
    if how == "q":
        s.send(b":q\r")
    elif how == "ctrl-c":
        s.send(b"\x03")
    elif how == "hup":
        os.kill(s.p.pid, signal.SIGHUP)
    else:
        s._stop_reader()
        os.close(s.master)
        s.closed = True
    try:
        s.p.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass
    took = time.monotonic() - t0
    print(how, "EXIT", s.p.poll(), "%.2f" % took, flush=True)
    print(how, "SOCKET_LEFT", os.path.exists(sock), flush=True)
    if how != "close":
        lflag = termios.tcgetattr(s.master)[3]
        print(how, "TTY", bool(lflag & termios.ICANON), bool(lflag & termios.ECHO), flush=True)
        s.finish(5)
        out = s.out
        print(how, "LEFT_ALT", out.rfind(b"\x1b[?1049l") > out.rfind(b"\x1b[?1049h"), flush=True)
    else:
        s.finish(5)
    got = [c.communicate(timeout=120)[0] for c in checks if c not in done]
    print(how, "CLIENT_GOT_REPORT", any('"findings"' in g for g in got), flush=True)
"##;
    let out = tiza_qa::pty_script_with_deadline(
        &dir,
        "quit.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), file.to_str().unwrap()],
        150,
    );
    for how in ["q", "ctrl-c", "hup", "close"] {
        let has = |what: &str| out.contains(&format!("{how} {what}"));
        assert!(has("STATUS 0"), "{how}:\n{out}");
        assert!(
            has("TOLD 1 True"),
            "{how}: no check was told `busy`, so none was shown running in the instance\n{out}"
        );
        let exit = out
            .lines()
            .find(|l| l.starts_with(&format!("{how} EXIT ")))
            .unwrap_or_else(|| panic!("{how}:\n{out}"));
        let fields: Vec<&str> = exit.split_whitespace().collect();
        assert_eq!(fields[2], "0", "{how}: {exit}");
        let took: f64 = fields[3].parse().unwrap();
        assert!(took < 1.0, "{how}: exited {took} s after quitting\n{out}");
        assert!(has("SOCKET_LEFT False"), "{how}:\n{out}");
        if how != "close" {
            assert!(has("TTY True True"), "{how}:\n{out}");
            assert!(has("LEFT_ALT True"), "{how}:\n{out}");
        }
        assert!(has("CLIENT_GOT_REPORT False"), "{how}:\n{out}");
    }
}

/// Corner case: an MCP client that stops waiting for `check` before it
/// ends. Its other calls (`status`) are answered meanwhile, and the
/// server still answers the check when it is done, with the report.
#[test]
fn corner_mcp_check_with_a_client_timeout_shorter_than_the_check() {
    let (_, live) = big_live("t210-mcp");
    let mut mcp = McpClient::start(&live.socket);
    let check = mcp.send("tools/call", json!({"name": "check", "arguments": {}}));
    assert!(
        mcp.reply_within(check, Duration::from_millis(200))
            .is_none(),
        "the check answered within 0.2 s: it did not run long enough to tell"
    );
    let status = mcp.send("tools/call", json!({"name": "status", "arguments": {}}));
    let s = mcp
        .reply_within(status, QUICK)
        .unwrap_or_else(|| panic!("MCP status was not answered within {QUICK:?}"));
    assert!(s["result"]["isError"] != true, "{s}");
    let v = mcp
        .reply_within(check, CHECK)
        .expect("the MCP check never answered");
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    let report: Value = serde_json::from_str(text).unwrap();
    assert_eq!(report["max_step"], 4, "{report}");
    assert!(report["revision"].is_u64(), "{report}");
}

/// Corner case: `render` and `--smoke` do not go through the live check
/// and are unchanged: a render of the big document, and one smoke frame
/// that starts and exits cleanly.
#[test]
fn corner_render_and_smoke_are_unchanged() {
    let file = big_file("t210-render");
    let out = tiza(&[
        "render",
        file.to_str().unwrap(),
        "--step",
        "2",
        "--format",
        "txt",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("step_0") && text.contains("function number 4"),
        "{text}"
    );
    let body = r##"
binary, doc = sys.argv[1], sys.argv[2]
s = Pty([binary, doc, "--smoke", "--no-socket", "--no-save"], 30, 120, ctty=True)
print("SMOKE", s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        file.parent().unwrap(),
        "smoke.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), file.to_str().unwrap()],
    );
    assert!(out.contains("SMOKE 0"), "{out}");
}

/// AC6: the CHANGELOG names the card under Fixed, and the agent guide
/// says what the reply's `revision` is.
#[test]
fn ac6_changelog_and_agent_guide() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let log = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    let fixed = &log[log.find("### Fixed").unwrap()..];
    let fixed = &fixed[..fixed[4..].find("\n## ").map_or(fixed.len(), |i| i + 4)];
    assert!(
        fixed.contains("(t-210"),
        "CHANGELOG.md has no Fixed entry for t-210"
    );
    let guide = std::fs::read_to_string(root.join("docs/agent-guide.md")).unwrap();
    assert!(
        guide.contains("t-210") && guide.contains("`revision`"),
        "the agent guide does not say what check's `revision` is"
    );
}
