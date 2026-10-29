//! A file of a newer Tiza on the real TUI: the moves that this version
//! skips and the values that fall back to their default are named on
//! stderr before the TUI starts (so they are on the terminal after it ends,
//! also in presentation mode), and the bar says how many there are, with
//! the effects that this version does not know, also before another
//! warning of the load. In presentation mode the last row says it for a
//! few seconds.

use tiza_qa::{fixture, scratch};

#[test]
fn moves_that_are_not_known_are_named_on_stderr_and_counted_on_the_bar() {
    let dir = scratch("newer-moves-open");
    // The compat file of moves, with one effect that is not known too, and
    // a named step past the last step: its warning takes the bar, and the
    // count stays before it.
    let mut doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture("compat/v1-newer-unknown-move.tiza")).unwrap(),
    )
    .unwrap();
    doc["annotations"][0]["effect"] = "slide".into();
    doc["steps"] = serde_json::json!([{"step": 9, "id": "nine"}]);
    std::fs::write(dir.join("newer.tiza"), doc.to_string()).unwrap();
    let body = r##"
binary, cwd, present = sys.argv[1], sys.argv[2], sys.argv[3] == "present"
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
argv = [binary, "--no-save", "--no-socket", "--theme", "dark"] + (["--present"] if present else []) + ["newer.tiza"]
s = Pty(argv, 30, 120, env=env)
s.wait_for(lambda out: b"\x1b[?1049h" in out, 10, "first frame")
s.wait_for(lambda out: b"see tiza check" in out, 10, "the bar or the last row")
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
sys.stdout.flush()
out = s.out.decode("utf-8", "replace")
cut = out.find("\x1b[?1049h")
print("BEFORE", repr(out[:cut]), flush=True)
print("SCREEN", repr(out[cut:]), flush=True)
"##;
    for present in [false, true] {
        let out = tiza_qa::pty_script(
            &dir,
            "newer.py",
            body,
            &[
                tiza_qa::bin().to_str().unwrap(),
                dir.to_str().unwrap(),
                if present { "present" } else { "normal" },
            ],
        );
        let before = out.lines().find_map(|l| l.strip_prefix("BEFORE ")).unwrap();
        let screen = out.lines().find_map(|l| l.strip_prefix("SCREEN ")).unwrap();
        for named in [
            "1 annotation has an effect that this version of Tiza does not know",
            "Tiza skips 3 moves that this version does not know, and a save does not keep them: move #3 (step 2, action \"teleport\")",
            "move #4 (step 2, align \"middle\" → center)",
        ] {
            assert!(
                before.contains(named),
                "present {present}: {named}: {before}"
            );
        }
        // On the bar, or on the last row for a few seconds in presentation
        // mode.
        assert!(
            screen.contains("1 effect, 3 moves and 2 values are not known: see tiza check"),
            "present {present}: {screen}"
        );
        // The warning of the named step is on the bar too, after the count.
        assert!(present || screen.contains("nine"), "{screen}");
    }
}
