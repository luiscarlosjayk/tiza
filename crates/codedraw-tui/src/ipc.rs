//! Local API server: Unix socket, one JSON request per line.
//! Each connection runs on its own thread and forwards requests to the TUI
//! thread over a channel; the response comes back over a one-shot channel.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use codedraw_core::protocol::{Request, Response};
use codedraw_core::socket;
use serde_json::Value;

pub struct IpcRequest {
    pub req: Request,
    pub reply: Sender<Response>,
}

/// Keeps the socket alive and removes it on drop.
pub struct IpcServer {
    pub path: PathBuf,
    pub rx: Receiver<IpcRequest>,
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn start(path: &Path) -> std::io::Result<IpcServer> {
    socket::prepare_dir(path)?;
    if path.exists() {
        // Is anyone listening? If not, it is an orphaned socket.
        if UnixStream::connect(path).is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                format!(
                    "an instance is already listening on {} (use --session for another one)",
                    path.display()
                ),
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    let (tx, rx) = channel::<IpcRequest>();
    thread::Builder::new()
        .name("codedraw-ipc".into())
        .spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { break };
                let tx = tx.clone();
                thread::spawn(move || {
                    let _ = handle(conn, tx);
                });
            }
        })?;
    Ok(IpcServer {
        path: path.to_path_buf(),
        rx,
    })
}

fn handle(stream: UnixStream, tx: Sender<IpcRequest>) -> std::io::Result<()> {
    let mut writer = stream.try_clone()?;
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(&line) {
            Err(e) => {
                let id = serde_json::from_str::<Value>(&line)
                    .ok()
                    .and_then(|v| v.get("id").cloned())
                    .unwrap_or(Value::Null);
                Response::err(id, "invalid_json", format!("invalid request: {e}"))
            }
            Ok(req) => {
                let (rtx, rrx) = channel();
                let id = req.id.clone();
                if tx.send(IpcRequest { req, reply: rtx }).is_err() {
                    Response::err(id, "unavailable", "the application is shutting down")
                } else {
                    rrx.recv().unwrap_or_else(|_| {
                        Response::err(id, "unavailable", "no response from the application")
                    })
                }
            }
        };
        writeln!(writer, "{}", serde_json::to_string(&resp)?)?;
        writer.flush()?;
    }
    Ok(())
}

/// Minimal client: one request, one response. Used by the CLI and the MCP bridge.
pub fn call(path: &Path, method: &str, params: Value) -> std::io::Result<Response> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    let req = Request {
        id: Value::from(1),
        method: method.to_string(),
        params,
    };
    writeln!(stream, "{}", serde_json::to_string(&req)?)?;
    stream.flush()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    serde_json::from_str(&line).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip_over_socket() {
        let dir = std::env::temp_dir().join(format!("codedraw-ipc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.sock");
        let server = start(&path).unwrap();
        let p2 = path.clone();
        let client = thread::spawn(move || call(&p2, "ping", json!({})).unwrap());
        let req = server.rx.recv().unwrap();
        assert_eq!(req.req.method, "ping");
        req.reply
            .send(Response::ok(req.req.id.clone(), json!({"type": "pong"})))
            .unwrap();
        let resp = client.join().unwrap();
        assert_eq!(resp.result.unwrap()["type"], "pong");
        drop(server);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
