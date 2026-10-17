//! Records the git commit the binary was built from, so `tiza --version`
//! and the MCP `status` tool can tell a stale build from a fresh one.

use std::process::Command;

fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short=10", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);
    println!(
        "cargo:rustc-env=TIZA_GIT_SHA={sha}{}",
        if dirty { "-dirty" } else { "" }
    );
    // Rebuild when HEAD moves (a new commit) or the index changes.
    for p in ["../../.git/HEAD", "../../.git/index"] {
        if std::path::Path::new(p).exists() {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    println!("cargo:rerun-if-env-changed=TIZA_GIT_SHA_OVERRIDE");
}
