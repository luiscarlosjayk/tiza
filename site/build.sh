#!/bin/sh
# Build the docs site: make the frames of the pages, prepare content/ from
# README.md and docs/, then run Zola.
# Usage: site/build.sh [--base-url URL]   (ZOLA, REPO, DOCS_REF and TIZA can be set in the environment)
# DOCS_REF is the git ref that the site describes: a release tag, or main (the default).
# TIZA is the binary that makes the frames. Without it, the script builds
# the binary of the repository (cargo build --release).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=${REPO:-$(dirname "$here")}
zola=${ZOLA:-zola}
tiza=${TIZA:-}
if [ -z "$tiza" ]; then
  (cd "$repo" && cargo build --release --locked -p tiza --quiet)
  tiza="$repo/target/release/tiza"
fi
frames=$(mktemp -d "${TMPDIR:-/tmp}/tiza-frames.XXXXXX")
trap 'rm -rf "$frames"' EXIT
python3 -I "$repo/scripts/docs_frames.py" "$tiza" --out "$frames"
python3 "$here/build.py" --repo "$repo" --ref "${DOCS_REF:-main}" --frames "$frames"
cd "$here"
"$zola" build "$@"
python3 "$here/check_links.py" "$here/public"
