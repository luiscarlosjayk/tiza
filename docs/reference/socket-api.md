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
- Every command that uses the socket takes `--socket` and `--session`: the TUI, `tiza mcp`, `tiza api`, `tiza scene`, `tiza render`, `tiza next`, `tiza prev` and `tiza goto`.
- To run several instances, give each one its own session.

When the socket cannot start, the instance starts without a socket. The status bar shows one of these messages:

| Message | When |
|---|---|
| `local API disabled: an instance is already listening on PATH (use --session for another one)` | The socket of the session has an instance already. |
| `local API disabled:` | The socket cannot start for another reason. |

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

### Read

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `code.get` | None | Gives the code on screen with line numbers, and the file and the window that it comes from. | `get_code` |
| `scene.get` | None | Gives the live document, with the code embedded, and the current step. | `export_image` |
| `scene.list` | None | Gives one line for each annotation, in the order of the document. | `list_annotations` |
| `render.text` | `width` | Gives the take at the current step as plain text. | `render_text` |
| `check` | `width` | Gives the report of `tiza check --json` for the live document. | `check` |

- `width` of `check` is from 20 to 400. The default is 100.

### Annotations

| Method | Params | What it does | MCP tool |
|---|---|---|---|
| `annotation.add` | `type` and its fields | Adds an annotation. | `add_box`, `add_arrow`, `add_note`, `add_callout`, `highlight`, `add_mark`, `add_spotlight` |
| `annotation.update` | `id` and the fields to change | Changes an annotation and keeps its id. | `update_annotation` |
| `annotation.delete` | `id` | Deletes an annotation. | `delete_annotation` |
| `scene.clear` | `step` | Deletes all the annotations, or the annotations of one step. | `clear_annotations` |
| `style.set` | `name`, `glyph`, `legend`, `color` | Defines a style, or changes it. | `define_style` |
| `style.list` | None | Gives the styles of the document, the built-in kinds and the aliases. | `list_kinds` |

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
| `step.set` | `step` | Shows a step. | `set_step` |
| `step.next` | None | Shows the next step. | `next_step` |
| `step.prev` | None | Shows the previous step. | `prev_step` |
| `step.goto` | `id` | Goes to a named step or a step number, and applies every move up to that step. | `goto_step` |

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
- `annotation.update` takes `shape`, which replaces the whole shape of the annotation.
- `annotation.update` takes `heads` with one end only. The other end stays.
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
- The message is the same as when a file opens, for example `.tiza file with schema 99; this version understands up to 1`.
- Tiza migrates a document with an older `schema`, as it does for a file. When Tiza saves the document, it writes the current schema.
- The presenter can undo a `scene.put` with `u`. The undo also gives back the step.
- For the save rules, see [Agents and saving](../concepts/saving.md#agents-and-saving).
- `scene.patch` takes these fields: `step`, `theme`, `padding`, `title`, `numbers`, `margin`, `styles`, `annotations`, `steps` and `loop`.
- A `step` after the last step becomes the last step.
- `padding`, `title` and `margin` take `null`, which gives back the default.
- `styles` merges the styles by name. A style with the same name replaces the old one.
- `steps` replaces the whole list of [named steps](document-format.md#named-steps). `[]` clears it.
- `annotations` merges full annotations by id. A new id adds an annotation, and the same id replaces one. `{"id": "a1", "deleted": true}` deletes one.
- `deleted` for an id that the document does not have fails with `invalid_argument`.
- A replaced annotation keeps its step without `step`, and its file without `file`. `file: null` is the main file.
- A new id gets the defaults of `annotation.add`. `file: null` is then the file on screen at its step.

For example:

```sh
tiza scene patch '{"step":2,"theme":"light","annotations":[{"id":"a1","deleted":true}]}'
```

Tiza refuses a `scene put` in these saving cases. The error code is `invalid_argument`. The document and the file on disk do not change.

| Message | When |
|---|---|
| `scene.put: the code sent is not what FILE holds now, and follow is on (the screen shows the file as it is): scene.get again; to change the code, edit FILE` | Follow mode is on, and the code sent for the main file is not the text of the file now. |
| `scene.put: FILE cannot be read (ERROR) and the document reads its code from it: put the file back, or :embed to keep the copy loaded` | The open document reads code from a file, the main file or a linked file that Tiza read before. Tiza cannot read that file now. |

`ERROR` is the read error of Tiza. It ends with the error text of the system.

### Keys that Tiza does not know

These methods refuse a key that they do not know: `annotation.update`, `scene.clear`, `scene.get`, `scene.patch`, `pointer.set` and `supervise.set`. The `move` object of `annotation.update` also refuses one. The error is `invalid_argument`, and its message names the keys that the method knows. `scene.get` knows no key.

## Results

| Method | Result |
|---|---|
| `ping` | The string `"pong"`. |
| `status` | The fields in [Fields of `status`](#fields-of-status). |
| `api.methods` | A list of `{method, description}`. |
| `api.schema` | `protocol`, `request`, `response`, and `methods`, a list of `{method, description, params, result}`. |
| `code.get` | `language`, `file`, `view`, `first_line`, `last_line` and `text`. |
| `scene.get` | `revision`, `step`, `max_step`, `document`, and `stale` and `held` when a move is stale. |
| `scene.list` | A list of strings, one for each annotation. |
| `render.text` | `text`, and `width`, the width of the take. |
| `check` | The report of `tiza check --json`, plus `revision`, the revision that it checked. |
| `annotation.add` | The [reply of an annotation tool](mcp-tools.md#reply-of-an-annotation-tool), without its sentence. |
| `annotation.update` | The same fields as `annotation.add`. |
| `annotation.delete` | `id` and `revision`. |
| `scene.clear` | `removed`, `remaining` and `revision`. |
| `style.set` | `name`, `style` and `revision`. |
| `style.list` | `document`, `builtin` and `aliases`. |
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
| `follow.status` | `enabled`, `path`, `reloads`, `last_reload`, `missing` and `reason`, when they apply. |
| `supervise.set` | The fields of `supervise.status`, and `message`. |
| `supervise.status` | `supervised`, `pending`, `accepted`, `rejected` and `timeout_secs`. |

- `message` tells what the method did, in words.
- `label` is the step label: the text of the status bar for the step, for example `step 3/7 · Shared borrow`.
- `pending` of `supervise.status` is a list of `{id, method, summary, age_ms}`. In `status`, `pending` is a number.
- `shown` of `pointer.set` is `false` while the presenter has the command line open. The pointer shows when the command line closes.
- The fields of the `check` report are in [Check report](cli.md#check-report).

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
| `warnings` | Warnings about the screen and the named steps, in words. |
| `supervised` | `true` in supervised mode. |
| `pending` | The number of changes that wait for the presenter. |
| `autosave` | `true` when Tiza saves each change to the document. |
| `document` | The absolute path of the `.tiza` file, when the file exists on disk. |
| `numbers` | `true` when Tiza shows the line numbers. |
| `margin` | The limit of the right margin in columns, when the document sets one. |
| `pointer` | The live pointer as `{line, to, by}`, or `null`. |
| `stale_moves` | The number of moves that do not match the code any more. |

- A field that does not apply is not in the result, except `file` and `pointer`, which are `null`.
- `loop` and `supervised` are not in the result when they are `false`. `pending` and `stale_moves` are not in the result when they are 0.
- `by` of `pointer` is `presenter` or `agent`. It tells who moved the pointer last.
- `revision` grows when something changes, by 1 or more. A request that changes nothing keeps it.
- A change is a change of the document or of what the instance shows. Examples are the step, the focus, the theme and supervised mode.
- One request can add more than 1. For example, `theme.set`, `scene.put` and `scene.patch` add 2.
- `scene.patch` with `{}`, and `annotation.update` with only `id`, change nothing. They keep `revision`.
- `pointer.set`, `pointer.clear` and `follow.set` do not change `revision`.
- In follow mode, a reload or a relink changes the document. Thus it changes `revision`.
- Compare `revision` to find a change. Do not count on steps of 1.

## Error codes

| Code | When |
|---|---|
| `out_of_range` | A line is not in the code, or a span does not fit on its line. |
| `not_found` | No annotation, move, named step or linked file has that name. |
| `duplicate_id` | The document has an annotation with that id already. |
| `text_not_found` | A `find`, a marker or the target of a move matches nothing in the code. |
| `invalid_argument` | A parameter is not valid, or the method does not exist. |
| `invalid_json` | The line is not a JSON request. |
| `unsupported_schema` | The document of a `scene.put` has a `schema` that is newer than this version. |
| `busy` | A newer `check` at another width replaced this `check` before it ran. |
| `rejected` | In supervised mode, the presenter did not accept the change. |
| `unavailable` | The instance stops, does not answer, or cannot start a `check`. |
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
| `` unknown method (`api.methods` lists them) `` | `invalid_argument` | The method does not exist. |
| `invalid parameters:` | `invalid_argument` | The parameters do not have the form of the method. |
| `check: replaced by a newer check (width N) before it ran; ask again` | `busy` | A newer `check` replaced this one. |
| `the presenter rejected this change (supervised mode)` | `rejected` | The presenter rejected the change. |
| `supervised mode went off before the presenter decided this change` | `rejected` | Supervised mode went off while the change waited. |
| `proposal pN expired after 60s without a decision (supervised mode)` | `rejected` | The presenter did not decide in 60 s. |
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
  - The instance does not reply in 75 seconds.
  - The instance closes the connection before it replies.
  - The reply is not JSON.
- Thus the instance can be busy or stopped, and not only absent.

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

- The other methods never wait. They are the methods that read, `supervise.set` with `"enabled": true` and `supervise.status`.
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
