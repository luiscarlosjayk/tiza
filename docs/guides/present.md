# Use the mouse

Use this page to select code with the mouse while you explain it.

Tiza is a terminal tool that draws annotations on code. To install it, see [Install](install.md).

## Turn the mouse on

The mouse is off by default. While mouse capture is on, a drag selects code in Tiza, not text in the terminal. To copy text, see [Copy text from the terminal](#copy-text-from-the-terminal).

| Command | What it does |
|---|---|
| `tiza FILE --mouse` | Starts Tiza with mouse capture on. |
| `:mouse on` | Turns mouse capture on. |
| `:mouse off` | Turns mouse capture off. |
| `:mouse` | Shows the state of mouse capture. |

## Select with the mouse

1. Drag over the code. When you release the button, the selection stays open.
2. Read the status bar. It shows the keys for the selection.
3. Press a key. See [Selection keys](#selection-keys).

To cancel the selection, press Esc or Ctrl-C.

To draw an arrow:

1. Select the first end of the arrow and press `a`.
2. Select the second end and press `a` again.

To cancel the arrow, press Esc or Ctrl-C.

## Copy text from the terminal

While mouse capture is on, use a modifier key to select text in the terminal:

| Terminal | How to select text |
|---|---|
| Ghostty, WezTerm and most Linux terminals | Hold Shift while you drag. |
| iTerm2 and Terminal.app | Hold Option while you drag. |

- In Zed, Cmd+click still opens links while mouse capture is on.
- In tmux, mouse events reach Tiza only with `set -g mouse on` in the tmux configuration.

## Mouse actions

| Action | Result |
|---|---|
| Click | Moves the cursor to the line and the column that you click. |
| Drag on one line | Selects the text that you drag over. |
| Drag over two or more lines | Selects those lines. |
| Wheel | Moves the cursor three lines, up or down. |

## Selection keys

When you release the button, the status bar shows the keys for the selection:

| Status bar | Selection |
|---|---|
| `selected: h highlight · m mark · a arrow · n note · b box (Esc)` | Text on one line |
| `lines A-B selected: b box · s spotlight · C callout · n note · h focus (Esc)` | Two or more lines |

The keys do these actions:

| Key | On text on one line | On two or more lines |
|---|---|---|
| `h` | Adds a highlight. | Sets the focus on the lines. |
| `m` | Opens `:mark` for the text. | Opens `:mark` for the lines. |
| `a` | Sets one end of an arrow. | Does nothing. |
| `n` | Opens `:note` for the line. | Opens `:note` for the first line. |
| `b` | Opens `:box` for the line. | Opens `:box` for the lines. |
| `s` | Adds a spotlight on the line. | Adds a spotlight on the lines. |
| `C` | Opens `:callout` for the line. | Opens `:callout` for the lines. |

## Messages

| Message | When |
|---|---|
| `mouse on: click moves, drag selects, wheel scrolls (Shift/Option-drag keeps the terminal's own selection)` | `:mouse on` turned mouse capture on. |
| `mouse on` | `:mouse` shows that mouse capture is on. |
| `mouse off` | `:mouse off` turned mouse capture off, or `:mouse` shows that it is off. |
| `usage: :mouse [on\|off]` | `:mouse` got an argument other than `on` or `off`. |
| `ID: highlight (:label ID text to name it)` | `h` added a highlight on the selected text. |
| `focus A-B (Esc clears)` | `h` set the focus on the selected lines. |
| `arrow: now select the target and press a (Esc cancels)` | `a` set the first end of an arrow. |
| `ID: arrow (:label ID text to name it)` | `a` set the second end and added the arrow. |
| `arrow cancelled` | Esc or Ctrl-C cancelled the arrow while no selection was open. |

## Special cases

- A click on a label or on the border of a box moves the cursor to the nearest code line above it.
- A click outside the code does nothing.
- The wheel stops at the first line and at the last line.
- While the help, the annotation list or the review is open, Tiza ignores the mouse.
- While the command line is open, a click does nothing.

### Presentation mode

Presentation mode (`P`) ignores each click, drag and wheel event. It hides the cursor and the selection. Thus you cannot see what you select. To annotate with the mouse, leave presentation mode first. Then the mouse works again, and you do not need `:mouse on` again.

## How it works

Tiza asks the terminal to send mouse events only while mouse capture is on. `:mouse on` and `:mouse off` change this at once. When Tiza quits, it turns mouse capture off. Tiza also turns mouse capture off after a crash. Then the terminal works as before.
