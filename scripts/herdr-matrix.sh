#!/usr/bin/env bash
# Terminal size matrix with Herdr panes: the same frame at several
# real terminal sizes, captured from a real terminal emulator (Herdr renders
# through libghostty-vt) and compared with committed snapshots. A QA-round
# step on a machine with Herdr (Luis's Mac), never part of `cargo test`.
#
#   scripts/herdr-matrix.sh              # capture into qa/herdr/latest/, print sizes
#   scripts/herdr-matrix.sh --check      # capture and diff with qa/herdr/*.{txt,ansi} (exit 1 on changes)
#   scripts/herdr-matrix.sh --update     # capture and rewrite qa/herdr/*.{txt,ansi} on purpose
#   scripts/herdr-matrix.sh --normalize F.ansi   # print what --check compares of an ansi capture (no Herdr)
#
# Options:
#   --file DOC        document to open (default: examples/ownership.rs.tiza, only read)
#   --tiza BIN        tiza binary (default: builds and uses target/debug/tiza)
#   --base PANE_ID    split this pane instead of working in a new tab
#   --snapshots DIR   committed snapshots (default: qa/herdr)
#   --out DIR         where captures land (default: qa/herdr/latest, git-ignored)
#
# Run it from a Herdr pane. By default it opens a tab of its own in the
# current pane's workspace (`herdr tab create --workspace W --cwd REPO
# --label tiza-matrix`), works from that tab's root pane and closes the tab
# at the end, also on errors or Ctrl-C: your panes are never split. With
# --base it splits that pane instead and closes every pane it opened.
#
# Sizes are the TERMINAL's, the ones Tiza sees. Herdr's `pane layout` rect
# counts the pane's frame too, so the script first measures that frame: it
# splits one probe pane, starts Tiza there and takes rect - `tiza api status`
# (3 columns x 2 rows on Herdr 0.9.5). Each case's pane is then split off the
# base already at target + frame (`herdr pane split BASE --direction right
# --ratio R`, R = 1 - (target_cols + frame)/base_width: the ratio is what the
# base keeps) and, for a row target, split again `--direction down` the same
# way. A pane that misses by a cell or so is fixed with `herdr pane resize`,
# always in the direction that GROWS the pane resized: to grow ours, ours
# towards the base (left/up); to shrink ours, the neighbour towards ours
# (right/down) -- Herdr does not move an edge a pane does not share. Then it
# launches Tiza there with a socket of its own; if Tiza does not see the
# target (a pane whose frame differs), the pane is resized by the difference.
# It waits for a stable first frame and saves `herdr pane read --source
# visible --format text` as <snapshots>/<case>-<cols>x<rows>.txt, with the
# size Tiza sees (Herdr's rect is printed next to it), and `--format ansi`
# next to it as <case>-<cols>x<rows>.ansi, the colours and
# attributes the text capture loses.
#
# --check compares the .txt byte for byte and the .ansi by meaning: both
# sides are reduced to the text of each row ("T row|text") and the runs of
# one style ("S row:first-last style", terminal cells from 1, style as
# fg=/bg=/ul= colours plus attributes; see ANSI_PY), so the same frame
# written with other bytes (`\e[m` for `\e[0m`, `38:2::r:g:b` for
# `38;2;r;g;b`, combined or separate sequences, `\e(B` or other non-SGR
# escapes, CRLF, palette colours 16-255 as RGB) is no change. A change is
# reported as `text differs` (the .txt), `text differs in the ansi capture`
# (same .txt, other text in the .ansi) or `style only`.
#
# Only the herdr 0.9.5 CLI is used: tab create/close and pane layout/list/
# split/resize/run/read/close. Everything that talks to Herdr goes through
# `herdr()` below; with TIZA_HERDR_FAKE=<state dir> it is simulated (panes
# are Python ptys, `pane read` is the live instance's `tiza render`), so
# A QA test can drive the rest of this script without Herdr.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE=capture
DOC="$ROOT/examples/ownership.rs.tiza"
TIZA_BIN="${TIZA:-}"
BASE=""
SNAPS="$ROOT/qa/herdr"
OUT=""
NORMALIZE=""
TIMEOUT="${TIZA_HERDR_TIMEOUT:-15}"   # seconds per wait (socket up, first frame)
TOL=1                                 # cells a pane may miss its target by
LABEL="tiza-matrix"

die() {
    echo "herdr-matrix: $*" >&2
    exit 2
}

while [ $# -gt 0 ]; do
    case "$1" in
    --check) MODE=check ;;
    --update) MODE=update ;;
    --file) DOC="${2:?--file needs a path}"; shift ;;
    --tiza) TIZA_BIN="${2:?--tiza needs a path}"; shift ;;
    --base) BASE="${2:?--base needs a pane id}"; shift ;;
    --snapshots) SNAPS="${2:?--snapshots needs a directory}"; shift ;;
    --out) OUT="${2:?--out needs a directory}"; shift ;;
    --normalize) NORMALIZE="${2:?--normalize needs an .ansi file}"; shift ;;
    -h | --help)
        sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'
        exit 0
        ;;
    *) die "unknown argument: $1 (see --help)" ;;
    esac
    shift
done
[ -n "$OUT" ] || OUT="$SNAPS/latest"
[ -z "$NORMALIZE" ] || [ -f "$NORMALIZE" ] || die "file not found: $NORMALIZE"
[ -n "$NORMALIZE" ] || [ -f "$DOC" ] || die "document not found: $DOC"
DOC="$(cd "$(dirname "$DOC")" && pwd)/$(basename "$DOC")"
command -v python3 >/dev/null || die "python3 is needed (Herdr's and Tiza's JSON, the ansi captures)"
# The last step of the document: the step of its last annotation or move.
LAST=""
[ -n "$NORMALIZE" ] || LAST="$(python3 -c 'import json, sys
d = json.load(open(sys.argv[1]))
print(max([a.get("step", 0) for a in d.get("annotations", [])] + [m["at_step"] for m in d.get("moves", [])]))' "$DOC")"

# The matrix: case, target cols, target rows of the terminal Tiza sees ("max"
# = as large as the base pane allows), mode. 75x50 is Zed split in half; the
# two normal-mode cases show the status bar, and the narrow one its
# small-terminal badge.
MATRIX="vt100 80 24 present
wide 120 40 present
zed-half 75 50 present
narrow 60 30 present
full max max present
normal 120 40 normal
normal-narrow 60 30 normal"

