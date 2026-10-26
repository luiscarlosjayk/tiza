#!/usr/bin/env bash
# Regenerates every image the README and the docs show, with Tiza
# itself: no vhs, ttyd, agg or ffmpeg. Run it in the same commit as any
# visual change to the renderer (AGENTS.md), so the README never lags.
#
#   scripts/readme-images.sh            # builds target/release/tiza first
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
# The `.tiza` examples are only read (`tiza render` never writes them).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# The README images are 92 columns wide. Every label of the example fits
# from 91 columns on (`tiza check` says so).
WIDTH=92
EXAMPLE=examples/ownership.rs.tiza
# The last step of the example: the highest step of an annotation or a move.
LAST="$(python3 -c 'import json, sys
d = json.load(open(sys.argv[1]))
print(max([a.get("step", 0) for a in d.get("annotations", [])] + [m["at_step"] for m in d.get("moves", [])]))' "$EXAMPLE")"

if [[ -z "${TIZA:-}" ]]; then
    cargo build --release --locked -p tiza >&2
    TIZA="$root/target/release/tiza"
fi

mkdir -p docs/img/src

render() { "$TIZA" render "$EXAMPLE" --width "$WIDTH" "$@"; }

# The animation leaves out step 0 (the bare code) and uses scale 1.5, to
# keep it light. The PNGs keep the default scale 2.
render --skip-zero --scale 1.5 --out docs/img/ownership.webp
render --step "$LAST" --out docs/img/ownership-final.png
for s in 1 3; do
    render --step "$s" --out "docs/img/src/step$s.png"
done

# The text block in the README: the last step as plain text, from the first
# row to the bottom border of the first box (lines 1-17).
txt="$(render --step "$LAST" --out - --format txt)"
block="$(printf '%s\n' "$txt" | awk '{ print } $1 == "17" { want = 1; next } want && /╰/ { exit }')"
python3 - "$block" <<'EOF'
import sys, pathlib
p = pathlib.Path("README.md")
s = p.read_text()
start, end = "<!-- readme-images:ownership-txt -->", "<!-- /readme-images:ownership-txt -->"
i, j = s.index(start), s.index(end)
s = s[: i + len(start)] + "\n```\n" + sys.argv[1] + "\n```\n" + s[j:]
p.write_text(s)
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
