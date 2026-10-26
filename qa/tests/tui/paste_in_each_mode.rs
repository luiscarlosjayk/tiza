//! Where a bracketed paste goes: the command line takes only the first line
//! of the paste, also when the terminal ends a pasted line with CR alone
//! (on a pty); a paste while the help, the list or a panel filter is open,
//! or in presentation mode, changes nothing and says how to paste.

use crossterm::event::KeyCode;
use serde_json::Value;
use tiza_qa::{Live, scratch};

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    println!(\"{a} {b}\");\n}\n";

#[test]
fn the_command_line_takes_the_first_line_of_a_paste_with_cr_alone() {
    let dir = scratch("paste-cr");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    // xterm and others send a pasted line end as CR. The second line must
    // not become the title of the box.
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--no-save", "--socket", "s.sock", "main.rs"], 30, 120, env=env)

def annotations():
    r = subprocess.run([binary, "--socket", "s.sock", "api", "scene.get", "{}"],
                       capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout)["document"]["annotations"] if r.returncode == 0 else []

def step(data, what):
    mark = len(s.out)
    s.send(data)
    return s.wait_for(lambda out: what in plain(out[mark:]), 10, repr(what))

if (s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
        and step(b":", ":")
        and step(b"\x1b[200~box 1-2\rnote 3 hi\x1b[201~", "1-2")
        and step(b"\r", "a1")):
    end = time.monotonic() + 10
    anns = annotations()
    while not anns and time.monotonic() < end:
        s.settle(0.05)
        anns = annotations()
    print("ANNS", json.dumps(anns), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "paste-cr.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    let anns: Value = out
        .lines()
        .find_map(|l| l.strip_prefix("ANNS "))
        .map(|j| serde_json::from_str(j).unwrap())
        .unwrap_or_else(|| panic!("{out}"));
    assert_eq!(anns.as_array().map(Vec::len), Some(1), "{out}");
    assert_eq!(anns[0]["type"], "box", "{anns}");
    assert!(anns[0].get("label").is_none_or(Value::is_null), "{anns}");
}

#[test]
fn a_paste_with_a_panel_open_or_in_presentation_changes_nothing() {
    let live = Live::from_code(CODE, "rs", "paste-blocked");
    let paste = "fn pasted() {}\n";
    let code = || live.with(|app| app.doc.code.clone());
    let before = code();
    for (keys, says) in [
        (&[KeyCode::Char('?')][..], "close the help with Esc"),
        (&[KeyCode::Char('/')][..], "close the help with Esc twice"),
        (
            &[KeyCode::Esc, KeyCode::Esc, KeyCode::Char('L')][..],
            "close the list with Esc",
        ),
        (&[KeyCode::Char('/')][..], "close the list with Esc twice"),
        (
            &[KeyCode::Esc, KeyCode::Esc, KeyCode::Char('P')][..],
            "ignored in presentation mode: press P to leave it",
        ),
    ] {
        live.keys(keys);
        let p = paste.to_string();
        live.with(move |app| app.paste(&p));
        let (message, error) = live.message().unwrap_or_default();
        assert!(
            error && message.starts_with("paste ignored") && message.contains(says),
            "{keys:?}: {message}"
        );
        assert_eq!(code(), before, "{keys:?}: the code changed");
    }
    // Out of presentation mode, the same paste replaces the code.
    live.keys(&[KeyCode::Char('P')]);
    let p = paste.to_string();
    live.with(move |app| app.paste(&p));
    assert_eq!(code(), ["fn pasted() {}"]);
}
