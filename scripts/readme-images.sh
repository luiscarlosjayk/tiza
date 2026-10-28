#!/usr/bin/env bash
# Regenerates every image the README and the docs show, with Tiza
# itself: no vhs, ttyd, agg or ffmpeg. Run it in the same commit as any
# visual change to the renderer (AGENTS.md), so the README never lags.
#
#   scripts/readme-images.sh            # builds target/release/tiza first
#   scripts/readme-images.sh --text     # only the text blocks: no images,
#                                       # no Chrome; builds tiza as the
#                                       # tests do (target/debug/tiza)
#   TIZA=path/to/tiza scripts/readme-images.sh
#   CHROME=path/to/chrome scripts/readme-images.sh
#
# Writes (all from examples/ownership.rs.tiza, dark theme, embedded fonts):
#   docs/img/ownership.webp        lossless animated WebP, from step 1 to
#                                  the last step (scale 1.5), with the
#                                  frames and the times of the GIF
#   docs/img/ownership-final.png   the last step (scale 2); the hero uses it too
#   docs/img/src/step{1,3}.png     the other real frames of the hero
#   docs/img/hero.png              1200×630 at device scale 2, from
#                                  docs/img/src/hero.html (needs Chrome or
#                                  Chromium; skipped with a message if none)
#   README.md                      the text block between the
#                                  readme-images:ownership-txt markers
#
# In each page of docs/ that git tracks, it also writes each text frame
# between these markers:
#   <!-- readme-images:frame doc=DOC step=N width=W -->
#   <!-- /readme-images:frame -->
# The frame is the output of `tiza render DOC --step N --width W --format
# md`, with the indent of the first marker. The tour of tally has a frame
# for each step.
#
# The `.tiza` examples are only read (`tiza render` never writes them).
set -euo pipefail

text_only=false
case "${1:-}" in
    "") ;;
    --text) text_only=true ;;
    *) echo "usage: scripts/readme-images.sh [--text]" >&2; exit 2 ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# The images never depend on the config file of whoever runs the script:
# an empty one, and an empty config folder.
no_config="$(mktemp -d)"
trap 'rm -rf "$no_config"' EXIT
: >"$no_config/config.toml"
export TIZA_CONFIG="$no_config/config.toml" XDG_CONFIG_HOME="$no_config"

# The README images are 92 columns wide. Every label of the example fits
# from 91 columns on (`tiza check` says so).
WIDTH=92
EXAMPLE=examples/ownership.rs.tiza
# The last step of the example: the highest step of an annotation or a move.
LAST="$(python3 -c 'import json, sys
d = json.load(open(sys.argv[1]))
print(max([a.get("step", 0) for a in d.get("annotations", [])] + [m["at_step"] for m in d.get("moves", [])]))' "$EXAMPLE")"

if [[ -z "${TIZA:-}" ]]; then
    if $text_only; then
        # The binary of the tests (tiza_qa::bin()), so cargo does not link
        # again between this script and the tests. The test hooks act only
        # with their variables, and this script sets none.
        cargo build -p tiza --features test-hooks >&2
        TIZA="$root/target/debug/tiza"
    else
        cargo build --release --locked -p tiza >&2
        TIZA="$root/target/release/tiza"
    fi
fi

render() { "$TIZA" render "$EXAMPLE" --width "$WIDTH" "$@"; }

if ! $text_only; then
    mkdir -p docs/img/src
    # The animation leaves out step 0 (the bare code) and uses scale 1.5, to
    # keep it light. The PNGs keep the default scale 2.
    render --skip-zero --scale 1.5 --out docs/img/ownership.webp
    render --step "$LAST" --out docs/img/ownership-final.png
    for s in 1 3; do
        render --step "$s" --out "docs/img/src/step$s.png"
    done
fi

# The text block in the README: the last step as plain text, from the first
# row to the bottom border of the first box (lines 1-17).
txt="$(render --step "$LAST" --out - --format txt)"
# A here-string, not a pipe: awk stops at the border, and under pipefail
# a writer that it leaves behind fails with SIGPIPE.
block="$(awk '{ print } $1 == "17" { want = 1; next } want && /╰/ { exit }' <<<"$txt")"
python3 - "$block" <<'EOF'
import sys, pathlib
p = pathlib.Path("README.md")
s = p.read_text()
start, end = "<!-- readme-images:ownership-txt -->", "<!-- /readme-images:ownership-txt -->"
i, j = s.index(start), s.index(end)
s = s[: i + len(start)] + "\n```\n" + sys.argv[1] + "\n```\n" + s[j:]
p.write_text(s)
EOF

