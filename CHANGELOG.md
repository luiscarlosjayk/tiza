# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/).

## [Unreleased]

### Added
- `status` has `save_error`: why the last save of the document failed, or `null` when the last save worked. An agent can see that its changes are not saved.

### Changed
- `tiza render FILE:A-B` also draws the annotations of `FILE.A-B.tiza` when that document exists, as `tiza FILE:A-B` opens it, and says so on stderr. Before, it showed only the lines.
- `tiza render FILE` without `--step` exports the last step of the document, the same as `--step LAST`. Thus a spotlight of the last step is on. Before, it showed all the annotations with no spotlight.
- `:margin N`, `:margin` and `--margin-cols` say that notes wrap at the margin, and that labels, legends and long notes are cut there. Before, they said only that notes wrap.
- An MP4 or a WebM export writes the `ffmpeg` command on stderr only when `ffmpeg` fails. Before, it wrote the command each time.
- A paste while the help, the list or the review is open, also while you type a filter, changes nothing. A paste in presentation mode changes nothing too. The status bar tells you what to do. Before, the paste replaced the code under the panel.

### Fixed
- A line range that starts with a digit but is not `A`, `A-B` or `A..B`, such as `main.rs:3-x`, gives the error of the range. Before, Tiza read `main.rs:3-x` as a file name and said `could not open`. `:link` and `:ref` at step 0 also refuse a bad range now. Before, they kept the range as a part of the file name.
- A paste on the command line adds only the first line of the pasted text, also when the terminal ends each pasted line with a carriage return. Before, the next lines went into the command too, for example as the title of a box.
- `--margin-cols` also changes a GIF, an animated WebP, an MP4, a WebM and a `.cast`. Before, only a still used it.
- A `.tape` with `--skip-zero` starts the TUI at step 1. Before, the TUI started at the step that the document saved. A `.tape` also passes `--theme`, `--theme-file` and `--lang` to the TUI, with `--no-save`. It refuses `--numbers`, `--no-numbers` and `--margin-cols` when they change the document, because the TUI takes these only from the document. Before, the tape dropped all these flags. Its `# Run:` line names the tape file.
- In a terminal wider than 402 columns, `tiza render` of the live instance exports 400 columns. Before, it failed with `width=N`. `:export` and `export_image` also export 400 columns at most.
- When Tiza cannot write a GIF, an animated WebP or a file of `--all-steps`, the message names the file that you gave. Before, it named a temporary file, and that file stayed.
- `export_image` checks `scale` only for a PNG, a WebP and a GIF, and `font_size` only for these and an SVG. Before, a bad `scale` also stopped a text export.
- With `skip_zero`, the reply of `export_image` gives `steps` from step 1, for example `1-7`. Before, it gave `0-7`.
- A `.cast` does not hold a step for a `wait` move that is stale, as the TUI does not. Before, it held the step for the time of the wait.
- The `wrote` line of a GIF, a WebP or a video shows a control character of the path as text, as the other `wrote` lines do.
- `--margin-cols` out of the range gives `--margin-cols=N: use a number between 12 and 200`. Before, the message offered `auto`, which only `:margin` takes.
- A command or an MCP tool that reaches the live instance, but gets no reply, says why: the instance did not answer within 75 s, or it closed the connection. Before, the message was `no Tiza instance is listening` for these cases too.
- The message about a `.tiza` document from a newer Tiza fits both a file and `scene.put`: `.tiza document with schema N: this version of Tiza reads schema 1 at most`.
- The `theme.set` schema and its error name all its values: `dark`, `light`, `terminal`, `auto` and `toggle`. The `add_mark` kinds and the help of `:mark` list `note` too. The descriptions of the MCP tools `check` and `add_move` and of the resource `tiza://scene` are in clear English.
- The help of `--interval` and `--system-fonts`, the `ffmpeg` messages, the `:export` message for a recording, the `skip_zero` description and the time-limit message of `export_image` are in clear English, without asides. The `ffmpeg` message no longer names Windows, which Tiza does not support.

## [0.1.1] - 2026-10-26

### Added
- WebP export, lossless. `tiza render --out demo.webp` writes an animation of all the steps, with the frames and the times of the GIF. `tiza render --step 3 --out p3.webp` writes a still of step 3, with the pixels of the PNG. `--all-steps --format webp`, `:export x.webp` and the MCP tool `export_image` also write WebP. A WebP is usually smaller than the PNG or the GIF of the same steps.
- The `tiza` binary contains libwebp, the WebP library of Google, in C. Thus `cargo install tiza` needs a C compiler. The release archives include the license of libwebp, `LICENSE-libwebp.txt`.
- The docs are on a website, https://tiza.luisk.dev. The site shows the docs of the latest release. It has a search box, a dark and a light theme, and a link on each heading.
- The agent pages have prompts to copy for your agent. One prompt sets up the Tiza MCP server. Three prompts show what to ask: explain the code, show a fix with a move, and review the code in supervised mode.
- The README and the agent setup page tell how to add the Tiza MCP server to Codex: `codex mcp add tiza -- tiza mcp`.

