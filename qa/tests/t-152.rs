//! t-152 (part A) — Before the repository goes public: the workflows build
//! with the pinned toolchain, a release publishes to crates.io, the crates.io
//! pages say what each crate is, and a crate unpacked from crates.io does not
//! take the commit of a git repository it happens to sit in.
//!
//! Acceptance criteria (part A of the card, plus its notes):
//! - Toolchain (note of 2026-10-19): every job of `ci.yml` and `release.yml`
//!   that runs cargo installs the toolchain `rust-toolchain.toml` pins
//!   (1.99.1, the only place its version is written) before the first cargo
//!   command, and the release build adds its target to that toolchain, not
//!   to `stable`.
//! - Publication: `release.yml` has a `publish` job that runs after the
//!   GitHub Release, only for a tag without suffix, publishes the workspace
//!   with `cargo publish --workspace` and the `CARGO_REGISTRY_TOKEN` secret,
//!   checks the tag against the workspace version, and skips what is already
//!   on crates.io (a re-run); the README says which secret to create.
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
//!   keeps its commit; `TIZA_GIT_SHA_OVERRIDE` still wins.
//!
//! The workflows cannot run here (Actions is off while the repository is
//! private): their structure is read as text, and the publish step's script
//! is run as written with `cargo publish` and `curl` replaced by fakes.

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
    for wf in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
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
        let mut with_cargo = 0;
        for (name, body) in jobs(&yml) {
            let Some(cargo) = first_cargo(&body) else {
                continue;
            };
            with_cargo += 1;
            let install = position(&body, "rustup toolchain install").unwrap_or_else(|| {
                panic!("{wf} job `{name}` runs cargo without `rustup toolchain install`")
            });
            assert!(
                install < cargo,
                "{wf} job `{name}`: the toolchain is installed after the first cargo command"
            );
        }
        assert!(with_cargo >= 2, "{wf}: the jobs were not read");
    }
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

