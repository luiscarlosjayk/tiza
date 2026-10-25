//! Typos in `:` commands never close the app, never leave the
//! terminal broken, and never create something other than what was typed.
//!
//! Acceptance criteria:
//! - AC1 and AC3 (export sizes are bounded, and the largest export is fast)
//!   are checked in `latency::export_sizes`.
//! - AC2 A panic restores the terminal (alternate screen left, cursor shown,
//!   bracketed paste and mouse off) and prints one `tiza crashed:` line
//!   naming the document and where to report it.
//! - AC4 Out-of-range numbers are errors or normalised with a notice in
//!   `:focus`, `:box`/`:callout`/`:spot`, `:until`, `:step ID N`.
//! - AC5 A line that does not exist, or `L:text` whose text is not on L, is
//!   an error that says where the text is — never an anchor elsewhere.
//! - AC6 Typo shapes (`:box 3 - 5`, `:hl 3 s`, `:hl s`, `:mark warn s`) get
//!   the usage line, not an annotation.
//! - AC7 Unknown commands suggest the nearest one (edit distance ≤ 2).
//! - AC8 Empty key/value, `:lang A B C`, `@bogus` are errors.
//! - AC9 This table drives the card's list through the real command runner.
//! - AC10 A rejected command never ends in a success message.

use tiza_qa::{Live, scratch};

const CODE: &str = "fn main() {\n    let s = String::from(\"hi\");\n    takes(s);\n    // println!(\"{s}\");\n}\n\nfn takes(s: String) {\n    println!(\"{s}\");\n}\n";

fn fresh(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.command("note 1 first");
    live.command("spot 7-9");
    live.command("step 2");
    live.command("note 8 later");
    live.command("step 1");
    live
}

fn count(live: &Live) -> usize {
    live.with(|a| a.doc.annotations.len())
}

/// Runs `cmd`; the message must be an error containing `expect`, and the
/// annotation count and the step must not change.
fn rejected(live: &Live, cmd: &str, expect: &str) {
    let before = (
        count(live),
        live.with(|a| a.doc.step),
        live.with(|a| a.doc.max_step()),
    );
    live.command(cmd);
    let (msg, err) = live.message().unwrap_or_default();
    assert!(err, "`:{cmd}` was not rejected: {msg}");
    assert!(
        msg.contains(expect),
        "`:{cmd}`: expected «{expect}», got: {msg}"
    );
    let after = (
        count(live),
        live.with(|a| a.doc.step),
        live.with(|a| a.doc.max_step()),
    );
    assert_eq!(before, after, "`:{cmd}` changed the document: {msg}");
}

#[test]
fn table_of_typos_is_rejected_with_a_way_forward() {
    let live = fresh("table");
    let n0 = count(&live);
    assert_eq!(n0, 3);
    for (cmd, expect) in [
        // AC6: shape typos
        ("box 3 - 5 etiqueta", "write A-B without spaces"),
        ("box -1-3 x", "usage: :box"),
        ("hl 3 s", "usage: :hl L:text"),
        ("hl s", "usage: :hl L:text"),
        ("mark warn s", "usage: :mark"),
        // AC5: wrong line / wrong text
        ("hl 3:S x", "use 2:S («S» is on L2, not L3)"),
        ("hl 999:x y", "line 999 does not exist"),
        ("arrow 1:s 999:s x", "is on L2, not L1"),
        ("arrow 2:s 999:s x", "line 999 does not exist"),
        (
            "mark warn 1:takes",
            "use 3:takes («takes» is on L3, not L1)",
        ),
        // AC4: ranges and steps
        ("focus 0", "line 0 does not exist"),
        ("focus 99-100", "line 99 does not exist"),
        ("box 0-0 x", "line 0 does not exist"),
        ("until a2 0", "needs N ≥ 1"),
        ("until a1 3", "a1 is not a spotlight"),
        ("step a1 99999", "beyond the last step (2)"),
        ("step a1 0", "steps ≥ 1"),
        // AC8: empty key/value and shapes
        ("set a1 =", "expected key=value, got «=»"),
        ("set a1 color=", "expected key=value, got «color=»"),
        ("style io glyph=", "expected key=value, got «glyph=»"),
        ("lang rs rs rs", "usage: :lang ID"),
        (
            "callout 7-9 @bogus txt",
            "unknown placement: use @below, @margin or @auto",
        ),
        // AC7: nearest command
        ("bx 3-5", "unknown command — did you mean :box?"),
        ("stpe 2", "did you mean :step, :style?"),
        ("colr a1 red", "did you mean :color?"),
        ("zzzzzz", "unknown command (try :help)"),
    ] {
        rejected(&live, cmd, expect);
    }
    assert_eq!(count(&live), n0);
    // `:focus 99-1` is normalised to the document with a notice (Luis's call).
    live.command("focus 2-99");
    let (msg, err) = live.message().unwrap();
    assert!(
        !err && msg.contains("focus on 2-9 (99 is past the last line, 9)"),
        "{msg}"
    );
    assert_eq!(live.with(|a| a.focus), Some((2, 9)));
}

