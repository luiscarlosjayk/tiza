# Prelude of every pty test in qa/tests: `tiza_qa::pty_script`
# prepends this file to the test's own Python body.
#
# Nothing here may block past DEADLINE: every read goes through select() with a
# short slice, in one reader thread per Pty that drains the pty all the time (so
# a child never blocks writing while the body does something else); a child
# that never writes (or never exits) is killed, and a SIGALRM backstop ends the
# whole script even if a body does something blocking by mistake. The Rust side (`tiza_qa::python`) adds its own wall-clock timeout
# around this process.
import fcntl, os, pty, re, select, signal, struct, subprocess, sys, termios, threading, time, tty, unicodedata

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
        # The child inherits the write end of this pipe: the read end gives
        # EOF when the child, and every process that kept it, is gone. So the
        # reader learns of the exit without reaping the child, which a body
        # may wait for itself (`closed_terminal` with `os.wait4`).
        self._gone, gone_w = os.pipe()
        self.p = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave,
                                  close_fds=True, pass_fds=(gone_w,), env=env, **kw)
        os.close(gone_w)
        # Kept open until the child is gone and its output is read: on macOS
        # the last close of the slave drops what the master has not read, so
        # a child that wrote and exited fast lost its last output when the
        # reader came late (under load).
        self._slave = slave
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
            watch = [self.master] if self._gone is None else [self.master, self._gone]
            try:
                r, _, _ = select.select(watch, [], [], 0.05)
            except (OSError, ValueError):
                r, chunk = None, b""
            else:
                if not r:
                    continue
                if self.master not in r:
                    # Only the pipe: the child is gone.
                    self._child_gone()
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

    def _child_gone(self):
        """The child is gone, and the master had nothing to read a moment
        ago. Read what came since, then close the slave. The master then
        reports EOF (EIO on Linux) when every other process that has the
        slave is gone too, a grandchild included. Nothing the child wrote is
        dropped: the slave was open here until now. (A child that has the pty
        as its controlling terminal, as `sh` takes it, does not even end its
        exit on macOS until its output is read.)"""
        os.close(self._gone)
        self._gone = None
        while True:
            try:
                r, _, _ = select.select([self.master], [], [], 0)
                chunk = os.read(self.master, 65536) if r else b""
            except (BlockingIOError, InterruptedError):
                chunk = b""
            except OSError:
                chunk = b""
            with self._cv:
                if chunk:
                    self._out += chunk
                    self._last = time.monotonic()
                self._cv.notify_all()
            if not chunk:
                break
        self._close_slave()

    def _close_slave(self):
        """Closes the slave once, from the reader or from `finish`."""
        with self._cv:
            slave, self._slave = self._slave, None
        if slave is not None:
            os.close(slave)

    def _left(self, end):
        return min(end, DEADLINE) - time.monotonic()

    def _wait(self, slice_s):
        """Waits (bounded by `slice_s`) for the reader thread to bring bytes,
        report EOF or be stopped. Caller holds `_cv`."""
        self._cv.wait(max(0.0, slice_s))

    def _drain(self):
        """The child exited: let the reader take what is left and stop at
        EOF (the master gives it once the harness closed its slave and every
        other process that has it is gone) or at the first quiet 0.2 s, at
        most 2 s."""
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
        if not self._reader.is_alive():
            # No reader: a child with the pty as its controlling terminal
            # would wait at its exit for its output to be read.
            self._close_slave()
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
            self._close_slave()
            if self._gone is not None:
                os.close(self._gone)
                self._gone = None
            os.close(self.master)
            self.closed = True
        return self.p.returncode

_SEQ = re.compile(r"\x1b\[([0-9;?<>=]*)([ -/]*)([@-~])|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[P^_][^\x1b]*\x1b\\|\x1b.|([^\x1b])", re.S)

def terminal_rows(raw, rows, cols):
    """The rows a terminal shows after `raw`: the cursor moves, erases and
    text that ratatui writes through crossterm; styles, modes and OSC
    strings change no cell."""
    grid = [[" "] * cols for _ in range(rows)]
    r = c = 0
    for m in _SEQ.finditer(raw.decode(errors="replace")):
        params, final, ch = m.group(1), m.group(3), m.group(4)
        if ch is not None:
            if ch == "\r":
                c = 0
            elif ch == "\n":
                r = min(r + 1, rows - 1)
            elif ch >= " ":
                w = 0 if unicodedata.combining(ch) else (2 if unicodedata.east_asian_width(ch) in "WF" else 1)
                if w and c < cols:
                    grid[r][c] = ch
                    if w == 2 and c + 1 < cols:
                        grid[r][c + 1] = ""
                c += w
        elif final is not None and not params.startswith("?"):
            n = [int(x) if x else 0 for x in params.split(";")] if params else []
            first = n[0] if n else 0
            if final in "Hf":
                r = min(max((n[0] if n else 1) - 1, 0), rows - 1)
                c = max((n[1] if len(n) > 1 else 1) - 1, 0)
            elif final == "J" and first == 2:
                grid = [[" "] * cols for _ in range(rows)]
            elif final == "K":
                lo, hi = {1: (0, c + 1), 2: (0, cols)}.get(first, (c, cols))
                for x in range(lo, min(hi, cols)):
                    grid[r][x] = " "
    return ["".join(row).rstrip() for row in grid]

# ---- end of the prelude; the test's own body follows ----
