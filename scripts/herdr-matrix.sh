#!/usr/bin/env bash
# Terminal size matrix with Herdr panes (t-071): the same frame at several
# real terminal sizes, captured from a real terminal emulator (Herdr renders
# through libghostty-vt) and compared with committed snapshots. A QA-round
# step on a machine with Herdr (Luis's Mac), never part of `cargo test`.
#
#   scripts/herdr-matrix.sh              # capture into qa/herdr/latest/, print sizes
#   scripts/herdr-matrix.sh --check      # capture and diff with qa/herdr/*.txt (exit 1 on changes)
#   scripts/herdr-matrix.sh --update     # capture and rewrite qa/herdr/*.txt on purpose
#
# Options:
#   --file DOC        document to open (default: examples/ownership.rs.tiza, only read)
#   --tiza BIN        tiza binary (default: builds and uses target/debug/tiza)
#   --base PANE_ID    pane to split from (default: Herdr's focused pane)
#   --snapshots DIR   committed snapshots (default: qa/herdr)
#   --out DIR         where captures land (default: qa/herdr/latest, git-ignored)
#
# Run it from a Herdr pane. For each case of the matrix it splits one new
# pane off the base pane (right, then down for a row target), launches Tiza
# there with a socket of its own, reads the pane's real size from
# `tiza api status` (Herdr does not report columns), nudges it towards the
# target with `herdr pane resize`, waits for a stable first frame and saves
# `herdr pane read --source visible --format text` as
# <snapshots>/<case>-<cols>x<rows>.txt (the real size, which may miss the
# target by a cell or two). Every pane it opens is closed again, also on
# errors or Ctrl-C; the user's panes are only split and resized back by
# Herdr when ours close.
#
# Only the herdr 0.9.5 CLI is used: pane split/resize/run/send-keys/read/
# close/list. Everything that talks to Herdr goes through `herdr()` below;
# with TIZA_HERDR_FAKE=<state dir> it is simulated (panes are Python ptys,
# `pane read` is the live instance's `tiza render`), so qa/tests/t-071.rs can
# drive the rest of this script without Herdr.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE=capture
DOC="$ROOT/examples/ownership.rs.tiza"
TIZA_BIN="${TIZA:-}"
BASE=""
SNAPS="$ROOT/qa/herdr"
OUT=""
TIMEOUT="${TIZA_HERDR_TIMEOUT:-15}"   # seconds per wait (socket up, first frame)
TOL=1                                 # cells a pane may miss its target by

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
    -h | --help)
        sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'
        exit 0
        ;;
    *) die "unknown argument: $1 (see --help)" ;;
    esac
    shift
done
[ -n "$OUT" ] || OUT="$SNAPS/latest"
[ -f "$DOC" ] || die "document not found: $DOC"
DOC="$(cd "$(dirname "$DOC")" && pwd)/$(basename "$DOC")"
command -v python3 >/dev/null || die "python3 is needed to read Herdr's and Tiza's JSON"

# The matrix: case, target cols, target rows ("max" = as large as the base
# pane allows), mode. 75x50 is Zed split in half; the two normal-mode cases
# show the status bar, and the narrow one its small-terminal badge (t-064).
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

# A tiny model of Herdr for the tests: the base pane p0 has the window size
# in $FAKE/window ("cols rows"); `split` gives the new pane `ratio` of the
# parent and `close` gives it back; `resize --direction left|up` grows a pane
# by `amount` × the window, taking it from the pane beside it (right|down
# shrinks it; TIZA_HERDR_FAKE_FLIP=1 swaps them, to check the
# script finds the direction by itself); `run` starts the command in a Python
# pty of the pane's size that follows later resizes; `read` is the live
# instance's text render. TIZA_HERDR_FAKE_FAIL_READ=1 makes `read` fail.
fake_herdr() {
    [ -f "$FAKE/window" ] || { echo "error: cannot connect to the herdr server" >&2; return 1; }
    local sub="$1 $2"
    shift 2
    case "$sub" in
    "pane list")
        python3 - "$FAKE" <<'PY'
import json, os, sys
d = sys.argv[1]
panes = []
for p in sorted(os.listdir(os.path.join(d, "panes"))):
    c, r = open(os.path.join(d, "panes", p, "size")).read().split()
    panes.append({"pane_id": p, "cwd": "/", "scroll": {"viewport_rows": int(r)}})
