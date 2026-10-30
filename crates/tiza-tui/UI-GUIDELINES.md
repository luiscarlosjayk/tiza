# Tiza UI guidelines

These rules apply to every surface that Tiza draws around the code: the status bar, the command line, the message line, the panels (help, annotation list, agent review) and the chips in presentation mode. A new surface follows these rules. When a rule and a surface do not agree, change the surface.

The mockups in this file are text. Each cell of a mockup is one terminal cell. They show the layout, not the colors. The section "Colors by role" gives the colors.

## 1. The panel

A panel is a box that opens over the code and closes again. The help, the annotation list and the agent review are panels. All panels use one component in the code (`panel.rs`).

```
  ╭─ Keys and commands ────────────────────────────────────────── esc close ─╮
  │ / type to filter keys and commands                                       │
  │                                                                          │
  │ Move                                                                     │
  │ j/k  ↓/↑      Move the cursor one line down or up.                       │
  │ h/l  ←/→      Move the cursor one column left or right.                  │
  ╰─ / filter · j/k scroll · g/G top/end ────────────────────── 1-14 of 130 ─╯
```

### 1.1 Frame and title

- The frame has round corners (`╭ ╮ ╰ ╯`), as the boxes on the code do.
- The title is on the top border, at the left, after `╭─ `.
- The title is a noun phrase in sentence case: `Keys and commands`, `Annotations · 7 · Moves · 6`, `Agent changes · 2 waiting`.
- A count goes after the title, after ` · `.
- Size: the width is the terminal width minus 4, from 40 to 100 cells. The panel is centered.
- The height is the terminal height minus 6. The height stays the same while the user filters, so the panel does not jump.

### 1.2 Close chip

- The close chip is on the top border, at the right: `esc close`.
- When a filter is set, the chip changes to `esc clear`.
- When the panel only hides and can open again, the chip is `esc hide` (agent review).
- `q` also closes a panel, and so does the key that opened it (`?`, `L`).

### 1.3 Footer

- The footer is on the bottom border.
- At the left, it shows the keys of this panel: `key word`, with ` · ` between them. Show at most five keys, the most useful first.
- At the right, it shows the position: `1-30 of 124` (rows) or `3 of 13` (selected row) or `14 matches` (filter).
- When the footer does not fit, remove keys from the right. The close key stays because the chip shows it.
- A panel whose keys all matter gives shorter forms of its footer, for example `y/n one · a/d all · j/k` in the review. The first form that fits shows, so that the keys stay at every width that a panel takes.

### 1.4 Filter line

- Row 1 of the panel is the filter line. It always shows, so the user knows that `/` filters.
- Empty: `/ type to filter …` in the muted color.
- Press `/` to type. The text shows after `/`, with a cursor `▏`.
- While the user types, every printable key goes to the filter. `↑`/`↓` scroll or move the selection. `Enter` keeps the filter and stops the typing. `Backspace` and `Ctrl-U` edit it.
- `Esc` clears the filter. A second `Esc` closes the panel.
- The filter matches text in the key, the description and the section name. It ignores case. It is a substring match, not a fuzzy match.
- Matching rows keep their section header.

### 1.5 Rows

- Section header: one row, accent and bold, with no indent. One blank row comes before each section, except the first.
- Sub-header: two cells of indent, muted and bold.
- Two-column row: the key column, two blank cells, then the description.
- A key never wraps. When a key is wider than its column, it takes its own row and the description starts on the next row, in the description column.
- A long description wraps inside its own column.
- Table row (list, review): fixed columns, two blank cells between them. Only the last text column is cut. It ends with `…`.

### 1.6 Selected row

- A panel with a selection shows `▸ ` at the left of the selected row. The other rows have two blank cells there.
- The selected row has the `highlight_bg` background across the full content width.
- `j`/`k` and `↓`/`↑` move the selection. Headers and blank rows are skipped.
- The panel scrolls so that the selected row is always visible.

### 1.7 Narrow terminals

- When the content width is less than 64 cells (a terminal of about 70 columns or less), each row stacks: the key on one row and the description below it, with 4 cells of indent.
- In a table, the last column goes to its own row, with 6 cells of indent.

```
  ╭─ Keys and commands ────────────────────── esc close ─╮
  │ / type to filter                                     │
  │                                                      │
  │ Move                                                 │
  │ j/k  ↓/↑                                             │
  │     Move the cursor one line down or up.             │
  │ h/l  ←/→                                             │
  │     Move the cursor one column left or right.        │
  │ w  b  e                                              │
  │     Go to the next word, the previous word or the    │
  │     end of the word.                                 │
  │ 0  $                                                 │
  │     Go to the start or the end of the line.          │
  │ g/G                                                  │
  │     Go to the first or the last line. Home and End   │
  │     do the same.                                     │
  ╰─ / filter · j/k scroll · g/G top/end ── 1-14 of 211 ─╯
```

