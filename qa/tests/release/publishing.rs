//! Before the repository goes public: the workflows build
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
//!   before the next). That every `.crate` carries the licence texts and
//!   `tiza` leaves out its tests is checked in `policy::package_licences`.
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
//! The workflows cannot run here (Actions is off while the repository is
//! private): their structure is read as text (and parsed as YAML).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tiza_qa::scratch;

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

/// The jobs of a workflow: name and the lines of its body, in order.
fn jobs(yml: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut in_jobs = false;
    for line in yml.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if !line.starts_with(' ') && !line.trim().is_empty() {
            break;
        }
        let is_header = line.starts_with("  ")
            && !line.starts_with("   ")
            && !line.trim_start().starts_with('#')
            && line.trim_end().ends_with(':');
        if is_header {
            out.push((line.trim().trim_end_matches(':').to_string(), Vec::new()));
        } else if let Some((_, body)) = out.last_mut() {
            body.push(line.to_string());
        }
    }
    out
}

fn job<'a>(jobs: &'a [(String, Vec<String>)], name: &str) -> &'a [String] {
    &jobs
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no job `{name}`"))
        .1
}

/// Index of the first line of `body` that runs cargo (a `run:` line or a
/// script line; never a comment, a step name or text a script prints).
fn first_cargo(body: &[String]) -> Option<usize> {
    body.iter().position(|l| {
        let t = l.trim_start();
        !t.starts_with('#')
            && !t.starts_with("echo ")
            && !t.starts_with("name:")
            && !t.starts_with("- name:")
            && (t.contains("cargo ") || t.contains("Swatinem/rust-cache"))
    })
}

fn position(body: &[String], needle: &str) -> Option<usize> {
    body.iter()
        .position(|l| !l.trim_start().starts_with('#') && l.contains(needle))
}

#[test]
fn toolchain_rust_toolchain_toml_is_the_only_version() {
    let toml = read("rust-toolchain.toml");
    assert!(
        toml.contains("channel = \"1.99.1\""),
        "rust-toolchain.toml pins 1.99.1"
    );
    // The jobs that run cargo (`schema` runs only Python, cargo-deny-action
    // installs the toolchain itself, the release job only downloads the
    // archives).
    let expected: [(&str, &[&str]); 2] = [
        (".github/workflows/ci.yml", &["check", "legibility"]),
        (".github/workflows/release.yml", &["build"]),
    ];
    for (wf, cargo_jobs) in expected {
        let yml = read(wf);
        for line in yml.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
            for stale in ["dtolnay/rust-toolchain", "@stable", "1.99", "rust-version:"] {
                assert!(
                    !line.contains(stale),
                    "{wf} names `{stale}`: the toolchain comes from rust-toolchain.toml only"
                );
            }
            assert!(!line.starts_with("toolchain:"), "{wf}: {line}");
        }
        let mut with_cargo = Vec::new();
        for (name, body) in jobs(&yml) {
            let Some(cargo) = first_cargo(&body) else {
                continue;
            };
            with_cargo.push(name.clone());
            let install = position(&body, "rustup toolchain install").unwrap_or_else(|| {
                panic!("{wf} job `{name}` runs cargo without `rustup toolchain install`")
            });
            assert!(
                install < cargo,
                "{wf} job `{name}`: the toolchain is installed after the first cargo command"
            );
        }
        assert_eq!(with_cargo, cargo_jobs, "{wf}: the jobs that run cargo");
    }
    // CI runs deny.toml (licenses, the network policy, duplicates) with
    // cargo-deny-action, which installs the toolchain of
    // rust-toolchain.toml itself (moved here from
    // toolchain_cargo_deny_reads_rust_toolchain_toml).
    let yml = read(".github/workflows/ci.yml");
    let jobs = jobs(&yml);
    let deny = job(&jobs, "deny").join("\n");
    assert!(deny.contains("EmbarkStudios/cargo-deny-action@v2"));
}

#[test]
fn toolchain_release_targets_go_to_the_pinned_toolchain() {
    let yml = read(".github/workflows/release.yml");
    let jobs = jobs(&yml);
    let build = job(&jobs, "build");
    let install = position(build, "rustup toolchain install").unwrap();
    let target = position(build, "rustup target add ${{ matrix.target }}")
        .expect("the build job adds its target with rustup");
    let cargo = position(build, "cargo build --release --locked --target").unwrap();
    assert!(
        install < target && target < cargo,
        "install → target → build"
    );
    assert!(yml.contains("target: x86_64-apple-darwin"));
}

