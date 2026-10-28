//! Helpers shared by the QA tests in `qa/tests/`.
//!
//! Each `tests/<area>/main.rs` is compiled as its own crate, with one
//! module for each behavior of that area.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex, Once, mpsc};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tiza_core::Document;
use tiza_tui::app::App;

pub mod markdown;

/// The root of the repository: the folder above `qa/`.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The text of a file of the repository, by its path from the root.
pub fn repo_file(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

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

/// A new empty scratch directory: `tiza-qa-{pid}-{name}`, and for a name
/// that this process used before, `tiza-qa-{pid}-{name}-{n}` (n = 2, 3…).
/// So two calls never share a folder, also with the same name. Tests run
/// in parallel in one process: a shared folder that the second call
/// emptied lost the first test's files. The first use keeps the short
/// name: a socket in the folder must stay under the length limit of a
/// Unix socket path (104 bytes on macOS). To give a folder to other code,
/// pass the `PathBuf`, not the name.
pub fn scratch(name: &str) -> PathBuf {
    static USES: Mutex<Option<std::collections::HashMap<String, usize>>> = Mutex::new(None);
    let n = {
        let mut uses = USES.lock().unwrap_or_else(|e| e.into_inner());
        let count = uses
            .get_or_insert_with(Default::default)
            .entry(name.to_string())
            .or_insert(0);
        *count += 1;
        *count
    };
    let folder = if n == 1 {
        format!("tiza-qa-{}-{name}", std::process::id())
    } else {
        format!("tiza-qa-{}-{name}-{n}", std::process::id())
    };
    let dir = std::env::temp_dir().join(folder);
    // A folder left by an earlier process with the same pid.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// How long a test waits for a follow watcher to start. A start takes
/// milliseconds; on macOS it takes seconds when the folder of the test
/// binary holds many files (see [`slow_follow_start`]).
pub const FOLLOW_START_BUDGET: Duration = Duration::from_secs(30);

/// Waits while `starting` says a follow watcher is still starting, on this
/// thread, so the app thread goes on answering requests. How long it took,
/// or, past `budget`, the message of a slow start.
pub fn wait_for_watcher(
    budget: Duration,
    mut starting: impl FnMut() -> bool,
) -> Result<Duration, String> {
    let start = Instant::now();
    while starting() {
        if start.elapsed() >= budget {
            return Err(slow_follow_start(start.elapsed()));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(start.elapsed())
}

/// Why a follow watcher took `waited` to start. On macOS, fseventsd reads
/// the folder of the client's executable each time a watcher starts, and
/// one start at a time per process: with a million old `.o` files in
/// `target/debug/deps` that is 2 s for each start, and tests in parallel
/// wait for each other's.
pub fn slow_follow_start(waited: Duration) -> String {
    let folder = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let files = folder
        .as_ref()
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .map(|entries| entries.count());
    let held = match (&folder, files) {
        (Some(dir), Some(n)) => format!(" ({} holds {n} files)", dir.display()),
        _ => String::new(),
    };
    let secs = waited.as_secs_f64();
    if cfg!(target_os = "macos") {
        format!(
            "the follow watcher did not start in {secs:.1} s: FSEvents starts it slowly. \
             fseventsd reads the folder of the test binary for each watcher{held}; \
             run `cargo clean` to empty it"
        )
    } else {
        format!("the follow watcher did not start in {secs:.1} s")
    }
}

/// The `tiza` binary of the current profile, built on first use.
pub fn bin() -> PathBuf {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let status = Command::new(env!("CARGO"))
            // The test hooks are in the binary the tests run and
            // never in a release build.
            .args(["build", "-p", "tiza", "--features", "test-hooks", "--quiet"])
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

/// The config that every child of the tests reads: an empty file, and an
/// empty config folder for a child without `TIZA_CONFIG`. The config of
/// whoever runs the tests (`~/.config/tiza/config.toml`, an exported
/// `TIZA_CONFIG`) never changes a result. A test that needs a config sets
/// `TIZA_CONFIG` after these.
pub fn isolated_config_env() -> [(&'static str, PathBuf); 2] {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    let dir = DIR.get_or_init(|| {
        let dir = scratch("no-config");
        std::fs::create_dir_all(dir.join("config")).unwrap();
        std::fs::write(dir.join("config.toml"), "").unwrap();
        dir
    });
    [
        ("TIZA_CONFIG", dir.join("config.toml")),
        ("XDG_CONFIG_HOME", dir.join("config")),
    ]
}

/// `tiza` as the tests run it: [`bin`] with [`isolated_config_env`]. Every
/// test starts the binary with this (or with a helper that uses it).
pub fn command() -> Command {
    let mut c = Command::new(bin());
    c.envs(isolated_config_env());
    c
}

/// Runs `tiza <args>` and returns its output.
pub fn tiza(args: &[&str]) -> Output {
    command().args(args).output().expect("run tiza")
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

/// Runs `tiza <args>` in `dir` with `TIZA_CONFIG=config` (relative to `dir`,
/// or absolute): its exit code, stdout and stderr.
pub fn tiza_with_config(dir: &Path, config: &str, args: &[&str]) -> (i32, String, String) {
    let out = command()
        .current_dir(dir)
        .args(args)
        .env("TIZA_CONFIG", config)
        .output()
        .expect("run tiza");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A headless Tiza instance: the real `App` and the real socket server,
/// driven from a background thread, so a test can talk to it over the socket
/// (`tiza api …`, MCP) and press keys as a presenter would.
pub struct Live {
    pub socket: PathBuf,
    tx: Mutex<Option<mpsc::Sender<Job>>>,
    join: Option<std::thread::JoinHandle<()>>,
    steps: Arc<Mutex<Vec<u32>>>,
}

type Job = Box<dyn FnOnce(&mut App) + Send>;

impl Live {
    /// Opens `doc` (saved to `save_path` on autosave) and starts serving `socket`.
    pub fn open(doc: Document, save_path: PathBuf, socket: PathBuf) -> Live {
        // A Unix socket path has room for 104 bytes on macOS: a long scratch
        // name fails here and says what to change.
        let len = socket.as_os_str().len();
        assert!(
            len < 100,
            "the socket path has {len} bytes, 100 or more: {}; use a shorter scratch name",
            socket.display()
        );
        let (tx, rx) = mpsc::channel::<Job>();
        let sock = socket.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let steps = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&steps);
        let join = std::thread::spawn(move || {
            let mut app = App::new(doc, save_path);
            let server = tiza_tui::ipc::start(&sock).expect("socket");
            app.socket_path = Some(server.path.clone());
            // See [`Live::steps`]: noted on this thread after each call that
            // can change the step, so no change is lost between two polls.
            let note = |app: &App| {
                let mut log = log.lock().unwrap();
                if log.last() != Some(&app.doc.step) {
                    log.push(app.doc.step);
                }
            };
            note(&app);
            ready_tx.send(()).unwrap();
            loop {
                while let Ok(r) = server.rx.try_recv() {
                    app.handle_ipc(r.req, r.reply);
                    note(&app);
                }
                app.follow_tick();
                note(&app);
                app.expire_proposals();
                note(&app);
                app.typing_tick();
                note(&app);
                app.auto_advance_tick(std::time::Instant::now());
                note(&app);
                app.present_message_tick(std::time::Instant::now());
                match rx.recv_timeout(std::time::Duration::from_millis(10)) {
                    Ok(job) => {
                        job(&mut app);
                        note(&app);
                    }
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
            steps,
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

    /// Every step the app thread has been at, in order: the step it opened
    /// on, then one entry for each change. The thread notes the step after
    /// each request, tick and job, and an auto-advance moves one step per
    /// tick, so a step that lasts less than a poll is in the list too. Read
    /// a sequence of steps from here, not by polling `doc.step`.
    pub fn steps(&self) -> Vec<u32> {
        self.steps.lock().unwrap().clone()
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
        self.wait_follow_started();
    }

    /// Waits until a follow watcher that is starting runs, outside any
    /// request: the tests save the file right after `follow on`, and the
    /// TUI does not wait for the watcher (it starts on a thread of its own).
    /// `api`, `command` and `keys` call it. Past [`FOLLOW_START_BUDGET`] the
    /// test fails and says why.
    pub fn wait_follow_started(&self) {
        if let Err(why) = self.wait_follow_started_within(FOLLOW_START_BUDGET) {
            panic!("{why}");
        }
    }

    /// [`Live::wait_follow_started`] with its own budget: how long the start
    /// took, or the message of a start that took longer.
    pub fn wait_follow_started_within(&self, budget: Duration) -> Result<Duration, String> {
        wait_for_watcher(budget, || self.with(|app| app.follow_starting()))
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
        self.wait_follow_started();
    }

    /// Calls the local API over the socket, like `tiza api` does.
    pub fn api(&self, method: &str, params: serde_json::Value) -> tiza_core::protocol::Response {
        let reply = tiza_tui::ipc::call(&self.socket, method, params).expect("socket call");
        self.wait_follow_started();
        reply
    }

    /// Calls the API and asserts success, returning the result.
    pub fn api_ok(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let r = self.api(method, params);
        assert!(r.error.is_none(), "{method}: {:?}", r.error);
        r.result.unwrap_or(serde_json::Value::Null)
    }

    /// The screen drawn with ratatui's TestBackend at `cols×rows`, cell by
    /// cell, with the symbol and the style of each cell.
    pub fn buffer(&self, cols: u16, rows: u16) -> ratatui::buffer::Buffer {
        self.with(move |app| {
            let mut t = Terminal::new(TestBackend::new(cols, rows)).unwrap();
            t.draw(|f| tiza_tui::ui::draw(f, app)).unwrap();
            t.backend().buffer().clone()
        })
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
    /// Lines of the server's stdout, read on a thread of its own, so a
    /// call the server never answers fails the test at a deadline instead
    /// of hanging `cargo test`.
    lines: mpsc::Receiver<String>,
    /// Replies that arrived while waiting for another one.
    early: std::collections::HashMap<u64, serde_json::Value>,
    next_id: u64,
    /// The `initialize` result (capabilities, instructions, serverInfo).
    init: serde_json::Value,
}

impl McpClient {
    /// Starts `tiza mcp --socket <socket>` and completes `initialize`.
    pub fn start(socket: &Path) -> McpClient {
        McpClient::start_with_env(socket, &[])
    }

    /// [`McpClient::start`] with extra environment variables for the server.
    pub fn start_with_env(socket: &Path, env: &[(&str, &str)]) -> McpClient {
        use std::io::{BufRead, Write};
        let mut child = command()
            .args(["mcp", "--socket", socket.to_str().unwrap()])
            // No config file unless the test gives one: `export_image`
            // reads it. A `TIZA_CONFIG` that names no file is what scripts
            // use for «no config».
            .env("TIZA_CONFIG", "/nonexistent/tiza-qa/config.toml")
            .envs(env.iter().copied())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn tiza mcp");
        let stdin = child.stdin.take().unwrap();
        let mut reader = std::io::BufReader::new(child.stdout.take().unwrap());
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let mut c = McpClient {
            child,
            stdin,
            lines,
            early: Default::default(),
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
        c
    }

    /// What `initialize` returned: capabilities, instructions, serverInfo.
    pub fn initialize_result(&self) -> &serde_json::Value {
        &self.init
    }

    /// Sends one JSON-RPC request and returns its reply (notifications are
    /// skipped). A server that does not answer within 120 s fails the test.
    pub fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let deadline = std::time::Duration::from_secs(120);
        self.request_within(method, params, deadline)
            .unwrap_or_else(|| panic!("tiza mcp did not answer {method} within {deadline:?}"))
    }

    /// [`McpClient::request`] with a deadline: `None` when the server has not
    /// answered by then (every call must answer).
    pub fn request_within(
        &mut self,
        method: &str,
        params: serde_json::Value,
        timeout: std::time::Duration,
    ) -> Option<serde_json::Value> {
        let id = self.send(method, params);
        self.reply_within(id, timeout)
    }

    /// Sends one JSON-RPC request without waiting: its id, for
    /// [`McpClient::reply_within`]. Several can be in flight at once (a
    /// slow export must not hold up `status`).
    pub fn send(&mut self, method: &str, params: serde_json::Value) -> u64 {
        use std::io::Write;
        let id = self.next_id;
        self.next_id += 1;
        let msg =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.stdin.write_all(msg.to_string().as_bytes()).unwrap();
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
        id
    }

    /// Whether the reply to request `id` already came while waiting for
    /// another one (it is kept for its own call): which of two came first.
    pub fn replied(&self, id: u64) -> bool {
        self.early.contains_key(&id)
    }

    /// The reply to request `id`, waiting at most `timeout`; replies to
    /// other requests that arrive meanwhile are kept for their own call.
    pub fn reply_within(
        &mut self,
        id: u64,
        timeout: std::time::Duration,
    ) -> Option<serde_json::Value> {
        if let Some(v) = self.early.remove(&id) {
            return Some(v);
        }
        let until = std::time::Instant::now() + timeout;
        loop {
            let left = until.saturating_duration_since(std::time::Instant::now());
            let line = match self.lines.recv_timeout(left) {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => return None,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("tiza mcp closed its stdout before answering request {id}")
                }
            };
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                match v["id"].as_u64() {
                    Some(got) if got == id => return Some(v),
                    Some(other) => {
                        self.early.insert(other, v);
                    }
                    None => {}
                }
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
    // The binaries that the script starts read the config of the tests.
    let mut child = Command::new("python3")
        .arg("-I")
        .arg(script)
        .args(args)
        .envs(isolated_config_env())
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

/// What `scripts/legibility.py` needs and this machine lacks:
/// `ffmpeg`, and `python3 -I -c "import PIL, numpy"` (Pillow and numpy are
/// QA-only dependencies, never installed by the tests). Empty means the
/// pipeline can run; otherwise each entry names one missing piece, for the
/// skip notice of the tests that run the script.
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

// ---- WebP files -------------------------------------------------------
// Two areas read the WebP exports (`export::webp` and the README images of
// `policy`), with image-webp, an independent decoder.

fn webp_decoder(bytes: &[u8]) -> image_webp::WebPDecoder<std::io::Cursor<&[u8]>> {
    image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).unwrap()
}

/// RGBA pixels and size of a WebP still.
pub fn webp_still(bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    let mut d = webp_decoder(bytes);
    assert!(!d.is_animated(), "an animation, not a still");
    let (w, h) = d.dimensions();
    let mut px = vec![0; d.output_buffer_size().unwrap()];
    d.read_image(&mut px).unwrap();
    if !d.has_alpha() {
        px = px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect();
    }
    (px, w, h)
}

/// The frames of an animated WebP and their times, put together here as
/// the decoder of libwebp does it: a frame without blending replaces its
/// rectangle; with blending, an opaque pixel replaces the one below and a
/// transparent pixel keeps it.
///
/// Not image-webp's `read_frame`: version 0.2.4 has a bug. It blends the
/// opaque pixels too, and its rounding takes 1 off some of them, so a
/// lossless frame would look lossy. libwebp copies them.
pub fn webp_frames(bytes: &[u8]) -> Vec<(Vec<u8>, u32)> {
    let u24 = |b: &[u8]| u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16;
    assert!(
        webp_decoder(bytes).is_animated(),
        "a still, not an animation"
    );
    // RGBA, also when the animation is opaque (the decoder then gives RGB).
    let (cw, ch) = webp_decoder(bytes).dimensions();
    let mut canvas = vec![0u8; (cw * ch * 4) as usize];
    let mut dispose = None;
    let mut out = Vec::new();
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let (id, body) = (&bytes[pos..pos + 4], &bytes[pos + 8..pos + 8 + size]);
        pos += 8 + size + (size & 1);
        if id != b"ANMF" {
            continue;
        }
        let (x, y) = (u24(&body[0..]) * 2, u24(&body[3..]) * 2);
        let (w, h) = (u24(&body[6..]) + 1, u24(&body[9..]) + 1);
        let blend = body[15] & 2 == 0;
        if let Some((dx, dy, dw, dh)) = dispose.take() {
            for r in dy..dy + dh {
                let i = ((r * cw + dx) * 4) as usize;
                canvas[i..i + (dw * 4) as usize].fill(0);
            }
        }
        let data = &body[16..];
        let mut still = b"RIFF".to_vec();
        still.extend((4 + data.len() as u32).to_le_bytes());
        still.extend(b"WEBP");
        still.extend(data);
        let (px, fw, fh) = webp_still(&still);
        assert_eq!((fw, fh), (w, h));
        for r in 0..h {
            for c in 0..w {
                let s = &px[((r * w + c) * 4) as usize..][..4];
                let i = (((y + r) * cw + x + c) * 4) as usize;
                match (blend, s[3]) {
                    (true, 0) => {}
                    (false, _) | (true, 255) => canvas[i..i + 4].copy_from_slice(s),
                    (true, a) => panic!("a blended pixel with alpha {a}"),
                }
            }
        }
        out.push((canvas.clone(), u24(&body[12..])));
        if body[15] & 1 == 1 {
            dispose = Some((x, y, w, h));
        }
    }
    out
}
