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
                    let resp = app.handle_request(r.req);
                    let _ = r.reply.send(resp);
                }
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
