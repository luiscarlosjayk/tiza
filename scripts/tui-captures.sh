#!/usr/bin/env bash
# Makes the captures of the TUI for the docs pages: the real TUI in a
# terminal (VHS), as near-lossless WebP images in docs/images/tui/. See
# scripts/tui_captures.py for the list of captures and what is fixed.
#
#   scripts/tui-captures.sh                     # every still capture
#   scripts/tui-captures.sh present-step scroll # these captures (scroll is animated)
#   scripts/tui-captures.sh --list              # the captures and their pages
#   scripts/tui-captures.sh --out /tmp/captures # the repository does not change
#   TIZA=path/to/tiza scripts/tui-captures.sh   # no build
#
# Needs macOS (the Menlo font), vhs, cwebp and img2webp: brew install vhs webp.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if [[ " $* " == *" --list "* ]]; then
  TIZA="${TIZA:-tiza}"
elif [[ -z "${TIZA:-}" ]]; then
  cargo build --release --locked -p tiza --quiet
  TIZA="$root/target/release/tiza"
fi

exec python3 -I "$root/scripts/tui_captures.py" "$TIZA" "$@"
