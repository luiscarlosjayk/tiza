# Control and invisible characters

This page tells how Tiza shows control and invisible characters in the code and in annotation texts.

**Control characters.** Code can hold bytes that a terminal acts on, for example ESC, NUL and BEL. Tiza never shows or returns them raw. `get_code`, `render_text`, every export and the `tiza://` resources show a C0 control as its picture, for example `␛` for ESC, `␀` and `␇`. They show DEL as `␡` and a C1 control as `�`, and they expand a tab to spaces. In a text reply, a bidi control (U+202A–U+202E, U+2066–U+2069, U+200E/F, U+061C) is `�`.

**Invisible characters in the code.** These are the invisible characters in the code:

- a bidi control;
- a zero-width space (U+200B) or a word joiner (U+2060);
- a byte order mark (U+FEFF) after the start of the file;
- a ZWJ or ZWNJ that joins nothing, for example `ad` U+200D `min`;
- every other default-ignorable character, for example a soft hyphen, an invisible operator, a tag character, or a variation selector after an ASCII character or after a letter of a script that does not use joiners.

Each invisible character takes one cell in `render_text` and every export. Tiza draws it as `⬚` (U+2B1A) in the theme's error color, so `ad⬚min` does not look like `admin`. Anchors and `find` count the real characters, so the grapheme columns do not change.

Tiza draws an invisible character that does a job as it is:

- a joiner inside an emoji sequence or an Indic conjunct;
- a joiner after a letter of a script that uses joiners, for example the ZWNJ of a Persian word;
- a variation selector after an emoji or another symbol, in a keycap, or after an ideograph or a letter of a script that uses joiners;
- a Mongolian free variation selector after a Mongolian letter;
- the tags of a subdivision flag, for example the flag of Scotland.

**Control and invisible characters in annotation texts.** A control character in an annotation text shows as its picture too. In a label of one line, a tab, a line break, a vertical tab, a form feed or NEL is a space. A zero-width character takes no cell, so `clipped` and `check` count what Tiza draws. A label of only zero-width characters is the same as no label. A note or a callout text of only zero-width characters is empty, and Tiza refuses it.

JSON replies (`tiza://scene`, `pretty` results) keep the real characters as `\uXXXX` escapes. A `find` must use the real character (`\u001b` in JSON), not its picture. An error that quotes your `find` shows such a character in Rust escapes (`text "a\u{1b}b" not found in the code`). Other errors show its picture.
