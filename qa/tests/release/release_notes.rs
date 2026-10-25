//! Release notes: the GitHub Release body was `git log` since the previous
//! tag (for 0.1.0, about two hundred Spanish commit subjects with board
//! ids). It is now a short English page written per version,
//! `docs/release-notes/vX.Y.Z.md`, which `release.yml` and
//! `scripts/release-local.sh` both publish through
//! `scripts/release-notes.sh`.
//!
//! Acceptance criteria:
//! - AC1 One notes file per version, by hand, in English, no board ids;
//!   v0.1.0's from QA 1's draft, its facts checked against the code.
//! - AC2 Both release paths use that file, checked before anything is
//!   built; a plain tag without it is refused naming the path; a suffixed
//!   tag takes its own notes (`vX.Y.Z-rc.1.md`), or two lines (prerelease,
//!   link to the CHANGELOG), never the notes of X.Y.Z, whose archive names
//!   and links are not its own (the self-review's finding).
//! - AC3 README: the status line names the current version (and the
//!   version script keeps it so); the macOS Gatekeeper note.
//! - AC4 The condensed CHANGELOG of 0.1.0 is a proposal for Luis, outside
//!   the repository until he confirms it.
//! - AC5 This file; CHANGELOG.
//!
//! Corner cases: a `#` line inside a code block of the notes; a notes file
//! with CRLF or a byte order mark.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tiza_qa::{fixture, scratch};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap()
}

/// A scratch copy of `scripts/release-notes.sh` with the notes `notes`
/// (file name, bytes) in its `docs/release-notes/`.
fn workspace(name: &str, notes: &[(&str, &[u8])]) -> PathBuf {
    let dir = scratch(name);
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("docs/release-notes")).unwrap();
    std::fs::copy(
        root().join("scripts/release-notes.sh"),
        dir.join("scripts/release-notes.sh"),
    )
    .unwrap();
    for (file, bytes) in notes {
        std::fs::write(dir.join("docs/release-notes").join(file), bytes).unwrap();
    }
    dir
}

fn notes(dir: &Path, tag: &str) -> Output {
    Command::new("bash")
        .arg(dir.join("scripts/release-notes.sh"))
        .args([tag, "owner/repo"])
        .current_dir("/")
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// A board card id: `t-` and three digits.
fn names_a_card(s: &str) -> bool {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(4)).any(|i| {
        b[i] == b't'
            && b[i + 1] == b'-'
            && b[i + 2..i + 5].iter().all(u8::is_ascii_digit)
            && (i == 0 || !b[i - 1].is_ascii_alphanumeric())
            && b.get(i + 5).is_none_or(|c| !c.is_ascii_digit())
    })
}

fn the_fixture() -> Vec<u8> {
    std::fs::read(fixture("release/release_notes/v9.9.9.md")).unwrap()
}

#[test]
fn a_plain_tag_gives_its_notes_as_written() {
    let fx = the_fixture();
    let dir = workspace("plain", &[("v9.9.9.md", &fx)]);
    let o = notes(&dir, "v9.9.9");
    assert!(o.status.success(), "{}", err(&o));
    // As written, the `#` line of the code block included: no commit
    // subjects, no hashes, no generated heading.
    assert_eq!(out(&o).as_bytes(), fx.as_slice());
    assert!(!names_a_card(&out(&o)));
}

#[test]
fn a_plain_tag_without_notes_is_refused_naming_the_path() {
    let dir = workspace("none", &[]);
    let o = notes(&dir, "v9.9.9");
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert!(out(&o).is_empty());
    assert!(
        err(&o).contains("docs/release-notes/v9.9.9.md"),
        "{}",
        err(&o)
    );
}