### Changed
- The reference of the `:` commands has a page for each group of commands. Each command has its own heading, which is the link to share, for example `#box`. A list after the syntax gives each option, its values and its default. Real Tiza frames show what the annotation commands draw.
- The animation of the README is a lossless WebP, 152 KB. Before, it was a GIF of 252 KB.
- `examples/ownership.rs.tiza` has 6 steps. A box with a label shows each way to pass a value: a move, a shared borrow and a mutable borrow. The callout at the end is gone. The README images show the new example.
- `tiza render --help` says what `--theme auto` and `--theme terminal` export: the theme of the document or `dark`, and `dark`.
- Messages, help and the schemas say "color", not "colour".
- The schema of a theme token no longer offers a palette name such as `orange`: Tiza does not take one there.
- `u` does not undo `--theme-file` now, as it does not undo `--theme`. `u` still undoes `:theme file PATH`.
- The README gives the install script first: `curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh`. The archives of Releases and `cargo install tiza` are the other ways to install.

### Fixed
- When the start-up wait for the colors of the terminal stops in the middle of an answer, Tiza takes the rest of the answer, which comes as key presses. Before, the theme could stay dark, or the rest acted as keys: it could move the cursor or open the command line.
- An image export of a very long document stops with a clear message, before it makes the SVG. When the lines of code alone make the image too large, `tiza render` and `export_image` stop before the layout too. Before, Tiza used gigabytes of memory and then failed with `invalid SVG`. This is true for PNG, WebP, GIF and video, and for SVG, which has a higher limit.
- `tiza render` refuses a `--margin` that is not 0 to 20 cells and a `--radius` that is not 0 to 100 px. Before, `NaN` or `inf` gave an empty image or a wrong error.
- `annotation.update` with `text` says what to send. Send `label` for the text of a note or the title of a callout. Send the whole callout as a new `shape` for the text of a callout. Before, it said that `label` changes the text of a callout.
- The MCP tool `update_annotation` refuses `text` itself. It says to remove the callout and add it again, and it no longer names fields that only the socket takes.
- An MCP tool that refuses an argument itself, such as an unknown `border`, ends its error with `(invalid_argument)`. A refusal of the live instance does the same. Before, these errors had no code, so an agent could not tell them apart.
- `tiza render --all-steps` refuses an `--out` such as `answer.md`, because it is a file name. Before, Tiza made a folder named `answer.md` with PNG files in it.
- The footer of the review shows `a accept all` and `d reject all`. Before, it showed `d none`, but `d` rejects every change that the review shows.
- Release archives include the licenses of the embedded fonts: `LICENSE-JetBrainsMono-OFL.txt` and `LICENSE-TizaSymbols-DejaVu.txt`, next to `LICENSE-MIT` and `LICENSE-APACHE`.
- Tiza does not remove a file at the socket path now. When `--socket` or `TIZA_SOCKET` names a file, a folder or a link, Tiza does not start the local API, and the status bar tells why. Tiza replaces only a socket that no instance listens on.
- Tiza changes permissions only of its own runtime folder, `tiza/` in `$XDG_RUNTIME_DIR`, `$TMPDIR` or `/tmp`. It makes that folder with 0700. It refuses the folder when it is a link or when it belongs to another user. For a socket path that you give, Tiza makes only the missing folders, and it does not change a folder that is there. Before, it made the folder of any socket private (0700), also your home folder or the current folder.
- When Tiza cannot use its runtime folder, it writes the line for `panics.log` to stderr when it quits.
- After a resize, Tiza asks the terminal for its colors only when `TIZA_NO_OSC` is not set. It asks the terminal that it reads the keys from, as it does at start. Before, it wrote the question to stdout.
- The release binary does not have the test command `:panic` now.
- An empty `HOME` counts as not set. Before, Tiza read `.config/tiza/config.toml` in the current folder.
- Without `HOME`, or with an empty `HOME`, a path that starts with `~/` in `:w`, `:saveas`, `:wq` or `:export` gives an error. Before, Tiza wrote the file in the current folder.
- With `--no-save`, `--theme-file` does not write the `.tiza` document at start now.
- `tiza FILE:A-B` opens `FILE.A-B.tiza` when it exists, as `tiza FILE` opens `FILE.tiza`, and says so on the status bar. Before, it started an empty document and autosave wrote over that file. Another range is another document.
- In supervised mode, the socket method `supervise.set` with `enabled: false` waits for the presenter, as any other change. Before, an agent could turn supervised mode off with no review. `enabled: true` and the presenter's `:supervise off` take effect at once. A script of the presenter that turns supervised mode off through the socket now waits for `y` too.
- When supervised mode goes off, each change that waited is rejected with the reason `supervised mode went off before the presenter decided this change`. A waiting `supervise.set` that turns it off gets the status instead.
- `supervise.set` refuses an unknown key, and wrong parameters get their error at once, also in supervised mode.
- The socket method `scene.clear` refuses parameters that it cannot read, with `invalid_argument`: a `step` that is not a whole number from 0, parameters that are not an object, and an unknown key. Before, such a request deleted every annotation. Supervised mode refuses it at once. The MCP tool `clear_annotations` refuses an unknown key and a `null` step too.
- The socket method `scene.put` refuses a document with a `schema` newer than this version (`unsupported_schema`), before the document changes. Before, Tiza saved that document, and then it could not open the file.
- In supervised mode, a socket request for a method that does not exist gets its error at once. Before, it waited as a change to review.
- `scene.patch {}` and an `annotation.update` that changes nothing leave `revision` as it is.
- The API schema names every field of the replies of `scene.clear`, `code.get`, `style.set` and `theme.set`.
- The help of `tiza scene patch` and the description of `scene.patch` name `steps`, `loop`, `numbers` and `margin`.
- In a theme table, the `[dark]`, `[light]` or `[terminal]` table of the theme in use wins over the flat tokens now. Before, the result depended on the names of the keys.
- A text token of a theme (`fg`, `muted`, `accent`, `note` or `error`) that reads under 4.5:1 moves toward black or white, whichever contrasts more with the background. It moves until it reads at 4.5:1. Before, on a mid-gray background it could move toward the wrong one and stop under 4.5:1.
- In the config file and in a theme file, a value that is not a color string leaves out only that token, with a report. Before, Tiza ignored the whole config file, and the theme file did not load. A theme file with no color token to apply, such as an empty file, does not load, and nothing changes.
- The config file reports each key or table other than `[theme]` on stderr.
- A document with a `theme_overrides` value that is not a string still does not open. The error now says that a token must be a color string or a table of color strings.
- MCP `export_image` applies the config file, as `tiza render` does. It says in `warning` what the config file left out, or that the config file does not parse. The export goes on.

