//! `scripts/release-check.py`, which release.yml and
//! `scripts/release-local.sh` run before a release: the README status gives
//! the version of the workspace and of the tag, the links of release.yml go
//! to headings of CONTRIBUTING.md, and its release section gives the steps
//! in order and the crates in dependency order.
//!
//! The script runs on a small workspace of its own in a scratch folder, not
//! on the docs of this repository: the job that builds the site and the
//! release check those.
//!
//! Corner cases: CRLF and a byte order mark; a heading inside a code
//! block; an empty file, and one that is not UTF-8; a crate that is not
//! published.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tiza_qa::{repo_root, scratch};

const README: &str = "# Tiza\n\n> Status: **v1.2.3** (early).\n";

const CONTRIBUTING: &str = "# Contributing

```sh
## Releases (maintainers)
```

## Releases (maintainers)

```sh
git switch main && git pull && git status --short
scripts/bump-version.sh X.Y.Z
$EDITOR docs/release-notes/vX.Y.Z.md
git add docs/release-notes/vX.Y.Z.md
git commit -am \"vX.Y.Z\"
cargo publish --workspace --dry-run --locked
scripts/release-check.py vX.Y.Z
git push origin main
git tag vX.Y.Z && git push origin vX.Y.Z
cargo login
cargo publish --locked -p tiza-core
cargo publish --locked -p tiza          # the binary
```

## After a release

Nothing.
";

const RELEASE_YML: &str = "name: Release
# crates.io is published by hand: CONTRIBUTING.md#releases-maintainers.
on: push
";

