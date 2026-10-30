#!/usr/bin/env python3
"""Frames of the docs pages: one real `tiza render` export for each frame.

A page asks for a frame with a comment, the block of `:` commands that
makes it, and the <img> that shows it:

    <!-- frame id=ID code=PATH:A-B [width=W] [theme=light|doc] -->
    ```
    :hl 3:takes
    ```

    <img src="../../images/commands/ID.webp" width="0" alt="...">

For each frame, the script opens lines A to B of PATH (relative to the
repository) in the TUI on a pseudo-terminal, types the commands, saves the
document with `:wq`, and exports the step on screen. Each command must
change the document: one that changes nothing (a typo, or a text that the
code does not have) stops the script.

    tiza render DOC --step N --format webp --font-size 13 --scale 2 --width W

- The theme is dark. `theme=light` gives light, `theme=doc` the theme that
  the commands gave the document.
- The width W is `width=`, or else the narrowest width from 20 columns at
  which `tiza check` finds nothing and the text export cuts no label with
  `…` (unless the code itself has `…`).
- The image goes where the src of its <img> points, and the `width` of the
  <img> becomes the width of the frame in CSS pixels (half its pixels).

A frame can also show a document of the repository as it is, with no
commands, for a move or a field that no `:` command makes:

    <!-- frame id=ID doc=PATH [step=N | animate=0|1] [interval=S] [width=W] [theme=…] -->

    <img src="../images/format/ID.webp" width="0" alt="...">

- `step=N` exports step N, a still. A `doc=` frame needs `step=` or
  `animate=`.
- `animate=0` exports every step from step 0 as an animated WebP, with
  the typing and the enter effects; `animate=1` starts at step 1
  (`--skip-zero`). A `code=` frame takes `animate=` too.
- `interval=S` is the time of each step in seconds (`--interval`).
- `files=PATH[,PATH…]` (`code=` only) copies files of the repository next
  to the code, under their own names, for a command that reads a file:
  `files=examples/themes/catppuccin-mocha.toml` with
  `:theme file catppuccin-mocha.toml`.
- The page shows the JSON of the document next to its frame. The document
  and its code go in docs/frames/.

Usage: docs_frames.py TIZA [--out DIR] [--only-doc] [PAGE...]
Without PAGE: every page under docs/ with a frame comment. With --out, the
pages and the images go under DIR, at their paths in the repository, and
the repository does not change. With --only-doc, only the `doc=` frames:
the check of the docs site makes them again and compares them with the
committed images (site/checks).
"""
import argparse
import atexit
import fcntl
import json
import os
import pty
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time


def no_config():
    """Every tiza that this script starts reads an empty config file, and
    an empty config folder: the config of whoever runs it never changes a
    result."""
    folder = tempfile.mkdtemp(prefix="tiza-no-config-")
    open(os.path.join(folder, "config.toml"), "w").close()
    os.environ["TIZA_CONFIG"] = os.path.join(folder, "config.toml")
    os.environ["XDG_CONFIG_HOME"] = folder
    atexit.register(shutil.rmtree, folder, True)


ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
COMMENT = re.compile(r"<!-- frame id=(\S+)((?: \w+=\S+)*) -->")
# The block of `:` commands right after the comment of a `code=` frame.
BLOCK = re.compile(r"\n```\n(.*?)\n```", re.S)
KEYS = {"code", "doc", "width", "theme", "step", "animate", "interval", "files"}