#[test]
fn toolchain_cargo_deny_reads_rust_toolchain_toml() {
    let yml = read(".github/workflows/ci.yml");
    let jobs = jobs(&yml);
    let deny = job(&jobs, "deny").join("\n");
    assert!(deny.contains("EmbarkStudios/cargo-deny-action@v2"));
    assert!(
        !deny.contains("rust-version:"),
        "cargo-deny-action installs the toolchain of rust-toolchain.toml itself"
    );
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

// --- Publication ---------------------------------------------------------

/// The `run: |` script of the step named `step` in job `job_name`, dedented.
fn step_script(yml: &str, job_name: &str, step: &str) -> String {
    let jobs = jobs(yml);
    let body = job(&jobs, job_name);
    let start = position(body, &format!("name: {step}"))
        .unwrap_or_else(|| panic!("no step `{step}` in `{job_name}`"));
    let run = body[start..]
        .iter()
        .position(|l| l.trim() == "run: |")
        .map(|i| start + i)
        .expect("the step has a `run: |` script");
    let key_indent = body[run].len() - body[run].trim_start().len();
    let mut lines = Vec::new();
    for l in &body[run + 1..] {
        if !l.trim().is_empty() && l.len() - l.trim_start().len() <= key_indent {
            break;
        }
        lines.push(l.as_str());
    }
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap();
    lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn publish_job_runs_after_the_release_for_final_tags() {
    let yml = read(".github/workflows/release.yml");
    let jobs = jobs(&yml);
    let publish = job(&jobs, "publish").join("\n");
    assert!(publish.contains("needs: release"), "after the release");
    assert!(
        publish.contains("if: ${{ !contains(github.ref_name, '-') }}"),
        "not for -rc / -alpha tags"
    );
    assert!(publish.contains("CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}"));
    assert!(publish.contains("cargo publish --workspace --locked"));
    // The prerelease flag of the release and the publish condition agree.
    assert!(yml.contains("prerelease: ${{ contains(github.ref_name, '-') }}"));
    // tiza-qa never goes to crates.io; `cargo publish --workspace` skips it.
    assert!(read("qa/Cargo.toml").contains("publish = false"));
    let readme = read("README.md");
    assert!(
        readme.contains("`CARGO_REGISTRY_TOKEN`"),
        "the README names the secret to create"
    );
}

/// Runs the publish step's script as written, in the repository, with fakes
/// first on PATH: `cargo publish` only records its arguments (every other
/// cargo command is the real one) and `curl` answers the crates.io index
/// with `published` already there.
fn run_publish(name: &str, tag: &str, token: &str, published: &[&str]) -> (Output, Vec<String>) {
    let script = step_script(
        &read(".github/workflows/release.yml"),
        "publish",
        "cargo publish",
    );
    let dir = scratch(name);
    let fake = dir.join("bin");
    std::fs::create_dir_all(&fake).unwrap();
    let log = dir.join("publish.log");
    let write_exe = |name: &str, body: String| {
        let p = fake.join(name);
        std::fs::write(&p, body).unwrap();
        let mut perm = std::fs::metadata(&p).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
        std::fs::set_permissions(&p, perm).unwrap();
    };
    write_exe(
        "cargo",
        format!(
            "#!/bin/sh\nif [ \"$1\" = publish ]; then echo \"$*\" >> '{}'; exit 0; fi\nexec '{}' \"$@\"\n",
            log.display(),
            env!("CARGO")
        ),
    );
    // The last path segment of the index URL is the crate's name.
    write_exe(
        "curl",
        "#!/bin/sh\nfor a; do url=$a; done\nname=${url##*/}\nfor p in $FAKE_PUBLISHED; do\n  if [ \"$p\" = \"$name\" ]; then printf '{\"name\":\"%s\",\"vers\":\"0.0.0-old\"}\\n{\"name\":\"%s\",\"vers\":\"%s\"}\\n' \"$name\" \"$name\" \"$FAKE_VERSION\"; exit 0; fi\ndone\necho 'curl: (22) The requested URL returned error: 404' >&2\nexit 22\n".to_string(),
    );
    let path = format!(
        "{}:{}",
        fake.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("bash")
        .args(["-c", &script])
        .current_dir(root())
        .env("PATH", path)
        .env("GITHUB_REF_NAME", tag)
        .env("CARGO_REGISTRY_TOKEN", token)
        .env("FAKE_PUBLISHED", published.join(" "))
        .env("FAKE_VERSION", workspace_version())
        .output()
        .unwrap();
    let calls = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    (out, calls)
}

fn workspace_version() -> String {
    let toml = read("Cargo.toml");
    let section = toml.split("[workspace.package]").nth(1).unwrap();
    section
        .lines()
        .find_map(|l| l.strip_prefix("version = "))
        .unwrap()
        .trim_matches('"')
        .to_string()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn publish_first_run_publishes_the_whole_workspace() {
    let tag = format!("v{}", workspace_version());
    let (out, calls) = run_publish("t152-first", &tag, "token", &[]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(calls, vec!["publish --workspace --locked".to_string()]);
}

#[test]
fn publish_rerun_skips_what_is_already_on_crates_io() {
    let tag = format!("v{}", workspace_version());
    let (out, calls) = run_publish("t152-rerun", &tag, "token", &["tiza-core", "tiza-tui"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(calls.len(), 1, "{calls:?}");
    let args = &calls[0];
    assert!(args.starts_with("publish --workspace --locked"), "{args}");
    assert!(args.contains("--exclude tiza-core") && args.contains("--exclude tiza-tui"));
    for other in ["tiza-mcp", "tiza-export", "tiza ", "tiza-qa"] {
        assert!(!args.contains(&format!("--exclude {other}")), "{args}");
    }
    assert!(
        text(&out).contains("already on crates.io"),
        "{}",
        text(&out)
    );
}

#[test]
fn publish_rerun_with_everything_published_does_nothing() {
    let tag = format!("v{}", workspace_version());
    let all = ["tiza-core", "tiza-tui", "tiza-mcp", "tiza-export", "tiza"];
    let (out, calls) = run_publish("t152-all", &tag, "token", &all);
    assert!(out.status.success(), "{}", text(&out));
    assert!(calls.is_empty(), "{calls:?}");
}

#[test]
fn publish_refuses_a_tag_that_is_not_the_workspace_version() {
    let (out, calls) = run_publish("t152-tag", "v9.9.9", "token", &[]);
    assert!(!out.status.success());
    assert!(text(&out).contains("does not match"), "{}", text(&out));
    assert!(calls.is_empty());
}

#[test]
fn publish_refuses_without_the_secret() {
    let tag = format!("v{}", workspace_version());
    let (out, calls) = run_publish("t152-secret", &tag, "", &[]);
    assert!(!out.status.success());
    assert!(
        text(&out).contains("CARGO_REGISTRY_TOKEN"),
        "{}",
        text(&out)
    );
    assert!(calls.is_empty());
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

// --- Internal crates -----------------------------------------------------

const INTERNAL: &[(&str, &str)] = &[
    ("tiza-core", "crates/tiza-core"),
    ("tiza-tui", "crates/tiza-tui"),
    ("tiza-mcp", "crates/tiza-mcp"),
    ("tiza-export", "crates/tiza-export"),
];

#[test]
fn ac3_internal_crates_say_so() {
    let meta = metadata();
    for (name, dir) in INTERNAL {
        let p = package(&meta, name);
        let description = p["description"].as_str().unwrap();
        assert!(
            description.starts_with("Internal crate of Tiza")
                && description.contains("cargo install tiza")
                && description.contains("no stable API"),
            "{name}: {description}"
        );
        let lib = read(&format!("{dir}/src/lib.rs"));
        let first = lib.lines().next().unwrap();
        assert!(
            first.starts_with(&format!("//! Internal crate of [Tiza]({REPO})")),
            "{name}: {first}"
        );
        let head: String = lib
            .lines()
            .take_while(|l| l.starts_with("//!") && *l != "//!")
            .map(|l| l.trim_start_matches("//!").trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            head.contains("`cargo install tiza`") && head.contains("no stable API"),
            "{name}: {head}"
        );
        let readme = readme_of(p);
        assert!(
            readme.starts_with(root().join(dir)),
            "{name}: its own README"
        );
        let md = std::fs::read_to_string(&readme).unwrap();
        assert!(md.starts_with(&format!("# {name}\n")), "{name}");
        assert!(
            md.contains(REPO) && md.contains("cargo install tiza"),
            "{name}"
        );
        assert!(md.contains("no stable API"), "{name}");
        assert!(md.lines().count() <= 15, "{name}: a short README");
        assert!(packaged(name).iter().any(|f| f == "README.md"), "{name}");
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
    // A commit in that repository changes nothing (no rerun, same stamp).
    git(
        &home,
        &["commit", "-q", "--allow-empty", "-m", "more dotfiles"],
    );
    assert_eq!(stamp(&krate, &target, &[]), "unknown");
}

#[test]
fn stamp_override_still_wins() {
    let (home, krate) = home_in_git("t152-override");
    let target = home.parent().unwrap().join("target");
    assert_eq!(
        stamp(&krate, &target, &[("TIZA_GIT_SHA_OVERRIDE", " v0.1.0 ")]),
        "v0.1.0"
    );
    assert_eq!(stamp(&krate, &target, &[]), "unknown", "back without it");
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