/// A workspace with `tiza` (1.2.3) on `tiza-core`, a crate that is not
/// published, the script, and the three pages it reads; the files of
/// `pages` take the place of those of the same path.
fn workspace(name: &str, pages: &[(&str, &[u8])]) -> PathBuf {
    let dir = scratch(name).join("ws");
    let mut files: Vec<(&str, Vec<u8>)> = vec![
        (
            "Cargo.toml",
            b"[workspace]\nresolver = \"3\"\nmembers = [\"core\", \"tiza\", \"internal\"]\n\n\
              [workspace.package]\nversion = \"1.2.3\"\nedition = \"2024\"\n"
                .to_vec(),
        ),
        (
            "core/Cargo.toml",
            b"[package]\nname = \"tiza-core\"\nversion.workspace = true\nedition.workspace = true\n"
                .to_vec(),
        ),
        (
            "tiza/Cargo.toml",
            b"[package]\nname = \"tiza\"\nversion.workspace = true\nedition.workspace = true\n\n\
              [dependencies]\ntiza-core = { path = \"../core\", version = \"1.2.3\" }\n"
                .to_vec(),
        ),
        (
            "internal/Cargo.toml",
            b"[package]\nname = \"tiza-internal\"\npublish = false\nversion.workspace = true\n\
              edition.workspace = true\n\n[dependencies]\ntiza = { path = \"../tiza\" }\n"
                .to_vec(),
        ),
        ("core/src/lib.rs", Vec::new()),
        ("tiza/src/lib.rs", Vec::new()),
        ("internal/src/lib.rs", Vec::new()),
        ("README.md", README.as_bytes().to_vec()),
        ("CONTRIBUTING.md", CONTRIBUTING.as_bytes().to_vec()),
        (".github/workflows/release.yml", RELEASE_YML.as_bytes().to_vec()),
        (
            "scripts/release-check.py",
            std::fs::read(repo_root().join("scripts/release-check.py")).unwrap(),
        ),
    ];
    for (page, bytes) in pages {
        let at = files.iter().position(|(f, _)| f == page).unwrap();
        files[at].1 = bytes.to_vec();
    }
    for (rel, bytes) in files {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    dir
}

/// `scripts/release-check.py ARGS` in `dir`, offline, with this toolchain's
/// cargo first on PATH.
fn check(dir: &Path, args: &[&str]) -> Output {
    let cargo_dir = Path::new(env!("CARGO")).parent().unwrap().to_path_buf();
    let path = format!(
        "{}:{}",
        cargo_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Command::new("python3")
        .arg("-I")
        .arg(dir.join("scripts/release-check.py"))
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .expect("python3")
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The script fails, and says each of `says`.
fn refused(o: &Output, says: &[&str]) {
    assert_eq!(o.status.code(), Some(1), "{}", err(o));
    for s in says {
        assert!(err(o).contains(s), "no {s:?} in:\n{}", err(o));
    }
}

#[test]
fn a_workspace_that_is_ready_passes() {
    let dir = workspace("ready", &[]);
    for args in [&[][..], &["v1.2.3"], &["v1.2.3-rc.1"]] {
        let o = check(&dir, args);
        assert!(o.status.success(), "{args:?}: {}", err(&o));
        assert_eq!(err(&o), "", "{args:?}");
    }
}

#[test]
fn a_status_of_another_version_is_refused_with_its_line() {
    let dir = workspace(
        "status",
        &[("README.md", b"# Tiza\n\n> Status: **v1.2.2**.\n")],
    );
    refused(
        &check(&dir, &["v1.2.3"]),
        &["README.md:3:", "does not give v1.2.3"],
    );
    let dir = workspace("no-status", &[("README.md", b"# Tiza\n")]);
    refused(&check(&dir, &[]), &["README.md has 0 status lines"]);
}

#[test]
fn a_tag_of_another_version_is_refused() {
    let dir = workspace("tag", &[]);
    refused(
        &check(&dir, &["v1.2.4"]),
        &["the tag v1.2.4 is not version 1.2.3", "bump-version.sh"],
    );
    refused(&check(&dir, &["v1.2.4-rc.1"]), &["is not version 1.2.3"]);
    let o = check(&dir, &["1.2.3"]);
    assert_eq!(o.status.code(), Some(2), "{}", err(&o));
}

#[test]
fn a_release_section_only_in_a_code_block_is_not_found() {
    // The same line in the code block above stays.
    let page = CONTRIBUTING.replace("```\n\n## Releases (maintainers)\n", "```\n\n## Releases\n");
    let dir = workspace("section", &[("CONTRIBUTING.md", page.as_bytes())]);
    refused(
        &check(&dir, &[]),
        &[
            "CONTRIBUTING.md has no section `## Releases (maintainers)`",
            ".github/workflows/release.yml:2: CONTRIBUTING.md has no heading with the id #releases-maintainers",
        ],
    );
}

#[test]
fn a_link_of_the_workflow_to_a_heading_that_is_not_there_is_refused() {
    let yml = RELEASE_YML.replace("#releases-maintainers", "#releases");
    let dir = workspace(
        "anchor",
        &[(".github/workflows/release.yml", yml.as_bytes())],
    );
    refused(
        &check(&dir, &[]),
        &[".github/workflows/release.yml:2: CONTRIBUTING.md has no heading with the id #releases"],
    );
}

#[test]
fn a_step_out_of_order_or_missing_is_refused() {
    // The check of the release comes after the tag.
    let page = CONTRIBUTING
        .replace("scripts/release-check.py vX.Y.Z\n", "")
        .replace(
            "cargo login\n",
            "cargo login\nscripts/release-check.py vX.Y.Z\n",
        );
    let dir = workspace("order", &[("CONTRIBUTING.md", page.as_bytes())]);
    refused(
        &check(&dir, &[]),
        &["`git push origin main` comes before `scripts/release-check.py vX.Y.Z`"],
    );
    let page = CONTRIBUTING.replace("cargo login\n", "");
    let dir = workspace("missing", &[("CONTRIBUTING.md", page.as_bytes())]);
    refused(&check(&dir, &[]), &["has no `cargo login`"]);
}

#[test]
fn a_crate_before_its_dependency_or_left_out_is_refused() {
    let page = CONTRIBUTING.replace(
        "cargo publish --locked -p tiza-core\ncargo publish --locked -p tiza          # the binary\n",
        "cargo publish --locked -p tiza\ncargo publish --locked -p tiza-core\n",
    );
    let dir = workspace("deps", &[("CONTRIBUTING.md", page.as_bytes())]);
    refused(
        &check(&dir, &[]),
        &["tiza depends on tiza-core, which is published after it"],
    );
    let page = CONTRIBUTING.replace("cargo publish --locked -p tiza-core\n", "");
    let dir = workspace("left-out", &[("CONTRIBUTING.md", page.as_bytes())]);
    refused(&check(&dir, &[]), &["does not publish tiza-core"]);
    // A crate with `publish = false` is not a step.
    let page = CONTRIBUTING.replace(
        "cargo login\n",
        "cargo login\ncargo publish --locked -p tiza-internal\n",
    );
    let dir = workspace("internal", &[("CONTRIBUTING.md", page.as_bytes())]);
    refused(
        &check(&dir, &[]),
        &["tiza-internal is not a crate of the workspace that is published"],
    );
}

#[test]
fn crlf_and_a_byte_order_mark_pass() {
    let crlf = |s: &str| {
        let mut b = b"\xEF\xBB\xBF".to_vec();
        b.extend(s.replace('\n', "\r\n").into_bytes());
        b
    };
    let (readme, page, yml) = (crlf(README), crlf(CONTRIBUTING), crlf(RELEASE_YML));
    let dir = workspace(
        "crlf",
        &[
            ("README.md", &readme),
            ("CONTRIBUTING.md", &page),
            (".github/workflows/release.yml", &yml),
        ],
    );
    let o = check(&dir, &["v1.2.3"]);
    assert!(o.status.success(), "{}", err(&o));
}

#[test]
fn an_empty_page_or_one_that_is_not_utf8_is_refused_naming_it() {
    let dir = workspace("empty", &[("CONTRIBUTING.md", b"\xEF\xBB\xBF\r\n")]);
    refused(&check(&dir, &[]), &["CONTRIBUTING.md is empty"]);
    let dir = workspace(
        "latin1",
        &[("README.md", b"> Status: **v1.2.3** \xE9t\xE9\n")],
    );
    refused(&check(&dir, &[]), &["README.md: not UTF-8"]);
}

#[test]
fn the_workflow_must_link_the_release_section() {
    let yml = "name: Release\non: push\n";
    let dir = workspace(
        "no-link",
        &[(".github/workflows/release.yml", yml.as_bytes())],
    );
    refused(
        &check(&dir, &[]),
        &[".github/workflows/release.yml does not link CONTRIBUTING.md#releases-maintainers"],
    );
}

/// The text of the section can name a command: only the code blocks give
/// the steps.
#[test]
fn a_command_named_in_the_text_is_not_a_step() {
    let page = CONTRIBUTING.replace(
        "## Releases (maintainers)\n\n```sh\n",
        "## Releases (maintainers)\n\nRun `cargo login` once on each machine.\n\n```sh\n",
    );
    let dir = workspace("prose", &[("CONTRIBUTING.md", page.as_bytes())]);
    let o = check(&dir, &[]);
    assert!(o.status.success(), "{}", err(&o));
}

#[test]
fn a_fence_that_closes_with_an_indent_and_spaces_closes() {
    let page = CONTRIBUTING.replace(
        "```\n\n## Releases (maintainers)\n",
        "  ```  \n\n## Releases (maintainers)\n",
    );
    let dir = workspace("fence", &[("CONTRIBUTING.md", page.as_bytes())]);
    let o = check(&dir, &[]);
    assert!(o.status.success(), "{}", err(&o));
}

/// A dev-dependency does not have to be on crates.io before the crate.
#[test]
fn a_dev_dependency_does_not_order_the_crates() {
    let core =
        b"[package]\nname = \"tiza-core\"\nversion.workspace = true\nedition.workspace = true\n\n\
                 [dev-dependencies]\ntiza = { path = \"../tiza\" }\n";
    let dir = workspace("dev-dep", &[("core/Cargo.toml", core)]);
    let o = check(&dir, &[]);
    assert!(o.status.success(), "{}", err(&o));
}

/// Both paths of a release run the check before they build anything.
#[test]
fn both_release_paths_run_the_check_before_they_build() {
    let yml = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml")).unwrap();
    let at = |s: &str| {
        yml.find(s)
            .unwrap_or_else(|| panic!("release.yml has no {s:?}"))
    };
    let run = at("run: scripts/release-check.py \"${GITHUB_REF_NAME}\"");
    assert!(
        at("\n  notes:\n") < run && run < at("\n  build:\n"),
        "release.yml runs the check outside the job `notes`"
    );
    assert!(
        yml.contains("\n    needs: notes\n"),
        "the job `build` does not wait for `notes`"
    );
    let local = std::fs::read_to_string(repo_root().join("scripts/release-local.sh")).unwrap();
    let run = local
        .find("scripts/release-check.py \"$tag\"")
        .expect("release-local.sh runs scripts/release-check.py with the tag");
    let build = local.find("cargo build").expect("release-local.sh builds");
    assert!(
        run < build,
        "release-local.sh builds before it checks the docs"
    );
}
