"""A time log for drive.py: it runs the driver and writes to stderr, each with
its time, the answers that the terminal sends to tiza, the text that tiza
draws on the last row when it is not the status bar (the command line), and
the background of each frame. No cargo test runs it.

Usage: python3 -I write_log.py drive.py TIZA DOC [ACTION ...] [OPTIONS]
"""
import os, re, runpy, sys, time

_write, _read = os.write, os.read
t0 = time.monotonic()
CELL = re.compile(rb"\x1b\[(\d+);(\d+)H(.*?)(?=\x1b\[\d+;\d+H|\x1b\[\?2026l|$)", re.S)


def log(msg):
    print(f"[{time.monotonic() - t0:5.2f}] {msg}", file=sys.stderr)


def write(fd, data):
    if data[:1] in (b"\x1b", b"]", b"[") or b"rgb" in data:
        log(f"-> tiza {data!r}")
    return _write(fd, data)


def read(fd, n):
    data = _read(fd, n)
    rows = [int(m.group(1)) for m in CELL.finditer(data)]
    last = max(rows, default=0)
    for m in CELL.finditer(data):
        if int(m.group(1)) != last:
            continue
        txt = re.sub(rb"\x1b\[[0-9;?]*[A-Za-z]", b"", m.group(3)).decode("utf-8", "replace").rstrip()
        if txt and not txt.startswith(" ╭"):
            log(f"<- tiza row {last} col {m.group(2).decode()}: {txt!r}")
    bg = re.findall(rb"48;2;(\d+;\d+;\d+)", data)
    if bg:
        log(f"<- tiza frame, background {bg[0].decode()}")
    return data


os.write, os.read = write, read
sys.argv = sys.argv[1:]
runpy.run_path(sys.argv[0], run_name="__main__")