/// The workflows are valid YAML (when python3 has PyYAML; CI's runners do).
#[test]
fn toolchain_workflows_parse_as_yaml() {
    let have = Command::new("python3")
        .args(["-c", "import yaml"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !have {
        eprintln!("publishing: python3 with PyYAML not found, YAML syntax not checked");
        return;
    }
    for wf in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let out = Command::new("python3")
            .args([
                "-c",
                "import sys, yaml; d = yaml.safe_load(open(sys.argv[1])); print(' '.join(d['jobs']))",
            ])
            .arg(root().join(wf))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{wf}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let names = String::from_utf8_lossy(&out.stdout);
        let ours: Vec<String> = jobs(&read(wf)).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names.split_whitespace().collect::<Vec<_>>(), ours, "{wf}");
    }
}

// --- Publication (by hand) ---------------------------------------------

/// Luis's decision (2026-10-22): the first publication on crates.io is made
/// by hand from his Mac, so no workflow publishes and none reads a secret.
#[test]
fn publish_no_workflow_publishes_to_crates_io() {
    for wf in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let yml = read(wf);
        let code: Vec<&str> = yml
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect();
        for gone in ["cargo publish", "secrets."] {
            assert!(
                !code.iter().any(|l| l.contains(gone)),
                "{wf} still has `{gone}`"
            );
        }
        assert!(
            jobs(&yml).iter().all(|(name, _)| name != "publish"),
            "{wf} has a `publish` job"
        );
    }
}

