# Privacy and network

This page tells what stays on your computer when you use Tiza. It also lists each file, socket, process and environment variable that the `tiza` binary uses.

## The network

The `tiza` binary has no network dependencies. It never opens a network connection.

- Agents connect through a local Unix socket, never through TCP.
- The MCP server, `tiza mcp`, talks to the agent on its standard input and output.
- Tiza has no telemetry, no crash reports and no update check. It has no identifier of the user or of the machine.

## What Tiza reads and writes

All of this stays on your computer:

- Tiza reads the files that you open, the files that a document names, its config file and a theme file that you give.
- Tiza writes the `.tiza` document, the copies of the document and the exports that you ask for.
- By default, Tiza keeps its socket and its log, `panics.log`, in a runtime folder. That folder is `0700`, so that only your user can open it.
- Tiza refuses a runtime folder that another user owns.
- Tiza starts `ffmpeg` for an MP4 or a WebM export. When the terminal does not give its size, the terminal library can run `tput`.

## Files that Tiza reads

| File | When Tiza reads it |
|---|---|
| The source file that you open | At start, and again when the file changes in follow mode. |
| A range of a file, `FILE:A-B` | At start, without the `FILE.tiza` document. |
| The `.tiza` document | When you open it or its source file, or give it to `tiza render`, `tiza check` or `tiza scene put`. |
| Any file, as code | When you give `tiza scene put --as-code FILE`, with the `.tiza` document of that file. |
| A file that a document links with `code.link` | When Tiza loads the document, when you give `:link PATH` or `:ref PATH`, or when an agent adds a `code.link` move. |
| The file in `follows` | When you give `:follow on`. |
| The config file | When the TUI starts, at each `tiza render`, `tiza check` and `tiza config`, and at each `export_image`. |
| A theme file | When you give `--theme-file PATH` or `:theme file PATH`. |
| A JSON file | When you give `tiza scene patch @PATH`. |
| The fonts of the system | When `tiza render` has `--system-fonts`. |

- When you give `-` as a file name, Tiza reads standard input.
- In follow mode, Tiza watches the folder of the file for changes. It does not watch the subfolders.
- `tiza check` reads the document and its linked files. It writes nothing.
- `tiza mcp` reads the modification time of its own binary.

### Paths in a document

A document names its code and its linked files with a path. A path can be relative or absolute.

- A relative path starts at the folder of the document.
- A `.tiza` document on standard input and `tiza check -` use the current folder.
- For a `scene.put` or a `move.add` from an agent, a relative path starts at the folder of the open document.
- Tiza does not read from disk a link that the document embeds in `views`.

> **Warning:** Tiza can read any file that you can read. Thus a document from another person can show a file of yours on the screen, or put it in an export. Read the paths in a document from another person before you open it.

### What agents send

An agent sends documents, code and commands through the socket, or through `tiza mcp`.

- When an agent sends a document with `scene.put`, Tiza reads the files that the document links.
- When an agent adds a `code.link` move, Tiza reads that file.
- `scene.get` and `code.get` give the code back to the agent.
- An agent can export an image to a path.
- `tiza api`, `tiza scene`, `tiza render` without a file and `tiza mcp` connect to the socket of an instance.

Connect only the agents that you trust with the code on your screen.

### Fonts

- Without `--system-fonts`, Tiza uses only the fonts that it contains.
- `--system-fonts` works for a PNG, a WebP, a GIF, an MP4 and a WebM export.
- `:export` and `export_image` never use the fonts of the system.

### The terminal

