# Prelude of every pty test in qa/tests: `tiza_qa::pty_script`
# prepends this file to the test's own Python body.
#
# Nothing here may block past DEADLINE: every read goes through select() with a
# short slice, in one reader thread per Pty that drains the pty all the time (so
# a child never blocks writing while the body does something else); a child
# that never writes (or never exits) is killed, and a SIGALRM backstop ends the
# whole script even if a body does something blocking by mistake. The Rust side (`tiza_qa::python`) adds its own wall-clock timeout
# around this process.
import fcntl, os, pty, re, select, signal, struct, subprocess, sys, termios, threading, time, tty

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
    (a terminal answer that is already waiting when the child asks).

    A background reader thread drains the pty into `out`
    from the start, whatever the body is doing: a body that runs `tiza api`
    with `subprocess.run` between two `wait_for`s no longer leaves the child
    blocked writing a frame into a full pty buffer (small on macOS), unable to
    answer its socket. `wait_for`/`settle` only look at what the thread has
    already read."""

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
        self.closed = False
        # Shared with the reader thread, always under `_cv`: the output so
        # far, whether the master reported EOF, when the last bytes came, and
        # the order to stop.
        self._cv = threading.Condition()
        self._out = b""
        self._eof = False
        self._last = time.monotonic()
        self._stop = False
        self._reader = threading.Thread(target=self._pump, daemon=True)
        self._reader.start()
        PTYS.append(self)

    @property
    def out(self):
        """Everything the child wrote so far (read by the reader thread)."""
        with self._cv:
            return self._out

    @property
    def eof(self):
        with self._cv:
            return self._eof

    def _pump(self):
        """The reader thread: select-bounded reads of the master into `_out`
        until EOF or `_stop`; wakes every waiter on each chunk."""
        while True:
            with self._cv:
                if self._stop:
                    return
            try:
                r, _, _ = select.select([self.master], [], [], 0.05)
            except (OSError, ValueError):
                r, chunk = None, b""
            else:
                if not r:
                    continue
                try:
                    chunk = os.read(self.master, 65536)
                except (BlockingIOError, InterruptedError):
                    continue
                except OSError:  # EIO: the slave side is gone (Linux)
                    chunk = b""
            with self._cv:
                if chunk:
                    self._out += chunk
                    self._last = time.monotonic()
                else:
                    self._eof = True
                self._cv.notify_all()
            if not chunk:
                return

    def _left(self, end):
        return min(end, DEADLINE) - time.monotonic()

    def _wait(self, slice_s):
        """Waits (bounded by `slice_s`) for the reader thread to bring bytes,
        report EOF or be stopped. Caller holds `_cv`."""
        self._cv.wait(max(0.0, slice_s))

    def _drain(self):
        """The child exited: let the reader take what is left and stop at
        EOF or the first quiet 0.2 s (macOS may never report EOF on the
        master), at most 2 s."""
        end = time.monotonic() + 2
        with self._cv:
            while not self._eof and self._left(end) > 0:
                quiet = time.monotonic() - self._last
                if quiet >= 0.2:
                    break
                self._wait(min(0.2 - quiet, self._left(end)))

    def wait_for(self, pred, timeout=10.0, what="condition"):
        """Waits until `pred(self.out)` holds, the child is gone, or `timeout`
        passes. Prints `WAIT_TIMEOUT <what>` and returns False on failure."""
        end = time.monotonic() + timeout
        while True:
            with self._cv:
                if pred(self._out):
                    return True
                left = self._left(end)
                if left <= 0 or self._eof:
                    break
                self._wait(min(left, 0.05))
                if pred(self._out):
                    return True
            if self.p.poll() is not None:
                self._drain()
                break
        ok = pred(self.out)
        if not ok:
            print("WAIT_TIMEOUT", what, flush=True)
        return ok

    def settle(self, seconds):
        """Lets the reader keep reading for `seconds` (a negative check's
        short settle)."""
        end = time.monotonic() + seconds
        with self._cv:
            while self._left(end) > 0 and not self._eof:
                self._wait(self._left(end))

    def _stop_reader(self):
        """Stops the reader thread (it notices within one 0.05 s slice), so
        the master can be closed under no one's select."""
        with self._cv:
            self._stop = True
            self._cv.notify_all()
        self._reader.join(timeout=5)
        if self._reader.is_alive():
            print("READER_STUCK", flush=True)

    def send(self, data):
        """Writes to the child's input; never blocks past the deadline."""
        while data and not self.closed:
            if self._left(DEADLINE) <= 0:
                print("SEND_TIMEOUT", flush=True)
                return False
            _, w, _ = select.select([], [self.master], [], 0.05)
            if not w:  # the reader thread keeps the child's output flowing
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
        """Waits for the child to exit while the reader drains its output;
        past `timeout` its process group is killed (`CHILD_KILLED`). Returns
        the exit code (negative when killed by a signal)."""
        end = time.monotonic() + timeout
        try:
            self.p.wait(timeout=max(0.0, self._left(end)))
        except subprocess.TimeoutExpired:
            pass
        if self.p.poll() is None:
            print("CHILD_KILLED", flush=True)
            self.kill()
        self._drain()
        if not self.closed:
            self._stop_reader()
            os.close(self.master)
            self.closed = True
        return self.p.returncode

# ---- end of the prelude; the test's own body follows ----
