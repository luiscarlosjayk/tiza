#!/usr/bin/env python3
"""Captures of the TUI for the docs pages: the real TUI in a terminal, as
near-lossless WebP images.

A `tiza render` export draws the code and the annotations. Some parts of
Tiza show only on the screen: presentation mode and its step counter, the
review sign of supervised mode, the live pointer, the focus, the panels and
the view that a `scroll` move places. This script runs the TUI in VHS, a
terminal in a headless browser, and takes screenshots of it. CAPTURES below
gives each capture: the document, the size of the terminal, the keys, and
the pages that show it.

The same for every capture:
- the font: Menlo at 26 px, in cells of about 16 x 31 px. Menlo comes with
  macOS.
  The exports use JetBrains Mono, so a capture and an export do not have the
  same glyphs;
- the dark theme of Tiza, in a terminal with the same background (#15171B);
- the size of the terminal, COLS x ROWS of the capture, set with `stty`;
- an empty config file and config folder, TIZA_NO_OSC=1, and `--no-save` on
  a copy of the document in a temporary folder.

A still capture is one screenshot, made a WebP with `cwebp`. An animated
capture is a series of screenshots, each one with its time. Equal screenshots
in a row become one frame, and `img2webp` makes an animated WebP. Both are
near-lossless (NEAR_LOSSLESS). A still capture gives the same bytes at each run, unless it
shows a time (the review shows how long each change waited). In an animated
capture, the frames of an enter effect can change from run to run, because
they depend on the time.

In each page of a capture, its <img> gets the width of the image in CSS
pixels: half its pixels.

Usage: tui_captures.py TIZA [--out DIR] [--list] [ID...]
Without ID: every still capture. An animated capture changes at each run, so
it runs only when you name it. --list prints the captures and their pages.
With --out, the images and the pages go under DIR, at their paths in the
repository, and the repository does not change.

Needs macOS (for Menlo), vhs (it runs ttyd and a headless Chromium), cwebp
and img2webp (from libwebp). It runs one vhs at a time and never publishes.
"""
import argparse
import atexit
import hashlib
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
IMAGES = "docs/images/tui"
# A row of VHS 0.12.1 is 30.77 px high at this size, so a cell is 16 x 31 px.
FONT, FONT_SIZE, CELL = "Menlo", 26, (16, 31)
PAD = 20
# The terminal of VHS uses some pixels beside the cells: this margin keeps all
# COLS columns and ROWS rows in the window. Each capture checks it before
# `stty` (see `natural.txt`): a terminal of VHS with fewer rows than Tiza
# draws puts the last row of Tiza on its own last row.
EXTRA = (40, 16)
TOOLS = ["vhs", "ttyd", "ffmpeg", "cwebp", "img2webp"]
# Near-lossless WebP at level 60: 23 % smaller than lossless for these
# screens, and no difference shows at 4x zoom (measured on present-step and
# live-pointer, 2026-10-30).
NEAR_LOSSLESS = "60"
THEME = {
    "name": "tiza-dark",
    "background": "#15171B",
    "foreground": "#E8E6E1",
    "cursor": "#E8E6E1",
    "selection": "#3A2A22",
    "black": "#15171B",
    "red": "#F47C6F",
    "green": "#98C379",
    "yellow": "#E2C27A",
    "blue": "#61AFEF",
    "magenta": "#C678DD",
    "cyan": "#56B6C2",
    "white": "#E8E6E1",
    "brightBlack": "#5A6170",
    "brightRed": "#F47C6F",
    "brightGreen": "#98C379",
    "brightYellow": "#E2C27A",
    "brightBlue": "#61AFEF",
    "brightMagenta": "#C678DD",
    "brightCyan": "#56B6C2",
    "brightWhite": "#FFFFFF",
}


def keys(*names):
    """VHS lines for keys: one character is typed, a longer name is a key
    of VHS (Enter, Escape, Right...)."""
    return [("vhs", f'Type "{k}"' if len(k) == 1 else k) for k in names]


def command(text):
    """A `:` command of Tiza, typed and sent with Enter."""
    return [("vhs", f"Type `{text}`"), ("vhs", "Enter")]


