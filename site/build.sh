#!/bin/sh
# Build the docs site: prepare content/ from README.md and docs/, then run Zola.
# Usage: site/build.sh [--base-url URL]   (ZOLA, REPO and DOCS_REF can be set in the environment)
# DOCS_REF is the git ref that the site describes: a release tag, or main (the default).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=${REPO:-$(dirname "$here")}
zola=${ZOLA:-zola}
python3 "$here/build.py" --repo "$repo" --ref "${DOCS_REF:-main}"
cd "$here"
"$zola" build "$@"
python3 "$here/check_links.py" "$here/public"
