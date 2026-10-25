# `:` commands

This page lists the commands that you type after `:` in the TUI. For the keys, see [Keys](keys.md).

Press `:` to open the command line. Type a command and press `Enter`. Press `Tab` to complete the name of a command. The keys of the command line are in [The command line](keys.md#the-command-line).

For example, open `examples/ownership.rs` and type these commands:

```
:mark move 3:takes the value moves
:hl 7:&t shared borrow
:arrow 2:s 3:s) the value moves
:box 15-17 takes owns s
:note 4 s no longer owns it
```

Each command adds one annotation at the step on screen. The status bar shows its id, for example `a1: move at L3`.

## Arguments

The commands use these arguments:

| Argument | What it means |
|---|---|
| `L` | A line number, or `.` for the cursor line. |
| `A-B` | The lines from `A` to `B`. `A` alone is one line. |
| `L:TEXT` | The first `TEXT` on line `L`. |
| `L:TEXT/N` | The `N`th `TEXT` on line `L`. |
| `L:` | The whole line `L`. |
| `.:TEXT` | The first `TEXT` on the cursor line. |
| `@'START'..@'END'` | The lines from the first `START` in the code to the first `END` after it. |
| `ID` | The id of an annotation, for example `a3`. |
| `N` | A number, for example a step number. |
| `KEY=VALUE` | A property of the annotation, for example `stroke=dashed`. |
| `[…]` | A part that you can leave out. |

- `TEXT` in `L:TEXT` is one word. It ends at the first space. The next word starts the next argument. Quotes do not change this.
- `L:TEXT` must find the text on line `L`. When the text is on another line, the message gives the correct line.
- `L:TEXT` looks only in the code. It does not look in the notes and the labels.
- In `:box`, `:callout`, `:spot` and `:focus`, a range can also be `A..B` or `A:B`. Write it without spaces. When `A` is after `B`, Tiza puts them in order.
- `:link` and `:ref` accept `A-B`, `A..B` or `A` after the path, and do not put the lines in order. See [Steps and files](#steps-and-files).
- `:box .` and `:callout .` use the cursor line as the range. `:spot .` does not: it reads `.` as an id.
- Put a marker in `'` or `"` quotes when it has a space. Without a space, the quotes are optional, for example `@begin..@end`.
- Tiza keeps the two markers in the document. It uses them to find the range again after the code changes.
- Put a value with spaces in `"` quotes, for example `legend="network I/O"`.
- Put the `KEY=VALUE` properties at the end of the command, after the label.

## Annotate

The text of an anchor ends at the first space. For example, `:hl 2:let s two words` highlights `let`, and its label is `s two words`. To mark a text that has a space, select it with `v`, then press `h` for a highlight or `m` for a mark. See [Annotate a selection](keys.md#annotate-a-selection). You can also use `L:` for the whole line.

| Command | What it does |
|---|---|
| `:mark KIND L:TEXT [LABEL]` | Adds a mark on the text. |
| `:hl L:TEXT [LABEL]` | Adds a highlight on the text. |
| `:arrow L:TEXT L:TEXT [LABEL]` | Adds an arrow from the first end to the second end. |
| `:box A-B [LABEL]` | Adds a box around the lines. |
| `:note L TEXT` | Adds a note on the line. |
| `:callout A[-B] [@PLACE] TEXT` | Adds a callout, a paragraph of text, for the lines. |
| `:spot A-B [ID…]` | Adds a spotlight: the step dims the code outside the lines and the annotations that you name. |
| `:spot ID…` | Adds a spotlight on the annotations. |
| `:until ID N` | Keeps the spotlight `ID` active up to step `N`. |
| `:style NAME glyph=G legend="TEXT" color=C` | Defines a mark style, or changes it. |
| `:style NAME` | Removes the style. When `NAME` is not a style, Tiza shows the usage. |
| `:style` | Shows the styles and the built-in kinds. |

A new annotation goes to the step on screen. See [Annotations](document-format.md#annotations).

### Options of `:mark` and `:hl`

- `KIND` is a built-in kind, an alias, the name of a style or a glyph. The kinds and the aliases are in [Kinds](document-format.md#kinds).
- A glyph is one or two columns wide, and has no ASCII letter or digit. For example, `:mark 🔥 7:len hot path` makes a style for this mark only. The label is then the legend.
- `KIND@COLOR` sets the color of the mark, for example `warn@red`. The colors are in [Color names](document-format.md#color-names).
- `emphasis=E` sets how Tiza draws the span: `fill` (the default), `underline`, `brackets` or `strike`. `:hl` also takes `style=E`.
- After `m` on a text selection, `:mark` uses the selected text. Then type only the kind and the label.

For example:

```
:style io glyph=⇄ legend="network I/O" color=cyan
:mark io 2:String
:hl 7:&t emphasis=underline
```

### Options of `:arrow`

- Each end is `L:TEXT` or `.:TEXT`. The end is the first character of the text.
- An end can also be `L` or `L:`, the start of line `L`. It can also be `TEXT` alone, the first `TEXT` in the code.
- `stroke=S` sets the line: `solid`, `dashed`, `dotted`, `thick` or `double`.
- `route=R` sets the corners of the line: `rounded` or `sharp`.
- `heads=START,END` sets the two heads. `heads=END` sets the end head and no start head. A head is `none`, `arrow`, `dot`, `bar` or `open`.
- Without `heads`, an arrow has no start head and an `arrow` end head.
- `start=H` and `end=H` set one head each.

For example:

```
:arrow 7:&t 19:s: borrow stroke=dashed heads=dot,arrow
```

### Options of `:box` and `:callout`

- `border=S` sets the border: `solid`, `dashed`, `dotted`, `thick` or `double`.
- `corners=R` sets the corners: `rounded` or `sharp`.
- `fill=yes` tints the inside, and `fill=no` does not. A box has a tint by default. A callout has none.
- `@PLACE` puts the callout in the right margin (`@margin`, the default), under the lines (`@below`) or where there is room (`@auto`).
- `title="TEXT"` gives the callout a title. Put it before the text of the callout.
- A callout and a note need text.

For example:

```
:box 6-8 shared border=double corners=sharp fill=no
:box @'fn len'..@'s.len()' len borrows
:callout 6-8 @below title="Borrow" len reads t and gives it back.
```

### Options of `:spot` and `:until`

- In `:spot`, a line range and the ids can come in any order. A marker range must come first. A word that starts with a digit is a line range. When you give two line ranges, Tiza uses the last one.
- `N` cannot be before the step of the spotlight.
- `:spot` also takes a marker range, for example `:spot @'fn len'..@'}'`.

## Change an annotation

| Command | What it does |
|---|---|
| `:label ID TEXT` | Changes the label. |
| `:color ID COLOR` | Changes the color. |
| `:set ID KEY=VALUE…` | Changes one or more properties. |
| `:step ID N` | Moves the annotation to step `N`. |
| `:del ID…` | Deletes the annotations. |
| `:clear` | Deletes all the annotations. |
| `:clear N` | Deletes the annotations of step `N`. |
| `:clear .` | Deletes the annotations of the step on screen. |

- `:label` changes the text of a note and the title of a callout. It does not change a spotlight.
- `COLOR` is a color name, `#rrggbb` or `auto`. `auto` gives the default color back.
- `:color ID` without a color sets `auto`.
- `:step ID N` takes a step from 1 to the step after the last one.
- `:del` deletes the ids in order. It stops at the first id that does not exist, and keeps the deletes before it.
- `u` undoes `:del` and `:clear`.

`:set` takes these properties:

| Property | Annotations |
|---|---|
| `label`, `color` and `step` | All the annotations. |
| `stroke`, `route`, `heads`, `start` and `end` | Arrows. |
| `border`, `corners` and `fill` | Boxes and callouts. |
| `emphasis` | Highlights and marks. |

- `label` also has the names `text` and `title`.
- On a highlight, `emphasis` also has the name `style`.

The values are the same as in [Annotate](#annotate). For example:

```
:set a3 stroke=thick end=bar
```

The fields are in [Document format](document-format.md#annotations).

## Steps and files

| Command | What it does |
|---|---|
| `:step N` | Goes to step `N`. |
| `:next` | Goes to the next step. |
| `:prev` | Goes to the previous step. |
| `:goto ID` | Goes to a named step, or to a step number. Tiza applies the steps from step 0 up to that step. |
| `:link PATH[:A-B]` | At step 1 or later, shows another file from this step on. |
| `:ref PATH[:A-B]` | Makes the document read its code from the file. |
| `:link` or `:ref` | Makes the document read its code from the source file again. |
| `:embed` | Copies the code into the document. |
| `:follow on` | Starts follow mode. |
| `:follow off` | Stops follow mode. |
| `:follow` | Shows the state of follow mode. |

- `:step N` can go to the step after the last step. That step is empty, and new annotations go there.
- When a move does not apply, `:step N`, `:next` and `:prev` show the warning of the move in place of `step N/M`.
- `:goto` is in [Go to a named step](document-format.md#go-to-a-named-step).
- `:link`, `:ref` and `:embed` are in [The document and its code](document-format.md#the-document-and-its-code). At step 0, `:link PATH` does the same as `:ref PATH`.
- At step 1 or later, `:link` refuses a range that starts at 0 or that has `A` after `B`. It also refuses a range of digits, `-` and `.` that is not `A`, `A-B` or `A..B`.
- At step 0, and with `:ref`, a range that is not correct becomes part of the file name. Then Tiza cannot read the file.
- Follow mode is in [Follow the editor](../guides/follow-the-editor.md).

## View

| Command | What it does |
|---|---|
| `:focus A-B` | Dims all the lines except `A` to `B`. |
| `:focus` | Clears the focus. |
| `:theme NAME` | Sets the theme. See [Theme names](themes.md#theme-names). |
| `:theme` | Changes between the dark and the light theme, as `t` does. |
| `:theme file PATH` | Loads the theme tokens from a TOML file. |
| `:lang LANG` | Sets the language of the code. |
| `:lang auto` | Finds the language again. |
| `:lang` | Shows the language. |
| `:title TEXT` | Sets the title of the document. |
| `:title` | Removes the title. |
| `:numbers on` | Shows the line numbers. |
| `:numbers off` | Hides the line numbers. |
| `:pad V [H]` | Sets the blank cells around the code and its annotations: `V` rows and `H` columns. |
| `:pad default` | Sets the default padding again. |
| `:margin N` | Sets the largest right margin, from 12 to 200 cells. |
| `:margin auto` | Removes the limit of the right margin. |
| `:layout stable` | Uses the take while you edit. |
| `:layout dynamic` | Lets the layout grow as the steps add arrows. |
| `:mouse on` | Turns mouse capture on. |
| `:mouse off` | Turns mouse capture off. |
| `:mouse` | Shows the state of mouse capture. |

- `:focus` takes lines that are in the code. When `B` is after the last line, Tiza stops the focus at the last line.
- `Esc` in normal mode also clears the focus. `h` on a line selection sets it.
- The themes are in [Themes](themes.md).
- How Tiza finds the language is in [How Tiza finds the language](cli.md#how-tiza-finds-the-language).
- `:pad V` sets `H` to two times `V`. `:pad 0 0` removes the padding.
- `:pad` also takes `V,H` with a comma. Tiza ignores the values after `H`.
- Tiza saves the title, the line numbers, the padding and the margin in the document.
- Exports also use the line numbers and the margin of the document. `tiza render --numbers`, `--no-numbers` and `--margin-cols` change them for one export.
- The margin is where notes, labels and legends wrap.
- Presentation mode and exports always use the take. `:layout` changes only normal mode.
- With `:layout stable`, the status bar shows `⊞ stable`.
- `:numbers`, `:pad`, `:margin` and `:layout` without a value show the current value.
- The mouse is in [Use the mouse](../guides/present.md).

## Export and agents

| Command | What it does |
|---|---|
| `:export [PATH] [step=N] [width=N] [theme=dark\|light]` | Writes a step to a text, ANSI, Markdown, SVG or PNG file. |
| `:supervise on` | Starts supervised mode. |
| `:supervise off` | Stops supervised mode. |
| `:supervise` | Shows the state of supervised mode and the number of changes that wait. |
| `:review` | Shows the changes of the agents that wait. |

- The options and the messages of `:export` are in [`:export`](exports.md#export).
- In supervised mode, each change of an agent waits for your decision.
- Supervised mode is in [Supervise an agent](../guides/supervise-an-agent.md). The keys of the review are in [Agent review](keys.md#agent-review).

## Save and quit

| Command | What it does |
|---|---|
| `:w` or `:w!` | Saves to the open document. |
| `:w PATH` | Writes a copy to `PATH`. |
| `:w! PATH` | Writes the copy over an existing file. |
| `:saveas PATH` | Saves to `PATH` and makes it the open document. |
| `:saveas! PATH` | Does the same over an existing file. |
| `:wq` | Saves, then quits. |
| `:wq PATH` | Writes a copy to `PATH`, then quits. |
| `:wq!` | Saves, then quits, also over stale moves. |
| `:wq! PATH` | Writes the copy over an existing file, then quits, also over stale moves. |
| `:q` | Quits. |
| `:q!` | Quits without saving. |
| `:autosave on`, `:autosave off` or `:autosave` | Starts autosave, stops it or shows its state. |

- `:w PATH`, `:saveas PATH` and `:wq PATH` refuse an existing file. The message is `PATH exists (add ! to overwrite)`.

- What each command saves, and when `:q` refuses, is in [Saving](../concepts/saving.md).

## Panels and undo

| Command | What it does |
|---|---|
| `:list` | Shows the annotation list, as `L` does. |
| `:present` | Starts or stops presentation mode, as `P` does. |
| `:help` | Shows the help, as `?` does. |
| `:undo` | Undoes the last change, as `u` does. |
| `:redo` | Redoes the change that you undid, as `U` does. |

## Short names

Some commands also have a short name or another name:

| Name | Command |
|---|---|
| `:m` | `:mark` |
| `:highlight` | `:hl` |
| `:ar` | `:arrow` |
| `:n` | `:note` |
| `:co` | `:callout` |
| `:spotlight` | `:spot` |
| `:kinds` | `:style` |
| `:col` | `:color` |
| `:delete`, `:rm` | `:del` |
| `:s` | `:step` |
| `:f` | `:focus` |
| `:t` | `:theme` |
| `:nums` | `:numbers` |
| `:padding` | `:pad` |
| `:supervised` | `:supervise` |
| `:ls` | `:list` |
| `:p` | `:present` |
| `:h`, `:?` | `:help` |
| `:u` | `:undo` |
| `:write`, `:write!` | `:w`, `:w!` |
| `:quit`, `:quit!` | `:q`, `:q!` |
| `:sav`, `:sav!` | `:saveas`, `:saveas!` |

`:n` is `:note`, not `:next`. `:p` is `:present`, not `:prev`.

## Values with other names

Some values also have another word:

| Command | Also accepted |
|---|---|
| `:follow on` | `:follow yes` or `:follow start` |
| `:follow off` | `:follow no` or `:follow stop` |
| `:autosave on` | `:autosave yes` |
| `:autosave off` | `:autosave no` |
| `:numbers on` | `:numbers yes` |
| `:numbers off` | `:numbers no` |
| `:margin auto` | `:margin off` or `:margin none` |
| `:pad default` | `:pad reset` |
| `:layout stable` | `:layout take` |
| `:layout dynamic` | `:layout auto` |
| `:theme` | `:theme toggle` |

The names of the themes are in [Theme names](themes.md#theme-names).

## Messages

These messages show on the status bar. In presentation mode, they show on the last row for 2 seconds.

### Messages of the annotation commands

| Message | When |
|---|---|
| `ID: LEGEND at LN` | `:mark` added a mark. |
| `ID: highlight at LN` | `:hl` added a highlight. |
| `ID: arrow LA → LB` | `:arrow` added an arrow. |
| `ID: box A-B` | `:box` added a box. |
| `ID: note at LN` | `:note` added a note. |
| `ID: callout at LA-B` | `:callout` added a callout. |
| `ID: spotlight at step N` | `:spot` added a spotlight. |
| `ID active until step N` | `:until` set the last step of a spotlight. |
| `style NAME defined (use it with :mark NAME …)` | `:style` defined or changed a style. |
| `style NAME removed` | `:style NAME` removed a style. |
| `ID updated` | `:label` changed a label. |
| `ID: color COLOR` | `:color` changed a color. |
| `ID: updated` | `:set` changed a property. |
| `ID now appears at step N` | `:step ID N` moved an annotation. |
| `ID → step N` | `:step ID N` moved an annotation to a step that does not draw it. |
| `N deleted` | `:del` deleted `N` annotations. |
| `N annotation(s) cleared (u to undo)` | `:clear` deleted `N` annotations. |
| `nothing to clear` | `:clear` found no annotation to delete. |

- A message about a new annotation can end with a step hint, for example `(step N, press n)`. The hint shows when the annotation goes to a later step than the step on screen.
- `ID → step N` ends with the reason, for example the file that the step shows.
- In a message, `LN` is the letter L and a line number, for example `L3`. `LA`, `LB`, `LA-B`, `LK` and `LL` follow the same rule.

### Errors of the annotation commands

| Message | When |
|---|---|
| `use K:TEXT («TEXT» is on LK, not LL)` | The text of `L:TEXT` is on another line. |
| `«TEXT» is not in the code shown (FILE). L:text skips notes and labels.` | The text is not in the code on screen. |
| `«TEXT» is in FILE at LN, outside the lines shown at step S (A-B).` | The text is in the file, but outside the lines on screen. |
| `«TEXT» is in FILE at LN, shown at step S.` | The text is in a file that another step shows. |
| `line N does not exist (the document spans A to B)` | The line is not in the code. |
| `annotation ID does not exist` | No annotation has the id. |
| `invalid range: R` | The range is not a range of lines. |
| `invalid range: R: write A-B without spaces` | The range has spaces around the `-`. |
| `markers: write @'start'..@'end' (two markers joined by ..)` | No `..` follows the first marker. |
| `markers: the end marker also starts with @` | The second marker has no `@`. |
| `markers: missing closing Q` | A marker has no closing quote `Q`. |
| `markers: empty marker` | A marker has no text. |
| `start marker «START» matches nothing in the code` | The code has no `START`. |
| `end marker «END» (after line N) matches nothing in the code` | The code has no `END` after `START`. |
| `end marker «END» appears only before the start marker «START» (line N)` | `END` is only before `START`. |
| `the note is empty` | `:note` got no text. |
| `the callout text is empty` | `:callout` got no text. |
| `unknown placement: use @below, @margin or @auto` | `:callout` got another `@` word. |
| `ID is not a spotlight` | `:until` got an annotation that is not a spotlight. |
| `ID starts at step S: :until ID N needs N ≥ S` | `:until` got a step before the step of the spotlight. |
| `unknown mark kind (kinds: KINDS; or define one with :style, or use a glyph/emoji)` | `:mark` got a kind that it does not know. |
| `a style needs at least a glyph` | `:style` defined a new style with no glyph, and `NAME` is not a built-in kind. |
| `unknown style field (glyph, legend, color)` | `:style` got another key. |
| `unknown color (use NAMES or #rrggbb)` | The color is not a color name or `#rrggbb`. |
| `unknown emphasis (fill, underline, brackets, strike)` | `emphasis=` got another value. |
| `` `KEY` applies to arrows only (ID is not an arrow) `` | `:set` got an arrow key for another annotation. |
| `` `KEY` applies to boxes and callouts only (ID is not one) `` | `:set` got a border key for another annotation. |
| `` `emphasis` applies to highlights and marks only (ID is not one) `` | `:set` got `emphasis` for another annotation. |
| `expected key=value, got «TEXT»` | A property has no key or no value. |
| `step must be a number` | `:set` got a `step` that is not a number. |
| `annotations live at steps ≥ 1 (step 0 is the code alone)` | `:step ID N` got step 0. |
| `step N is beyond the last step (M): an annotation can go up to step K (the next empty one)` | `:step ID N` got a step number past `K`, the first empty step. |

In `is not in the code shown`, `FILE` can end with the lines on screen, for example `main.rs:12-48`.

### Errors of the values

| Message | When |
|---|---|
| `unknown stroke (use solid, dashed, dotted, thick, double)` | `stroke=` got another value. |
| `unknown border (use solid, dashed, dotted, thick, double)` | `border=` got another value. |
| `unknown route (use rounded, sharp)` | `route=` got another value. |
| `unknown corners (use rounded, sharp)` | `corners=` got another value. |
| `heads must be END or START,END (each one of none, arrow, dot, bar, open)` | `heads=` got another value. |
| `unknown start head (use none, arrow, dot, bar, open)` | `start=` got another value. |
| `unknown end head (use none, arrow, dot, bar, open)` | `end=` got another value. |
| `fill must be yes\|tint or no\|none` | `fill=` got another value. |
| `unknown key (stroke, route, heads, start, end)` | `:arrow` got another key, or `:set` got a key that it does not know. |
| `unknown key (border, corners, fill)` | `:box` or `:callout` got another key. |
| `unknown key for :hl (emphasis=fill\|underline\|brackets\|strike)` | `:hl` got another key. |
| `unknown key for :mark (emphasis=fill\|underline\|brackets\|strike)` | `:mark` got another key. |

### Messages of the step, file and view commands

| Message | When |
|---|---|
| `step N/M` or `step N/M · TITLE` | `:step`, `:next`, `:prev` or `:goto` reached a step. |
| `step N/M: empty, new annotations go here` | `:step N` opened the empty step after the last step. |
| `step N is beyond the last step (M); :step K opens a new one` | `:step N` got a step number past `K`, the first empty step. |
| `move #N added: step S shows FILE (:ref path to change the document's own file)` | `:link PATH` at step 1 or later added a `code.link` move. |
| `the .tiza now references FILE and shows its code (N annotations kept; u undoes)` | `:ref` or `:link` loaded the code of another file. |
| `the .tiza now references FILE` | `:ref` or `:link` referenced a file that has the code on screen. |
| `the .tiza already references FILE` | `:ref` or `:link` got no path, and the document references a file already. |
| `line ranges start at 1` | `:link PATH:A-B` at step 1 or later got `A` = 0. |
| `range A-B: the first line must not be after the last` | `:link PATH:A-B` at step 1 or later got `A` after `B`. |
| `` malformed line range `R`: use A, A-B or A..B `` | `:link PATH:R` at step 1 or later got digits, `-` and `.` that are not a range. |
| `code frozen inside the .tiza` | `:embed` copied the code into the document. |
| `step N is not a valid number` | `:goto` got a number that is too large. |
| `focus on A-B` | `:focus` set the focus. |
| `focus on A-C (B is past the last line, C)` | `:focus` stopped the focus at the last line `C`. |
| `focus cleared` | `:focus` cleared the focus. |
| `language: LANG` | `:lang LANG` set a language. |
| `language: LANG (no grammar: shown as plain text; try rs, ts, py, go, toml…)` | `:lang LANG` set a language that has no grammar. |
| `language: LANG (HOW)` | `:lang auto` found the language. |
| `language: LANG (:lang auto re-detects, :lang <id> overrides)` | `:lang` shows the language. |
| `language: LANG (plain text; :lang <id> to set one)` | `:lang` shows a language that has no grammar. |
| `title updated` | `:title` set or removed the title. |
| `numbers on (saved in the document)` | `:numbers on` showed the line numbers. |
| `numbers off: the gutter is hidden here, in exports and for agents (saved in the document; :numbers on restores it)` | `:numbers off` hid the line numbers. |
| `numbers already on` or `numbers already off` | The line numbers were in that state already. |
| `padding V H` | `:pad` set the padding. |
| `padding V H (default)` | `:pad default` set the default padding. |
| `padding V H (shown as CV CH: the terminal is too small for more)` | The terminal is too small for the padding. |
| `padding V H (from --padding, not saved)` | `:pad` shows a padding from `--padding`. |
| `margin N: notes, labels and legends wrap at N cells (saved in the document)` | `:margin N` set the margin. |
| `margin=N: use a number between 12 and 200, or auto` | `:margin N` got a number out of the range. |
| `margin auto` | `:margin auto` removed the limit. |
| `margin already auto` | The margin had no limit already. |
| `margin auto (the right margin takes what the width leaves)` | `:margin` shows that the margin has no limit. |
| `margin N (notes wrap at N cells; :margin auto removes the cap)` | `:margin` shows the limit. |
| `layout stable: lanes and margin sized for every step, as in presentation mode and exports` | `:layout stable` set the take as the layout. |
| `layout dynamic: the authoring screen grows lanes as steps add arrows (exports stay stable)` | `:layout dynamic` set the layout that grows. |
| `presentation mode (P to exit)` | `:present` started presentation mode. |
| `supervised mode on (N pending)` or `supervised mode off (N pending)` | `:supervise` shows the state. |
| `no agent changes wait` | `:review` found no change to show. |

- The other messages of `:goto` are in [Messages](document-format.md#messages). They are for an unknown id and a step after the last step.
- In `<id>` and in `usage: :lang ID`, the id is a language id, `LANG`.
- In the `padding` messages, `V H` is the padding in use, and `CV CH` is the padding that fits on the screen.
- `HOW` is `file extension`, `file name`, `shebang`, `modeline` or `default: plain text`.
- In the `move #N added` message, `FILE` ends with `:A-B` when you gave lines. A warning about the move can come before it.
- In `already references FILE`, `FILE` ends with `:A-B` when the document references lines. The `now references` messages give the path alone.
- A `references` message can end with `; following NAME` when follow mode watches the file.
- The other messages of `:link`, `:ref` and `:embed` are in [Follow the editor](../guides/follow-the-editor.md#messages).
- In presentation mode, `:layout` without a value adds `(presentation mode is always stable)`. `:layout stable` and `:layout dynamic` show their usual message.

### Usage messages

A command with wrong arguments shows its usage:

| Message | Command |
|---|---|
| `usage: :mark KIND[@COLOR] L:text [label]   KIND: KINDS \| a :style name \| a glyph` | `:mark` |
| `usage: :hl L:text [label]   (L:text = the text on line L; L: alone = the whole line; .:text = the cursor line)` | `:hl` |
| `usage: :arrow L:text L:text [label] [stroke=dashed] [heads=none,arrow] [route=sharp]` | `:arrow` |
| `usage: :box A-B [label] [border=double] [corners=sharp] [fill=no]   or   :box @'start'..@'end' [label]   (ERROR)` | `:box` |
| `usage: :note L text  (L may be . for the cursor line)` | `:note` |
| `usage: :callout A[-B] [@below\|@margin\|@auto] [title="…"] text   (or @'start'..@'end')   (ERROR)` | `:callout` |
| `usage: :spot A-B [ID…]  \|  :spot ID…` | `:spot` |
| `usage: :until ID N   (last step in which the spotlight stays active)` | `:until` |
| `usage: :style NAME glyph=G legend="text" color=C   (:style NAME removes it)` | `:style` |
| `usage: :label ID text` | `:label` |
| `usage: :color ID name\|#rrggbb\|auto   (names: NAMES)` | `:color` |
| `usage: :set ID key=value…   (label, color, step; arrows: stroke, route, heads, start, end; boxes/callouts: border, corners, fill; highlights/marks: emphasis)` | `:set` |
| `usage: :del ID [ID…]` | `:del` |
| `usage: :clear  \|  :clear N  \|  :clear .  (all, one step, the current step)` | `:clear` |
| `usage: :step N  \|  :step ID N` | `:step` |
| `usage: :step ID N` | `:step`, when `N` in `:step ID N` is not a number |
| `usage: :goto ID  (a named step, or a step number)` | `:goto` |
| `usage: :focus A-B  \|  :focus   (clear)` | `:focus` |
| `usage: :lang ID  \|  :lang auto  \|  :lang   (one language id, e.g. rs, ts, py)` | `:lang` |
| `usage: :pad V [H] \| :pad default   (blank cells around the drawing; :pad 0 0 removes it)` | `:pad` |
| `` margin X: use `:margin N` (12-200 cells) or `:margin auto` `` | `:margin` |
| `` numbers X: use `:numbers on` or `:numbers off` `` | `:numbers` |
| `` layout X: use `:layout stable` or `:layout dynamic` `` | `:layout` |
| `usage: :link path[:A-B]   (without a path, uses the source file)` | `:link` |
| `usage: :follow [on\|off]` | `:follow` |
| `usage: :autosave [on\|off]` | `:autosave` |
| `usage: :supervise [on\|off]` | `:supervise` |

- `ERROR` is the error of the range, for example `invalid range: 3 - 5: write A-B without spaces`.
- `KINDS` lists the styles of the document and the built-in kinds. `NAMES` lists the color names. `X` is the value that you typed.
- When `:mark` gets a kind and an anchor with no `:`, the usage also explains `L:TEXT`.
- The message for a wrong `:theme` name is in [Messages](themes.md#messages).
- The usage message of `:mouse` is in [Messages](../guides/present.md#messages).

### Unknown commands

| Message | When |
|---|---|
| `unknown command — did you mean :NAME?` | No command has this name, and one command name is near it. |
| `unknown command — did you mean :NAME, :NAME?` | Two or three commands are equally near it. |
| `unknown command (try :help)` | No command is near it. |

A name is near when at most two changes make it the name that you typed. A change adds, removes or replaces one letter.

## How it works

The help (`?`) gets its commands from one table in the code. A test checks that the help has a row for each command name.