# The document of the `scroll` capture: a camera step (2) between a note at
# the top of the code (1) and a box that draws in at the target (3). The
# terminal shows 8 lines of code, so the view must move to show line 15.
SCROLL_DOC = {
    "schema": 1,
    "title": "A camera step",
    "source": {"language": "rs", "first_line": 1},
    "code": {"file": "ownership.rs"},
    "theme": "dark",
    "padding": [1, 2],
    "moves": [{"at_step": 2, "action": "scroll", "find": "fn takes", "align": "top"}],
    "annotations": [
        {"id": "n1", "step": 1, "type": "note", "line": 2, "text": "s is born"},
        {
            "id": "b1",
            "step": 3,
            "type": "box",
            "from_line": 15,
            "to_line": 17,
            "label": "takes owns s and frees it",
            "effect": "draw",
        },
    ],
}

# The document of the `badge-labels` capture: the code fits in 70 columns,
# but the label of its highlight does not.
BADGE_DOC = {
    "schema": 1,
    "title": "A label that does not fit",
    "source": {"language": "rs", "first_line": 1},
    "code": ["fn total(price: u32, count: u32) -> u32 {", "    price * count", "}"],
    "annotations": [
        {
            "id": "h1",
            "step": 1,
            "type": "highlight",
            "span": {"line": 2, "start": 4, "end": 17},
            "label": "the price of one item times the number of items, before the tax and the discount",
        }
    ],
}

OWNERSHIP = ["examples/ownership.rs.tiza", "examples/ownership.rs"]
TALLY = ["examples/tally-tour.tiza", "examples/tally"]


def normal(cid, pages, do, step="3", files=OWNERSHIP, doc="ownership.rs.tiza", size=(100, 30)):
    """A capture in normal mode, at a step of a document."""
    total = "6" if doc == "ownership.rs.tiza" else "?"
    return {
        "id": cid,
        "pages": pages,
        "files": files,
        "args": ["--step", step, doc],
        "size": size,
        "ready": rf"step {step}\/" + (total if total != "?" else ""),
        "do": do,
    }


