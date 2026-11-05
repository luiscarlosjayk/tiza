//! Helpers of the tests that talk to the socket of a live app.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::Live;

/// `method` through the request handler of the app, as the socket gives it,
/// at the injected clock `at`, so the 1 s grouping is tested without sleeping.
pub fn ipc_at(live: &Live, method: &str, params: Value, at: Instant) -> Value {
    let method = method.to_string();
    live.with(move |a| {
        let (tx, rx) = mpsc::channel();
        let req = tiza_core::protocol::Request {
            id: json!(1),
            method,
            params,
        };
        a.handle_ipc_at(req, tx, at);
        let resp = rx.recv().unwrap();
        resp.result.unwrap_or_else(|| panic!("{:?}", resp.error))
    })
}

/// Sends `method` on another thread; join the handle for the response.
pub fn send_async(
    socket: &std::path::Path,
    method: &str,
    params: Value,
) -> thread::JoinHandle<tiza_core::protocol::Response> {
    let socket = socket.to_path_buf();
    let method = method.to_string();
    thread::spawn(move || tiza_tui::ipc::call(&socket, &method, params).expect("socket"))
}

/// The socket path of the live app.
pub fn sock(live: &Live) -> &str {
    live.socket.to_str().unwrap()
}

/// Waits (up to 10 s) until the app holds `n` proposals.
pub fn wait_pending(live: &Live, n: usize) {
    let start = std::time::Instant::now();
    while live.with(|a| a.proposals.len()) < n {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "waiting for {n} proposals"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
