//! No ticket id (`t-` and three digits) in the repository, `qa/` too.

use std::path::{Path, PathBuf};

use tiza_qa::repository_files;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

// ---------- No ticket ids ----------

/// Read as binary whatever their bytes: fonts and images. Any other file
/// that is not UTF-8 is skipped too.
const BINARY_EXT: [&str; 9] = [
    "ttf", "otf", "woff", "woff2", "png", "gif", "jpg", "jpeg", "ico",
];

/// The files to read: the repository's files but `qa/.gitleaksignore`
/// (its entries are paths of old commits, as gitleaks reports them).
fn files_to_read() -> Vec<String> {
    let mut files = repository_files();
    files.retain(|f| f != "qa/.gitleaksignore");
    files
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
    let files = files_to_read();
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