print(json.dumps({"panes": panes}))
PY
        ;;
    "pane split")
        local parent=p0 dir="" ratio=0.5
        while [ $# -gt 0 ]; do
            case "$1" in
            --direction) dir="$2"; shift ;;
            --ratio) ratio="$2"; shift ;;
            --cwd) shift ;;
            *) parent="$1" ;;
            esac
            shift
        done
        [ -d "$FAKE/panes/$parent" ] || { echo "error: pane not found: $parent" >&2; return 1; }
        local n id
        n=$(cat "$FAKE/next" 2>/dev/null || echo 1)
        echo $((n + 1)) >"$FAKE/next"
        id="p$n"
        mkdir -p "$FAKE/panes/$id"
        read -r pc pr <"$FAKE/panes/$parent/size"
        awk -v c="$pc" -v r="$pr" -v k="$ratio" -v d="$dir" -v f="$FAKE/panes/$parent/size" 'BEGIN {
            if (d == "right") { n = int(c * k + 0.5); printf "%d %d\n", n, r; printf "%d %d\n", c - n, r > f }
            else { n = int(r * k + 0.5); printf "%d %d\n", c, n; printf "%d %d\n", c, r - n > f } }' \
            >"$FAKE/panes/$id/size"
        echo "$parent $dir" >"$FAKE/panes/$id/parent"
        echo "{\"pane_id\":\"$id\"}"
        ;;
    "pane resize")
        local pane="" dir="" amount=0
        while [ $# -gt 0 ]; do
            case "$1" in
            --pane) pane="$2"; shift ;;
            --direction) dir="$2"; shift ;;
            --amount) amount="$2"; shift ;;
            esac
            shift
        done
        [ -d "$FAKE/panes/$pane" ] || { echo "error: pane not found: $pane" >&2; return 1; }
        local wc wr split_dir field delta x chain="" anc="" pdir
        read -r wc wr <"$FAKE/window"
        if [ "$dir" = left ] || [ "$dir" = right ]; then split_dir=right; field=1; else split_dir=down; field=2; fi
        delta=$(awk -v a="$amount" -v d="$dir" -v w="$wc" -v h="$wr" -v flip="${TIZA_HERDR_FAKE_FLIP:-0}" 'BEGIN {
            s = (d == "left" || d == "up") ? 1 : -1; if (flip == "1") s = -s
            n = (d == "left" || d == "right") ? w : h; printf "%d", s * int(a * n + 0.5) }')
        # The space comes from the pane beside the split along that axis: walk
        # up to it; every pane on the way changes with this one.
        x="$pane"
        while [ -f "$FAKE/panes/$x/parent" ]; do
            chain="$chain $x"
            read -r anc pdir <"$FAKE/panes/$x/parent"
            [ "$pdir" = "$split_dir" ] && break
            x="$anc"
            anc=""
        done
        [ -n "$anc" ] || return 0
        # Both sides keep at least 2 cells.
        local mine theirs
        mine=$(cut -d' ' -f"$field" "$FAKE/panes/$pane/size")
        theirs=$(cut -d' ' -f"$field" "$FAKE/panes/$anc/size")
        [ $((mine + delta)) -ge 2 ] || delta=$((2 - mine))
        [ $((theirs - delta)) -ge 2 ] || delta=$((theirs - 2))
        for x in $chain; do
            awk -v f="$field" -v d="$delta" '{ $f += d; print }' "$FAKE/panes/$x/size" >"$FAKE/panes/$x/size.new"
            mv "$FAKE/panes/$x/size.new" "$FAKE/panes/$x/size"
        done
        awk -v f="$field" -v d="$delta" '{ $f -= d; print }' "$FAKE/panes/$anc/size" >"$FAKE/panes/$anc/size.new"
        mv "$FAKE/panes/$anc/size.new" "$FAKE/panes/$anc/size"
        ;;
    "pane run")
        local pane="$1" cmd="$2"
        [ -d "$FAKE/panes/$pane" ] || { echo "error: pane not found: $pane" >&2; return 1; }
        printf '%s\n' "$cmd" >"$FAKE/panes/$pane/cmd"
        printf '%s\n' "$cmd" >>"$FAKE/log"
        python3 -c "$FAKE_PTY_HOST" "$FAKE/panes/$pane" "$cmd" </dev/null >/dev/null 2>&1 &
        echo $! >"$FAKE/panes/$pane/pid"
        ;;
    "pane send-keys" | "pane send-text")
        local pane="$1"
        shift
        [ -d "$FAKE/panes/$pane" ] || { echo "error: pane not found: $pane" >&2; return 1; }
        printf '%s\n' "$*" >>"$FAKE/panes/$pane/input"
        ;;
    "pane read")
        local pane="$1"
        [ -d "$FAKE/panes/$pane" ] || { echo "error: pane not found: $pane" >&2; return 1; }
        [ "${TIZA_HERDR_FAKE_FAIL_READ:-0}" != 1 ] || { echo "error: read failed (fake)" >&2; return 1; }
        local sock
        sock=$(sed -n 's/.*--socket \([^ ]*\).*/\1/p' "$FAKE/panes/$pane/cmd" 2>/dev/null || true)
        [ -n "$sock" ] || return 0
        "$TIZA_BIN" render --socket "$sock" --format txt --out - 2>/dev/null || true
        ;;
    "pane close")
        local pane="$1"
        [ -d "$FAKE/panes/$pane" ] || { echo "error: pane not found: $pane" >&2; return 1; }
        if [ -f "$FAKE/panes/$pane/pid" ]; then
            touch "$FAKE/panes/$pane/stop"
            local pid
            pid=$(cat "$FAKE/panes/$pane/pid")
            # The host kills its child and exits on `stop`; wait for it.
            local i=0
            while kill -0 "$pid" 2>/dev/null && [ $i -lt 100 ]; do
                sleep 0.05
                i=$((i + 1))
            done
            kill -9 "$pid" 2>/dev/null || true
        fi
        # Give the space back to the parent, as Herdr does.
        local parent dir pc pr c r
        read -r parent dir <"$FAKE/panes/$pane/parent"
        read -r c r <"$FAKE/panes/$pane/size"
        if [ -f "$FAKE/panes/$parent/size" ]; then
            read -r pc pr <"$FAKE/panes/$parent/size"
            if [ "$dir" = right ]; then pc=$((pc + c)); else pr=$((pr + r)); fi
            echo "$pc $pr" >"$FAKE/panes/$parent/size"
        fi
        rm -rf "$FAKE/panes/$pane"
        ;;
    *)
        echo "error: unknown command: $sub" >&2
        return 1
        ;;
    esac
}