#[test]
fn valid_forms_still_work_and_report_what_they_did() {
    let live = fresh("valid");
    for (cmd, expect) in [
        ("box 7-9 takes", ": box 7-9"),
        ("hl 3:s x", ": highlight at L3"),
        ("hl 3: whole", ": highlight at L3"),
        ("mark move 3:s moved", "at L3"),
        ("arrow 2:s 7:s move", ": arrow L2 → L7"),
        ("arrow 2 7 plain", ": arrow L2 → L7"),
        ("focus 7-9", "focus on 7-9"),
        ("step a1 2", "a1 now appears at step 2"),
        ("step a1 3", "a1 now appears at step 3"),
        ("until a2 3", "a2 active until step 3"),
        ("callout 7-9 @below why", ": callout at L7-9"),
        ("set a1 color=blue", "a1"),
        ("style io glyph=⚡ legend=io", "style io defined"),
        ("lang ts", "language: ts"),
        (
            "note 4 🔥👩‍👩‍👧‍👦 RTL\u{200f} tab\t%s%n \"quotes\" $(id)",
            ": note at L4",
        ),
    ] {
        live.command(cmd);
        let (msg, err) = live.message().unwrap();
        assert!(!err && msg.contains(expect), "`:{cmd}`: {msg}");
    }
    // A 20 000-character label is slow to type but valid.
    let long = format!("note 5 {}", "x".repeat(20_000));
    live.command(&long);
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
}

#[test]
fn a_panic_restores_the_terminal_and_says_where_the_work_is() {
    // A pty via Python (always on the runners), `TIZA_TEST_PANIC=1` enables
    // the hidden `:panic` command.
    let dir = scratch("panic");
    std::fs::write(dir.join("src.rs"), CODE).unwrap();
    let stdout = tiza_qa::pty_script(
        &dir,
        "panic.py",
        r#"
binary, source = sys.argv[1], sys.argv[2]
env = dict(os.environ, TIZA_TEST_PANIC="1")
s = Pty(["bash", "-c", f"{binary} {source} --no-socket --no-save; echo EXIT=$?"], 30, 100, env=env)
# Keys typed before the first frame would be read by the start-up OSC query.
if s.wait_for(lambda out: b"\x1b[?1049h" in out, 10, "first frame"):
    s.send(b":panic\r")
s.wait_for(lambda out: b"EXIT=" in out, 20, "EXIT=")
s.finish(10)
out = s.out
text = plain(out)
print("ALTSCREEN_OUT" if b"\x1b[?1049l" in out else "no-restore")
print("CURSOR_SHOWN" if b"\x1b[?25h" in out else "no-cursor")
print("PASTE_OFF" if b"\x1b[?2004l" in out else "no-paste-off")
i = text.find("tiza crashed:")
print(text[i:i+400].replace("\n", " ") if i >= 0 else "NO-CRASH-LINE")
print(text[text.find("EXIT="):].strip())
"#,
        &[
            tiza_qa::bin().to_str().unwrap(),
            dir.join("src.rs").to_str().unwrap(),
        ],
    );
    assert!(stdout.contains("ALTSCREEN_OUT"), "{stdout}");
    assert!(stdout.contains("CURSOR_SHOWN"), "{stdout}");
    assert!(stdout.contains("PASTE_OFF"), "{stdout}");
    assert!(
        stdout.contains("tiza crashed: forced by :panic")
            && stdout.contains("src.rs.tiza")
            && stdout.contains("github.com/luiscarlosjayk/tiza/issues"),
        "{stdout}"
    );
    assert!(stdout.contains("EXIT=101"), "{stdout}");
    // Without the env var the command does not exist.
    let live = fresh("nopanic");
    rejected(&live, "panic", "unknown command");
}
