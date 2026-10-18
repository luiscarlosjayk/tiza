#!/usr/bin/env bash
# Speed baseline of the release binary (t-133): see scripts/bench.py.
set -euo pipefail
exec python3 "$(dirname "$0")/bench.py" "$@"
