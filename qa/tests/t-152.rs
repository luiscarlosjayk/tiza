//! t-152 (part A) — Before the repository goes public: the workflows build
//! with the pinned toolchain, CONTRIBUTING.md says how to
//! publish to crates.io by hand, the crates.io pages say what each crate is, and a crate unpacked
//! from crates.io does not take the commit of a git repository it happens to
//! sit in.
//!
//! Acceptance criteria (part A of the card, plus its notes):
//! - Toolchain (note of 2026-10-19): every job of `ci.yml` and `release.yml`
//!   that runs cargo installs the toolchain `rust-toolchain.toml` pins
//!   (1.99.1, the only place its version is written) before the first cargo
//!   command, and the release build adds its target to that toolchain, not
//!   to `stable`.
//! - Publication (changed by Luis on 2026-10-22): no workflow publishes to
//!   crates.io and no secret is needed; CONTRIBUTING.md
//!   (`## Releases (maintainers)`, the section release.yml names) gives every
//!   manual step in order (version with
//!   `scripts/bump-version.sh`, commit, workspace dry run before the tag,
//!   tag, then one crate at a time in dependency order, checking the index
//!   before the next). Every `.crate` carries the licence texts; `tiza`
//!   leaves out its tests (QA round 0).
//! - AC2 crates.io page of `tiza`: a readme packaged in the crate with no
//!   relative links or images, `homepage`, `documentation` on GitHub (not
//!   docs.rs), at most five keywords and valid categories.
//! - AC3 `tiza-core`, `tiza-tui`, `tiza-mcp` and `tiza-export` say they are
//!   internal crates published only so that `cargo install tiza` works, with
//!   no stable API: first line of the lib.rs `//!`, the `description` and a
//!   short packaged README with the link to the repository.
//! - Build stamp (QA note 1, 2026-10-18): a crate unpacked from crates.io
//!   under a directory that is a git repository (a home kept in git) is
//!   stamped `unknown`, not with that repository's commit; a checkout of Tiza
//!   keeps its commit; `TIZA_GIT_SHA_OVERRIDE` still wins; the first commit
//!   of a repository made around the crate stamps it (QA round 0).
//!
//! Part B (Luis, 2026-10-22): the board is private, so no ticket id (`t-`
//! and three digits) is left outside `qa/` and commit messages. AGENTS.md
//! says so, and `ids_none_outside_qa` fails with file:line on any other,
//! save a quoted qa test target in `scripts/bench.py`.
//!
//! The workflows cannot run here (Actions is off while the repository is
//! private): their structure is read as text (and parsed as YAML).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const REPO: &str = "https://github.com/luiscarlosjayk/tiza";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

// --- crates.io pages -----------------------------------------------------

/// The files `cargo package` would put in the crate.
fn packaged(name: &str) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args([
            "package",
            "--list",
            "--offline",
            "--allow-dirty",
            "-p",
            name,
        ])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// QA P2-2: every crate carries the licence texts (links in each crate
/// directory to the repository's, packaged as files), and QA P3-6: `tiza`
/// leaves out its `tests/`, which read `../../examples/`.
#[test]
fn packages_carry_the_licences_and_tiza_not_its_tests() {
    for (name, dir) in [
        ("tiza", "crates/tiza-cli"),
        ("tiza-core", "crates/tiza-core"),
        ("tiza-export", "crates/tiza-export"),
        ("tiza-tui", "crates/tiza-tui"),
        ("tiza-mcp", "crates/tiza-mcp"),
    ] {
        let files = packaged(name);
        for licence in ["LICENSE-MIT", "LICENSE-APACHE"] {
            assert!(files.iter().any(|f| f == licence), "{name}: {licence}");
            assert_eq!(
                read(&format!("{dir}/{licence}")),
                read(licence),
                "{name}: the repository's {licence}"
            );
        }
        // The fonts that the binary embeds go with their licenses.
        if name == "tiza-export" {
            for licence in [
                "assets/fonts/LICENSE-JetBrainsMono-OFL.txt",
                "assets/fonts/LICENSE-TizaSymbols-DejaVu.txt",
            ] {
                assert!(files.iter().any(|f| f == licence), "{name}: {licence}");
            }
        }
        if name == "tiza" {
            assert!(
                !files.iter().any(|f| f.starts_with("tests/")),
                "tiza packages {files:?}"
            );
            assert!(files.iter().any(|f| f == "src/main.rs"));
        }
    }
}