# The fake pane: runs `bash -c CMD` on a pty of the size in DIR/size, applies
# every later change of that file (the kernel sends SIGWINCH), drains the
# output and ends the child when DIR/stop appears.
FAKE_PTY_HOST='
import fcntl, os, pty, select, signal, struct, sys, termios, time
d, cmd = sys.argv[1], sys.argv[2]
def size():
    c, r = open(os.path.join(d, "size")).read().split()
    return int(c), int(r)
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
    rm -rf "$WORK"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

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
walk(json.load(sys.stdin))' | sort
}

# split PARENT|"" DIRECTION → sets NEW_PANE to the new pane's id (found by
# comparing `pane list` before and after, whatever `split` itself prints).
split() {
    local parent="$1" dir="$2" before after new
    before="$(pane_ids)"
    if [ -n "$parent" ]; then
        herdr pane split "$parent" --direction "$dir" --ratio 0.5 --cwd "$ROOT" >/dev/null
    else
        herdr pane split --direction "$dir" --ratio 0.5 --cwd "$ROOT" >/dev/null
    fi
    after="$(pane_ids)"
    new="$(comm -13 <(printf '%s\n' "$before") <(printf '%s\n' "$after") | head -n 1)"
    [ -n "$new" ] || die "herdr pane split did not create a pane"
    printf '%s\n' "$new" >>"$OPENED"
    NEW_PANE="$new"
}

# ------------------------------------------------------------ measuring ----

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


# wait_status SOCKET → "cols rows" once the instance answers (or dies).
wait_status() {
    local end=$((SECONDS + TIMEOUT)) s
    while [ "$SECONDS" -le "$end" ]; do
        s="$(status "$1")"
        [ -n "$s" ] && { printf '%s\n' "$s"; return 0; }
        sleep 0.1
    done
    die "Tiza did not answer on $1 within ${TIMEOUT}s"
}

# wait_change SOCKET "cols rows" → the size once it differs (or the same
# after 3 s: the resize did nothing).
wait_change() {
    local end=$((SECONDS + 3)) s
    while [ "$SECONDS" -le "$end" ]; do
        s="$(status "$1")"
        if [ -n "$s" ] && [ "$s" != "$2" ]; then
            printf '%s\n' "$s"
            return 0
        fi
        sleep 0.05
    done
    printf '%s\n' "$2"
}

