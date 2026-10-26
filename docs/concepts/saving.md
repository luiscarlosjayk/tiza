# Saving

Tiza saves each change to the document when you make it. This page tells when Tiza does not save, which commands save, and what Tiza writes when you quit.

## Autosave

Autosave is on when you open a file, except with `--no-save`. Tiza writes each change to the `.tiza` document next to the source file. For example, `tiza src/main.rs` saves to `src/main.rs.tiza`.

Type `:autosave` to see the state. The `status` reply of the API also has the field `autosave`.

## When changes are not saved

In these cases, a change stays in memory and is not on disk:

| Case | How it starts | Status bar after a change |
|---|---|---|
| No-save mode | `tiza FILE --no-save` | `unsaved (:saveas PATH · :q! discards)` |
| Autosave off | `:autosave off` | `unsaved (:w)` |
| A document from an agent | A `scene put` that turns autosave off | `unsaved (:w)` |
| A failed save | A save that cannot write the `.tiza` file | `read-only • unsaved (:w)` |

- Before a change, the status bar shows `no-save` in no-save mode and `autosave off` with autosave off. After a change, the status bar text in the table replaces it.
- **No-save mode** keeps the document on disk as it is. Use it to annotate an example or a demo. `:w` and `:wq` write over the open document, so the status bar tells you to use `:saveas PATH`. After `:saveas PATH`, Tiza saves each change to the new file.
- **Autosave off** keeps your changes until you save them. `:w` saves them. `:autosave on` starts autosave again.
- **A document from an agent** is on screen. See [Agents and saving](#agents-and-saving).
- **A failed save** does not stop autosave. Tiza tries again at each change. See [A failed save](#a-failed-save).

## Save commands

The commands work as in Vim.

| Command | What it does |
|---|---|
| `:w` | Saves to the open document. |
| `:w PATH` | Writes a copy to `PATH` and keeps saving to the open document. |
| `:w! PATH` | Writes the copy over an existing file. |
| `:saveas PATH` or `:sav PATH` | Saves to `PATH` and makes it the open document. |
| `:sav! PATH` | Does the same over an existing file. |
| `:wq` | Saves, then quits. |
| `:wq PATH` | Writes a copy to `PATH`, then quits. |
| `:wq! PATH` | Writes the copy over an existing file, then quits. |
| `:autosave on` | Starts autosave and saves the changes that are not saved. |
| `:autosave off` | Stops autosave. |
| `:autosave` | Shows the state of autosave. |

- A command that takes a `PATH` does not write over an existing file without `!`.
- When `PATH` is the open document, `:w PATH` and `:saveas PATH` save to it, as `:w` does.

The messages of these commands are in [Messages of the save commands](../reference/commands/messages.md#messages-of-the-save-commands).

### Paths

- A relative path starts at the folder where you started Tiza.
- `~` at the start of a path is your home folder.
- Tiza makes the folders that are missing.
- `wrote a copy to` and `saved as` show the full path. When the path is too long, Tiza shows its middle folders as `…`.
- `saved to` shows the path of the open document as you opened it. `PATH exists` shows the path as you typed it.
- A copy writes a relative code reference relative to its own folder, for example `../main.rs`. Its linked files and `follows` use the same rule. An absolute reference stays absolute.
- When the folder of the copy and the folder of the code share only the root, the copy uses an absolute path. Thus the copy opens from any folder.
- When the relative form of a linked file would name the main file, the copy uses an absolute path.

The file that `:saveas` moves to uses the same path rules as a copy.

## Quitting

A quit command does not save more than the current mode saves. With autosave on and no failed save, each change is on disk when you make it. Then quitting writes nothing more. `:wq` saves because it includes `:w`.

| Command | Autosave on | Autosave off or no-save mode | After a failed save |
|---|---|---|---|
| `:q` | Quits. | Refuses while changes are not saved. | Refuses while changes are not saved. |
| `:wq` | Saves, then quits. | Saves, then quits. | Tries the save again, and quits only when it works. |
| `:q!` | Quits. | Quits without saving and prints what it did not save. | Quits without saving and prints what it did not save. |
| `Ctrl-C` | Quits. | Quits without saving and prints what it did not save. | With autosave on, tries the save one more time, then quits. |

- A move is stale when it does not match the code after a reload or a paste. While a move is stale, `:q` and `:wq` refuse in every mode.
- When a panel, the command line or a selection is open, `Ctrl-C` closes it and does not quit. `Ctrl-C` also cancels an arrow that you draw.
- `:wq`, `Ctrl-C` and a closed terminal try a failed save again. `Ctrl-C` and a closed terminal do this only with autosave on. `:q!` never tries.
- When the save fails again, Tiza prints `tiza: could not save FILE: ERROR` before the line of what it did not save.
- When `:wq` cannot save a read-only `.tiza` file, it shows `FILE is read-only`. For other failures, such as a read-only folder, it shows the error of the system.

The messages of `:q` are in [Messages of the save commands](../reference/commands/messages.md#messages-of-the-save-commands).

When Tiza quits without saving, it prints a line on stderr. For example:

```
tiza: quit without saving main.rs.tiza: 1 change not saved (note a1)
```

With autosave off, `:wq PATH` writes the copy and prints the same line for the open document.

## A failed save

A save fails when the `.tiza` file or its folder is read-only. It also fails for other reasons, for example a full disk. Autosave stays on, and Tiza tries again at each change.

1. The first save that fails shows `read-only: could not save FILE — :saveas PATH to save elsewhere`. Tiza shows this message one time.
2. After that, the status bar shows the message of each command again.
3. The status bar keeps `read-only • unsaved (:w)` until a save works. With no unsaved change, it shows `read-only`.
4. When the file is writable again, the next autosave works. With autosave off, the next `:w` that works does the same. Tiza shows `saved to FILE` one time and removes `read-only • unsaved (:w)` from the status bar.

After a failed save, `:saveas PATH` shows `saved to PATH`, not `saved as PATH`.

## Agents and saving

An agent can send a document with `scene put`. Autosave never writes a document from another source, or code that the agent changed, over the open document. `:w` writes it over the open document on purpose.

| Case | Message |
|---|---|
| The code of the document comes from another source. | `another document loaded: autosave off (:w saves over FILE, :saveas PATH saves elsewhere, :autosave on resumes)` |
| The code sent is not the same as the file on disk. | `not saved — autosave off: REASON (:w saves it embedded in FILE, :saveas PATH saves elsewhere, :autosave on resumes)` |
| The code sent is not the same as the file on disk, and autosave is off already. | `not saved: REASON (shown embedded)` |
| The code sent is the copy that Tiza loaded, and the file changed on disk after that. | `FILE changed on disk since it was loaded: still read from it; the screen keeps the copy loaded (:follow on reloads it)` |
| Autosave is off, the code sent is the text of a file that the open document shows embedded, and no other message of the put shows. | `FILE is a reference again; autosave still off (:autosave on resumes)` |

- `REASON` is `the code sent for FILE differs from the file on disk`. For a linked file that Tiza did not read before and cannot read now, it is `FILE is not a file on disk`. Tiza joins several reasons with `; `.
- When the code sent is not the same as one file, and another file changed on disk, the status bar shows one message. The second part starts with ` · also `.
- In the last two rows, `FILE` can be a list of files. With more than one file, the last message says `are` in place of `is`.
- With follow mode on, a document from another source also turns follow mode off. The message says so.
- When the code sent is the copy that Tiza loaded, and only the file changed on disk, the code stays a code reference. Autosave goes on. The screen keeps the copy that Tiza loaded.
- After a `scene get` and a `scene put` of the open document, a document that reads its code from a file still reads it from the file. Tiza embeds nothing.
- After a `scene put` of changed code, the document holds the code embedded. A `scene put` of the text of the file makes the code a file reference again.
- Tiza drops a `views` entry that no `code.link` names. It does not save it.
- A `scene put` that changes nothing writes nothing.

Tiza refuses a `scene put` in some saving cases. See [`scene.put` and `scene.patch`](../reference/socket-api.md#sceneput-and-scenepatch).

## When the terminal closes

Tiza quits as with `Ctrl-C` when its terminal closes. A closed pane or tab and a lost SSH session are examples.

Tiza cannot print to that terminal. It writes its lines to `panics.log` in its runtime folder. Each line has this form:

```
TIME tiza VERSION (COMMIT) pid PID: the terminal went away: TEXT
```

`TIME` is in seconds since 1970. `TEXT` is the line that Tiza could not print.

The runtime folder is `$XDG_RUNTIME_DIR/tiza/`. When that variable is empty or not set, it is `$TMPDIR/tiza/`, or else `/tmp/tiza/`.

When Tiza gets a hang-up signal and its terminal is still open, it prints on that terminal.

## What never writes

`tiza render`, `tiza scene get` and `--smoke` never write the document. An export does not write the document either.

## How it works

Each save is an atomic replace. Tiza writes a temporary file next to the document, then renames it over the document. A rename does not look at the read-only bit of the old file. Thus Tiza checks that bit first, and does not save a read-only file.
