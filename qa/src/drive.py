#!/usr/bin/env python3
"""A driver for a hands-on tester of the TUI, in an EMULATED terminal. No
cargo test runs it.

Runs tiza in a pseudo-terminal, as its controlling terminal. What tiza
writes goes through the emulator (wideterm.py in this folder, which answers
the cursor position, OSC 11 and, with --fg, OSC 10), and the emulator's
answers go back to tiza: whole, cut, late, or not at all. Before the first
action it waits until tiza listens: the mark of a build with the feature
`test-hooks` (see `Pty.wait_listening` in pty.py), or --start seconds.

Usage: python3 -I drive.py TIZA DOC [ACTION ...] [OPTIONS]

Actions, in order:
  k:TEXT        type TEXT (\\r, \\x1b and \\uXXXX are read as escapes)
  w:SEC         wait SEC seconds while the screen goes on
  s:LABEL       print the screen
  raw           print what tiza wrote since the last raw (repr)
  api:M:JSON    call the socket method M with JSON
  r:COLSxROWS   resize the terminal (tiza gets SIGWINCH and asks the colors again)

Options:
  --cols N --rows N       the size (default 80 x 12)
  --light                 a light background (the answer to OSC 11)
  --fg                    answer the text color too (OSC 10)
  --start SEC             wait at most SEC seconds for the mark (default 10)
  --wide CHARS            draw these characters two cells wide (⚠↪⏸▶)
  --vs16-narrow           a sign with VS16 takes one cell (as some terminals)
  --nowrap                no autowrap
  --no-answer             the terminal answers nothing
  --cut N                 cut each answer after N bytes; the rest comes later
  --cut-target T          what to cut: osc, cpr or all (default osc)
  --cut-delay SEC         when the rest comes (default 0.3)
  --late SEC              each answer comes SEC seconds late, whole
  --early TEXT            keys typed before tiza reads (a typed ESC: \\x1b)
  --before TEXT           what the shell line had before tiza (emulator only)
  --theme T               --theme of tiza (default auto)
  --arg X                 one more argument for tiza (repeat it)
"""
import argparse, fcntl, os, pty, re, select, shutil, struct, subprocess, sys, tempfile, termios, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from wideterm import Term  # noqa: E402

# The mark of a build with `test-hooks`: the event loop listens.
LISTENING = b"\x1b]7799;tiza-listens\x07"

ANSWER = re.compile(rb"\x1b\[\d+;\d+R|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")


