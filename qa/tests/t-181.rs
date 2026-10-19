//! t-181 — `tiza --version` shows a stale commit when built in a git
//! worktree: build.rs watched `../../.git/HEAD`, which in a worktree is a
//! file pointing elsewhere, so cargo never reran it.
//!
//! Acceptance criteria:
//! - AC1 build.rs finds the real git dir and the ref HEAD points to and
//!   emits `rerun-if-changed` for HEAD, that ref's file (and `packed-refs`)
//!   and the index, in a normal checkout and in a worktree.
//! - AC2 In a worktree: build, commit (or `git reset --hard` to another
//!   commit), build again → the new commit, and `-dirty` only when the tree
//!   really is dirty.
//! - AC3 Outside git it still builds, with the fallback it has today
//!   (`unknown`), and `TIZA_GIT_SHA_OVERRIDE` wins.
//! - AC4 This file; CHANGELOG (Fixed).
//!
//! Building the whole workspace per case would take minutes, so each test
//! builds a tiny crate whose build script is a copy of
//! `crates/tiza-core/build.rs` and whose binary prints `TIZA_GIT_SHA`, inside
//! a git repository of its own (never the project's `.git`), with a worktree
//! whose path has spaces. Each case only rebuilds: what it checks is that
//! cargo reruns the script when the commit moves, which is the bug. The
//! moves are chosen so that each watched file is the only one that changes
//! (a ref moved from another checkout: its reflog; a detached HEAD without
//! reflog: HEAD; reftable: `reftable/`); `packed-refs` is a safety net no
//! case isolates, since moving a packed ref writes its loose file and log.

use std::path::{Path, PathBuf};
use std::process::Command;

use tiza_qa::scratch;

fn build_rs() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/tiza-core/build.rs");
    std::fs::read_to_string(p).unwrap()
}

/// Where the tiny crate sits in a checkout: two levels down, as
/// `crates/tiza-core` does in the project (so the old `../../.git/…` paths
/// would exist in a main checkout).
const CRATE: &str = "crates/stamp";

/// Writes the tiny crate into `dir`.
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

/// Git variables a parent process (a hook, say) may have set: they would
/// point these commands at another repository.
const GIT_ENV: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
];

/// Git never looks for a repository above the temp dir, so a crate outside
/// the tests' own repositories really is outside git.
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
        .env("GIT_AUTHOR_NAME", "t181")
        .env("GIT_AUTHOR_EMAIL", "t181@example.com")
        .env("GIT_COMMITTER_NAME", "t181")
        .env("GIT_COMMITTER_EMAIL", "t181@example.com")
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn head(dir: &Path) -> String {
    git(dir, &["rev-parse", "--short=10", "HEAD"])
}

