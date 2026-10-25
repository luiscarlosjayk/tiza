# Command line

These commands open code in Tiza. The paragraph after them tells how Tiza finds the language of the code.

```sh
tiza src/main.rs            # open a file
tiza src/main.rs:12-48      # only a range of lines (numbering is preserved)
tiza src/main.rs --follow   # reload when the file is saved in the editor; annotations follow their text
pbpaste | tiza              # code from the clipboard (macOS); --lang rs when it cannot be guessed
tiza examples/ownership.rs.tiza --present   # open the demo in presentation mode
```

The language is detected from the file extension, then well-known names (`Makefile`, `Dockerfile`, `Cargo.lock`, `.bashrc`…), then a `#!` line, then a Vim/Emacs modeline in the first or last five lines; pasted or piped code uses the same shebang/modeline rule. `--lang` and `:lang <id>` override it (saved in the document); `:lang auto` re-detects and says how; an id without a grammar is shown as plain text with a warning.

For each flag of `tiza render`, see [Options of `tiza render`](exports.md#options-of-tiza-render).
