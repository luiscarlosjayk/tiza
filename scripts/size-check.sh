#!/usr/bin/env bash
# Size budget of the release binary and its download: see scripts/size_check.py.
set -euo pipefail
exec python3 "$(dirname "$0")/size_check.py" "$@"