/// `cargo run` of the tiny crate in the checkout `dir` (one target dir per
/// checkout, as a worktree has its own), with extra environment; returns
/// its stamp.
fn stamp(dir: &Path, env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(env!("CARGO"));
    for v in GIT_ENV {
        cmd.env_remove(v);
    }
    let out = cmd
        .args(["run", "--quiet"])
        .env("GIT_CEILING_DIRECTORIES", ceiling())
        .current_dir(dir.join(CRATE))
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env_remove("TIZA_GIT_SHA_OVERRIDE")
        .envs(env.iter().copied())
        .output()
        .expect("cargo run");
    assert!(
        out.status.success(),
        "cargo run in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository with the tiny crate committed on `main`, and a worktree of
/// it on branch `wt` at "<root>/a work tree" (spaces on purpose).
fn repo_and_worktree(name: &str) -> (PathBuf, PathBuf) {
    let root = scratch(name);
    let main = root.join("main");
    write_crate(&main.join(CRATE));
    std::fs::write(main.join(".gitignore"), "target/\n").unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    git(&main, &["add", "."]);
    git(&main, &["commit", "-q", "-m", "one"]);
    let wt = root.join("a work tree");
    git(
        &main,
        &["worktree", "add", "-q", "-b", "wt", wt.to_str().unwrap()],
    );
    (main, wt)
}

/// Test steps 1-2 and AC2: in a worktree the stamp follows commits and
/// resets, and says `-dirty` only while the tree is.
#[test]
fn ac1_ac2_worktree_follows_commit_reset_and_dirty() {
    let (_main, wt) = repo_and_worktree("t181-wt");
    assert!(wt.join(".git").is_file(), "a worktree's .git is a file");
    let first = head(&wt);
    assert_eq!(stamp(&wt, &[]), first);

    git(&wt, &["commit", "-q", "--allow-empty", "-m", "two"]);
    let second = head(&wt);
    assert_ne!(second, first);
    assert_eq!(stamp(&wt, &[]), second, "a commit in the worktree");

    git(&wt, &["reset", "-q", "--hard", &first]);
    assert_eq!(stamp(&wt, &[]), first, "a reset to another commit");

    // The build script's own `git status` leaves the index alone (it would
    // otherwise rerun the script, and rebuild, on the next build).
    let index = PathBuf::from(git(&wt, &["rev-parse", "--git-path", "index"]));
    let index = if index.is_absolute() {
        index
    } else {
        wt.join(index)
    };
    let mtime = || std::fs::metadata(&index).unwrap().modified().unwrap();
    // A tracked file with a newer mtime and the same content: a `git status`
    // that may take locks would refresh (rewrite) the index for it. Move the
    // branch without touching the index, so that the script reruns.
    let tracked = wt.join(CRATE).join("src/main.rs");
    std::fs::write(&tracked, std::fs::read(&tracked).unwrap()).unwrap();
    let before = mtime();
    git(&wt, &["update-ref", "refs/heads/wt", &second]);
    assert_eq!(stamp(&wt, &[]), second);
    assert_eq!(mtime(), before, "the build rewrote the index");

    // A staged change makes it dirty; committing it makes it clean again.
    std::fs::write(wt.join("note.txt"), "x\n").unwrap();
    git(&wt, &["add", "note.txt"]);
    assert_eq!(stamp(&wt, &[]), format!("{second}-dirty"));
    git(&wt, &["commit", "-q", "-m", "three"]);
    assert_eq!(stamp(&wt, &[]), head(&wt), "clean after the commit");
}

/// Test step 3: the same in the main checkout of the repository.
#[test]
fn ac1_main_checkout_follows_commits() {
    let (main, _wt) = repo_and_worktree("t181-main");
    assert!(main.join(".git").is_dir());
    assert_eq!(stamp(&main, &[]), head(&main));
    git(&main, &["commit", "-q", "--allow-empty", "-m", "two"]);
    assert_eq!(stamp(&main, &[]), head(&main));
    // The branch moves without touching HEAD or the index (as a fetch and
    // update-ref can do): only the ref file changes.
    let before = git(&main, &["rev-parse", "--short=10", "HEAD~1"]);
    git(&main, &["update-ref", "refs/heads/main", "HEAD~1"]);
    assert_eq!(stamp(&main, &[]), before, "the ref moved under HEAD");
    // A commit in the worktree moves another branch: the main checkout's
    // stamp stays right.
    let wt = main.parent().unwrap().join("a work tree");
    git(&wt, &["commit", "-q", "--allow-empty", "-m", "elsewhere"]);
    assert_eq!(stamp(&main, &[]), head(&main));
}

/// Corner case: a detached HEAD has no ref; HEAD itself moves. The second
/// move touches only HEAD (not the index), so it is HEAD's watch that sees it.
#[test]
fn corner_detached_head() {
    let (_main, wt) = repo_and_worktree("t181-detached");
    git(&wt, &["commit", "-q", "--allow-empty", "-m", "two"]);
    git(&wt, &["checkout", "-q", "--detach"]);
    assert_eq!(stamp(&wt, &[]), head(&wt));
    git(&wt, &["commit", "-q", "--allow-empty", "-m", "detached"]);
    assert_eq!(stamp(&wt, &[]), head(&wt));
    // With no HEAD reflog (removed, and not created again with reflogs off)
    // only HEAD itself changes.
    let log = PathBuf::from(git(&wt, &["rev-parse", "--git-path", "logs/HEAD"]));
    let log = if log.is_absolute() { log } else { wt.join(log) };
    std::fs::remove_file(&log).unwrap();
    assert_eq!(stamp(&wt, &[]), head(&wt));
    let back = git(&wt, &["rev-parse", "--short=10", "HEAD~2"]);
    git(
        &wt,
        &[
            "-c",
            "core.logAllRefUpdates=false",
            "update-ref",
            "--no-deref",
            "HEAD",
            "HEAD~2",
        ],
    );
    assert_eq!(stamp(&wt, &[]), back, "HEAD moved, the index did not");
    assert!(!log.exists(), "the HEAD reflog stayed off");
}

/// Corner case: with the refs packed, a nested branch (`feat/x`) lives only
/// in `packed-refs` and git prunes its empty directory; moving it without
/// touching the index (update-ref, as a fetch or a `reset --soft` would) is
/// seen through its reflog.
#[test]
fn corner_packed_nested_branch() {
    let (main, _wt) = repo_and_worktree("t181-packed");
    let wt = main.parent().unwrap().join("packed tree");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/x",
            wt.to_str().unwrap(),
        ],
    );
    git(&wt, &["commit", "-q", "--allow-empty", "-m", "two"]);
    assert_eq!(stamp(&wt, &[]), head(&wt));
    git(&main, &["pack-refs", "--all"]);
    let common = PathBuf::from(git(&wt, &["rev-parse", "--git-common-dir"]));
    let common = if common.is_absolute() {
        common
    } else {
        wt.join(common)
    };
    assert!(
        !common.join("refs/heads/feat").exists(),
        "the ref is only packed now, its directory pruned"
    );
    assert_eq!(stamp(&wt, &[]), head(&wt));
    // Moved from the main checkout (as a fetch there would): the worktree's
    // HEAD reflog and index do not change, only the branch's own reflog.
    let back = git(&wt, &["rev-parse", "--short=10", "HEAD~1"]);
    git(&main, &["update-ref", "refs/heads/feat/x", "feat/x~1"]);
    assert_eq!(stamp(&wt, &[]), back, "the packed ref moved");
    git(&wt, &["commit", "-q", "--allow-empty", "-m", "three"]);
    assert_eq!(stamp(&wt, &[]), head(&wt));
}

