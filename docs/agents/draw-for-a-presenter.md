# Draw for a presenter

Use this page to draw annotations on the code that a presenter has open in Tiza. The person who writes the instructions of an agent can use it too.

Tiza is a terminal tool that draws annotations on code. To install it, see [Install](../guides/install.md). To connect your client, see [Connect an agent](setup.md). Ask the presenter to run `tiza FILE` in a terminal. This starts the live instance that you need.

A presenter can start with a prompt like this one:

```text prompt
Use Tiza to explain the code that I have open in it.
Draw one step for each idea, with short labels.
Then go back to step 0 and tell me how many steps there are.
```

## Draw the annotations

1. Call `status`. It tells you that an instance listens, and it gives `cols` and `rows`. It also gives `warnings` when there are some.
2. Call `get_code`. Read the line numbers, and pick exact texts for the anchors.
   - The code can hold characters that you cannot see, for example a zero-width space. The presenter sees `⬚` for each one. After the code, a note names them, for example `# line 3: U+200B zero width space, col 5`.
   - For a text with such a character, put the real character in the `find`, for example `"ad\u200bmin"` in JSON. To point at the character itself, use `line` and the `col` of the note. A `find` with only the joiner of `ad` U+200D `min` fails, because the joiner is part of the character `d`.
3. Add the annotations in the order of the explanation. Give each one the same `step` as the one before, or a higher one.
4. Call `set_step` with `0`. Step 0 shows only the code, so the take can start there.
5. Call `check`. Fix each error. The presenter decides about each warning.
6. Call `render_text`. It gives the take as the audience sees it.
7. Let the presenter go through the steps with `n` and `p`, or call `next_step`.

The instance holds the document. Do not edit the `.tiza` file while the instance has it open. `status` can give these `warnings`:

| Warning | What to do |
|---|---|
| `N labels cut at C columns: IDS` | Make those labels shorter, or ask the presenter for a wider terminal. |
| `labels go under the lines at C columns` | Keep the labels short. |
| `code needs N columns at C columns` | Ask the presenter to widen the terminal to `N` columns, or to open only the lines that you need. |

