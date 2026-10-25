//! t-082 — MCP tool list after a rebuild: build identity and the reconnect hint.
//!
//! Acceptance criteria:
//! - AC1 `tiza --version` prints `<semver> (<git sha>)`; the sha is also
//!   `status.build` on the socket (same value as `tiza_core::GIT_SHA`).
//! - AC2 The MCP `status` tool adds `bridge {version, build}`; when the binary
//!   on disk is newer than the running server it adds `binary_rebuilt: true`
//!   and a `hint` that names the reconnect; when the TUI reports another build
//!   it adds `instance_build`.
//! - AC3 README and docs/agent-guide.md document the reconnect (`/mcp`) and
//!   why `tools/list_changed` is not a substitute.

use serde_json::json;
use tiza_qa::{Live, McpClient, tiza_ok};

#[test]
fn ac1_version_and_status_carry_the_build() {
    let v = tiza_ok(&["--version"]);
    let sha = tiza_core::GIT_SHA;
    assert!(
        v.trim() == format!("tiza {} ({sha})", tiza_core::VERSION),
        "{v}"
    );
    assert!(sha == "unknown" || sha.len() >= 7, "{sha}");
    let live = Live::from_code("fn a() {}\n", "rs", "t082-status");
    let st = live.api_ok("status", json!({}));
    assert_eq!(st["build"], json!(sha));
}

#[test]
fn ac2_mcp_status_reports_the_bridge_and_a_rebuild() {
    let live = Live::from_code("fn a() {}\n", "rs", "t082-mcp");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("status", json!({}));
    assert!(!err, "{text}");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["bridge"]["version"], json!(tiza_core::VERSION));
    assert_eq!(v["bridge"]["build"], json!(tiza_core::GIT_SHA));
    assert!(v["bridge"].get("binary_rebuilt").is_none(), "{text}");
    assert!(v["bridge"].get("hint").is_none(), "{text}");

    // The server-side logic, without touching the real binary: a server whose
    // recorded mtime is older than the file on disk reports the rebuild.
    let server = tiza_mcp::TizaServer::new(live.socket.clone());
    assert!(!server.binary_rebuilt());
    let stale =
        tiza_mcp::TizaServer::with_exe_mtime(live.socket.clone(), Some(std::time::UNIX_EPOCH));
    assert!(stale.binary_rebuilt());
    let bridge = stale.bridge_status(&json!({"build": tiza_core::GIT_SHA}));
    assert_eq!(bridge["binary_rebuilt"], json!(true));
    assert!(
        bridge["hint"].as_str().unwrap().contains("/mcp"),
        "{bridge}"
    );
    // A TUI from another commit: instance_build is reported even without a rebuild.
    let bridge = server.bridge_status(&json!({"build": "0000000000"}));
    assert_eq!(bridge["instance_build"], json!("0000000000"));
    assert!(bridge["hint"].as_str().unwrap().contains("restart the TUI"));
    assert!(bridge.get("binary_rebuilt").is_none());
}