/// Corner case: a reftable repository (`git init --ref-format=reftable`,
/// Git 2.45+) keeps every ref in `reftable/`; a `reset --soft` moves the
/// branch without touching the index. Skipped, saying so, on an older git.
#[test]
fn corner_reftable_repository() {
    let root = scratch("t181-reftable");
    let repo = root.join("repo");
    write_crate(&repo.join(CRATE));
    std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    let init = Command::new("git")
        .args(["init", "-q", "--ref-format=reftable", "-b", "main"])
        .current_dir(&repo)
        .output()
        .expect("git");
    if !init.status.success() {
        eprintln!(
            "t-181: this git has no reftable support, case not run: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        return;
    }
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "two"]);
    assert_eq!(stamp(&repo, &[]), head(&repo));
    let back = git(&repo, &["rev-parse", "--short=10", "HEAD~1"]);
    git(&repo, &["reset", "-q", "--soft", "HEAD~1"]);
    assert_eq!(stamp(&repo, &[]), back, "the ref moved in reftable/");
}

/// AC3: the override wins, in git or not; outside git the stamp is
/// `unknown`; a `git` that fails is the same fallback, with no panic.
#[test]
fn ac3_override_outside_git_and_failing_git() {
    let (_main, wt) = repo_and_worktree("t181-override");
    assert_eq!(stamp(&wt, &[("TIZA_GIT_SHA_OVERRIDE", "v9.9.9")]), "v9.9.9");
    assert_eq!(stamp(&wt, &[]), head(&wt), "back to git without it");

    let loose = scratch("t181-nogit");
    write_crate(&loose.join(CRATE));
    assert_eq!(stamp(&loose, &[]), "unknown");
    assert_eq!(
        stamp(&loose, &[("TIZA_GIT_SHA_OVERRIDE", "v1.0.0")]),
        "v1.0.0"
    );

    // A `git` first on PATH that always fails (as a missing one would).
    let fake = scratch("t181-fakegit");
    let script = fake.join("git");
    std::fs::write(&script, "#!/bin/sh\nexit 127\n").unwrap();
    let mut perm = std::fs::metadata(&script).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    std::fs::set_permissions(&script, perm).unwrap();
    let path = format!(
        "{}:{}",
        fake.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let broken = scratch("t181-brokengit");
    write_crate(&broken.join(CRATE));
    assert_eq!(stamp(&broken, &[("PATH", &path)]), "unknown");
}

/// AC1: the project's own build script watches the files git names for this
/// checkout (a worktree here or the main checkout), never `../../.git/…`.
#[test]
fn ac1_project_build_script_watches_git_paths() {
    let src = build_rs();
    assert!(!src.contains("../../.git"), "the old fixed paths are gone");
    for needle in [
        "--git-path",
        "--symbolic-full-name",
        "packed-refs",
        "logs/",
        "reftable",
        "index",
        "GIT_OPTIONAL_LOCKS",
        "TIZA_GIT_SHA_OVERRIDE",
    ] {
        assert!(src.contains(needle), "build.rs lacks {needle}");
    }
}