def frames_of(page_path, page):
    """The frames of a page, each with its id, its options, its `:` commands
    (`None` for a `doc=` frame), and the end of its comment and block. A
    frame that the script cannot make stops it."""
    out = []
    for m in COMMENT.finditer(page):
        fid = m.group(1)
        where = f"docs-frames: {page_path}: frame {fid}"
        opts = dict(x.split("=", 1) for x in m.group(2).split())
        unknown = sorted(set(opts) - KEYS)
        if unknown:
            sys.exit(f"{where}: unknown key {unknown[0]}= (the keys: {', '.join(sorted(KEYS))})")
        if ("code" in opts) == ("doc" in opts):
            sys.exit(f"{where}: give code=PATH:A-B or doc=PATH, one of the two")
        if "step" in opts and "animate" in opts:
            sys.exit(f"{where}: step= is a still and animate= an animation: give one of the two")
        if "step" in opts and "code" in opts:
            sys.exit(f"{where}: a code= frame shows the step on screen: step= is for doc=")
        if "doc" in opts and "step" not in opts and "animate" not in opts:
            sys.exit(f"{where}: a doc= frame needs step=N (a still) or animate=0|1")
        if "interval" in opts and "animate" not in opts:
            sys.exit(f"{where}: interval= is only for animate=")
        if "files" in opts and "code" not in opts:
            sys.exit(f"{where}: files= copies files next to the code of a code= frame")
        for key, pattern in [("step", r"\d+"), ("animate", r"[01]"), ("width", r"\d+"),
                             ("interval", r"\d+(\.\d+)?"), ("code", r"\S+:\d+-\d+")]:
            if key in opts and not re.fullmatch(pattern, opts[key]):
                sys.exit(f"{where}: {key}={opts[key]} is not a value of {key}=")
        cmds, end = None, m.end()
        if "code" in opts:
            b = BLOCK.match(page, m.end())
            if not b:
                sys.exit(f"{where}: no block of `:` commands right after the comment")
            cmds = [line for line in b.group(1).split("\n") if line.startswith(":")]
            end = b.end()
        out.append((fid, opts, cmds, end))
    return out


def run(*args, timeout=120, **kw):
    """Runs a command; one that does not end in `timeout` s stops the script."""
    try:
        return subprocess.run(list(args), capture_output=True, text=True, timeout=timeout, **kw)
    except subprocess.TimeoutExpired:
        sys.exit(f"docs-frames: {' '.join(args[:3])} did not end in {timeout} s")


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def wait(cond, error, timeout=10):
    """Polls `cond` until it is true, or stops the script with `error`."""
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        v = cond()
        if v:
            return v
        time.sleep(0.05)
    sys.exit(f"docs-frames: {error}")


def make_doc(tiza, fid, path, rng, cmds, work, sock, files=()):
    """Types `cmds` in the TUI on lines `rng` of a copy of `path`, with a
    copy of each of `files` next to it, saves the document with `:wq`, and
    returns its path."""
    shutil.copy(os.path.join(ROOT, path), work)
    for f in files:
        shutil.copy(os.path.join(ROOT, f), work)
    name = os.path.basename(path)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 120, 0, 0))

    def own_terminal():
        # The pty is the controlling terminal of the TUI: it opens /dev/tty.
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
    proc = subprocess.Popen([tiza, f"{name}:{rng}", "--socket", sock], cwd=work, env=env,
                            stdin=slave, stdout=slave, stderr=slave, close_fds=True,
                            preexec_fn=own_terminal)
    os.close(slave)

    def drain():
        while True:
            try:
                if not os.read(master, 65536):
                    return
            except OSError:
                return

    reader = threading.Thread(target=drain, daemon=True)
    reader.start()

    def type_line(text):
        # One key at a time, as a person types.
        for ch in text + "\r":
            os.write(master, ch.encode())
            time.sleep(0.005)

    def revision():
        try:
            r = subprocess.run([tiza, "--socket", sock, "api", "status", "{}"],
                               capture_output=True, text=True, timeout=10)
        except subprocess.TimeoutExpired:
            return None
        return json.loads(r.stdout)["revision"] if r.returncode == 0 else None

    def answered():
        # The revision now, when the TUI answers within 10 s.
        seen = []
        wait(lambda: seen.append(revision()) or seen[-1] is not None,
             f"{fid}: the TUI does not answer")
        return seen[-1]

    try:
        answered()
        for cmd in cmds:
            before = answered()
            type_line(cmd)
            wait(lambda: (revision() or before) > before,
                 f"{fid}: `{cmd}` does not change the document (a typo, or a text "
                 f"that lines {rng} of {path} do not have?)")
        # The TUI runs the keys in order: when it has quit, `:wq` saved the
        # document with every command in it.
        type_line(":wq")
        wait(lambda: proc.poll() is not None, f"{fid}: the TUI does not quit after :wq")
    finally:
        if proc.poll() is None:
            proc.kill()
        proc.wait()
        os.close(master)
        # No thread of this frame is left when the next TUI starts.
        reader.join(10)
    doc = os.path.join(work, f"{name}.{rng}.tiza")
    if not os.path.exists(doc):
        sys.exit(f"docs-frames: {fid}: no {doc} after :wq")
    return doc


