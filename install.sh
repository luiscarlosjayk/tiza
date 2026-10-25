#!/bin/sh
# Installs the tiza binary of a GitHub Release on macOS or Linux:
#
#   curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh
#
# It downloads the tar.gz for this platform, checks it against its .sha256
# file and moves `tiza` to ~/.local/bin (or $TIZA_INSTALL_DIR). It never asks
# for root access, and it writes only in that folder and in a temporary
# folder that it removes. `sh -s -- --help` lists the variables.
#
# The downloads always come from GitHub Releases, wherever this file is
# served from. TIZA_DOWNLOAD_BASE replaces
# https://github.com/luiscarlosjayk/tiza/releases: it is for tests only.
#
# Everything runs in main, on the last line: when a download of this file
# stops early, the shell runs nothing of it.

set -eu

REPO=luiscarlosjayk/tiza
SCRIPT_URL=https://raw.githubusercontent.com/$REPO/main/install.sh

usage() {
    cat <<EOF
Install the tiza binary of a GitHub Release.

Usage:
  curl -fsSL $SCRIPT_URL | sh
  curl -fsSL $SCRIPT_URL | sh -s -- --help

Environment variables:
  TIZA_VERSION      The release tag to install, for example v0.1.0.
                    The default is the latest release.
  TIZA_INSTALL_DIR  The folder for the binary.
                    The default is \$HOME/.local/bin.

Binaries: macOS on Apple silicon and Intel, and Linux x86_64 with glibc 2.35
or later. On other platforms, install from source: cargo install tiza
EOF
}

say() {
    printf '%s\n' "$*"
}

warn() {
    printf 'warning: %s\n' "$*" >&2
}

# fail LINE...: writes the first line after "error: " and the other lines
# as they are, then stops the script.
fail() {
    printf 'error: %s\n' "$1" >&2
    shift
    for line in "$@"; do
        printf '%s\n' "$line" >&2
    done
    exit 1
}

from_source() {
    fail "$1" "Install Tiza from source with Rust (https://rustup.rs): cargo install tiza"
}

# The Linux binary is built on Ubuntu 22.04 and needs its glibc, 2.35.
check_glibc() {
    libc=$(getconf GNU_LIBC_VERSION 2>/dev/null || true)
    if [ -z "$libc" ] && command -v ldd >/dev/null 2>&1; then
        libc=$(ldd --version 2>&1 | head -n 1 || true)
    fi
    case $libc in
        *musl*) from_source "This Linux uses musl libc. The Tiza binary needs glibc 2.35 or later." ;;
    esac
    glibc=${libc##* }
    major=${glibc%%.*}
    minor=${glibc#*.}
    minor=${minor%%.*}
    case $glibc:$major:$minor in
        *.*:[0-9]*:[0-9]*) ;;
        *) glibc= ;;
    esac
    case $major$minor in
        *[!0-9]*) glibc= ;;
    esac
    if [ -z "$glibc" ]; then
        warn "Cannot find the glibc version. The Tiza binary needs glibc 2.35 or later."
        return
    fi
    if [ "$major" -lt 2 ] || { [ "$major" -eq 2 ] && [ "$minor" -lt 35 ]; }; then
        from_source "This Linux has glibc $glibc. The Tiza binary needs glibc 2.35 or later."
    fi
}

# Prints 1 on Apple silicon, also in a process under Rosetta 2.
apple_silicon() {
    if command -v sysctl >/dev/null 2>&1; then
        sysctl -n hw.optional.arm64 2>/dev/null || true
    elif [ -x /usr/sbin/sysctl ]; then
        /usr/sbin/sysctl -n hw.optional.arm64 2>/dev/null || true
    fi
}

# Sets os and target, the target names of .github/workflows/release.yml.
detect_target() {
    os=$(uname -s)
    arch=$(uname -m)
    case $arch in
        arm64 | aarch64) arch=aarch64 ;;
        x86_64 | amd64) arch=x86_64 ;;
    esac
    case $os in
        Darwin)
            # On Apple silicon, uname says x86_64 under Rosetta 2 or when
            # an Intel uname comes first in PATH: install the native binary.
            if [ "$arch" = x86_64 ] && [ "$(apple_silicon)" = 1 ]; then
                arch=aarch64
            fi
            case $arch in
                aarch64 | x86_64) target=$arch-apple-darwin ;;
                *) from_source "Tiza has no binary for macOS on $arch." ;;
            esac
            ;;
        Linux)
            case $arch in
                x86_64) target=x86_64-unknown-linux-gnu ;;
                *) from_source "Tiza has no binary for Linux on $arch." ;;
            esac
            check_glibc
            ;;
        MINGW* | MSYS* | CYGWIN* | Windows_NT)
            fail "Tiza does not support Windows." \
                "Install WSL2. Then run this command in a WSL2 terminal:" \
                "  curl -fsSL $SCRIPT_URL | sh"
            ;;
        *) from_source "Tiza has no binary for $os on $arch." ;;
    esac
}