#[test]
fn a_prerelease_takes_its_own_notes_or_two_lines() {
    let fx = the_fixture();
    let own = b"# Tiza v9.9.9-rc.2\n\nA prerelease with notes of its own.\n";
    let dir = workspace("rc", &[("v9.9.9.md", &fx), ("v9.9.9-rc.2.md", own)]);
    // Never the notes of 9.9.9: their archive names are not the rc's.
    let o = notes(&dir, "v9.9.9-rc.1");
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(
        out(&o),
        "Prerelease of Tiza 9.9.9, to try before the release.\n\
         What changed: [CHANGELOG](https://github.com/owner/repo/blob/v9.9.9-rc.1/CHANGELOG.md).\n"
    );
    let o = notes(&dir, "v9.9.9-rc.2");
    assert_eq!(out(&o).as_bytes(), own.as_slice(), "{}", err(&o));
    // Semantic-version suffixes the `v*` trigger lets through.
    for tag in ["v9.9.9-beta-2", "v9.9.9-rc.1+build.5"] {
        let o = notes(&dir, tag);
        assert!(o.status.success(), "{tag}: {}", err(&o));
        assert!(out(&o).starts_with("Prerelease of Tiza 9.9.9"), "{tag}");
    }
    for bad in ["9.9.9", "v9.9", "vX.Y.Z", "v9.9.9-", "v9.9.9 rc"] {
        assert_eq!(notes(&dir, bad).status.code(), Some(2), "{bad}");
    }
}

#[test]
fn a_byte_order_mark_and_crlf_are_removed() {
    let fx = the_fixture();
    let mut crlf = b"\xEF\xBB\xBF".to_vec();
    crlf.extend(
        String::from_utf8(fx.clone())
            .unwrap()
            .replace('\n', "\r\n")
            .bytes(),
    );
    let dir = workspace("crlf", &[("v9.9.9.md", &crlf)]);
    let o = notes(&dir, "v9.9.9");
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(out(&o).as_bytes(), fx.as_slice());
}

#[test]
fn notes_that_name_a_card_are_refused() {
    let dir = workspace(
        "card",
        // The id is written in two parts, so that the repository holds none.
        &[(
            "v9.9.9.md",
            concat!("# Tiza v9.9.9\n\n- Fixed the bar (t", "-173).\n").as_bytes(),
        )],
    );
    let o = notes(&dir, "v9.9.9");
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert!(err(&o).contains("board card"), "{}", err(&o));
    // A word that only contains the pattern is not a card.
    let dir = workspace(
        "word",
        &[(
            "v9.9.9.md",
            b"# Tiza v9.9.9\n\nUTF-8 and the test-123 suite.\n",
        )],
    );
    assert!(notes(&dir, "v9.9.9").status.success());
}

#[test]
fn the_v0_1_0_notes_match_what_the_release_builds() {
    let n = read("docs/release-notes/v0.1.0.md");
    assert!(!names_a_card(&n));
    let yml = read(".github/workflows/release.yml");
    for target in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-unknown-linux-gnu",
    ] {
        assert!(
            n.contains(&format!("tiza-v0.1.0-{target}.tar.gz")),
            "{target}"
        );
        assert!(yml.contains(&format!("target: {target}")), "{target}");
    }
    // glibc 2.35: the Linux archive is built in an Ubuntu 22.04 container.
    assert!(n.contains("glibc 2.35"));
    assert!(yml.contains(r#"container: "ubuntu:22.04", target: x86_64-unknown-linux-gnu"#));
    // Rust 1.88: the workspace's minimum.
    assert!(n.contains("Rust 1.88"));
    assert!(read("Cargo.toml").contains("rust-version = \"1.88\""));
    assert!(n.contains("xattr -d com.apple.quarantine"));
    // The archives unpack into a folder named as the archive.
    assert!(
        read(".github/workflows/release.yml")
            .contains(r#"name="tiza-${GITHUB_REF_NAME}-${{ matrix.target }}""#)
    );
    assert!(n.contains("cp tiza-v0.1.0-<target>/tiza"));
    assert!(root().join("docs/img/ownership.gif").exists());
}

#[test]
fn the_readme_status_and_the_gatekeeper_note() {
    let readme = read("README.md");
    let toml = read("Cargo.toml");
    let version = toml
        .split("[workspace.package]")
        .nth(1)
        .and_then(|s| s.lines().find_map(|l| l.strip_prefix("version = \"")))
        .and_then(|v| v.strip_suffix('"'))
        .unwrap();
    assert!(
        readme.contains(&format!("\n> Status: **v{version}**")),
        "the status names v{version}"
    );
    assert!(read("scripts/bump-version.sh").contains("> Status: **v$new**"));
}

// ---------- Review ----------

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {}", err(&o));
}

#[test]
fn notes_not_committed_are_refused() {
    let fx = the_fixture();
    let dir = workspace("git", &[("v9.9.9.md", &fx)]);
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "scripts"]);
    git(&dir, &["commit", "-qm", "script"]);
    let o = notes(&dir, "v9.9.9");
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert!(err(&o).contains("is not committed"), "{}", err(&o));
    git(&dir, &["add", "docs"]);
    let o = notes(&dir, "v9.9.9");
    assert!(o.status.success(), "{}", err(&o));
}