Tiza sends the color query to the terminal only when the theme is `auto`, the default, or `terminal`. [Terminal support](../reference/terminals.md#the-color-query) gives the rules of the wait.

- At start, Tiza sends the color query through `/dev/tty` when standard input is not a terminal.
- At each resize, Tiza sends it again through the same terminal as at start.
- At start, with any theme, Tiza also sends the width probe: it writes `⚠`, `↪`, `⏸` and `▶` and asks the terminal for the cursor position after each one. [The width probe](../reference/terminals.md#the-width-probe) gives the rules.
- `TIZA_NO_OSC` stops the color query at start and at each resize, and the width probe. Any value works, even an empty one.
- When standard input is a pipe, Tiza reads its keys from `/dev/tty`.

### The config file

Tiza reads one config file: `$TIZA_CONFIG`, else `$XDG_CONFIG_HOME/tiza/config.toml`, else `~/.config/tiza/config.toml`. Without `HOME` and without `XDG_CONFIG_HOME`, Tiza reads no config file. The full rules are in [Where Tiza finds the file](../reference/config.md#where-tiza-finds-the-file).

## Files that Tiza writes

| File | When Tiza writes it |
|---|---|
| The `.tiza` document | At each change with autosave on, and at each save command (see [Saving](saving.md)). |
| A copy of the document | When you give `:w PATH` or `:wq PATH`. |
| The document at a new path | When you give `:saveas PATH`, and at each save after it. |
| An export | When you give `:export PATH` or `tiza render --out PATH`, or an agent calls `export_image`. |
| One file for each step | When you give `tiza render --all-steps --out FOLDER/`. |
| A JSON schema | When you give `tiza schema --out PATH`. |
| The folder of the socket | At start, when the socket is on. |
| The runtime folder | Before each line of `panics.log`. |
| The socket | At start, except with `--no-socket`. |
| `panics.log` | When Tiza recovers from an error in an agent request, or when its terminal closes. |

### Where the document goes

| Source | Document |
|---|---|
| A file, `FILE` | `FILE.tiza`, next to the file |
| A range of a file, `FILE:A-B` | `FILE.A-B.tiza`, next to the file |
| Code on standard input | `tiza-TIME.tiza`, in the current folder |
| A `.tiza` file on standard input | `tiza-stdin.tiza`, in the current folder |

`TIME` is the time in seconds since 1970. Autosave writes the document at the first change.

### How Tiza writes a file

These files go to a temporary file in the same folder first. Then Tiza renames that file to the final name:

- The `.tiza` document.
- A GIF, an animated WebP, an MP4 or a WebM export.
- The files of `--all-steps`.
- The export of `export_image`.

Tiza writes the other exports and `tiza schema --out` directly.

- A rename replaces a symbolic link at the final name with a regular file. The target of the link does not change.
- Tiza makes the missing folders for the `.tiza` document, `:w`, `:saveas`, `:wq`, `:export` and `--all-steps`.
- `tiza render --out FILE`, `export_image` and `tiza schema --out` do not make missing folders.
- A relative path in `export_image` starts at the current folder of `tiza mcp`.

When a write stops before its end, the temporary file stays:

| Write | Temporary file |
|---|---|
| The `.tiza` document | `NAME.tiza.tmp`, where `NAME` is the name without its last extension |
| A GIF, an animated WebP or a file of `--all-steps` | `.NAME.tiza-tmp`, where `NAME` is the full name |
| An `export_image` file | `.NAME.tiza-PID.tmp`, where `NAME` is the full name |
| An MP4 or a WebM export | `.STEM.tiza-tmp.EXT`, where `STEM` is the name without its extension |

### Files of `--all-steps`

`--all-steps` removes old `step-N` files of the same format that this run did not write.

- It keeps the `step-N` files of other formats, and gives a warning.
- It keeps a `step-N` file with a different number of digits.
- It never removes a folder or a symbolic link.

### Video exports

- Tiza writes the frames of an MP4 or a WebM export to `tiza-video-PID` in the temporary folder of the system.
- The temporary folder is `$TMPDIR`. Without `TMPDIR`, macOS uses a folder for each user, and Linux uses `/tmp`. With an empty `TMPDIR`, Tiza uses the current folder.
- Tiza removes that folder at the end and after an error. When a signal stops the export, for example Ctrl-C, the folder stays.
- `ffmpeg` writes the video to `.STEM.tiza-tmp.EXT` in the output folder. Tiza then renames it. When `ffmpeg` fails, Tiza removes that file.

## The runtime folder

The runtime folder holds the socket and `panics.log`.

- It is `$XDG_RUNTIME_DIR/tiza/`. When that variable is empty or not set, it is `$TMPDIR/tiza/`, or else `/tmp/tiza/`.
- Tiza makes the `tiza/` folder with `0700`. When `tiza/` has wider permissions, Tiza sets it to `0700`.
- Tiza makes each missing folder above `tiza/` with `0700`.
- Tiza refuses a `tiza/` that is a symbolic link, that another user owns, or that it cannot set to `0700`.
- When Tiza refuses the folder and the socket is in it, Tiza starts with no socket. The status bar tells why.
- At start, Tiza makes the folder of the socket only when the socket is on. With `--socket PATH`, that is the folder of `PATH`.
- With `--no-socket`, Tiza makes no folder at start.
- Before each line of `panics.log`, Tiza makes the runtime folder. It does this also with `--socket PATH` or `--no-socket`.
- After it makes the socket, Tiza sets the socket to `0600`.
- The socket is `SESSION.sock`, or `default.sock` without a session.
- To set the session, use `--session NAME` or `$TIZA_SESSION`.
- To set the full path of the socket, use `--socket PATH` or `$TIZA_SOCKET`.
- To start Tiza with no socket, use `--no-socket`.

`--socket PATH` and `$TIZA_SOCKET` also change what Tiza does on disk:

- Tiza makes each missing folder of `PATH` with `0700`. It does not change a folder that exists.
- When a socket is at `PATH` and no instance listens there, Tiza removes it.
- When a regular file, a folder or a link is at `PATH`, Tiza does not remove it. Tiza starts with no socket, and the status bar tells why.

### Messages

| Message | When |
|---|---|
| `local API disabled: Tiza does not use DIR: it belongs to another user. Set XDG_RUNTIME_DIR or TMPDIR to a folder of yours, or give a socket path with --socket` | Another user owns the `tiza/` folder. |
| `local API disabled: Tiza does not use DIR: it is not a folder. Set XDG_RUNTIME_DIR or TMPDIR to a folder of yours, or give a socket path with --socket` | The `tiza/` folder is a symbolic link or a file. |
| `local API disabled: cannot make DIR private (0700)` | Tiza cannot set the `tiza/` folder to `0700`. |
| `local API disabled: PATH is there and is not a socket: Tiza does not remove it. Give another socket path with --socket or TIZA_SOCKET` | A regular file, a folder or a link is at the path of the socket. |
| `local API disabled: an instance is already listening on PATH (use --session for another one)` | Another Tiza listens on the path of the socket. |
| `local API disabled: cannot make the socket PATH private (0600)` | Tiza cannot set the socket to `0600`. |
| `local API disabled: cannot check the socket PATH` | A socket is at the path, but Tiza cannot connect to it, for example the socket of another user. |
| `PATH: ~ needs HOME, and HOME is empty or not set. Give the full path` | You type a path that starts with `~` in `:w`, `:saveas`, `:wq` or `:export`, and `HOME` is empty or not set. |

- The messages show in the status bar.
- `DIR` is the runtime folder. `PATH` is the path of the socket, or the path that you typed.
- The `cannot make` and `cannot check` messages end with the error of the system.

### Removal of the socket

- Tiza removes the socket when it quits, when its terminal closes and after an internal error.
- When a signal kills Tiza, for example `SIGKILL`, the socket stays. The next start on that path removes it.

### `panics.log`

- Tiza adds lines to `panics.log`. It never removes the file. Tiza does not send it.
- Each line has the time, the version, the commit, the process id and a line of text.
- The text can quote a line of your document or name your files.
- The file gets the permissions of your umask. The `0700` folder protects it.
- When Tiza refuses the runtime folder, it writes the line on standard error when it quits, not in `panics.log`.

## Processes

Tiza itself starts one process: `ffmpeg`, for an MP4 or a WebM export.

- Tiza uses `$TIZA_FFMPEG`. Otherwise, it uses the first `ffmpeg` in `PATH`.
- Tiza does not include `ffmpeg` and does not download it. A GIF or a WebP export does not need it.
- When the terminal does not give its size, the terminal library, not Tiza, can run `tput cols` and `tput lines`.

## Environment variables

Tiza reads these variables:

| Variable | What Tiza uses it for |
|---|---|
| `TIZA_CONFIG` | The path of the config file. |
| `TIZA_SESSION` | The name of the session. |
| `TIZA_SOCKET` | The path of the socket. |
| `TIZA_FFMPEG` | The path of `ffmpeg`. |
| `TIZA_NO_OSC` | Stops the color query and the width probe. |
| `HOME` | The config file, and `~` at the start of a path in `:w`, `:saveas`, `:wq` and `:export`. |
| `XDG_CONFIG_HOME` | The folder of the config file. |
| `XDG_RUNTIME_DIR` | The runtime folder. |
| `TMPDIR` | The runtime folder, when `XDG_RUNTIME_DIR` is empty or not set. |
| `PATH` | Where to find `ffmpeg`, and `tput` for the terminal library. |

- A video export also uses `TMPDIR` for its temporary folder.
- Without `HOME`, or with an empty `HOME`, the path `~` or a path that starts with `~/` gives an error. See [Messages](#messages).
- `~` does not work in `:theme file` or in `:link`. On the command line, your shell expands `~`.
- The libraries that Tiza uses also read some variables. Examples are `TERM`, `COLORTERM`, `NO_COLOR`, `CLICOLOR` and `RUST_BACKTRACE`.
- `tiza mcp` also reads `TOKIO_WORKER_THREADS`.
- With `--system-fonts`, the font library reads `FONTCONFIG_FILE`, `HOME` and `XDG_CONFIG_HOME`.

## How it works

- [`deny.toml`](../../deny.toml) bans a list of network crates for the whole workspace, for example `reqwest`, `hyper` and `rustls`. It also bans the `net` feature of `tokio`.
- CI runs `cargo-deny` on each push to `main` and on each pull request. Thus a change that adds one of these crates makes CI fail.
- `cargo-deny` does not see code that uses `std::net` directly.
- To check this yourself, read `Cargo.lock`. It has no HTTP or TLS crate.