## [0.1.0] - 2026-10-25

### Added
- `tiza render FILE --out demo.gif` and MCP `export_image` write an animated GIF from Tiza's own frames, one for each step, with `code.patch` moves typed at their speed. `--out demo.mp4` or `.webm` encodes the same frames with your own `ffmpeg`, from `PATH` or `$TIZA_FFMPEG`.
- `tiza render FILE --all-steps --out DIR/` writes one file for each step, `step-00.png` to `step-NN.png` (or `--format svg|txt|ansi|md`), all at the same pixel size. It removes the files of that format that an earlier run left after the last step.
- A `code.link` move shows another file, with its own annotations (field `file`), at its step and the steps after it. The moves `cursor`, `select` and `wait` put the cursor on a line, draw a selection or pause.
- Document `steps` entries give a step an `id`, a `title` and an `auto_advance_ms`, and `"loop": true` goes back to step 0 after the last step. `:goto ID`, `tiza goto ID`, socket `step.goto` and MCP `goto_step` go to a step by id or number.
- `f` shows or hides a live pointer that dims all the code except the cursor line or a `v`/`V` selection, and follows each motion. Agents set it with socket `pointer.set` or MCP `point_at`, and Tiza never saves or exports it.
- `tiza check FILE` lists every problem in a document, each with its annotation or move, step, file and line. The MCP tool and the socket method `check` give the same report on the live document.
- The replies of `annotation.add` and `annotation.update` tell if the annotation is on screen (`visible`, `shown_step`, `clipped`, and `hidden` with the reason). When an agent adds or changes an annotation that the view does not show, the status bar tells the presenter where it is.
- `:theme auto` and `--theme auto` follow the terminal's background, and `:theme terminal` and `--theme terminal` draw with the terminal's own palette. You can override token colors in `[theme]` of `~/.config/tiza/config.toml`, in `theme_overrides` of the document, or with `--theme-file PATH`.
- `:margin N` limits the right margin so that long notes wrap earlier, and `:numbers on|off` shows or hides the line numbers. The document keeps both settings, and every export uses them.
- With `--no-save` or `:autosave off`, Tiza does not write the sidecar, and `:q` refuses to quit with unsaved changes. Tiza treats a read-only sidecar the same way and shows one notice.
- `:saveas PATH` (`:sav`) writes the document to PATH, and all later saves go to PATH. Like `:w PATH` and `:wq PATH`, it does not overwrite a file that exists unless you add `!`.
- `SECURITY.md` tells how to report a vulnerability in private. `CONTRIBUTING.md` tells how to report a bug, and where to give ideas and feedback. Tiza does not accept pull requests for now.
- `docs/demos/scenarios.md` describes twelve ways to use Tiza with an agent. The new examples are `examples/tally-tour.tiza`, a tour of several files in `examples/tally/`, and `examples/stack-and-queue.tiza`.
- The new README section «Terminal support» tells what Tiza needs from a terminal, with known problems in Terminal.app, tmux and Zed. The new section «What Tiza reads and writes» tells that the binary never opens a network connection.
- `install.sh` installs the `tiza` binary of the latest release on macOS and Linux: `curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh`. It checks the SHA-256 checksum of the download and puts `tiza` in `~/.local/bin`, or in `$TIZA_INSTALL_DIR`. `TIZA_VERSION` sets the release to install.

