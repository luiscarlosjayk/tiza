# Install Tiza

Use this page to install Tiza on macOS or Linux, or to update it.

Tiza is a terminal tool that draws annotations on code.

## What you need

- macOS on Apple silicon or Intel, or Linux. Tiza does not support Windows in version 0.x.
- For the script or an archive on Linux: an x86_64 processor and glibc 2.35 or later. Examples are Ubuntu 22.04, Debian 12 and Fedora 36.
- On other Linux systems, for example arm64, musl or an older glibc, install with `cargo`.
- To install with `cargo`, Rust 1.88 or later.

## Install with the script

Run the install script:

```sh
curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh
```

The script downloads the archive for your system. It checks the SHA-256 checksum of the archive. Then it puts `tiza` in `~/.local/bin`. It does not use `sudo`. If `~/.local/bin` is not in your `PATH`, the script shows the line to add.

On Linux arm64, or with musl or a glibc older than 2.35, the script stops. It tells you to run `cargo install tiza`.

To check the install, run `tiza --version`. It shows the version and the git commit of the build, for example:

```
tiza 0.1.0 (ee7dbda8e2)
```

### Options of the script

Two environment variables change the install:

| Variable | What it does |
|---|---|
| `TIZA_VERSION` | Sets the release to install, for example `v0.1.0`. |
| `TIZA_INSTALL_DIR` | Sets the folder for the binary. |

For example:

```sh
curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | TIZA_VERSION=v0.1.0 TIZA_INSTALL_DIR="$HOME/bin" sh
```

- Without `TIZA_VERSION`, the script installs the latest release. It never installs a prerelease.
- To install a prerelease, set `TIZA_VERSION` to its tag, for example `v0.2.0-rc.1`.

### Read the script first

To read the script before you run it, download it first:

```sh
curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh -o install.sh
less install.sh
sh install.sh
```

## Other ways to install

| Source | How |
|---|---|
| crates.io | `cargo install tiza` |
| `main` of the repository | `cargo install --git https://github.com/luiscarlosjayk/tiza tiza` |
| An archive | Download it from [Releases](https://github.com/luiscarlosjayk/tiza/releases). |

- The notes of each release tell which archive is for which system.
- An archive is about 7 MB. The [Releases](https://github.com/luiscarlosjayk/tiza/releases) page gives the exact size.
- `scripts/size-check.sh` keeps the archive under 8 MB and the binary under 16 MB.

## Open Tiza on macOS

The binaries do not have an Apple Developer ID signature. If you download an archive with a web browser, macOS stops `tiza` the first time. It shows `"tiza" Not Opened`.

To let macOS open `tiza`, run this command one time:

```sh
xattr -d com.apple.quarantine PATH
```

`PATH` is the path of the `tiza` binary. A download with `curl` or with the install script does not have this problem.

## Update Tiza

- To update Tiza, run the install command again.
- To get a notification of new versions, watch the repository on GitHub: **Watch** → **Custom** → **Releases**.
- Tiza does not check for updates itself.

## See also

- [Follow the editor](follow-the-editor.md)
- [Check before you record](check-before-you-record.md)
- [Supervise an agent](supervise-an-agent.md)
