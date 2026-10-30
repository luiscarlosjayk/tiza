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

## Commands by task

| Page | Commands |
|---|---|
| [Annotation commands](commands/annotate.md) | `:mark`, `:hl`, `:arrow`, `:box`, `:note`, `:callout`, `:spot`, `:until`, `:style` |
| [Change commands](commands/change.md) | `:label`, `:color`, `:set`, `:step ID N`, `:del`, `:clear` |
| [Step and file commands](commands/steps-and-files.md) | `:step`, `:next`, `:prev`, `:goto`, `:link`, `:ref`, `:embed`, `:scroll`, `:follow` |
| [View commands](commands/view.md) | `:focus`, `:theme`, `:lang`, `:title`, `:numbers`, `:pad`, `:margin`, `:layout`, `:mouse` |
| [Export and agent commands](commands/export-and-agents.md) | `:export`, `:supervise`, `:review` |
| [Save, quit, panel and undo commands](commands/save-and-quit.md) | `:w`, `:saveas`, `:wq`, `:q`, `:autosave`, `:list`, `:present`, `:help`, `:undo`, `:redo` |
| [Command messages](commands/messages.md) | The messages and the errors of all the commands |

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
| `L@C` | The character at column `C` of line `L`. The column counts the characters from 0. |
| `L@S..E` | The characters from column `S` to column `E` of line `L`. The selection keys write this form. |
| `@'START'..@'END'` | The lines from the first `START` in the code to the first `END` after it. |
| `ID` | The id of an annotation, for example `a3`. |
| `N` | A number, for example a step number. |
| `KEY=VALUE` | A property of the annotation, for example `stroke=dashed`. |
| `[…]` | A part that you can leave out. |

- `TEXT` in `L:TEXT` is one word. It ends at the first space. The next word starts the next argument. Quotes do not change this.
- `L:TEXT` must find the text on line `L`. When the text is on another line, the message gives the correct line.
- `L:TEXT` looks only in the code. It does not look in the notes and the labels.
- In `:box`, `:callout`, `:spot` and `:focus`, a range can also be `A..B` or `A:B`. Write it without spaces. When `A` is after `B`, Tiza puts them in order.
- `:link` and `:ref` accept `A-B`, `A..B` or `A` after the path, and do not put the lines in order. See [`:link`](commands/steps-and-files.md#link).
- `:box .` and `:callout .` use the cursor line as the range. `:spot .` does not: it reads `.` as an id.
- Put a marker in `'` or `"` quotes when it has a space. Without a space, the quotes are optional, for example `@begin..@end`.
- Tiza keeps the two markers in the document. It uses them to find the range again after the code changes.
- Put a value with spaces in `"` quotes, for example `legend="network I/O"`.
- Put the `KEY=VALUE` properties at the end of the command, after the label.

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
| `:theme auto` | `:theme host` |

The names of the themes are in [Theme names](themes.md#theme-names).

These values of the annotation commands and of `:set` also have other words:

| Value | Also accepted |
|---|---|
| `solid` | `line` |
| `dashed` | `dash` |
| `dotted` | `dot` or `dots` |
| `thick` | `bold` or `heavy` |
| `rounded` | `round`, `curved` or `curve` |
| `sharp` | `square` or `elbow` |
| `fill=yes` | `fill=tint`, `fill=true` or `fill=on` |
| `fill=no` | `fill=none`, `fill=false` or `fill=off` |
| `emphasis=fill` | `emphasis=tint` or `emphasis=bg` |
| `emphasis=underline` | `emphasis=under` |
| `emphasis=brackets` | `emphasis=bracket` or `emphasis=outline` |
| `emphasis=strike` | `emphasis=strikethrough` or `emphasis=del` |
| `@margin` | `@right` |
| `@below` | `@under` |

Some keys also have another name:

| Key | Also accepted | Commands |
|---|---|---|
| `border=` | `stroke=` | `:box` and `:callout` |
| `corners=` | `route=` | `:box` and `:callout` |
| `emphasis=` | `style=` | `:hl`, and `:set` on a highlight |
| `label=` | `title=`, and `text=` except on a callout (there `text=` is its paragraph) | `:set` |
| `glyph=` | `g=` | `:style` |
| `legend=` | `l=` or `text=` | `:style` |
| `color=` | `c=` | `:style` |

## How it works

The help (`?`) gets its commands from one table in the code. A test checks that the help has a row for each command name.
