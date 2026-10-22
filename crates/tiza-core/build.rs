//! Records the git commit the binary was built from, so `tiza --version`
//! and the MCP `status` tool can tell a stale build from a fresh one.
//!
//! `TIZA_GIT_SHA_OVERRIDE` (non-empty, trimmed) is used instead, for builds
//! that know better (a release from a tarball). Otherwise the stamp is `git
//! rev-parse --short=10 HEAD`, plus `-dirty` when tracked files differed
//! from it when the stamp was computed (a staged change, say; an unstaged
//! edit alone does not rerun this script), or `unknown` outside git or
//! without a working `git`.
//!
//! Only a repository that tracks this package names its commit: a crate
//! unpacked from crates.io under a directory that happens to be a git
//! repository (a home kept in git, say) or verified by `cargo package` in
//! the ignored `target/package/` is stamped `unknown`, not with a commit
//! that did not build it. Its version already says what it is.
//!
//! The script reruns when the commit can have moved (t-181). The files to
//! watch come from git itself, because in a worktree `.git` is a file and
//! HEAD, the index and the branch refs live elsewhere (`--git-path` maps
//! each name to this checkout's or the common dir): HEAD and its reflog
//! (a detached HEAD moves there), the index, the ref HEAD points to, its
//! reflog (still there, and only touched by this branch, while the ref
//! itself sits in `packed-refs`), `packed-refs`, and `reftable/` in a
//! reftable repository.

use std::path::PathBuf;
use std::process::Command;

/// `git ARGS` in the package directory, without its trailing newline;
/// `None` on any failure. `GIT_OPTIONAL_LOCKS=0` keeps `git status` from
/// refreshing the index, which would rerun this script on the next build.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout)
        .trim_end_matches(['\n', '\r'])
        .to_string();
    (!text.is_empty()).then_some(text)
}

/// The absolute path git uses for NAME in this checkout (`--git-path`
/// answers relative to the current directory in a plain checkout).
fn git_path(name: &str) -> Option<PathBuf> {
    let p = PathBuf::from(git(&["rev-parse", "--git-path", name])?);
    if p.is_absolute() {
        Some(p)
    } else {
        Some(std::env::current_dir().ok()?.join(p))
    }
}

/// The files whose change can move the commit or the dirty flag.
fn watched() -> Vec<PathBuf> {
    let mut names = vec![
        "HEAD".to_string(),
        "logs/HEAD".to_string(),
        "index".to_string(),
        "packed-refs".to_string(),
        "reftable".to_string(),
    ];
    // `refs/heads/NAME` on a branch, `HEAD` when detached.
    if let Some(r) =
        git(&["rev-parse", "--symbolic-full-name", "HEAD"]).filter(|r| r.starts_with("refs/"))
    {
        names.push(format!("logs/{r}"));
        names.push(r);
    }
    let mut paths: Vec<PathBuf> = names.iter().filter_map(|n| git_path(n)).collect();
    // A path that does not exist makes cargo rerun on every build.
    paths.retain(|p| p.exists());
    paths
}

/// Whether the git repository around the package directory tracks this
/// build script, that is, whether its commit is the source being built.
fn tracked_here() -> bool {
    git(&["ls-files", "--error-unmatch", "--", "build.rs"]).is_some()
}

fn stamp() -> String {
    let Some(sha) = git(&["rev-parse", "--short=10", "HEAD"]) else {
        return "unknown".into();
    };
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some();
    format!("{sha}{}", if dirty { "-dirty" } else { "" })
}

fn main() {
    println!("cargo:rerun-if-env-changed=TIZA_GIT_SHA_OVERRIDE");
    let sha = match std::env::var("TIZA_GIT_SHA_OVERRIDE") {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ if tracked_here() => {
            for p in watched() {
                println!("cargo:rerun-if-changed={}", p.display());
            }
            stamp()
        }
        _ => "unknown".into(),
    };
    println!("cargo:rustc-env=TIZA_GIT_SHA={sha}");
}
