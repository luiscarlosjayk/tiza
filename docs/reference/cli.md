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
tiza talk.tiza --present                    # present a talk: several documents in a row
```

`FILE` is one of these:

<dl>
<dt><code>PATH</code></dt>
<dd>

The source file, or its document `PATH.tiza` when that file exists.

</dd>
<dt><code>PATH:A-B</code></dt>
<dd>

Lines `A` to `B` of the source file, with their real line numbers.

A range can also be `A..B`.

With a range, Tiza saves to `FILE.A-B.tiza`. When that file exists, Tiza opens it, as it opens `FILE.tiza` without a range, and says so on the status bar. Another range, such as `A-C`, is another document. See [Where the document goes](../concepts/privacy-and-network.md#where-the-document-goes).

When Tiza clamps a range, it saves under the clamped lines. For example, `tiza three.txt:2..9` on a file of three lines saves to `three.txt.2-3.tiza`.

When a range goes past the end of the file, Tiza shows the lines that exist.

</dd>
<dt><code>PATH:A</code></dt>
<dd>

Line `A` of the source file.

</dd>
<dt><code>NAME.tiza</code></dt>
<dd>

The document.

A `.tiza` path with a range opens the document and ignores the range.

</dd>
<dt><code>-</code></dt>
<dd>

Code or a document from standard input.

Text on standard input that starts with `{` and contains `"schema"` is a document. Other text is code.

</dd>
</dl>

Without `FILE`, Tiza reads standard input when it is not a terminal.

## Commands

<dl>
<dt><code>tiza [FILE]</code></dt>
<dd>

Opens the code or the document in the TUI.

</dd>
<dt><code>tiza render [FILE]</code></dt>
<dd>

Exports a document, or what the live instance shows, without the TUI.

For each flag of `tiza render`, see [Options of `tiza render`](exports.md#options-of-tiza-render).

</dd>
<dt><code>tiza check FILE</code></dt>
<dd>

Checks a document before you record, and writes nothing.

`FILE` can be `-` for standard input. It can also be a source file that has a `.tiza` document next to it.

With a master `.tiza`, it checks each scene of the talk and the master. See [Check and export the talk](../guides/present-a-long-talk.md#3-check-and-export-the-talk).

See [Check before you record](../guides/check-before-you-record.md).

</dd>
<dt><code>tiza config</code></dt>
<dd>

Shows the config file in use, each value with its source, the personal styles and the warnings of the file.

See [Config file](config.md#tiza-config).

</dd>
<dt><code>tiza mcp</code></dt>
<dd>

Starts the MCP server on standard input and output.

See [Set up an agent](../agents/setup.md). The tools are in [MCP tools](mcp-tools.md).

</dd>
<dt><code>tiza scene get</code></dt>
<dd>

Prints the document of the live instance as JSON.

</dd>
<dt><code>tiza scene list</code></dt>
<dd>

Prints the annotations of the live instance, one on each line.

</dd>
<dt><code>tiza scene text</code></dt>
<dd>

Prints the take on screen as text.

</dd>
<dt><code>tiza scene status</code></dt>
<dd>

Prints the status of the live instance as JSON.

</dd>
<dt><code>tiza scene put FILE</code></dt>
<dd>

Replaces the document of the live instance with `FILE`.

`FILE` can be `-` for standard input.

</dd>
<dt><code>tiza scene patch JSON</code></dt>
<dd>

Changes part of the document of the live instance.

`JSON` is the text of the JSON, `-` for standard input or `@PATH` for a file.

</dd>
<dt><code>tiza next</code></dt>
<dd>

Makes the live instance go to the next step. In a talk, at the last step of a scene, it goes to the next scene, as `n` does.

</dd>
<dt><code>tiza prev</code></dt>
<dd>

Makes the live instance go to the previous step. In a talk, at step 0 of a scene, it goes to the scene before, as `p` does.

</dd>
<dt><code>tiza goto ID</code></dt>
<dd>

Makes the live instance go to a named step or to a step number.

</dd>
<dt><code>tiza close</code></dt>
<dd>

Closes the live instance, as `:q` does, and waits until it exits.

See [Close an instance](#close-an-instance).

</dd>
<dt><code>tiza api METHOD [PARAMS]</code></dt>
<dd>

Calls one socket method and prints the JSON reply.

`PARAMS` is the text of the JSON, or `-` for standard input. Without `PARAMS`, Tiza sends `{}`.

</dd>
<dt><code>tiza schema [KIND]</code></dt>
<dd>

Prints a JSON Schema: `document` for the `.tiza` format, or `api` for the socket methods. `document` is the default.

</dd>
<dt><code>tiza help [COMMAND]</code></dt>
<dd>

Prints the help of `tiza` or of a command.

</dd>
</dl>

- The live instance is the running TUI that agents and these commands talk to. See [Global flags](#global-flags).
- `tiza scene` and `tiza api` are in [Socket API](socket-api.md).
- `tiza next`, `tiza prev` and `tiza goto` are in [Go to a named step](document-format.md#go-to-a-named-step).

### Options of the commands

<dl>
<dt><code>--width N</code> of <code>tiza check</code></dt>
<dd>

Checks the labels and the code lines in a terminal of `N` columns, as the TUI shows them. The side padding comes off `N`: the `padding` of the document, else `pad` of the config file, else 1 column at each side. The default is `width` of the config file, else 100.

`N` is from 20 to 400.

</dd>
<dt><code>--json</code> of <code>tiza check</code></dt>
<dd>

Prints the findings as JSON.

</dd>
<dt><code>--strict</code> of <code>tiza check</code></dt>
<dd>

Exits with code 1 also for warnings.

</dd>
<dt><code>--scene X</code> of <code>tiza check</code></dt>
<dd>

With a master `.tiza`, checks only the scene `X`, as `tiza check SCENE` does. `X` is the position of the scene or its name. Without it, Tiza checks every scene of the talk.

A skipped scene is refused: the message gives the command that checks its file.

</dd>
<dt><code>--width N</code> of <code>tiza scene text</code></dt>
<dd>

Prints the take at `N` columns, in place of the width of the screen.

`N` is from 20 to 400.

</dd>
<dt><code>--as-code</code> of <code>tiza scene put</code></dt>
<dd>

Shows a file that is not a document as code, and does not refuse it.

</dd>
<dt><code>--out PATH</code> of <code>tiza schema</code></dt>
<dd>

Writes the schema to `PATH`, in place of standard output.

</dd>
<dt><code>--force</code> of <code>tiza close</code></dt>
<dd>

Closes the live instance also when it has unsaved changes. The changes are lost.

</dd>
</dl>


For example:

```sh
tiza check demo.tiza --json
tiza api annotation.add '{"type":"note","line":4,"text":"hello"}'
tiza scene patch @change.json
tiza schema api --out api.schema.json
```

## Flags of the TUI

These flags work with `tiza [FILE]`:

<dl>
<dt><code>--lang LANG</code></dt>
<dd>

Sets the language of the code.

`LANG` is a language id, for example `rs`, `ts` or `py`, or a full name such as `rust`. It does not work with a talk: each scene has its own language.

</dd>
<dt><code>--follow</code></dt>
<dd>

Starts follow mode.

See [Follow the editor](../guides/follow-the-editor.md).

</dd>
<dt><code>--present</code></dt>
<dd>

Starts in presentation mode.

</dd>
<dt><code>--step N</code></dt>
<dd>

Opens the document at step `N`.

A step after the last step opens the last step. In a talk, it is a step of the scene that the talk opens at.

</dd>
<dt><code>--scene X</code></dt>
<dd>

Opens a talk at scene `X`: a position or a name.

Digits only are a position, from 1. Other text is a name, and it opens the first scene with that name. Without `--scene`, the talk opens where it was left. It works only with a master `.tiza`. See [Present a long talk](../guides/present-a-long-talk.md).

</dd>
<dt><code>--focus A-B</code></dt>
<dd>

Dims all the lines except `A` to `B`.

It also takes `A`, `A..B` or `A:B`. Press `Esc` to clear the focus.

</dd>
<dt><code>--theme NAME</code></dt>
<dd>

Sets the theme. See [Theme names](themes.md#theme-names).

</dd>
<dt><code>--theme-file PATH</code></dt>
<dd>

Loads the theme tokens of a TOML file into the document.

It does not work with a talk: put the theme in each scene. See [Themes](themes.md).

</dd>
<dt><code>--padding V[,H]</code></dt>
<dd>

Sets the blank cells around the code and its annotations for this session.

`--padding V` sets `H` to two times `V`. Tiza does not save `--padding` in the document. `:pad` saves it.

</dd>
<dt><code>--mouse</code></dt>
<dd>

Starts with mouse capture on.

See [Use the mouse](../guides/use-the-mouse.md).

</dd>
<dt><code>--no-counter</code></dt>
<dd>

Hides the step counter of presentation mode for this session. Then the last row shows code.

`counter = false` in the config file hides it for every session. See [Config file](config.md#which-value-wins).

</dd>
<dt><code>--counter</code></dt>
<dd>

Shows the step counter of presentation mode, also when the config file sets `counter = false`.

</dd>
<dt><code>--no-save</code></dt>
<dd>

Starts in no-save mode.

See [Saving](../concepts/saving.md).

</dd>
<dt><code>--no-socket</code></dt>
<dd>

Starts without the socket, so agents and `tiza next` cannot reach this TUI.

</dd>
<dt><code>-h</code> or <code>--help</code></dt>
<dd>

Prints the help.

</dd>
<dt><code>-V</code> or <code>--version</code></dt>
<dd>

Prints the version and the commit.

</dd>
</dl>

For example:

```sh
tiza examples/ownership.rs.tiza --present --step 3
tiza src/main.rs --theme light --padding 1,4
```

## Global flags

These flags work with each command:

<dl>
<dt><code>--socket PATH</code></dt>
<dd>

Uses the socket at `PATH`.

</dd>
<dt><code>--session NAME</code></dt>
<dd>

Uses the socket of the session `NAME`.

</dd>
</dl>

Tiza picks the socket from the first of these that is set:

1. `--socket PATH`.
2. The variable `TIZA_SOCKET`.
3. `--session NAME`.
4. The variable `TIZA_SESSION`.
5. The session `default`.

- A session has one instance. To run two or more instances, give each one its own session. Each instance has its own socket.
- When an instance listens on the socket already, a second `tiza FILE` does not start. It exits with code 1 and tells you how to close the first one or to use another session. See [Close an instance](#close-an-instance).
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
5. The header of a diff: the first line that is not blank starts with `diff --git`, or it starts with `--- ` and the next line starts with `+++ `. The code then gets the colors of a `.diff` file, for example in `git diff | tiza`.
6. A Vim or an Emacs modeline in the first five or the last five lines.
7. Otherwise, plain text (`txt`).

- Code from standard input has no file name. Thus Tiza skips items 2 and 3.
- Case does not matter in a file name or in `LANG`.
- When the value of `--lang` contains `/` or `\`, Tiza reads it as a path. It finds the language from items 2 to 7. See [The language of the selected text](../zed.md#the-language-of-the-selected-text).
- A language without a grammar shows as plain text. `--lang` then writes a warning on stderr.
- In a Markdown file, a fenced code block gets the colors of its language, also when its fence is indented by 1 to 3 spaces or is in a list item. A fenced code block in a `>` quote shows as plain text.
- `:lang auto` finds the language again from items 2 to 7. The message tells which item gave it.
- A paste keeps the language of the code. When the code is plain text, a `#!` line, the header of a diff or a modeline of the pasted text gives the language.

Tiza keeps the language in the document. `:lang` saves it with autosave on. `--lang` changes it for the session. The next save writes it. A document that you open again uses its saved language.

## Check report

`tiza check --json` prints this report.

<dl>
<dt><code>ok</code></dt>
<dd>

`true` when the check found no error.

</dd>
<dt><code>errors</code></dt>
<dd>

The number of errors.

</dd>
<dt><code>warnings</code></dt>
<dd>

The number of warnings.

</dd>
<dt><code>width</code></dt>
<dd>

The columns of code of the check: the terminal less its side padding.

</dd>
<dt><code>max_step</code></dt>
<dd>

The last step of the document. It is not there when the document does not load.

</dd>
<dt><code>min_clean_width</code></dt>
<dd>

The narrowest width from which no label is cut, or `null`.

`min_clean_width` and `narrower` are in columns of code. Give them to `tiza render --width`, which has no padding. To pick the width of the terminal, use the values in `terminal`.

`min_clean_width` is `null` when the document does not load, or when no width up to 400 shows each label.

</dd>
<dt><code>narrower</code></dt>
<dd>

The narrower widths that are clean too: `clean`, `code_fits_from` and `margin_from`.

`narrower` is there only when `width` is under `min_clean_width` and no label is cut at `width`.

</dd>
<dt><code>terminal</code></dt>
<dd>

The terminal of the check: its `width`, the `padding` at each side, and `min_clean_width` and `narrower` in columns of the terminal.

</dd>
<dt><code>findings</code></dt>
<dd>

The list of findings.

Each finding has `level`, `code` and `message`. It also has `id`, `step`, `file`, `line` and `text` when they apply.

`file` names a linked file. `text` is the full text of a cut label, in a `clipped_label` finding.

The messages of the findings give the width of the terminal. A `clipped_code` message also gives the columns of code for `tiza render`.

</dd>
</dl>

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
| `move` | Warning | The target of a `scroll` move does not match at its step, or its `find` matches more than one time. Or a move has an action, a value or a key that this version of Tiza does not know, also a key inside a `select` point or a `rebind` entry. See [Moves from a newer version](document-format.md#moves-from-a-newer-version). |
| `rebind` | Error | The `rebind` of a move is not valid, or its target does not match after the patch. |
| `stale` | Error | In a live instance, a move does not match the code after a reload or a paste. |
| `unknown_id` | Error | The `ids` of a spotlight name an annotation that does not exist. |
| `unknown_id` | Warning | A `rebind` names an annotation that does not exist. Tiza ignores that `rebind`. |
| `unknown_file` | Error | An annotation names a file that no `code.link` shows. |
| `until` | Error | The `until` of an annotation is before its `step`. Thus the annotation never shows. |
| `until` | Warning | The `ids` of a spotlight name an annotation at a step after the `until` of that annotation. The spotlight does not keep it in focus there. |
| `anchor` | Error | An anchor does not match at its step. |
| `anchor` | Warning | A span or an arrow end of an annotation has a key that this version of Tiza does not know. Tiza ignores it. |
| `marker` | Error | A marker does not match, or the end marker is only before the start marker. |
| `marker` | Warning | A marker matches more than one time. |
| `unanchored` | Warning | A move removes the code that an annotation is on. |
| `outside_window` | Warning | An annotation is outside the `code.link` window of its step, or a later patch pushes it out. |
| `not_shown` | Warning | An annotation belongs to a file that its own step does not show. |
| `empty_step` | Warning | A step changes nothing. A step where an annotation goes, because a spotlight stops or an `until` passes, gets this warning only when its take is the same as the take of the step before. |
| `clipped_label` | Warning | A label is cut, or is not drawn, at the width of the check. |
| `clipped_code` | Warning | A code line is cut with `…` at the width of the check. There is one warning for each file with a cut line, and one more when a move changes which lines are cut. The warning gives the first five lines, the steps, and the width from which every code line of the take fits, or that no width up to 400 shows them whole. When a move gives the cut lines other numbers, the warning gives their numbers at its first step. |
| `center` | Warning | The document has `center`, and its code is wider than the check. The code stays at the left. The warning gives the width from which Tiza centers it, or says that no width up to 400 centers it. |
| `effect` | Warning | Experimental. An annotation has the enter effect `draw` but no stroke, or an effect that this version of Tiza does not know. It comes in with no effect. See [Enter effects](document-format.md#enter-effects-experimental). |

### Check report of a talk

With a master `.tiza`, `tiza check --json` prints this report.

<dl>
<dt><code>ok</code>, <code>errors</code>, <code>warnings</code></dt>
<dd>

As in the report of a document, for the master and all its scenes.

</dd>
<dt><code>clean_terminal_width</code></dt>
<dd>

The narrowest terminal at which every scene shows its labels whole: the widest of the scenes. Each scene has its own padding.

It is `null` when a scene that shows was not checked, or when a scene cuts a label at each width up to 400.

</dd>
<dt><code>clean_code_width</code></dt>
<dd>

The same in columns of code, for `tiza render --width`. It can come from another scene than `clean_terminal_width`.

</dd>
<dt><code>findings</code></dt>
<dd>

All the findings: first those of the master, then those of each scene. A finding of a scene has `scene`, its position, and `scene_file`, its path as the master gives it. The other keys are those of a finding of a document.

A problem of an entry of the master has the code `talk`.

</dd>
<dt><code>scenes</code></dt>
<dd>

One item for each entry of the master, in order: `position`, `name`, `file`, `status` and `report`.

`status` is one of these:

- `checked`: `report` is the report of `tiza check SCENE`.
- `skipped`: the scene has `skip`. Tiza does not read it.
- `missing`: Tiza cannot read the file.
- `talk`: the file is a master.
- `duplicate`: a scene before it shows the same file.
- `invalid`: the entry is not valid.

</dd>
</dl>

## Close an instance

`tiza close` closes the live instance of the session, as `:q` does. Use it when you lost the terminal of an instance, or to start a new one in the same session.

```sh
tiza close --session talk
```

- Tiza sends `app.quit` to the instance and waits until its process exits. Then it writes `closed the Tiza instance of the session NAME` (or `on PATH` for `--socket`) and exits with code 0. The next `tiza --session NAME` starts at once.
- When the instance has unsaved changes, it does not close, and `tiza close` exits with code 1. `tiza close --force` closes it, and the changes are lost. Unsaved changes are changes with autosave off or `--no-save`, changes that Tiza could not save, and the marks of stale moves.
- When the instance does not answer in 10 s, or does not exit in 10 s after its answer, `tiza close` exits with code 1. Look at the terminal of the instance, or end it with `kill`. The instance does not close later for that request.
- `tiza close` does not remove a socket file. When no instance listens on the socket (its process was killed with `kill -KILL`), the next `tiza` in the session replaces it.
- `kill` (SIGTERM) and `kill -INT` (SIGINT) also close an instance in order, with the save rules of Ctrl-C. The instance exits with code 143 or 130. A second signal ends it at once.

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
| `tiza: a talk on stdin has no folder for its scenes: give the path of the master .tiza` | `tiza check -` or `tiza render -` got a master. |
| `tiza: --scene opens a scene of a talk: give a master .tiza that lists the scenes` | `--scene` came with a file that is not a master. |
| `` tiza: scene N is skipped (`skip` in the talk); …; to check it anyway: `tiza check FILE` `` | `tiza check --scene` named a skipped scene. |
| ``tiza: FILE is neither a .tiza document nor a `scene get` envelope; to show it as code, pass --as-code`` | `tiza scene put` got a file that is not a document. |
| `tiza: FILE: … scene put sends a document as it is, so it refuses this one` | `tiza scene put` got a document with an effect, a move or a key that this version of Tiza does not know. The key can be a key of a move or a key inside a position. Open the file in Tiza and save it, or remove them. |
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
| `tiza: the session NAME has a Tiza instance already (PATH): tiza close --session NAME closes it, or open this one with --session OTHER (or --no-socket)` | A second `tiza FILE` starts in a session that has a live instance. For the session `default`, the message says `tiza close` and `--session NAME`. |
| `tiza: PATH has a Tiza instance already: tiza close --socket PATH closes it, or open this one with --socket OTHER (or --no-socket)` | A second `tiza FILE` starts on a socket from `--socket` or `TIZA_SOCKET` that has a live instance. |
| `closed the Tiza instance of the session NAME` | `tiza close` closed the instance, and its process exited. With `--socket` or `TIZA_SOCKET`, the message says `on PATH`. This message is on stdout. |
| `tiza: the instance has unsaved changes: tiza close --session NAME --force closes it and loses them` | `tiza close` found unsaved changes. The instance stays open. The command in the message has the same `--session NAME` or `--socket PATH` as yours, or no flag for the session `default`. |
| `tiza: the Tiza instance on PATH did not close in 10 s: look at its terminal, or end it with kill` | `tiza close` got no reply in 10 s, or the instance did not exit 10 s after its reply. |
| `tiza: the Tiza instance on PATH gave no answer that Tiza can read: ERROR` | `tiza close` reached the socket, but the connection failed, or the answer is not a Tiza answer. |

- The messages `no grammar for …` and `config ignored: …` are warnings. Tiza goes on.
- When the text after the last `:` does not start with a digit and has other characters than digits, `-` and `.`, Tiza reads the whole text as a file name, for example `a:b.rs`. Then the message is `could not open FILE`.
- A file that is there opens as a file, also when its name has a `:` and a digit, for example `notes 12:00.md`.
- `tiza: config ignored: PATH` ends with the error text of the system.
- The warnings of the config file are in [Warnings](config.md#warnings) of the config file.
- For `tiza scene text --width`, the live instance checks the width. Then the message ends with ` (invalid_argument)`, as in `tiza: MESSAGE (CODE)`.
- `could not open FILE`, `could not read PATH` and `no Tiza instance is listening on …` can end with the error of the system. `--padding expects V or V,H` can end with the reason of the error.
- The errors of `tiza goto` are in [Messages](document-format.md#messages).
- On an error, `tiza` exits with code 1. A wrong flag or argument exits with code 2 and prints the usage.
- After SIGTERM, the TUI exits with code 143. After SIGINT from outside the TUI, it exits with code 130. After SIGHUP, it exits with code 0.
- `tiza render` to MP4 or WebM without `ffmpeg` also exits with code 2. See [Exports](exports.md).

Tiza shows these messages in the status bar:

| Message | When |
|---|---|
| `the file has N lines: range A-B clamped to C-D` | The range goes past the end of the file. |
| `opened the existing PATH` | The range has a `FILE.A-B.tiza` document from an earlier session, and Tiza opened it. |
| `NAME:LINE: KEY: … (N warnings: tiza config)` | The config file has warnings. At start, the status bar shows the first one. See [Warnings](config.md#warnings). |
| `N annotations outside the code and not drawn: IDS` | When the document opens, annotations point at lines that the code does not have. |
| `tiza check gives the problem of N annotations: IDS` | When the document opens, annotations are on lines of the code, but they fail another check. `tiza check` gives the problem. |
| `N entries are not known: see tiza check` | The document has effects, moves or values that this version of Tiza does not know. See [Moves from a newer version](document-format.md#moves-from-a-newer-version). |

- For a file with one line, the first message says `line`, not `lines`.
- With one annotation, the messages of the open document say `annotation`. When the document has more than one of these problems at start, one message gives them, with `;` between them.
- The causes of `local API disabled` are in [The runtime folder](../concepts/privacy-and-network.md#the-runtime-folder).
