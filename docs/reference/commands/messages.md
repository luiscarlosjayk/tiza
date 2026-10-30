# Command messages

This page lists the messages of the `:` commands. The messages show on the status bar. In presentation mode, they show on the last row for 2 seconds.

The commands are in [`:` commands](../commands.md).

## Messages of the annotation commands

| Message | When |
|---|---|
| `ID: LEGEND at LN` | `:mark` added a mark. |
| `ID: LEGEND at LN (style NAME from config.toml)` | `:mark` added a mark with a personal style, and copied the style into the document. |
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

## Errors of the annotation commands

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
| `effect draw needs a stroke: use fade, or put draw on a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike` | `:set` or a command that adds an annotation got `effect=draw` for an annotation without a stroke. |
| `unknown effect (use draw, fade or none)` | `:set`, `:box`, `:arrow`, `:hl`, `:mark`, `:note`, `:callout` or `:spot` got another value for `effect`. |
| `annotations live at steps ≥ 1 (step 0 is the code alone)` | `:step ID N` got step 0. |
| `step N is beyond the last step (M): an annotation can go up to step K (the next empty one)` | `:step ID N` got a step number past `K`, the first empty step. |

In `is not in the code shown`, `FILE` can end with the lines on screen, for example `main.rs:12-48`.

## Errors of the values

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
| `unknown key (stroke, route, heads, start, end, effect)` | `:arrow` got another key, or `:set` got a key that it does not know. |
| `unknown key (border, corners, fill, effect)` | `:box` or `:callout` got another key. |
| `unknown key for :hl (emphasis=fill\|underline\|brackets\|strike, effect=draw\|fade)` | `:hl` got another key. |
| `unknown key for :mark (emphasis=fill\|underline\|brackets\|strike, effect=draw\|fade)` | `:mark` got another key. |

## Messages of the step, file and view commands

| Message | When |
|---|---|
| `step N/M` or `step N/M · TITLE` | `:step`, `:next`, `:prev` or `:goto` reached a step. |
| `step N/N: empty, new annotations go here` | `:step N` opened the empty step after the last step. The status bar also counts that step. |
| `step N is beyond the last step (M); :step K opens a new one` | `:step N` got a step number past `K`, the first empty step. |
| `move #N added: step S shows FILE (:ref path to change the document's own file)` | `:link PATH` at step 1 or later added a `code.link` move. |
| `move #N added: step S scrolls to TARGET (ALIGN)` | `:scroll` added a `scroll` move. |
| `scroll: go to step 1 or later (step 0 is the code before any move)` | `:scroll` at step 0. |
| `the .tiza now references FILE and shows its code (N annotations kept; u undoes)` | `:ref` or `:link` loaded the code of another file. |
| `the .tiza now references FILE` | `:ref` or `:link` referenced a file that has the code on screen. |
| `the .tiza already references FILE` | `:ref` or `:link` got no path, and the document references a file already. |
| `line ranges start at 1` | `:link` or `:ref` got `PATH:A-B` with `A` = 0. |
| `range A-B: the first line must not be after the last` | `:link` or `:ref` got `PATH:A-B` with `A` after `B`. |
| `` malformed line range `R`: use A, A-B or A..B `` | `:link` or `:ref` got `PATH:R`. `R` starts with a digit, or it has only digits, `-` and `.`, but it is not a range. |
| `code frozen inside the .tiza` | `:embed` copied the code into the document. |
| `step N is not a valid number` | `:goto` got a number that is too large. |
| `focus on A-B` | `:focus` set the focus. |
| `focus on A-C (B is past the last line, C)` | `:focus` stopped the focus at the last line `C`. |
| `focus cleared` | `:focus` cleared the focus. |
| `language: LANG` | `:lang LANG` set a language. |
| `language: LANG (no grammar: shown as plain text; try rs, ts, py, go, toml…)` | `:lang LANG` set a language that has no grammar. |
| `language: LANG (HOW)` | `:lang auto` found the language. |
| `language: LANG (:lang auto detects it again, :lang LANG sets another)` | `:lang` shows the language. |
| `language: LANG (plain text; :lang LANG sets one)` | `:lang` shows a language that has no grammar. |
| `title updated` | `:title` set or removed the title. |
| `numbers on (saved in the document)` | `:numbers on` showed the line numbers. |
| `numbers off: the gutter is hidden here, in exports and for agents (saved in the document; :numbers on restores it)` | `:numbers off` hid the line numbers. |
| `numbers already on` or `numbers already off` | The document already sets that value. |
| `numbers on` or `numbers off` | `:numbers` shows the state of the line numbers. |
| `numbers on (from config.toml)` or `numbers off (from config.toml)` | `:numbers` shows the state that the config file sets, because the document does not set it. |
| `padding V H` | `:pad` set the padding. |
| `padding V H (default)` | `:pad default` set the default padding. |
| `padding V H (from config.toml)` | `:pad default` set the padding of the config file, or `:pad` shows it. |
| `padding V H (shown as CV CH: the terminal is too small for more)` | The terminal is too small for the padding. |
| `padding V H (from --padding, not saved)` | `:pad` shows a padding from `--padding`. |
| `margin N: notes wrap at N cells, and labels, legends and long notes are cut there (saved in the document)` | `:margin N` set the margin. |
| `margin=N: use a number between 12 and 200, or auto` | `:margin N` got a number out of the range. |
| `margin auto` | `:margin auto` removed the limit. |
| `margin already auto` | The margin had no limit already. |
| `margin auto (the right margin takes what the width leaves)` | `:margin` shows that the margin has no limit. |
| `margin N (notes wrap at N cells, and labels, legends and long notes are cut there; :margin auto removes the cap)` | `:margin` shows the limit. |
| `layout stable: lanes and margin sized for every step, as in presentation mode and exports` | `:layout stable` set the take as the layout. |
| `layout dynamic: the authoring screen grows lanes as steps add arrows (exports stay stable)` | `:layout dynamic` set the layout that grows. |
| `layout stable` or `layout dynamic` | `:layout` shows the layout. |
| `presentation mode (P to exit)` | `:present` started presentation mode. |
| `supervised mode on (N pending)` or `supervised mode off (N pending)` | `:supervise` shows the state. |
| `no agent changes wait` | `:review` found no change to show. |

