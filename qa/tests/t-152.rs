//! t-152 (part A) — Before the repository goes public: the workflows build
//! with the pinned toolchain, docs/contributing/releasing.md says how to
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
//!   crates.io and no secret is needed; docs/contributing/releasing.md
//!   (`# Releases (maintainers)`, the page release.yml names) gives every
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

use tiza_qa::{bin, scratch};

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
        eprintln!("t-152: python3 with PyYAML not found, YAML syntax not checked");
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

/// The text of `docs/contributing/releasing.md`, the page release.yml
/// names: its H1 is `# Releases (maintainers)`.
fn release_section() -> String {
    let page = read("docs/contributing/releasing.md");
    assert!(
        page.starts_with("# Releases (maintainers)\n"),
        "docs/contributing/releasing.md starts with `# Releases (maintainers)`"
    );
    page
}

/// QA P3-4: a real section, with the anchor release.yml points at.
#[test]
fn publish_release_section_has_an_anchor_release_yml_uses() {
    let section = release_section();
    let heading = section.lines().next().unwrap().trim_start_matches("# ");
    assert_eq!(anchor(heading), "releases-maintainers");
    let yml = read(".github/workflows/release.yml");
    assert!(
        yml.contains("docs/contributing/releasing.md#releases-maintainers"),
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
    for d in ["crates", "qa/src", "scripts"] {
        copy_dir(&root().join(d), &dir.join(d));
    }
    std::fs::create_dir_all(dir.join("qa/tests")).unwrap();
    std::fs::copy(root().join("qa/Cargo.toml"), dir.join("qa/Cargo.toml")).unwrap();
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
    let dir = workspace_copy("t152-bump");
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
fn ac2_tiza_crates_io_metadata() {
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
fn ac2_tiza_readme_is_packaged_and_works_on_crates_io() {
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

fn build_rs() -> String {
    read("crates/tiza-core/build.rs")
}

/// A tiny crate whose build script is a copy of tiza-core's and whose binary
/// prints `TIZA_GIT_SHA` (as in t-181), written into `dir`.
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
        .env("GIT_AUTHOR_NAME", "t152")
        .env("GIT_AUTHOR_EMAIL", "t152@example.com")
        .env("GIT_COMMITTER_NAME", "t152")
        .env("GIT_COMMITTER_EMAIL", "t152@example.com")
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
    let (home, krate) = home_in_git("t152-home");
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
    let (home, krate) = home_in_git("t152-tracked");
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
    let root = scratch("t152-new-repo");
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
    let root = scratch("t152-ignored");
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
        eprintln!("t-152: no git, case not run");
        return;
    };
    if !out.status.success() {
        eprintln!("t-152: not a git checkout, case not run");
        return;
    }
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let version = Command::new(bin()).arg("--version").output().unwrap();
    let said = String::from_utf8_lossy(&version.stdout);
    assert!(said.contains(&format!("({head}")), "{said}");
}

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
    let line = r#"ms = frame_median("t-191", "ac6")"#;
    let at = ticket_ids(line)[0];
    assert!(allowed("scripts/bench.py", line, at));
    assert!(!allowed("scripts/other.py", line, at));
    let bare = "# see t-191";
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

/// The links of SECURITY.md, CONTRIBUTING.md, the README and the release
/// notes: a relative one names a file of the repository, an absolute one
/// this repository, and the reports go where Luis decided (2026-10-24):
/// Discussions, the bug form, the private vulnerability report.
#[test]
fn ac7_the_reporting_links_resolve_and_go_to_the_right_place() {
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

/// The logo files Luis chose, kept in the repository as Herdr keeps its
/// own: the README's hero shows the logo (t-121 checks it is this one), and
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
    // Neither a crate nor the release tar (README, licences, examples and
    // the binary) carries them.
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

// --- The help of `tiza render` (docs review, 2026-10-24) ---------------

#[test]
fn render_help_names_every_format_that_out_writes() {
    let out = Command::new(bin())
        .args(["render", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    let summary = help.lines().next().unwrap();
    // Each extension `--out` takes, and the name the summary gives it.
    let formats = [
        (".txt", "text"),
        (".md", "Markdown"),
        (".ansi", "ANSI"),
        (".svg", "SVG"),
        (".png", "PNG"),
        (".gif", "GIF"),
        (".mp4", "MP4"),
        (".webm", "WebM"),
        (".cast", "asciicast"),
        (".tape", "VHS"),
    ];
    let listed = help
        .lines()
        .find(|l| l.contains("The extension picks the format"))
        .expect("the --out help lists the extensions");
    let count = listed.matches("`.").count();
    assert_eq!(count, formats.len(), "{listed}");
    for (ext, name) in formats {
        assert!(
            listed.contains(&format!("`{ext}`")),
            "--out does not list {ext}"
        );
        assert!(
            summary.contains(name),
            "the summary does not name {name}: {summary}"
        );
    }
}
