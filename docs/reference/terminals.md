# Terminal support

This page tells what Tiza needs from your terminal, and what Tiza does when the terminal does not have it. It also lists the terminals where Tiza has a test record.

## Use Tiza in your terminal

The `dark` and `light` themes need a terminal with 24-bit color. Tiza also needs a monospace font with box-drawing characters. Most of the other terminal features are optional.

1. Open a file with `tiza FILE`.
2. Look at the lines of a box or an arrow. They must join with no gaps. If they do not, see [Box-drawing characters](#box-drawing-characters).
3. Look at the colors. If they look wrong, start Tiza with `--theme terminal`. See [A terminal without 24-bit color](#a-terminal-without-24-bit-color).

To paste code, paste it into your terminal. Tiza shows the pasted code in place of the code on screen.

| Feature | Tiza uses it for | Without it |
|---|---|---|
| 24-bit color | The colors of the `dark` and `light` themes. | The colors are wrong, so use `--theme terminal`. |
| Box-drawing characters | The borders of boxes and the lines of arrows. | The lines have gaps, or wrong characters show. |
| Bracketed paste | A paste of code into the terminal. | Tiza reads a paste as keys, so pipe the code into Tiza. |
| The color query | The auto theme: `light` or `dark` from the background of the terminal. | Tiza keeps the [theme that it starts with](#no-answer-to-the-color-query). |
| Mouse events | Mouse capture, only when you turn it on. | The mouse does not work in Tiza. |
| Synchronized output (mode 2026) | Each frame shows at one time. | A big change of the view can show part of two frames for a moment. |

## Tested terminals

| Terminal | Version and system | Date | What was checked |
|---|---|---|---|
| The Zed terminal | Zed 1.25.3 on macOS | 2026-10-24 | The tasks of the [Zed guide](../zed.md). |
| The Zed terminal | Zed 1.24.2 on macOS 27.0.1 | 2026-10-18 | The step tasks of the [Zed guide](../zed.md), with Tiza in Herdr in the terminal dock. |
| Herdr | Herdr, version not recorded, on macOS | 2026-10-24 | The frames of the terminal size matrix in `qa/herdr/`. |
| Herdr, Terminal.app and the Zed terminal | Herdr 0.9.5, Terminal.app build 488 and Zed, version not recorded, on macOS | 2026-10-22 | The width of `⬚`: one cell in each terminal. |
| tmux | tmux 3.5a on Debian trixie and tmux 3.3a on Debian bookworm, in Docker | 2026-10-18 | The color query only. See [tmux](#tmux). |

Tiza has no test record for other terminals. To report a result, see [Report a terminal problem](#report-a-terminal-problem).

## What Tiza turns on in the terminal

| Feature | What Tiza does |
|---|---|
| The alternate screen and raw mode | Tiza turns them on at start and off when it quits. |
| Bracketed paste | Tiza turns it on at start and off when it quits. |
| Mouse capture | Tiza turns it on only with `--mouse` or `:mouse on`, and off with `:mouse off`. |
| The color query | Tiza sends it at start and at each resize, only with the auto theme or the terminal theme. |
| The cursor | Tiza hides it, except on the `:` command line. |
| Synchronized output (mode 2026) | Tiza sends each frame in one synchronized update. |
| The kitty keyboard protocol | Tiza never turns it on. |

- Tiza shows the cursor again when it quits.
- With synchronized output, the terminal shows all of each frame at one time. A big change of the view never shows part of the old frame and part of the new one. A terminal without mode 2026 ignores it.
- After a crash, Tiza ends a synchronized update that is open. Then it leaves the alternate screen and turns off raw mode. Then it turns off mouse capture and bracketed paste.
- An error in an agent request does not stop Tiza. Tiza answers the request with an error and writes a line to `panics.log`.
- Tiza does not read `COLORTERM` in its own code. For one theme, it sends the same colors to each terminal. See [Environment variables](../concepts/privacy-and-network.md#environment-variables).
- No key needs the kitty keyboard protocol. See [Keys](#keys).

## Messages

| Message | When |
|---|---|
| `` tiza: the TUI needs a terminal on stdout; use `tiza render` for text output `` | You redirect the output of Tiza to a file or a pipe. |
| `` tiza: the TUI needs a terminal to read keys from (ERROR); use `tiza render` for text output or `tiza check` to check a document `` | Standard output is a terminal, but Tiza has no terminal to read keys from. |
| `nothing to paste: the clipboard text is empty` | Outside the command line, you paste no text or only white space. |
| `cannot paste while moves are applied (step N): go back to step 0 first` | You paste at a step where a move is applied. |
| `paste ignored: close the PANEL with Esc, then paste again` | You paste while the help, the list or the review is open. `PANEL` is `help`, `list` or `review`. With a filter, the message says `Esc twice`, because the first `Esc` clears the filter. |
| `paste ignored in presentation mode: press P to leave it, then paste again` | You paste in presentation mode. |

- The two `tiza:` messages go to stderr. Then Tiza exits with code 1.
- Tiza checks the terminal after it opens the file. A file that does not open gives its own error first.
- `ERROR` is the error text of the system.
- The paste messages show in the status bar.

For example, `ERROR` is this text on macOS:

```
Device not configured (os error 6)
```

On Linux, `ERROR` is this text:

```
No such device or address (os error 6)
```

## Special cases

### A terminal without 24-bit color

The `dark` and `light` themes send each color as a 24-bit color. Tiza does not switch to 256 colors. In a terminal without 24-bit color, these colors are wrong.

- Start Tiza with `--theme terminal`, or type `:theme terminal`.
- The tokens of the terminal theme send no 24-bit color. [Use the terminal colors](themes.md#use-the-terminal-colors) gives the rules for tokens and for `#rrggbb` colors.
- Terminal.app has 24-bit color only from macOS 26. On an older macOS, use `--theme terminal` or another terminal.

### Box-drawing characters

Tiza draws with Unicode box-drawing characters and arrows, for example `─ │ ╭ ╯ ┆ ━ ▶`.

- Use a monospace font in which these characters fill the cell. Then the lines join with no gaps.
- A glyph of a custom style, such as an emoji, needs a font that has it.
- In Zed, set `"line_height": "standard"` in the `terminal` settings. See [The Zed terminal](../zed.md#the-zed-terminal).

### Paste

Tiza turns on bracketed paste at start. Then the terminal marks the start and the end of each paste.

- A paste in normal mode replaces the code on screen with the pasted code.
- A paste also replaces the code in a `v` or `V` selection.
- While a panel is open, also while you type its filter, a paste changes nothing. The status bar tells you to close the panel. In presentation mode, a paste changes nothing too.
- A paste into the command line adds the text before the first line end. A line end is a line feed or a carriage return. Tiza removes the spaces at the end of that text.
- A terminal without bracketed paste sends a paste as typed keys. Then Tiza runs each character as a key.
- In that terminal, pipe the code into Tiza. For example, type `pbpaste | tiza --lang rs` on macOS.

### Mouse capture

Tiza asks the terminal for mouse events only while mouse capture is on. Without mouse capture, the terminal keeps its own text selection.

- To turn mouse capture on or off, see [Turn the mouse on](../guides/present.md#turn-the-mouse-on).
- While mouse capture is on, a drag selects code in Tiza, not text in the terminal.
- To select text in the terminal, see [Copy text from the terminal](../guides/present.md#copy-text-from-the-terminal).

### Signs that some terminals draw wide

Unicode gives `⚠` (U+26A0), `↪` (U+21AA), `⏸` (U+23F8) and `▶` (U+25B6) one cell, but some terminals draw them two cells wide. The status bar counts one cell for each, so in such a terminal the bar is too long for its row.

- At start, Tiza measures how wide the terminal draws each of these signs. See [The width probe](#the-width-probe).
- When the terminal draws a sign two cells wide, the text that Tiza shows outside the code shows a plain form in its place: `!` for `⚠`, `>` for `↪` and `▶`, and `||` for `⏸`. This text is the status bar with its hints and messages, the messages of the command line, the panels, and the messages and the review chip of presentation mode.
- A step title, a label and a message that you write get the plain form there too.
- The code and the annotations keep the signs, also in the exports. In such a terminal, a `⚠` in a line of code moves the rest of that line by one cell. At the right edge of the screen, the line can then go past the edge: the screen moves up one row, or the end of the line is lost.
- What you type in the command line keeps the signs.
- When the terminal draws the signs one cell wide, or does not answer, Tiza shows them as they are.

### Invisible characters

Tiza draws each invisible character as `⬚` (U+2B1A). [Control and invisible characters](characters.md) gives the rules. Herdr, Terminal.app and the Zed terminal draw `⬚` in one cell. See [Tested terminals](#tested-terminals).

### tmux

The result of the color query depends on the version of tmux. These results come from a test in Docker on Linux. A test program acted as the outer terminal, with a light background.

| tmux version | What tmux does with the color query | What Tiza does |
|---|---|---|
| 3.5a | tmux answers with the colors of the outer terminal. | The auto theme works with no configuration. |
| 3.3a | tmux does not send the color query to the outer terminal and does not answer it. | Tiza keeps the [theme that it starts with](#no-answer-to-the-color-query). |

- With tmux 3.3a, pick the theme yourself: start Tiza with `--theme light`, or press `t`. You can also upgrade tmux.
- Tiza has no test record for the versions between 3.3a and 3.5a.
- Tiza sends the color query as plain OSC and does not wrap it for tmux. So `set -g allow-passthrough on` changes neither result.
- To send the color query again after a theme change with no resize, see [The auto theme](themes.md#the-auto-theme).
- The `set -g mouse` setting of tmux and mouse capture in Tiza decide together which program gets the clicks, the drags and the wheel.
- tmux sends 24-bit color to the outer terminal only when it knows that the outer terminal has it. To tell tmux, add `set -as terminal-features ',*:RGB'` to the tmux configuration.

### When the terminal closes

Tiza quits as with Ctrl-C when its terminal closes. See [When the terminal closes](../concepts/saving.md#when-the-terminal-closes).

## Report a terminal problem

Report a problem with a terminal in [Discussions](https://github.com/luiscarlosjayk/tiza/discussions). Give this information:

- The name and the version of the terminal.
- The output of `echo "$TERM $COLORTERM"`.
- Whether tmux or Herdr runs between the terminal and Tiza.
- A screenshot.

If you have steps that reproduce the problem, use the [bug report form](https://github.com/luiscarlosjayk/tiza/issues/new/choose).

## How it works

### The color query

The color query asks the terminal for its background color with OSC 11, and for its text color with OSC 10. [Themes](themes.md#how-it-works) and [Privacy and network](../concepts/privacy-and-network.md#the-terminal) give the other rules of the color query.

- The auto theme uses the background answer to pick `light` or `dark`.
- `:theme auto` sends no color query. It uses the last answer.

With the terminal theme, Tiza uses the background answer for these:

- The tint of a highlight and of the status bar.
- The theme that `t` changes to.
- The theme of an export, `dark` or `light`.

Tiza reads an answer in the form `rgb:R/G/B`, with 1 to 4 hex digits for each part, or `#rrggbb`. The answer can end with `ESC \` or with BEL.

At start, Tiza turns on raw mode on that terminal for the wait. After the wait, it puts back the old mode. Tiza waits with these rules:

1. Tiza sends the color query, when the theme needs it, and the [width probe](#the-width-probe). Then it reads what the terminal sends.
2. When the terminal has answered all of them, Tiza stops the wait. That is OSC 11 and OSC 10, when Tiza sent the color query, and the cursor reports of the width probe.
3. When nothing comes for 100 ms, Tiza stops.
4. When something comes more than 200 ms after the color query and the width probe, Tiza stops after it reads it.

- So the wait is 100 ms when the terminal does not answer and you do not type.
- When data comes while Tiza waits, the wait can be almost 300 ms. For example, you type, or a slow answer comes over SSH.
- When the terminal answers only OSC 11, Tiza uses that answer after 100 ms with no new data.
- Tiza drops a paste or a mouse report that comes during the wait.
- An answer that comes after the wait arrives as key presses. Tiza takes it out of the keys and uses it.
- When the wait stops in the middle of an answer, the rest of the answer comes as key presses. Tiza takes it out of the keys too, when it comes before the first frame of the TUI or not more than 1 s after the first frame.
- When the wait stops right after the `ESC` of an answer, Tiza cannot tell that `ESC` from the Escape key. Thus Tiza keeps it until the next key. When the next key is `]` or `[`, the `ESC` is the start of the answer. When the next key is another key, the `ESC` is your Escape key, and Tiza uses it before that key.
- When no key comes in 1 s after the first frame, Tiza uses the `ESC` as your Escape key. Thus an Escape that you type at the very end of the wait can come up to 1 s late.
- When you type Escape and then `]` at the very end of the wait, Tiza takes them for the start of an answer, and drops them after 1 s.

### The width probe

At start, also with a theme that needs no color query, Tiza measures how wide the terminal draws `⚠`, `↪`, `⏸` and `▶`. See [Signs that some terminals draw wide](#signs-that-some-terminals-draw-wide).

- Tiza saves the cursor and asks for its position (`ESC [ 6 n`). When the answer comes, Tiza writes the signs if the cursor is two columns or more before the last column. For each sign, it puts the cursor back, writes the sign and asks again. The column of each answer (`ESC [ row ; col R`), less the first column, gives the width of the sign.
- Then Tiza puts the cursor back and clears the line from there. The signs show for a moment where the cursor was. The text before the cursor stays.
- When the cursor is nearer to the right edge, Tiza writes no sign and clears nothing. Thus it measures no sign, and shows the signs as they are. There, a sign can go to the next row and move the screen up, and the answer can give a wrong column.
- The cursor is near the right edge only when the line where you start Tiza has text up to that edge. An example is output of `printf` with no line end. In a terminal that draws the signs two cells wide, the status bar can then be too long for its row, as before.
- The probe uses the same wait as the color query. Tiza writes the signs after the first answer. Thus the probe adds two round trips to the terminal at start. With a theme that needs the color query, it adds one round trip. A terminal that does not answer the probe adds 100 ms.
- An answer that comes after the wait is not a key, and Tiza does not use it: it shows the signs as they are. On a slow link, such as SSH with more than 100 ms of round trip, this can be the case. Then the status bar can be too long for its row, as before.
- When the wait stops in the middle of an answer, Tiza takes its rest out of the keys too, as for the color query. This is also true when the wait stops right after the `ESC` of the first answer. There is one exception: the terminal answered nothing else, and the wait stops right after the `ESC [` of the first answer. Then that `ESC [` is a key, as an arrow key that you type, and the rest of the answer comes as key presses.
- On a slow link, an answer can come after the wait but before the TUI starts. Then the terminal can show it, for example `^[[1;2R`, on the line where you started Tiza. This is also true for an answer to the color query.
- `TIZA_NO_OSC` stops the width probe too.

### No answer to the color query

With no answer to OSC 11, Tiza shows no message. It keeps the theme that it starts with:

- A document that names no theme gets `dark`.
- With `--theme auto`, a document that names a theme keeps that theme.
- The terminal theme stays `terminal`. It uses the tint for a dark background.

### Keys

- No key needs the kitty keyboard protocol. Tiza reads the keys as a plain terminal sends them.
- Tiza reads keys from `/dev/tty` when standard input is not a terminal, for example a pipe or a file. See [Privacy and network](../concepts/privacy-and-network.md#the-terminal).
- Tiza reads `Shift-Tab` as the back-tab sequence `ESC [ Z`.