# ---------------------------------------------------------------- Herdr ----

FAKE="${TIZA_HERDR_FAKE:-}"

herdr() {
    if [ -n "$FAKE" ]; then
        fake_herdr "$@"
    else
        command herdr "$@"
    fi
}

# A tiny model of Herdr for the tests. $FAKE/window ("cols rows") is a tab's
# area; the user's pane p0 fills tab w1:t1 of workspace w1 and is the
# current pane. `tab create --workspace w1` opens tab w1:tN with a root pane
# of the whole area, `tab close` closes it with all its panes. Each pane's
# rect is its terminal plus a frame, TIZA_HERDR_FAKE_FRAME ("3 2" by default,
# as Herdr 0.9.5 draws), or TIZA_HERDR_FAKE_FRAME_BELOW for panes split off
# `--direction down`. `split P --direction right|down --ratio R` leaves P the
# share R of its rect and puts the new pane to its right (below);
# TIZA_HERDR_FAKE_SKEW=N gives the new pane N cells more (or fewer).
# `resize --pane P --direction D --amount A` moves P's edge on side D by A x
# the split it belongs to, but only if P shares an edge on that side: a pane
# at the right never shrinks with `resize right` (Herdr 0.9.5 behaves so).
# `layout` gives the rects of one tab's panes, `close` gives the space back
# to the parent, `run` starts the command in a Python pty of the pane's
# terminal size that follows later resizes, `read` is the live instance's
# text render (`--format ansi`: its ansi render, with CRLF line ends as
# Herdr prints them). Every call is appended to $FAKE/calls.
# TIZA_HERDR_FAKE_SGR=other writes that ansi render with other SGR bytes for
# the same styles (see SGR_OTHER_PY), as another Herdr version might.
# TIZA_HERDR_FAKE_FAIL_READ=1 makes `read` fail; TIZA_HERDR_FAKE_TAB_OUTPUT=none
# makes `tab create` print nothing (the script must find the tab another way).
fake_herdr() {
    [ -f "$FAKE/window" ] || { echo "error: cannot connect to the herdr server" >&2; return 1; }
    printf '%s\n' "$*" >>"$FAKE/calls"
    local sub="$1 $2"
    shift 2
    case "$sub" in
    "pane list" | "pane layout" | "pane split" | "pane resize" | "tab create")
        python3 -c "$FAKE_MODEL" "$FAKE" "$sub" "$@"
        ;;
    "pane run")
        local pane="$1" cmd="$2"
        [ -d "$FAKE/panes/$pane" ] || { echo "error: pane not found: $pane" >&2; return 1; }
        local frame="${TIZA_HERDR_FAKE_FRAME:-3 2}" parent="" dir=""
        [ -f "$FAKE/panes/$pane/parent" ] && read -r parent dir <"$FAKE/panes/$pane/parent"
        [ "$dir" = down ] && [ -n "${TIZA_HERDR_FAKE_FRAME_BELOW:-}" ] && frame="$TIZA_HERDR_FAKE_FRAME_BELOW"
        printf '%s\n' "$frame" >"$FAKE/panes/$pane/frame"
        printf '%s\n' "$cmd" >"$FAKE/panes/$pane/cmd"
        printf '%s\n' "$cmd" >>"$FAKE/log"
        python3 -c "$FAKE_PTY_HOST" "$FAKE/panes/$pane" "$cmd" </dev/null >/dev/null 2>&1 &
        echo $! >"$FAKE/panes/$pane/pid"
        ;;
    "pane read")
        local pane="$1"
        [ -d "$FAKE/panes/$pane" ] || { echo "error: pane not found: $pane" >&2; return 1; }
        [ "${TIZA_HERDR_FAKE_FAIL_READ:-0}" != 1 ] || { echo "error: read failed (fake)" >&2; return 1; }
        local sock format=text
        case " $* " in *" --format ansi "*) format=ansi ;; esac
        sock=$(sed -n 's/.*--socket \([^ ]*\).*/\1/p' "$FAKE/panes/$pane/cmd" 2>/dev/null || true)
        [ -n "$sock" ] || return 0
        if [ "$format" = text ]; then
            "$TIZA_BIN" render --socket "$sock" --format txt --out - 2>/dev/null || true
        elif [ "${TIZA_HERDR_FAKE_SGR:-}" = other ]; then
            "$TIZA_BIN" render --socket "$sock" --format ansi --out - 2>/dev/null | python3 -c "$SGR_OTHER_PY" || true
        else
            "$TIZA_BIN" render --socket "$sock" --format ansi --out - 2>/dev/null | awk '{ printf "%s\r\n", $0 }' || true
        fi
        ;;
    "pane close")
        local pane="$1"
        [ -d "$FAKE/panes/$pane" ] || { echo "error: pane not found: $pane" >&2; return 1; }
        fake_stop "$pane"
        # Give the space back to the parent, as Herdr does.
        local parent dir pc pr c r
        if [ -f "$FAKE/panes/$pane/parent" ]; then
            read -r parent dir <"$FAKE/panes/$pane/parent"
            read -r c r <"$FAKE/panes/$pane/size"
            if [ -f "$FAKE/panes/$parent/size" ]; then
                read -r pc pr <"$FAKE/panes/$parent/size"
                if [ "$dir" = right ]; then pc=$((pc + c)); else pr=$((pr + r)); fi
                echo "$pc $pr" >"$FAKE/panes/$parent/size"
            fi
        fi
        rm -rf "${FAKE:?}/panes/$pane"
        ;;
    "tab close")
        local tab="$1" p
        [ -f "$FAKE/tabs/$tab" ] || { echo "error: tab not found: $tab" >&2; return 1; }
        for p in "$FAKE"/panes/*; do
            [ -f "$p/tab" ] && [ "$(cat "$p/tab")" = "$tab" ] || continue
            fake_stop "$(basename "$p")"
            rm -rf "$p"
        done
        rm -f "$FAKE/tabs/$tab"
        ;;
    *)
        echo "error: unknown command: $sub" >&2
        return 1
        ;;
    esac
}

# fake_stop PANE: the pane's host kills its child and exits on `stop`.
fake_stop() {
    local d="$FAKE/panes/$1" pid i=0
    [ -f "$d/pid" ] || return 0
    touch "$d/stop"
    pid=$(cat "$d/pid")
    while kill -0 "$pid" 2>/dev/null && [ $i -lt 100 ]; do
        sleep 0.05
        i=$((i + 1))
    done
    kill -9 "$pid" 2>/dev/null || true
}

# The fake layout model (see fake_herdr): panes/<id>/size is the rect "cols
# rows", panes/<id>/parent is "<parent id> right|down" for a pane split off
# another, panes/<id>/tab its tab (w1:t1 when absent), tabs/<id> one file per
# tab the model opened.
FAKE_MODEL='
import json, os, sys
d, cmd, args = sys.argv[1], sys.argv[2], sys.argv[3:]
P = os.path.join(d, "panes")
def ids():
    return sorted(os.listdir(P), key=lambda x: int(x[1:]))
def size(p):
    return [int(x) for x in open(os.path.join(P, p, "size")).read().split()]
def put(p, s):
    open(os.path.join(P, p, "size"), "w").write("%d %d\n" % (s[0], s[1]))
def parent(p):
    f = os.path.join(P, p, "parent")
    return open(f).read().split() if os.path.exists(f) else [None, None]
def tab(p):
    f = os.path.join(P, p, "tab")
    return open(f).read().strip() if os.path.exists(f) else "w1:t1"
def counter(name, first):
    f = os.path.join(d, name)
    n = int(open(f).read()) if os.path.exists(f) else first
    open(f, "w").write("%d\n" % (n + 1))
    return n
def new_pane(t):
    pid = "p%d" % counter("next", 1)
    os.makedirs(os.path.join(P, pid))
    open(os.path.join(P, pid, "tab"), "w").write(t + "\n")
    return pid
def opts(a):
    o, pos, i = {}, [], 0
    while i < len(a):
        if a[i].startswith("--"):
            nxt = a[i + 1] if i + 1 < len(a) else ""
            if nxt.startswith("--") or nxt == "":
                o[a[i][2:]] = ""
                i += 1
            else:
                o[a[i][2:]] = nxt
                i += 2
        else:
            pos.append(a[i])
            i += 1
    return o, pos
def fail(msg):
    print("error: " + msg, file=sys.stderr)
    sys.exit(1)
def exists(p):
    return os.path.isdir(os.path.join(P, p))
o, pos = opts(args)
if cmd == "pane list":
    print(json.dumps({"result": {"panes": [{"pane_id": p, "tab_id": tab(p), "workspace_id": "w1"} for p in ids()]}}))
elif cmd == "pane layout":
    w, h = [int(x) for x in open(os.path.join(d, "window")).read().split()]
    me = "p0" if "current" in o else o.get("pane", "")
    if not exists(me):
        fail("pane not found: " + me)
    t = tab(me)
    mine = [p for p in ids() if tab(p) == t]
    root = [p for p in mine if parent(p)[0] is None][0]
    panes = [{"pane_id": p, "rect": {"x": 0, "y": 0, "width": size(p)[0], "height": size(p)[1]}} for p in mine]
    print(json.dumps({"result": {"workspace_id": "w1", "tab_id": t, "zoomed": False,
        "focused_pane_id": root, "area": {"x": 0, "y": 0, "width": w, "height": h},
        "panes": panes, "splits": []}}))
elif cmd == "tab create":
    if o.get("workspace") != "w1":
        fail("workspace not found: " + o.get("workspace", "(none)"))
    t = "w1:t%d" % counter("next_tab", 2)
    os.makedirs(os.path.join(d, "tabs"), exist_ok=True)
    json.dump(o, open(os.path.join(d, "tabs", t), "w"))
    pid = new_pane(t)
    put(pid, [int(x) for x in open(os.path.join(d, "window")).read().split()])
    if os.environ.get("TIZA_HERDR_FAKE_TAB_OUTPUT") != "none":
        print(json.dumps({"result": {"tab": {"tab_id": t, "workspace_id": "w1", "label": o.get("label")},
            "root_pane": {"pane_id": pid, "tab_id": t}}}))
elif cmd == "pane split":
    par = pos[0] if pos else o.get("pane", "p0")
    if not exists(par):
        fail("pane not found: " + par)
    dirn, ratio = o.get("direction", "right"), float(o.get("ratio", "0.5"))
    f = 0 if dirn == "right" else 1
    s = size(par)
    new = s[f] - int(s[f] * ratio + 0.5) + int(os.environ.get("TIZA_HERDR_FAKE_SKEW", "0"))
    new = max(2, min(s[f] - 2, new))
    pid = new_pane(tab(par))
    c = list(s)
    c[f] = new
    s[f] -= new
    put(par, s)
    put(pid, c)
    open(os.path.join(P, pid, "parent"), "w").write("%s %s\n" % (par, dirn))
    print(json.dumps({"result": {"pane": {"pane_id": pid}}}))
elif cmd == "pane resize":
    pane, dirn, amount = o.get("pane", ""), o.get("direction", ""), float(o.get("amount", "0.05"))
    if not exists(pane):
        fail("pane not found: " + pane)
    axis = "right" if dirn in ("left", "right") else "down"
    f = 0 if axis == "right" else 1
    def band(p):
        # p and the panes split off it across the other axis: they share
        # the edges of p along this axis and change with it.
        out = [p]
        for q in ids():
            a, b = parent(q)
            if a == p and b != axis:
                out += band(q)
        return out
    if dirn in ("left", "up"):
        # The low-side edge: the nearest split along this axis above pane.
        x = pane
        while True:
            a, b = parent(x)
            if a is None:
                sys.exit(0)  # no edge on that side: nothing moves
            if b == axis:
                break
            x = a
        grow, shrink = x, a
    else:
        kids = [q for q in ids() if parent(q) == [pane, axis]]
        if not kids:
            sys.exit(0)  # a pane at the right (bottom) has no edge there
        grow, shrink = pane, kids[-1]
    g, t = size(grow)[f], size(shrink)[f]
    delta = int(amount * (g + t) + 0.5)
    delta = max(0, min(delta, t - 2))
    for p in band(grow):
        s = size(p)
        s[f] += delta
        put(p, s)
    for p in band(shrink):
        s = size(p)
        s[f] -= delta
        put(p, s)
'

# The fake pane: runs `bash -c CMD` on a pty of the pane's terminal size
# (DIR/size minus DIR/frame), applies every later change of that file (the
# kernel sends SIGWINCH), drains the output and ends the child when DIR/stop
# appears.
FAKE_PTY_HOST='
import fcntl, os, pty, select, signal, struct, sys, termios, time
d, cmd = sys.argv[1], sys.argv[2]
fc, fr = [int(x) for x in open(os.path.join(d, "frame")).read().split()]
def size():
    c, r = open(os.path.join(d, "size")).read().split()
    return max(1, int(c) - fc), max(1, int(r) - fr)
pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    os.execvp("bash", ["bash", "-c", cmd])
last = None
while True:
    if os.path.exists(os.path.join(d, "stop")):
        break
    try:
        s = size()
    except (OSError, ValueError):
        break
    if s != last:
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", s[1], s[0], 0, 0))
        last = s
    r, _, _ = select.select([fd], [], [], 0.05)
    if r:
        try:
            if not os.read(fd, 65536):
                break
        except OSError:
            break
    done, _ = os.waitpid(pid, os.WNOHANG)
    if done:
        sys.exit(0)
try:
    os.kill(pid, signal.SIGTERM)
    for _ in range(40):
        if os.waitpid(pid, os.WNOHANG)[0]:
            sys.exit(0)
        time.sleep(0.05)
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
except (OSError, ChildProcessError):
    pass
'

# The fake `pane read --format ansi` of TIZA_HERDR_FAKE_SGR=other: the same
# styles in other bytes -- foregrounds as `38:2::r:g:b`, neighbouring SGR
# sequences merged into one, `\e[0m` as `\e[m`, CRLF line ends.
SGR_OTHER_PY='
import re, sys
s = sys.stdin.read()
s = re.sub(r"\x1b\[38;2;(\d+);(\d+);(\d+)m", lambda m: "\x1b[38:2::%s:%s:%sm" % m.groups(), s)
merge = re.compile(r"\x1b\[([0-9;:]+)m\x1b\[([0-9;:]+)m")
while merge.search(s):
    s = merge.sub(lambda m: "\x1b[%s;%sm" % m.groups(), s)
s = s.replace("\x1b[0m", "\x1b[m").replace("\n", "\r\n")
sys.stdout.write(s)
'

# --------------------------------------------------------- ansi captures ----

# An ansi capture → its meaning, the same however the SGR bytes are written:
# "T row|text" per row (trailing blanks and blank rows at the end trimmed, as
# in the .txt) and "S row:first-last style" per run of cells in one style
# that is not the default, columns counted in terminal cells from 1 (a wide
# character takes two). Colours are fg=/bg=/ul= #rrggbb, or idxN for the 16
# palette colours the terminal's theme decides; 16-255 become their xterm
# RGB. A blank cell keeps only what shows on it: its background, or all of
# its style under reverse, underline, strike or overline. An SGR code it does
# not know stays as sgrN until a reset, so a stray one shows up as a style
# change. Every other escape sequence (CSI, OSC, DCS/APC/PM/SOS strings,
# charset designations such as `\e(B`) and C0 control is dropped whole.
ANSI_PY='
import re, sys, unicodedata
data = open(sys.argv[1], "rb").read().decode("utf-8", "replace")
data = data.replace("\r\n", "\n").replace("\r", "")
def xterm(n):
    if n < 232:
        n -= 16
        level = lambda v: 0 if v == 0 else 55 + 40 * v
        return "#%02x%02x%02x" % (level(n // 36), level(n // 6 % 6), level(n % 6))
    g = 8 + 10 * (n - 232)
    return "#%02x%02x%02x" % (g, g, g)
def idx(n):
    return "idx%d" % n if n < 16 else xterm(n)
def num(s):
    return int(s) if s.isdigit() else 0
def color(kind, vals):
    # vals after 38/48/58 and the kind: [n] for 5; [r, g, b] for 2, or with
    # colons [colour space, r, g, b, ...] (ITU T.416) when there are more.
    if kind == "5" and vals:
        return idx(num(vals[0]) % 256)
    if kind == "2" and len(vals) >= 3:
        rgb = vals[:3] if len(vals) == 3 else vals[1:4]
        return "#%02x%02x%02x" % tuple(num(v) % 256 for v in rgb)
    return None
UNDERLINES = ("underline", "underline-double", "underline-curly", "underline-dotted", "underline-dashed")
ON = {1: "bold", 2: "dim", 3: "italic", 4: "underline", 5: "blink", 6: "blink",
      7: "reverse", 8: "hidden", 9: "strike", 21: "underline-double", 53: "overline"}
OFF = {22: ("bold", "dim"), 23: ("italic",), 24: UNDERLINES, 25: ("blink",),
       27: ("reverse",), 28: ("hidden",), 29: ("strike",), 55: ("overline",)}
SLOT = {38: "fg", 48: "bg", 58: "ul"}
def underline(st, name):
    st["a"].difference_update(UNDERLINES)
    if name:
        st["a"].add(name)
def sgr(params, st):
    ps = params.split(";") if params else ["0"]
    i = 0
    while i < len(ps):
        p = ps[i]
        i += 1
        if ":" in p:
            sub = p.split(":")
            head = num(sub[0])
            if head in SLOT:
                st["c"][SLOT[head]] = color(sub[1] if len(sub) > 1 else "", sub[2:])
            elif head == 4 and len(sub) == 2 and num(sub[1]) < len(UNDERLINES) + 1:
                underline(st, ([None] + list(UNDERLINES))[num(sub[1])])
            else:
                st["a"].add("sgr" + p)
            continue
        n = num(p)
        if n == 0:
            st["c"].clear()
            st["a"].clear()
        elif n in SLOT:
            kind = ps[i] if i < len(ps) else ""
            take = 2 if kind == "5" else 4 if kind == "2" else 1
            st["c"][SLOT[n]] = color(kind, ps[i + 1:i + take])
            i += take
        elif n in (4, 21):
            underline(st, ON[n])
        elif n in ON:
            st["a"].add(ON[n])
        elif n in OFF:
            st["a"].difference_update(OFF[n])
        elif n in (39, 49, 59):
            st["c"].pop({39: "fg", 49: "bg", 59: "ul"}[n], None)
        elif 30 <= n <= 37 or 90 <= n <= 97:
            st["c"]["fg"] = idx(n - 30 if n < 90 else n - 82)
        elif 40 <= n <= 47 or 100 <= n <= 107:
            st["c"]["bg"] = idx(n - 40 if n < 100 else n - 92)
        else:
            st["a"].add("sgr%d" % n)
SHOWS_ON_BLANK = {"reverse", "strike", "overline"}.union(UNDERLINES)
def key(st, ch):
    c, a = dict(st["c"]), set(st["a"])
    if ch == " " and not a & SHOWS_ON_BLANK:
        c, a = {"bg": c.get("bg")}, set()
    cs = ["%s=%s" % (k, v) for k, v in sorted(c.items()) if v]
    return " ".join(cs + sorted(a))
def width(ch):
    if unicodedata.combining(ch):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
token = re.compile(
    r"\x1b\[([0-?]*)([ -/]*)([@-~])"            # CSI
    r"|\x1b\][^\x07\x1b\n]*(?:\x07|\x1b\\)?"    # OSC, terminated or not
    r"|\x1b[PX^_][^\x1b]*(?:\x1b\\)?"           # DCS, SOS, PM, APC strings
    r"|\x1b[ -/]*[0-~]?"                        # other escapes: \e(B, \e7, \e=
    r"|\n|[\x00-\x09\x0b-\x1a\x1c-\x1f\x7f]|[^\x1b\n]")
st = {"c": {}, "a": set()}
rows, row = [], []
for m in token.finditer(data):
    t = m.group(0)
    if t == "\n":
        rows.append(row)
        row = []
    elif m.group(3) == "m" and not m.group(2) and not m.group(1).startswith(("<", "=", ">", "?")):
        sgr(m.group(1), st)
    elif not t.startswith("\x1b") and (t >= " " and t != "\x7f"):
        row.append((t, key(st, t), width(t)))
rows.append(row)
text = ["".join(c for c, _, _ in r).rstrip() for r in rows]
while text and not text[-1]:
    text.pop()
for n, t in enumerate(text, 1):
    print("T %d|%s" % (n, t))
for n, r in enumerate(rows, 1):
    runs, col = [], 1
    for _, k, w in r:
        if w == 0:
            continue
        if runs and runs[-1][2] == k:
            runs[-1][1] = col + w - 1
        else:
            runs.append([col, col + w - 1, k])
        col += w
    for first, last, k in runs:
        if k:
            print("S %d:%d-%d %s" % (n, first, last, k))
'

[ -z "$NORMALIZE" ] || { python3 -c "$ANSI_PY" "$NORMALIZE"; exit 0; }

# ----------------------------------------------------- Herdr is reachable ----

if [ -z "$FAKE" ]; then
    type -P herdr >/dev/null 2>&1 ||
        die "herdr is not installed (not on PATH). Install Herdr and run this script from one of its panes."
fi
if ! herdr pane list >/dev/null 2>&1; then
    die "Herdr is not running, or this shell cannot reach it ('herdr pane list' failed). Run this script from a Herdr pane."
fi

if [ -z "$TIZA_BIN" ]; then
    (cd "$ROOT" && cargo build -p tiza --quiet) || die "building tiza failed"
    TIZA_BIN="$ROOT/target/debug/tiza"
fi
[ -x "$TIZA_BIN" ] || die "tiza binary not found: $TIZA_BIN"

# --------------------------------------------------------------- cleanup ----

WORK="$(mktemp -d /tmp/tiza-herdr.XXXXXX)"
OPENED="$WORK/opened"   # panes this script opened and has not closed, one per line
: >"$OPENED"
TAB=""                  # the tab this script opened, closed last

close_pane() {
    herdr pane close "$1" >/dev/null 2>&1 || true
    grep -vx "$1" "$OPENED" >"$OPENED.new" || true
    mv "$OPENED.new" "$OPENED"
}

cleanup() {
    local p
    # Newest first: the pane split off a pane goes before it.
    sed '1!G;h;$!d' "$OPENED" 2>/dev/null | while read -r p; do
        herdr pane close "$p" >/dev/null 2>&1 || true
    done
    if [ -n "$TAB" ]; then
        herdr tab close "$TAB" >/dev/null 2>&1 ||
            echo "herdr-matrix: could not close tab $TAB (label $LABEL); close it by hand" >&2
    fi
    rm -rf "$WORK"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# ------------------------------------------------------------ measuring ----

# Herdr's JSON → "focused|workspace|tab <id>" and "pane <id> <width> <height>"
# lines: every object with a pane_id (or a nested pane.pane_id) and a rect
# with width/height, wherever it sits in the snapshot, and the first
# focused_pane_id / workspace_id / tab_id found.
LAYOUT_PY='
import json, sys
try:
    v = json.load(sys.stdin)
except ValueError:
    sys.exit(1)
panes, first = {}, {}
def walk(x):
    if isinstance(x, dict):
        for k, name in (("focused_pane_id", "focused"), ("workspace_id", "workspace"), ("tab_id", "tab")):
            if isinstance(x.get(k), str):
                first.setdefault(name, x[k])
        pid, r = x.get("pane_id"), x.get("rect")
        if pid is None and isinstance(x.get("pane"), dict):
            pid = x["pane"].get("pane_id")
        if isinstance(pid, str) and isinstance(r, dict) and "width" in r and "height" in r:
            panes[pid] = (int(r["width"]), int(r["height"]))
        for y in x.values():
            walk(y)
    elif isinstance(x, list):
        for y in x:
            walk(y)
walk(v)
for name, value in first.items():
    print(name, value)
for p, (w, h) in panes.items():
    print("pane", p, w, h)
'

# The answer of `tab create` / `pane split` → "tab <id>" and "pane <id>": the
# documented result.tab and result.root_pane (or result.pane) first, then the
# first tab_id / pane_id anywhere. Nothing printed when it is not JSON.
IDS_PY='
import json, sys
try:
    v = json.load(sys.stdin)
except ValueError:
    sys.exit(0)
def get(x, *path):
    for k in path:
        if not isinstance(x, dict):
            return None
        x = x.get(k)
    return x if isinstance(x, str) else None
def find(x, key):
    if isinstance(x, dict):
        if isinstance(x.get(key), str):
            return x[key]
        x = list(x.values())
    if isinstance(x, list):
        for y in x:
            r = find(y, key)
            if r:
                return r
    return None
r = v.get("result", v) if isinstance(v, dict) else v
tab = get(r, "tab", "tab_id") or get(r, "tab", "id") or find(v, "tab_id")
pane = (get(r, "root_pane", "pane_id") or get(r, "root_pane", "id") or get(r, "pane", "pane_id")
        or get(r, "pane", "id") or find(v, "pane_id"))
if tab:
    print("tab", tab)
if pane:
    print("pane", pane)
'

# layout → refreshes $WORK/layout.txt from the base pane's tab (the current
# pane's before the base is known).
layout() {
    if [ -n "$BASE" ]; then
        herdr pane layout --pane "$BASE" >"$WORK/layout.json" || die "herdr pane layout --pane $BASE failed"
    elif ! herdr pane layout --current >"$WORK/layout.json"; then
        # Herdr also names the calling pane in the environment of its panes.
        [ -n "${HERDR_PANE_ID:-}" ] || die "herdr pane layout --current failed; pass --base PANE_ID"
        herdr pane layout --pane "$HERDR_PANE_ID" >"$WORK/layout.json" ||
            die "herdr pane layout --current and --pane $HERDR_PANE_ID failed; pass --base PANE_ID"
    fi
    python3 -c "$LAYOUT_PY" <"$WORK/layout.json" >"$WORK/layout.txt" ||
        die "herdr pane layout did not print JSON: $(head -c 300 "$WORK/layout.json")"
}

# field NAME FILE → the id after NAME in a LAYOUT_PY / IDS_PY output.
field() {
    awk -v n="$1" '$1 == n { print $2; exit }' "$2"
}

# rect PANE → "cols rows" of PANE from a fresh layout.
rect() {
    local r
    layout
    r="$(awk -v p="$1" '$1 == "pane" && $2 == p { print $3, $4 }' "$WORK/layout.txt")"
    [ -n "$r" ] || die "herdr pane layout gave no rect.width/height for pane $1: $(head -c 300 "$WORK/layout.json")"
    printf '%s\n' "$r"
}

# wait_rect PANE "cols rows" → PANE's size once it differs (or the same after
# 1 s: the resize did nothing).
wait_rect() {
    local end=$((SECONDS + 1)) s
    while :; do
        s="$(rect "$1")"
        if [ "$s" != "$2" ] || [ "$SECONDS" -ge "$end" ]; then
            printf '%s\n' "$s"
            return 0
        fi
        sleep 0.05
    done
}

axis_of() {
    if [ "$1" = cols ]; then echo "${2% *}"; else echo "${2#* }"; fi
}

pane_ids() {
    herdr pane list | python3 -c '
import json, sys
def walk(v):
    if isinstance(v, dict):
        if "pane_id" in v:
            print(v["pane_id"])
        for x in v.values():
            walk(x)
    elif isinstance(v, list):
        for x in v:
            walk(x)
walk(json.load(sys.stdin))' | sort -u
}

# new_pane_since BEFORE → the first pane id in `pane list` not in BEFORE.
new_pane_since() {
    comm -13 <(printf '%s\n' "$1") <(pane_ids) | head -n 1
}

# split PARENT DIRECTION RATIO → sets NEW_PANE to the new pane's id (from
# split's result.pane, or by comparing `pane list` before and after).
# RATIO is the share PARENT keeps; the new pane goes right of (below) it.
split() {
    local parent="$1" dir="$2" ratio="$3" before new
    before="$(pane_ids)"
    herdr pane split "$parent" --direction "$dir" --ratio "$ratio" --cwd "$ROOT" >"$WORK/split.json"
    new="$(python3 -c "$IDS_PY" <"$WORK/split.json" | awk '$1 == "pane" { print $2 }')"
    if [ -z "$new" ] || ! printf '%s\n' "$(pane_ids)" | grep -qxF "$new"; then
        new="$(new_pane_since "$before")"
    fi
    [ -n "$new" ] || die "herdr pane split did not create a pane: $(head -c 300 "$WORK/split.json")"
    printf '%s\n' "$new" >>"$OPENED"
    NEW_PANE="$new"
}

# keep_ratio TARGET TOTAL → the ratio the parent keeps so the new pane gets
# TARGET of TOTAL cells.
keep_ratio() {
    awk -v t="$1" -v n="$2" 'BEGIN { printf "%.4f", 1 - t / n }'
}

# fit PANE NEIGHBOUR AXIS TARGET TOTAL → PANE's rect after fixing the last
# cells of AXIS (cols|rows). PANE is right of (below) NEIGHBOUR and both
# share TOTAL cells. Every resize grows the pane it names: PANE towards
# NEIGHBOUR (left|up) to grow, NEIGHBOUR towards PANE (right|down) to shrink
# PANE. --amount is a share of the split, as in Herdr (0.1 of 202 = 20
# cells); one that moves nothing is doubled.
fit() {
    local pane="$1" nb="$2" axis="$3" target="$4" total="$5"
    local grow shrink size cur next diff amount scale=1 i
    if [ "$axis" = cols ]; then grow=left; shrink=right; else grow=up; shrink=down; fi
    size="$(rect "$pane")"
    cur=$(axis_of "$axis" "$size")
    for i in 1 2 3 4 5 6; do
        diff=$((target - cur))
        [ "$diff" -eq 0 ] && break
        amount=$(awk -v d="${diff#-}" -v s="$scale" -v n="$total" 'BEGIN { printf "%.4f", d * s / n }')
        if [ "$diff" -gt 0 ]; then
            herdr pane resize --pane "$pane" --direction "$grow" --amount "$amount" >/dev/null
        else
            herdr pane resize --pane "$nb" --direction "$shrink" --amount "$amount" >/dev/null
        fi
        size="$(wait_rect "$pane" "$size")"
        next=$(axis_of "$axis" "$size")
        if [ "$next" -eq "$cur" ]; then scale=$((scale * 2)); else scale=1; fi
        cur=$next
    done
    diff=$((target - cur))
    [ "${diff#-}" -le "$TOL" ] ||
        echo "herdr-matrix: pane $pane has $cur $axis, target $target (Herdr would not get closer)" >&2
    printf '%s\n' "$size"
}

# status SOCKET → "cols rows" of the live instance, or nothing.
status() {
    "$TIZA_BIN" api status --socket "$1" 2>/dev/null | python3 -c '
import json, sys
try:
    v = json.load(sys.stdin)
except ValueError:
    sys.exit(1)
v = v.get("result", v)
print(v["cols"], v["rows"])' 2>/dev/null || true
}

# tiza_size SOCKET "cols rows" → the size the instance reports once it
# answers and shows the expected size (or what it reports after 3 s more);
# with an empty expected size, once two reports 0.2 s apart agree.
tiza_size() {
    local end=$((SECONDS + TIMEOUT)) s prev=""
    while :; do
        s="$(status "$1")"
        [ -n "$s" ] && break
        [ "$SECONDS" -le "$end" ] || die "Tiza did not answer on $1 within ${TIMEOUT}s"
        sleep 0.1
    done
    end=$((SECONDS + 3))
    while [ "$SECONDS" -le "$end" ]; do
        if [ -n "$2" ]; then
            [ "$s" = "$2" ] && break
        else
            [ "$s" = "$prev" ] && break
            prev="$s"
        fi
        sleep 0.2
        s="$(status "$1")"
    done
    printf '%s\n' "$s"
}

# wait_frame PANE → the visible text once two reads in a row agree (and are
# not blank), with trailing blanks trimmed; the ansi read taken with each
# text read must agree too, and the last one is left in $WORK/frame.ansi.
wait_frame() {
    local end=$((SECONDS + TIMEOUT)) prev="" cur
    rm -f "$WORK/frame.ansi"
    while [ "$SECONDS" -le "$end" ]; do
        cur="$(herdr pane read "$1" --source visible --format text | sed 's/[[:space:]]*$//')" ||
            die "herdr pane read $1 failed"
        herdr pane read "$1" --source visible --format ansi >"$WORK/next.ansi" ||
            die "herdr pane read $1 --format ansi failed"
        if [ -n "$(printf '%s' "$cur" | tr -d '[:space:]')" ] && [ "$cur" = "$prev" ] &&
            cmp -s "$WORK/next.ansi" "$WORK/frame.ansi"; then
            printf '%s\n' "$cur"
            return 0
        fi
        prev="$cur"
        mv "$WORK/next.ansi" "$WORK/frame.ansi"
        sleep 0.2
    done
    die "no stable frame in pane $1 within ${TIMEOUT}s"
}

# run_tiza PANE SOCKET FLAGS → starts Tiza on the document in PANE.
run_tiza() {
    herdr pane run "$1" "$(printf '%q' "$TIZA_BIN") $(printf '%q' "$DOC") ${3:+$3 }--no-save --socket $(printf '%q' "$2")" >/dev/null
}

# ------------------------------------------------------------- the base ----

if [ -z "$BASE" ]; then
    # A tab of our own in the current pane's workspace.
    layout
    ws="$(field workspace "$WORK/layout.txt")"
    [ -n "$ws" ] || ws="${HERDR_WORKSPACE_ID:-}"
    [ -n "$ws" ] ||
        die "cannot tell the current pane's workspace (no workspace_id in herdr pane layout --current, no HERDR_WORKSPACE_ID); pass --base PANE_ID: $(head -c 300 "$WORK/layout.json")"
    before="$(pane_ids)"
    herdr tab create --workspace "$ws" --cwd "$ROOT" --label "$LABEL" >"$WORK/tab.json" ||
        die "herdr tab create --workspace $ws failed"
    python3 -c "$IDS_PY" <"$WORK/tab.json" >"$WORK/tab.txt"
    TAB="$(field tab "$WORK/tab.txt")"
    BASE="$(field pane "$WORK/tab.txt")"
    if [ -z "$BASE" ] || ! printf '%s\n' "$(pane_ids)" | grep -qxF "$BASE"; then
        BASE="$(new_pane_since "$before")"
    fi
    if [ -z "$BASE" ]; then
        [ -n "$TAB" ] || echo "herdr-matrix: close the tab labelled $LABEL by hand" >&2
        die "herdr tab create: no root pane in its answer or in herdr pane list: $(head -c 300 "$WORK/tab.json")"
    fi
    if [ -z "$TAB" ]; then
        layout
        TAB="$(field tab "$WORK/layout.txt")"
    fi
    if [ -z "$TAB" ]; then
        # Closing its root pane closes the tab, as the tab id is unknown.
        printf '%s\n' "$BASE" >>"$OPENED"
        die "cannot tell the id of the new tab (not in herdr tab create, not in herdr pane layout --pane $BASE): $(head -c 300 "$WORK/tab.json")"
    fi
    echo "working in a new tab $TAB (root pane $BASE), closed at the end"
fi
base="$(rect "$BASE")"

# Herdr's frame: the probe pane's rect minus the terminal Tiza sees in it.
split "$BASE" right 0.5
probe="$NEW_PANE"
prect="$(rect "$probe")"
run_tiza "$probe" "$WORK/frame.sock" ""
pseen="$(tiza_size "$WORK/frame.sock" "")"
FC=$((${prect% *} - ${pseen% *}))
FR=$((${prect#* } - ${pseen#* }))
close_pane "$probe"
if [ "$FC" -lt 0 ] || [ "$FR" -lt 0 ] || [ "$FC" -gt 20 ] || [ "$FR" -gt 20 ]; then
    die "Herdr's frame does not make sense: a ${prect% *}x${prect#* } pane, Tiza sees ${pseen% *}x${pseen#* }"
fi
echo "herdr frame: ${FC}x${FR} cells (a ${prect% *}x${prect#* } pane, Tiza sees ${pseen% *}x${pseen#* })"

# ------------------------------------------------------------------ run ----

mkdir -p "$OUT"
rm -f "$OUT"/*.txt "$OUT"/*.ansi
printf '%-13s %-8s %-8s %-8s %s\n' case target tiza herdr file
CAPTURED=""
while read -r name tc tr mode; do
    sock="$WORK/$name.sock"
    # The base pane has its whole size back: the last case's panes are closed.
    base="$(rect "$BASE")"
    bw=${base% *}
    if [ "$tc" = max ]; then
        wc=$((bw - 12))   # leave the base pane about 12 columns
    else
        wc=$((tc + FC))
    fi
    if [ "$wc" -gt $((bw - 2)) ]; then
        echo "herdr-matrix: $name: the base pane has $bw columns, a pane of $wc does not fit" >&2
        wc=$((bw - 2))
    fi
    split "$BASE" right "$(keep_ratio "$wc" "$bw")"
    col="$NEW_PANE"
    pane="$col"
    size="$(fit "$col" "$BASE" cols "$wc" "$bw")"
    ch=${size#* }
    if [ "$tr" != max ]; then
        wr=$((tr + FR))
        if [ "$wr" -gt $((ch - 2)) ]; then
            echo "herdr-matrix: $name: the base pane has $ch rows, a pane of $wr does not fit" >&2
            wr=$((ch - 2))
        fi
        split "$col" down "$(keep_ratio "$wr" "$ch")"
        pane="$NEW_PANE"
        size="$(fit "$pane" "$col" rows "$wr" "$ch")"
    fi
    # What Tiza should see: the target, or the pane less the frame on a "max" axis.
    want_c=$tc want_r=$tr
    [ "$tc" = max ] && want_c=$((${size% *} - FC))
    [ "$tr" = max ] && want_r=$((${size#* } - FR))
    flags="--present --step $LAST"
    [ "$mode" = normal ] && flags="--step $LAST"
    run_tiza "$pane" "$sock" "$flags"
    seen="$(tiza_size "$sock" "$want_c $want_r")"
    if [ "$seen" != "$want_c $want_r" ]; then
        # This pane's frame is not the measured one: resize by the difference.
        echo "herdr-matrix: $name: Tiza sees ${seen% *}x${seen#* } in a ${size% *}x${size#* } pane, not ${want_c}x${want_r}; resizing the pane by the difference" >&2
        dc=$((want_c - ${seen% *}))
        dr=$((want_r - ${seen#* }))
        if [ "$dc" -ne 0 ] && [ "$tc" != max ]; then
            size="$(fit "$col" "$BASE" cols $((${size% *} + dc)) "$bw")"
        fi
        if [ "$dr" -ne 0 ] && [ "$tr" != max ]; then
            size="$(fit "$pane" "$col" rows $((${size#* } + dr)) "$ch")"
        fi
        seen="$(tiza_size "$sock" "$want_c $want_r")"
        [ "$seen" = "$want_c $want_r" ] ||
            echo "herdr-matrix: $name: Tiza sees ${seen% *}x${seen#* }, target ${want_c}x${want_r}" >&2
    fi
    frame="$(wait_frame "$pane")"
    tiza="${seen% *}x${seen#* }"
    real="${size% *}x${size#* }"
    file="$name-$tiza.txt"
    printf '%s\n' "$frame" >"$OUT/$file"
    cp "$WORK/frame.ansi" "$OUT/${file%.txt}.ansi"
    target="${tc}x${tr}"
    [ "$tr" = max ] && target=max
    printf '%-13s %-8s %-8s %-8s %s\n' "$name" "$target" "$tiza" "$real" "$OUT/$file"
    CAPTURED="$CAPTURED $file"
    # Close this case's panes before the next one, so each split starts from
    # the base pane at its own size.
    [ "$pane" = "$col" ] || close_pane "$pane"
    close_pane "$col"
done <<<"$MATRIX"

case "$MODE" in
capture)
    echo "captured into $OUT; --check compares with $SNAPS, --update accepts them"
    ;;
update)
    mkdir -p "$SNAPS"
    for file in $CAPTURED; do
        rm -f "$SNAPS/${file%-*}"-[0-9]*x[0-9]*.txt "$SNAPS/${file%-*}"-[0-9]*x[0-9]*.ansi
        cp "$OUT/$file" "$SNAPS/$file"
        cp "$OUT/${file%.txt}.ansi" "$SNAPS/${file%.txt}.ansi"
    done
    echo "updated $SNAPS"
    ;;
check)
    changed=""
    for file in $CAPTURED; do
        name="${file%-*}"
        old=""
        for f in "$SNAPS/$name"-[0-9]*x[0-9]*.txt; do
            [ -f "$f" ] && old="$(basename "$f")"
        done
        if [ -z "$old" ]; then
            echo "CHANGED $name: no snapshot in $SNAPS (run --update)"
            changed="$changed $name"
        elif [ "$old" != "$file" ]; then
            echo "CHANGED $name: size ${old#"$name"-} → ${file#"$name"-}"
            changed="$changed $name"
        elif ! diff -u "$SNAPS/$old" "$OUT/$file" >"$WORK/diff"; then
            echo "CHANGED $name: text differs ($(grep -c '^[-+][^-+]' "$WORK/diff" || true) lines)"
            sed -n '1,40p' "$WORK/diff"
            changed="$changed $name"
        elif [ ! -f "$SNAPS/${old%.txt}.ansi" ]; then
            echo "CHANGED $name: no ${old%.txt}.ansi in $SNAPS (run --update)"
            changed="$changed $name"
        else
            # Same text: the ansi captures by meaning, not by bytes.
            python3 -c "$ANSI_PY" "$SNAPS/${old%.txt}.ansi" >"$WORK/old.norm"
            python3 -c "$ANSI_PY" "$OUT/${file%.txt}.ansi" >"$WORK/new.norm"
            if ! diff -u "$WORK/old.norm" "$WORK/new.norm" >"$WORK/diff"; then
                if grep -q '^[-+]T ' "$WORK/diff"; then
                    rows=$(grep '^[-+]T ' "$WORK/diff" | sed 's/^.T \([0-9]*\).*/\1/' | sort -u | wc -l | tr -d ' ')
                    echo "CHANGED $name: text differs in the ansi capture ($rows rows)"
                else
                    rows=$(grep '^[-+]S ' "$WORK/diff" | sed 's/^.S \([0-9]*\):.*/\1/' | sort -u | wc -l | tr -d ' ')
                    echo "CHANGED $name: style only ($rows rows; lines are S row:first-last style)"
                fi
                grep '^[-+][TS] ' "$WORK/diff" | sed -n '1,40p'
                changed="$changed $name"
            fi
        fi
    done
    if [ -n "$changed" ]; then
        echo "panes that changed:$changed"
        exit 1
    fi
    echo "no differences"
    ;;
esac
