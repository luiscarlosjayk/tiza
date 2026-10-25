# Keys

Press these keys in the Tiza window.

| Key | Action |
|---|---|
| `j`/`k`, `↑`/`↓`, `g`/`G` | move the line cursor |
| `h`/`l` (or `←`/`→`), `w`/`b`/`e`, `0`/`$` | move the column cursor (word motions) |
| `v` / `V` / `i` | select text / select lines / select the word under the cursor |
| with a selection: `h` `m` `n` `C` `b` `a` `s` | highlight (text selection; a line selection makes a focus) · mark (then type `KIND [label]`) · note · callout (then type the text) · box · arrow (source `a`, target `a`) · spotlight |
| `Tab` / `Shift-Tab` | select an annotation; then `Enter` edit label · `e` set properties · `x` delete · `c` color · `+`/`-` move step |
| `n` / `p` (or `PageDown`/`PageUp`; `→`/`←` in presentation mode) | next / previous step; step 0 shows only the code (with `"loop": true`, `n` past the last step is step 0 again) |
| `u` / `U` | undo / redo (the message names what was undone) |
| `t` | dark ⇄ light theme (like Excalidraw); fixes it in the document and stops following the terminal |
| `P` | presentation mode (hides the UI; one geometry for the whole take — see `:layout`). `:` still works there: the command line shows on the last row while you type, its result (or error) stays for ~2 s, then the frame is clean again |
| `f` | live pointer, like a laser pointer: the cursor line (or the `v`/`V` selection) stays as it is and the rest of the code dims, as with a spotlight; it follows `j`/`k` and every motion (in presentation mode too, where the cursor stays hidden), stays on its line across `n`/`p`, and `f` again or `Esc` turns it off. Nothing is created: it is never saved, never in the undo stack and never in an export, a GIF or a recording. An agent can point with the same pointer (`point_at`) |
| `L` | annotation list (`j`/`k` select, Enter goes to the step, `x` deletes, `u`/`U` undo and redo, `/` filters) · `?` help (press `/` in it to filter) · `:` commands · `q` quit (`:q!` quits without saving, also over stale moves) |
| `Ctrl-C` | cancels, or quits, as in vim: where something is open it closes it like `Esc` — the command line (its text is dropped, never run), a `v`/`V` selection, an arrow being drawn, the help, the list or the review — and with nothing open it quits as `:q!` (see **Saving**: it writes nothing the mode had not written already, a failed autosave is tried once more, and stderr says what was not saved). In normal and presentation mode, with the live pointer on, while a step types, with an annotation selected with `Tab` or a notice on the bar, it quits |
