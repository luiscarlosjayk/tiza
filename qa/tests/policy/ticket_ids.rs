//! No ticket id (`t-` and three digits) in the repository, `qa/` too.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

// --- The files git tracks -----------------------------------------------

/// Git variables a parent process may have set: they would point these
/// commands at another repository.
const GIT_ENV: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
];

// ---------- No ticket ids ----------

/// Read as binary whatever their bytes: fonts and images. Any other file
/// that is not UTF-8 is skipped too.
const BINARY_EXT: [&str; 9] = [
    "ttf", "otf", "woff", "woff2", "png", "gif", "jpg", "jpeg", "ico",
];

/// The repository's files, relative to its root, but `qa/.gitleaksignore`
/// (its entries are paths of old commits, as gitleaks reports them): the ones
/// git tracks (`git ls-files`; untracked files are left out, so a local
/// notes folder in the checkout does not fail the gate), else a walk of the
/// tree without `target/` and `.git`.
fn repository_files() -> Vec<String> {
    let root = root();
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
    files.retain(|f| f != "qa/.gitleaksignore");
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

/// Where `\bt-\d{3}\b` matches in `line` (byte offsets): `t-` and exactly
/// three digits, standing as a word of their own.
fn ticket_ids(line: &str) -> Vec<usize> {
    let b = line.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(k) = line[from..].find("t-") {
        let at = from + k;
        let digits = b
            .get(at + 2..at + 5)
            .is_some_and(|d| d.iter().all(u8::is_ascii_digit));
        let before = at == 0 || !word(b[at - 1]);
        let after = b.get(at + 5).is_none_or(|&c| !word(c));
        if digits && before && after {
            out.push(at);
        }
        from = at + 2;
    }
    out
}

/// No file names a ticket: each one found is listed as file:line with the
/// rule from AGENTS.md.
#[test]
fn ids_none_in_the_repository() {
    let rule = read("AGENTS.md")
        .lines()
        .find(|l| l.starts_with("- No ticket ids"))
        .map(str::to_string)
        .expect("AGENTS.md states the rule on ticket ids");
    let files = repository_files();
    assert!(
        files.iter().any(|f| f == "README.md") && files.iter().any(|f| f.ends_with(".rs")),
        "the listing missed the README or the sources: {files:?}"
    );
    let mut found = Vec::new();
    for f in &files {
        let ext = Path::new(f)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        if ext.is_some_and(|e| BINARY_EXT.contains(&e.as_str())) {
            continue;
        }
        let Ok(bytes) = std::fs::read(root().join(f)) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            if !ticket_ids(line).is_empty() {
                found.push(format!("{f}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} line(s) name a ticket:\n{}\n\nThe rule, from AGENTS.md:\n{rule}",
        found.len(),
        found.join("\n")
    );
}
