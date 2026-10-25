# Saving

Tiza saves each change to the document when you make it. This page tells when Tiza does not save, which commands save, and what Tiza writes when you quit.

## Autosave

Autosave is on when you open a file. Tiza writes each change to the `.tiza` document next to the source file. For example, `tiza src/main.rs` saves to `src/main.rs.tiza`.

Type `:autosave` to see the state. The `status` reply of the API also has the field `autosave`.

## When Tiza does not save

Four cases stop autosave. One rule applies to all four: the changes stay in memory until you save them or quit.

| Case | How it starts | Status bar |
|---|---|---|
| No-save mode | `tiza FILE --no-save` | `no-save`, and `unsaved (:saveas PATH · :q! discards)` after a change |
| Autosave off | `:autosave off` | `autosave off`, and `• unsaved (:w)` after a change |
| Read-only document | The `.tiza` file or its folder is read-only. | `read-only • unsaved (:w)` |
| A document from an agent | A `scene put` of a document from another file, or with code that is not the same as the file on disk | `• unsaved (:w)` |

- **No-save mode** keeps the document on disk as it is. Use it to annotate an example or a demo. `:w` writes over the open document, so the status bar tells you to use `:saveas PATH`. After `:saveas PATH`, Tiza saves each change to the new file.
- **Autosave off** keeps your changes until you save them. `:w` saves them. `:autosave on` starts autosave again.
- **A read-only document** has its own sequence of messages. See [A read-only document](#a-read-only-document).
- **A document from an agent** is on screen. Tiza never writes it over the open document. The message starts with `not saved — autosave off:` and gives the reason.

## Save commands

The commands work as in Vim. No command writes over an existing file without `!`.

| Command | What it does |
|---|---|
| `:w` | Saves to the open document. |
| `:w PATH` | Writes a copy to `PATH` and keeps saving to the open document. |
| `:w! PATH` | Writes the copy over an existing file. |
| `:saveas PATH` or `:sav PATH` | Saves to `PATH` and makes it the open document. |
| `:sav! PATH` | Does the same over an existing file. |
| `:wq` | Saves, then quits. |
| `:wq PATH` | Writes a copy to `PATH`, then quits. |
| `:autosave on` | Starts autosave and saves the changes that are not saved. |
| `:autosave off` | Stops autosave. |
| `:autosave` | Shows the state of autosave. |

The status bar shows one of these messages:

| Message | When |
|---|---|
| `saved to FILE` | `:w` saved the open document. |
| `wrote a copy to PATH (FILE is still the file open)` | `:w PATH` wrote a copy. |
| `saved as PATH (autosave on)` or `saved as PATH (autosave off)` | `:saveas PATH` saved to the new file. |
| `autosave on (saved to FILE)` | `:autosave on` saved the changes that were not saved. |
| `PATH exists (add ! to overwrite)` | The file exists and the command has no `!`. |

### Paths

- A relative path starts at the folder where you started Tiza.
- `~` at the start of a path is your home folder.
- Tiza makes the folders that are missing.
- The message shows the full path. When the path is too long, Tiza shows its middle folders as `…`.
- A copy writes its code reference relative to its own folder, for example `../main.rs`. Its linked files and `follows` use the same rule.
- When the folder of the copy and the folder of the code share only the root, the copy uses an absolute path. Thus the copy opens from any folder.

The file that `:saveas` moves to uses the same path rules as a copy.

## Quitting

A quit command does not save more than the current mode saves. With autosave on, each change is on disk when you make it, so quitting writes nothing more. `:wq` saves because it includes `:w`.

| Command | Autosave on | Autosave off or no-save mode | Read-only document |
|---|---|---|---|
| `:q` | Quits. | Refuses while changes are not saved. | Refuses while changes are not saved. |
| `:wq` | Saves, then quits. | Saves, then quits. | Refuses with `FILE is read-only`. |
| `:q!` | Quits. | Quits without saving and prints what it did not save. | Quits without saving and prints what it did not save. |
| Ctrl-C | Quits. | Quits without saving and prints what it did not save. | Tries the save one more time, then quits. |

Only Ctrl-C tries a failed save again. `:q!` never does. When the save fails again, Ctrl-C quits as `:q!` does.

When `:q` refuses, the status bar shows one of these messages:

| Case | Message |
|---|---|
| Autosave off | `unsaved changes (autosave off): :w saves to FILE, :saveas PATH saves elsewhere, :q! quits without saving` |
| No-save mode | `unsaved changes (--no-save): :saveas PATH saves them elsewhere, :q! discards them` |
| Read-only document | `unsaved (read-only): :q! quits without saving, :saveas PATH saves elsewhere — FILE` |

When Tiza quits without saving, it prints a line on stderr:

```
tiza: quit without saving main.rs.tiza: 1 change not saved (note a1)
```

With autosave off, `:wq PATH` writes the copy and prints the same line for the open document.

## A read-only document

Tiza does not save a `.tiza` file that is read-only, or a file in a read-only folder.

1. The first autosave that fails shows `read-only: could not save FILE — :saveas PATH to save elsewhere`. Tiza shows this message one time.
2. After that, the status bar shows the message of each command again.
3. The status bar keeps `read-only • unsaved (:w)` until a save works.
4. When the file is writable again, the next autosave works. Tiza shows `saved to FILE` one time and removes `read-only • unsaved (:w)` from the status bar.

## Agents and saving

- A `scene put` of a document from another file stops autosave. See [When Tiza does not save](#when-tiza-does-not-save).
- After a `scene get` and a `scene put` of the open document, a document that reads its code from a file still reads it from the file. Tiza embeds nothing.
- A `scene put` that changes nothing writes nothing.

## When the terminal closes

Tiza quits as with Ctrl-C when its terminal closes. A closed pane or tab and a lost SSH session are examples.

Tiza cannot print to that terminal. It writes the stderr line to `panics.log` in its runtime directory. The directory is `$XDG_RUNTIME_DIR/tiza/`. When that variable is not set, it is `$TMPDIR/tiza/` or `/tmp/tiza/`.

## What never writes

`tiza render`, `tiza scene get` and `--smoke` never write the document. An export does not write the document either.

## How it works

Each save is an atomic replace. Tiza writes a temporary file next to the document, then renames it over the document. A rename does not look at the read-only bit of the old file. Thus Tiza checks that bit first, and does not save a read-only file.