### Changed
- The status bar warns only when the step on screen loses something: `! 3 labels cut` when labels or notes are cut or not drawn, and `! code needs 83 cols` when a code line is clipped. 83 is the terminal width from which the step shows every code line, with the line numbers, the arrow lanes and the padding. It no longer shows `! small terminal` on every terminal under 100 columns. `status.warnings` gives the details: the width, and the id of each label that is cut or not drawn.
- `examples/ownership.rs.tiza` has shorter labels, `into takes` and `borrowed`, and line 4 of its code says `// error: borrow of moved value`. At 100 columns no label is cut at any step, and at 80 columns the code fits.
- In a message on the status bar, a long path gives up its middle folders first (`/Users/…/nuevo/x.tiza`), so the file name stays.
- With `--no-save`, the status bar no longer suggests `:w`, which writes over the file that you opened. With changes, it shows `unsaved (:saveas PATH · :q! discards)`, and `:q` names the same two ways out. After `:saveas PATH`, every change saves to the new file. With `:autosave off` on your own file, the bar still shows `unsaved (:w)`.
- The status bar tells why Tiza does not save. It shows `no-save` with `--no-save`, which keeps the file that you opened as it is. It shows `autosave off` after `:autosave off`, and `read-only` when Tiza cannot write the file. After `:saveas PATH`, the message is `saved as PATH (autosave on)`, or `(autosave off)`. On a narrow bar, `no-save` and `autosave off` give way to a message or to the keys of a selection.
- The help (`?`) has sections in the order of the work: Move, Steps, Select and annotate, Edit an annotation, View, Commands, and Save and quit. Each row has two columns: the key or the command, and its description. Press `/` and type to show only the rows that match. `Esc` clears the filter first, and a second `Esc` closes the help. Each `:` command shows its full syntax, with the placeholders in the color of notes. The help and the list need a terminal of at least 40×12. In a smaller terminal, they do not open, and the status bar tells the size that they need. The hints on the status bar put the key first, for example `? help` and `▸ a2 … · enter label · e set`. When the bar is narrow, the hints keep their keys. In selection mode, the keys of the selection take the room of the optional segments. The mode `ARROW→` is now `ARROW`. When an error does not fit on the status bar, the bar also hides the step, and a warning shows only its sign `!`. `docs/ui-guidelines.md` gives the rules for the help and the other panels.
- The annotation list (`L`) is a panel like the help. It groups the annotations by step and by file, in columns: id, type, lines and label. The moves have a section of their own. `j`/`k` select a row, and Enter goes to its step with the annotation selected, as Tab does. Before, Enter closed the list. `x` deletes the row, and `u` and `U` undo and redo while the list stays open. `/` filters the rows. An unanchored annotation shows `! unanchored`, and a stale move shows `! stale`.
- The agent review no longer opens by itself. The status bar and presentation mode show `⏸ N to review · :review`. `:review` shows the review as a panel, where `j`/`k` select a change and `y` or `n` decides it.
- Ctrl with a letter no longer does what the letter does. For example, `Ctrl-U` does not undo and `Ctrl-Q` does not quit. Only `Ctrl-C` and `Ctrl-R` have a meaning.
- `:w PATH`, `:saveas PATH` and `:export PATH` take a relative path from the folder where you started Tiza, and `~` at its start is your home folder. They create the folders that are missing, and the message shows the full path of the file.
- When the text of an `L:text` anchor is not in the code shown, the message names that code, or the step that shows the text. It also says that L:text skips notes and labels, and it fits on a status bar of 120 columns. The hint for a text on another line starts with the command (`use 18:push_back`), so a narrow status bar does not cut it.
- Outside presentation mode, `←` and `→` move the cursor column, as `h` and `l` do. `n`, `p`, PageDown and PageUp change the step in every mode. To present with a clicker, use presentation mode (`P`), where `←` and `→` change the step, or a clicker that sends PageUp and PageDown.
- A document that names no theme follows the terminal's background, which Tiza asks for at start and after each resize. `t`, `:theme NAME` or `--theme` sets a fixed theme.
- Tiza draws two or more labels of one line under that line, and in narrow terminals an arrow's label is at its arrowhead (`─▶ move`). Where an arrow stub crosses the rail of another arrow, the stub has a gap of one cell, so it looks like it goes under.
- Presentation mode and every export use one geometry for the whole take, so the code does not move sideways between steps. In presentation mode, `:` shows the command line on the last row, and `Esc` leaves presentation mode.
- Exports set notes, labels and legends in JetBrains Mono, the font of the code, and an SVG embeds only the font faces that it uses. The red of the dark theme is lighter (`#FF4A66`), so that it stays legible after video compression.
- The `underline` emphasis uses the thin overline `‾`, not `▔`. The `underline`, `brackets` and `strike` emphases use 75 % of the annotation color, in the TUI and in SVG and PNG.
- The warning badge of the status bar is an ASCII `!`, not `⚠`, so terminals that draw `⚠` two cells wide no longer scroll. A message about an annotation on a later step tells how to see it, for example `(step 1, press n)`.
- `:w PATH` writes a copy that opens from its new folder, and Tiza continues to save to the open file, as in Vim. `Ctrl-C` cancels an open command line, selection, overlay or unfinished arrow, and quits Tiza in all other cases.
- `u` and `U` tell what they undid or redid, for example `undone: note a3`. A failed add, update or delete no longer leaves an extra redo entry.
- `tiza scene put` accepts the envelope of `scene get` and refuses other files that are not a document, unless you add `--as-code`. A put of a document from another source stops autosave, so an agent cannot overwrite the presenter's file.
- Tiza highlights files of 800 lines or more on several threads, and a presentation frame with moves takes about 5 ms, not 46 ms. Read-only requests such as `status` and `render.text` no longer redraw the screen.
- Socket and MCP errors name a parameter of the wrong type, for example `invalid type: string "three", expected u32`. For a value outside a fixed set, such as a color or a border, the error lists the valid names.
- For MCP clients that load tools on demand, such as Claude Code, the server instructions tell when and how to use Tiza. `status`, `get_code`, `render_text` and `add_box` carry `_meta["anthropic/alwaysLoad"]: true`, so the first call needs no search.
- `docs/zed.md` shows step tasks (`tiza next|prev|goto`) for Zed 1.24 that do not make the terminal blink. It also gives a selection task that Zed accepts and a list of known problems. The tasks for the selected text and for 40 lines from the cursor work in Zed 1.25.3, which runs each task with your shell.
- The README tells how to install with `cargo install tiza` from crates.io, and how to remove the macOS quarantine flag from the unsigned binary. Each GitHub Release has notes written by hand for that version, and its Linux binary runs with glibc 2.35 or later.
- `:link path[:A-B]` at step 1 or later records a `code.link` move. To make the document refer to a file, use `:ref`, or `:link` at step 0.
- `render.text` with a width (MCP `render_text`, `tiza scene text --width`) gives the take at that width. This is the same text that `tiza render --step S --width N --format txt` writes.