CAPTURES = [
    {
        "id": "present-step",
        "pages": ["docs/reference/keys.md", "docs/guides/present.md"],
        "files": OWNERSHIP,
        "args": ["--present", "--step", "3", "ownership.rs.tiza"],
        "size": (100, 30),
        "ready": r"3\/6",
        "do": [("shot", 0)],
    },
    {
        "id": "present-review",
        "pages": ["docs/guides/supervise-an-agent.md"],
        "files": OWNERSHIP,
        "args": ["--present", "--step", "3", "ownership.rs.tiza"],
        "size": (100, 30),
        "ready": r"3\/6",
        "agent": [{"type": "note", "line": 12, "text": "push changes u here"}],
        "do": command(":supervise on")
        + [("wait", r"1 to review"), ("sleep", 2500), ("shot", 0)],
    },
    {
        "id": "live-pointer",
        "pages": ["docs/reference/keys.md"],
        "files": OWNERSHIP,
        "args": ["--present", "--step", "3", "ownership.rs.tiza"],
        "size": (100, 30),
        "ready": r"3\/6",
        # The pointer goes to line 1, to line 6 and off, then on with lines 6 to 8.
        "do": keys("f", "g", "j", "j", "j", "j", "j", "f", "V", "j", "j", "f")
        + [("sleep", 500), ("shot", 0)],
    },
    {
        "id": "focus",
        "pages": ["docs/reference/commands/view.md"],
        "files": OWNERSHIP,
        "args": ["--step", "3", "ownership.rs.tiza"],
        "size": (100, 30),
        "ready": r"step 3\/6",
        "do": command(":focus 6-8") + [("sleep", 500), ("shot", 0)],
    },
    {
        "id": "help",
        "pages": ["docs/reference/keys.md"],
        "files": OWNERSHIP,
        "args": ["--step", "3", "ownership.rs.tiza"],
        "size": (100, 30),
        "ready": r"step 3\/6",
        "do": keys("?") + [("sleep", 800), ("shot", 0)],
    },
    {
        "id": "list",
        "pages": ["docs/reference/keys.md"],
        "files": OWNERSHIP,
        "args": ["--step", "3", "ownership.rs.tiza"],
        "size": (100, 30),
        "ready": r"step 3\/6",
        "do": keys("L") + [("sleep", 800), ("shot", 0)],
    },
    {
        "id": "review",
        "pages": ["docs/reference/keys.md", "docs/guides/supervise-an-agent.md"],
        "files": OWNERSHIP,
        "args": ["--step", "3", "ownership.rs.tiza"],
        "size": (100, 30),
        "ready": r"step 3\/6",
        "agent": [
            {"type": "note", "line": 7, "text": "len only reads t"},
            {"type": "note", "line": 11, "text": "push changes u"},
        ],
        "do": command(":supervise on")
        + [("wait", r"2 to review")]
        + command(":review")
        # The review shows how long each change waited: shoot while both say 0s.
        + [("wait", r"changes u. +0s"), ("shot", 0)],
    },
    normal(
        "status-bar-link",
        ["docs/reference/document-format.md"],
        [("sleep", 500), ("shot", 0)],
        step="2",
        files=TALLY,
        doc="tally-tour.tiza",
    ),
    normal("layout-dynamic", ["docs/reference/commands/view.md"],
           command(":layout dynamic") + [("sleep", 500), ("shot", 0)], step="1"),
    normal("layout-stable", ["docs/reference/commands/view.md"],
           command(":layout stable") + [("sleep", 500), ("shot", 0)], step="1"),
    # A code line wider than the terminal gives the first badge.
    normal("badge-code", ["docs/guides/check-before-you-record.md"],
           [("wait", r"code needs"), ("shot", 0)], size=(58, 20)),
    # The code fits, but the label does not.
    {
        "id": "badge-labels",
        "pages": ["docs/guides/check-before-you-record.md"],
        "files": [],
        "document": ("badge.tiza", BADGE_DOC),
        "args": ["--step", "1", "badge.tiza"],
        "size": (70, 12),
        "ready": r"step 1\/1",
        "do": [("wait", r"labels? cut"), ("shot", 0)],
    },
    # An export does not use the padding of the TUI: the padding of the
    # document (1 row and 2 columns), then `:pad 0 0`.
    normal("pad-doc", ["docs/reference/commands/view.md"], [("sleep", 300), ("shot", 0)]),
    normal("pad-zero", ["docs/reference/commands/view.md"],
           command(":pad 0 0") + [("sleep", 500), ("shot", 0)]),
    # VHS sends no mouse events: `v` and `e` make the same selection as a drag
    # over `String` with the left button, and the screen is the same.
    normal("selection", ["docs/guides/present.md", "docs/reference/keys.md"],
           keys("g", "j", "j", "j", "j", "j", "0", "w", "w", "w", "v", "e")
           + [("wait", r"SELECT"), ("sleep", 300), ("shot", 0)]),
    {
        "id": "scroll",
        "animated": True,
        "pages": ["docs/reference/document-format.md"],
        "files": ["examples/ownership.rs"],
        "document": ("scroll.tiza", SCROLL_DOC),
        "args": ["--present", "--step", "1", "scroll.tiza"],
        "size": (100, 10),
        "ready": r"1\/3",
        "do": [("shot", 1800)]
        + keys("Right")
        + [("wait", r"2\/3"), ("sleep", 300), ("shot", 1800)]
        + keys("Right")
        + [("burst", 30, 40), ("sleep", 300), ("shot", 2500)],
    },
]

# The agent of a capture: when supervised mode is on, it asks for each
# change. Each request waits for the decision of the presenter, so each one
# runs in the background. It also writes the status of the instance, to check
# the size of the terminal.
HELPER = r"""#!/bin/bash
tiza=$1; sock=$2; shift 2
until "$tiza" api status --socket "$sock" > status.json 2>/dev/null; do sleep 0.1; done
[ $# -eq 0 ] && exit 0
until "$tiza" api status --socket "$sock" 2>/dev/null | grep -Eq '"supervised": ?true'; do
  sleep 0.1
done
for p in "$@"; do
  "$tiza" api annotation.add "$p" --socket "$sock" > /dev/null 2>&1 &
  sleep 0.3
done
wait
"""


def fail(msg):
    sys.exit(f"tui_captures.py: {msg}")


