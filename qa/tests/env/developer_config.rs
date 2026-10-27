//! The QA helpers never read the config file of whoever runs the tests:
//! not `~/.config/tiza/config.toml`, not `$XDG_CONFIG_HOME/tiza`, not an
//! exported `TIZA_CONFIG`. CI has no config file, so only this test can
//! see there that a helper lost the isolation.
// `set_var` is unsafe in Rust 2024.
#![allow(unsafe_code)]

use std::ffi::OsString;

use serde_json::json;
use tiza_qa::{Live, McpClient, fixture, pty_script, scratch, tiza, tiza_ok, tiza_with_config};

/// A config that changes every output below when Tiza reads it.
const HOSTILE: &str = "[defaults]\nnumbers = false\nwidth = 60\npad = \"3 9\"\n\
                       [theme]\nbg = \"#ff0000\"\n[styles.io]\nglyph = \"x\"\n";

#[test]
fn the_helpers_do_not_read_the_config_of_the_developer() {
    let _env = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("developer-config");
    let file = dir.join("file.rs");
    std::fs::copy(fixture("shared/personal_config/file.rs"), &file).unwrap();
    let f = file.to_str().unwrap().to_string();
    let bin = tiza_qa::bin();
    // What each way of starting Tiza gives. `bin()` built the binary first:
    // cargo needs the real home folder.
    let outputs = |when: &str| -> Vec<(&'static str, String)> {
        let check = tiza(&["check", &f, "--json"]);
        let pty = pty_script(
            &dir,
            &format!("probe-{when}.py"),
            "binary, path = sys.argv[1:3]\n\
             r = subprocess.run([binary, \"render\", path, \"--format\", \"txt\"],\n\
                                capture_output=True, text=True, timeout=10)\n\
             print(r.stdout, end=\"\")\n",
            &[bin.to_str().unwrap(), &f],
        );
        let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", "developer-mcp");
        let svg = dir.join(format!("mcp-{when}.svg"));
        let mut mcp = McpClient::start(&live.socket);
        let (err, text, _) = mcp.call("export_image", json!({"path": svg.to_str().unwrap()}));
        assert!(!err, "{text}");
        // A test that takes `TIZA_CONFIG` away still gets an empty config
        // folder.
        let no_file = tiza_qa::command()
            .env_remove("TIZA_CONFIG")
            .args(["render", &f, "--format", "txt"])
            .output()
            .unwrap();
        vec![
            ("tiza_ok", tiza_ok(&["render", &f, "--format", "txt"])),
            (
                "command() without TIZA_CONFIG",
                String::from_utf8_lossy(&no_file.stdout).into_owned(),
            ),
            ("tiza", String::from_utf8_lossy(&check.stdout).into_owned()),
            ("pty_script", pty),
            ("McpClient", std::fs::read_to_string(&svg).unwrap()),
        ]
    };
    let before = outputs("before");
    // The test can fail: Tiza that reads this config draws something else.
    std::fs::write(dir.join("hostile.toml"), HOSTILE).unwrap();
    let (_, read, _) = tiza_with_config(&dir, "hostile.toml", &["render", &f, "--format", "txt"]);
    assert_ne!(read, before[0].1, "the config must change a render");
    let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", "developer-mcp");
    let hostile = dir.join("hostile.toml");
    let mut mcp =
        McpClient::start_with_env(&live.socket, &[("TIZA_CONFIG", hostile.to_str().unwrap())]);
    let svg = dir.join("mcp-read.svg");
    let (err, text, _) = mcp.call("export_image", json!({"path": svg.to_str().unwrap()}));
    assert!(!err, "{text}");
    let mcp_before = &before.iter().find(|(w, _)| *w == "McpClient").unwrap().1;
    assert_ne!(
        &std::fs::read_to_string(&svg).unwrap(),
        mcp_before,
        "the config must change an export_image"
    );
    // A developer with a config in each place that Tiza looks.
    let home = dir.join("home");
    let xdg = dir.join("xdg");
    for folder in [home.join(".config/tiza"), xdg.join("tiza")] {
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("config.toml"), HOSTILE).unwrap();
    }
    let keys = ["HOME", "XDG_CONFIG_HOME", "TIZA_CONFIG"];
    let saved: Vec<Option<OsString>> = keys.iter().map(std::env::var_os).collect();
    // SAFETY: each test of this crate holds ENV_LOCK while it changes or
    // reads the environment, so no other thread uses it now.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_CONFIG_HOME", &xdg);
        std::env::set_var("TIZA_CONFIG", dir.join("hostile.toml"));
    }
    let after = outputs("after");
    // SAFETY: as above.
    unsafe {
        for (k, v) in keys.iter().zip(&saved) {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
    for ((what, b), (_, a)) in before.iter().zip(&after) {
        assert_eq!(a, b, "{what} read the config of the developer");
    }
}