### Removed
- The binary no longer contains the brand serif (*Tiza Serif*) for notes, labels and legends in exports, and is about 345 KB smaller. Tiza rejects `--mono-labels` as an unknown option.
- `ping` on the socket answers the plain string `"pong"`, not a `{type, protocol}` object. `status` gives `protocol`.

### Fixed
- The MCP tool `update_annotation` takes `file`, as the socket `annotation.update` does: a linked file as a `code.link` names it, or `null` for the main file. Without `file`, the annotation keeps its file.
- `annotation.update` refuses a key that it does not know, names the key and lists the keys that it accepts. Before, `{"text": …}` on a note answered OK and changed nothing: to change the text of a note or a callout, send `label`.
- A paste turns follow mode off and tells you, so a later save does not reload over the paste. The `.tiza` keeps the followed file in an optional `follows` field, so `:follow on` can reload that file in a later session.
- With moves applied (step 1 or later), follow mode waits and loads the change at step 0. A save in that state no longer leaves a `.tiza` that does not open.
- Tiza no longer freezes when follow starts or stops its file watcher while macOS is slow to report file changes, for example during a build. If the file watcher cannot start, follow turns off and the status bar shows why.
- A follow reload or a paste that shifts the lines under a move no longer makes that move change another line. Tiza marks such a move as `stale`, skips it and warns before you quit.
- A follow reload, a paste or `:ref` keeps the step and the file of each annotation. Before, an annotation at step 0 went to step 1. An annotation of the main file at a step that shows another file went to that file. The autosave wrote these changes to the `.tiza`.
- `annotation.update`, MCP `update_annotation` and the `:` commands that change an annotation keep its step and its file. Before, an annotation of the main file at a step that shows another file went to that file. `scene.patch` of an annotation that exists keeps the step that you send, so step 0 stays at step 0. Without `step` or `file`, the annotation keeps its step or its file. `annotation.update` also takes `file`: a linked file, or `null` for the main file. `scene.patch` takes the same names of a linked file as `annotation.add`. In a patch of an annotation that exists, `file: null` is the main file. Tiza refuses the patch if the main file does not have that line.
- When `+`, `-`, `:step ID N` or an agent's update takes an annotation to a step that shows another file, the message says so. For example, the status bar shows `(main.rs, not shown at step 3)`. Before, the message said `press n`, but that step did not draw the annotation. The reply's `hidden` and the MCP tools give the same reason.
- A new `code.patch` move shifts the annotations and later moves that it affects, so an agent's `scene.get` and `scene.put` no longer fail. That round trip keeps file references and writes nothing when nothing changed.
- `:q!` and `Ctrl-C` no longer write the sidecar when they quit. They show on stderr the changes that Tiza did not save.
- If you close the terminal without `:q`, Tiza ends the session as with `Ctrl-C`. It no longer stays at 100 % of a CPU, aborts or leaves its socket.
- Tiza draws control characters in the code as one-cell Control Pictures, for example `␛` and `␀`. A file can no longer send escape sequences to the terminal, break the SVG or make the PNG fail.
- Tiza shows each invisible character in the code, such as a bidi control or a zero-width space, as `⬚` (U+2B1A) in the theme's error color. Tiza draws a joiner inside an emoji sequence, or inside a word of a script that uses joiners, as it is.
- In annotation texts, a control character such as ESC shows as its picture, `␛`. A zero-width character takes no cell, so `tiza check` and `clipped` measure a label as Tiza draws it.
- Tiza refuses a note or a callout text of only zero-width characters, as it refuses one of only spaces.
- Tiza removes a byte order mark at the start of a file each time it reads the file. Before, a referenced file that Tiza read again kept it as an invisible first column.
- Input that is too large or not valid, for example `:export x.png width=99999`, no longer crashes Tiza or leaves the terminal broken. A bug in a socket or MCP request gives the error `internal`, and the document and the TUI stay as they were.
- After Enter, Tiza clears a selection that the `:` command did not use, so a later `:mark` cannot go on old text. When you cancel a command line that a selection opened, Tiza also clears that selection.
- Without a width, `render.text` (MCP `render_text`, `tiza scene text`, `tiza://render`) gives the take at the width of the screen's code area. The text now agrees with `visible`, `clipped` and `check` at that width, which the new `width` field of the reply names.
- Margin callouts and long labels. Documents with none of these cases render as before.
- On a narrow status bar, `· last HH:MM:SSZ` of follow mode gives way first, and `⟳ following` stays. A warning that does not fit whole shows only `!`. Before, the bar cut `! code needs 89 cols` in the middle of a word after a reload.
- A message with a count of one uses the singular: `1 line pasted (1 annotation kept)`, `the file has 1 line` and `1 frame`. Before, it said `1 lines` and `1 annotations`.
  - A margin callout with no room for its block in the margin is a whole block under its line, as in the narrow layout. Before, it was a `┆` label that Tiza cut with `…` up to 103 columns, or that wrapped over the next lines.
  - Two margin callouts on one line do not overlap. A block starts only on the rows of its own line, beside the blocks there when the margin has room. If the margin has no room, the block goes under its line.
  - A margin block can leave fewer than 14 columns beside it. Then the labels of the lines that it covers go under their own line. Before, Tiza did not draw these labels, or drew them beside another line.
  - A label under a line stops one cell before the right rail of its box, with `…`. It also stops one cell before a margin block on its row. `tiza check` counts the label as clipped. Before, the label crossed the rail into the margin, or the block hid its end with no warning.
  - A callout block under its line no longer cuts its text beside short code. Its text wraps at the width of the block, also inside a box. Before, each row lost its end beside code of 11 columns or less, with no warning.
  - A label in the margin does not continue beside a line that draws its labels under it. Before, it continued there and could write over those labels.
  - A margin block does not start on the top row of the box of the next line. Before, it could start there, beside the lines of that box.
  - An empty label (`""`, only spaces or only zero-width characters) is the same as no label. It does not make a lone `·` row, an empty `warning:`, a gap in a box border or an empty callout title. It does not move the other labels of its line under the line.
