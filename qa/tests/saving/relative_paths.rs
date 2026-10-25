//! `:w PATH`, `:saveas PATH` and `:export PATH` took a relative path from
//! the folder where Tiza started, but nothing said so, and their messages
//! named the path as it was typed: the presenter could not tell where the
//! file went. `:export` did not create a missing folder, and `~` was a
//! folder named `~`.
//!
//! Acceptance criteria:
//! - AC1 README, the help and the agent guide say it in STE: a relative
//!   path starts at the folder where you started Tiza; Tiza creates the
//!   folders that are missing.
//! - AC2 The message shows the full path written.
//! - AC3 `:export` does the same as `:w` and `:saveas`.
//! - AC4 This file; CHANGELOG (Changed).
//!
//! Corner cases: `~` at the start is the home folder; a folder that cannot
//! be created is named in the error.

use tiza_qa::{bin, scratch};

#[test]
fn relative_paths_start_at_the_folder_where_tiza_started() {
    let dir = scratch("relative_paths");
    let work = dir.join("work");
    let start = dir.join("start");
    let home = dir.join("home");
    for d in [&work, &start, &home] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(work.join("main.rs"), "fn main() {\n    let a = 1;\n}\n").unwrap();
    // A file where a folder should be: it cannot be created.
    std::fs::write(start.join("blocker"), "").unwrap();
    let body = r##"
import json
binary, start, home = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(start)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", HOME=home)
env.pop("TIZA_TEST_HOOK", None)
s = Pty([binary, "--socket", "s.sock", "../work/main.rs"], 30, 160, env=env)

def bar():
    r = subprocess.run([binary, "--socket", "s.sock", "api", "follow.status", "{}"],
                       capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout).get("bar")

def run(cmd):
    s.send(("\x1b:" + cmd + "\r").encode())
    end = time.monotonic() + 10
    while time.monotonic() < end:
        b = bar()
        if b and b.split()[0] in ("wrote", "saved", "exported", "could", "cannot"):
            return b
        time.sleep(0.05)
    return bar()

if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    for name, cmd in [("W", "w copy.tiza"), ("SAVEAS", "saveas ./a/b.tiza"),
                      ("EXPORT", "export out/shot.txt"), ("HOME", "w ~/h.tiza"),
                      ("BLOCKED", "w blocker/x.tiza")]:
        print(name, json.dumps(run(cmd)), flush=True)
        # A message clears the bar for the next one.
        api = subprocess.run([binary, "--socket", "s.sock", "api", "status", "{}"],
                             capture_output=True, text=True, timeout=10)
    s.send(b":q!\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "paths.py",
        body,
        &[
            bin().to_str().unwrap(),
            start.to_str().unwrap(),
            home.to_str().unwrap(),
        ],
    );
    let said = |name: &str| -> String {
        let l = out
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{name} ")))
            .unwrap_or_else(|| panic!("no {name}\n{out}"));
        serde_json::from_str::<Option<String>>(l)
            .unwrap()
            .unwrap_or_default()
    };
    let start = start.canonicalize().unwrap();
    let real = |p: &std::path::Path| p.canonicalize().unwrap().display().to_string();
    // `:w copy.tiza`: in the folder where Tiza started, named in full.
    assert!(start.join("copy.tiza").exists(), "{out}");
    assert!(
        said("W").starts_with("wrote a copy to /")
            && said("W").contains(&real(&start.join("copy.tiza"))),
        "{out}"
    );
    // `:saveas ./a/b.tiza`: the folder `a` is created.
    assert!(start.join("a/b.tiza").exists(), "{out}");
    assert!(
        said("SAVEAS").contains(&real(&start.join("a/b.tiza"))),
        "{out}"
    );
    // `:export out/shot.txt`: the same, folder included.
    assert!(start.join("out/shot.txt").exists(), "{out}");
    assert!(
        said("EXPORT").contains(&real(&start.join("out/shot.txt"))),
        "{out}"
    );
    // `~` is the home folder, not a folder named `~`.
    assert!(home.join("h.tiza").exists(), "{out}");
    assert!(!start.join("~").exists(), "{out}");
    // A folder that cannot be created is named.
    let blocked = said("BLOCKED");
    assert!(
        blocked.starts_with("could not create the folder ")
            && blocked.contains(&real(&start.join("blocker"))),
        "{out}"
    );
    assert!(out.contains("EXIT 0"), "{out}");
}
