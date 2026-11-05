#!/usr/bin/env bash
# Counts the files in target/debug/deps and fails when they are more than a
# limit. On macOS, a file watcher of `tiza` (follow) starts more slowly when
# the folder of its binary holds many files: with a million, each start
# took seconds and the whole Mac was slow. The dev profile of Cargo.toml
# keeps the count low (`split-debuginfo = "packed"`); this script checks it.
#
#   scripts/deps-check.sh               # the limit is 50000 files
#   scripts/deps-check.sh --limit N
#
# It reads CARGO_TARGET_DIR, else target/ of the repository. A folder that
# is not there has 0 files. Exit 1 over the limit, 2 on bad arguments.
set -euo pipefail
LIMIT=50000
while [ $# -gt 0 ]; do
  case $1 in
    --limit)
      [ $# -ge 2 ] && [[ $2 =~ ^[0-9]+$ ]] || { echo "usage: deps-check.sh [--limit N]" >&2; exit 2; }
      LIMIT=$2; shift 2 ;;
    *) echo "usage: deps-check.sh [--limit N]" >&2; exit 2 ;;
  esac
done
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEPS=${CARGO_TARGET_DIR:-$ROOT/target}/debug/deps
N=0
[ -d "$DEPS" ] && N=$(find "$DEPS" -type f | wc -l | tr -d ' ')
if [ "$N" -gt "$LIMIT" ]; then
  echo "target/debug/deps: $N files, more than the limit of $LIMIT. Run \`cargo clean --profile dev\`: it keeps the release build." >&2
  exit 1
fi
echo "target/debug/deps: $N files (limit $LIMIT)"
