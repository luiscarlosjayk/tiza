//! Helpers of the tests that read the files of the repository.

use std::process::{Command, Output};

use crate::repo_root;

/// The files `cargo package` would put in the crate.
pub fn packaged(name: &str) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args([
            "package",
            "--list",
            "--offline",
            "--allow-dirty",
            "-p",
            name,
        ])
        .current_dir(repo_root())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// The stdout and the stderr of `o`.
pub fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}
