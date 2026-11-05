#!/usr/bin/env bash
# Makes the frames of the docs pages: one real `tiza render` export, a
# lossless WebP, for each `<!-- frame … -->` comment of a page. See
# scripts/docs_frames.py for the comment and the rules. The frames are not in
# the repository: site/build.sh makes them for the site. Run this script to
# see them before a push.
#
#   scripts/docs-frames.sh                     # every page with a frame, into target/docs-frames/
#   scripts/docs-frames.sh docs/reference/commands/view.md
#   scripts/docs-frames.sh --out /tmp/frames   # another folder
#   TIZA=path/to/tiza scripts/docs-frames.sh   # no build
#
# Needs python3 on macOS or Linux. It drives the TUI on a pseudo-terminal,
# so it also runs without a terminal, for example in CI.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if [[ -z "${TIZA:-}" ]]; then
  cargo build --release --locked -p tiza --quiet
  TIZA="$root/target/release/tiza"
fi

exec python3 -I "$root/scripts/docs_frames.py" "$TIZA" "$@"
