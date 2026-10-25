# Terminal support

This page tells what Tiza needs from your terminal.

**Verified on macOS in Zed's integrated terminal and in [Herdr](https://herdr.dev)**, the terminals Tiza is built and used in. Any terminal with truecolor (24-bit colour) and Unicode box drawing should work; the others (Ghostty, WezTerm, iTerm2, Kitty, Alacritty, Terminal.app, tmux, Linux terminals) are **not tested** — reports welcome.

What Tiza asks of the terminal, and what happens without it:

- **Truecolor.** The `dark` and `light` themes are explicit RGB colours. Tiza does not read `COLORTERM` and has no 256-colour fallback, so in a terminal without 24-bit colour they come out wrong. `--theme terminal` (`:theme terminal`) draws Tiza's roles and the syntax with the terminal's own 16 ANSI colours instead (a colour you set yourself as `#rrggbb` is still sent as 24-bit).
- **Unicode box drawing** (`─ │ ╭ ╯ ┆ ━ ▶`) in a monospace font whose box glyphs fill the cell, so lines join without gaps. Emoji in your own styles need a font that has them.
- **Invisible characters.** Tiza shows each invisible character in the code as `⬚` (U+2B1A), one cell in the error color of the theme. Examples are a bidi control, a zero-width space and a joiner that joins nothing. Thus `ad⬚min` does not look like `admin`. Tiza draws a joiner inside an emoji sequence or a Persian word as it is. `⬚` is one cell wide in Herdr, Terminal.app and Zed.
- **Bracketed paste.** Tiza turns it on: pasting code into the window replaces the code shown (into the `:` line, it pastes the first line). A terminal without bracketed paste delivers a paste as typed keys, which Tiza runs as commands — pipe the code in instead (`pbpaste | tiza`).
- **Mouse, only if you ask.** Without `--mouse` / `:mouse on` Tiza never requests mouse reporting and the terminal keeps its own selection; with it, see **Mouse** above.
- **The OSC 11 / OSC 10 colour query** (auto appearance, see [Themes](themes.md)): sent at start and on every resize, only when the document names no theme or with `--theme terminal`; Tiza waits 100 ms at most and falls back to dark in silence when nothing answers.
- **Plain keys.** Tiza does not enable the kitty keyboard protocol and no shortcut depends on it (`Shift-Tab` is the classic back-tab every terminal sends).

Known caveats (not tested here):

- **Terminal.app** has 24-bit colour only since macOS 26; on older macOS use `--theme terminal` or another terminal.
- **tmux.** What happens to the colour query depends on the tmux version (checked in Docker on Linux, under a light outer terminal): **tmux 3.5a** asks the outer terminal itself and answers Tiza's query with its colours, so auto appearance works with no configuration; **tmux 3.3a** neither lets the query out nor answers it, so Tiza stays dark — pick the theme by hand there (`--theme light`, `t`) or upgrade tmux. Versions between 3.3a and 3.5a are not tested. Tiza sends the query as plain OSC, not wrapped for tmux's passthrough, so `set -g allow-passthrough on` changes neither case. tmux does not resize when the outer terminal changes theme, so `kill -WINCH $(pgrep tiza)` asks again. tmux's own `set -g mouse on|off` decides, together with Tiza's `--mouse`, who gets clicks, drags and the wheel. tmux passes 24-bit colour through only when it knows the outer terminal has it (`set -as terminal-features ',*:RGB'`).
- **Zed** needs `line_height: "standard"` in the terminal settings for box drawing without gaps ([docs/zed.md](../zed.md)).

A problem with a terminal? Report it in [Discussions](https://github.com/luiscarlosjayk/tiza/discussions). Give the name and the version of the terminal, the output of `echo "$TERM $COLORTERM"`, whether tmux or Herdr is between them, and a screenshot. If you have steps that reproduce the problem, use the [bug report form](https://github.com/luiscarlosjayk/tiza/issues/new/choose).
