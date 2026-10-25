# Install Tiza

The [README](https://github.com/luiscarlosjayk/tiza#installation) gives the install command. This page gives the options of the install script, the update and the install by hand.

Two environment variables change the install. `TIZA_VERSION` sets the release, for example `v0.1.0`. `TIZA_INSTALL_DIR` sets the folder for the binary:

```sh
curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | TIZA_VERSION=v0.1.0 TIZA_INSTALL_DIR="$HOME/bin" sh
```

Without `TIZA_VERSION`, the script installs the latest release and never a prerelease. To install a prerelease, set `TIZA_VERSION` to its tag, for example `v0.2.0-rc.1`.

To install from `main` of the repository, use Rust 1.88 or later:

```sh
cargo install --git https://github.com/luiscarlosjayk/tiza tiza
```

To read the script before you run it, download it first:

```sh
curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh -o install.sh
less install.sh
sh install.sh
```

To update Tiza, run the install command again. To get a notice of new versions, watch the repository on GitHub (**Watch** → **Custom** → **Releases**). Tiza itself does not check for updates.

**By hand:** download the archive for your system from [Releases](https://github.com/luiscarlosjayk/tiza/releases). The notes of each release tell which archive is for which system. The Linux archive needs glibc 2.35 or later, for example Ubuntu 22.04, Debian 12 or Fedora 36. The download size of an archive is 6 to 7 MB. `scripts/size-check.sh` keeps the archive under 8 MB and the binary under 16 MB. Tiza does not support Windows in version 0.x.

**macOS:** the binaries do not have an Apple Developer ID signature. If you download an archive with a web browser, macOS stops `tiza` the first time and shows *"tiza" Not Opened*. To fix this, run `xattr -d com.apple.quarantine /path/to/tiza` one time. A download with `curl` or with the install script does not have this problem.