# fit PANE SOCKET AXIS TARGET "cols rows" → the size after nudging AXIS
# (cols|rows) towards TARGET. Herdr's units for --amount and which direction
# grows a pane are learnt from what each resize actually did.
fit() {
    local pane="$1" sock="$2" axis="$3" target="$4" size="$5"
    local grow shrink cur i diff dir amount next moved per_cell
    if [ "$axis" = cols ]; then grow=left; shrink=right; else grow=up; shrink=down; fi
    cur=$(axis_of "$axis" "$size")
    per_cell=$(awk -v c="$cur" 'BEGIN { printf "%.6f", 0.5 / (c > 0 ? c : 1) }')
    for i in 1 2 3 4 5 6 7 8; do
        diff=$((target - cur))
        [ "${diff#-}" -le "$TOL" ] && break
        if [ "$diff" -gt 0 ]; then dir=$grow; else dir=$shrink; fi
        amount=$(awk -v d="${diff#-}" -v k="$per_cell" 'BEGIN { a = d * k; if (a < 0.0001) a = 0.0001; if (a > 200) a = 200; printf "%.4f", a }')
        herdr pane resize --pane "$pane" --direction "$dir" --amount "$amount" >/dev/null
        size="$(wait_change "$sock" "$size")"
        next=$(axis_of "$axis" "$size")
        moved=$((next - cur))
        if [ "$moved" -eq 0 ]; then
            # Too small for Herdr to move a cell (or it was at a limit): go bigger.
            per_cell=$(awk -v k="$per_cell" 'BEGIN { printf "%.6f", k * 4 }')
            continue
        fi
        if { [ "$diff" -gt 0 ] && [ "$moved" -lt 0 ]; } || { [ "$diff" -lt 0 ] && [ "$moved" -gt 0 ]; }; then
            local t=$grow
            grow=$shrink
            shrink=$t
        fi
        per_cell=$(awk -v a="$amount" -v m="${moved#-}" 'BEGIN { printf "%.6f", a / m }')
        cur=$next
    done
    printf '%s\n' "$size"
}

axis_of() {
    if [ "$1" = cols ]; then echo "${2% *}"; else echo "${2#* }"; fi
}

# wait_frame PANE → the visible text once two reads in a row agree (and are
# not blank), with trailing blanks trimmed.
wait_frame() {
    local end=$((SECONDS + TIMEOUT)) prev="" cur
    while [ "$SECONDS" -le "$end" ]; do
        cur="$(herdr pane read "$1" --source visible --format text | sed 's/[[:space:]]*$//')" ||
            die "herdr pane read $1 failed"
        if [ -n "$(printf '%s' "$cur" | tr -d '[:space:]')" ] && [ "$cur" = "$prev" ]; then
            printf '%s\n' "$cur"
            return 0
        fi
        prev="$cur"
        sleep 0.2
    done
    die "no stable frame in pane $1 within ${TIMEOUT}s"
}

# ------------------------------------------------------------------ run ----

mkdir -p "$OUT"
rm -f "$OUT"/*.txt
printf '%-13s %-8s %-8s %s\n' case target real file
CAPTURED=""
while read -r name tc tr mode; do
    sock="$WORK/$name.sock"
    split "$BASE" right
    col="$NEW_PANE"
    pane="$col"
    if [ "$tr" != max ]; then
        split "$col" down
        pane="$NEW_PANE"
    fi
    flags="--present --step 7"
    [ "$mode" = normal ] && flags="--step 7"
    herdr pane run "$pane" "$(printf '%q' "$TIZA_BIN") $(printf '%q' "$DOC") $flags --no-save --socket $(printf '%q' "$sock")" >/dev/null
    size="$(wait_status "$sock")"
    if [ "$tc" = max ]; then
        # Leave the base pane about 12 columns.
        tc=$(($(axis_of cols "$size") * 2 - 12))
    fi
    size="$(fit "$pane" "$sock" cols "$tc" "$size")"
    [ "$tr" = max ] || size="$(fit "$pane" "$sock" rows "$tr" "$size")"
    frame="$(wait_frame "$pane")"
    real="${size% *}x${size#* }"
    file="$name-$real.txt"
    printf '%s\n' "$frame" >"$OUT/$file"
    target="${tc}x${tr}"
    [ "$tr" = max ] && target=max
    printf '%-13s %-8s %-8s %s\n' "$name" "$target" "$real" "$OUT/$file"
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
        rm -f "$SNAPS/${file%-*}"-[0-9]*x[0-9]*.txt
        cp "$OUT/$file" "$SNAPS/$file"
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
            echo "CHANGED $name: frame differs ($(grep -c '^[-+][^-+]' "$WORK/diff" || true) lines)"
            sed -n '1,40p' "$WORK/diff"
            changed="$changed $name"
        fi
    done
    if [ -n "$changed" ]; then
        echo "panes that changed:$changed"
        exit 1
    fi
    echo "no differences"
    ;;
esac
