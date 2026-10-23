#!/usr/bin/env bash
# Sets the version of a release, before its commit and its tag:
#
#   scripts/bump-version.sh 0.1.0
#
# - Cargo.toml: the workspace version and the `version` of the four internal
#   crates in [workspace.dependencies] (a 0.x requirement is exact, so the
#   five move together);
# - Cargo.lock: `cargo update --workspace`;
# - CHANGELOG.md: the `## [Unreleased]` entries become `## [X.Y.Z] - date`
#   (today), under a new empty `## [Unreleased]`;
# - README.md: the version in the `> Status:` line.
#
# It changes those four files and nothing else; then write the release
# notes, commit, dry-run and tag as the README says ("Releases
# (maintainers)"). A final X.Y.Z only.

set -euo pipefail

usage() {
  sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

case "${1:-}" in
  -h|--help) usage 0 ;;
esac
[ $# -eq 1 ] || usage 2
new="$1"
if ! [[ $new =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "the version is X.Y.Z (no v, no suffix), got '$new'" >&2
  exit 2
fi

cd "$(dirname "$0")/.."

old="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)"
[ -n "$old" ] || { echo "no version in [workspace.package] of Cargo.toml" >&2; exit 1; }
[ "$new" != "$old" ] || { echo "the workspace is already at $old" >&2; exit 1; }
grep -qx '## \[Unreleased\]' CHANGELOG.md || { echo "CHANGELOG.md has no '## [Unreleased]' heading" >&2; exit 1; }
[ "$(grep -c "^> Status: \*\*v$old\*\*" README.md)" = 1 ] || { echo "README.md has no '> Status: **v$old**' line" >&2; exit 1; }
if grep -q "^## \[$new\]" CHANGELOG.md; then
  echo "CHANGELOG.md already has a $new section" >&2
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

# Literal replacement (no regex: the version has dots) in the two sections;
# exactly five lines must change, or nothing is written.
awk -v old="version = \"$old\"" -v new="version = \"$new\"" '
  /^\[/ { section = $0 }
  section == "[workspace.package]" && $0 == old { $0 = new; n++ }
  section == "[workspace.dependencies]" && /^tiza-(core|tui|mcp|export) = \{/ {
    i = index($0, old)
    if (i) { $0 = substr($0, 1, i - 1) new substr($0, i + length(old)); n++ }
  }
  { print }
  END { if (n != 5) { print "expected 5 versions to change in Cargo.toml, found " n > "/dev/stderr"; exit 1 } }
' Cargo.toml > "$tmp"
cat "$tmp" > Cargo.toml

cargo update --workspace --quiet

awk -v heading="## [$new] - $(date +%Y-%m-%d)" '
  !done && $0 == "## [Unreleased]" { print; print ""; print heading; done = 1; next }
  { print }
' CHANGELOG.md > "$tmp"
cat "$tmp" > CHANGELOG.md

awk -v old="> Status: **v$old**" -v new="> Status: **v$new**" '
  index($0, old) == 1 { $0 = new substr($0, length(old) + 1) }
  { print }
' README.md > "$tmp"
cat "$tmp" > README.md

cargo metadata --locked --no-deps --format-version 1 > /dev/null
echo "version $old -> $new: Cargo.toml, Cargo.lock, CHANGELOG.md and README.md updated"
