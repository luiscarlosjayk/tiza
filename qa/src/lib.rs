//! Helpers shared by the per-ticket QA tests in `qa/tests/`.
//!
//! Each `tests/t-XXX.rs` is compiled as its own crate and maps to one board
//! task: its `//!` header repeats the task's acceptance criteria, and each
//! `#[test]` is one of the manual test steps or corner cases from the card.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, Once, mpsc};

use codedraw_core::Document;
use codedraw_tui::app::App;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

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
    let dir = std::env::temp_dir().join(format!("codedraw-qa-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The `codedraw` binary of the current profile, built on first use.
pub fn bin() -> PathBuf {
    static BUILD: Once = Once::new();
    BUILD.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "codedraw-cli", "--quiet"])
            .args(if profile_dir().ends_with("release") {
                vec!["--release"]
            } else {
                vec![]
            })
            .status()
            .expect("cargo build");
        assert!(status.success(), "building codedraw-cli failed");
    });
    profile_dir().join("codedraw")
}

fn profile_dir() -> PathBuf {
    // target/<profile>/deps/t_xxx-hash → target/<profile>
    let exe = std::env::current_exe().unwrap();
    exe.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Runs `codedraw <args>` and returns its output.
pub fn codedraw(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("run codedraw")
}

/// Runs `codedraw <args>`, asserting success, and returns stdout as text.
pub fn codedraw_ok(args: &[&str]) -> String {
    let out = codedraw(args);
    assert!(
        out.status.success(),
        "codedraw {args:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// A headless CodeDraw instance: the real `App` and the real socket server,
/// driven from a background thread, so a test can talk to it over the socket
/// (`codedraw api …`, MCP) and press keys as a presenter would.
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
            let server = codedraw_tui::ipc::start(&sock).expect("socket");
            app.socket_path = Some(server.path.clone());
            ready_tx.send(()).unwrap();
            loop {
                while let Ok(r) = server.rx.try_recv() {
                    app.handle_ipc(r.req, r.reply);
                }
                app.follow_tick();
                app.expire_proposals();
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
            dir.join("doc.codedraw"),
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
                codedraw_tui::on_key(app, k, KeyModifiers::NONE);
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

    /// Calls the local API over the socket, like `codedraw api` does.
    pub fn api(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> codedraw_core::protocol::Response {
        codedraw_tui::ipc::call(&self.socket, method, params).expect("socket call")
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
            t.draw(|f| codedraw_tui::ui::draw(f, app)).unwrap();
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
            t.draw(|f| codedraw_tui::ui::draw(f, app)).unwrap();
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

/// A minimal MCP client over stdio: spawns `codedraw mcp`, performs the
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
    /// Starts `codedraw mcp --socket <socket>` and completes `initialize`.
    pub fn start(socket: &Path) -> McpClient {
        use std::io::{BufRead, Write};
        let mut child = Command::new(bin())
            .args(["mcp", "--socket", socket.to_str().unwrap()])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn codedraw mcp");
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
            serde_json::json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "codedraw-qa", "version": "0"}}),
        );
        assert!(init["result"]["serverInfo"]["name"] == "codedraw", "{init}");
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
                .expect("read from codedraw mcp");
            assert!(
                n > 0,
                "codedraw mcp closed its stdout before answering {method}"
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