#[test]
fn empty_notes_are_refused() {
    for (name, bytes) in [
        ("empty", &b""[..]),
        ("bom", &b"\xEF\xBB\xBF\r\n\r\n"[..]),
        ("blank", &b"  \n\t\n"[..]),
    ] {
        let dir = workspace(&format!("empty-{name}"), &[("v9.9.9.md", bytes)]);
        let o = notes(&dir, "v9.9.9");
        assert_eq!(o.status.code(), Some(1), "{name}: {}", out(&o));
        assert!(err(&o).contains("has no text"), "{name}: {}", err(&o));
    }
}

#[test]
fn the_byte_order_mark_goes_whatever_perl_unicode_says() {
    let fx = the_fixture();
    let mut bom = b"\xEF\xBB\xBF".to_vec();
    bom.extend(&fx);
    let dir = workspace("perl", &[("v9.9.9.md", &bom)]);
    for value in ["SD", "SDA", ""] {
        let o = Command::new("bash")
            .arg(dir.join("scripts/release-notes.sh"))
            .args(["v9.9.9", "owner/repo"])
            .env("PERL_UNICODE", value)
            .env("LC_ALL", "en_US.UTF-8")
            .output()
            .unwrap();
        assert_eq!(out(&o).as_bytes(), fx.as_slice(), "PERL_UNICODE={value:?}");
    }
}

#[test]
fn through_a_symlink_from_elsewhere() {
    let fx = the_fixture();
    let dir = workspace("link", &[("v9.9.9.md", &fx)]);
    let other = scratch("link-other");
    std::os::unix::fs::symlink(dir.join("scripts/release-notes.sh"), other.join("notes.sh"))
        .unwrap();
    let o = Command::new("bash")
        .arg(other.join("notes.sh"))
        .arg("v9.9.9")
        .current_dir(&other)
        .output()
        .unwrap();
    assert_eq!(out(&o).as_bytes(), fx.as_slice(), "{}", err(&o));
}

#[test]
fn a_card_written_with_another_hyphen_is_refused() {
    for text in ["(t\u{2011}173)", "t\u{2010}173", "t\u{2212}173"] {
        let body = format!("# Tiza v9.9.9\n\nFixed {text}.\n");
        let dir = workspace("hyphen", &[("v9.9.9.md", body.as_bytes())]);
        let o = notes(&dir, "v9.9.9");
        assert_eq!(o.status.code(), Some(1), "{text}: {}", out(&o));
    }
}

#[test]
fn a_local_linux_archive_needs_glibc_2_35_at_most() {
    let local = read("scripts/release-local.sh");
    let check = local.find("glibc 2.35 at most").expect("the check");
    assert!(
        check < local.find("cargo build").unwrap(),
        "before building"
    );
    assert!(local.contains("ubuntu:22.04 container"));
}

#[test]
fn bump_version_refuses_a_status_line_twice_and_writes_nothing() {
    let dir = scratch("bump");
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    for f in ["Cargo.toml", "CHANGELOG.md", "scripts/bump-version.sh"] {
        std::fs::copy(root().join(f), dir.join(f)).unwrap();
    }
    let readme = read("README.md");
    let line = readme
        .lines()
        .find(|l| l.starts_with("> Status: **v"))
        .unwrap();
    let twice = format!("{readme}\n{line}\n");
    std::fs::write(dir.join("README.md"), &twice).unwrap();
    let before = std::fs::read(dir.join("Cargo.toml")).unwrap();
    let o = Command::new("bash")
        .arg(dir.join("scripts/bump-version.sh"))
        .arg("9.8.7")
        .current_dir(&dir)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1), "{}", out(&o));
    assert!(err(&o).contains("2 times"), "{}", err(&o));
    assert_eq!(std::fs::read(dir.join("Cargo.toml")).unwrap(), before);
    assert_eq!(
        std::fs::read_to_string(dir.join("README.md")).unwrap(),
        twice
    );
}