def narrowest_width(tiza, doc, work, code_has_ellipsis):
    for w in range(20, 401):
        c = json.loads(run(tiza, "check", doc, "--width", str(w), "--json", cwd=work).stdout)
        if c["errors"] or c["warnings"]:
            continue
        txt = run(tiza, "render", doc, "--format", "txt", "--width", str(w), "--out", "-", cwd=work).stdout
        if "…" in txt and not code_has_ellipsis:
            continue
        return w
    sys.exit(f"docs-frames: no width up to 400 where {doc} is clean")


def webp_width(path):
    """Width in pixels of a WebP file, from the header of its first chunk
    (the WebP container format: VP8L, VP8X or VP8)."""
    with open(path, "rb") as f:
        b = f.read(30)
    chunk = b[12:16]
    if b[:4] != b"RIFF" or b[8:12] != b"WEBP" or len(b) < 30:
        sys.exit(f"docs-frames: {path} is not a WebP file")
    if chunk == b"VP8L":
        return (int.from_bytes(b[21:25], "little") & 0x3FFF) + 1
    if chunk == b"VP8X":
        return int.from_bytes(b[24:27], "little") + 1
    if chunk == b"VP8 ":
        return int.from_bytes(b[26:28], "little") & 0x3FFF
    sys.exit(f"docs-frames: {path} has a first chunk {chunk!r}")


def frame_pages():
    pages = []
    for d, _, files in os.walk(os.path.join(ROOT, "docs")):
        for f in files:
            p = os.path.join(d, f)
            if f.endswith(".md") and "<!-- frame " in read(p):
                pages.append(os.path.relpath(p, ROOT))
    return sorted(pages)


