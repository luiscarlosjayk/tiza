//! Local API server: Unix socket, one JSON request per line.
//! Each connection runs on its own thread and forwards requests to the TUI
//! thread over a channel; the response comes back over a one-shot channel.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use serde_json::Value;
use tiza_core::protocol::{Request, Response};

pub struct IpcRequest {
    pub req: Request,
    pub reply: Sender<Response>,
}

/// Keeps the socket alive and removes it on drop.
pub struct IpcServer {
    pub path: PathBuf,
    pub rx: Receiver<IpcRequest>,
    /// The device and inode of the socket that this server made: on drop,
    /// only that socket is removed, never a file that took its path since.
    made: (u64, u64),
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        let ours = std::fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.file_type().is_socket() && (m.dev(), m.ino()) == self.made);
        if ours {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub fn start(path: &Path) -> std::io::Result<IpcServer> {
    crate::runtime::prepare_parent(path)?;
    match std::fs::symlink_metadata(path) {
        // Is anyone listening? A refused connection is an orphaned socket;
        // any other error (another user's socket) leaves it there.
        Ok(meta) if meta.file_type().is_socket() => match UnixStream::connect(path) {
            Ok(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    format!(
                        "an instance is already listening on {} (use --session for another one)",
                        path.display()
                    ),
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                std::fs::remove_file(path)?;
            }
            Err(e) => {
                return Err(std::io::Error::new(
                    e.kind(),
                    format!("cannot check the socket {}: {e}", path.display()),
                ));
            }
        },
        // A file, a folder or a link that a mistyped `--socket` or a stray
        // TIZA_SOCKET names: never removed.
        Ok(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "{} is there and is not a socket: Tiza does not remove it. Give another socket path with --socket or TIZA_SOCKET",
                    path.display()
                ),
            ));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let listener = UnixListener::bind(path)?;
    let made = match std::fs::symlink_metadata(path) {
        Ok(m) => (m.dev(), m.ino()),
        Err(e) => {
            let _ = std::fs::remove_file(path);
            return Err(e);
        }
    };
    let (tx, rx) = channel::<IpcRequest>();
    // From here on, an error drops `server`, which removes the socket.
    let server = IpcServer {
        path: path.to_path_buf(),
        rx,
        made,
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!(
                "cannot make the socket {} private (0600): {e}",
                path.display()
            ),
        )
    })?;
    thread::Builder::new()
        .name("tiza-ipc".into())
        .spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { break };
                let tx = tx.clone();
                thread::spawn(move || {
                    let _ = handle(conn, tx);
                });
            }
        })?;
    Ok(server)
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
///
/// A connection that fails means that no instance listens at `path`. An
/// instance that does not answer in time gives a `TimedOut` error, and one
/// that closes the connection first gives `UnexpectedEof`:
/// [`unanswered`] says these two in words.
pub fn call(path: &Path, method: &str, params: Value) -> std::io::Result<Response> {
    use std::io::ErrorKind;
    let mut stream = UnixStream::connect(path)?;
    // Longer than the supervised-mode proposal timeout: a parked request is
    // answered (accepted, rejected or expired) before the client gives up.
    let wait = ANSWER_TIMEOUT;
    stream.set_read_timeout(Some(wait))?;
    let req = Request {
        id: Value::from(1),
        method: method.to_string(),
        params,
    };
    writeln!(stream, "{}", serde_json::to_string(&req)?)?;
    stream.flush()?;
    let mut line = String::new();
    match BufReader::new(stream).read_line(&mut line) {
        Ok(0) => {
            return Err(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "closed the connection before it answered",
            ));
        }
        Ok(_) => {}
        // The read timeout: EAGAIN on macOS and Linux.
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
            return Err(std::io::Error::new(
                ErrorKind::TimedOut,
                format!("did not answer within {} s", wait.as_secs()),
            ));
        }
        Err(e) => return Err(e),
    }
    serde_json::from_str(&line).map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))
}

/// How long [`call`] waits for the answer.
pub const ANSWER_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(crate::app::PROPOSAL_TIMEOUT.as_secs() + 15);

/// Why the instance at `path` gave no answer, in words, when a [`call`]
/// reached it: it did not answer in time, or it closed the connection.
/// `None` for another error, such as no instance at `path`.
pub fn unanswered(path: &Path, e: &std::io::Error) -> Option<String> {
    let why = match e.kind() {
        std::io::ErrorKind::TimedOut => format!(
            "did not answer within {} s: it is busy or stopped",
            ANSWER_TIMEOUT.as_secs()
        ),
        std::io::ErrorKind::UnexpectedEof => {
            "closed the connection before it answered: it possibly quit".to_string()
        }
        _ => return None,
    };
    Some(format!("the Tiza instance on {} {why}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_call_with_no_answer_says_why() {
        let p = Path::new("/run/t.sock");
        let late = std::io::Error::new(std::io::ErrorKind::TimedOut, "late");
        assert_eq!(
            unanswered(p, &late).as_deref(),
            Some(
                "the Tiza instance on /run/t.sock did not answer within 75 s: it is busy or stopped"
            )
        );
        let closed = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);
        assert!(
            unanswered(p, &closed)
                .unwrap()
                .contains("closed the connection")
        );
        // No instance there: the caller's own message.
        let none = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert_eq!(unanswered(p, &none), None);
    }

    #[test]
    fn roundtrip_over_socket() {
        let dir = std::env::temp_dir().join(format!("tiza-ipc-{}", std::process::id()));
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
