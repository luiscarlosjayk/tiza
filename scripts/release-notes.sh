#!/usr/bin/env bash
# Prints the body of the GitHub Release for a tag:
#
#   scripts/release-notes.sh v0.1.0 [OWNER/REPO]
#
# - vX.Y.Z: docs/release-notes/vX.Y.Z.md as written (a leading byte order
#   mark and CRLF line ends removed). Without that file it fails and names
#   the path: a version is not released without its notes.
# - vX.Y.Z-suffix (-rc.N, -alpha.N): the notes of X.Y.Z when they exist,
#   otherwise two lines saying it is a prerelease of X.Y.Z, with a link to
#   the CHANGELOG at the tag.
#
# Notes that name a board card (`t-` and three digits) are refused: the
# release body is public. .github/workflows/release.yml and
# scripts/release-local.sh both write notes.md with it.

set -euo pipefail

usage() {
  sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

case "${1:-}" in
  -h|--help) usage 0 ;;
esac
[ $# -ge 1 ] && [ $# -le 2 ] || usage 2
tag="$1"
repo="${2:-luiscarlosjayk/tiza}"
if ! [[ $tag =~ ^v([0-9]+\.[0-9]+\.[0-9]+)(-[0-9A-Za-z.]+)?$ ]]; then
  echo "the tag is vX.Y.Z or vX.Y.Z-suffix (for example v0.1.0 or v0.1.0-rc.1), got '$tag'" >&2
  exit 2
fi
version="${BASH_REMATCH[1]}"
suffix="${BASH_REMATCH[2]}"

cd "$(dirname "$0")/.."
notes="docs/release-notes/v$version.md"

if [ -f "$notes" ]; then
  body="$(perl -pe 's/^\xEF\xBB\xBF// if $. == 1; s/\r$//' "$notes")"
  if printf '%s\n' "$body" | grep -Eq '(^|[^A-Za-z0-9])t-[0-9]{3}([^0-9]|$)'; then
    echo "$notes names a board card (t- and three digits): the release notes are public" >&2
    exit 1
  fi
  printf '%s\n' "$body"
elif [ -n "$suffix" ]; then
  printf 'Prerelease of Tiza %s, to try before the release.\n' "$version"
  printf 'What changed: [CHANGELOG](https://github.com/%s/blob/%s/CHANGELOG.md).\n' "$repo" "$tag"
else
  echo "no release notes for $tag: write $notes first (README, \"Releases (maintainers)\")" >&2
  exit 1
fi
