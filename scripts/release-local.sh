#!/usr/bin/env bash
# Builds and packages a Tiza release on this machine, the same way
# .github/workflows/release.yml does, for when GitHub Actions is off.
#
#   scripts/release-local.sh v0.1.0                 # targets this machine can build
#   scripts/release-local.sh v0.1.0 --target x86_64-unknown-linux-gnu
#   scripts/release-local.sh v0.1.0-rc.1 --allow-untagged   # rehearse before tagging
#
# Output in dist/<tag>/: one tiza-<tag>-<target>.tar.gz (binary, README,
# licences, examples) and its .sha256 per target, plus notes.md with the
# commits since the previous tag. Then create the release by hand on
# GitHub (Releases → Draft a new release) and attach those files.
#
# Default targets: on macOS, aarch64-apple-darwin and x86_64-apple-darwin
# (the Intel one cross-compiled, as the workflow does); on Linux x86_64,
# x86_64-unknown-linux-gnu. Nothing is uploaded and nothing is pushed.

set -euo pipefail

usage() {
  sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

tag=""
targets=()
allow_untagged=0
out_root=""
while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --target) [ $# -ge 2 ] || { echo "--target needs a value" >&2; exit 2; }; targets+=("$2"); shift 2 ;;
    --allow-untagged) allow_untagged=1; shift ;;
    --out) [ $# -ge 2 ] || { echo "--out needs a directory" >&2; exit 2; }; out_root="$2"; shift 2 ;;
    -*) echo "unknown option: $1" >&2; usage 2 ;;
    *) [ -z "$tag" ] || { echo "only one tag, got '$tag' and '$1'" >&2; exit 2; }; tag="$1"; shift ;;
  esac
done
[ -n "$tag" ] || { echo "missing the tag (for example v0.1.0)" >&2; usage 2; }
if ! [[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
  echo "the tag must look like v0.1.0 or v0.1.0-rc.1, got '$tag'" >&2
  exit 2
fi
# --out is read from where the script was called, as any path argument;
# without it, dist/ at the top of the checkout.
case "$out_root" in ""|/*) ;; *) out_root="$PWD/$out_root" ;; esac

cd "$(git rev-parse --show-toplevel)"
[ -n "$out_root" ] || out_root="dist"

# The release must be exactly what the tag names: a clean tree at the tag.
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "the working tree has uncommitted changes: commit or stash them first" >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  if [ "$(git rev-parse "$tag^{commit}")" != "$(git rev-parse HEAD)" ]; then
    echo "tag $tag is not HEAD: run 'git checkout $tag' first" >&2
    exit 1
  fi
elif [ "$allow_untagged" -eq 0 ]; then
  echo "tag $tag does not exist: create it (git tag $tag && git push origin $tag) or pass --allow-untagged to rehearse" >&2
  exit 1
else
  echo "note: $tag is not a tag yet; packaging HEAD $(git rev-parse --short HEAD) under that name (rehearsal)"
fi

# The binary says its version (`tiza --version`): the tag must name it.
version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)"
# v0.1.0-rc.1 goes with 0.1.0: the X.Y.Z parts must be equal.
tag_base="${tag#v}"; tag_base="${tag_base%%-*}"
if [ "$tag_base" != "${version%%-*}" ]; then
  if [ "$allow_untagged" -eq 0 ]; then
    echo "tag $tag does not match the workspace version $version (Cargo.toml): make the version commit first" >&2
    exit 1
  fi
  echo "note: the binaries say version $version, not ${tag#v} (rehearsal)"
fi

if [ ${#targets[@]} -eq 0 ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-*) targets=(aarch64-apple-darwin x86_64-apple-darwin) ;;
    Linux-x86_64) targets=(x86_64-unknown-linux-gnu) ;;
    *) echo "no default targets for $(uname -s) $(uname -m): pass --target" >&2; exit 2 ;;
  esac
fi

sha256() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1"; else sha256sum "$1"; fi
}

host="$(rustc -vV | sed -n 's/^host: //p')"
out="$out_root/$tag"
rm -rf "$out"
mkdir -p "$out"

for target in "${targets[@]}"; do
  echo "== $target"
  if command -v rustup >/dev/null 2>&1 && ! rustup target list --installed | grep -qx "$target"; then
    rustup target add "$target"
  fi
  # Into ./target whatever CARGO_TARGET_DIR says: the copy below reads it.
  cargo build --release --locked --target "$target" --target-dir target -p tiza
  name="tiza-$tag-$target"
  stage="$out/$name"
  mkdir -p "$stage"
  cp "target/$target/release/tiza" "$stage/"
  cp README.md LICENSE-MIT LICENSE-APACHE "$stage/"
  # Only what the tag has: release.yml packs a fresh checkout, and a local
  # tree may hold build output or scratch files under examples/.
  git archive HEAD examples | tar -x -C "$stage"
  # No macOS extended attributes: the archive release.yml would make.
  COPYFILE_DISABLE=1 tar --no-xattrs -C "$out" -czf "$out/$name.tar.gz" "$name"
  (cd "$out" && sha256 "$name.tar.gz" > "$name.tar.gz.sha256")
  rm -rf "$stage"
  # Smoke: the binary for this machine must start from the archive.
  if [ "$target" = "$host" ]; then
    check="$(mktemp -d)"
    tar -C "$check" -xzf "$out/$name.tar.gz"
    said="$("$check/$name/tiza" --version)"
    echo "$said"
    # The build stamp names this commit, clean: not an older binary.
    case "$said" in
      *"($(git rev-parse --short=10 HEAD))"*) ;;
      *) echo "the packaged binary is not HEAD's clean build: $said" >&2; exit 1 ;;
    esac
    rm -rf "$check"
  fi
done

prev="$(git describe --tags --abbrev=0 "$tag^" 2>/dev/null || git describe --tags --abbrev=0 HEAD^ 2>/dev/null || true)"
repo="$(git config --get remote.origin.url || true)"
repo="$(printf '%s' "$repo" | sed -nE 's#^.*github\.com[:/]([^/]+/[^/]+)$#\1#p' | sed 's#\.git$##')"
[ -n "$repo" ] || repo="luiscarlosjayk/tiza"
{
  echo "## Tiza $tag"
  echo
  echo "Installation: download the tar.gz for your platform, verify the .sha256 and copy \`tiza\` into your PATH, or \`cargo install --git https://github.com/$repo tiza --tag $tag\`."
  echo
  echo "### Changes"
  if [ -n "$prev" ]; then git log --no-merges --pretty='- %s (%h)' "$prev..HEAD"; else git log --no-merges --pretty='- %s (%h)'; fi
} > "$out/notes.md"

echo
echo "Done: $out"
ls -l "$out"
echo
case "$tag" in *-*) pre="yes (the tag has a suffix)" ;; *) pre="no" ;; esac
cat <<EOF
Next, on GitHub (https://github.com/$repo/releases/new):
  1. Choose the tag $tag (push it first if it is not there).
  2. Title: Tiza $tag. Paste $out/notes.md as the description.
  3. Attach every .tar.gz and .sha256 of every machine that ran this script.
  4. Set as a pre-release: $pre.
EOF
