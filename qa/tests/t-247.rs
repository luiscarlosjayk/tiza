//! With `--no-save` and changes, the bar said `unsaved (:w)`: it invited
//! `:w`, which writes over the file opened, the one `--no-save` is there to
//! keep as it is (an example annotated without touching it).
//!
//! Acceptance criteria:
//! - AC1 With `--no-save` and changes, the bar says
//!   `unsaved (:saveas PATH · :q! discards)`, or `unsaved (:saveas · :q!)`
//!   when it is narrow, at 80 and 120 columns.
//! - AC2 With `:autosave off` on the user's own file the hint keeps
//!   `(:w)`; read-only keeps its own text.
//! - AC3 `:q` refused with unsaved changes in no-save mode names the same
//!   two ways out.
//! - AC4 The help, the README section «Saving» and the CHANGELOG say it.
//!
//! Corner cases: after `:saveas PATH` the document saves there, and the
//! hint is `(:w)` again; a narrow bar still names both commands.

use tiza_qa::{Live, bin, scratch};

const CODE: &str = "fn main() {\n    let a = 1;\n}\n";

/// An instance as `tiza FILE --no-save` opens it, with one change.
fn no_save_with_a_change(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.with(|a| {
        a.autosave = false;
        a.no_save = true;
    });
    live.command("note 2 hi");
    live
}

fn bar(live: &Live, cols: u16) -> String {
    live.screen(cols, 30).lines().last().unwrap().to_string()
}

#[test]
fn ac1_the_bar_names_saveas_and_q_bang_at_120_and_80() {
    let live = no_save_with_a_change("t247-bar");
    let b120 = bar(&live, 120);
    assert!(
        b120.contains("unsaved (:saveas PATH · :q! discards)"),
        "{b120}"
    );
    assert!(!b120.contains("(:w)"), "{b120}");
    // At 80 the bar may need the short form: both commands stay.
    let b80 = bar(&live, 80);
    assert!(
        b80.contains("unsaved (:saveas PATH · :q! discards)")
            || b80.contains("unsaved (:saveas · :q!)"),
        "{b80}"
    );
}

#[test]
fn corner_a_narrow_bar_keeps_both_commands() {
    let live = no_save_with_a_change("t247-narrow");
    let b = bar(&live, 50);
    assert!(b.contains("unsaved (:saveas · :q!)"), "{b}");
    assert!(b.chars().count() <= 50, "{b}");
}

#[test]
fn ac2_autosave_off_on_your_own_file_keeps_w_and_read_only_its_text() {
    let live = Live::from_code(CODE, "rs", "t247-own");
    live.command("autosave off");
    live.command("note 2 hi");
    let b = bar(&live, 120);
    assert!(b.contains("unsaved (:w)"), "{b}");
    assert!(!b.contains(":saveas"), "{b}");
    // A sidecar that cannot be written keeps its own words.
    live.with(|a| a.save_error = Some("read-only".into()));
    let b = bar(&live, 120);
    assert!(b.contains("read-only • unsaved (:w)"), "{b}");
}

#[test]
fn ac3_q_refused_names_saveas_and_q_bang() {
    let live = no_save_with_a_change("t247-quit");
    live.command("q");
    assert!(!live.with(|a| a.quit));
    let (msg, err) = live.message().unwrap();
    assert!(err, "{msg}");
    assert_eq!(
        msg,
        "unsaved changes (--no-save): :saveas PATH saves them elsewhere, :q! discards them"
    );
    live.command("q!");
    assert!(live.with(|a| a.quit));
}

#[test]
fn corner_after_saveas_the_hint_is_w_again() {
    let live = no_save_with_a_change("t247-saveas");
    let dir = scratch("t247-saveas-to");
    let to = dir.join("mine.tiza");
    live.command(&format!("saveas {}", to.display()));
    assert!(to.exists(), "{:?}", live.message());
    assert!(!live.with(|a| a.no_save));
    let b = bar(&live, 120);
    assert!(!b.contains("unsaved"), "saved: {b}");
    live.command("note 1 again");
    let b = bar(&live, 120);
    assert!(b.contains("unsaved (:w)"), "{b}");
}

#[test]
fn corner_autosave_on_ends_no_save() {
    let live = no_save_with_a_change("t247-autosave-on");
    live.command("autosave on");
    assert!(!live.with(|a| a.no_save));
    assert!(!live.with(|a| a.dirty));
}

#[test]
fn ac3_the_binary_with_no_save_refuses_q_and_names_both_ways_out() {
    let dir = scratch("t247-pty");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let body = r##"
import json
binary, work = sys.argv[1], sys.argv[2]
os.chdir(work)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
env.pop("TIZA_TEST_HOOK", None)
s = Pty([binary, "--no-save", "--socket", "s.sock", "main.rs"], 30, 120, env=env)

def bar():
    r = subprocess.run([binary, "--socket", "s.sock", "api", "follow.status", "{}"],
                       capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout).get("bar")

if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    s.send(b"\x1b:note 2 hi\r")
    s.wait_for(lambda out: b"a1" in out, 10, "the note")
    s.send(b"\x1b:q\r")
    end = time.monotonic() + 10
    said = None
    while time.monotonic() < end:
        said = bar()
        if said and said.startswith("unsaved"):
            break
        time.sleep(0.05)
    print("BAR", json.dumps(said), flush=True)
    print("SIDECAR", os.path.exists("main.rs.tiza"), flush=True)
    s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "no_save.py",
        body,
        &[bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(
        out.contains(
            r#"BAR "unsaved changes (--no-save): :saveas PATH saves them elsewhere, :q! discards them""#
        ),
        "{out}"
    );
    assert!(out.contains("SIDECAR False"), "{out}");
    assert!(out.contains("EXIT 0"), "{out}");
}

#[test]
fn ac4_the_help_the_readme_and_the_changelog_say_it() {
    let help: Vec<String> = tiza_tui::keys::HELP
        .iter()
        .flat_map(|(_, rows)| rows.iter().map(|r| r.key_text()))
        .collect();
    assert!(
        help.iter().any(|t| t
            == "With --no-save, :saveas PATH saves to another file, and :q! discards the changes."),
        "{help:?}"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |f: &str| std::fs::read_to_string(root.join(f)).unwrap();
    assert!(read("README.md").contains("unsaved (:saveas PATH · :q! discards)"));
    assert!(read("CHANGELOG.md").contains("unsaved (:saveas PATH · :q! discards)"));
}
