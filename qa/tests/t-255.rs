//! A privacy check of 2026-10-25 found five places where Tiza touched
//! more than its own files, or wrote to a terminal that did not ask.
//!
//! Acceptance criteria:
//! - AC1 `:panic` is only in a build with `test-hooks`. The guard in t-166
//!   (`v2_the_test_hooks_are_never_in_a_release_build`) now reads every
//!   test hook of every crate.
//! - AC2 Tiza never removes a file, a folder or a link at the socket path
//!   (a mistyped `--socket`, a stray `TIZA_SOCKET`): it refuses the path
//!   and says so. It replaces only an orphaned socket.
//! - AC3 Tiza changes permissions only of its own runtime folder (`tiza/`
//!   in `$XDG_RUNTIME_DIR`, `$TMPDIR` or `/tmp`): it makes the folder with
//!   0700, refuses a link or another user's folder there, and makes wider
//!   permissions 0700. For a socket path that the user gives, it makes
//!   only the missing folders, each with 0700, and leaves a folder that is
//!   there as it is. A refused runtime folder does not lose a `panics.log`
//!   line: the line goes to stderr when Tiza quits, never over the screen.
//! - AC4 The colour query after a resize honours `TIZA_NO_OSC` and goes to
//!   the terminal that the keys come from, as the query at start does.
//! - AC5 An empty `HOME` counts as not set: Tiza reads no config file from
//!   the current folder. A `~` in `:w`, `:saveas`, `:wq` or `:export`
//!   without `HOME` is an error, and nothing is written.
//!
//! Corner cases: another user's runtime folder needs a second user (root,
//! `chown`), which the tests do not have; a link stands in for it, on the
//! same path through the code, and the owner check runs with another user
//! id. Socket paths stay short (short scratch
//! names, one-letter folders): macOS refuses one of 104 bytes or more.

use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Command;

use tiza_qa::{Live, bin, scratch};

fn mode(p: &Path) -> u32 {
    std::fs::symlink_metadata(p).unwrap().permissions().mode() & 0o777
}

fn chmod(p: &Path, m: u32) {
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap();
}

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

/// Runs `tiza --smoke --no-save ARGS main.rs` (one frame, then exit) on a
/// pty in `dir`, with `env` on top of an environment without
/// `TIZA_SOCKET` or `TIZA_SESSION`, and returns the frame as text.
fn smoke(dir: &Path, name: &str, args: &[&str], env: &[(&str, &str)]) -> String {
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let env: std::collections::HashMap<_, _> = env.iter().copied().collect();
    let body = r##"
import json
binary, cwd, args, extra = sys.argv[1], sys.argv[2], json.loads(sys.argv[3]), json.loads(sys.argv[4])
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
for k in ("TIZA_SOCKET", "TIZA_SESSION", "TIZA_TEST_HOOK"):
    env.pop(k, None)
env.update(extra)
s = Pty([binary, "--smoke", "--no-save", *args, "main.rs"], 30, 300, env=env)
print("EXIT", s.finish(10))
print("SCREEN", json.dumps(plain(s.out)))
"##;
    let out = tiza_qa::pty_script(
        dir,
        name,
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &serde_json::to_string(args).unwrap(),
            &serde_json::to_string(&env).unwrap(),
        ],
    );
    assert!(out.contains("EXIT 0"), "{out}");
    let screen = out.lines().find_map(|l| l.strip_prefix("SCREEN ")).unwrap();
    serde_json::from_str(screen).unwrap()
}

// ---------------------------------------------------------------- AC2