- Smaller fixes:
  - A box that starts on the cursor line keeps its title row on screen.
  - In presentation mode, a proposal in supervised mode shows the `⏸ N to review` chip.
  - `render --step N` and `:export step=N` with N after the last step give a warning and show the last step. Before, they could show a different take, for example with the spotlight of the last step off.
  - A `.tape` file puts `Output` in quotes, so paths with spaces work in VHS 0.12.
  - Tiza refuses to write a `.tape` when a path has a control character, a newline or a `"`.
  - `tiza --version` no longer shows an old commit in a git worktree, or the commit of an unrelated repository for a crate from crates.io.
  - A `rebind` that names a deleted annotation gives a warning, and the `.tiza` still opens.
  - A paste at step 0 applies the moves again, so annotations on typed code stay.
  - With no terminal for keys, Tiza prints one line about `tiza render` and `tiza check` and exits 1, not a crash report.
  - Tiza no longer loses a terminal resize in the first milliseconds after start.

## [0.0.1] - 2026-10-16

This is the first working demo of Tiza.

### Added
- A terminal UI shows code with syntax highlighting, a line cursor and dark and light themes (`t`). It has steps (`n`/`p`), presentation mode (`P`), undo and redo (`u`/`U`), an annotation list (`L`) and help (`?`).
- Annotations attach to the text of the code, not to screen cells, so they stay in place when you scroll, resize or change the theme. The types are highlights, marks, boxes, arrows along a lane or on one line, and notes in the margin or below the line.
- A `callout` annotation is a paragraph in a border that explains a range of lines, with an optional title, `placement` and `width`. Add it with `:callout`, `C` on a selection or MCP `add_callout`.
- A `spotlight` annotation dims the code outside a range, or the annotations outside a group, for one step. Add it with `:spot` or MCP `add_spotlight`, and use `:until` to keep it until a later step.
- Marks use kinds for any language: `info`, `warn`, `error`, `ok`, `question`, `important`, `read`, `write`, `move`, `new`, `drop`, `scope`, `flow` and `todo`. A mark can also take an inline `style` (glyph, legend, color) or a named style from the document's `styles` (`:style`, MCP `define_style`).
- Each annotation takes a `color`, a palette name or `#rrggbb`, and arrows with no color rotate through a palette. Arrows take `stroke`, `route` and `heads`, and boxes and callouts take `border`, `corners` and `fill`.
- Highlights and marks take an `emphasis`: `fill` (the default tint), `underline` (`▔` under the span), `brackets` (`⌈ ⌉`) or `strike`.
- Boxes, callouts and spotlights can take their range from two text markers: `find_start` and `find_end` in the API, or `:box @'// begin'..@'// end'` in the TUI.
- Step 0 shows only the code, and Tiza adds each new annotation to the step on screen. `:clear` deletes all annotations or the annotations of one step, and `u` restores them.
- A `code.patch` move inserts, replaces or deletes whole lines at its step, and `p` reverts it. With `typing: line|char`, Tiza types the new content in place.
- Annotations shift with the lines that a move changes, and a move's `rebind` tells where an annotation goes when its text is gone. Agents use socket `move.add` or MCP `add_move`, `delete_move` and `list_moves`.
- Follow mode (`--follow`, `:follow on`) reloads the code about 120 ms after the editor saves the file. After a reload or a paste, each annotation stays on its text, and `u` brings back the annotations whose text is gone.
- Vim-style selection: `h`/`l`/`w`/`b`/`e`/`0`/`$` move the column cursor, and `v`, `V` and `i` select text, lines or a word. On a selection, `h`, `m`, `n`, `C`, `b`, `a` and `s` add a highlight, mark, note, callout, box, arrow or spotlight.
- `Tab` and `Shift-Tab` select an annotation in reading order. On it, `Enter` edits the label, `e` sets properties, `x` deletes it, `c` changes the color and `+`/`-` move it to another step.
- Mouse support is off by default, so the terminal keeps its own text selection (`--mouse`, `:mouse on`). A click moves the cursor, a drag selects text or lines, and the wheel scrolls.
- The `:` command line has commands such as `:mark`, `:arrow`, `:box`, `:hl`, `:note`, `:step`, `:theme`, `:w` and `:q`. `Tab` completes commands, kinds, ids and themes, and `↑`/`↓` show the history.
- Tiza reads code from a file, a line range of a file (`a.rs:12-48`), stdin or a bracketed paste. Tiza finds the language from the extension, the file name, a `#!` line or a modeline, unless `--lang` or `:lang` sets it.
- Tiza saves the `.tiza` document (JSON, `schema: 1`) automatically and atomically to a sidecar next to the source file. The document refers to the source file by path, `:embed` puts a copy of the code in it, and `:link` refers to a file.
- `--padding V[,H]` and `:pad V [H]` set the padding around the drawing, and the document keeps it. The default is one blank column at each side.
- `tiza render FILE --out x.svg` or `x.png` exports a step as SVG or PNG, with embedded fonts, so the image is the same everywhere. Without FILE, it exports what the live instance shows, and MCP `export_image` does the same for agents.
- Exports set notes, labels and legends in *Tiza Serif*, a Latin subset of Libron, and the code in JetBrains Mono. `--mono-labels` sets all text in JetBrains Mono.
- `tiza render --out x.txt`, `x.md` or `x.ansi` exports a step as plain text, a fenced Markdown block or 24-bit ANSI. In the TUI, `:export FILE` writes any of the five formats, with `step=`, `width=` and `theme=`.
- `--out x.cast` writes an asciicast v3 file of all steps, and `--out x.tape` writes a VHS tape that runs the real TUI through the steps. `--interval`, `--skip-zero` and `--title` change them.
- Tiza has a local API on a Unix socket (NDJSON), and the CLI commands `tiza scene`, `tiza api` and `tiza render` use it. The socket has 0600 permissions and never listens on TCP.
- `tiza scene put FILE` replaces the live document as one change that `u` undoes. `tiza scene patch JSON` merges partial changes, and merges annotations by id.
- `tiza mcp` is an MCP server with 26 tools that take a text anchor (`find`) or a line and column. It also gives four resources: `tiza://scene`, `tiza://code`, `tiza://render` and `tiza://render.md`.
- In supervised mode (`:supervise on`), each change that an agent asks for waits as a proposal until you press `y` or `n`. Tiza rejects a proposal after 60 s without a decision.
- `tiza --version` shows the git commit of the build. The MCP `status` tool tells an agent when to reconnect after a new build of the binary.
- The status bar shows a `⚠` badge when the terminal is smaller than 100×30 or a code line is clipped. Below about 100 columns, Tiza draws notes under their line, not in the margin.
- `tiza schema document` and `tiza schema api` write the JSON Schema of the document and of the local API. `-` means stdin for `tiza -`, `tiza render -` and `tiza api METHOD -`.
- The examples `examples/ownership.rs.tiza` and `examples/ownership-moves.rs.tiza` show annotations, steps and moves. `docs/agent-guide.md` is a guide for agents, and `docs/zed.md` gives a Zed task that sends the selection to Tiza.
- Each tag has a GitHub Release with binaries for macOS (Apple Silicon and Intel) and Linux x86_64. You can also build from source with `cargo install --git` and Rust 1.88 or later.

