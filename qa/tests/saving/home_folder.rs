//! An empty or missing `HOME`: no config from the current folder, and a `~`
//! path is an error that writes nothing.

use std::process::Command;

use tiza_qa::{Live, scratch};

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

// ---------------------------------------------------------------- AC5

#[test]
fn an_empty_home_reads_no_config_from_the_current_folder() {
    let dir = scratch("config");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    std::fs::create_dir_all(dir.join(".config/tiza")).unwrap();
    std::fs::write(dir.join(".config/tiza/config.toml"), "not toml [").unwrap();
    let render = |home: &str| {
        let out = tiza_qa::command()
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
    // With HOME at that folder, Tiza reads the file (and says where it is
    // not valid): the file is where an empty HOME used to lead.
    let read = render(dir.to_str().unwrap());
    assert!(
        read.contains("tiza: config: ") && read.contains("config.toml:1:"),
        "{read}"
    );
    let empty = render("");
    assert!(!empty.contains("tiza: config"), "{empty}");
}

/// Set for the child copy of this test binary that
/// `a_tilde_without_home_is_an_error_and_writes_nothing` runs.
const TILDE_CHILD: &str = "TIZA_QA_TILDE_CHILD";

/// Runs in a child copy of this test binary, with `HOME` empty or not set
/// and the current folder of its own: both belong to the whole process,
/// and the other tests here run at the same time.
#[test]
fn a_tilde_without_home_is_an_error_and_writes_nothing() {
    for (case, home) in [("empty", Some("")), ("unset", None)] {
        let dir = scratch(&format!("tilde-{case}"));
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "home_folder::child_tilde_commands",
                "--exact",
                "--nocapture",
            ])
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
fn child_tilde_commands() {
    if std::env::var_os(TILDE_CHILD).is_none() {
        return;
    }
    let live = Live::from_code(CODE, "rs", "tilde-live");
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