def unescape(text):
    """`\\r`, `\\n`, `\\x1b` and `\\uXXXX` in TEXT as the characters, in UTF-8."""
    text = re.sub(r"\\u([0-9a-fA-F]{4})", lambda m: chr(int(m.group(1), 16)), text)
    for esc, ch in (("\\x1b", "\x1b"), ("\\r", "\r"), ("\\n", "\n")):
        text = text.replace(esc, ch)
    return text.encode("utf-8")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("tiza")
    ap.add_argument("doc")
    ap.add_argument("actions", nargs="*")
    ap.add_argument("--cols", type=int, default=80)
    ap.add_argument("--rows", type=int, default=12)
    ap.add_argument("--light", action="store_true")
    ap.add_argument("--fg", action="store_true")
    ap.add_argument("--start", type=float, default=10.0)
    ap.add_argument("--wide", default="")
    ap.add_argument("--vs16-narrow", action="store_true")
    ap.add_argument("--nowrap", action="store_true")
    ap.add_argument("--no-answer", action="store_true")
    ap.add_argument("--cut", type=int, default=0)
    ap.add_argument("--cut-target", default="osc", choices=["osc", "cpr", "all"])
    ap.add_argument("--cut-delay", type=float, default=0.3)
    ap.add_argument("--late", type=float, default=0.0)
    ap.add_argument("--early", default="")
    ap.add_argument("--before", default="")
    ap.add_argument("--theme", default="auto")
    ap.add_argument("--arg", action="append", default=[])
    a = ap.parse_intermixed_args()

    term = Term(a.rows, a.cols, wide=a.wide, autowrap=not a.nowrap, cpr=not a.no_answer,
                vs16_wide=not a.vs16_narrow, light=a.light, fg=a.fg)
    if a.before:
        term.feed(unescape(a.before))
    run_dir = tempfile.mkdtemp(prefix="tiza-drive-")
    sock = os.path.join(run_dir, "tz.sock")
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", a.rows, a.cols, 0, 0))

    def own_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    env = dict(os.environ, TERM="xterm-256color")
    env.pop("TIZA_NO_OSC", None)
    if a.early:
        # Typed before tiza reads: in the input queue of the terminal.
        os.write(master, unescape(a.early))
    argv = [a.tiza, "--theme", a.theme, "--no-save", "--socket", sock, *a.arg, a.doc]
    try:
        proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, env=env,
                                close_fds=True, preexec_fn=own_terminal)
    except Exception:
        shutil.rmtree(run_dir, ignore_errors=True)
        raise
    os.close(slave)
    queue = []  # (when, bytes): what the terminal sends to tiza, in order
    since_raw = bytearray()
    seen = bytearray()  # all that tiza wrote, to find the mark
    started = time.monotonic()

    def schedule(answers):
        now = time.monotonic()
        last = max([w for w, _ in queue], default=now)
        for m in ANSWER.finditer(answers):
            ans = m.group(0)
            kind = "osc" if ans.startswith(b"\x1b]") else "cpr"
            at = max(now + a.late, last)
            if a.cut and a.cut_target in (kind, "all") and len(ans) > a.cut:
                queue.append((at, ans[: a.cut]))
                queue.append((at + a.cut_delay, ans[a.cut:]))
                last = at + a.cut_delay
            else:
                queue.append((at, ans))
                last = at

    def pump(seconds, until=None):
        end = time.monotonic() + seconds
        while True:
            if until is not None and until():
                return
            now = time.monotonic()
            while queue and queue[0][0] <= now:
                os.write(master, queue.pop(0)[1])
            left = end - now
            if left <= 0:
                return
            wait = min(left, 0.02, max(queue[0][0] - now, 0) if queue else 0.02)
            r, _, _ = select.select([master], [], [], wait)
            if r:
                try:
                    data = os.read(master, 65536)
                except OSError:
                    return
                if not data:
                    return
                since_raw.extend(data)
                seen.extend(data)
                answers = term.feed(data)
                if answers and not a.no_answer:
                    schedule(answers)

    def screen(label):
        print(f"--- {label} (EMULATED, {a.cols}x{a.rows}, t={time.monotonic() - started:.2f}s, "
              f"scrolls={term.scrolls}, cpr_asked={term.cpr_asked})")
        for line in term.lines():
            print(line.rstrip())

    def api(method, params):
        r = subprocess.run([a.tiza, "--socket", sock, "api", method, params],
                           capture_output=True, text=True, timeout=10)
        return (r.stdout or r.stderr).strip()

    try:
        pump(a.start, until=lambda: LISTENING in seen)
        if proc.poll() is not None:
            print(f"--- tiza exited before it listened (exit {proc.returncode})")
        elif LISTENING not in seen:
            print(f"--- no mark of a listening tiza after {a.start}s (a build without test-hooks?)")
        for act in a.actions:
            if act.startswith("k:"):
                os.write(master, unescape(act[2:]))
                pump(0.3)
            elif act.startswith("w:"):
                pump(float(act[2:]))
            elif act.startswith("s:"):
                pump(0.2)
                screen(act[2:])
            elif act == "raw":
                pump(0.2)
                print("--- raw:", repr(bytes(since_raw)))
                since_raw.clear()
            elif act.startswith("r:"):
                cols, rows = (int(x) for x in act[2:].split("x"))
                fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
                term.rows, term.cols = rows, cols
                term.grid = [[" "] * cols for _ in range(rows)]
                term.r, term.c, term.pending = min(term.r, rows - 1), min(term.c, cols - 1), False
                a.cols, a.rows = cols, rows
                pump(0.5)
            elif act.startswith("api:"):
                _, method, params = (act.split(":", 2) + ["{}"])[:3]
                print(f"--- api {method}: {api(method, params or '{}')}")
                pump(0.2)
            else:
                sys.exit(f"unknown action {act!r}")
    finally:
        try:
            os.write(master, b"\x1b\x1b:q!\r")
        except OSError:
            pass  # tiza is gone already
        # Read what tiza writes while it quits: on macOS it waits until its
        # last output is read.
        end = time.monotonic() + 5
        while proc.poll() is None and time.monotonic() < end:
            r, _, _ = select.select([master], [], [], 0.05)
            if r:
                try:
                    os.read(master, 65536)
                except OSError:
                    break
        if proc.poll() is None:
            proc.kill()
        proc.wait()
        os.close(master)
        shutil.rmtree(run_dir, ignore_errors=True)
    print(f"--- exit {proc.returncode}")


if __name__ == "__main__":
    main()