### Changed
- CodeDraw is now Tiza: the binary, the crate, the `.tiza` sidecar, `TIZA_SOCKET`, `TIZA_SESSION` and the MCP server `tiza` use the new name. Rename old `.codedraw` files to `.tiza`, because Tiza does not read them.
- All user-facing text (TUI, CLI, MCP, docs and examples) is in English. Commands, JSON fields and the Spanish color aliases (`azul`, `verde`) do not change, and `:theme` still accepts `oscuro` and `claro`.
- A frame with 2,000 lines and 200 annotations takes about 7 ms in a release build, not about 220 ms.

### Fixed
- Follow mode sees saves on macOS also when the path goes through a symlink, such as `/var` and `/private/var`.
- A document with `"code": []` loads as one empty line, and annotations outside the code no longer crash the layout. Tiza names them when it opens the document.
- In the dark theme, syntax colors with less than 4.5:1 contrast get closer to the text color until they pass WCAG AA.
- Margin labels no longer go next to the wrong line when an earlier label wraps. A label keeps its own row and ends with `…` when it has no more space.
- A callout below the code is inside a box only when the box holds its whole range. A callout wider than the box goes after its bottom border.
- Smaller fixes:
  - A horizontal arrow stub that crosses a rail keeps the rail's color at the junction, so it looks like it goes under the rail.
  - A range that is not valid, such as `12-` or `1-2-3`, gives an error, not one line.
  - `:wq` does not quit when the write fails.
  - Tiza replaces control characters in labels, notes and titles with spaces, so each stays on one line.
  - Tiza limits the `step` of a document to its last step and rejects duplicate ids.
  - The line-number gutter keeps every digit of three-digit numbers.
  - `:pad 0 0` removes the padding, and `:pad default` restores it.
  - SVG and PNG sizes no longer gain a pixel from float rounding.
  - On a narrow terminal, the status bar makes its hints shorter and does not hide them.
  - Help and list overlays no longer panic on terminals smaller than 8×4.
