//! The notice on the bar when the TUI opens a document: an annotation is
//! outside the code only when its lines or columns are, and an annotation
//! that fails another check in the code (a mark kind that Tiza does not
//! know) sends to `tiza check`. See `render::open_notice` for stderr.

use tiza_qa::scratch;

/// The screen of the TUI at 120 columns, in normal mode, on a document of
/// 10 lines with these annotations.
fn bar_of(name: &str, annotations: serde_json::Value) -> String {
    let dir = scratch(name);
    let code: Vec<String> = (1..=10).map(|i| format!("let v{i} = {i};")).collect();
    std::fs::write(
        dir.join("doc.tiza"),
        serde_json::json!({
            "schema": 1,
            "source": {"language": "rs", "first_line": 1},
            "code": code,
            "annotations": annotations
        })
        .to_string(),
    )
    .unwrap();
    let body = r##"
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--no-save", "--no-socket", "--theme", "dark", "doc.tiza"], 30, 120, env=env)
s.wait_for(lambda out: b"\x1b[?1049h" in out, 10, "first frame")
s.wait_for(lambda out: b"annotation" in out, 10, "the notice on the bar")
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
out = s.out.decode("utf-8", "replace")
print("SCREEN", repr(out[out.find("\x1b[?1049h"):]), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "bar.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    out.lines()
        .find_map(|l| l.strip_prefix("SCREEN "))
        .unwrap()
        .to_string()
}

#[test]
fn the_bar_does_not_call_a_mark_kind_that_tiza_does_not_know_outside_the_code() {
    let screen = bar_of(
        "bar-notice-kind",
        serde_json::json!([
            {"id": "m1", "step": 1, "type": "mark", "kind": "moved", "span": {"line": 3, "start": 4, "end": 6}}
        ]),
    );
    assert!(
        screen.contains("tiza check gives the problem of 1 annotation: m1"),
        "{screen}"
    );
    assert!(!screen.contains("outside the code"), "{screen}");
}

#[test]
fn the_bar_names_an_annotation_outside_the_code() {
    let screen = bar_of(
        "bar-notice-outside",
        serde_json::json!([
            {"id": "b1", "step": 1, "type": "box", "from_line": 50, "to_line": 60}
        ]),
    );
    assert!(
        screen.contains("1 annotation outside the code and not drawn: b1"),
        "{screen}"
    );
    assert!(!screen.contains("tiza check gives"), "{screen}");
}
