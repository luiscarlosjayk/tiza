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

use std::process::Command;

use tiza_qa::{Live, bin, scratch};

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

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
