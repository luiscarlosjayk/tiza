#!/usr/bin/env bash
# Speed baseline of the release binary: see scripts/bench.py.
set -euo pipefail
exec python3 "$(dirname "$0")/bench.py" "$@"
