# Prelude of every pty test in qa/tests (t-119 retrabajo): `tiza_qa::pty_script`
# prepends this file to the test's own Python body.
#
# Nothing here may block past DEADLINE: every read goes through select() with a
# short slice, a child that never writes (or never exits) is killed, and a
# SIGALRM backstop ends the whole script even if a body does something blocking
# by mistake. The Rust side (`tiza_qa::python`) adds its own wall-clock timeout
# around this process.
import fcntl, os, pty, re, select, signal, struct, subprocess, sys, termios, time, tty

DEADLINE_S = float(os.environ.get("TIZA_PTY_DEADLINE", "40"))
DEADLINE = time.monotonic() + DEADLINE_S
ANSI = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
PTYS = []


def plain(raw):
    """Output bytes as text without CSI sequences."""
    return ANSI.sub("", raw.decode(errors="replace"))


def _backstop(signum, frame):
    for t in PTYS:
        try:
            os.killpg(t.p.pid, signal.SIGKILL)
        except OSError:
            pass
    print("PTY_DEADLINE", flush=True)
    os._exit(3)


signal.signal(signal.SIGALRM, _backstop)
signal.alarm(int(DEADLINE_S) + 2)


class Pty:
    """One child on a pseudo-terminal, in its own session.

    `ctty=True` makes the pty its controlling terminal (so it can open
    /dev/tty); `raw=True` puts the line discipline in raw mode before the
    child starts; `seed` is written to the child's input before it starts
    (a terminal answer that is already waiting when the child asks)."""

    def __init__(self, argv, rows, cols, env=None, ctty=False, raw=False, seed=b""):
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        if raw:
            tty.setraw(slave)
        if seed:
            os.write(self.master, seed)

        def make_ctty():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        kw = dict(preexec_fn=make_ctty) if ctty else dict(start_new_session=True)
        self.p = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave,
                                  close_fds=True, env=env, **kw)
        os.close(slave)
        os.set_blocking(self.master, False)
        self.ctty = ctty
        self.out = b""
        self.eof = False
        self.closed = False
        PTYS.append(self)

    def _left(self, end):
        return min(end, DEADLINE) - time.monotonic()

    def _read(self, slice_s):
        """At most one read, waiting at most `slice_s`. True when bytes came."""
        if self.eof or self.closed:
            return False
        r, _, _ = select.select([self.master], [], [], max(0.0, slice_s))
        if not r:
            return False
        try:
            chunk = os.read(self.master, 65536)
        except (BlockingIOError, InterruptedError):
            return False
        except OSError:  # EIO: the slave side is gone (Linux)
            self.eof = True
            return False
        if not chunk:
            self.eof = True
            return False
        self.out += chunk
        return True

    def _drain(self):
        """The child exited: take what is left and stop at the first quiet
        0.2 s (macOS may never report EOF on the master)."""
        end = time.monotonic() + 2
        while self._left(end) > 0 and self._read(min(0.2, self._left(end))):
            pass

    def wait_for(self, pred, timeout=10.0, what="condition"):
        """Reads until `pred(self.out)` holds, the child is gone, or `timeout`
        passes. Prints `WAIT_TIMEOUT <what>` and returns False on failure."""
        end = time.monotonic() + timeout
        while not pred(self.out):
            left = self._left(end)
            if left <= 0 or self.eof:
                break
            if not self._read(min(left, 0.05)) and self.p.poll() is not None:
                self._drain()
                break
        ok = pred(self.out)
        if not ok:
            print("WAIT_TIMEOUT", what, flush=True)
        return ok

    def settle(self, seconds):
        """Keeps reading for `seconds` (a negative check's short settle)."""
        end = time.monotonic() + seconds
        while self._left(end) > 0 and not self.eof:
            self._read(min(self._left(end), 0.05))

    def send(self, data):
        """Writes to the child's input; never blocks past the deadline."""
        while data and not self.closed:
            if self._left(DEADLINE) <= 0:
                print("SEND_TIMEOUT", flush=True)
                return False
            _, w, _ = select.select([], [self.master], [], 0.05)
            if not w:
                self._read(0)  # keep the child's output flowing
                continue
            try:
                n = os.write(self.master, data)
            except (BlockingIOError, InterruptedError):
                continue
            except OSError:
                return False
            data = data[n:]
        return True

    def resize(self, rows, cols):
        """New window size. With `ctty=True` the kernel itself sends one
        SIGWINCH to the child's foreground group (Linux and macOS); sending
        another by hand would arrive as a second resize under load. Without a
        controlling terminal nobody is told, so the signal is sent here."""
        fcntl.ioctl(self.master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        if not self.ctty:
            try:
                os.killpg(self.p.pid, signal.SIGWINCH)
            except OSError:
                pass

    def kill(self):
        try:
            os.killpg(self.p.pid, signal.SIGKILL)
        except OSError:
            pass
        try:
            self.p.kill()
        except OSError:
            pass
        try:
            self.p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            print("CHILD_UNKILLABLE", flush=True)

    def finish(self, timeout=10.0):
        """Waits for the child to exit while draining its output; past
        `timeout` its process group is killed (`CHILD_KILLED`). Returns the
        exit code (negative when killed by a signal)."""
        end = time.monotonic() + timeout
        while self.p.poll() is None and self._left(end) > 0:
            if self.eof:  # nothing more to read: just wait, bounded
                try:
                    self.p.wait(timeout=max(0.0, self._left(end)))
                except subprocess.TimeoutExpired:
                    pass
                break
            self._read(min(self._left(end), 0.05))
        if self.p.poll() is None:
            print("CHILD_KILLED", flush=True)
            self.kill()
        self._drain()
        if not self.closed:
            os.close(self.master)
            self.closed = True
        return self.p.returncode

# ---- end of the prelude; the test's own body follows ----