- When `N` is 1, the first warning says `label`.
- In `IDS`, ` (not drawn)` follows the id of a label that Tiza does not draw. The list gives at most 5 ids, then `…`.
- `warnings` also has the problems of the [named steps](../reference/document-format.md#entries-that-tiza-does-not-use).

## Give the anchors

- Give an anchor as `{"find": "exact text"}`. Add `line` or `occurrence` when the text occurs more than once. See [Anchor](../reference/mcp-tools.md#anchor).
- When a tool fails with `text_not_found`, read the code again. Do not guess a column.
- For a range of lines, give `find_start` and `find_end`, or give the lines. Use a marker that occurs only where you mean it. See [Range](../reference/mcp-tools.md#range).

## Pick the tool

| To | Use |
|---|---|
| Put a border around a function or a block | `add_box` |
| Add a short sentence to one line | `add_note` |
| Explain a range of lines in two or three sentences | `add_callout` |
| Show where a value goes | `add_arrow` |
| Give a span a meaning, such as `move` or `read` | `add_mark` |
| Color a span that has no special meaning | `highlight` |
| Dim the code and the annotations outside the lines that you name | `add_spotlight` |
| Point at a line while you talk | `point_at` |

- `list_kinds` gives the [kinds](../reference/document-format.md#kinds) of a mark. For a style that many marks use, call `define_style` once. For one mark only, give `style`.
- `list_kinds` also gives the personal styles of the presenter, in `personal`. Use one as `kind`, as any other kind. The first mark that uses it copies the style into the document, so `scene.get` then shows it in `styles`.
- Do not read the config file of the presenter. `status` gives the values that apply, for example `numbers` and `autosave`.
- Give each variable its own `color`. Use the same color for every annotation about that variable.
- For arrows, use `solid` for a move of ownership, `dashed` for a shared borrow and `thick` for a mutable borrow.
- Use the `underline` emphasis to show a span and keep its syntax colors. Use `strike` for a line that goes away.
- When the margin has no room, Tiza draws a callout with `placement` `margin` or `auto` below its last line.

## Example

This example is for `examples/ownership.rs`, with no annotations. The tools give the ids `a1` to `a6` in order. The last call changes the arrow `a2`. On line 3, the first `s` is the last letter of `takes`. Thus the anchor of the mark has `"occurrence": 2`.

```json
{"name":"add_mark","arguments":{"kind":"move","at":{"line":3,"find":"s","occurrence":2},"label":"s moves into takes","step":1}}
{"name":"add_arrow","arguments":{"from":{"line":2,"find":"s"},"to":{"line":15,"find":"s: String"},"label":"move","step":1,"color":"orange"}}
{"name":"add_note","arguments":{"line":4,"text":"s no longer owns it: using it here does not compile","step":2}}
{"name":"add_box","arguments":{"from_line":15,"to_line":17,"label":"takes owns s and frees it at the end","step":2}}
{"name":"add_arrow","arguments":{"from":{"line":7,"find":"&t"},"to":{"line":19,"find":"s: &String"},"label":"shared borrow","stroke":"dashed","step":3}}
{"name":"add_callout","arguments":{"from_line":15,"to_line":25,"title":"three ways to pass a value","text":"takes(s) moves the String. len(&s) borrows it read-only. push(&mut s) borrows it mutably.","placement":"auto","step":3,"color":"cyan"}}
{"name":"update_annotation","arguments":{"id":"a2","stroke":"thick","head_end":"bar"}}
```

## Check that the annotation shows

Each add tool and `update_annotation` reply with the place of the annotation on the screen of the presenter. The fields and the messages are in [Reply of an annotation tool](../reference/mcp-tools.md#reply-of-an-annotation-tool).

- When `visible` is `false`, `hidden` tells why. The annotation is in the document. Do not add it again.
- The status bar tells the presenter that the annotation is hidden. Do not repeat this in the chat, unless the presenter asks. See [Changes that you cannot see](../guides/supervise-an-agent.md#changes-that-you-cannot-see).
- Do not change the step on screen to show it, unless the presenter asks.
- For a label that Tiza cuts, and for the width of the reply, see [Reply of an annotation tool](../reference/mcp-tools.md#reply-of-an-annotation-tool).

## Check the document

`check` gives the report of `tiza check --json` for the live document. See [Check a live instance](../guides/check-before-you-record.md#check-a-live-instance).

- `min_clean_width` is the narrowest width with no cut label. A code line can still be cut at that width. Then a `clipped_code` warning gives the width from which every code line fits. Export at the wider of the two widths, or make the labels that the report names shorter.
- `check` only reads.
- The report is about the `revision` in its reply. When a later change returns a newer `revision`, call `check` again.
- At most one `check` runs and one waits. After `busy`, call `check` again. See [Error codes](../reference/socket-api.md#error-codes).

## Fix or clear annotations

To change an annotation and keep its id, call `update_annotation`. See [`update_annotation`](../reference/mcp-tools.md#update_annotation).

- For a callout, `label` changes the title. No tool changes the text of a callout.
- `clear_annotations` deletes all the annotations, or the annotations of one `step`.

## Point while you talk

`point_at` puts the live pointer on a line or on a range of lines. The other code dims. The view scrolls to the line, and the cursor of the presenter goes there too. `clear_pointer` or the `f` key of the presenter turns it off.

- The presenter and you share one pointer. The rules are in [The live pointer](../reference/keys.md#the-live-pointer) and in [Fields of `status`](../reference/socket-api.md#fields-of-status).
- Use `point_at` to show a line for a short time. Use `add_spotlight` when the line belongs to a step of the take.

## When the presenter edits the file

- `set_follow` with `"enabled": true` reloads the code each time that the presenter saves the file. The presenter can also type `:follow on`.
- When a `code.patch` is applied at the step on the screen, follow mode is paused and a save waits. The `follow` field of `status` gives `paused`, `pending` and `resume_step`. The reply of `set_follow` also gives the `reason`. Tiza follows only the main file, not a file that a `code.link` shows.
- When you save the file yourself, compare `hash` in the `follow` field of `status` with the sha256 of the file. When they are the same, Tiza read your save. See [Agents and follow mode](../guides/follow-the-editor.md#agents-and-follow-mode).
- Tiza keeps each annotation on its text, and drops it when its text is gone. See [What happens to annotations](../guides/follow-the-editor.md#what-happens-to-annotations).
- After a reload, call `list_annotations`, and add the annotations that you need again.
- A move can become stale after a reload. See [When the presenter edits the file](moves-and-steps.md#when-the-presenter-edits-the-file).
- The rules are in [Agents and follow mode](../guides/follow-the-editor.md#agents-and-follow-mode). The messages are in [Messages](../guides/follow-the-editor.md#messages).

## The screen and the exports

- To change the theme, see [Agents and themes](../reference/themes.md#agents-and-themes).
- `status.margin` is the limit of the right margin, when the document sets one. A long note wraps at it, and takes the rows of the lines under it.
- `status.numbers` tells you whether the line numbers show. Give the real line numbers in your anchors, even when the numbers do not show.
- `render_text` and `export_image` use one layout for all the steps. See [One layout for the take](../reference/exports.md#one-layout-for-the-take).
- During a typing animation, `render_text` gives the step as it is after the typing.
- The extension of the `path` of `export_image` selects the format. See [Where each format works](../reference/exports.md#where-each-format-works).
- No tool writes an MP4. Tell the presenter to run `tiza render --out demo.mp4`, which needs `ffmpeg`.
- When an export takes too long, export fewer steps, a smaller image or one step. See [Errors of `export_image`](../reference/exports.md#errors-of-export_image).

## Secrets and saving

> **Warning:** The audience sees the same code as `get_code` gives, in every export. Only control and invisible characters look different. When the code holds a real secret, tell the presenter before the recording starts. Do not put the secret in a label. See [Checklist](../guides/check-before-you-record.md#checklist).

- An error does not repeat a wrong value of `color`, `border` or another field with a fixed set of values. An error repeats a value of the wrong type, cut at 40 characters. See [Errors](../reference/mcp-tools.md#errors).
- No tool publishes or uploads a file. See [Agents](../concepts/privacy-and-network.md#what-agents-send).
- Tiza saves each change to the `.tiza` file at once. In no-save mode, with autosave off or with a read-only document, the changes stay in memory. See [When changes are not saved](../concepts/saving.md#when-changes-are-not-saved).
- `status.autosave` is `false` in no-save mode and with autosave off. With a read-only document, it stays `true` while the saves fail.
- A `scene.put` of another document, or of changed code of the open document, stops autosave. See [Agents and saving](../concepts/saving.md#agents-and-saving).

## Retries and errors

- Give your own `id`, for example `"id": "move-s"`. Then a second try of the same call fails with `duplicate_id`, and adds no copy. See [Arguments of all the add tools](../reference/mcp-tools.md#arguments-of-all-the-add-tools).
- After `internal`, tell the presenter. Do not try again in a loop. Tiza undid all of the request.
- After `rejected`, see [For the agent](supervised-mode.md#for-the-agent). The other codes are in [Error codes](../reference/socket-api.md#error-codes).
- When a tool is not in your list, search for what you want to draw. See [Find the tools](../reference/mcp-tools.md#find-the-tools).
- When `status` gives `binary_rebuilt`, see [After you rebuild Tiza](setup.md#after-you-rebuild-tiza).

## See also

- [MCP tools](../reference/mcp-tools.md) and [Socket API](../reference/socket-api.md)
- [Change the code between steps](moves-and-steps.md) and [Supervised mode](supervised-mode.md)
- [Control and invisible characters](../reference/characters.md) and [Demo scenarios](../demos/scenarios.md)