# Sets base, the releases URL, and proto, the protocol that curl may use.
pick_base() {
    base=${TIZA_DOWNLOAD_BASE:-https://github.com/$REPO/releases}
    base=${base%/}
    case $base in
        https://*) proto='=https' ;;
        http://*)
            proto='=http'
            warn "TIZA_DOWNLOAD_BASE uses http. Use it only for tests."
            ;;
        *) fail "TIZA_DOWNLOAD_BASE must start with https://." ;;
    esac
}

pick_tools() {
    if command -v curl >/dev/null 2>&1; then
        downloader=curl
    elif command -v wget >/dev/null 2>&1; then
        downloader=wget
        wget_help=$(wget --help 2>&1 || true)
    else
        fail "This script needs curl or wget. Install one of them, then run the script again."
    fi
    if command -v sha256sum >/dev/null 2>&1; then
        hasher=sha256sum
    elif command -v shasum >/dev/null 2>&1; then
        hasher=shasum
    else
        fail "This script needs sha256sum or shasum to check the download."
    fi
}

# Sets dir, the install folder, and checks it before any download.
pick_dir() {
    if [ -n "${TIZA_INSTALL_DIR:-}" ]; then
        dir=$TIZA_INSTALL_DIR
    elif [ -n "${HOME:-}" ]; then
        dir=$HOME/.local/bin
    else
        fail "HOME is not set. Set TIZA_INSTALL_DIR to the folder for the binary."
    fi
    if [ -d "$dir/tiza" ]; then
        fail "$dir/tiza is a folder. Remove it, or set TIZA_INSTALL_DIR to another folder."
    fi
    # The install makes the missing folders in the first folder of the path
    # that exists. Check that folder before the download.
    probe=$dir
    while [ ! -e "$probe" ]; do
        case $probe in
            */*) probe=${probe%/*} ;;
            *) probe=. ;;
        esac
        probe=${probe:-/}
    done
    if [ ! -d "$probe" ]; then
        fail "$probe is not a folder. Set TIZA_INSTALL_DIR to a folder."
    fi
    if [ ! -w "$probe" ]; then
        fail "You cannot write in $probe. Set TIZA_INSTALL_DIR to a folder of your user."
    fi
}

# wget_run ARG...: wget with the options of this wget that match the curl
# calls: TLS 1.2 or later, three tries, and no HSTS file in the home folder.
wget_run() {
    case $wget_help in
        *--no-hsts*) set -- --no-hsts "$@" ;;
    esac
    case $wget_help in
        *--secure-protocol*) set -- --secure-protocol=TLSv1_2 "$@" ;;
    esac
    case $wget_help in
        *--tries*) set -- --tries=3 --timeout=30 "$@" ;;
    esac
    wget "$@"
}

# fetch URL FILE. The shell writes FILE, because the curl snap of Ubuntu
# cannot write in /tmp.
fetch() {
    if [ "$downloader" = curl ]; then
        curl --proto "$proto" --tlsv1.2 -fsSL "$1" >"$2"
    else
        wget_run -q -O - "$1" >"$2"
    fi
}

# Sets tag. GitHub redirects releases/latest to releases/tag/TAG, and it
# skips prereleases. The API is not used: it has a rate limit.
pick_tag() {
    if [ -n "${TIZA_VERSION:-}" ]; then
        tag=v${TIZA_VERSION#v}
        tag_note="from TIZA_VERSION"
    else
        latest=$base/latest
        if [ "$downloader" = curl ]; then
            # The URL at the end of the redirects: a "location" line in
            # ~/.curlrc makes curl follow them in any case. No -f: only the
            # URL counts, not the page.
            location=$(curl --proto "$proto" --tlsv1.2 -sSL -o /dev/null -w '%{url_effective}' "$latest" || true)
        else
            location=$(wget_run -S --max-redirect=0 -O /dev/null "$latest" 2>&1 |
                sed -n 's/^ *[Ll]ocation: *\([^ ]*\).*/\1/p' | head -n 1)
        fi
        tag=${location##*/tag/}
        tag_note="the latest release"
    fi
    case $tag in
        v[0-9]*) ;;
        *) tag= ;;
    esac
    case $tag in
        *[!A-Za-z0-9._-]*) tag= ;;
    esac
    if [ -z "$tag" ]; then
        if [ -n "${TIZA_VERSION:-}" ]; then
            fail "TIZA_VERSION=$TIZA_VERSION is not a release tag. Use a tag such as v0.1.0."
        fi
        fail "Cannot find the latest release at $latest." \
            "Set TIZA_VERSION to a release tag, for example: TIZA_VERSION=v0.1.0"
    fi
}

