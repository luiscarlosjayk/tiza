# View commands

Use these commands to change how Tiza shows the code. Tiza saves the theme, the title, the line numbers, the padding and the margin in the document. Exports use the theme, the line numbers and the margin. A Markdown export shows the title above the code. An SVG export writes it as the SVG title, which it does not draw.

Each image on this page is a frame of a `tiza render` export. The code is `examples/ownership.rs`. A frame shows only what an export draws, so some commands have no frame.

## `:focus`

Dims all the lines except a range. `:focus` alone clears the focus.

```
:focus A-B
:focus
```

For example, `:focus 6-8` dims all the lines except 6 to 8.

- `:focus` takes lines that are in the code. When `B` is after the last line, Tiza stops the focus at the last line.
- A range can also be `A..B` or `A:B`. Write it without spaces.
- `Esc` in normal mode also clears the focus. `h` on a line selection sets it.
- Tiza does not save the focus. `:export` shows it, but `tiza render` does not. To dim lines at one step, use [`:spot`](annotate.md#spot).

## `:theme`

Sets the theme, or changes between the dark and the light theme.

```
:theme NAME
:theme
:theme auto
:theme file PATH
```

For example, set the light theme:

<!-- frame id=theme-light code=examples/ownership.rs:1-3 theme=doc -->
```
:theme light
:hl 3:takes
```

<img src="../../images/commands/theme-light.webp" width="364" alt="Lines 1 to 3 on a light background, with a highlight on takes.">

### Notes on `:theme`

- `:theme` alone does the same as `t`.
- `:theme auto` follows the theme of the terminal again. The document then names no theme. See [The auto theme](../themes.md#the-auto-theme).
- `:theme file PATH` loads the theme tokens from a TOML file.
- The names of the themes are in [Theme names](../themes.md#theme-names).

## `:lang`

Sets the language of the code, finds it again, or shows it.

```
:lang LANG
:lang auto
:lang
```

For example, `:lang py` shows the code as Python.

- `LANG` is a language id, for example `rs`, `ts` or `py`.
- `:lang auto` finds the language again. `:lang` alone shows the language.
- How Tiza finds the language is in [How Tiza finds the language](../cli.md#how-tiza-finds-the-language).

## `:title`

Sets the title of the document. `:title` alone removes the title.

```
:title TEXT
:title
```

For example, `:title Ownership`.

## `:numbers`

Shows or hides the line numbers.

```
:numbers on
:numbers off
```

For example, hide the line numbers:

<!-- frame id=numbers-off code=examples/ownership.rs:1-3 -->
```
:numbers off
```

<img src="../../images/commands/numbers-off.webp" width="341" alt="Lines 1 to 3 with no line numbers.">

### Notes on `:numbers`

- `:numbers` alone shows the current value.
- `tiza render --numbers` and `--no-numbers` change the line numbers for one export.

## `:pad`

Sets the blank cells around the code and its annotations: `V` rows and `H` columns.

```
:pad V [H]
:pad default
```

For example, `:pad 0 0` removes the padding.

- `:pad default` sets the default padding again.
- `:pad V` sets `H` to two times `V`.
- `:pad` also takes `V,H` with a comma. Tiza ignores the values after `H`.
- `:pad` alone shows the current value.

## `:margin`

Sets the largest width of the right margin, from 12 to 200 cells. Tiza puts notes, labels and legends in the right margin when it has room, and under the line when it has no room. In the right margin, Tiza cuts a note, a label or a legend that is longer than the margin, and ends it with `…`.

```
:margin N
:margin auto
```

For example, `:margin 40` limits the right margin to 40 cells.

- `:margin auto` removes the limit. The right margin then takes the width that the code leaves.
- `:margin` alone shows the current value.
- `tiza render --margin-cols` changes the margin for one export.

## `:layout`

Sets the layout of normal mode.

```
:layout stable
:layout dynamic
```

- `:layout stable` uses the take while you edit. The status bar shows `⊞ stable`.
- `:layout dynamic` lets the layout grow as the steps add arrows.
- Presentation mode and exports always use the take.
- `:layout` alone shows the current value.

## `:mouse`

Turns mouse capture on or off, or shows its state.

```
:mouse on
:mouse off
:mouse
```

The mouse is in [Use the mouse](../../guides/present.md).

## `:effects`

Experimental. Sets the enter effects of this session, or shows them.

```
:effects on
:effects off
:effects draw
:effects dim
:effects draw,dim
:effects
```

- The effects play when the step goes forward. See [Enter effects](../document-format.md#enter-effects-experimental).
- `:effects` does not change the document.