def check_tools():
    if sys.platform != "darwin" or not os.path.exists("/System/Library/Fonts/Menlo.ttc"):
        fail("needs macOS, for the Menlo font of the captures")
    missing = [t for t in TOOLS if shutil.which(t) is None]
    if missing:
        fail(f"needs {', '.join(missing)} on PATH; on macOS: brew install vhs webp")


def tape(cap, tiza, tmp):
    """The VHS tape of a capture, and the screenshots that it takes: (file,
    time in ms)."""
    cols, rows = cap["size"]
    width = 2 * PAD + EXTRA[0] + cols * CELL[0]
    height = 2 * PAD + EXTRA[1] + rows * CELL[1]
    q = shlex.quote
    sock = q(os.path.join(tmp, "s.sock"))
    home, tiza = q(os.path.join(tmp, "home")), q(tiza)
    env = (
        f"export HOME={home} XDG_CONFIG_HOME={home} TIZA_CONFIG={q(os.path.join(tmp, 'config.toml'))}"
        " TIZA_NO_OSC=1 PS1=''"
    )
    agent = " ".join(q(json.dumps(p)) for p in cap.get("agent", []))
    args = " ".join(q(a) for a in cap["args"])
    lines = [
        'Output "vhs.gif"',
        'Set Shell "bash"',
        f'Set FontFamily "{FONT}"',
        f"Set FontSize {FONT_SIZE}",
        f"Set Width {width}",
        f"Set Height {height}",
        f"Set Padding {PAD}",
        "Set CursorBlink false",
        "Set TypingSpeed 10ms",
        f"Set Theme {json.dumps(THEME)}",
        "Hide",
        f"Type `cd {q(tmp)} && {env} && tput cols > natural.txt && tput lines >> natural.txt"
        f" && stty cols {cols} rows {rows} && clear`",
        "Enter",
        f"Type `bash helper.sh {tiza} {sock} {agent} &`",
        "Enter",
        f"Type `clear && {tiza} --no-save --socket {sock} --theme dark {args}`",
        "Enter",
        f"Wait+Screen@15s /{cap['ready']}/",
        "Sleep 300ms",
    ]
    shots = []

    def shot(ms):
        name = f"shot-{len(shots):03}.png"
        shots.append((name, ms))
        return [f'Screenshot "{name}"']

    for act in cap["do"]:
        kind = act[0]
        if kind == "vhs":
            lines.append(act[1])
        elif kind == "wait":
            lines.append(f"Wait+Screen@15s /{act[1]}/")
        elif kind == "sleep":
            lines.append(f"Sleep {act[1]}ms")
        elif kind == "shot":
            lines += ["Show"] + shot(act[1]) + ["Sleep 200ms", "Hide"]
        elif kind == "burst":
            lines.append("Show")
            for _ in range(act[1]):
                lines += shot(act[2]) + [f"Sleep {act[2]}ms"]
            lines.append("Hide")
    return "\n".join(lines) + "\n", shots


