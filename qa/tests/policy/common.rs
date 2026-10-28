//! Helpers for more than one module of the policy area.

use std::path::Path;
use std::process::Command;

use tiza_qa::repo_root;

/// Git variables a parent process may have set: they would point these
/// commands at another repository.
const GIT_ENV: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
];

/// The repository's files, relative to its root: the ones git tracks
/// (`git ls-files`; untracked files are left out, so a local notes folder
/// in the checkout does not fail the gate), else a walk of the tree without
/// `target/` and `.git`.
pub fn repository_files() -> Vec<String> {
    let root = repo_root();
    let mut cmd = Command::new("git");
    for v in GIT_ENV {
        cmd.env_remove(v);
    }
    let listed = cmd
        .args(["ls-files", "-z", "--cached"])
        .current_dir(&root)
        .output()
        .ok()
        .filter(|o| o.status.success());
    let mut files: Vec<String> = match listed {
        Some(o) => String::from_utf8_lossy(&o.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        None => {
            let mut v = Vec::new();
            walk(&root, &root, &mut v);
            v
        }
    };
    files.sort();
    files.dedup();
    files
}

/// Every regular file under `dir`, relative to `root`, with `/` separators;
/// `target` and `.git` (a directory, or a worktree's file) are left out.
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        if name == ".git" || name == "target" {
            continue;
        }
        let Ok(ft) = e.file_type() else {
            continue;
        };
        let path = e.path();
        if ft.is_dir() {
            walk(root, &path, out);
        } else if ft.is_file()
            && let Ok(rel) = path.strip_prefix(root)
        {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}
