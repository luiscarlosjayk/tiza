# Command line

This page lists the shell commands, the arguments and the flags of `tiza`. It also explains how Tiza finds the language of the code.

## Open code

Give `tiza` a file, or send code to its standard input. For example:

```sh
tiza src/main.rs                            # open a file
tiza src/main.rs:12-48                      # open lines 12 to 48, with their line numbers
tiza src/main.rs --follow                   # reload when you save the file in the editor
pbpaste | tiza --lang rs                    # open code from the clipboard (macOS)
tiza examples/ownership.rs.tiza --present   # open a document in presentation mode
```

`FILE` is one of these:

| Argument | What Tiza opens |
|---|---|
| `PATH` | The source file, or its document `PATH.tiza` when that file exists. |
| `PATH:A-B` | Lines `A` to `B` of the source file, with their real line numbers. |
| `PATH:A` | Line `A` of the source file. |
| `NAME.tiza` | The document. |
| `-` | Code or a document from standard input. |

- A range can also be `A..B`.
- Without `FILE`, Tiza reads standard input when it is not a terminal.
- Text on standard input that starts with `{` and contains `"schema"` is a document. Other text is code.
- With a range, Tiza saves to `FILE.A-B.tiza`. When that file exists, Tiza opens it, as it opens `FILE.tiza` without a range, and says so on the status bar. Another range, such as `A-C`, is another document. See [Where the document goes](../concepts/privacy-and-network.md#where-the-document-goes).
- A `.tiza` path with a range opens the document and ignores the range.
- When Tiza clamps a range, it saves under the clamped lines. For example, `tiza three.txt:2..9` on a file of three lines saves to `three.txt.2-3.tiza`.
- When a range goes past the end of the file, Tiza shows the lines that exist.

## Commands

| Command | What it does |
|---|---|
| `tiza [FILE]` | Opens the code or the document in the TUI. |
| `tiza render [FILE]` | Exports a document, or what the live instance shows, without the TUI. |
| `tiza check FILE` | Checks a document before you record, and writes nothing. |
| `tiza mcp` | Starts the MCP server on standard input and output. |
| `tiza scene get` | Prints the document of the live instance as JSON. |
| `tiza scene list` | Prints the annotations of the live instance, one on each line. |
| `tiza scene text` | Prints the take on screen as text. |
| `tiza scene status` | Prints the status of the live instance as JSON. |
| `tiza scene put FILE` | Replaces the document of the live instance with `FILE`. |
| `tiza scene patch JSON` | Changes part of the document of the live instance. |
| `tiza next` | Makes the live instance go to the next step. |
| `tiza prev` | Makes the live instance go to the previous step. |
| `tiza goto ID` | Makes the live instance go to a named step or to a step number. |
| `tiza api METHOD [PARAMS]` | Calls one socket method and prints the JSON reply. |
| `tiza schema [KIND]` | Prints a JSON Schema: `document` for the `.tiza` format, or `api` for the socket methods. `document` is the default. |
| `tiza help [COMMAND]` | Prints the help of `tiza` or of a command. |

- The live instance is the running TUI that agents and these commands talk to. See [Global flags](#global-flags).
- For each flag of `tiza render`, see [Options of `tiza render`](exports.md#options-of-tiza-render).
- `tiza check` is in [Check before you record](../guides/check-before-you-record.md).
- `tiza mcp` is in [Set up an agent](../agents/setup.md). The tools are in [MCP tools](mcp-tools.md).
- `tiza scene` and `tiza api` are in [Socket API](socket-api.md).
- `tiza next`, `tiza prev` and `tiza goto` are in [Go to a named step](document-format.md#go-to-a-named-step).

### Options of the commands

| Flag | Command | What it does |
|---|---|---|
| `--width N` | `tiza check` | Checks the labels at `N` columns. The default is `width` of the config file, else 100. |
| `--json` | `tiza check` | Prints the findings as JSON. |
| `--strict` | `tiza check` | Exits with code 1 also for warnings. |
| `--width N` | `tiza scene text` | Prints the take at `N` columns, in place of the width of the screen. |
| `--as-code` | `tiza scene put` | Shows a file that is not a document as code, and does not refuse it. |
| `--out PATH` | `tiza schema` | Writes the schema to `PATH`, in place of standard output. |

- `N` in `--width N` is from 20 to 400.
- `JSON` in `tiza scene patch` is the text of the JSON, `-` for standard input or `@PATH` for a file.
- `PARAMS` in `tiza api` is the text of the JSON, or `-` for standard input. Without `PARAMS`, Tiza sends `{}`.
- `FILE` in `tiza scene put` and `tiza check` can be `-` for standard input.
- `tiza check` also takes a source file that has a `.tiza` document next to it.

For example:

```sh
tiza check demo.tiza --json
tiza api annotation.add '{"type":"note","line":4,"text":"hello"}'
tiza scene patch @change.json
tiza schema api --out api.schema.json
```

## Flags of the TUI

These flags work with `tiza [FILE]`:

| Flag | What it does |
|---|---|
| `--lang LANG` | Sets the language of the code. |
| `--follow` | Starts follow mode. |
| `--present` | Starts in presentation mode. |
| `--step N` | Opens the document at step `N`. |
| `--focus A-B` | Dims all the lines except `A` to `B`. |
| `--theme NAME` | Sets the theme. See [Theme names](themes.md#theme-names). |
| `--theme-file PATH` | Loads the theme tokens of a TOML file into the document. |
| `--padding V[,H]` | Sets the blank cells around the code and its annotations for this session. |
| `--mouse` | Starts with mouse capture on. |
| `--no-save` | Starts in no-save mode. |
| `--no-socket` | Starts without the socket, so agents and `tiza next` cannot reach this TUI. |
| `-h` or `--help` | Prints the help. |
| `-V` or `--version` | Prints the version and the commit. |

- `LANG` is a language id, for example `rs`, `ts` or `py`, or a full name such as `rust`.
- `--step N` after the last step opens the last step.
- `--focus` also takes `A`, `A..B` or `A:B`. Press `Esc` to clear the focus.
- `--padding V` sets `H` to two times `V`. Tiza does not save `--padding` in the document. `:pad` saves it.
- Follow mode is in [Follow the editor](../guides/follow-the-editor.md).
- The themes and `--theme-file` are in [Themes](themes.md).
- Mouse capture is in [Use the mouse](../guides/present.md).
- No-save mode is in [Saving](../concepts/saving.md).

For example:

```sh
tiza examples/ownership.rs.tiza --present --step 3
tiza src/main.rs --theme light --padding 1,4
```

## Global flags

These flags work with each command:

| Flag | What it does |
|---|---|
| `--socket PATH` | Uses the socket at `PATH`. |
| `--session NAME` | Uses the socket of the session `NAME`. |

Tiza picks the socket from the first of these that is set:

1. `--socket PATH`.
2. The variable `TIZA_SOCKET`.
3. `--session NAME`.
4. The variable `TIZA_SESSION`.
5. The session `default`.

- Use a session to run two or more instances. Each instance has its own socket.
- In a session name, Tiza changes each character to `_`, except ASCII letters, digits, `-` and `_`.
- An empty `TIZA_SOCKET` or `TIZA_SESSION` counts as not set. An empty `--session` gives the socket `.sock`.
- `tiza render` uses the socket only without `FILE`. `tiza check` and `tiza schema` do not use it.
- The folder of the socket is in [The runtime folder](../concepts/privacy-and-network.md#the-runtime-folder).
- The variables are in [Environment variables](../concepts/privacy-and-network.md#environment-variables).

## How Tiza finds the language

Tiza uses the first of these that gives a language:

1. `--lang LANG`, or `:lang LANG` in the TUI.
2. The extension of the file, when Tiza has a grammar for it.
3. A well-known file name, for example `Makefile`, `Dockerfile`, `Cargo.lock` or `.bashrc`.
4. A `#!` line on the first line of the code.
5. A Vim or an Emacs modeline in the first five or the last five lines.
6. Otherwise, plain text (`txt`).

- Code from standard input has no file name. Thus Tiza skips items 2 and 3.
- Case does not matter in a file name or in `LANG`.
- When the value of `--lang` contains `/` or `\`, Tiza reads it as a path. It finds the language from items 2 to 6. See [The language of the selected text](../zed.md#the-language-of-the-selected-text).
- A language without a grammar shows as plain text. `--lang` then writes a warning on stderr.
- `:lang auto` finds the language again from items 2 to 6. The message tells which item gave it.
- A paste keeps the language of the code. When the code is plain text, a `#!` line or a modeline of the pasted text gives the language.

Tiza keeps the language in the document. `:lang` saves it with autosave on. `--lang` changes it for the session. The next save writes it. A document that you open again uses its saved language.

## Check report

`tiza check --json` prints this report.

| Field | Meaning |
|---|---|
| `ok` | `true` when the check found no error. |
| `errors` | The number of errors. |
| `warnings` | The number of warnings. |
| `width` | The width of the check. |
| `max_step` | The last step of the document. It is not there when the document does not load. |
| `min_clean_width` | The narrowest width from which no label is cut, or `null`. |
| `narrower` | The narrower widths that are clean too: `clean`, `code_fits_from` and `margin_from`. |
| `findings` | The list of findings. |

- `min_clean_width` is `null` when the document does not load, or when no width up to 400 shows each label.
- `narrower` is there only when `width` is under `min_clean_width` and no label is cut at `width`.
- Each finding has `level`, `code` and `message`. It also has `id`, `step`, `file`, `line` and `text` when they apply.
- `file` names a linked file. `text` is the full text of a cut label, in a `clipped_label` finding.

The `code` of a finding is one of these:

| Code | Level | What it reports |
|---|---|---|
| `schema` | Error | The document does not follow the `.tiza` format. |
| `code` | Error | Tiza cannot read the code of the document. |
| `bounds` | Error | A number is out of range, for example a step past the limit. |
| `link` | Error | A linked file cannot be read. |
| `duplicate_id` | Error | Two annotations have the same id. |
| `steps` | Error | An entry of `steps` is not valid. |
| `steps` | Warning | An entry of `steps` names a step past the last one, or has a key that Tiza does not know. |
| `move` | Error | A move is not valid, or it does not apply at its step. |
| `rebind` | Error | The `rebind` of a move is not valid, or its target does not match after the patch. |
| `stale` | Error | In a live instance, a move does not match the code after a reload or a paste. |
| `unknown_id` | Error | The `ids` of a spotlight name an annotation that does not exist. |
| `unknown_id` | Warning | A `rebind` names an annotation that does not exist. Tiza ignores that `rebind`. |
| `unknown_file` | Error | An annotation names a file that no `code.link` shows. |
| `anchor` | Error | An anchor does not match at its step. |
| `marker` | Error | A marker does not match, or the end marker is only before the start marker. |
| `marker` | Warning | A marker matches more than one time. |
| `unanchored` | Warning | A move removes the code that an annotation is on. |
| `outside_window` | Warning | An annotation is outside the `code.link` window of its step, or a later patch pushes it out. |
| `not_shown` | Warning | An annotation belongs to a file that its own step does not show. |
| `empty_step` | Warning | A step changes nothing. |
| `clipped_label` | Warning | A label is cut, or is not drawn, at the width of the check. |

## Messages

Tiza writes these messages on stderr:

| Message | When |
|---|---|
| `tiza: give a file (tiza src/main.rs:12-48) or pipe code through stdin (pbpaste \| tiza --lang rs)` | Tiza gets no file, and its standard input is a terminal. |
| `tiza: empty stdin` | The standard input is empty or has only blank characters. |
| `` tiza: the TUI needs a terminal on stdout; use `tiza render` for text output `` | The standard output of Tiza is not a terminal. |
| `tiza: could not open FILE: line ranges start at 1` | The range starts at line 0. |
| `tiza: could not open FILE: range A-B: the first line must not be after the last` | `A` is after `B`. |
| `` tiza: could not open FILE: malformed line range `R`: use A, A-B or A..B `` | The text after the last `:` starts with a digit, or it has only digits, `-` and `.`. But it is not `A`, `A-B` or `A..B`. |
| `tiza: could not open FILE` | Tiza cannot read the file. |
| `tiza: --padding expects V or V,H` | `--padding` got another value. |
| `tiza: invalid range: R` | `--focus` got a value that is not a range. |
| `tiza: width=N: use a number between 20 and 400` | `tiza check --width` got a number out of the range. |
| `tiza: FILE is not a .tiza document and has no .tiza document next to it (DOCUMENT): nothing to check` | `tiza check` got a source file without a document next to it. |
| ``tiza: FILE is neither a .tiza document nor a `scene get` envelope; to show it as code, pass --as-code`` | `tiza scene put` got a file that is not a document. |
| `tiza: could not read PATH` | `tiza check`, `tiza scene put` or `tiza scene patch @PATH` cannot read the file. |
| `tiza: the patch is not valid JSON: ERROR` | The JSON of `tiza scene patch` does not parse. |
| `tiza: the parameters are not valid JSON: ERROR` | The `PARAMS` of `tiza api` do not parse. |
| `tiza: the parameters from stdin are not valid JSON: ERROR` | The `PARAMS` of `tiza api -` do not parse. |
| `wrote PATH` | `tiza schema --out PATH` wrote the file. |
| `tiza: config: PATH:LINE: WARNING` | Tiza does not take an entry of the config file. The warning tells why. |
| `tiza: config ignored: PATH: ERROR` | Tiza cannot read the config file. |
| `` tiza: no grammar for `LANG`; the code is shown as plain text `` | `--lang` got a language that has no grammar. |
| `` tiza: no Tiza instance is listening on PATH (open `tiza <file>` or pass --socket/--session) `` | A command finds no live instance. |
| `tiza: the Tiza instance on PATH did not answer within 75 s: it is busy or stopped; look at its terminal` | A command reached the live instance, but it did not reply in time. |
| `tiza: the Tiza instance on PATH closed the connection before it answered: it possibly quit; look at its terminal` | The live instance closed the connection before it replied. |
| `tiza: MESSAGE (CODE)` | The live instance answers a command with an error. |

- The messages `no grammar for …` and `config ignored: …` are warnings. Tiza goes on.
- When the text after the last `:` does not start with a digit and has other characters than digits, `-` and `.`, Tiza reads the whole text as a file name, for example `a:b.rs`. Then the message is `could not open FILE`.
- A file that is there opens as a file, also when its name has a `:` and a digit, for example `notes 12:00.md`.
- `tiza: config ignored: PATH` ends with the error text of the TOML reader or of the system.
- For `tiza scene text --width`, the live instance checks the width. Then the message ends with ` (invalid_argument)`, as in `tiza: MESSAGE (CODE)`.
- `could not open FILE`, `could not read PATH` and `no Tiza instance is listening on …` can end with the error of the system. `--padding expects V or V,H` can end with the reason of the error.
- The errors of `tiza goto` are in [Messages](document-format.md#messages).
- On an error, `tiza` exits with code 1. A wrong flag or argument exits with code 2 and prints the usage.
- `tiza render` to MP4 or WebM without `ffmpeg` also exits with code 2. See [Exports](exports.md).

Tiza shows these messages in the status bar:

| Message | When |
|---|---|
| `the file has N lines: range A-B clamped to C-D` | The range goes past the end of the file. |
| `opened the existing PATH` | The range has a `FILE.A-B.tiza` document from an earlier session, and Tiza opened it. |
| `local API disabled: an instance is already listening on PATH (use --session for another one)` | A second instance starts in a session that already has an instance. |

- For a file with one line, the first message says `line`, not `lines`.
- After `local API disabled`, agents and the step commands talk to the first instance.
- The other causes of `local API disabled` are in [The runtime folder](../concepts/privacy-and-network.md#the-runtime-folder).