def capture(cap, tiza, out_dir):
    """Runs the tape of a capture and writes its WebP. Returns its path."""
    # A socket path has at most 104 bytes: keep the folder short.
    tmp = tempfile.mkdtemp(prefix="tz-", dir=None if len(tempfile.gettempdir()) < 60 else "/tmp")
    atexit.register(shutil.rmtree, tmp, True)
    os.makedirs(os.path.join(tmp, "home"))
    open(os.path.join(tmp, "config.toml"), "w").close()
    for f in cap["files"]:
        src = os.path.join(ROOT, f)
        if os.path.isdir(src):
            shutil.copytree(src, os.path.join(tmp, os.path.basename(f)))
        else:
            shutil.copy(src, tmp)
    if "document" in cap:
        name, doc = cap["document"]
        with open(os.path.join(tmp, name), "w") as fh:
            json.dump(doc, fh, indent=1)
    with open(os.path.join(tmp, "helper.sh"), "w") as fh:
        fh.write(HELPER)
    text, shots = tape(cap, tiza, tmp)
    with open(os.path.join(tmp, "capture.tape"), "w") as fh:
        fh.write(text)
    # vhs runs ttyd, a browser, a shell, Tiza and the helper: in its own group,
    # so that a timeout or Ctrl-C stops all of them and nothing else.
    proc = subprocess.Popen(["vhs", "capture.tape"], cwd=tmp, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, start_new_session=True)
    try:
        log, _ = proc.communicate(timeout=300)
    except BaseException:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        fail(f"{cap['id']}: vhs did not end in 300 s, or the run was stopped")
    if proc.returncode != 0:
        fail(f"{cap['id']}: vhs failed:\n{log[-2000:]}")
    # With `stty`, Tiza draws COLS x ROWS cells. A smaller terminal of VHS would
    # show them wrong: a row past its last row lands on its last row.
    natural = [int(n) for n in open(os.path.join(tmp, "natural.txt")).read().split()]
    if natural[0] < cap["size"][0] or natural[1] < cap["size"][1]:
        fail(f"{cap['id']}: the terminal of VHS has {natural[0]}x{natural[1]} cells, "
             f"less than {cap['size'][0]}x{cap['size'][1]}; make EXTRA larger")
    try:
        status = json.load(open(os.path.join(tmp, "status.json")))
    except (OSError, ValueError):
        fail(f"{cap['id']}: the instance gave no status; the TUI did not start")
    if (status.get("cols"), status.get("rows")) != tuple(cap["size"]):
        fail(f"{cap['id']}: the terminal was {status.get('cols')}x{status.get('rows')}, "
             f"not {cap['size'][0]}x{cap['size'][1]}")
    out = os.path.join(out_dir, IMAGES, cap["id"] + ".webp")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    if len(shots) == 1:
        cmd = ["cwebp", "-quiet", "-near_lossless", NEAR_LOSSLESS,
               os.path.join(tmp, shots[0][0]), "-o", out]
    else:
        frames = []
        for name, ms in shots:
            digest = hashlib.sha256(open(os.path.join(tmp, name), "rb").read()).digest()
            if frames and frames[-1][2] == digest:
                frames[-1][1] += ms
            else:
                frames.append([name, ms, digest])
        cmd = ["img2webp", "-loop", "0", "-lossless", "-near_lossless", NEAR_LOSSLESS]
        for name, ms, _ in frames:
            cmd += ["-d", str(max(ms, 20)), os.path.join(tmp, name)]
        cmd += ["-o", out]
    subprocess.run(cmd, check=True, capture_output=True)
    return out, os.path.join(tmp, shots[0][0])


def png_width(path):
    with open(path, "rb") as fh:
        head = fh.read(24)
    return int.from_bytes(head[16:20], "big")


def set_width(cap, px, out_dir):
    """Gives each <img> of the capture, in its pages, half the pixel width."""
    src = re.compile(r'(<img src="[^"]*images/tui/' + re.escape(cap["id"]) + r'\.webp" width=")\d+(")')
    for page in cap["pages"]:
        path = os.path.join(ROOT, page)
        dest = os.path.join(out_dir, page)
        # With --out, a page with two captures keeps the width of the first.
        text = open(dest if os.path.exists(dest) else path).read()
        new, n = src.subn(lambda m: f"{m.group(1)}{px // 2}{m.group(2)}", text)
        if n == 0:
            print(f"note: {page} has no <img> of {cap['id']}", file=sys.stderr)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        if new != text or not os.path.exists(dest):
            with open(dest, "w") as fh:
                fh.write(new)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("tiza")
    ap.add_argument("ids", nargs="*")
    ap.add_argument("--out")
    ap.add_argument("--list", action="store_true")
    a = ap.parse_intermixed_args()
    by_id = {c["id"]: c for c in CAPTURES}
    if a.list:
        for c in CAPTURES:
            print(f"{c['id']}: {c['size'][0]}x{c['size'][1]}, {', '.join(c['pages'])}")
        return
    unknown = [i for i in a.ids if i not in by_id]
    if unknown:
        fail(f"no capture {', '.join(unknown)}; --list gives them")
    check_tools()
    tiza = os.path.abspath(a.tiza)
    out_dir = os.path.abspath(a.out) if a.out else ROOT
    for c in [by_id[i] for i in a.ids] or [c for c in CAPTURES if not c.get("animated")]:
        out, first = capture(c, tiza, out_dir)
        set_width(c, png_width(first), out_dir)
        print(f"wrote {os.path.relpath(out, out_dir)} ({os.path.getsize(out)} bytes)")


if __name__ == "__main__":
    main()