def main():
    no_config()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("tiza", help="the tiza binary")
    ap.add_argument("--out", default=ROOT, help="write the pages and the images under this folder")
    ap.add_argument("--only-doc", action="store_true",
                    help="only the doc= frames (no TUI): the check of the docs site")
    ap.add_argument("pages", nargs="*", help="pages relative to the repository (default: every page with a frame)")
    # Intermixed: `TIZA --out DIR PAGE` and `TIZA PAGE --out DIR` both work.
    a = ap.parse_intermixed_args()
    tiza = os.path.abspath(a.tiza)
    if not os.access(tiza, os.X_OK):
        sys.exit(f"docs-frames: {tiza} is not a program")
    # Real paths: a symbolic link cannot take a write outside the folders.
    out = os.path.realpath(a.out)
    pages = a.pages or frame_pages()
    root = os.path.realpath(ROOT)
    docs = os.path.join(root, "docs")
    for page_path in pages:
        full = os.path.realpath(os.path.join(ROOT, page_path))
        if os.path.isabs(page_path) or os.path.commonpath([full, docs]) != docs:
            sys.exit(f"docs-frames: {page_path} is not a page under docs/")
    if not pages:
        print("no page under docs/ has a frame comment")
        return
    # Every check first, so that a bad page writes nothing.
    seen = set()
    for page_path in pages:
        page = read(os.path.join(ROOT, page_path))
        for fid, opts, _, end in frames_of(page_path, page):
            if not re.fullmatch(r"[A-Za-z0-9_-]+", fid):
                sys.exit(f"docs-frames: {page_path}: the frame id {fid!r} has a character "
                         "other than a letter, a digit, - or _")
            if fid in seen:
                sys.exit(f"docs-frames: {page_path}: a second frame has the id {fid}")
            seen.add(fid)
            key = "code" if "code" in opts else "doc"
            rel = opts[key].rsplit(":", 1)[0] if key == "code" else opts[key]
            path = os.path.realpath(os.path.join(ROOT, rel))
            if os.path.isabs(rel) or os.path.commonpath([path, root]) != root \
                    or not os.path.isfile(path):
                sys.exit(f"docs-frames: {page_path}: frame {fid}: {key}={opts[key]} is not a file "
                         "of the repository")
            if key == "doc" and not rel.endswith(".tiza"):
                sys.exit(f"docs-frames: {page_path}: frame {fid}: doc={rel} is not a .tiza document")
            names = {os.path.basename(rel)}
            for f in opts.get("files", "").split(",") if "files" in opts else []:
                fp = os.path.realpath(os.path.join(ROOT, f))
                if not f or os.path.isabs(f) or os.path.commonpath([fp, root]) != root \
                        or not os.path.isfile(fp):
                    sys.exit(f"docs-frames: {page_path}: frame {fid}: files= {f!r} is not a file "
                             "of the repository")
                if os.path.basename(f) in names:
                    sys.exit(f"docs-frames: {page_path}: frame {fid}: two files have the name "
                             f"{os.path.basename(f)}")
                names.add(os.path.basename(f))
            img = re.search(r'<img src="([^"]*/%s\.webp)"' % re.escape(fid), page[end:])
            if not img:
                sys.exit(f"docs-frames: {page_path}: frame {fid} has no <img> after it")
            dest = os.path.realpath(os.path.join(out, os.path.dirname(page_path), img.group(1)))
            if os.path.commonpath([dest, out]) != out:
                sys.exit(f"docs-frames: {page_path}: the <img> of {fid} points outside {out}")
        dest_page = os.path.realpath(os.path.join(out, page_path))
        if os.path.commonpath([dest_page, out]) != out:
            sys.exit(f"docs-frames: {page_path} would go outside {out}")
    total = frames = 0
    # A socket path has at most 104 bytes: keep the folder short.
    tmp_base = None if len(tempfile.gettempdir()) < 60 else "/tmp"
    with tempfile.TemporaryDirectory(prefix="docs-frames-", dir=tmp_base) as tmp:
        for page_path in pages:
            page = read(os.path.join(ROOT, page_path))
            for fid, opts, cmds, end in frames_of(page_path, page):
                if a.only_doc and "doc" not in opts:
                    continue
                img = re.search(r'<img src="([^"]*/%s\.webp)"' % re.escape(fid), page[end:])
                dest = os.path.realpath(os.path.join(out, os.path.dirname(page_path), img.group(1)))
                work = os.path.join(tmp, fid)
                os.makedirs(work)
                sock = os.path.join(tmp, f"{frames}.sock")
                frames += 1
                if "code" in opts:
                    path, rng = opts["code"].rsplit(":", 1)
                    files = opts["files"].split(",") if "files" in opts else []
                    doc = make_doc(tiza, fid, path, rng, cmds, work, sock, files)
                    ellipsis = "…" in read(os.path.join(ROOT, path))
                else:
                    # The document as it is in the repository: the export
                    # only reads it.
                    doc = os.path.join(ROOT, opts["doc"])
                    wide = run(tiza, "render", doc, "--format", "txt", "--width", "400", "--out", "-",
                               cwd=work)
                    if wide.returncode:
                        sys.exit(f"docs-frames: {fid}: {wide.stderr.strip()}")
                    ellipsis = "…" in wide.stdout
                width = opts.get("width") or narrowest_width(tiza, doc, work, ellipsis)
                theme = [] if opts.get("theme") == "doc" else ["--theme", opts.get("theme", "dark")]
                if "animate" in opts:
                    # Every step, with the typing and the enter effects.
                    steps = ["--skip-zero"] if opts["animate"] == "1" else []
                    steps += ["--interval", opts["interval"]] if "interval" in opts else []
                elif "step" in opts:
                    steps = ["--step", opts["step"]]
                else:
                    # The step on screen: without --step, an export shows the
                    # last step of the document, not the step it was saved at.
                    steps = ["--step", str(json.loads(read(doc))["step"])]
                os.makedirs(os.path.dirname(dest), exist_ok=True)
                r = run(tiza, "render", doc, *steps, "--format", "webp", "--font-size", "13",
                        "--scale", "2", "--width", str(width), *theme, "--out", dest, cwd=work)
                if r.returncode:
                    sys.exit(f"docs-frames: {fid}: {r.stderr.strip()}")
                css = webp_width(dest) // 2
                page = re.sub(r'(<img src="[^"]*/%s\.webp" width=")[^"]*(")' % re.escape(fid),
                              rf"\g<1>{css}\g<2>", page)
                size = os.path.getsize(dest)
                total += size
                print(f"{fid:<22} {os.path.basename(page_path):<20} {width:>3} columns  {css:>4} px  {size:>7} bytes",
                      flush=True)
            dest_page = os.path.realpath(os.path.join(out, page_path))
            os.makedirs(os.path.dirname(dest_page), exist_ok=True)
            with open(dest_page, "w", encoding="utf-8") as f:
                f.write(page)
    print(f"total {total} bytes")


if __name__ == "__main__":
    main()