- The other messages of `:goto` are in [Messages](../document-format.md#messages). They are for an unknown id and a step after the last step.
- In `<id>` and in `usage: :lang ID`, the id is a language id, `LANG`.
- In the `padding` messages, `V H` is the padding in use, and `CV CH` is the padding that fits on the screen.
- `HOW` is `file extension`, `file name`, `shebang`, `modeline` or `default: plain text`.
- In the `move #N added` message, `FILE` ends with `:A-B` when you gave lines. A warning about the move can come before it.
- In `already references FILE`, `FILE` ends with `:A-B` when the document references lines. The `now references` messages give the path alone.
- A `references` message can end with `; following NAME` when follow mode watches the file.
- The other messages of `:link`, `:ref` and `:embed` are in [Messages of follow mode](#messages-of-follow-mode).
- In presentation mode, `:layout` without a value adds `(presentation mode is always stable)`. `:layout stable` and `:layout dynamic` show their usual message.

## Messages of the save commands

| Message | When |
|---|---|
| `saved to FILE` | `:w` saved the open document. |
| `wrote a copy to PATH (FILE is still the file open)` | `:w PATH` wrote a copy. |
| `saved as PATH (autosave on)` or `saved as PATH (autosave off)` | `:saveas PATH` saved to the new file. |
| `autosave on (saved to FILE)` | `:autosave on` saved the changes that were not saved. |
| `autosave on` | `:autosave on` started autosave, or `:autosave` shows that it is on. |
| `autosave off (:w saves explicitly)` | `:autosave off` stopped autosave. |
| `autosave off` | `:autosave` shows that autosave is off. |
| `autosave off (unsaved changes: :w)` | `:autosave` shows that autosave is off and that changes are not saved. |
| `PATH exists (add ! to overwrite)` | The file exists and the command has no `!`. |

When `:q` refuses, the status bar shows one of these messages:

| Case | Message |
|---|---|
| Autosave off | `unsaved changes (autosave off): :w saves to FILE, :saveas PATH saves elsewhere, :q! quits without saving` |
| No-save mode | `unsaved changes (--no-save): :saveas PATH saves them elsewhere, :q! discards them` |
| After a failed save | `unsaved (read-only): :q! quits without saving, :saveas PATH saves elsewhere (FILE)` |

## Messages of follow mode

| Message | When |
|---|---|
| `following FILE` | `:follow on` started follow mode. |
| `following FILE; step N loads a save` | Follow mode started while a `code.patch` is applied. A save loads at step `N`. See [Moves](../../guides/follow-the-editor.md#moves). |
| `already following FILE` | Follow mode was on already. |
| `FILE reloaded (#N): K annotations kept` | A reload kept all the annotations. |
| `FILE reloaded (#N): K annotations kept, D dropped (text gone; u undoes)` | A reload dropped `D` annotations. |
| `step N loads the change to FILE` | You saved the file while a `code.patch` is applied. The change waits for step N. |
| `FILE removed; waiting for it to come back` | The file is not on disk. |
| `FILE is back; following it` | The file is on disk again. |
| `FILE can be read again; following it` | During a pause, Tiza could not read the file. Now it can, and the file holds the code on screen. |
| `cannot follow FILE: not valid UTF-8 (keeping the previous content)` | The file is not text that Tiza can read. |
| `nothing to follow: the code is embedded (use :link path to reference a file)` | `:follow on` found embedded code. |
| `the .tiza already references FILE` | `:link` with no path found the code reference. |
| `the .tiza now references FILE` | `:link PATH` or `:ref PATH` found the same code in the file. |
| `FILE is shown by a code.link at step N, with annotations of its own: delete that link first to reference it` | `:link PATH` or `:ref PATH` named a file that a `code.link` shows. |
| `undone: reload #N` | `u` undid a reload. |
| `follow is off: :follow on reloads FILE` | `u` brought back the code of the file. |
| `follow off` | `:follow off` stopped follow mode. |
| `follow off: the change to FILE is not loaded` | `:follow off` stopped follow mode while a change waits. |
| `follow off: cannot watch DIR: ERROR` | The file watcher could not start. |
| `following FILE (N reloads)` | `:follow` shows that follow mode is on, with the number of reloads. |
| `follow off (:follow on to reload when the file is saved)` | `:follow` shows that follow mode is off. |
| `follow off: WHY` | `:follow` shows why follow mode is off. |

- `N` in `step N loads a save` and `step N loads the change to FILE` is the step before the first patch. An earlier step loads the change too.
- `already following FILE` gets the same `; step N loads a save` end while a patch is applied.
- `DIR` is the folder of the file. `ERROR` is the error of the system, or `the watcher did not start`.
- In `following FILE (N reloads)`, one reload gives `1 reload`. After the first reload, `, last at T` comes before the `)`. While a patch is applied, `; paused; loads at step N` also comes before the `)`, with ` or earlier` after `N` when `N` is not 0.
- `WHY` is the reason. For example, the code on screen is not the code of the file, or the file watcher stopped.

## Usage messages

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
| `usage: :set ID key=value…   (label, color, step; callouts: text, title; arrows: stroke, route, heads, start, end; boxes/callouts: border, corners, fill; highlights/marks: emphasis)` | `:set` |
| `usage: :del ID [ID…]` | `:del` |
| `usage: :clear  \|  :clear N  \|  :clear .  (all, one step, the current step)` | `:clear` |
| `usage: :step N  \|  :step ID N` | `:step` |
| `usage: :step ID N` | `:step`, when `N` in `:step ID N` is not a number |
| `usage: :goto ID  (a named step, or a step number)` | `:goto` |
| `usage: :focus A-B  \|  :focus   (clear)` | `:focus` |
| `usage: :lang LANG  \|  :lang auto  \|  :lang   (one language, such as rs, ts or py)` | `:lang` |
| `usage: :pad V [H] \| :pad default   (blank cells around the drawing; :pad 0 0 removes it)` | `:pad` |
| `` margin X: use `:margin N` (12-200 cells) or `:margin auto` `` | `:margin` |
| `` numbers X: use `:numbers on` or `:numbers off` `` | `:numbers` |
| `` layout X: use `:layout stable` or `:layout dynamic` `` | `:layout` |
| `usage: :link path[:A-B]   (without a path, uses the source file)` | `:link` |
| `usage: :scroll N \| @'text' \| ID \| annotation=ID  [occurrence=N] [align=top\|center\|bottom\|show]` | `:scroll` |
| `align=A: use top, center, bottom or show` | `:scroll` with an `align` that Tiza does not know |
| `usage: :follow [on\|off]` | `:follow` |
| `usage: :autosave [on\|off]` | `:autosave` |
| `usage: :supervise [on\|off]` | `:supervise` |

- `ERROR` is the error of the range, for example `invalid range: 3 - 5: write A-B without spaces`.
- `KINDS` lists the styles of the document and the built-in kinds. `NAMES` lists the color names. `X` is the value that you typed.
- When `:mark` gets a kind and an anchor with no `:`, the usage also explains `L:TEXT`.
- The message for a wrong `:theme` name is in [Messages](../themes.md#messages).
- The usage message of `:mouse` is in [Messages](../../guides/present.md#messages).

## Unknown commands

| Message | When |
|---|---|
| `unknown command: did you mean :NAME?` | No command has this name, and one command name is near it. |
| `unknown command: did you mean :NAME, :NAME?` | Two or three commands are equally near it. |
| `unknown command (try :help)` | No command is near it. |

A name is near when at most two changes make it the name that you typed. A change adds, removes or replaces one letter.
