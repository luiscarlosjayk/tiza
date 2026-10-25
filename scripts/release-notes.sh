#!/usr/bin/env bash
# Prints the body of the GitHub Release for a tag:
#
#   scripts/release-notes.sh v0.1.0 [OWNER/REPO]
#
# - vX.Y.Z: docs/release-notes/vX.Y.Z.md as written (a leading byte order
#   mark and CRLF line ends removed). Without that file it fails and names
#   the path: a version is not released without its notes.
# - A prerelease (vX.Y.Z-rc.1, -alpha.2): its own notes,
#   docs/release-notes/vX.Y.Z-rc.1.md, when they exist; otherwise two lines
#   saying it is a prerelease of X.Y.Z, with a link to the CHANGELOG. Never
#   the notes of X.Y.Z: their archive names and links are not its own.
#
# The notes must be committed (in a git checkout: the release is built from
# the tag), have some text, and name no board card (`t-` and three digits):
# the release body is public. .github/workflows/release.yml and
# scripts/release-local.sh both use it.

set -euo pipefail

usage() {
  sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

case "${1:-}" in
  -h|--help) usage 0 ;;
esac
[ $# -ge 1 ] && [ $# -le 2 ] || usage 2
tag="$1"
repo="${2:-luiscarlosjayk/tiza}"
if ! [[ $tag =~ ^v([0-9]+\.[0-9]+\.[0-9]+)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
  echo "the tag is vX.Y.Z or vX.Y.Z-suffix (for example v0.1.0 or v0.1.0-rc.1), got '$tag'" >&2
  exit 2
fi
version="${BASH_REMATCH[1]}"
pre="${BASH_REMATCH[2]}${BASH_REMATCH[3]}"

# The checkout this script belongs to, also when called through a symlink.
self="$0"
while [ -L "$self" ]; do
  link="$(readlink "$self")"
  case "$link" in
    /*) self="$link" ;;
    *) self="$(dirname "$self")/$link" ;;
  esac
done
cd "$(dirname "$self")/.."

if [ -n "$pre" ]; then
  notes="docs/release-notes/$tag.md"
else
  notes="docs/release-notes/v$version.md"
fi

if [ -f "$notes" ]; then
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1 &&
    ! git ls-files --error-unmatch -- "$notes" >/dev/null 2>&1; then
    echo "$notes is not committed: git add it (the release is built from the tag)" >&2
    exit 1
  fi
  # Bytes, whatever PERL_UNICODE says: the mark is three bytes.
  body="$(perl -C0 -pe 's/^\xEF\xBB\xBF// if $. == 1; s/\r$//' "$notes")"
  if ! printf '%s' "$body" | grep -q '[^[:space:]]'; then
    echo "$notes has no text" >&2
    exit 1
  fi
  # A board card (`t-` and three digits), also with a Unicode hyphen or a
  # minus sign.
  if ! printf '%s\n' "$body" |
    perl -C0 -ne 'exit 1 if /(?<![A-Za-z0-9])t(?:-|\xE2\x80[\x90-\x95]|\xE2\x88\x92)[0-9]{3}(?![0-9])/'; then
    echo "$notes names a board card (t- and three digits): the release notes are public" >&2
    exit 1
  fi
  printf '%s\n' "$body"
elif [ -n "$pre" ]; then
  printf 'Prerelease of Tiza %s, to try before the release.\n' "$version"
  printf 'What changed: [CHANGELOG](https://github.com/%s/blob/%s/CHANGELOG.md).\n' "$repo" "$tag"
else
  echo "no release notes for $tag: write $notes first (docs/contributing/releasing.md, \"Releases (maintainers)\")" >&2
  exit 1
fi