/// The heading of the release page, as GitHub anchors it: lowercase,
/// letters, digits, `-` and `_` kept, spaces to `-`, the rest dropped.
fn anchor(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// The release section of CONTRIBUTING.md, the section release.yml
/// names: from `## Releases (maintainers)` to the next H2.
fn release_section() -> String {
    let page = read("CONTRIBUTING.md");
    let start = page
        .find("\n## Releases (maintainers)\n")
        .expect("CONTRIBUTING.md has `## Releases (maintainers)`")
        + 1;
    let rest = &page[start..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 4);
    rest[..end].to_string()
}

/// QA P3-4: a real section, with the anchor release.yml points at.
#[test]
fn publish_release_section_has_an_anchor_release_yml_uses() {
    let section = release_section();
    let heading = section.lines().next().unwrap().trim_start_matches("## ");
    assert_eq!(anchor(heading), "releases-maintainers");
    let yml = read(".github/workflows/release.yml");
    assert!(
        yml.contains("CONTRIBUTING.md#releases-maintainers"),
        "release.yml points at the section"
    );
    // QA P3-5: no blank line at the end of the workflow.
    assert!(yml.ends_with('\n') && !yml.ends_with("\n\n"));
}

/// QA P2-1: the section gives every step, in order: the version (script,
/// commit), the dry run before the tag (pushing the tag starts the
/// release), the tag, then one crate at a time in dependency order, and how
/// to check the index before the next one.
#[test]
fn publish_release_page_has_the_manual_steps_in_order() {
    let section = release_section();
    let at = |needle: &str| {
        section
            .find(needle)
            .unwrap_or_else(|| panic!("the section lacks `{needle}`"))
    };
    let steps = [
        "git status --short",
        "scripts/bump-version.sh X.Y.Z",
        "$EDITOR docs/release-notes/vX.Y.Z.md",
        "git add docs/release-notes/vX.Y.Z.md",
        "git commit -am \"vX.Y.Z\"",
        "cargo publish --workspace --dry-run --locked",
        "git push origin main",
        "git tag vX.Y.Z && git push origin vX.Y.Z",
        "cargo login",
        "cargo publish --locked -p tiza-core\n",
        "cargo publish --locked -p tiza-export\n",
        "cargo publish --locked -p tiza-tui\n",
        "cargo publish --locked -p tiza-mcp\n",
        "cargo publish --locked -p tiza\n",
    ];
    let positions: Vec<usize> = steps.iter().map(|s| at(s)).collect();
    for (w, pair) in positions.windows(2).zip(steps.windows(2)) {
        assert!(w[0] < w[1], "`{}` comes after `{}`", pair[0], pair[1]);
    }
    assert!(at("https://index.crates.io/ti/za/tiza-core") > positions[13]);
    assert!(read("scripts/bump-version.sh").starts_with("#!/usr/bin/env bash\n"));
    // The order is the dependency order of the manifests.
    let meta = metadata();
    let deps = |name: &str| -> Vec<String> {
        package(&meta, name)["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap().to_string())
            .filter(|d| d.starts_with("tiza"))
            .collect()
    };
    let order = ["tiza-core", "tiza-export", "tiza-tui", "tiza-mcp", "tiza"];
    for (i, name) in order.iter().enumerate() {
        for dep in deps(name) {
            let j = order.iter().position(|o| *o == dep).unwrap();
            assert!(j < i, "{name} depends on {dep}, published after it");
        }
    }
    // Every crate of the order exists and nothing publishable is left out.
    let publishable: Vec<String> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["publish"] != serde_json::json!([]))
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    let mut sorted = publishable.clone();
    sorted.sort();
    let mut expected: Vec<String> = order.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(sorted, expected);
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

/// A copy of the workspace (manifests, lock, sources, CHANGELOG and the
/// script) in a scratch directory.
fn workspace_copy(name: &str) -> PathBuf {
    let dir = scratch(name).join("ws");
    for f in [
        "Cargo.toml",
        "Cargo.lock",
        "CHANGELOG.md",
        "README.md",
        "rust-toolchain.toml",
    ] {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(root().join(f), dir.join(f)).unwrap();
    }
    for d in ["crates", "qa/src", "site/checks/src", "scripts"] {
        copy_dir(&root().join(d), &dir.join(d));
    }
    // The members without a crates/ folder: their manifests, and no tests.
    for m in ["qa", "site/checks"] {
        std::fs::create_dir_all(dir.join(m).join("tests")).unwrap();
        let manifest = format!("{m}/Cargo.toml");
        std::fs::copy(root().join(&manifest), dir.join(&manifest)).unwrap();
    }
    dir
}

/// `scripts/bump-version.sh ARGS` in `dir`, offline, with this toolchain's
/// cargo first on PATH.
fn bump(dir: &Path, args: &[&str]) -> Output {
    let cargo_dir = Path::new(env!("CARGO")).parent().unwrap().to_path_buf();
    let path = format!(
        "{}:{}",
        cargo_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Command::new("bash")
        .arg(dir.join("scripts/bump-version.sh"))
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap()
}

fn versions(dir: &Path) -> Vec<(String, String)> {
    let out = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--no-deps",
            "--locked",
            "--offline",
            "--format-version",
            "1",
        ])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut v: Vec<(String, String)> = meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap().to_string(),
                p["version"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    v.sort();
    v
}

/// QA P2-1: the script moves the five versions, the lock and the CHANGELOG
/// together, so `--locked` passes right after it; it refuses a bad version
/// and a second run, and then writes nothing.
#[test]
fn publish_bump_version_moves_versions_lock_and_changelog() {
    let dir = workspace_copy("bump");
    let before = read("CHANGELOG.md");
    let out = bump(&dir, &["9.8.7"]);
    assert!(out.status.success(), "{}", text(&out));
    for (name, version) in versions(&dir) {
        assert_eq!(version, "9.8.7", "{name}");
    }
    let toml = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert_eq!(toml.matches("version = \"9.8.7\"").count(), 5, "{toml}");
    let changelog = std::fs::read_to_string(dir.join("CHANGELOG.md")).unwrap();
    let (head, rest) = before.split_once("## [Unreleased]\n").unwrap();
    assert!(changelog.starts_with(&format!("{head}## [Unreleased]\n\n## [9.8.7] - ")));
    let after_heading = changelog
        .split_once("## [9.8.7] - ")
        .unwrap()
        .1
        .split_once('\n')
        .unwrap();
    let date = after_heading.0;
    assert!(
        date.len() == 10 && date.chars().filter(|c| *c == '-').count() == 2,
        "{date}"
    );
    assert_eq!(after_heading.1, rest, "the entries move under the version");

    // The README's status line moves with the version.
    let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
    assert!(
        readme.contains("\n> Status: **v9.8.7**"),
        "the README status"
    );
    let snapshot = |d: &Path| {
        ["Cargo.toml", "Cargo.lock", "CHANGELOG.md", "README.md"]
            .map(|f| std::fs::read_to_string(d.join(f)).unwrap())
    };
    let done = snapshot(&dir);
    let again = bump(&dir, &["9.8.7"]);
    assert_eq!(again.status.code(), Some(1), "{}", text(&again));
    for bad in ["v9.9.9", "9.9", "9.9.9-rc.1"] {
        let out = bump(&dir, &[bad]);
        assert_eq!(out.status.code(), Some(2), "{bad}: {}", text(&out));
    }
    assert_eq!(snapshot(&dir), done, "a refused run writes nothing");
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

// --- crates.io pages -----------------------------------------------------

fn metadata() -> serde_json::Value {
    let out = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    serde_json::from_slice(&out.stdout).unwrap()
}

/// The `readme` of a package, as a path (cargo metadata gives it relative
/// to the package's manifest).
fn readme_of(p: &serde_json::Value) -> PathBuf {
    let manifest = PathBuf::from(p["manifest_path"].as_str().unwrap());
    let readme = p["readme"].as_str().expect("readme");
    manifest.parent().unwrap().join(readme)
}

fn package<'a>(meta: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    meta["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no package {name}"))
}

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

/// Every link and image target of a Markdown text: `](target)`.
fn link_targets(md: &str) -> Vec<String> {
    md.split("](")
        .skip(1)
        .map(|rest| rest.split(')').next().unwrap_or("").to_string())
        .collect()
}

/// Slugs that exist on crates.io (checked against its API on 2026-10-22).
const CATEGORIES: &[&str] = &[
    "command-line-utilities",
    "development-tools",
    "visualization",
];

#[test]
fn tiza_crates_io_metadata() {
    let meta = metadata();
    let tiza = package(&meta, "tiza");
    let homepage = tiza["homepage"].as_str().expect("homepage");
    let docs = tiza["documentation"].as_str().expect("documentation");
    assert!(homepage.starts_with(REPO), "{homepage}");
    assert!(
        docs.starts_with(REPO) && !docs.contains("docs.rs"),
        "{docs}"
    );
    let keywords: Vec<&str> = tiza["keywords"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert!((1..=5).contains(&keywords.len()), "{keywords:?}");
    for k in &keywords {
        // crates.io: ASCII letters, digits, `_`, `-`, `+`, starting with a letter, ≤ 20.
        assert!(k.len() <= 20 && k.starts_with(|c: char| c.is_ascii_alphabetic()));
        assert!(
            k.chars()
                .all(|c| c.is_ascii_alphanumeric() || "_-+".contains(c))
        );
    }
    let categories: Vec<&str> = tiza["categories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert!(!categories.is_empty() && categories.len() <= 5);
    for c in &categories {
        assert!(
            CATEGORIES.contains(c),
            "{c} is not a crates.io category we checked"
        );
    }
}

#[test]
fn tiza_readme_is_packaged_and_works_on_crates_io() {
    let meta = metadata();
    let readme = readme_of(package(&meta, "tiza"));
    assert!(readme.is_file(), "{}", readme.display());
    assert!(packaged("tiza").iter().any(|f| f == "README.md"));
    let md = std::fs::read_to_string(&readme).unwrap();
    assert!(md.contains("cargo install tiza"));
    assert!(
        md.contains(&format!("{REPO}#readme")),
        "links to the full README"
    );
    let targets = link_targets(&md);
    assert!(
        targets
            .iter()
            .any(|t| t.ends_with(".png") || t.ends_with(".gif"))
    );
    for t in targets {
        assert!(
            t.starts_with("https://") || t.starts_with('#'),
            "relative link `{t}` does not work on crates.io"
        );
    }
}

// --- Build stamp ---------------------------------------------------------

fn build_rs() -> String {
    read("crates/tiza-core/build.rs")
}

/// A tiny crate whose build script is a copy of tiza-core's and whose binary
/// prints `TIZA_GIT_SHA` (as in the version stamp tests), written into `dir`.
fn write_crate(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"stamp\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(dir.join("build.rs"), build_rs()).unwrap();
    std::fs::write(
        dir.join("src/main.rs"),
        "fn main() {\n    println!(\"{}\", env!(\"TIZA_GIT_SHA\"));\n}\n",
    )
    .unwrap();
}

/// Git variables a parent process may have set: they would point these
/// commands at another repository.
const GIT_ENV: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
];

/// Git never looks for a repository above the temp dir.
fn ceiling() -> String {
    let t = std::env::temp_dir();
    let real = t.canonicalize().unwrap_or_else(|_| t.clone());
    format!("{}:{}", t.display(), real.display())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let mut cmd = Command::new("git");
    for v in GIT_ENV {
        cmd.env_remove(v);
    }
    let out = cmd
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", ceiling())
        .env("GIT_AUTHOR_NAME", "publishing")
        .env("GIT_AUTHOR_EMAIL", "qa@example.com")
        .env("GIT_COMMITTER_NAME", "publishing")
        .env("GIT_COMMITTER_EMAIL", "qa@example.com")
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// `cargo run` of the tiny crate at `krate` (its target dir in `target`).
fn stamp(krate: &Path, target: &Path, env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(env!("CARGO"));
    for v in GIT_ENV {
        cmd.env_remove(v);
    }
    let out = cmd
        .args(["run", "--quiet"])
        .env("GIT_CEILING_DIRECTORIES", ceiling())
        .current_dir(krate)
        .env("CARGO_TARGET_DIR", target)
        .env_remove("TIZA_GIT_SHA_OVERRIDE")
        .envs(env.iter().copied())
        .output()
        .expect("cargo run");
    assert!(
        out.status.success(),
        "cargo run in {}: {}",
        krate.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A home directory kept in git (dotfiles committed), with a crate unpacked
/// where cargo unpacks registry sources.
fn home_in_git(name: &str) -> (PathBuf, PathBuf) {
    let root = scratch(name);
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join(".bashrc"), "export EDITOR=vi\n").unwrap();
    git(&home, &["init", "-q", "-b", "main"]);
    git(&home, &["add", ".bashrc"]);
    git(&home, &["commit", "-q", "-m", "dotfiles"]);
    let krate = home.join(".cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tiza-core-0.1.0");
    write_crate(&krate);
    (home, krate)
}

/// QA note 1: in a home kept in git, the unpacked crate is not that
/// repository's commit.
#[test]
fn stamp_crate_from_crates_io_in_a_home_kept_in_git_is_unknown() {
    let (home, krate) = home_in_git("home");
    let target = home.parent().unwrap().join("target");
    assert_eq!(stamp(&krate, &target, &[]), "unknown");
    // A commit in that repository reruns the script, with the same stamp.
    git(
        &home,
        &["commit", "-q", "--allow-empty", "-m", "more dotfiles"],
    );
    assert_eq!(stamp(&krate, &target, &[]), "unknown");
}

/// The rule is "the repository tracks this package": once the same files
/// are committed there, that commit did build them.
#[test]
fn stamp_a_repository_that_tracks_the_crate_gives_its_commit() {
    let (home, krate) = home_in_git("tracked");
    let target = home.parent().unwrap().join("target");
    git(&home, &["add", "."]);
    git(&home, &["commit", "-q", "-m", "vendored"]);
    let head = git(&home, &["rev-parse", "--short=10", "HEAD"]);
    assert_eq!(stamp(&krate, &target, &[]), head);
}

/// QA P3-3: an unpacked tarball becomes a repository after a first build
/// (`git init`, then a commit): that commit reruns the script and stamps
/// itself, with no `touch build.rs`; so does the next one.
#[test]
fn stamp_first_commit_of_a_new_repository_stamps_it() {
    let root = scratch("new-repo");
    let krate = root.join("tarball");
    write_crate(&krate);
    let target = root.join("target");
    git(&krate, &["init", "-q", "-b", "main"]);
    assert_eq!(stamp(&krate, &target, &[]), "unknown", "nothing tracked");
    git(&krate, &["add", "."]);
    git(&krate, &["commit", "-q", "-m", "first"]);
    let first = git(&krate, &["rev-parse", "--short=10", "HEAD"]);
    assert_eq!(stamp(&krate, &target, &[]), first);
    git(&krate, &["commit", "-q", "--allow-empty", "-m", "second"]);
    let second = git(&krate, &["rev-parse", "--short=10", "HEAD"]);
    assert_eq!(stamp(&krate, &target, &[]), second);
}

/// Corner case: `cargo package` verifies the crate in `target/package/` of
/// the checkout, an ignored directory: that build is not the checkout's
/// commit either.
#[test]
fn stamp_ignored_directory_inside_a_checkout_is_unknown() {
    let root = scratch("ignored");
    let repo = root.join("repo");
    write_crate(&repo.join("crates/tiza-core"));
    std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let head = git(&repo, &["rev-parse", "--short=10", "HEAD"]);
    let target = root.join("target");
    assert_eq!(stamp(&repo.join("crates/tiza-core"), &target, &[]), head);
    let unpacked = repo.join("target/package/tiza-core-0.1.0");
    write_crate(&unpacked);
    assert_eq!(stamp(&unpacked, &root.join("target2"), &[]), "unknown");
}

/// This checkout still stamps its own commit into the real binary.
#[test]
fn stamp_this_checkout_keeps_its_commit() {
    let Ok(out) = Command::new("git")
        .args(["rev-parse", "--short=10", "HEAD"])
        .current_dir(root())
        .output()
    else {
        eprintln!("publishing: no git, case not run");
        return;
    };
    if !out.status.success() {
        eprintln!("publishing: not a git checkout, case not run");
        return;
    }
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let version = tiza_qa::command().arg("--version").output().unwrap();
    let said = String::from_utf8_lossy(&version.stdout);
    assert!(said.contains(&format!("({head}")), "{said}");
}

// --- How to help, and security (AC7) ----------------------------------

/// The links of SECURITY.md, CONTRIBUTING.md, the README and the release
/// notes: a relative one names a file of the repository, an absolute one
/// this repository, and the reports go where Luis decided (2026-10-24):
/// Discussions, the bug form, the private vulnerability report.
#[test]
fn the_reporting_links_resolve_and_go_to_the_right_place() {
    let security = read("SECURITY.md");
    assert!(security.contains(&format!("<{REPO}/security/advisories/new>")));
    for name in ["SECURITY.md", "CONTRIBUTING.md"] {
        for t in link_targets(&read(name)) {
            if t.starts_with("https://") {
                assert!(t.starts_with(REPO), "{name}: {t}");
            } else {
                let file = t.split('#').next().unwrap();
                assert!(root().join(file).is_file(), "{name}: {t}");
            }
        }
    }
    let readme = read("README.md");
    for link in [
        "(CONTRIBUTING.md)".to_string(),
        "(SECURITY.md)".to_string(),
        format!("({REPO}/discussions)"),
    ] {
        assert!(readme.contains(&link), "the README does not link {link}");
    }
    let notes = read("docs/release-notes/v0.1.0.md");
    for (name, md) in [("the release notes", &notes), ("the README", &readme)] {
        assert!(md.contains(&format!("({REPO}/discussions)")), "{name}");
        assert!(
            md.contains(&format!("({REPO}/issues/new/choose)")),
            "{name}"
        );
        assert!(
            !md.contains(&format!("({REPO}/issues)")),
            "{name} sends people to Issues without the form"
        );
    }
}

// --- Brand assets (Luis, 2026-10-24) -----------------------------------

/// The logo files Luis chose, kept in the repository as Herdr keeps its
/// own: the README's hero shows the logo (the README image tests check it is this one), and
/// no crate and no release tar carries them.
const BRAND: [&str; 5] = [
    "logo.svg",
    "logo.png",
    "og-card.png",
    "mark.svg",
    "mark-small.svg",
];

#[test]
fn brand_the_files_stay_out_of_the_packages() {
    let about = read("docs/brand/README.md");
    for f in BRAND {
        assert!(root().join("docs/brand").join(f).is_file(), "{f}");
        assert!(
            about.contains(&format!("`{f}`")),
            "docs/brand/README.md: {f}"
        );
    }
    let svg = read("docs/brand/logo.svg");
    for color in ["#22272b", "#f3f0e7"] {
        assert!(svg.contains(color) && about.contains(color), "{color}");
    }
    // Outlines, not text: the logo does not depend on the font, and has
    // nothing that runs or loads.
    for f in ["logo.svg", "mark.svg", "mark-small.svg"] {
        let s = read(&format!("docs/brand/{f}"));
        for bad in ["<text", "font-family", "<script", "href", "<image"] {
            assert!(!s.contains(bad), "{f} has {bad}");
        }
    }
    // Neither a crate nor the release tar (README, licences, the font
    // licences, examples and the binary) carries them.
    for name in ["tiza", "tiza-core", "tiza-tui", "tiza-mcp", "tiza-export"] {
        let files = packaged(name);
        assert!(
            files.iter().all(|f| !f.contains("brand/")),
            "{name} packages brand files"
        );
    }
    let release = read("scripts/release-local.sh");
    assert!(release.contains("cp README.md LICENSE-MIT LICENSE-APACHE \"$stage/\""));
    assert!(!release.contains("docs/brand"));
}