## 2. Keys and commands

- Write keys as the user presses them: `j`, `G`, `Esc`, `Enter`, `Tab`, `S-Tab`, `Ctrl-C`, `PgDn`, `←`.
- In panel footers and chips, keys are lower case (`esc close`, `enter go`), as in herdr. In the help table and in messages, use the names above.
- A pair is two keys with one meaning in two directions. Join them with `/`: `j/k`, `n/p`, `+/-`.
- Alternatives for the same action are separated by two blank cells: `n  PgDn`.
- A `:` command shows its full syntax, as the user types it: `:box A-B [label]`.
  - Literal text is bold.
  - A placeholder is in upper case (`ID`, `KIND`, `PATH`) or is a lower-case word (`text`, `label`, `path`). It has the `note` color.
  - Square brackets mean that the part is optional. `|` separates the values to choose from.
- Use the same key with the same meaning in every panel:

| Key | Meaning in every panel |
|---|---|
| `j/k`, `↓/↑` | Scroll, or move the selection. |
| `g/G`, `Home/End` | Go to the first or the last row. |
| `PgDn/PgUp`, `Space` | Scroll one page. |
| `/` | Start the filter. |
| `Enter` | Do the main action on the selected row. Without a selection, close. |
| `x` | Delete the selected row. `u` undoes it. |
| `Esc` | Go back one level: clear the filter, then close. |
| `q` | Close. |

## 3. Colors by role

Use only the roles of the theme. Never write a color value in the code of a surface.

| Element | Role | Dark | Light |
|---|---|---|---|
| Panel background | `cmd_bg` | `#1D2026` | `#FBFAF7` |
| Frame, title, section header, `/` of the filter | `accent` | `#F08B5B` | `#B7410E` |
| Key column, literal command text, typed filter | `fg`, bold | `#E8E6E1` | `#1F2328` |
| Description | `fg` | `#E8E6E1` | `#1F2328` |
| Placeholder in a command (`ID`, `text`) | `note` | `#E2C27A` | `#7A5A00` |
| Sub-header, footer words, position, empty filter, file names in the list | `muted` | `#8A919E` | `#6B7079` |
| Selected row background | `highlight_bg` | `#3A2A22` | `#F3DED2` |
| Selection mark `▸`, `· current` | `accent`, bold | | |
| Warning in a row (`! unanchored`, `! stale`), error message | `error`, bold | `#F47C6F` | `#B42318` |
| Status bar | `status_bg`, `status_fg` | `#272B33` | `#E8E6DF` |

With the `terminal` theme, the roles map to the terminal's own 16 colors. The rules stay the same.

## 4. Spacing

- One cell of padding inside the frame, left and right.
- One blank row after the filter line.
- One blank row between sections.
- Two blank cells between columns.
- Two blank cells between the margin of the screen and the panel at 80 columns.

## 5. Truncation

- Cut text at the end, with `…`. Never cut in the middle of a key or a command.
- The cut counts terminal cells, not bytes or characters. Cut on whole graphemes (the code has `cut_to_width`).
- Never cut the part of a message that tells the user what to do. The status bar already keeps the tail of a message (`fit_message`). Panels follow the same rule.
- In the status bar, the keys of a hint stay. The label goes first.

## 6. Wide characters

- A label can hold wide characters: CJK text, emoji, a mark glyph. Count them as two cells.
- Never put half of a wide character at the end of a column. Leave the cell blank.
- Show control characters as their pictures (`printable_line`), as the code does.
- New glyphs in the UI chrome must be one cell wide in every common terminal. Use the glyphs Tiza already draws: `╭ ╮ ╰ ╯ ─ │ ▸ · … ⟳ ▏`. Do not add `●`: some terminals draw it two cells wide.
- A glyph with an emoji form (`⚠ ↪ ⏸ ▶`) can be two cells wide in some terminals. Tiza measures each one at start (`appearance::PROBED`) and shows its plain form where it is wide. A new glyph of that kind goes in `PROBED` with a plain form, and its text goes through `App::plain`.

## 7. Empty states

An empty panel says why it is empty and what to do next. Use one or two short sentences.

```
  │ no key or command matches «zzz»                                          │
  │                                                                          │
  │ Press Esc to clear the filter.                                           │
```

| Panel | Empty text |
|---|---|
| Help, filter with no match | `no key or command matches «zzz»` and `Press Esc to clear the filter.` |
| List, no annotations | `no annotations yet` and `Select text with v, then press m, n, b or a.` |
| List, filter with no match | `no annotation or move matches «zzz»` |
| A step with no annotations | `no annotations` (muted, under the step header) |
| Review, nothing waits | Do not open. The message line says `no agent changes wait`. |