# The text frames of the docs. A page that is up to date does not change.
python3 - "$TIZA" <<'EOF'
import os, re, subprocess, sys
tiza = sys.argv[1]
START = re.compile(r"^(\s*)<!-- readme-images:frame (.*) -->$")
END = "<!-- /readme-images:frame -->"
# Without the git variables of a parent process: they would point git at
# another repository.
env = {k: v for k, v in os.environ.items()
       if k not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR",
                    "GIT_OBJECT_DIRECTORY")}
listed = subprocess.run(["git", "ls-files", "-z", "--", "docs"], env=env, check=True,
                        capture_output=True).stdout.decode().split("\0")

def fail(page, n, why):
    sys.exit("readme-images: %s:%d: %s" % (page, n, why))

for page in sorted(p for p in listed if p.endswith(".md")):
    with open(page, encoding="utf-8") as f:
        text = f.read()
    if "readme-images:frame" not in text:
        continue
    lines = text.split("\n")
    out = []
    i = 0
    while i < len(lines):
        out.append(lines[i])
        m = START.match(lines[i])
        if not m:
            i += 1
            continue
        fields = dict(re.findall(r"(\w+)=(\S+)", m.group(2)))
        for name in ("doc", "step", "width"):
            if name not in fields:
                fail(page, i + 1, "the frame marker has no %s=" % name)
        j = i + 1
        while j < len(lines) and lines[j].strip() != END:
            if START.match(lines[j]):
                break
            j += 1
        if j == len(lines) or lines[j].strip() != END:
            fail(page, i + 1, "the frame marker has no %s after it" % END)
        r = subprocess.run(
            [tiza, "render", fields["doc"], "--step", fields["step"], "--width",
             fields["width"], "--format", "md", "--out", "-"],
            capture_output=True, text=True)
        if r.returncode != 0:
            fail(page, i + 1, r.stderr.strip())
        frame = r.stdout
        # A blank line gets no indent: no line ends with spaces.
        out.extend(m.group(1) + l if l else "" for l in frame.rstrip("\n").split("\n"))
        out.append(lines[j])
        i = j + 1
    new = "\n".join(out)
    if new != text:
        with open(page, "w", encoding="utf-8") as f:
            f.write(new)
        print("readme-images: wrote the frames of %s" % page, file=sys.stderr)
EOF

find_chrome() {
    if [[ -n "${CHROME:-}" ]]; then echo "$CHROME"; return; fi
    local c
    for c in chrome-headless-shell headless_shell google-chrome google-chrome-stable chromium chromium-browser chrome; do
        if command -v "$c" >/dev/null 2>&1; then command -v "$c"; return; fi
    done
    for c in "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
             "/Applications/Chromium.app/Contents/MacOS/Chromium"; do
        if [[ -x "$c" ]]; then echo "$c"; return; fi
    done
}

if $text_only; then
    exit 0
fi

chrome="$(find_chrome || true)"
if [[ -z "$chrome" ]]; then
    echo "readme-images: no Chrome/Chromium found (set CHROME=…): docs/img/hero.png NOT regenerated" >&2
else
    args=(--disable-gpu --hide-scrollbars --force-device-scale-factor=2
          --window-size=1200,630 --virtual-time-budget=5000
          --default-background-color=0d1117ff
          --screenshot="$root/docs/img/hero.png")
    # The old headless shell is headless already; full Chrome needs the flag.
    case "$(basename "$chrome")" in
        *headless*) ;;
        *) args=(--headless=new "${args[@]}") ;;
    esac
    if [[ "$(id -u)" == 0 ]]; then args=(--no-sandbox "${args[@]}"); fi
    "$chrome" "${args[@]}" "file://$root/docs/img/src/hero.html" >/dev/null 2>&1
fi

for f in docs/img/ownership.webp docs/img/ownership-final.png docs/img/hero.png \
         docs/img/src/step1.png docs/img/src/step3.png; do
    [[ -f "$f" ]] && printf '%9d bytes  %s\n' "$(wc -c <"$f")" "$f"
done
