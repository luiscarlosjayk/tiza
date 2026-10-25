//! Theme bugs that the check of the themes page found.
//!
//! Acceptance criteria:
//! - AC1 With `--no-save`, `--theme-file` does not write the `.tiza`
//!   document at start: no new file, and a file that is there keeps its
//!   bytes. Without `--no-save` it writes the tokens at start, as before.
//! - AC2 A `--theme-file` on the command line is not an edit that `u`
//!   takes back, as for `--theme`: after `u` the tokens stay.
//!
//! Corner cases: `:theme file PATH` keeps its undo entry; `--theme-file`
//! still says how many tokens it applied.

use std::path::Path;

use tiza_qa::{bin, scratch};

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

/// Runs `tiza --smoke ARGS hello.rs` (one frame, then exit) on a pty in
/// `dir` and returns the frame as text.
fn smoke(dir: &Path, name: &str, args: &[&str]) -> String {
    let body = r##"
import json
binary, cwd, args = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--smoke", "--no-socket", *args, "hello.rs"], 30, 200, env=env)
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
        ],
    );
    assert!(out.contains("EXIT 0"), "{out}");
    let screen = out.lines().find_map(|l| l.strip_prefix("SCREEN ")).unwrap();
    serde_json::from_str(screen).unwrap()
}

#[test]
fn ac1_with_no_save_the_theme_file_writes_no_document() {
    let dir = scratch("t256-no-save");
    std::fs::write(dir.join("hello.rs"), CODE).unwrap();
    std::fs::write(dir.join("m.toml"), "note = \"#303030\"\n").unwrap();
    let sidecar = dir.join("hello.rs.tiza");
    let applied = "theme file m.toml: 1 token(s) applied";
    // No document yet: none after the start.
    let screen = smoke(&dir, "none.py", &["--no-save", "--theme-file", "m.toml"]);
    assert!(screen.contains(applied), "{screen}");
    assert!(!sidecar.exists(), "--no-save wrote {}", sidecar.display());
    // Without --no-save the tokens are written at start, as before: the
    // test sees a write when there is one.
    smoke(&dir, "save.py", &["--theme-file", "m.toml"]);
    let saved = std::fs::read(&sidecar).unwrap();
    assert!(
        String::from_utf8_lossy(&saved).contains("#303030"),
        "no tokens in the document"
    );
    let mtime = std::fs::metadata(&sidecar).unwrap().modified().unwrap();
    // A document that is there keeps its bytes, also with other tokens.
    std::fs::write(dir.join("m.toml"), "note = \"#404040\"\n").unwrap();
    let screen = smoke(&dir, "kept.py", &["--no-save", "--theme-file", "m.toml"]);
    assert!(screen.contains(applied), "{screen}");
    assert_eq!(std::fs::read(&sidecar).unwrap(), saved);
    assert_eq!(
        std::fs::metadata(&sidecar).unwrap().modified().unwrap(),
        mtime
    );
}

#[test]
fn ac2_undo_after_start_keeps_the_theme_file_tokens() {
    let dir = scratch("t256-undo");
    std::fs::write(dir.join("hello.rs"), CODE).unwrap();
    std::fs::write(dir.join("m.toml"), "note = \"#303030\"\n").unwrap();
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--no-save", "--socket", "s.sock", "--theme-file", "m.toml", "hello.rs"], 30, 120, env=env)
def scene():
    r = subprocess.run([binary, "api", "scene.get", "--socket", "s.sock"], capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout)["document"] if r.returncode == 0 else {}
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    # `u`, then a command whose effect the API shows: once it shows, the
    # `u` before it was handled.
    s.send(b"u:margin 40\r")
    end = time.monotonic() + 10
    doc = scene()
    while doc.get("margin") != 40 and time.monotonic() < end:
        s.settle(0.05)
        doc = scene()
    print("MARGIN", doc.get("margin"))
    print("TOKENS", json.dumps(doc.get("theme_overrides")))
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "undo.py",
        body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(out.contains("MARGIN 40"), "{out}");
    let tokens = out
        .lines()
        .find_map(|l| l.strip_prefix("TOKENS "))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(tokens.contains("#303030"), "after u: {tokens}\n{out}");
}