## 8. Messages

Messages show on the message line (the right side of the status bar) and, in presentation mode, on the last row for 2 s.

- Use ASD-STE100 Simplified Technical English. One idea per sentence. Active voice. At most 20 words in an instruction.
- A confirmation names what changed: `a3 deleted (u to undo)`.
- An error says what went wrong, then gives the exact command that fixes it, as the user can type it:
  - `unknown kind «nope»: try :mark warn 3:zz or :style nope glyph=G`
  - `line 99 is outside the code (lines 10-36): try :note 36 text`
- Repeat the user's own values in the fix (`3:zz`), so the user can edit and run it again.
- Quote user text with `« »`.
- Do not show a list longer than the room. Give the command that shows the list (`:style` lists the kinds).
- The message keeps its tail (`(step 2, press n)`) when it is cut.

## 9. Minimum terminal size

- Tiza draws the code at any size.
- A panel needs 40 columns and 12 rows. In a smaller terminal, the panel does not open. The message line says `the help needs 40×12 or more`. In a terminal narrower than 60 columns, it says only `needs 40×12`, so that the bar can show all of it.
- When the terminal becomes too small while a panel is open, the panel closes and the message line gives the same message.
- The two-column layout needs a content width of 64 cells. Below that, rows stack (1.7).
- The status bar removes optional chips first: theme, cursor line, annotation count, file, size. It keeps the mode, the step and the message.
- In selection mode, the keys of the selection are the most useful hint. They take the room of the optional chips, and a warning keeps only its sign `!`. When they still do not fit, the hint becomes shorter, and the keys stay to the last.
- An error message is about the last key. When it does not fit, the status bar removes the optional chips and the step, and a warning shows only its sign `!`. The mode and the state of the document (save, follow, supervised) stay. The chips come back with the next key.

## 10. Presentation mode

Presentation mode (`P`) shows only the code and the annotations. It hides:

- the status bar;
- the hints and the selection mark `▸`;
- the cursor.

It keeps, on the last row:

- the step counter `2/5` at the right, in the muted color, when the take has more than one step;
- the chip `⏸ 2 to review · :review` at the left, when agent changes wait;
- the command line while the user types after `:`, and its result for 2 s.

A panel opens in presentation mode only when the user opens it. A panel never shows in an export, a GIF or a recording made by Tiza.

In presentation mode, `←`/`→` change the step. A panel must not use `←`/`→`, so that a presenter's clicker cannot change the panel by mistake.

## 11. DX checklist

Tiza's users are developers who use the keyboard. Check each new surface against this list.

- [ ] The surface shows how to leave it (the chip) and the keys that work in it (the footer).
- [ ] The keys are the keys developers know from vim, lazygit, k9s, helix and herdr. Nothing new where a known key exists.
- [ ] A key has the same meaning in every panel (section 2).
- [ ] From the help, the user finds any key or `:` command with `/` and a few letters.
- [ ] The help shows each `:` command as the user types it.
- [ ] An error says what went wrong and gives the exact command that fixes it.
- [ ] The surface opens in the next frame. It reads no file and starts no process. It does not flicker.
- [ ] Nothing opens over the code while the user types. A waiting item shows as a chip.
- [ ] The surface works at 80×24, at 200 columns, and at 60 columns with stacked rows.
- [ ] All text follows section 8.

## 12. How to add a surface

1. Use the panel component. Give it a title, a chip, rows and footer keys. Do not draw a frame by hand.
2. Put each new key in the key table (`keys.rs`). The help reads the same table, and a test checks that the table and the key handler agree.
3. Use only theme roles for colors (section 3).
4. Write the empty state (section 7) and the messages (section 8).
5. Add a QA test that captures the surface at 80, 120 and 60 columns.

## 13. Inventory of the surfaces

| Surface | Where in the code | What these rules change |
|---|---|---|
| Status bar | `draw_status` in `ui.rs` | Hints use `key word` with ` · `. A selection hint keeps its keys and cuts the label first. |
| Command line | `draw_command_line` | No change. Completions keep the muted color. |
| Message line | right side of the status bar, `draw_present_message` | Errors give the fix (section 8). |
| Help (`?`) | `keys.rs`, `panel.rs` and `draw_panel` | Panel with sections, two columns and a filter. |
| Annotation list (`L`) | `list.rs`, `panel.rs` and `draw_panel` | Panel grouped by step, with columns and a selection. |
| Agent review (`:review`) | `list.rs`, `panel.rs` and `draw_panel` | Panel with a selection. The keys go to the footer. |
| Presentation chips | `⏸ N to review`, step counter | Same text as the status bar chip. |
