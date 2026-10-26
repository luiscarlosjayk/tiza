# Save, quit, panel and undo commands

Use these commands to save the document, to quit, to open a panel and to undo a change. This page has no frames, because these commands do not change the drawing.

## `:w`

Saves the document, or writes a copy of it.

```
:w
:w PATH
:w! PATH
```

- `:w` and `:w!` save to the open document.
- `:w PATH` writes a copy to `PATH`. The open document stays the same.
- `:w PATH` refuses an existing file. `:w! PATH` writes over it.

## `:saveas`

Saves to a new file and makes it the open document.

```
:saveas PATH
:saveas! PATH
```

- `:saveas PATH` refuses an existing file. `:saveas! PATH` writes over it.

## `:wq`

Saves, then quits.

```
:wq
:wq PATH
:wq!
:wq! PATH
```

- `:wq PATH` writes a copy to `PATH`, then quits. It refuses an existing file.
- `:wq!` also saves and quits when a move is stale. `:wq! PATH` also writes over an existing file, and also quits when a move is stale.

## `:q`

Quits. `:q!` quits without saving.

```
:q
:q!
```

- `:q` refuses when the document has changes that are not saved. The messages are in [Messages of the save commands](messages.md#messages-of-the-save-commands).
- `:q` also refuses while a move is stale, because the marks of a stale move are lost when Tiza closes. `:wq!` and `:q!` quit.
- What each command saves, and when `:q` refuses, is in [Saving](../../concepts/saving.md).

## `:autosave`

Starts autosave, stops it, or shows its state.

```
:autosave on
:autosave off
:autosave
```

- `:autosave on` also accepts `yes`. `:autosave off` also accepts `no`.

## Panels and undo

| Command | What it does |
|---|---|
| `:list` | Shows the annotation list, as `L` does. |
| `:present` | Starts or stops presentation mode, as `P` does. |
| `:help` | Shows the help, as `?` does. |
| `:undo` | Undoes the last change, as `u` does. |
| `:redo` | Redoes the change that you undid, as `U` does. |
