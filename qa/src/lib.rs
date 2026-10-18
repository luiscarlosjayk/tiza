//! Helpers shared by the per-ticket QA tests in `qa/tests/`.
//!
//! Each `tests/t-XXX.rs` is compiled as its own crate and maps to one board
//! task: its `//!` header repeats the task's acceptance criteria, and each
//! `#[test]` is one of the manual test steps or corner cases from the card.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, Once, mpsc};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tiza_core::Document;
use tiza_tui::app::App;

/// Path of a fixture under `qa/fixtures/`.
pub fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(rel)
}

/// Path of a bundled example under `examples/` (the demos, not test data).
pub fn example(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples")
        .join(rel)
}

/// A scratch directory unique to this test binary + name, emptied on each call.
pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tiza-qa-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The `tiza` binary of the current profile, built on first use.
pub fn bin() -> PathBuf {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "tiza", "--quiet"])
            .args(if profile_dir().ends_with("release") {
                vec!["--release"]
            } else {
                vec![]
            })
            .status()
            .expect("cargo build");
        assert!(status.success(), "building tiza failed");
    });
    profile_dir().join("tiza")
}

fn profile_dir() -> PathBuf {
    // target/<profile>/deps/t_xxx-hash → target/<profile>
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Runs `tiza <args>` and returns its output.
pub fn tiza(args: &[&str]) -> Output {
    Command::new(bin()).args(args).output().expect("run tiza")
}

/// Runs `tiza <args>`, asserting success, and returns stdout as text.
pub fn tiza_ok(args: &[&str]) -> String {
    let out = tiza(args);
    assert!(
        out.status.success(),
        "tiza {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// A headless Tiza instance: the real `App` and the real socket server,
/// driven from a background thread, so a test can talk to it over the socket
/// (`tiza api …`, MCP) and press keys as a presenter would.
pub struct Live {
    pub socket: PathBuf,
    tx: Mutex<Option<mpsc::Sender<Job>>>,
    join: Option<std::thread::JoinHandle<()>>,
}

type Job = Box<dyn FnOnce(&mut App) + Send>;

impl Live {
    /// Opens `doc` (saved to `save_path` on autosave) and starts serving `socket`.
    pub fn open(doc: Document, save_path: PathBuf, socket: PathBuf) -> Live {
        let (tx, rx) = mpsc::channel::<Job>();
        let sock = socket.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let join = std::thread::spawn(move || {
            let mut app = App::new(doc, save_path);
            let server = tiza_tui::ipc::start(&sock).expect("socket");
            app.socket_path = Some(server.path.clone());
            ready_tx.send(()).unwrap();
            loop {
                while let Ok(r) = server.rx.try_recv() {
                    app.handle_ipc(r.req, r.reply);
                }
                app.follow_tick();
                app.expire_proposals();
                app.typing_tick();
                app.auto_advance_tick(std::time::Instant::now());
                app.present_message_tick(std::time::Instant::now());
                match rx.recv_timeout(std::time::Duration::from_millis(10)) {
                    Ok(job) => job(&mut app),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                // `quit` is only a flag here: tests inspect it after `q`.
            }
        });
        ready_rx.recv().unwrap();
        Live {
            socket,
            tx: Mutex::new(Some(tx)),
            join: Some(join),
        }
    }

    /// Opens a document from code text with a throwaway save path.
    pub fn from_code(code: &str, lang: &str, name: &str) -> Live {
        let dir = scratch(name);
        Live::open(
            Document::from_code(code, lang, None),
            dir.join("doc.tiza"),
            dir.join("live.sock"),
        )
    }

    /// Runs `f` on the app thread and returns its result.
    pub fn with<R: Send + 'static>(&self, f: impl FnOnce(&mut App) -> R + Send + 'static) -> R {
        let (rtx, rrx) = mpsc::channel();
        let guard = self.tx.lock().unwrap();
        guard
            .as_ref()
            .unwrap()
            .send(Box::new(move |app| {
                let _ = rtx.send(f(app));
            }))
            .unwrap();
        rrx.recv().unwrap()
    }

    /// Presses keys exactly as the TUI would handle them.
    pub fn keys(&self, keys: &[KeyCode]) {
        let keys: Vec<KeyCode> = keys.to_vec();
        self.with(move |app| {
            for k in keys {
                tiza_tui::on_key(app, k, KeyModifiers::NONE);
            }
        });
    }

    /// Types text as individual key presses (use `KeyCode::Enter` via `keys`).
    pub fn type_text(&self, text: &str) {
        let keys: Vec<KeyCode> = text.chars().map(KeyCode::Char).collect();
        self.keys(&keys);
    }

    /// Runs a `:` command directly.
    pub fn command(&self, cmd: &str) {
        let cmd = cmd.to_string();
        self.with(move |app| app.run_command(&cmd));
    }

    /// Calls the local API over the socket, like `tiza api` does.
    pub fn api(&self, method: &str, params: serde_json::Value) -> tiza_core::protocol::Response {
        tiza_tui::ipc::call(&self.socket, method, params).expect("socket call")
    }

    /// Calls the API and asserts success, returning the result.
    pub fn api_ok(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let r = self.api(method, params);
        assert!(r.error.is_none(), "{method}: {:?}", r.error);
        r.result.unwrap_or(serde_json::Value::Null)
    }

    /// The screen as plain text, drawn with ratatui's TestBackend at `cols×rows`.
    pub fn screen(&self, cols: u16, rows: u16) -> String {
        self.with(move |app| {
            let mut t = Terminal::new(TestBackend::new(cols, rows)).unwrap();
            t.draw(|f| tiza_tui::ui::draw(f, app)).unwrap();
            let buf = t.backend().buffer().clone();
            (0..rows)
                .map(|y| {
                    (0..cols)
                        .map(|x| buf[(x, y)].symbol().to_string())
                        .collect::<String>()
                        .trim_end()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    /// Two snapshot-friendly dumps of the screen at `cols×rows`: the symbols
    /// (one line per row, trailing blanks trimmed) and the styles (per row, a
    /// run-length list of `x:fg/bg[+mods]` wherever the style changes).
    /// Display of the TestBackend does not carry colors, hence the second dump.
    pub fn snapshot(&self, cols: u16, rows: u16) -> (String, String) {
        self.with(move |app| {
            let mut t = Terminal::new(TestBackend::new(cols, rows)).unwrap();
            t.draw(|f| tiza_tui::ui::draw(f, app)).unwrap();
            let buf = t.backend().buffer().clone();
            let symbols = (0..rows)
                .map(|y| {
                    (0..cols)
                        .map(|x| buf[(x, y)].symbol().to_string())
                        .collect::<String>()
                        .trim_end()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n");
            let color = |c: ratatui::style::Color| match c {
                ratatui::style::Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
                ratatui::style::Color::Reset => "reset".into(),
                other => format!("{other:?}").to_lowercase(),
            };
            let mut styles = String::new();
            for y in 0..rows {
                let mut last: Option<String> = None;
                let mut row = format!("{y:>3}:");
                for x in 0..cols {
                    let c = &buf[(x, y)];
                    let mut mods = Vec::new();
                    let m = c.modifier;
                    if m.contains(ratatui::style::Modifier::BOLD) {
                        mods.push("b");
                    }
                    if m.contains(ratatui::style::Modifier::ITALIC) {
                        mods.push("i");
                    }
                    if m.contains(ratatui::style::Modifier::DIM) {
                        mods.push("d");
                    }
                    if m.contains(ratatui::style::Modifier::REVERSED) {
                        mods.push("r");
                    }
                    if m.contains(ratatui::style::Modifier::UNDERLINED) {
                        mods.push("u");
                    }
                    let key = format!(
                        "{}/{}{}",
                        color(c.fg),
                        color(c.bg),
                        if mods.is_empty() {
                            String::new()
                        } else {
                            format!("+{}", mods.join(""))
                        }
                    );
                    if last.as_deref() != Some(&key) {
                        row.push_str(&format!(" {x}:{key}"));
                        last = Some(key);
                    }
                }
                styles.push_str(&row);
                styles.push('\n');
            }
            (symbols, styles)
        })
    }

    /// The last status-line message and whether it was an error.
    pub fn message(&self) -> Option<(String, bool)> {
        self.with(|app| app.message.clone())
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.tx.lock().unwrap().take();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// A minimal MCP client over stdio: spawns `tiza mcp`, performs the
/// initialize handshake and lets a test call tools one request at a time,
/// reading each JSON-RPC reply as it arrives (so reconnection and errors can
/// be observed in order, unlike a one-shot pipe).
pub struct McpClient {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    reader: std::io::BufReader<std::process::ChildStdout>,
    next_id: u64,
    /// The `initialize` result (capabilities, instructions, serverInfo).
    init: serde_json::Value,
}

impl McpClient {
    /// Starts `tiza mcp --socket <socket>` and completes `initialize`.
    pub fn start(socket: &Path) -> McpClient {
        use std::io::{BufRead, Write};
        let mut child = Command::new(bin())
            .args(["mcp", "--socket", socket.to_str().unwrap()])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn tiza mcp");
        let stdin = child.stdin.take().unwrap();
        let reader = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut c = McpClient {
            child,
            stdin,
            reader,
            next_id: 1,
            init: serde_json::Value::Null,
        };
        let init = c.request(
            "initialize",
            serde_json::json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "tiza-qa", "version": "0"}}),
        );
        assert!(init["result"]["serverInfo"]["name"] == "tiza", "{init}");
        c.init = init["result"].clone();
        c.stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .unwrap();
        c.stdin.flush().unwrap();
        let _ = &mut c.reader as &mut dyn BufRead;
        c
    }

    /// What `initialize` returned: capabilities, instructions, serverInfo.
    pub fn initialize_result(&self) -> &serde_json::Value {
        &self.init
    }

    /// Sends one JSON-RPC request and returns its reply (notifications are skipped).
    pub fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        use std::io::{BufRead, Write};
        let id = self.next_id;
        self.next_id += 1;
        let msg =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.stdin.write_all(msg.to_string().as_bytes()).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
        loop {
            let mut line = String::new();
            let n = self
                .reader
                .read_line(&mut line)
                .expect("read from tiza mcp");
            assert!(
                n > 0,
                "tiza mcp closed its stdout before answering {method}"
            );
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line)
                && v["id"] == id
            {
                return v;
            }
        }
    }

    /// `tools/list` → tool names.
    pub fn tool_names(&mut self) -> Vec<String> {
        let v = self.request("tools/list", serde_json::json!({}));
        v["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }

    /// The `tools/list` result in full (names, descriptions, input schemas).
    pub fn tools(&mut self) -> serde_json::Value {
        self.request("tools/list", serde_json::json!({}))["result"]["tools"].clone()
    }

    /// `tools/call` → (is_error, text of the first content block, raw reply).
    pub fn call(
        &mut self,
        name: &str,
        args: serde_json::Value,
    ) -> (bool, String, serde_json::Value) {
        let v = self.request(
            "tools/call",
            serde_json::json!({"name": name, "arguments": args}),
        );
        let is_error = v["result"]["isError"] == true || !v["error"].is_null();
        let text = v["result"]["content"][0]["text"]
            .as_str()
            .map(str::to_string)
            .or_else(|| v["error"]["message"].as_str().map(str::to_string))
            .unwrap_or_default();
        (is_error, text, v)
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What a Python helper run returned. `status` is `None` when it was killed
/// at its wall-clock deadline.
#[derive(Debug)]
pub struct PyRun {
    pub stdout: String,
    pub stderr: String,
    pub status: Option<std::process::ExitStatus>,
    pub elapsed: std::time::Duration,
}

impl PyRun {
    pub fn timed_out(&self) -> bool {
        self.status.is_none()
    }
}

/// Runs `python3 -I script args…` with a hard wall-clock `timeout`: past it
/// the process is killed and the run comes back with `status: None`, so a
/// stuck child fails a test in seconds instead of hanging `cargo test`.
/// stdout/stderr are drained on their own threads (a full pipe never stalls
/// the child) and whatever was printed before the kill is kept.
pub fn python(script: &Path, args: &[&str], timeout: std::time::Duration) -> PyRun {
    python_env(script, args, timeout, &[])
}

fn python_env(
    script: &Path,
    args: &[&str],
    timeout: std::time::Duration,
    env: &[(&str, String)],
) -> PyRun {
    use std::io::Read;
    use std::process::Stdio;
    let start = std::time::Instant::now();
    let mut child = Command::new("python3")
        .arg("-I")
        .arg(script)
        .args(args)
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("python3");
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let drain = |mut r: Box<dyn Read + Send>, tx: mpsc::Sender<()>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf);
            let _ = tx.send(());
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let out = drain(Box::new(child.stdout.take().unwrap()), done_tx.clone());
    let err = drain(Box::new(child.stderr.take().unwrap()), done_tx);
    // Both pipes reach EOF when python exits (its pty children never inherit
    // them: `close_fds=True`), so waiting on the two readers is waiting on it.
    let mut finished = true;
    for _ in 0..2 {
        let left = timeout.saturating_sub(start.elapsed());
        if done_rx.recv_timeout(left).is_err() {
            finished = false;
            break;
        }
    }
    let status = if finished {
        Some(child.wait().expect("wait python3"))
    } else {
        let _ = child.kill();
        let _ = child.wait();
        None
    };
    PyRun {
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
        status,
        elapsed: start.elapsed(),
    }
}

/// The Python prelude of every pty test (`qa/src/pty.py`): a `Pty` class whose
/// reader thread drains the pty all the time with `select`-bounded reads,
/// `wait_for(pred, timeout)` instead of sleeps, `finish()` that kills a child
/// that does not exit, and a SIGALRM backstop at `TIZA_PTY_DEADLINE` seconds.
pub const PTY_PRELUDE: &str = include_str!("pty.py");

/// Default hard deadline of one pty script, Python side (seconds). The Rust
/// side kills the interpreter 10 s after it.
pub const PTY_DEADLINE_S: u64 = 40;

/// Writes `PTY_PRELUDE` + `body` to `dir/name`, runs it with `args` and
/// returns its stdout. Panics with everything captured if the run had to be
/// killed or Python itself failed.
pub fn pty_script(dir: &Path, name: &str, body: &str, args: &[&str]) -> String {
    pty_script_with_deadline(dir, name, body, args, PTY_DEADLINE_S)
}

/// `pty_script` with its own Python-side deadline (seconds).
pub fn pty_script_with_deadline(
    dir: &Path,
    name: &str,
    body: &str,
    args: &[&str],
    deadline_s: u64,
) -> String {
    let script = dir.join(name);
    std::fs::write(&script, format!("{PTY_PRELUDE}\n{body}")).unwrap();
    let run = python_env(
        &script,
        args,
        std::time::Duration::from_secs(deadline_s + 10),
        &[("TIZA_PTY_DEADLINE", deadline_s.to_string())],
    );
    assert!(
        !run.timed_out(),
        "pty script {name} killed after {:?}\nstdout:\n{}\nstderr:\n{}",
        run.elapsed,
        run.stdout,
        run.stderr
    );
    assert!(
        run.stderr.trim().is_empty() || run.status.is_some_and(|s| s.success()),
        "pty script {name} failed ({:?}):\n{}\nstdout:\n{}",
        run.status,
        run.stderr,
        run.stdout
    );
    run.stdout
}

/// What `scripts/legibility.py` (t-072) needs and this machine lacks:
/// `ffmpeg`, and `python3 -I -c "import PIL, numpy"` (Pillow and numpy are
/// QA-only dependencies, never installed by the tests). Empty means the
/// pipeline can run; otherwise each entry names one missing piece, for the
/// skip notice of the tests that run the script (t-072, t-119).
pub fn legibility_tools_missing() -> Vec<&'static str> {
    let ok = |cmd: &str, args: &[&str]| {
        Command::new(cmd)
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let mut missing = Vec::new();
    if !ok("ffmpeg", &["-version"]) {
        missing.push("ffmpeg");
    }
    if !ok("python3", &["-I", "-c", "import PIL, numpy"]) {
        if !ok("python3", &["-I", "-c", "pass"]) {
            missing.push("python3");
        } else {
            if !ok("python3", &["-I", "-c", "import PIL"]) {
                missing.push("Pillow (python3 -I -c \"import PIL\")");
            }
            if !ok("python3", &["-I", "-c", "import numpy"]) {
                missing.push("numpy (python3 -I -c \"import numpy\")");
            }
        }
    }
    missing
}
