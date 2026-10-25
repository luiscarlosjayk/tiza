# Socket API

The local socket and the CLI give the same API as the MCP server.

The contract is published as JSON Schema: [`schema/tiza.schema.json`](https://github.com/luiscarlosjayk/tiza/blob/main/schema/tiza.schema.json) (the document) and [`schema/api.schema.json`](https://github.com/luiscarlosjayk/tiza/blob/main/schema/api.schema.json) (every socket method, params and result), regenerated with `tiza schema document|api` and checked in CI; the live instance also answers `api.schema`. `-` means stdin everywhere: `cat demo.tiza | tiza render - --out -`, `pbpaste | tiza render - --lang rs --out shot.png`, `tiza api annotation.add -`.

The same API is available as a Unix socket with line-delimited JSON (design borrowed from herdr) and from the CLI:

```sh
tiza scene status                 # is there an instance? what is it showing?
tiza scene list                   # annotations
tiza scene text                   # the take on screen, as text (`--width N`: the take at N)
tiza api annotation.add '{"type":"note","line":4,"text":"hello"}'
tiza scene put other.tiza      # replace the live document (u undoes, step included); also takes a `scene get` envelope; a plain file only with --as-code
tiza scene patch '{"step":2,"theme":"light","annotations":[{"id":"a1","deleted":true}]}'   # partial change, merged by id
tiza api move.add '{"at_step":2,"action":"code.patch","op":"insert","after":"let b = 2;","content":"    let c = 3;","typing":"line"}'   # the code changes at step 2
tiza render demo.tiza --out - --step 2 --width 100   # without opening the TUI
tiza check demo.tiza --json        # preflight of a file: errors, warnings, the narrowest clean width
```

Several instances: `--session name` (or `TIZA_SESSION`). Explicit socket: `--socket path` (or `TIZA_SOCKET`). The socket lives in `$XDG_RUNTIME_DIR` or `$TMPDIR` with 0600 permissions and **never** listens on TCP.

## Schema

`tiza schema document` and `tiza schema api` print the JSON Schemas (also committed under `schema/`); the live instance answers `api.schema` with the params and result of every method, so a client can validate what it sends. Agents without a terminal can pipe documents: `tiza render - --out shot.png < demo.tiza`.

## Without MCP: socket and CLI

Same contract, one JSON request per line over the Unix socket:

```sh
printf '%s\n' '{"id":1,"method":"annotation.add","params":{"type":"note","line":4,"text":"hello"}}' | nc -U "$TMPDIR/tiza/default.sock"
tiza api annotation.add '{"type":"note","line":4,"text":"hello"}'
tiza api api.methods
tiza check demo.tiza --json   # preflight of a file, no instance needed (exit 1 on errors)
```

Response: `{"id":1,"result":{...}}` or `{"id":1,"error":{"code":"text_not_found","message":"..."}}`. Codes: `out_of_range`, `not_found`, `duplicate_id`, `text_not_found`, `invalid_argument`, `invalid_json`, `unavailable`, `busy` (a `check` replaced by a newer one before it ran: ask again).
