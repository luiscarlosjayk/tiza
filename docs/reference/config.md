# Config file

The config file holds your own settings for every session. It gives the values that a session starts with, your personal styles and your theme colors. This page tells where Tiza finds the file, what each key does, which value wins, and what Tiza does with a wrong entry.

## An example

This file sets every key of `[defaults]`, one personal style and one color for each theme:

```toml
[defaults]
pad = "1 2"
numbers = false
mouse = true
autosave = false
layout = "stable"
width = 120
effect_fps = 30

[styles.decision]
glyph = "◆"
legend = "decision"
color = "purple"

[theme.dark]
accent = "#a6e3a1"

[theme.light]
accent = "#b7410e"
```

Put the file at `~/.config/tiza/config.toml`. Then run [`tiza config`](#tiza-config) to see what Tiza takes from it.

A config file is optional. Each key is optional too. A key that the file does not set keeps its built-in value.

## Where Tiza finds the file

Tiza takes the path of the file from the first item of this list that applies:

1. `$TIZA_CONFIG`, when it is set.
2. `$XDG_CONFIG_HOME/tiza/config.toml`, when that variable is set.
3. `~/.config/tiza/config.toml`.

- An empty `TIZA_CONFIG`, `XDG_CONFIG_HOME` or `HOME` counts as not set. A `TIZA_CONFIG` with only spaces also counts as not set.
- Without `HOME` and without `XDG_CONFIG_HOME`, Tiza reads no config file.
- When no file is at the path, Tiza uses the built-in values.
- When `TIZA_CONFIG` names a file that does not exist, Tiza also writes a warning.
- When Tiza cannot read the file, it ignores the file and writes `tiza: config ignored: PATH`.
- When a part of the file has an error, Tiza ignores only that part. See [Warnings](#warnings).

## When Tiza reads the file

| Program | When it reads the file | What it takes |
|---|---|---|
| The TUI | When it starts. | All the tables. |
| `tiza render` | At each export. | `[theme]`, `numbers` and `width`. |
| `tiza check` | At each check. | `numbers` and `width`. |
| `tiza config` | Each time that you run it. | All the tables, to show them. |
| `tiza mcp` | At each `export_image`. | `[theme]`. |

- The TUI reads the file only when it starts. A change to the file applies at the next start.
- `tiza config` only shows the file. It changes nothing.
- Without a `FILE` argument, `tiza render` exports the live instance. Then it takes the width and `numbers` from the instance, and only `[theme]` from the config file.
- `tiza mcp` reads its own config file: the file that its environment gives, not the file of the instance. `export_image` takes `numbers` from the instance.
- `tiza check` uses `numbers` for the gutter when the document does not set it.
- The `check` method of the socket and the `check` MCP tool use the `width` of the config file of the instance.

## `[defaults]`

The values that a session starts with.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `pad` | string or integer | `"0 1"` | The blank cells around the code on the screen: `"V H"` gives `V` rows and `H` columns. |
| `numbers` | `true` or `false` | `true` | The line-number gutter. |
| `mouse` | `true` or `false` | `false` | Mouse capture. |
| `autosave` | `true` or `false` | `true` | Autosave of the document. |
| `layout` | `"stable"` or `"dynamic"` | `"dynamic"` | The layout of normal mode. |
| `width` | integer | `100` | The width of `tiza render` and `tiza check`, from 20 to 400 columns. |
| `effect_fps` | `30` or `60` | `60` | The frames each second of an enter effect on the screen. |

- `pad = "V"` sets `H` to two times `V`. A number, for example `pad = 2`, does the same. `pad = "default"` gives the built-in value.
- `numbers`, `mouse` and `autosave` also take the strings `"on"`, `"off"`, `"yes"` and `"no"`.
- `pad` changes only the screen. An export does not draw the padding.
- In presentation mode, the layout is always stable.
- `effect_fps` changes only the screen. A GIF, a WebP or a video plays an effect at 25 frames each second.

## Which value wins

A flag, a [`:` command](commands.md) and the document win over the config file. The config file wins over the built-in value. The config file never writes a value into the document.

| Key | Flag | Saved in the document | `:` command |
|---|---|---|---|
| `pad` | `--padding V,H`, for the session | Yes, as `padding` | `:pad V H`, `:pad default` |
| `numbers` | `--numbers` and `--no-numbers` of `tiza render`, for one export | Yes, as `numbers` | `:numbers on`, `:numbers off` |
| `mouse` | `--mouse` | No | `:mouse on`, `:mouse off` |
| `autosave` | `--no-save` | No | `:autosave on`, `:autosave off` |
| `layout` | No flag | No | `:layout stable`, `:layout dynamic` |
| `width` | `--width N` of `tiza render` and `tiza check` | No | No command |
| `effect_fps` | No flag | No | No command |

- **`pad`:** `--padding` wins over the padding of the document for the session. `:pad V H` saves the padding in the document. `:pad default` removes it from the document. Then the padding of the config file applies. When the file has no `pad`, the built-in value applies.
- **`numbers`:** When the document has a `numbers` field, it wins over the config file. `:numbers on` and `:numbers off` always write the field, also when the gutter is already in that state. Then the document looks the same on a computer without your config file. To use the config file again, remove the field from the `.tiza` file.
- **`mouse`:** Mouse capture is on at start when you give `--mouse` or the file sets `mouse = true`. The file cannot turn off `--mouse`. `:mouse` changes it for the session.
- **`autosave`:** `--no-save` wins over `autosave = true`. `:autosave` changes it for the session. `:autosave on` also ends no-save mode.
- **`layout`:** `:layout` changes it for the session.
- **`width`:** For `tiza render` and `tiza check`, `--width` wins, then the file, then 100. For the `check` method and the `check` MCP tool, their `width` argument wins, then the file of the instance, then 100.
- **`effect_fps`:** Only the file sets it, for every session.

## `[styles]`

Your personal styles. Each `[styles.NAME]` table is one style, with the fields of `:style`. Use a personal style as a kind: `:mark NAME`, or `kind` of an agent's `add_mark`.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `glyph` | string | Required | The glyph of the mark. It cannot be empty. |
| `legend` | string | No default | The legend next to the glyph. |
| `color` | string | No default | The color of the mark: a color name or `#rrggbb`. |

- The color names and their aliases are in [Color names](document-format.md#color-names).
- The name of a style cannot be empty and cannot have spaces.
- The name of a style cannot be a built-in kind or an alias, for example `warn` or `todo`. Personal styles do not change the built-in kinds. The kinds and the aliases are in [Kinds](document-format.md#kinds).

### When a personal style goes into the document

The first time that a mark uses a personal style, Tiza copies the style into the document. Then the `.tiza` file looks the same on a computer without your config file.

- Tiza copies the style when a mark of that kind comes into the document: `:mark`, an agent's `add_mark`, a change of a mark to that kind, `scene.put` or `scene.patch`.
- When the document has a style with the same name, the style of the document wins. Tiza does not copy the personal style.
- `:style NAME` with `glyph=`, `legend=` or `color=` on a personal style changes a copy of it in the document. The config file does not change.
- `:mark` shows `ID: LEGEND at LN (style NAME from config.toml)` when it copies a style.

## `[theme]`

Your theme colors. The `[theme]` table sets color tokens for every theme. The subtables `[theme.dark]`, `[theme.light]` and `[theme.terminal]` set them for one theme. The tokens and their values are in [Themes](themes.md#where-the-tokens-go).

- The document wins over the config file. The config file wins over the built-in theme.
- When a text color of `[theme]` is too faint on the background, Tiza changes it to a color with more contrast. `tiza config` lists each of these changes as a warning, with its theme.

## `tiza config`

`tiza config` shows the config file in use, each value of `[defaults]` with its source, the personal styles, the `[theme]` tokens and the warnings. It changes nothing.

With the file of [An example](#an-example), it shows:

```text
file: /home/dev/.config/tiza/config.toml
defaults:
  pad        1 2      config
  numbers    off      config
  mouse      on       config
  autosave   off      config
  layout     stable   config
  width      120      config
  effect_fps 30       config
styles (config; a document style with the same name wins):
  decision ◆  decision  @purple
theme (config): [dark] accent = #a6e3a1, [light] accent = #b7410e
warnings: none
```

- The `file:` line gives the path of the file. Without a file, it gives `file: none (no file at PATH): every value is built-in`. Without `HOME` and without `XDG_CONFIG_HOME`, it gives `file: none (no home folder): every value is built-in`.
- In `defaults:`, the source of each value is `config` or `built-in`. A flag, a `:` command and the document can still win over it. See [Which value wins](#which-value-wins).
- The warnings are the lines that Tiza writes on stderr, without `tiza: config: `.
- `tiza config` exits with 0, also when the file has warnings. It exits with 1 when Tiza cannot read the file. Then it writes `tiza: config: PATH: ERROR`.

## Warnings

Tiza never stops because of the config file. When it cannot use an entry, it ignores that entry and writes a warning. The other entries apply.

- The TUI, `tiza render` and `tiza check` write each warning on stderr. Each line starts with `tiza: config: `, and then gives the path and the line of the entry.
- The TUI writes these lines before it takes the screen. You see them after you quit.
- At start, the status bar shows the first warning, then `(N warnings: tiza config)`. That warning has the form `NAME:LINE: KEY: …`: the file name without its folder, and the key without its table.
- `tiza config` lists all the warnings. See [`tiza config`](#tiza-config).
- The JSON reply of `export_image` has a `warning` field. It says what the config file left out, or that Tiza cannot read the config file.

| Message | When |
|---|---|
| `tiza: config: PATH:LINE: [defaults] KEY: VALUE is not valid: HINT` | A value of `[defaults]` is wrong. `HINT` gives the values that the key takes. |
| `tiza: config: PATH:LINE: [defaults] KEY: unknown key (keys: KEYS)` | `[defaults]` has a key that Tiza does not know. |
| `tiza: config: PATH:LINE: KEY: unknown key (Tiza reads the tables [theme], [defaults] and [styles])` | The file has a key or a table that Tiza does not read. |
| `tiza: config: PATH:LINE: KEY: expected a table, got TYPE` | `theme`, `defaults` or `styles` is not a table. |
| `tiza: config: PATH:LINE: [styles] NAME: a style needs a glyph` | A style has no `glyph`. Tiza ignores the style. |
| `tiza: config: PATH:LINE: [styles] NAME: a built-in kind has this name; personal styles cannot change built-in kinds` | A style has the name of a built-in kind or of an alias. Tiza ignores the style. |
| `tiza: config: PATH:LINE: [styles] "": a style needs a name` | A style has an empty name. Tiza ignores the style. |
| `tiza: config: PATH:LINE: [styles] "NAME": a style name has no spaces` | A style name has a space. Tiza ignores the style. |
| `tiza: config: PATH:LINE: [styles] NAME: expected a table with glyph, legend and color, got TYPE` | A style is not a table. Tiza ignores the style. |
| `tiza: config: PATH:LINE: [styles.NAME] FIELD: expected a string, got TYPE` | A field of a style is not a string. Tiza ignores the field. |
| `tiza: config: PATH:LINE: [styles.NAME] glyph: a glyph cannot be empty` | `glyph` is an empty string. Tiza ignores the field. |
| `tiza: config: PATH:LINE: [styles.NAME] color: "VALUE" is not valid: use COLORS or #rrggbb` | `color` is not a color name or `#rrggbb`. Tiza ignores the field. |
| `tiza: config: PATH:LINE: [styles.NAME] FIELD: unknown field (fields: FIELDS)` | A style has a field that Tiza does not know. Tiza ignores the field. |
| `tiza: config: PATH:LINE: [TABLE] KEY: duplicate key, the first value is used` | A key is in a table two times. |
| `tiza: config: PATH:LINE: [TABLE]: duplicate table: Tiza reads the entries of both` | A table is in the file two times. |
| `tiza: config: PATH:LINE: N more keys that are in the file two times: Tiza uses the first value of each` | More than 200 keys are in the file two times. |
| `tiza: config: PATH:LINE: this line is not valid UTF-8: Tiza ignores its entries` | A line has bytes that are not UTF-8. |
| `tiza: config: PATH:LINE: ERROR: the table header has an error` | A table header does not parse. Tiza ignores the entries under it, up to the next header. |
| `tiza: config: PATH:LINE: HEADER: a table or a value above it has this name` | A table header has the name of a key above it, or of a table of the other kind, for example `[x]` and `[[x]]`. Tiza ignores the entries under it, up to the next header. |
| `tiza: config: PATH:LINE: this line is not valid UTF-8: the table header has an error` | A table header has bytes that are not UTF-8. Tiza ignores the entries under it, up to the next header. |
| `tiza: config: PATH:LINE: [TABLE] KEY: ERROR` | A line does not parse. |
| `tiza: config: PATH:LINE: too many errors` | The file has more than 200 errors. Tiza ignores the rest of the file. |
| `tiza: config: PATH: TIZA_CONFIG names this file, but it does not exist` | `TIZA_CONFIG` names a file that is not there. |
| `tiza: config ignored: PATH` | Tiza cannot read the file. It uses no config file. |

- `KEYS` lists the keys of `[defaults]`. `FIELDS` lists the fields of a style. `COLORS` lists the color names.
- After 200 keys that are in the file two times, one warning gives the number of the other ones.
- `ERROR` is the text of the TOML reader. `tiza: config ignored: PATH` ends with the error text of the system.
- Some warnings make Tiza ignore more lines than the line of the entry. For example, Tiza ignores a table that it does not read, and all the entries of that table. Then the warning ends with `, so Tiza ignores lines A-B`. Any warning of the table can have this end. The list of lines can also be `line N` or `lines A-B and C`.
- The warnings about `[theme]` are in [Themes](themes.md#messages).

For example, a file with six errors gives these warnings:

```text
tiza: config: /home/dev/.config/tiza/config.toml:2: [defaults] width: 5 is not valid: use a number of columns between 20 and 400
tiza: config: /home/dev/.config/tiza/config.toml:3: [defaults] colour: unknown key (keys: pad, numbers, mouse, autosave, layout, width, effect_fps)
tiza: config: /home/dev/.config/tiza/config.toml:5: [defaults] numbers: duplicate key, the first value is used
tiza: config: /home/dev/.config/tiza/config.toml:7: [styles] risk: a style needs a glyph, so Tiza ignores lines 7-9
tiza: config: /home/dev/.config/tiza/config.toml:11: unclosed table, expected `]`: the table header has an error, so Tiza ignores lines 11-12
tiza: config: /home/dev/.config/tiza/config.toml:16: this line is not valid UTF-8: Tiza ignores its entries
```

When a file has the same key 203 times, Tiza writes a warning for each of the first 200 copies. Then it writes:

```text
tiza: config: /home/dev/.config/tiza/config.toml:203: 2 more keys that are in the file two times: Tiza uses the first value of each
```

A folder at the path of the file gives:

```text
tiza: config ignored: /home/dev/.config/tiza/config.toml: Is a directory (os error 21)
```