tmp=
staged=
cleanup() {
    if [ -n "$staged" ]; then
        rm -f "$staged"
    fi
    if [ -n "$tmp" ]; then
        rm -rf "$tmp"
    fi
}

main() {
    for arg in "$@"; do
        case $arg in
            -h | --help)
                usage
                exit 0
                ;;
            *)
                printf 'error: unknown option: %s\n\n' "$arg" >&2
                usage >&2
                exit 2
                ;;
        esac
    done

    detect_target
    pick_base
    pick_tools
    pick_dir
    pick_tag
    say "Platform: $target"
    say "Version: $tag ($tag_note)"

    trap cleanup EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    tmpdir=${TMPDIR:-/tmp}
    tmp=$(mktemp -d "${tmpdir%/}/tiza-install.XXXXXX")

    # The names of the package step of release.yml.
    name=tiza-$tag-$target
    url=$base/download/$tag/$name.tar.gz
    say "Download: $url"
    fetch "$url" "$tmp/$name.tar.gz" ||
        fail "Cannot download $url." \
            "Make sure that release $tag exists and has a binary for $target."
    fetch "$url.sha256" "$tmp/$name.tar.gz.sha256" ||
        fail "Cannot download $url.sha256."

    want=$(cat "$tmp/$name.tar.gz.sha256")
    want=${want%% *}
    if [ "$hasher" = sha256sum ]; then
        got=$(sha256sum "$tmp/$name.tar.gz")
    else
        got=$(shasum -a 256 "$tmp/$name.tar.gz")
    fi
    got=${got%% *}
    if [ "${#want}" -ne 64 ] || [ "$got" != "$want" ]; then
        fail "The SHA-256 checksum of $name.tar.gz does not match its .sha256 file." \
            "Tiza is not installed."
    fi
    say "Checksum: OK (SHA-256)"

    tar -xzf "$tmp/$name.tar.gz" -C "$tmp" ||
        fail "Cannot extract $name.tar.gz. Tiza is not installed."
    if [ ! -f "$tmp/$name/tiza" ]; then
        fail "$name.tar.gz has no tiza binary. Tiza is not installed."
    fi

    mkdir -p -- "$dir" || fail "Cannot make the folder $dir."
    dir=$(CDPATH='' cd -- "$dir" && pwd)
    staged=$dir/.tiza.install.$$
    mv -f "$tmp/$name/tiza" "$staged"
    chmod 755 "$staged"
    # A download with curl or wget has no quarantine flag. Remove one anyway.
    if [ "$os" = Darwin ] && xattr -p com.apple.quarantine "$staged" >/dev/null 2>&1; then
        xattr -d com.apple.quarantine "$staged" ||
            warn "Cannot remove the quarantine flag of the new binary."
    fi
    # Run the new binary before it replaces the old one: when it does not
    # start on this machine, the old one stays.
    if ! started=$("$staged" --version </dev/null 2>&1); then
        fail "The tiza binary of $name does not start on this machine:" "$started" \
            "Tiza is not installed."
    fi
    mv -f "$staged" "$dir/tiza"
    staged=
    say "Installed: $dir/tiza"

    case ":${PATH:-}:" in
        *":$dir:"* | *":$dir/:"*)
            # dash gives DIR//tiza for a PATH entry DIR/.
            found=$(command -v tiza || true)
            found_dir=${found%/tiza}
            while [ "${found_dir%/}" != "$found_dir" ]; do
                found_dir=${found_dir%/}
            done
            if [ -n "$found" ] && [ "$found_dir" != "$dir" ]; then
                warn "The command tiza runs $found, which comes before $dir in PATH."
            fi
            ;;
        *)
            # The folder between double quotes, as text to copy: sh and fish
            # take \\, \" and \$ there, and sh also \`.
            fish_dir=$(printf '%s\n' "$dir" | sed 's/[\\"$]/\\&/g')
            # shellcheck disable=SC2016 # A backquote for sed, not a command.
            sh_dir=$(printf '%s\n' "$fish_dir" | sed 's/`/\\`/g')
            case ${SHELL:-} in
                */fish)
                    say "$dir is not in PATH. Run this command in fish, then open a new terminal:"
                    say "  fish_add_path \"$fish_dir\""
                    ;;
                *)
                    case ${SHELL:-} in
                        */zsh) rc=.zshrc ;;
                        */bash) if [ "$os" = Darwin ]; then rc=.bash_profile; else rc=.bashrc; fi ;;
                        *) rc=.profile ;;
                    esac
                    say "$dir is not in PATH. Add this line to ${HOME:-\$HOME}/$rc, then open a new terminal:"
                    say "  export PATH=\"$sh_dir:\$PATH\""
                    ;;
            esac
            ;;
    esac

    if ! "$dir/tiza" --version </dev/null; then
        fail "$dir/tiza is installed, but it does not start."
    fi
    say 'Next: run "tiza FILE" to open a file, or "tiza --help" for the options.'
}

main "$@"