#[test]
fn ac2_a_file_a_folder_or_a_link_at_the_socket_path_is_never_removed() {
    let dir = scratch("t255-not-a-socket");
    let notes = dir.join("notes.txt");
    std::fs::write(&notes, "my notes\n").unwrap();
    let folder = dir.join("a-folder");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("inside.txt"), "x").unwrap();
    let link = dir.join("a-link");
    std::os::unix::fs::symlink(&notes, &link).unwrap();
    for path in [&notes, &folder, &link] {
        let err = match tiza_tui::ipc::start(path) {
            Ok(_) => panic!("{} was taken as a socket", path.display()),
            Err(e) => e.to_string(),
        };
        assert!(
            err.contains("is there and is not a socket: Tiza does not remove it"),
            "{err}"
        );
    }
    assert_eq!(std::fs::read_to_string(&notes).unwrap(), "my notes\n");
    assert!(folder.join("inside.txt").is_file());
    assert_eq!(std::fs::read_link(&link).unwrap(), notes);
}

#[test]
fn ac2_a_file_that_took_the_socket_path_stays_when_tiza_quits() {
    let dir = scratch("t255-took");
    let path = dir.join("s.sock");
    let server = tiza_tui::ipc::start(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, "mine\n").unwrap();
    drop(server);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine\n");
}

#[test]
fn ac2_an_orphaned_socket_is_replaced() {
    let dir = scratch("t255-orphan");
    let path = dir.join("old.sock");
    // Bound by a process that then exits: a listener of this process could
    // live on for a moment in a child that another test forks.
    let bound = Command::new("python3")
        .args([
            "-I",
            "-c",
            "import socket, sys; socket.socket(socket.AF_UNIX).bind(sys.argv[1])",
        ])
        .arg(&path)
        .status()
        .unwrap();
    assert!(bound.success());
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_socket(),
        "the socket file stays after its process"
    );
    let server = tiza_tui::ipc::start(&path).unwrap();
    assert!(UnixStream::connect(&server.path).is_ok());
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn ac2_the_binary_keeps_the_file_that_socket_or_tiza_socket_names() {
    let dir = scratch("t255-binary-socket");
    let notes = dir.join("notes.txt");
    for (name, args, env) in [
        ("flag", vec!["--socket", "notes.txt"], vec![]),
        ("env", vec![], vec![("TIZA_SOCKET", "notes.txt")]),
    ] {
        std::fs::write(&notes, "my notes\n").unwrap();
        let screen = smoke(&dir, &format!("{name}.py"), &args, &env);
        assert!(
            screen.contains("local API disabled: notes.txt is there and is not a socket"),
            "{name}: {screen}"
        );
        assert_eq!(
            std::fs::read_to_string(&notes).unwrap(),
            "my notes\n",
            "{name}"
        );
    }
}

#[test]
fn ac2_a_client_command_leaves_the_file_that_socket_names() {
    let dir = scratch("t255-client");
    let notes = dir.join("notes.txt");
    std::fs::write(&notes, "my notes\n").unwrap();
    chmod(&notes, 0o644);
    for args in [&["api", "status"][..], &["next"][..]] {
        let out = Command::new(bin())
            .arg("--socket")
            .arg(&notes)
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success(), "{args:?}: {out:?}");
        assert_eq!(
            std::fs::read_to_string(&notes).unwrap(),
            "my notes\n",
            "{args:?}"
        );
        assert_eq!(mode(&notes), 0o644, "{args:?}");
    }
}

// ---------------------------------------------------------------- AC3

#[test]
fn ac3_a_given_socket_path_leaves_the_folders_that_are_there() {
    let dir = scratch("t255-g");
    let home = dir.join("h");
    std::fs::create_dir(&home).unwrap();
    chmod(&home, 0o755);
    // In a folder that is there: the socket, and the folder as it was.
    let server = tiza_tui::ipc::start(&home.join("x.sock")).unwrap();
    assert_eq!(mode(&home), 0o755, "a folder that is there keeps its mode");
    assert_eq!(mode(&server.path), 0o600);
    drop(server);
    // Missing folders: each made with 0700, the one above them untouched.
    let server = tiza_tui::ipc::start(&home.join("n/d/y.sock")).unwrap();
    assert_eq!(mode(&home.join("n")), 0o700);
    assert_eq!(mode(&home.join("n/d")), 0o700);
    assert_eq!(mode(&home), 0o755);
    drop(server);
}

