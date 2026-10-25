# Files and environment

This page lists each file, socket, process and environment variable that the `tiza` binary uses. [Privacy and network](../concepts/privacy-and-network.md) tells why all of it stays on your computer.

## Files that Tiza reads

| File | When Tiza reads it |
|---|---|
| The source file that you open | At start, and again when the file changes in follow mode. |
| A range of a file, `FILE:A-B` | At start, without the `FILE.tiza` document. |
| The `.tiza` document | When you open it or its source file, or give it to `tiza render`, `tiza check` or `tiza scene put`. |
| Any file, as code | When you give `tiza scene put --as-code FILE`, with the `.tiza` document of that file. |
| A file that a document links with `code.link` | When Tiza loads the document, when you give `:link PATH` or `:ref PATH`, or when an agent adds a `code.link` move. |
| The file in `follows` | When you give `:follow on`. |
| The config file | When the TUI starts, and at each `tiza render`. |
| A theme file | When you give `--theme-file PATH` or `:theme file PATH`. |
| A JSON file | When you give `tiza scene patch @PATH`. |
| The fonts of the system | When `tiza render` has `--system-fonts`. |

- When you give `-` as a file name, Tiza reads standard input.
- In follow mode, Tiza watches the folder of the file for changes. It does not watch the subfolders.
- `tiza check` reads the document and its linked files. It writes nothing.
- `tiza mcp` reads the modification time of its own binary.

### Paths in a document

- A relative path starts at the folder of the document.
- A `.tiza` document on standard input and `tiza check -` use the current folder.
- For a `scene.put` or a `move.add` from an agent, a relative path starts at the folder of the open document.
- Tiza does not read from disk a link that the document embeds in `views`.

### What agents send

An agent sends documents, code and commands through the socket.

- When an agent sends a document with `scene.put`, Tiza reads the files that the document links.
- When an agent adds a `code.link` move, Tiza reads that file.
- `scene.get` and `code.get` give the code back to the agent.
- `tiza api`, `tiza scene`, `tiza render` without a file and `tiza mcp` connect to the socket of an instance.

### Fonts

- Without `--system-fonts`, Tiza uses only the fonts that it contains.
- `--system-fonts` works for a PNG, a GIF, an MP4 and a WebM export.
- `:export` and `export_image` never use the fonts of the system.

### The terminal

Tiza asks the terminal for its colors only when the theme is `auto`, the default, or `terminal`.

- At start, Tiza asks through `/dev/tty` when standard input is not a terminal. `TIZA_NO_OSC` stops this question. Any value works, even an empty one.
- At each resize, Tiza asks again through standard output. `TIZA_NO_OSC` does not stop this question.
- When standard input is a pipe, Tiza reads its keys from `/dev/tty`.

### The config file

Tiza looks for the config file in this order:

1. `$TIZA_CONFIG`.
2. `$XDG_CONFIG_HOME/tiza/config.toml`.
3. `~/.config/tiza/config.toml`.

- An empty `TIZA_CONFIG` or `XDG_CONFIG_HOME` counts as not set.
- An empty `HOME` counts as set. Then Tiza looks for `.config/tiza/config.toml` in the current folder.
- Without `HOME` and without `XDG_CONFIG_HOME`, Tiza reads no config file.
- When the file does not exist, Tiza uses its defaults.
- When Tiza cannot read or parse the file, it shows a warning and uses its defaults.

## Files that Tiza writes

| File | When Tiza writes it |
|---|---|
| The `.tiza` document | At each change with autosave on, and at each save command (see [Saving](../concepts/saving.md)). |
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
- A GIF, an MP4 or a WebM export.
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
| A GIF or a file of `--all-steps` | `.NAME.tiza-tmp`, where `NAME` is the full name |
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
- Tiza makes the missing folders up to the folder of the socket or of `panics.log`. It sets only the last folder to `0700`. The others get the permissions of your umask.
- Tiza does not check the result of that change, or the owner of the folder.
- At start, Tiza makes the folder of the socket only when the socket is on. With `--socket PATH`, that is the folder of `PATH`.
- With `--no-socket`, Tiza makes no folder at start.
- Before each line of `panics.log`, Tiza makes the runtime folder. It does this also with `--socket PATH` or `--no-socket`.
- After it makes the socket, Tiza sets the socket to `0600`.
- The socket is `SESSION.sock`, or `default.sock` without a session.
- To set the session, use `--session NAME` or `$TIZA_SESSION`.
- To set the full path of the socket, use `--socket PATH` or `$TIZA_SOCKET`.
- To start Tiza with no socket, use `--no-socket`.

`--socket PATH` and `$TIZA_SOCKET` also change what Tiza does on disk:

- Tiza sets the folder of `PATH` to `0700`.
- When a file is at `PATH` and no instance listens there, Tiza removes that file.

### Removal of the socket

- Tiza removes the socket when it quits, when its terminal closes and after an internal error.
- When a signal kills Tiza, for example `SIGKILL`, the socket stays. The next start on that path removes it.

### `panics.log`

- Tiza adds lines to `panics.log`. It never removes the file. Tiza does not send it.
- Each line has the time, the version, the commit, the process id and a line of text.
- The text can quote a line of your document or name your files.
- The file gets the permissions of your umask. The `0700` folder protects it.

## Processes

Tiza itself starts one process: `ffmpeg`, for an MP4 or a WebM export.

- Tiza uses `$TIZA_FFMPEG`. Otherwise, it uses the first `ffmpeg` in `PATH`.
- Tiza does not include `ffmpeg` and does not download it. A GIF export does not need it.
- When the terminal does not give its size, the terminal library, not Tiza, can run `tput cols` and `tput lines`.

## Environment variables

Tiza reads these variables:

| Variable | What Tiza uses it for |
|---|---|
| `TIZA_CONFIG` | The path of the config file. |
| `TIZA_SESSION` | The name of the session. |
| `TIZA_SOCKET` | The path of the socket. |
| `TIZA_FFMPEG` | The path of `ffmpeg`. |
| `TIZA_NO_OSC` | Stops the question about the colors of the terminal at start. |
| `TIZA_TEST_PANIC` | Turns on the hidden command `:panic`, which stops Tiza on purpose for the tests. |
| `HOME` | The config file, and `~` at the start of a path in `:w`, `:saveas`, `:wq` and `:export`. |
| `XDG_CONFIG_HOME` | The folder of the config file. |
| `XDG_RUNTIME_DIR` | The runtime folder. |
| `TMPDIR` | The runtime folder, when `XDG_RUNTIME_DIR` is empty or not set. |
| `PATH` | Where to find `ffmpeg`, and `tput` for the terminal library. |

- A video export also uses `TMPDIR` for its temporary folder.
- `~` does not work in `:theme file` or in `:link`. On the command line, your shell expands `~`.
- The libraries that Tiza uses also read some variables. Examples are `TERM`, `COLORTERM`, `NO_COLOR`, `CLICOLOR` and `RUST_BACKTRACE`.
- `tiza mcp` also reads `TOKIO_WORKER_THREADS`.
- With `--system-fonts`, the font library reads `FONTCONFIG_FILE`, `HOME` and `XDG_CONFIG_HOME`.
