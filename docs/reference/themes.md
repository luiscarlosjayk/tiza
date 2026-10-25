# Themes

This page tells how to pick the colors of Tiza and how to change one color. It covers the built-in themes, the auto theme, the terminal theme, the color tokens and the theme of each export.

## Pick a theme

Tiza has two built-in themes, `dark` and `light`. It also has a `terminal` theme that uses the colors of your terminal.

| To | Do this |
|---|---|
| Change between dark and light | Press `t`, or type `:theme` without a name. |
| Pick a theme | Type `:theme dark`, `:theme light` or `:theme terminal`. |
| Pick a theme when you start Tiza | Add `--theme dark`, `--theme light` or `--theme terminal`. |
| Follow the terminal again | Type `:theme auto`, or start with `--theme auto`. |

When you pick a theme, Tiza stops following the terminal. Tiza keeps the theme in the document, and the next save writes it. Then the next session opens with the same theme. `t` and `--theme` do not save the document. See [Themes and saving](#themes-and-saving).

## The auto theme

A document that names no theme follows the terminal. This is the auto theme. When Tiza starts, it asks the terminal for its background color. A light background gives the `light` theme, and a dark background gives `dark`. The status bar shows `light·auto` or `dark·auto`.

- When the terminal sends no answer for 100 ms, Tiza stops waiting. It shows no message. A document that names no theme then gets `dark`.
- A pseudo-terminal with no terminal program behind it is an example of a terminal that sends no answer.
- When the variable `TIZA_NO_OSC` is set, also to an empty value, Tiza does not send the query at start. A resize still sends it.
- With `--theme auto`, a document that names a theme keeps that theme until the terminal answers.
- `:theme auto` sends no query. It uses the last answer of the terminal. With no answer yet, it keeps the theme on screen until an answer comes.
- Tiza keeps the keys that you type while it waits. For example, `nn` right after `tiza FILE --present` moves two steps.
- Tiza asks again at each resize of the terminal.
- When the theme of your terminal changes with no resize, type `kill -WINCH $(pgrep tiza)` in another shell. Tiza then asks again. [Terminal support](terminals.md) tells what each tmux version does with the query.
- These stop the auto theme: `t`, `:theme NAME`, `--theme NAME`, a document that names a theme, and an agent that sets a fixed theme.
- `:theme auto` starts the auto theme again. Then the document names no theme.

## Use the terminal colors

`:theme terminal` and `--theme terminal` draw Tiza with the palette of your terminal. The background and the text use the default colors of the terminal. The other tokens use the 16 ANSI colors of the terminal. The syntax uses the colors of the terminal too. Then Tiza looks like the rest of your terminal.

- Tiza does not pick the terminal theme by itself. When the terminal answers, the auto theme picks `dark` or `light`.
- The tokens of the terminal theme send no 24-bit color. Use the theme in a terminal that has no 24-bit color.
- The tokens of the config file and of the document apply to the terminal theme too. Tiza sends a token in `#rrggbb`, `#rgb` or `rgb()` form as a 24-bit color. It does the same with an annotation color in `#rrggbb` form.
- The terminal theme picks the tint of a highlight and of the status bar from the answer. A dark terminal gets ANSI color 8, and a light terminal gets ANSI color 7.
- `t` goes from `terminal` to the built-in theme that matches the background of the terminal. With no answer, `t` goes to `dark`.
- An export cannot use the palette of the terminal. See [Exports](#exports).

## Change a color

Each color of a theme has a name: its token. You can give a token a new value in a theme file, in the config file or in the document.

1. Write a TOML file with the tokens that you want to change. For example, `my-theme.toml`:

   ```toml
   accent = "#fab387"
   note = "#f9e2af"
   blue = "#89b4fa"
   ```

2. In Tiza, type `:theme file my-theme.toml`. Or start Tiza with `tiza FILE --theme-file my-theme.toml`.
3. Tiza puts the tokens in the document, in the field `theme_overrides`. With autosave on, Tiza saves them at once. `:export` and `export_image` use them at once. `tiza render` and the next session use them after the save.

- A theme file replaces all the tokens that the document had. It does not add to them.
- Press `u` to undo `:theme file`. Press `U` or Ctrl-R to do it again.
- `--theme-file` writes the tokens in the document when Tiza starts, also with `--no-save`. To keep a document as it is, use `tiza render --theme-file`.
- To use the same tokens in each document, put them in the `[theme]` table of the config file. See [Where the tokens go](#where-the-tokens-go).
- `tiza render FILE --theme-file PATH` uses the file for one export, in place of the tokens of the document. It does not change the document.
- The [Catppuccin Mocha example](../../examples/themes/catppuccin-mocha.toml) is the file to copy. It sets the tokens of Tiza to the published Catppuccin Mocha colors. It passes the legibility check. See [Legibility](#legibility).
- No token sets a color of the syntax, such as the color of a keyword.

## Theme names

| Name | What it shows | Also accepted by `:theme` |
|---|---|---|
| `dark` | The built-in dark theme, with the One Half Dark syntax colors. | `oscuro` |
| `light` | The built-in light theme, with the One Half Light syntax colors. | `claro` |
| `terminal` | The palette of your terminal. | `term`, `ansi` |
| `auto` | `dark` or `light`, from the background of the terminal. | `host` |

- `:theme` without a name, and `:theme toggle`, change the theme as `t` does. Unlike `t`, they save the document when autosave is on.
- `:t` is the short form of `:theme`.
- `--theme` and `tiza render --theme` take only `dark`, `light`, `terminal` and `auto`.
- `theme.set` and `set_theme` take the names and the aliases of the table, except `host`.
- `:export theme=` and `export_image` take only `dark`, `light`, `oscuro` and `claro`.

| Status bar | Meaning |
|---|---|
| `dark·auto` or `light·auto` | Tiza follows the terminal, or keeps this theme until an answer comes. |
| `terminal·auto` | Tiza follows the terminal, but keeps the terminal theme until an answer comes. |
| `dark`, `light` or `terminal` | The theme is fixed. |

Presentation mode hides the status bar.

## Color tokens

The table lists each token that you can set, with its main use.

| Token | What it colors |
|---|---|
| `bg` | The background of the code, also in an image export. |
| `fg` | The default text. |
| `muted` | The secondary text, such as the line numbers and the legends. |
| `dimmed` | The code outside the focus, and the annotations that a spotlight leaves out. |
| `accent` | The selection, the `:` of the command line, and a mark that has no color of its own. |
| `box_line` | A box that has no color of its own. |
| `note` | A note or a callout that has no color of its own. |
| `highlight_bg` | The background of a highlight that has no color, and of the selected row in a list. |
| `status_bg` | The background of the status bar. |
| `status_fg` | The text of the status bar. |
| `cmd_bg` | The background of the command line and of the panels. |
| `error` | The error messages, and the `⬚` sign for an invisible character. |
| `orange`, `blue`, `green`, `purple`, `pink`, `cyan`, `yellow`, `red`, `gray` | The annotation color with that name. |
| `rail`, `box_bg`, `arrow` | Nothing: Tiza accepts these tokens and draws nothing with them. |

- In `dark` and `light`, `orange` is the `accent` color and `gray` is the `muted` color, until you set them.
- An arrow without a color takes the next color of this list: `orange`, `blue`, `green`, `purple`, `cyan`, `pink`, `yellow`. To change the color of these arrows, set these tokens.
- In the terminal theme, `highlight_bg` is also the tint of a selection and of a mark. This is also so in `dark` or `light` when `bg` or the mark color is a color of the terminal palette.
- An export message can name `rail`, `box_bg` or `arrow` when one of them has an ANSI color or `reset`.

## Color values

| Value | Example | Meaning |
|---|---|---|
| `#rrggbb` | `#fab387` | An RGB color. |
| `#rgb` | `#abc` | The short form: `#abc` is `#aabbcc`. |
| `rgb(R, G, B)` | `rgb(250, 179, 135)` | An RGB color. Each number is from 0 to 255. |
| An ANSI name | `red`, `bright-blue` | A color of the terminal palette. |
| `ansiN` | `ansi208` | The color `N` of the terminal palette, from 0 to 255. |
| `reset` | `reset` | The default color of the terminal. |

- An ANSI name, for example `red` or `bright-blue`. It gives a color of the terminal palette. The ANSI names are `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan` and `white`. Add `bright-` before a name for its bright color. `brightblue` with no dash also works.
- `default` and `none` are the same as `reset`.
- A color name of an annotation, such as `orange`, is not a color value. `red`, `blue` and the other names that are also ANSI names give the colors of the terminal.
- Tiza ignores the case and the spaces around a value. A token name must be in lower case: `Accent` is an unknown token.
- An ANSI color and `reset` have no value in an export. See [Exports](#exports).

## Where the tokens go

| File | Where the tokens go |
|---|---|
| The config file | The `[theme]` table, and the subtables `[theme.dark]`, `[theme.light]` and `[theme.terminal]`. |
| A theme file | At the top of the file, with the subtables `[dark]`, `[light]` and `[terminal]`. |
| A theme file with a `[theme]` table | The `[theme]` table, with the subtables `[theme.dark]`, `[theme.light]` and `[theme.terminal]`. |
| The document | The field `theme_overrides`, with the subtables as the objects `dark`, `light` and `terminal`. |

For example, this config file sets a different accent on `dark` and on `light`:

```toml
[theme.dark]
accent = "#a6e3a1"

[theme.light]
accent = "#b7410e"
```

- A subtable applies only when its theme is in use.
- Do not set one token both in a table and in its subtable. Which value wins depends on the name of the token.
- Tiza applies the tokens in this order, and the last one wins: the built-in theme, the config file, the document.
- `:theme NAME` and `t` change the base theme. The tokens stay and apply on the new theme.
- When a theme file has a `[theme]` table, Tiza ignores the keys outside it. It does not report them.

### Wrong tokens and values

- In each table, Tiza ignores an unknown token, an unknown subtable and a color value that it cannot read. It applies the other tokens.
- Tiza does not report a wrong value in the subtable of a theme that is not in use.
- A theme file does not load when Tiza cannot read it, when its TOML is not valid, or when a value is not a string.
- Then `:theme file` shows the error and changes nothing. `--theme-file` shows the error, and Tiza starts without the file. `tiza render` exits with 1.
- In the config file, a value that is not a string makes Tiza ignore the whole file.
- In the `theme_overrides` of a document, a value that is not a string stops the document from opening. Then Tiza writes `tiza: could not open FILE` and exits with 1.

### The config file

Tiza reads the config file when it starts. `tiza render` reads it at each export. Tiza takes the path of the file from the first item of this list that applies:

1. `$TIZA_CONFIG`, when it is set.
2. `$XDG_CONFIG_HOME/tiza/config.toml`, when that variable is set.
3. `~/.config/tiza/config.toml`.

- An empty `$XDG_CONFIG_HOME` counts as not set. A `$TIZA_CONFIG` that is empty or has only spaces also counts as not set.
- When the file at that path does not exist, Tiza uses no config file.
- Tiza reads only the `[theme]` table. It ignores the other keys and tables of the file, and it does not report them.
- When Tiza cannot read or parse the config file, it ignores the file. It writes `tiza: config ignored: PATH` on stderr.

## Messages

| Message | When |
|---|---|
| `theme NAME` | `:theme NAME` or `:theme` changed the theme. |
| `theme auto: following the terminal (NAME now)` | `:theme auto` follows the terminal, and the last answer gave `NAME`. |
| `theme auto: following the terminal (no answer yet; a resize asks again)` | `:theme auto` follows the terminal, but the terminal has not answered. |
| `unknown theme (dark, light, terminal, auto, file PATH; :theme alone toggles)` | `:theme` got a name that it does not know. |
| `theme file PATH: N token(s) applied on NAME` | `:theme file PATH` or `--theme-file PATH` loaded the file. |
| `undone: theme file` | `u` removed the tokens of the last theme file. |
| `redone: theme file` | `U` or Ctrl-R put the tokens of the theme file back. |
| `PATH: KEY: expected a colour string, got TYPE` | A value in a theme file is not a string. |
| `PATH: [NAME] KEY: expected a colour string, got TYPE` | A value in a subtable of a theme file is not a string. |
| `` theme: unknown token `KEY` (roles: …; palette: …) `` | A table has a token that Tiza does not know. |
| `theme: unknown table [NAME] (dark, light, terminal)` | A table has a subtable that Tiza does not know. |
| `theme KEY: VALUE: use #rrggbb, rgb(r, g, b), an ANSI name (red, bright-blue…), ansiN or reset` | A token has a value that Tiza cannot read. |
| `theme KEY: OLD reads at R:1 on the background, pushed to NEW for 4.5:1` | A text token was too faint, and Tiza changed it. |
| `the terminal theme has no palette outside the terminal: exported with NAME` | An export of the terminal theme used `dark` or `light`. |
| `theme tokens set to a terminal colour have no value in an export (KEYS): the NAME theme's are used` | An export replaced the ANSI and `reset` tokens. |
| `theme= must be dark or light` | `:export` got `theme=terminal` or another name. |
| `tiza: config ignored: PATH` | Tiza cannot read or parse the config file. |
| `tiza: could not open FILE` | A value in `theme_overrides` of the document is not a string. |
| `` `theme` must be a string: dark, light or toggle `` | `theme.set` got a `theme` that is not a string. |
| `unknown theme (dark, light, terminal, auto, toggle)` | `theme.set` got a name that it does not know. |
| `unknown theme (dark, light)` | `export_image` got a `theme` that it does not take. |

- In `theme file PATH: N token(s) applied on NAME`, `N` counts the keys at the top of the file. A subtable counts as one key, and an unknown key counts too.
- When Tiza cannot read a theme file, or its TOML is not valid, the message is `PATH:` and the error text.
- `tiza: config ignored: PATH` ends with the error text of the TOML reader or of the system.
- `tiza: could not open FILE` ends with the error text of the JSON reader.
- For a wrong `#` or `rgb()` value, the text after `VALUE:` tells what is wrong.
- `:export` adds an export message after ` — `. `tiza render` writes it on stderr. `export_image` does not show it.

### When Tiza shows the token reports

The token reports are the messages about an unknown token, an unknown table, a wrong value and a faint token.

- The TUI shows them after `:theme`, `:theme NAME`, `:theme toggle`, `:theme file` and `--theme-file`. It adds them after ` — `, separated by `; `.
- The TUI does not show the reports of the config file or of the document at start. Only `--theme-file` shows reports at start.
- The TUI shows no reports after `t`, after `:theme auto` or after a change from an agent.
- `tiza render` writes each report on its own line on stderr, after `tiza: `.
- `export_image` shows no report.

## Exports

An export cannot use the palette of the terminal. Each export picks its theme as this table shows. The Tokens column is for `dark` and `light`.

| Export | Theme | Tokens |
|---|---|---|
| `:export` | The theme on screen, or `theme=dark` or `theme=light`. | The config file and the document. |
| `tiza render` | The `--theme` flag, else the theme of the document, else `dark`. | The config file, and the document or `--theme-file`. |
| `export_image` | Its `theme`, `dark` or `light`, else the theme of the document, else `dark`. | The document only. |

- `:export` exports `light` when the auto theme shows `light`. `tiza render` and `export_image` cannot ask the terminal. They export a document that follows the terminal as `dark`.
- `tiza render --theme auto` uses the theme of the document, else `dark`.
- `tiza render --theme terminal` exports `dark`, and writes the message about the terminal theme on stderr.
- With the terminal theme, `:export` uses the built-in theme that matches the background of the terminal. With no answer, it uses `dark`. `tiza render` and `export_image` always use `dark`.
- An export of the terminal theme uses the plain `dark` or `light` theme. No token applies: not the config file, the document, `--theme-file` or `[theme.terminal]`.
- In an export of `dark` or `light`, a token with an ANSI color or `reset` takes the color of the built-in theme.

## Themes and saving

- With autosave on, these save the document at once: `:theme` with or without a name, `:theme file`, `theme.set`, `set_theme`, and a `scene patch` with `theme`. See [Saving](../concepts/saving.md).
- `t`, `--theme NAME` and `--theme auto` change the theme of the document, but do not save it. The next save writes the change.
- When you quit after `t` with no other change, the document keeps its old theme.
- `--theme-file` saves the document when Tiza starts. It does this also with `--no-save`.
- The document keeps a theme only when the theme is fixed. A document that follows the terminal has no `theme` field.

## Agents and themes

- `set_theme` (MCP) and `theme.set` (socket) take `dark`, `light`, `terminal`, `auto` or `toggle`. They also take the aliases `oscuro`, `claro`, `term` and `ansi`.
- With no `theme`, or an empty one, `theme.set` changes between dark and light as `t` does.
- The reply has the field `auto`. It tells whether the instance follows the terminal.
- A `scene patch` or a `scene put` with `theme` sets a fixed theme, as `:theme NAME` does.
- Under `:supervise`, a `theme.set` waits for the presenter. The presenter can reject it. With no decision for 60 s, it fails.
- `export_image` picks its theme as [Exports](#exports) tells.

## How it works

Tiza asks the terminal with OSC 11 for the background and OSC 10 for the text. It sends them as plain OSC, not wrapped for tmux. A background is dark when 0.299 R + 0.587 G + 0.114 B is less than half of 255.

- Tiza sends the query only with the auto theme or the terminal theme. It sends it at start and at each resize.
- Tiza uses the answer to OSC 10 only to stop waiting early. No color of Tiza comes from it.
- At start, Tiza reads the answer before its input library reads the terminal. It gives the other keys that it read to the editor, in order.
- After start, the answer comes in as key presses. Tiza takes it out of the keys.
- Tiza does not turn on mode 2031, in which the terminal reports a change of its colors. The input library would read that report as key presses.

### Legibility

The built-in themes use RGB colors that stay legible in a 1080p video. CI runs [`scripts/legibility.py`](../../scripts/legibility.py) on step 7 of `examples/ownership.rs.tiza`, in `dark` and in `light`. The script renders a PNG and scales it to 1920×1080. Then it compresses the frame as YouTube does.

- The script measures the text colors of the SVG export against the background.
- It skips a color under 1.5:1 before the compression, because such a color is a tint, not text.
- It skips a color that fills fewer than 40 pixels of the frame.
- Each color that it measures must keep a contrast of 4.5:1 or more. Otherwise the script exits with 1.

To check a theme file, run the script with `--theme-file`:

```sh
python3 scripts/legibility.py examples/ownership.rs.tiza --step 7 --theme-file examples/themes/catppuccin-mocha.toml
```

The script needs Python with Pillow and numpy, and ffmpeg. It runs `target/debug/tiza`, or the binary that `--bin` or `TIZA_BIN` names. Without ffmpeg, it exits with 2.

Tiza also keeps text legible when it applies tokens:

- It checks `fg`, `muted`, `accent`, `note` and `error` against `bg`. Tiza reports each token that it changes.
- A token under 4.5:1 moves toward black when the WCAG relative luminance of `bg` is more than 0.5. Otherwise it moves toward white.
- This rule is not the dark rule of [How it works](#how-it-works). For example, `#a0a0a0` is a light background, but a faint token on it moves toward white.
- On a mid-gray `bg`, such as `#808080`, white reads at less than 4.5:1. Tiza still moves the token toward white, so the token stays under 4.5:1.
- The move also stops at 90 % of the way. Thus a token can stay under 4.5:1 on another `bg` too.
- In these cases, the message still ends with `for 4.5:1`.
- When it draws the code, Tiza moves a syntax color under 4.5:1 toward `fg`.
- Tiza does not check a color of the terminal palette. It checks no token when `bg` is a color of the terminal palette. It moves no syntax color when `bg` or `fg` is a color of the terminal palette.