// --- Build stamp ---------------------------------------------------------

/// Git variables a parent process may have set: they would point these
/// commands at another repository.
const GIT_ENV: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
];

// ---------- part B: no ticket ids outside qa/ ----------

/// Read as binary whatever their bytes: fonts and images. Any other file
/// that is not UTF-8 is skipped too.
const BINARY_EXT: [&str; 9] = [
    "ttf", "otf", "woff", "woff2", "png", "gif", "jpg", "jpeg", "ico",
];

/// The repository's files outside `qa/`, relative to its root: the ones
/// git tracks (`git ls-files`; untracked files are left out, so a local
/// notes folder in the checkout does not fail the gate), else a walk of the
/// tree without `target/` and `.git`.
fn files_outside_qa() -> Vec<String> {
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
    files.retain(|f| !f.starts_with("qa/"));
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

/// The one exception: `scripts/bench.py` runs qa tests by name
/// (`cargo test --test "t-NNN"`), and such a quoted name is allowed while
/// `qa/tests/t-NNN.rs` exists. Anywhere else, unquoted, or naming no qa
/// test, it is an id like any other.
fn allowed(file: &str, line: &str, at: usize) -> bool {
    file == "scripts/bench.py"
        && line[..at].ends_with('"')
        && line[at + 5..].starts_with('"')
        && root()
            .join("qa/tests")
            .join(format!("{}.rs", &line[at..at + 5]))
            .is_file()
}

/// No file outside `qa/` names a ticket: each one found is listed as
/// file:line with the rule from AGENTS.md.
#[test]
fn ids_none_outside_qa() {
    let rule = read("AGENTS.md")
        .lines()
        .find(|l| l.starts_with("- No ticket ids"))
        .map(str::to_string)
        .expect("AGENTS.md states the rule on ticket ids");
    let files = files_outside_qa();
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
            if ticket_ids(line).into_iter().any(|at| !allowed(f, line, at)) {
                found.push(format!("{f}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} line(s) outside qa/ name a ticket:\n{}\n\nThe rule, from AGENTS.md:\n{rule}",
        found.len(),
        found.join("\n")
    );
}

/// The matcher is `\bt-\d{3}\b`, and the bench.py exception is as narrow as
/// it says.
#[test]
fn ids_matcher_and_bench_exception() {
    assert_eq!(ticket_ids("(t-123) and t-456."), vec![1, 12]);
    assert_eq!(
        ticket_ids("t-12, t-1234, at-123, t-123b, t_123"),
        Vec::<usize>::new()
    );
    assert_eq!(ticket_ids("// t-123"), vec![3]);
    let line = r#"ms = frame_median("t-152", "ac6")"#;
    let at = ticket_ids(line)[0];
    assert!(allowed("scripts/bench.py", line, at));
    assert!(!allowed("scripts/other.py", line, at));
    let bare = "# see t-152";
    assert!(!allowed("scripts/bench.py", bare, ticket_ids(bare)[0]));
    let none = r#"frame_median("t-999")"#;
    assert!(!root().join("qa/tests/t-999.rs").exists());
    assert!(!allowed("scripts/bench.py", none, ticket_ids(none)[0]));
}

// --- How to help, and security (AC7) ----------------------------------

/// A YAML file of the repository as JSON, through PyYAML or the Ruby of
/// the system (macOS and the GitHub runners have it); `None` when neither
/// is there.
fn yaml(rel: &str) -> Option<serde_json::Value> {
    let path = root().join(rel);
    let tries: [(&str, &[&str]); 2] = [
        (
            "python3",
            &[
                "-c",
                "import sys, json, yaml; print(json.dumps(yaml.safe_load(open(sys.argv[1]))))",
            ],
        ),
        (
            "ruby",
            &[
                "-ryaml",
                "-rjson",
                "-e",
                "puts JSON.generate(YAML.safe_load(File.read(ARGV[0])))",
            ],
        ),
    ];
    for (bin, args) in tries {
        let Ok(out) = Command::new(bin).args(args).arg(&path).output() else {
            continue;
        };
        if out.status.success() {
            return Some(serde_json::from_slice(&out.stdout).unwrap());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        // A parser that is there and refuses the file is a failure.
        assert!(
            err.contains("No module named") || err.contains("cannot load such file"),
            "{rel} is not valid YAML ({bin}): {err}"
        );
    }
    None
}

#[test]
fn ac7_the_issue_and_discussion_forms_are_valid_and_complete() {
    let files = [
        ".github/ISSUE_TEMPLATE/bug.yml",
        ".github/ISSUE_TEMPLATE/config.yml",
        ".github/DISCUSSION_TEMPLATE/ideas.yml",
        ".github/DISCUSSION_TEMPLATE/feedback.yml",
    ];
    for f in files {
        assert!(root().join(f).is_file(), "{f} is missing");
    }
    // As text, always: no blank issues, and the ways out.
    let config = read(".github/ISSUE_TEMPLATE/config.yml");
    assert!(config.contains("blank_issues_enabled: false"));
    assert!(config.contains(&format!("{REPO}/discussions")));
    assert!(config.contains(&format!("{REPO}/security/advisories/new")));
    let Some(bug) = yaml(files[0]) else {
        eprintln!("t-152: no PyYAML and no Ruby, the forms are not parsed");
        return;
    };
    let ids = |form: &serde_json::Value| -> Vec<String> {
        form["body"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|b| b["id"].as_str().map(str::to_string))
            .collect()
    };
    assert_eq!(
        ids(&bug),
        [
            "confirm",
            "current-behavior",
            "expected-behavior",
            "reproduction",
            "effect",
            "version",
            "os",
            "terminal",
            "panics-log"
        ]
    );
    let body = bug["body"].as_array().unwrap();
    let kinds = ["markdown", "checkboxes", "textarea", "input"];
    for b in body {
        assert!(kinds.contains(&b["type"].as_str().unwrap()), "{b}");
        if b["type"] != "markdown" {
            assert!(b["attributes"]["label"].is_string(), "{b}");
        }
    }
    // Both boxes are required; everything but panics.log too.
    let boxes = body.iter().find(|b| b["id"] == "confirm").unwrap();
    let options = boxes["attributes"]["options"].as_array().unwrap();
    assert_eq!(options.len(), 2);
    assert!(options.iter().all(|o| o["required"] == true));
    for b in body.iter().filter(|b| b["type"] != "markdown") {
        let id = b["id"].as_str().unwrap();
        if id != "confirm" {
            assert_eq!(b["validations"]["required"], id != "panics-log", "{id}");
        }
    }
    let intro = body[0]["attributes"]["value"].as_str().unwrap();
    assert!(intro.contains("not for ideas, feature requests, feedback or questions"));
    let config = yaml(files[1]).unwrap();
    assert_eq!(config["blank_issues_enabled"], false);
    for link in config["contact_links"].as_array().unwrap() {
        assert!(link["url"].as_str().unwrap().starts_with(REPO), "{link}");
        assert!(
            link["name"].is_string() && link["about"].is_string(),
            "{link}"
        );
    }
    for (f, want) in [
        (files[2], ["problem", "change", "why"]),
        (files[3], ["use", "feedback", "version"]),
    ] {
        let form = yaml(f).unwrap();
        assert!(form["title"].is_string(), "{f}");
        assert_eq!(ids(&form), want, "{f}");
    }
}

// --- Brand assets (Luis, 2026-10-24) -----------------------------------
