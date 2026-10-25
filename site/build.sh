#!/bin/sh
# Build the docs site: prepare content/ from README.md and docs/, then run Zola.
# Usage: site/build.sh [--base-url URL]   (ZOLA and REPO can be set in the environment)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=${REPO:-$(dirname "$here")}
zola=${ZOLA:-zola}
python3 "$here/build.py" --repo "$repo"
cd "$here"
"$zola" build "$@"
python3 "$here/check_links.py" "$here/public"
