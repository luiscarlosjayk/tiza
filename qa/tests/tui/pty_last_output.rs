//! The pty harness (`qa/src/pty.py`) keeps the last output of a child that
//! exits fast. On macOS the last close of a pty's slave drops what the
//! master has not read; the harness closed its copy of the slave at once,
//! so under load, when its reader came late, a child that wrote one line
//! and exited lost the line. Now the harness keeps the slave open until
//! the child is gone (a pipe that the child inherits tells it, with no
//! reaping) and its output is read; then the master reports EOF when the
//! last process that has the slave is gone, a grandchild included.
//!
//! A child that has the pty as its controlling terminal (`/bin/sh` takes
//! it) behaves otherwise on macOS: it does not end its exit until its
//! output is read. So most of these tests run a Python child, which does
//! not take it.

use tiza_qa::scratch;

/// Runs a pty-harness `body` with no arguments and returns its stdout.
fn run(name: &str, body: &str) -> String {
    let dir = scratch(name);
    tiza_qa::pty_script(&dir, &format!("{name}.py"), body, &[])
}

/// The value printed after `key ` on a line of `out`.
fn value<'a>(out: &'a str, key: &str) -> &'a str {
    out.lines()
        .find_map(|l| l.strip_prefix(&format!("{key} ")))
        .unwrap_or_else(|| panic!("no {key} in:\n{out}"))
}

#[test]
fn the_last_line_of_a_child_is_there_for_a_reader_that_comes_late() {
    // The reader stops before the child writes, as a reader that a loaded
    // machine schedules late. The child writes its line when it reads one
    // and exits at once; only after its exit is the master read, until the
    // line comes (10 s at most). With the slave closed in the harness, the
    // exit of the child was the last close, and macOS dropped the line.
    let out = run(
        "pty-late-reader",
        r#"
child = "import sys; sys.stdin.readline(); sys.stdout.write('the last line\\n'); sys.stdout.flush(); sys.exit(3)"
s = Pty([sys.executable, "-c", child], 10, 40)
s._stop_reader()
s.send(b"go\n")
code = s.p.wait(timeout=10)
got = b""
end = time.monotonic() + 10
while b"the last line" not in got and time.monotonic() < end:
    try:
        if select.select([s.master], [], [], 0.05)[0]:
            got += os.read(s.master, 65536)
    except OSError:
        break
print("CODE", code, flush=True)
print("LATE", repr(got), flush=True)
"#,
    );
    assert_eq!(value(&out, "CODE"), "3", "{out}");
    assert!(value(&out, "LATE").contains("the last line"), "{out}");
}

#[test]
fn eof_comes_when_the_child_is_gone_and_its_output_is_read() {
    // `printf` does not take the pty: while the harness has the slave, the
    // master gives no EOF. The harness closes its slave when the child is
    // gone, and then the master gives EOF (EIO on Linux).
    let out = run(
        "pty-eof",
        r#"
s = Pty(["/usr/bin/printf", "done\n"], 10, 40)
code = s.finish(10)
print("CODE", code, flush=True)
print("EOF", s.eof, flush=True)
print("OUT", repr(s.out), flush=True)
"#,
    );
    assert_eq!(value(&out, "CODE"), "0", "{out}");
    assert_eq!(value(&out, "EOF"), "True", "{out}");
    assert!(value(&out, "OUT").contains("done"), "{out}");
}

#[test]
fn a_grandchild_that_writes_after_the_child_is_gone_is_read() {
    // The child starts a grandchild that has the pty too, and exits. The
    // grandchild writes when it reads a line, sent after the exit of the
    // child: the harness keeps reading until it is gone too.
    let out = run(
        "pty-grandchild",
        r#"
grandchild = "import sys; sys.stdin.readline(); print('from the grandchild', flush=True)"
child = "import subprocess, sys; subprocess.Popen([sys.executable, '-c', %r]); print('child done', flush=True)" % grandchild
s = Pty([sys.executable, "-c", child], 10, 40)
s.wait_for(lambda out: b"child done" in out, 10, "the child")
code = s.p.wait(timeout=10)
s.send(b"go\n")
end = time.monotonic() + 10
with s._cv:
    while b"from the grandchild" not in s._out and time.monotonic() < end:
        s._wait(0.05)
print("CODE", code, flush=True)
print("GRANDCHILD", b"from the grandchild" in s.out, flush=True)
"#,
    );
    assert_eq!(value(&out, "CODE"), "0", "{out}");
    assert_eq!(value(&out, "GRANDCHILD"), "True", "{out}");
}

#[test]
fn finish_ends_a_child_that_waits_for_a_stopped_reader() {
    // `sh` takes the pty as its controlling terminal: on macOS it does not
    // end its exit while its output is unread and the slave is open. With
    // the reader stopped, `finish` closes the slave first, so the exit ends.
    let out = run(
        "pty-stopped-reader",
        r#"
s = Pty(["/bin/sh", "-c", "read x; printf 'unread\n'; exit 4"], 10, 40)
s._stop_reader()
s.send(b"go\n")
print("CODE", s.finish(10), flush=True)
"#,
    );
    assert_eq!(value(&out, "CODE"), "4", "{out}");
}

#[test]
fn a_child_that_writes_a_lot_just_before_it_exits_gives_all_of_it() {
    // 1 MiB with no line end, then exit 3: far more than a pty holds. The
    // last read before the harness closes the slave takes what comes
    // between an empty select and the exit of the child; no test pins that
    // window, and this one checks that nothing of a large output is lost.
    let out = run(
        "pty-a-lot",
        r#"
child = "import sys; sys.stdout.buffer.write(b'x' * 1048576); sys.stdout.flush(); sys.exit(3)"
s = Pty([sys.executable, "-c", child], 10, 40)
code = s.finish(20)
print("CODE", code, flush=True)
print("XS", s.out.count(b"x"), flush=True)
print("EOF", s.eof, flush=True)
"#,
    );
    assert_eq!(value(&out, "CODE"), "3", "{out}");
    assert_eq!(value(&out, "XS"), "1048576", "{out}");
    assert_eq!(value(&out, "EOF"), "True", "{out}");
}
