# Socket API

This page describes the local socket of a Tiza instance. It covers the connection, the requests, the methods, the error codes and the commands. The [MCP tools](mcp-tools.md) send the same requests.

## Connect

Each instance listens on a Unix socket on your computer, unless you start it with `--no-socket`. The socket never listens on TCP. See [Privacy and network](../concepts/privacy-and-network.md#the-network).

Tiza selects the socket in this order:

1. `--socket PATH`.
2. `$TIZA_SOCKET`.
3. `SESSION.sock` in the runtime folder. `SESSION` is `--session NAME`, else `$TIZA_SESSION`, else `default`.

- For the runtime folder, the permissions and what `--socket` changes on disk, see [The runtime folder](../concepts/privacy-and-network.md#the-runtime-folder).
- An empty variable is the same as a variable that is not set.
- An empty `--session` is not the same as no session. It gives the socket `.sock`.
- In a session name, each character that is not an ASCII letter, a digit, `-` or `_` becomes `_`.
- Every command that uses the socket takes `--socket` and `--session`: the TUI, `tiza mcp`, `tiza api`, `tiza scene`, `tiza render`, `tiza next`, `tiza prev`, `tiza goto` and `tiza close`.
- A session has one instance. To run several instances, give each one its own session.
- When an instance listens on the socket already, a second instance does not start. It exits with code 1, and its message tells how to close the first one (`tiza close`) or to use another session. See [Messages](cli.md#messages).

When the socket cannot start for another reason, the instance starts without a socket. The status bar shows `local API disabled: ` and the reason.

- After `local API disabled: ` comes a message of Tiza or an error of the system.
- The messages of Tiza are in [Messages](../concepts/privacy-and-network.md#messages).

## Requests and responses

Send one JSON object on one line. The instance answers with one JSON object on one line.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `id` | any JSON value | Required | A value that the response gives back, for example `1` or `"a"`. |
| `method` | string | Required | The name of the method, for example `annotation.add`. |
| `params` | object | No default | The parameters of the method. |

The response has `id` and one of these fields:

- `result`: the result of the method.
- `error`: an object with `code` and `message`. Use the `code` in your client. The `message` is for a person.

For example, when `TMPDIR` is set and `XDG_RUNTIME_DIR`, `TIZA_SOCKET` and `TIZA_SESSION` are not set:

```sh
printf '%s\n' '{"id":1,"method":"annotation.add","params":{"type":"note","line":4,"text":"hello"}}' | nc -U "$TMPDIR/tiza/default.sock"
```

```json
{"id":1,"result":{"hidden":"it appears at step 1","id":"a17","revision":4,"shown_step":0,"step":1,"visible":false}}
```

- A connection can send several requests. The instance answers them in the order that they come.
- In supervised mode, a request that waits also stops the requests after it on the same connection, for up to 60 s.
- Thus use one connection for each request, as `tiza api` does.
- The instance skips an empty line.
- A line that is not JSON gets the error `invalid_json`. Its `id` is `null` when the instance cannot read it.
- A line without `id`, or without `method` as a string, also gets the error `invalid_json`.
- A request without `params` is the same as `"params": null`.
- These methods refuse `null` with `invalid parameters:`: `annotation.add`, `annotation.update`, `annotation.delete`, `scene.clear`, `scene.put`, `scene.patch`, `move.add`, `move.delete`, `step.set`, `step.goto`, `pointer.set` and `check`.
- For `scene.clear`, `scene.patch` and `check`, send `{}` when you have no parameter to give. `pointer.set` with `{}` fails, because it needs a place.
- `style.set`, `follow.set` and `supervise.set` also refuse `null`, because they need a field.
- The other methods take `null` as no parameters. Thus `focus.set` clears the focus and `theme.set` changes the theme between dark and light.
- A result can get new fields in a new version. Ignore a field that you do not know.
- `status` and `api.schema` give `protocol`, the version of the protocol. It is `1`.
- Each command that sends a request waits 75 s for a response. This includes `tiza mcp`, `tiza next`, `tiza prev`, `tiza goto` and `tiza render` without a file.
- `tiza close` gives the instance 10 s to answer, and then 10 s more to exit.
- A request in supervised mode waits at most 60 s.

## Methods

`api.methods` gives each method with its description. The MCP tool column names every tool that sends the method.

### Instance

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `ping` | None | Gives `"pong"` when the instance answers. | None |
| `status` | None | Gives the state of the instance. | `status`, `list_annotations`, `export_image` |
| `api.methods` | None | Gives the list of the methods with a description of each one. | None |
| `api.schema` | None | Gives the JSON Schema of each method and of the request and response lines. | None |
| `app.quit` | `force` (optional) | Closes the instance, as `:q` does. See [`app.quit`](#appquit). | None |

### Read

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `code.get` | None | Gives the code on screen with line numbers, the file and the window that it comes from, and the characters that the screen does not show as they are. | `get_code` |
| `scene.get` | None | Gives the live document, with the code embedded, and the current step. | `export_image` |
| `scene.list` | None | Gives one line for each annotation, in the order of the document. | `list_annotations` |
| `render.text` | `width` | Gives the take at the current step as plain text. | `render_text` |
| `check` | `width` | Gives the report of `tiza check --json` for the live document. | `check` |

- `width` of `check` is the columns of code, from 20 to 400, as `render.text` takes it. The default is the `width` of the config file, else 100. `tiza check --width` takes the width of a terminal, and its report also has `terminal`. See [Check report](cli.md#check-report).

### Annotations

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `annotation.add` | `type` and its fields | Adds an annotation. | `add_box`, `add_arrow`, `add_note`, `add_callout`, `highlight`, `add_mark`, `add_spotlight` |
| `annotation.update` | `id` and the fields to change | Changes an annotation and keeps its id. | `update_annotation` |
| `annotation.delete` | `id` | Deletes an annotation. | `delete_annotation` |
| `scene.clear` | `step` | Deletes all the annotations, or the annotations of one step. | `clear_annotations` |
| `style.set` | `name`, `glyph`, `legend`, `color` | Defines a style, or changes it. | `define_style` |
| `style.list` | None | Gives the styles of the document, the personal styles of the config file that the document does not have, the built-in kinds and the aliases. | `list_kinds` |

### Document

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `scene.put` | `document` | Replaces the live document. | None |
| `scene.patch` | The fields to change | Changes some fields of the live document. | None |

### Moves

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `move.add` | A [move](document-format.md#moves) | Adds a move at a step. | `add_move` |
| `move.delete` | `index` | Deletes a move by its index, from 1. | `delete_move` |
| `move.list` | None | Gives the moves, the unanchored annotations and the view on screen. | `list_moves` |

### Steps

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `step.set` | `step` | Shows a step. In presentation mode, to the step on screen it does what `step.goto` does. | `set_step` |
| `step.next` | None | Shows the next step. In a talk, at the last step it opens step 0 of the next scene. | `next_step` |
| `step.prev` | None | Shows the previous step. In a talk, at step 0 it opens the last step of the scene before. | `prev_step` |
| `step.goto` | `id` | Goes to a named step or a step number, and applies every move up to that step. To the step on screen, it puts the cursor and the view back where the last `scroll`, `cursor` or `code.link` move up to that step puts them. | `goto_step` |

### Talk

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `talk.get` | None | Gives the talk: the master, the scene on screen and each entry of its list. | `talk_status` |
| `talk.goto` | `scene`, `step`, `force` | Opens a scene of the talk by its position or its name, at a step. | `goto_scene` |
| `talk.put` | `scenes`, `force` | Replaces the list of scenes: their order, `skip` and names. It writes the list in the master. | `set_talk_scenes` |
| `talk.insert` | `file`, `at`, `scene`, `name`, `force` | Adds a scene to the list. With `scene`, it writes the file of the scene first. | `insert_scene` |

A talk is a master `.tiza` that lists other `.tiza` files. See [Talk methods](#talk-methods) for the rules. The other methods act on the scene on screen.

### Focus and pointer

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `focus.set` | `from_line`, `to_line` | Dims the lines outside a range, or clears the focus. | `set_focus` |
| `pointer.set` | `line`, `find`, `from`, `to` | Puts the live pointer on a line or a range of lines. | `point_at` |
| `pointer.clear` | None | Turns the live pointer off. | `clear_pointer` |

### Theme and follow mode

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `theme.set` | `theme` | Changes the theme. | `set_theme` |
| `follow.set` | `enabled` | Turns follow mode on or off. | `set_follow` |
| `follow.status` | None | Gives the state of follow mode. | None |

### Supervised mode methods

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `supervise.set` | `enabled` | Turns supervised mode on or off. | None |
| `supervise.status` | None | Gives the state of supervised mode and the changes that wait. | None |

The arguments of the MCP tools are in [Arguments](mcp-tools.md#arguments). The socket uses the same names, with the differences in [Differences from the MCP tools](#differences-from-the-mcp-tools). The [schema](#schema) gives the full contract of each method: its parameters and all the fields of its result.

## Differences from the MCP tools

- `annotation.add` takes `type`, one of the seven [annotation types](document-format.md#annotation-types), and the fields of that type.
- `annotation.add` takes `file`, the file that the annotation belongs to, as a `code.link` names it. Without `file`, or with `null`, the annotation goes to the file on screen at its step.
- An arrow takes `heads`, an object `{start, end}`, in place of `head_start` and `head_end`. A missing end gets its default.
- An anchor also takes `end`, the grapheme after the span, with `line` and `col`. Without `end`, a span goes to the end of the line.
- Tiza uses `end` only when the anchor has no `find`. The `from` and `to` of an arrow ignore `end`.
- `annotation.update` takes the new place in `move`: `{"id": "a1", "move": {"at": {"find": "s: String"}}}`. `move` takes `at`, `from`, `to`, `from_line` and `to_line`.
- `annotation.update` takes `shape`, which replaces the whole shape of the annotation and its `until`. A `shape` without `until` removes the `until`.
- `annotation.add` and `annotation.update` take `until`, the last step that shows the annotation. In `annotation.update`, an absent `until` keeps it, and `null` removes it. A request with `until` and also a `shape` fails with `invalid_argument`: give `until` in the shape or alone.
- `annotation.update` takes `heads` with one end only. The other end stays.
- `annotation.add` and `annotation.update` take `effect`, the experimental [enter effect](document-format.md#enter-effects-experimental): `draw`, `fade` or `null`. In `annotation.update`, an absent `effect` keeps the effect, and `null` removes it.
- `draw` on an annotation without a stroke fails with `invalid_argument`. `annotation.update` checks it when the request has `effect`, `shape` or `emphasis`. Other updates keep the effect, also on an annotation from a file.
- An annotation with an `until` before its `step` never shows. Thus a request that makes one fails with `invalid_argument`. `annotation.add` checks it at the step that the annotation gets, also when the request has no `step`. `annotation.update` checks it when the request has `step`, `until` or `shape`. An update without them does not check it. This is also true for an annotation from a file that already has such an `until`.
- `theme.set` without `theme` changes between dark and light, as [Agents and themes](themes.md#agents-and-themes) tells. The MCP tool `set_theme` needs `theme`.

### `scene.clear`

- Send `{}` to clear every annotation, or `{"step": N}` to clear one step. `N` is a whole number from 0 to 4294967295.
- Tiza refuses any other parameters with `invalid_argument`, and changes nothing.
- The presenter can undo a `scene.clear` with `u`.

| Message | When |
|---|---|
| `invalid parameters: step must be a whole number, 0 or more. Leave it out to clear every annotation` | `step` is a string, a negative number, a decimal number, `null`, or more than 4294967295. |
| ``invalid parameters: send a JSON object. `{}` clears every annotation, and `{"step": N}` clears step N`` | The parameters are not a JSON object, for example `null`, an array or no `params`. |
| ``invalid parameters: unknown field `stp`, expected `step` `` | The parameters have a key that `scene.clear` does not know, here `stp`. |

### `scene.put` and `scene.patch`

- `scene.put` takes `document`, a `.tiza` object with its code embedded. The instance cannot read a file reference that you send.
- Tiza refuses a document with a newer `schema` with `unsupported_schema`, and nothing changes.
- The message is the same as when a file opens, for example `.tiza document with schema 99: this version of Tiza reads schema 2 at most`.
- Tiza migrates a document with an older `schema`, as it does for a file. When Tiza saves the document, and in the copy of `scene.get`, it writes the lowest schema that the document needs. See [`schema`](document-format.md#top-level-fields).
- The presenter can undo a `scene.put` with `u`. The undo also gives back the step.
- For the save rules, see [Agents and saving](../concepts/saving.md#agents-and-saving).
- `scene.patch` takes these fields: `step`, `theme`, `padding`, `title`, `numbers`, `margin`, `center`, `styles`, `annotations`, `steps` and `loop`.
- A `step` after the last step becomes the last step.
- `padding`, `title` and `margin` take `null`, which gives back the default.
- `styles` merges the styles by name. A style with the same name replaces the old one.
- `steps` replaces the whole list of [named steps](document-format.md#named-steps). `[]` clears it.
- `annotations` merges full annotations by id. A new id adds an annotation, and the same id replaces one. `{"id": "a1", "deleted": true}` deletes one.
- `deleted` for an id that the document does not have fails with `invalid_argument`.
- A replaced annotation keeps its step without `step`, its file without `file`, and its `until` without `until`. `file: null` is the main file, and `until: null` removes the `until`.
- A new id gets the defaults of `annotation.add`. `file: null` is then the file on screen at its step.
- `scene.put` and `scene.patch` accept an annotation with an `until` before its `step`, as Tiza does when it opens a file. `tiza check` reports it with the error `until`.

For example:

```sh
tiza scene patch '{"step":2,"theme":"light","annotations":[{"id":"a1","deleted":true}]}'
```

Tiza refuses a `scene put` in these saving cases. The error code is `invalid_argument`. The document and the file on disk do not change.

| Message | When |
|---|---|
| `scene.put: the code sent is not what FILE holds now, and follow is on (the screen shows the file as it is): scene.get again; to change the code, edit FILE` | Follow mode is on, and the code sent for the main file is not the text of the file now. |
| ``scene.put: follow reads FILE over the code on screen: a put cannot change `follows` while follow is on. Send `follows` as scene.get gives it, or leave it out`` | Follow mode is on over embedded code, and the put sends a `follows` that names another file, or other `lines`. This is the case while a change waits in a pause, and while the followed file is missing or cannot be read. The same file written another way, for example `./main.rs` or its absolute path, works. |
| `scene.put: follow is paused, and the code on screen is not what FILE holds now: FILE loads at step N or earlier. Send the code as scene.get gives it; to change the code, edit FILE` | Follow mode is paused with a change that waits, and the code sent is not the code on screen. With `resume_step` 0, the message says `at step 0`. A `scene.put` of the code on screen works. See [Agents and follow mode](../guides/follow-the-editor.md#agents-and-follow-mode). |
| `scene.put: FILE cannot be read (ERROR) and the document reads its code from it: put the file back, or :embed to keep the copy loaded` | The open document reads code from a file, the main file or a linked file that Tiza read before. Tiza cannot read that file now. While follow mode is paused, a put of the code on screen works also then, when FILE is the followed file. |

`ERROR` is the read error of Tiza. It ends with the error text of the system.

### Keys that Tiza does not know

These methods refuse a key that they do not know: `annotation.update`, `scene.clear`, `scene.get`, `scene.patch`, `pointer.set` and `supervise.set`. The `move` object of `annotation.update` also refuses one. The error is `invalid_argument`, and its message names the keys that the method knows. `scene.get` knows no key.

A position also refuses a key that it does not know, in every method. These are the positions:

- the `at`, `from` and `to` of `annotation.add`, `annotation.update` and `move.add`;
- the span and the arrow ends of the annotations of `scene.put` and `scene.patch`;
- the `from` and `to` of a `select` move of `scene.put`;
- the `rebind` entries.

For example, `"at": {"line": 3, "start": 4}` fails, because a position calls its first grapheme `col`. A span calls it `start`.

When a method refuses a key that it does not know, the message names a known key that is near. That key is one or two letters from it, or it starts the key (`col` for `column`). For `start`, `col` and `text` in a position or a span, the message names the field of that position:

| Message | When |
|---|---|
| ``invalid parameters: unknown field `colum`, expected one of `line`, `col`, `find`, `occurrence`, `end`. Did you mean `col`?`` | A position has the key `colum`. |
| ``invalid parameters: unknown field `start`, expected one of `line`, `col`, `find`, `occurrence`, `end`. `start` is `col` in a position.`` | A position has the key `start`. |

`move.add` and `scene.put` refuse a key of a move that the action of the move does not know. The message names the first of these keys in alphabetical order, and the keys of the action. When that key is one or two letters from a key of the action, the message names that key. When that key is a key of another action, the message names that action. In `scene.put`, the message also names the move by its number in `moves`. The keys that `move.list` adds to a move (`index`, `summary`, `stale` and `stale_reason`) are not refused. Thus you can send an entry of `move.list` to `move.add` again.

| Message | When |
|---|---|
| ``invalid parameters: unknown field `alig` of a scroll move, expected one of `at_step`, `action`, `line`, `find`, `occurrence`, `annotation`, `align`. Did you mean `align`?`` | `move.add` got a `scroll` move with the key `alig`. |
| ``invalid parameters: unknown field `line` of a code.patch move, expected one of …. `line` is a key of a cursor or scroll move: send the `action` of the move`` | `move.add` got a move with `"action": "code.patch"` and `line`. The MCP tool `add_move` gives it too for `line` with no `action`, because its default action is `code.patch`. |
| ``invalid parameters: move #3: unknown field `alig` of a scroll move, …`` | `scene.put` got a document whose third move has the key `alig`. |

A file with a key that this version does not know opens, and `tiza check` gives a warning (see [Moves from a newer version](document-format.md#moves-from-a-newer-version)).

## Results

| Method | Result |
|---|---|
| `ping` | The string `"pong"`. |
| `status` | The fields in [Fields of `status`](#fields-of-status). |
| `api.methods` | A list of `{method, description}`. |
| `api.schema` | `protocol`, `request`, `response`, and `methods`, a list of `{method, description, params, result}`. |
| `code.get` | `language`, `file`, `view`, `first_line`, `last_line`, `text`, `invisible` and `removed`. See [Invisible characters in `code.get`](#invisible-characters-in-codeget) and [Removed lines in `code.get`](#removed-lines-in-codeget). |
| `scene.get` | `revision`, `step`, `max_step`, `document`, and `stale` and `held` when a move is stale. |
| `scene.list` | A list of strings, one for each annotation. |
| `render.text` | `text`, and `width`, the width of the take. |
| `check` | The report of `tiza check --json`, plus `revision`, the revision that it checked. |
| `annotation.add` | The [reply of an annotation tool](mcp-tools.md#reply-of-an-annotation-tool), without its sentence. |
| `annotation.update` | The same fields as `annotation.add`. |
| `annotation.delete` | `id` and `revision`. |
| `scene.clear` | `removed`, `remaining` and `revision`. |
| `style.set` | `name`, `style` and `revision`. |
| `style.list` | `document`, `personal`, `builtin` and `aliases`. |
| `scene.put` | `revision`, `lines`, `annotations`, `step` and `max_step`. |
| `scene.patch` | The fields of `scene.put`, and the ids in `added`, `replaced` and `deleted` when there are some. |
| `move.add` | `index`, `moves`, `max_step`, `revision`, and `warning` when annotations are not drawn at a step any more. |
| `move.delete` | The same fields as `move.add`. |
| `move.list` | `moves`, `unanchored`, `step`, `max_step`, and `view` when a `code.link` shows a file. |
| `step.set` | `step`, `max_step`, `revision`, `label`, and `id` and `title` of a named step. |
| `step.next` | The same fields as `step.set`. |
| `step.prev` | The same fields as `step.set`. |
| `step.goto` | The same fields as `step.set`. |
| `focus.set` | `focus`, the range in focus or `null`, and `revision`. |
| `pointer.set` | `pointer` and `shown`. |
| `pointer.clear` | The same fields as `pointer.set`, with `pointer` as `null`. |
| `theme.set` | `theme`, `auto` and `revision`. |
| `follow.set` | The fields of `follow.status`, and `message`. |
| `follow.status` | `enabled`, `path`, `reloads`, `reads`, `hash`, `last_reload`, `missing`, `paused`, `pending`, `resume_step` and `reason`, when they apply. |
| `supervise.set` | The fields of `supervise.status`, and `message`. |
| `supervise.status` | `supervised`, `pending`, `accepted`, `rejected` and `timeout_secs`. |

- `message` tells what the method did, in words.
- `label` is the step label: the text of the status bar for the step, for example `step 3/7 · Shared borrow`.
- `pending` of `supervise.status` is a list of `{id, method, summary, age_ms}`. In `status`, `pending` is a number, and `pending` of its `follow` field is `true` or `false`.
- `shown` of `pointer.set` is `false` while the presenter has the command line open. The pointer shows when the command line closes.
- `hash` of `follow.status` is the sha256 of the followed file as Tiza last read it, in lowercase hex, as `shasum -a 256 FILE` prints it. It is the hash of the whole file, also when the code is a range of it. It is `null` when Tiza could not read the file, for example when the file is removed. `reads` counts the reads of the file, also the reads that load nothing. A save of the same bytes does not change `hash`: then `reads` increases. See [Agents and follow mode](../guides/follow-the-editor.md#agents-and-follow-mode).
- The fields of the `check` report are in [Check report](cli.md#check-report).

### Invisible characters in `code.get`

`text` gives the real characters of the code. Some of them do not show on the screen as they are. `invisible` is a list of these characters, in the order of the code. Each item is `{line, col, code_point, name}`, for example `{"line": 3, "col": 5, "code_point": "U+200B", "name": "zero width space"}`.

- `col` is the grapheme, counted from 0, as the `col` of an anchor. A joiner in the grapheme of a letter has the column of that letter. For example, in `ad` U+200D `min`, the joiner has `col` 1.
- The list has each character that the screen shows as `⬚`, and each control character other than the tab. See [Control and invisible characters](characters.md).
- A character that has a function on the screen is not in the list, for example the joiner of an emoji sequence. A tab is not in the list.
- The list is empty when the code has none of these characters.
- A `find` for a text with such a character must hold the real character, for example `"ad\u200bmin"` in JSON. To point at the character itself, use `line` and `col`. See [A `find` inside a character](#a-find-inside-a-character).

### Removed lines in `code.get`

At the step of a `code.patch` move with `diff`, the screen also shows the lines that the move removed. See [A diff](document-format.md#a-diff). `text` does not hold them. `removed` is a list of their groups, in the order of the screen. Each item is `{line, place, lines}`, for example `{"line": 8, "place": "before", "lines": ["    println!(\"{t} has {n} letters\");"]}`.

- `place` is `before` when the group is above line `line`. It is `after` for the group under the last line.
- `lines` is the text of each removed line.
- The list is empty at a step with no removed lines, and while the step types.
- A `find` does not match a removed line. When the text of a `find` is only on a removed line, the error `text_not_found` says `that text is a removed line of the diff at step N`.

### A `find` inside a character

A character (a grapheme) can be more than one code point, for example `d` with a joiner, `e` with a combining accent, or an emoji sequence.

- When a column comes from a `find`, that edge of the text must be on the edge of a grapheme:
  - a span, the `at` of a highlight or a mark, uses the two edges;
  - an arrow end, `from` or `to`, and the `from` of a `select` use the start of the text;
  - the `to` of a `select` uses the end of the text.
- This applies in `annotation.add`, `annotation.update` with `shape` or `move`, `move.add`, `scene.put`, and a `rebind` entry of a span or an arrow end.
- When that edge is inside a grapheme, the method fails with `invalid_argument`. For example, a joiner alone in `ad` U+200D `min` on line 1 gives:

  ```
  text "\u{200d}" starts inside the character "d\u{200d}" at line 1, col 1: put the whole character in the find, or use line and col
  ```

- Put the whole character in the `find`, for example `"d\u200d"` in JSON, or use `line` and `col`.
- When only the line counts, this is not an error: a `find_start` or `find_end` marker, the `at` of a note (in `annotation.update` with `move`, and in a `rebind`), the target of a `code.patch`, and `pointer.set`.
- `annotation.update` refuses a new `shape` when a `rebind` of the annotation would not work with it. An example is a note whose `rebind` has a joiner alone, which cannot become a highlight.
- `occurrence` counts every match, also a match inside a grapheme.

### Fields of `status`

| Field | Meaning |
|---|---|
| `app` | The name of the program, `tiza`. |
| `version` | The version of the instance. |
| `build` | The git commit of the instance. |
| `protocol` | The version of the protocol. |
| `revision` | A number that grows with the changes. See the list under this table. |
| `file` | The file on screen: the path of the main file as Tiza opened it, or the file that the `code.link` gives. |
| `lines` | The number of lines on screen, which is the size of the window of a `code.link`. |
| `first_line`, `last_line` | The first line and the last line of the code on screen. |
| `view` | The `code.link` view on screen, as `FILE` or `FILE:A-B`, when it is not the main file. |
| `annotations` | The number of annotations. |
| `step`, `max_step` | The current step and the last step. |
| `step_id`, `step_title` | The `id` and the `title` of the current step, when it is a named step. |
| `step_label` | The text of the status bar for the step. |
| `auto_advance_ms` | The `auto_advance_ms` of the current step, when it has one. |
| `loop` | `true` when the document loops. |
| `cols`, `rows` | The size of the terminal. |
| `width` | The width of the code area of the screen: `cols` less the side padding on each side, from 20 to 400, or 100 when the size of the terminal is not known. `render.text`, `tiza render`, `:export` and `export_image` use it when they get no width. |
| `warnings` | Warnings about the screen and the named steps, in words. When the file has effects or moves that this version of Tiza does not know, a warning names each one and says that a save does not keep it. It goes away after the first save (see [Moves from a newer version](document-format.md#moves-from-a-newer-version)). |
| `supervised` | `true` in supervised mode. |
| `pending` | The number of changes that wait for the presenter. |
| `autosave` | `true` when Tiza saves each change to the document. |
| `document` | The absolute path of the `.tiza` file, when the file exists on disk. |
| `numbers` | `true` when Tiza shows the line numbers. When the document does not set `numbers`, the config file of the instance sets it. |
| `margin` | The limit of the right margin in columns, when the document sets one. |
| `pointer` | The live pointer as `{line, to, by}`, or `null`. |
| `stale_moves` | The number of moves that do not match the code any more. |
| `save_error` | Why the last save of the document failed, for example a read-only folder, or `null` when the last save worked. |
| `follow` | While follow mode is on: `paused`, `pending`, `resume_step`, `reads` and `hash`, as `follow.status` gives them. |
| `effect` | Experimental. `true` while an enter effect plays on the screen. |
| `talk` | When the instance plays a talk: `master`, `title`, and the scene on screen as `position`, `name`, `shown` and `shown_count`. See [Talk methods](#talk-methods). |

- A field that does not apply is not in the result, except `file`, `pointer` and `save_error`, which are `null`.
- `loop`, `supervised` and `effect` are not in the result when they are `false`. `pending` and `stale_moves` are not in the result when they are 0.
- `by` of `pointer` is `presenter` or `agent`. It tells who moved the pointer last.
- `revision` grows when something changes, by 1 or more. A request that changes nothing keeps it.
- A change is a change of the document or of what the instance shows. Examples are the step, the focus, the theme and supervised mode.
- One request can add more than 1. For example, `theme.set`, `scene.put` and `scene.patch` add 2.
- `scene.patch` with `{}`, and `annotation.update` with only `id`, change nothing. They keep `revision`.
- `pointer.set`, `pointer.clear` and `follow.set` do not change `revision`.
- In follow mode, a reload or a relink changes the document. Thus it changes `revision`.
- Compare `revision` to find a change. Do not count on steps of 1.

### `app.quit`

`app.quit` closes the instance, as `:q` does. `tiza close` sends it.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `force` | boolean | `false` | `true` closes the instance also when that loses work. |

- The result is `{"closing": true}`. The instance sends it, and then exits. The requests after it are not applied: they get `unavailable`.
- Without `force`, the instance refuses to close when that loses work, with `unsaved_changes`. Work is lost when there are changes with autosave off or `--no-save`, changes that Tiza could not save, or stale moves (their marks are not saved).
- When nothing is lost, it closes in each mode: read-only, `--no-save` and autosave off.
- It does not wait for review in supervised mode. Closing does not change the document, and `tiza close` is for a terminal that nobody can see. Each change that waits for review is not applied: its request gets `unavailable`. If the process exits before the connection writes that reply, the client sees only the end of the connection.
- No MCP tool sends it. An agent does not close the instance of a presenter.
- No parameters, or `null`, is the same as `{}`. A key that is not `force` gets `invalid_argument`.
- An `app.quit` that waited more than 10 s in the instance (it was busy) gets `unavailable`, and the instance stays open. `tiza close` stops waiting then, and tells you that the instance did not close.

### Talk methods

A talk is a master `.tiza` file that lists other `.tiza` files, the scenes. See [Talk](document-format.md#talk). The methods of a document (`annotation.add`, `scene.put`, `step.set` and the others) act on the scene on screen. The `talk.*` methods act on the talk.

A scene has a **position**, its place in the list of the master from 1. A skipped entry has a position too, so a `skip` does not change the position of the other scenes. A scene also has a **name**: the `name` of its entry, else its file name up to the first dot. A file name that is all digits, such as `2024.tiza`, gives no name.

`status.talk`:

| Field | Meaning |
|---|---|
| `master` | The absolute path of the master. |
| `title` | The `title` of the talk, when it has one. |
| `position` | The position of the scene on screen. |
| `name` | The name of the scene on screen, when it has one. |
| `shown` | The number of the scene on screen among the scenes that show, from 1, or `null` when the scene on screen has `skip`. |
| `shown_count` | The number of scenes that show. |

- `talk.get` gives these fields and `scenes`, one item for each entry of the master: `entry`, `position`, `name`, `skip` and `steps`.
  - `entry` is the entry as Tiza writes it: the path alone, or `{file, name, skip}` with only the fields that are set. You can send the `entry` values back to `talk.put`.
  - `steps` is the last step of the scene, or `null` for a skipped entry. Tiza does not read a skipped entry. A scene with no annotations, and no moves after step 0, has `steps` 0: in a talk, it is one image. The `max_step` of `status` is the last step of the document alone, so it is 1 for such a scene.
- A reply of `step.set`, `step.next`, `step.prev`, `step.goto` and `talk.goto` in a talk has `scene`, with the `position` and the `name` of the scene on screen. Look at it after `step.next`: after the last step of a scene, `step.next` opens the next scene at step 0.
- The requests of one batch go in order, and each one acts on the scene on screen when it runs. A request after a `step.next` or a `talk.goto` that opens another scene acts on the new scene.

`talk.goto`:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `scene` | integer or string | Required | A position (`7` or `"7"`), or a name. A name goes to the first scene that shows with that name. |
| `step` | integer or string | `0` | A step number or a step `id` of that scene. |
| `force` | boolean | `false` | `true` opens the scene also when the scene on screen has changes that Tiza could not save. They are lost. |

- To the scene on screen, `talk.goto` goes to the step, as `step.goto` does.
- A skipped scene, a position that is not in the list and a name that no scene has give `not_found`. The message gives the positions, the skipped scenes and the names.
- Changes that Tiza could not save stop the change of scene with `unsaved_changes`. This occurs with a read-only file or with `:autosave off`. `step.next` and `step.prev` stop in the same way. With `--no-save`, the change of scene goes on, and the changes wait in memory, as in [Present a long talk](../guides/present-a-long-talk.md).

`talk.put` and `talk.insert`:

| Field | Method | Type | Meaning |
|---|---|---|---|
| `scenes` | `talk.put` | array | The whole new list. Each entry is a path relative to the master, or `{file, name, skip}`. An entry that the list does not have leaves the talk, and the reply names its file in `removed`. |
| `file` | `talk.insert` | string | The `.tiza` file of the new scene, relative to the master. |
| `at` | `talk.insert` | integer | The position of the new scene, from 1 to the number of entries + 1. |
| `scene` | `talk.insert` | object | Optional. A whole document, as `scene.put` takes it. Tiza writes it to `file`, which must not exist. Without `scene`, `file` must be a `.tiza` file that opens. |
| `name` | `talk.insert` | string | Optional. The `name` of the new entry. |
| `force` | both | boolean | `true` writes the list also when the list of the master changed on disk. The other keys of the master stay. |

- Before Tiza writes a file, it checks the whole list as it checks a master when it opens: each scene that shows must open, a file shows only once, and a name obeys the rules. A request with an error changes nothing.
- Tiza writes only `scenes` of the master. The `title`, the `at` and the keys that this version does not know stay.
- The scene on screen keeps its place by its file. You can give it `skip`: it stays on screen, and `step.next` then opens the next scene that shows. A list without the scene on screen is an error: go to another scene first.
- `talk.insert` writes a new scene file only in the folder of the master or in a folder under it. It refuses an absolute path, a `..` and a folder that does not exist. Tiza makes no folder. When Tiza cannot write the master after the scene file, it removes the scene file again.
- With `--no-save`, Tiza does not write the master: `talk.put` and `talk.insert` give `invalid_argument`. `:autosave on` lets Tiza write again.
- When the master changed on disk while the talk ran, they give `conflict`. Give `force: true` to write your list over it.
- In supervised mode, `talk.goto`, `talk.put` and `talk.insert` wait for the presenter, and `talk.get` does not. When a `talk.put` moves the scene of a `talk.goto` that waits, the presenter can accept the `talk.goto`, and it gets `conflict`. A change of scene answers each change that waits with `unavailable`: the change was for the scene before.
- With no talk open, each `talk.*` method gives `unavailable`.

## Error codes

| Code | When |
|---|---|
| `out_of_range` | A line is not in the code, or a span does not fit on its line. |
| `not_found` | No annotation, move, named step or linked file has that name. |
| `duplicate_id` | The document has an annotation with that id already. |
| `text_not_found` | A `find`, a marker or the target of a move matches nothing in the code. |
| `invalid_argument` | A parameter is not valid, or the method does not exist. A `find` that starts or ends inside a character where the column counts is not valid. |
| `invalid_json` | The line is not a JSON request. |
| `unsupported_schema` | The document of a `scene.put` has a `schema` that is newer than this version. |
| `busy` | A newer `check` at another width replaced this `check` before it ran. |
| `rejected` | In supervised mode, the presenter did not accept the change. |
| `unavailable` | The instance stops, does not answer, or cannot start a `check`. A `talk.*` method when no talk is open. |
| `unsaved_changes` | An `app.quit` without `force` would lose work. In a talk, a change of scene would lose changes that Tiza could not save. |
| `conflict` | In a talk, the master changed on disk, or a change that waited for the presenter names a scene that moved. |
| `io` | Tiza cannot read or write a file, for example a scene of a talk. |
| `internal` | A bug in Tiza stopped the request. |

- After `busy`, send the `check` again.
- After `internal`, the instance keeps running. It undoes the whole request: the document, the screen and the undo entry go back to what they were.
- Do not try again in a loop after `rejected` or `internal`. Tell the presenter.

### Messages

These messages come from the socket itself. The other messages come from each method.

| Message | Code | When |
|---|---|---|
| `invalid request:` | `invalid_json` | The line is not a JSON request. |
| `the application is shutting down` | `unavailable` | The instance stops. |
| `no response from the application` | `unavailable` | The instance did not answer the request. |
| `Tiza closed before the review: the change was not applied` | `unavailable` | In supervised mode, the instance closed while the change waited for review. |
| `app.quit waited more than 10 s, and tiza close stops waiting then: the instance stays open. Send it again` | `unavailable` | The instance was busy, and an `app.quit` waited too long. |
| `` unknown method (`api.methods` lists them) `` | `invalid_argument` | The method does not exist. |
| `invalid parameters:` | `invalid_argument` | The parameters do not have the form of the method. |
| `check: replaced by a newer check (width N) before it ran; ask again` | `busy` | A newer `check` replaced this one. |
| `the presenter rejected this change (supervised mode)` | `rejected` | The presenter rejected the change. |
| `supervised mode went off before the presenter decided this change` | `rejected` | Supervised mode went off while the change waited. |
| `agent change pN expired after 60s without a decision (supervised mode)` | `rejected` | The presenter did not decide in 60 s. |
| `check: could not start the check:` | `unavailable` | The instance could not start the thread of the `check`. |
| `METHOD: a bug in Tiza stopped this request (a panic); please report it` | `internal` | A bug stopped the request. |

- `invalid request: ` and `invalid parameters: ` end with the error of the JSON parser. `check: could not start the check: ` ends with the error of the system.
- `METHOD` is `request` when the method does not exist.
- The `internal` message adds `, and nothing of it was kept` for a method that changes something.
- It ends with ` (details in PATH)` when Tiza could write `panics.log`. `PATH` is the path of that file.

## Command line

These commands use the socket of the live instance.

| Command | What it does |
|---|---|
| `tiza api METHOD [PARAMS]` | Sends one request and prints the result as JSON. |
| `tiza scene status` | Prints the result of `status`. |
| `tiza scene get` | Prints the result of `scene.get`. |
| `tiza scene list` | Prints one line for each annotation. |
| `tiza scene text [--width N]` | Prints the take at the current step as text. |
| `tiza scene put FILE [--as-code]` | Replaces the live document with a `.tiza` file or the output of `tiza scene get`. |
| `tiza scene patch JSON` | Sends a `scene.patch` and prints the result. |
| `tiza next`, `tiza prev` | Shows the next or the previous step, and prints the label of the step. |
| `tiza goto ID` | Goes to a named step or a step number, and prints the label of the step. |
| `tiza close [--force]` | Sends `app.quit` and waits until the instance exits. See [Close an instance](cli.md#close-an-instance). |
| `tiza render --out PATH` | Without a file, exports what the live instance shows. |
| `tiza mcp` | Starts the MCP server. |

For example:

```sh
tiza scene status
tiza api annotation.add '{"type":"note","line":4,"text":"hello"}'
tiza api move.add '{"at_step":2,"action":"code.patch","op":"insert","after":"let b = 2;","content":"    let c = 3;","typing":"line"}'
tiza scene put other.tiza
tiza render --out shot.png
```

- `PARAMS` is a JSON object. Without it, `tiza api` sends `{}`.
- `tiza scene put` reads the code of a file reference and sends it embedded. It also sends the linked files.
- `tiza scene put` refuses a file that is not a document. With `--as-code`, it shows the file as code.
- `tiza scene put` takes the `step` of the output of `tiza scene get`.
- `tiza scene patch` takes `@FILE` to read the JSON from a file.
- `tiza scene text` without `--width` uses the width of the code area of the screen. The add tools use this width for `visible`.
- `tiza render` without a file exports the live instance. See [The live instance](exports.md#the-live-instance).
- The other commands print the result as indented JSON.

`tiza check FILE --json` does not use the socket. It checks a `.tiza` file on disk and prints the report as JSON. It exits with 1 when the report has errors. See [Check report](cli.md#check-report).

### Errors on the command line

A command prints an error on stderr after `tiza: ` and exits with 1.

| Message | When |
|---|---|
| `MESSAGE (CODE)` | The instance refused the request. |
| `` no Tiza instance is listening on PATH (open `tiza <file>` or pass --socket/--session) `` | The command did not get a reply from the instance. |
| `the Tiza instance on PATH did not answer within 75 s: it is busy or stopped; look at its terminal` | The instance got the request, but it did not reply in time. |
| `the Tiza instance on PATH closed the connection before it answered: it possibly quit; look at its terminal` | The instance closed the connection before it replied. |
| `the parameters are not valid JSON` | `PARAMS` of `tiza api` is not JSON. |
| `the parameters from stdin are not valid JSON` | The standard input of `tiza api METHOD -` is not JSON. |
| `the patch is not valid JSON` | The JSON of `tiza scene patch` is not valid. |
| `` FILE is neither a .tiza document nor a `scene get` envelope; to show it as code, pass --as-code `` | `tiza scene put` got a file that is not a document. |
| `could not read PATH` | `tiza scene put` or `tiza scene patch @PATH` cannot read the file. |
| `FILE: not a valid .tiza document` | `tiza scene put` got a document that is not valid. |
| ``FILE: the `document` of the scene envelope is not a valid document`` | `tiza scene put` got an output of `tiza scene get` with a document that is not valid. |
| `could not open FILE` | `tiza scene put --as-code` cannot open the file as code. |
| `could not read the code from stdin` | `tiza scene put --as-code -` cannot use the standard input as code. |

- For a file that Tiza cannot read, `could not read FILE` comes first, also with `--as-code`.
- These messages end with `: ` and an error: `no Tiza instance is listening`, `could not read`, `could not open`, the two messages about a document that is not valid, and the three messages about JSON. The error comes from the system, or from the parser.
- The message `no Tiza instance is listening` comes for each of these cases:
  - The command cannot connect to the socket.
  - The command cannot write the request.
  - The reply is not JSON.

### Standard input

`-` reads the standard input in these commands:

- `tiza api METHOD -` reads `PARAMS`.
- `tiza scene put -` reads the document.
- `tiza scene patch -` reads the JSON.
- `tiza render -` reads a document or code. For code, `--lang` gives the language. See [Exports](exports.md).
- `tiza check -` reads a document.
- `tiza -` reads the code that the TUI shows. `tiza` with a pipe on its standard input does the same.

For example:

```sh
cat demo.tiza | tiza render - --out -
pbpaste | tiza render - --lang rs --out shot.png
tiza render - --out shot.png < demo.tiza
```

## Schema

The contract of the socket is a JSON Schema. It has the parameters and the result of each method, and the request and response lines.

- `tiza schema api` prints it. `--out PATH` writes it to a file.
- `api.schema` gives the same schema from the live instance. A client can check what it sends before it sends it.
- The repository holds it in [`schema/api.schema.json`](../../schema/api.schema.json). A test makes sure that this file is equal to the output of `tiza schema api`.
- `tiza schema document` prints the schema of the `.tiza` document, [`schema/tiza.schema.json`](../../schema/tiza.schema.json).

## Supervised mode

In supervised mode, a request that changes something waits until the presenter accepts it or rejects it. See [Supervised mode](../agents/supervised-mode.md).

These methods wait:

- Annotations: `annotation.add`, `annotation.update`, `annotation.delete`, `scene.clear`, `style.set`.
- Document: `scene.put`, `scene.patch`.
- Moves: `move.add`, `move.delete`.
- Steps: `step.set`, `step.next`, `step.prev`, `step.goto`.
- Focus and pointer: `focus.set`, `pointer.set`, `pointer.clear`.
- Theme and follow mode: `theme.set`, `follow.set`.
- Supervised mode: `supervise.set` with `"enabled": false`. Its summary in the review is `turn supervised mode off`.

- The other methods never wait. They are the methods that read, `supervise.set` with `"enabled": true`, `supervise.status` and `app.quit`.
- An accepted request gives its usual result. A rejected request fails with `rejected`.
- When supervised mode goes off, Tiza rejects each change that waits. A waiting `supervise.set` with `"enabled": false` gets its usual result, not `rejected`.
- Some requests get their error at once and do not wait:
  - A method that does not exist. It gets `invalid_argument` with ``unknown method (`api.methods` lists them)``.
  - A `scene.clear` with wrong parameters.
  - A `scene.put` with a newer `schema`.
  - A `supervise.set` with wrong parameters, for example an unknown key or no `enabled`.
- Other wrong parameters still wait. They fail when the presenter accepts the change.

## How it works

- The design of the protocol comes from herdr: one request on each line, and method names with dots.
- Each connection has its own thread. The thread sends each request to the TUI, and the TUI is the only part that changes the document.
- `check` runs on a copy of the document in its own thread. The screen and the other requests do not wait for it. At most one `check` runs and one waits.
- The MCP server is a client of the socket. Most tools send one method. `list_annotations` and `export_image` send two. `status` adds the `bridge` object of the server.