#[test]
fn ac3_the_runtime_folder_is_made_private_and_a_link_there_is_refused() {
    let dir = scratch("t255-rt");
    // Missing: made with 0700 (and the default socket in it); the folder
    // above it keeps its mode.
    let rt = dir.join("m");
    std::fs::create_dir(&rt).unwrap();
    chmod(&rt, 0o755);
    let screen = smoke(
        &dir,
        "m.py",
        &[],
        &[("XDG_RUNTIME_DIR", rt.to_str().unwrap())],
    );
    assert!(!screen.contains("local API disabled"), "{screen}");
    assert_eq!(mode(&rt.join("tiza")), 0o700);
    assert_eq!(mode(&rt), 0o755);
    // There with wider permissions: made 0700.
    let rt = dir.join("w");
    std::fs::create_dir_all(rt.join("tiza")).unwrap();
    chmod(&rt.join("tiza"), 0o777);
    let screen = smoke(
        &dir,
        "w.py",
        &[],
        &[("XDG_RUNTIME_DIR", rt.to_str().unwrap())],
    );
    assert!(!screen.contains("local API disabled"), "{screen}");
    assert_eq!(mode(&rt.join("tiza")), 0o700);
    // A link there (as another user's folder would be): refused, and the
    // folder that it points to keeps its mode and stays empty.
    let rt = dir.join("l");
    std::fs::create_dir(&rt).unwrap();
    let elsewhere = dir.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    chmod(&elsewhere, 0o755);
    std::os::unix::fs::symlink(&elsewhere, rt.join("tiza")).unwrap();
    let screen = smoke(
        &dir,
        "l.py",
        &[],
        &[("XDG_RUNTIME_DIR", rt.to_str().unwrap())],
    );
    assert!(
        screen.contains("local API disabled: Tiza does not use")
            && screen.contains("tiza: it is not a folder"),
        "{screen}"
    );
    assert_eq!(mode(&elsewhere), 0o755);
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
}

#[test]
fn ac3_another_users_runtime_folder_is_refused_with_the_way_out() {
    let dir = scratch("t255-owner");
    let meta = std::fs::symlink_metadata(&dir).unwrap();
    assert!(tiza_tui::runtime::check_own(&dir, &meta, meta.uid()).is_ok());
    let err = tiza_tui::runtime::check_own(&dir, &meta, meta.uid().wrapping_add(1)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    // The message names the folder, why, and the ways out.
    let msg = err.to_string();
    for part in [
        &dir.display().to_string()[..],
        "another user",
        "XDG_RUNTIME_DIR",
        "TMPDIR",
        "--socket",
    ] {
        assert!(msg.contains(part), "no {part:?} in {msg:?}");
    }
}

#[test]
fn ac3_a_refused_runtime_folder_sends_the_panics_log_line_to_stderr() {
    // The runtime folder is a link (refused); the socket is elsewhere, so a
    // request still comes in. A request that panics (the test hook) has its
    // line on stderr, with why it is not in panics.log, once Tiza has left
    // the alternate screen: never over the TUI.
    let dir = scratch("t255-log-line");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let rt = dir.join("rt");
    std::fs::create_dir(&rt).unwrap();
    let elsewhere = dir.join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, rt.join("tiza")).unwrap();
    let body = r##"
import json
binary, cwd, rt = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", XDG_RUNTIME_DIR=rt,
           TIZA_TEST_HOOK="request:status")
