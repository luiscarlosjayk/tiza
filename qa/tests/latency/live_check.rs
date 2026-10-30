//! Quitting while a live check runs exits at once.

use std::path::PathBuf;

use serde_json::{Value, json};
use tiza_qa::scratch;

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
        "title": format!("Big tutorial ({lines} lines)"),
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

/// Step 4 / AC5: `:q`, Ctrl-C, SIGHUP and the terminal going away without
/// a SIGHUP (the pty closed from the master side) while a check
/// runs inside the instance. That it runs there is shown by the instance
/// itself: of three checks at three widths (the default, 60 and 70), one is
/// told `busy` — replaced while it waited, which only happens while another
/// check runs — and the other two get no reply before the quit. The first
/// check waits for the file `check-hold` (`TIZA_TEST_HOOK=check:hold`), so
/// it still runs when Tiza quits, however fast the machine;
/// `status.checks` shows that it waits. Then Tiza
/// exits with status 0 (within a second on an idle machine; waited for up
/// to 10 s, so a loaded one is no false red), leaves the alternate screen with
/// the line discipline restored (canonical mode and echo on), removes its
/// socket, and no check's client gets a report.
#[test]
fn quitting_while_a_check_runs_exits_at_once() {
    let _alone = crate::alone();
    let file = big_file("ac5");
    let dir = file.parent().unwrap().to_path_buf();
    let runtime = dir.join("run");
    std::fs::create_dir_all(&runtime).unwrap();
    let body = r##"
import json
binary, doc, runtime = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(os.path.dirname(doc))
# While this file is there, a check waits before it computes. It stays
# until the end: the instance quits while its first check waits.
hold = os.path.join(runtime, "tiza", "check-hold")
os.makedirs(os.path.dirname(hold), exist_ok=True)
open(hold, "w").close()
env = dict(os.environ, TIZA_TEST_HOOK="check:hold", XDG_RUNTIME_DIR=runtime)
# How long one check of this document takes here and now (a loaded machine
# stretches it as it stretches the quit): the quit must take far less.
t = time.monotonic()
subprocess.run([binary, "check", doc, "--json"], capture_output=True, timeout=120)
print("CHECK_TIME %.3f" % (time.monotonic() - t), flush=True)
for how in ["q", "ctrl-c", "hup", "close"]:
    sock = how + ".sock"
    s = Pty([binary, "--socket", sock, "--no-save", doc], 30, 120, env=env)
    if not s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 20, "first frame"):
        break
    checks = [subprocess.Popen([binary, "api", "check", json.dumps(p), "--socket", sock], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
              for p in [{}, {"width": 60}, {"width": 70}]]
    # Until a check waits for the file (a status call while it does).
    held = None
    try:
        end = time.monotonic() + 10
        while True:
            r = subprocess.run([binary, "api", "status", "--socket", sock], capture_output=True, text=True, timeout=10)
            held = json.loads(r.stdout)["checks"]["held"] if r.returncode == 0 else None
            if held == 1 or time.monotonic() > end:
                break
            s.settle(0.02)
        print(how, "STATUS", r.returncode, flush=True)
    except subprocess.TimeoutExpired:
        print(how, "STATUS_TIMEOUT", flush=True)
    print(how, "HELD", held, flush=True)
    end = time.monotonic() + 10
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
        &[
            tiza_qa::bin().to_str().unwrap(),
            file.to_str().unwrap(),
            runtime.to_str().unwrap(),
        ],
        150,
    );
    let check_time: f64 = out
        .lines()
        .find_map(|l| l.strip_prefix("CHECK_TIME "))
        .unwrap_or_else(|| panic!("{out}"))
        .parse()
        .unwrap();
    for how in ["q", "ctrl-c", "hup", "close"] {
        let has = |what: &str| out.contains(&format!("{how} {what}"));
        assert!(has("STATUS 0"), "{how}:\n{out}");
        assert!(
            has("HELD 1"),
            "{how}: the first check did not wait for the file\n{out}"
        );
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
        // That the quit did not wait for the check is shown by the order
        // (no check's client got a report, below); how soon it exits is a
        // condition with a generous timeout, not a budget.
        let took: f64 = fields[3].parse().unwrap();
        assert!(took < 10.0, "{how}: exited {took} s after quitting\n{out}");
        println!("{how}: exited {took} s after quitting; one check takes {check_time} s");
        // Waiting for the running check would take about one check.
        if check_time >= 1.0 {
            assert!(
                took < check_time / 2.0,
                "{how}: exited {took} s after quitting, a check takes {check_time} s\n{out}"
            );
        }
        assert!(has("SOCKET_LEFT False"), "{how}:\n{out}");
        if how != "close" {
            assert!(has("TTY True True"), "{how}:\n{out}");
            assert!(has("LEFT_ALT True"), "{how}:\n{out}");
        }
        assert!(has("CLIENT_GOT_REPORT False"), "{how}:\n{out}");
    }
}