env.pop("TIZA_SOCKET", None)
s = Pty([binary, "--no-save", "--socket", "s.sock", "main.rs"], 30, 120, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    # The reply comes after the line was logged (or held).
    r = subprocess.run([binary, "api", "status", "--socket", "s.sock"], capture_output=True, text=True, timeout=10)
    print("REPLY", json.dumps(r.stdout + r.stderr))
    s.send(b":q\r")
print("EXIT", s.finish(10))
out = s.out
left = out.rfind(b"\x1b[?1049l")
print("BEFORE", json.dumps(plain(out[:left]) if left >= 0 else ""))
print("AFTER", json.dumps(plain(out[left:]) if left >= 0 else ""))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "log_line.py",
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            rt.to_str().unwrap(),
        ],
    );
    let part = |key: &str| -> String {
        let line = out.lines().find_map(|l| l.strip_prefix(key));
        serde_json::from_str(line.unwrap_or_else(|| panic!("no {key}in:\n{out}"))).unwrap()
    };
    assert!(out.contains("EXIT 0"), "{out}");
    assert!(part("REPLY ").contains("a bug in Tiza"), "{out}");
    let (before, after) = (part("BEFORE "), part("AFTER "));
    assert!(
        !before.contains("panicked at"),
        "the line was written over the TUI: {before}"
    );
    assert!(
        after.contains("tiza: status panicked at")
            && after.contains("(not written to panics.log: Tiza does not use"),
        "{out}"
    );
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
}

// ---------------------------------------------------------------- AC4

// ---------------------------------------------------------------- AC5

#[test]
fn ac5_an_empty_home_reads_no_config_from_the_current_folder() {
    let dir = scratch("t255-config");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    std::fs::create_dir_all(dir.join(".config/tiza")).unwrap();
    std::fs::write(dir.join(".config/tiza/config.toml"), "not toml [").unwrap();
    let render = |home: &str| {
        let out = Command::new(bin())
            .args(["render", "main.rs"])
            .current_dir(&dir)
            .env("HOME", home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("TIZA_CONFIG")
            .output()
            .unwrap();
        assert!(out.status.success(), "HOME={home:?}: {out:?}");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    // With HOME at that folder, Tiza reads the file (and says that it is
    // not valid): the file is where an empty HOME used to lead.
    let read = render(dir.to_str().unwrap());
    assert!(read.contains("config ignored"), "{read}");
    let empty = render("");
    assert!(!empty.contains("config ignored"), "{empty}");
}

/// Set for the child copy of this test binary that
/// `ac5_a_tilde_without_home_is_an_error_and_writes_nothing` runs.
const TILDE_CHILD: &str = "TIZA_QA_TILDE_CHILD";

/// Runs in a child copy of this test binary, with `HOME` empty or not set
/// and the current folder of its own: both belong to the whole process,
/// and the other tests here run at the same time.
#[test]
fn ac5_a_tilde_without_home_is_an_error_and_writes_nothing() {
    for (case, home) in [("empty", Some("")), ("unset", None)] {
        let dir = scratch(&format!("t255-tilde-{case}"));
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args(["ac5_child_tilde_commands", "--exact", "--nocapture"])
            .current_dir(&dir)
            .env(TILDE_CHILD, "1");
        match home {
            Some(h) => child.env("HOME", h),
            None => child.env_remove("HOME"),
        };
        let out = child.output().unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.status.success() && text.contains("TILDE CHILD DONE"),
            "{case}: {text}"
        );
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(
            left.is_empty(),
            "{case}: written to the current folder: {left:?}"
        );
    }
}

/// The child of the test above; does nothing in a normal run.
#[test]
fn ac5_child_tilde_commands() {
    if std::env::var_os(TILDE_CHILD).is_none() {
        return;
    }
    let live = Live::from_code(CODE, "rs", "t255-tilde-live");
    let save = live.with(|app| app.save_path.clone());
    for (cmd, path) in [
        ("saveas ~/s.tiza", "~/s.tiza"),
        ("export ~/e.txt", "~/e.txt"),
        ("w ~/w.tiza", "~/w.tiza"),
        ("wq ~/q.tiza", "~/q.tiza"),
    ] {
        live.command(cmd);
        assert_eq!(
            live.message(),
            Some((
                format!("{path}: ~ needs HOME, and HOME is empty or not set. Give the full path"),
                true
            )),
            "{cmd}"
        );
    }
    assert_eq!(live.with(|app| app.save_path.clone()), save);
    assert!(!live.with(|app| app.quit), ":wq quit");
    println!("TILDE CHILD DONE");
}
