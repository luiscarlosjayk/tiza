# Getting started

In this lesson, you open an example of Tiza, go through its steps, add a note and a box, and export an image. Tiza draws annotations on code: notes, boxes, arrows and marks. Each annotation comes in at a step. It takes about 10 minutes.

You need:

- macOS, or Linux x86_64 with glibc 2.35 or later. For other systems, see [Install Tiza](guides/install.md).
- A terminal.
- `curl`.

## 1. Install Tiza

1. Run the install script:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh
   ```

2. If the script shows a line to add to your `PATH`, add it. Then open a new terminal.
3. Run `tiza --version`.

You see the version and the git commit of the build, in this form:

```
tiza VERSION (COMMIT)
```

For the other ways to install, see [Install Tiza](guides/install.md).

## 2. Get the example

Make a folder for the lesson. Then download the example into it:

```sh
mkdir tiza-lesson && cd tiza-lesson
curl -fsSLO https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/examples/ownership.rs
curl -fsSLO https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/examples/ownership.rs.tiza
```

You have two files: `ownership.rs`, the code, and `ownership.rs.tiza`, its annotations. If a download fails, copy the two files from the `examples/` folder of the repository.

## 3. Open the example

Run:

```sh
tiza ownership.rs.tiza
```

Step 0 shows the code, with no annotations. The TUI also shows the status bar on the last row. When the code is longer than the terminal, press `j` and `k` to move.

<!-- frame id=start-open doc=examples/ownership.rs.tiza step=0 -->

<img src="images/guides/start-open.webp" width="598" alt="The code of ownership.rs, lines 1 to 25, with no annotations.">

<details><summary>The document: <code>examples/ownership.rs.tiza</code></summary>

```json
{
  "schema": 1,
  "title": "Ownership in Rust: move, borrow and &mut",
  "source": {
    "language": "rs",
    "first_line": 1
  },
  "code": {
    "file": "ownership.rs"
  },
  "styles": {
    "borrow_mut": {
      "glyph": "✎▶",
      "legend": "&mut borrow",
      "color": "yellow"
    }
  },
  "step": 0,
  "theme": "dark",
  "padding": [1, 2],
  "annotations": [
    {
      "id": "a1",
      "step": 1,
      "type": "mark",
      "kind": "move",
      "span": {
        "line": 3,
        "start": 10,
        "end": 11
      },
      "label": "into takes",
      "color": "orange"
    },
    {
      "id": "a2",
      "step": 1,
      "type": "arrow",
      "from": {
        "line": 3,
        "col": 10
      },
      "to": {
        "line": 15,
        "col": 9
      },
      "label": "move",
      "color": "orange"
    },
    {
      "id": "a3",
      "step": 3,
      "type": "note",
      "line": 4,
      "text": "s no longer owns it: using it here does not compile",
      "color": "orange"
    },
    {
      "id": "a4",
      "step": 3,
      "type": "box",
      "from_line": 15,
      "to_line": 17,
      "label": "takes owns s and frees it at the end",
      "color": "orange"
    },
    {
      "id": "a5",
      "step": 3,
      "type": "mark",
      "kind": "drop",
      "span": {
        "line": 17,
        "start": 0,
        "end": 1
      },
      "label": "s dies here"
    },
    {
      "id": "a6",
      "step": 4,
      "type": "mark",
      "kind": "read",
      "span": {
        "line": 7,
        "start": 16,
        "end": 18
      },
      "label": "borrowed",
      "color": "cyan"
    },
    {
      "id": "a7",
      "step": 4,
      "type": "arrow",
      "from": {
        "line": 7,
        "col": 16
      },
      "to": {
        "line": 19,
        "col": 7
      },
      "label": "borrow &",
      "color": "cyan",
      "stroke": "dashed"
    },
    {
      "id": "a8",
      "step": 4,
      "type": "note",
      "line": 8,
      "text": "t is still valid",
      "color": "cyan"
    },
    {
      "id": "a9",
      "step": 5,
      "type": "mark",
      "kind": "borrow_mut",
      "span": {
        "line": 11,
        "start": 9,
        "end": 15
      }
    },
    {
      "id": "a10",
      "step": 5,
      "type": "arrow",
      "from": {
        "line": 11,
        "col": 9
      },
      "to": {
        "line": 23,
        "col": 8
      },
      "label": "&mut",
      "color": "yellow",
      "stroke": "thick"
    },
    {
      "id": "a11",
      "step": 5,
      "type": "box",
      "from_line": 23,
      "to_line": 25,
      "label": "borrows mutably: the caller sees the change",
      "color": "yellow"
    },
    {
      "id": "a12",
      "step": 6,
      "type": "note",
      "line": 12,
      "text": "u is now \"hey!\"",
      "color": "yellow"
    },
    {
      "id": "a13",
      "step": 1,
      "type": "arrow",
      "from": {
        "line": 2,
        "col": 12
      },
      "to": {
        "line": 2,
        "col": 8
      },
      "label": "s is born",
      "color": "purple"
    },
    {
      "id": "s1",
      "step": 2,
      "type": "spotlight",
      "from_line": 15,
      "to_line": 17,
      "ids": [
        "a1",
        "a2"
      ]
    },
    {
      "id": "s2",
      "step": 4,
      "type": "spotlight",
      "ids": [
        "a6",
        "a7",
        "a8"
      ]
    },
    {
      "id": "a14",
      "step": 4,
      "type": "box",
      "from_line": 19,
      "to_line": 21,
      "label": "borrows read-only: main keeps using t",
      "color": "cyan"
    },
    {
      "id": "a15",
      "step": 1,
      "type": "note",
      "line": 1,
      "text": "three ways to pass a value",
      "color": "cyan"
    }
  ]
}
```

</details>

## 4. Go through the steps

1. Press `n` six times. At each step, new annotations show.
2. Press `p` to go back one step.
3. Press `n` to go to step 6 again.

The status bar on the last row shows `step 6/6`. Step 6 shows all the annotations of the example:

<!-- frame id=start-steps doc=examples/ownership.rs.tiza step=6 -->

<img src="images/guides/start-steps.webp" width="598" alt="Step 6: arrows and marks for a move, a borrow and a mutable borrow, a drop mark, notes, and a box around each of the functions takes, len and push.">

<details><summary>The document: <code>examples/ownership.rs.tiza</code></summary>

```json
{
  "schema": 1,
  "title": "Ownership in Rust: move, borrow and &mut",
  "source": {
    "language": "rs",
    "first_line": 1
  },
  "code": {
    "file": "ownership.rs"
  },
  "styles": {
    "borrow_mut": {
      "glyph": "✎▶",
      "legend": "&mut borrow",
      "color": "yellow"
    }
  },
  "step": 0,
  "theme": "dark",
  "padding": [1, 2],
  "annotations": [
    {
      "id": "a1",
      "step": 1,
      "type": "mark",
      "kind": "move",
      "span": {
        "line": 3,
        "start": 10,
        "end": 11
      },
      "label": "into takes",
      "color": "orange"
    },
    {
      "id": "a2",
      "step": 1,
      "type": "arrow",
      "from": {
        "line": 3,
        "col": 10
      },
      "to": {
        "line": 15,
        "col": 9
      },
      "label": "move",
      "color": "orange"
    },
    {
      "id": "a3",
      "step": 3,
      "type": "note",
      "line": 4,
      "text": "s no longer owns it: using it here does not compile",
      "color": "orange"
    },
    {
      "id": "a4",
      "step": 3,
      "type": "box",
      "from_line": 15,
      "to_line": 17,
      "label": "takes owns s and frees it at the end",
      "color": "orange"
    },
    {
      "id": "a5",
      "step": 3,
      "type": "mark",
      "kind": "drop",
      "span": {
        "line": 17,
        "start": 0,
        "end": 1
      },
      "label": "s dies here"
    },
    {
      "id": "a6",
      "step": 4,
      "type": "mark",
      "kind": "read",
      "span": {
        "line": 7,
        "start": 16,
        "end": 18
      },
      "label": "borrowed",
      "color": "cyan"
    },
    {
      "id": "a7",
      "step": 4,
      "type": "arrow",
      "from": {
        "line": 7,
        "col": 16
      },
      "to": {
        "line": 19,
        "col": 7
      },
      "label": "borrow &",
      "color": "cyan",
      "stroke": "dashed"
    },
    {
      "id": "a8",
      "step": 4,
      "type": "note",
      "line": 8,
      "text": "t is still valid",
      "color": "cyan"
    },
    {
      "id": "a9",
      "step": 5,
      "type": "mark",
      "kind": "borrow_mut",
      "span": {
        "line": 11,
        "start": 9,
        "end": 15
      }
    },
    {
      "id": "a10",
      "step": 5,
      "type": "arrow",
      "from": {
        "line": 11,
        "col": 9
      },
      "to": {
        "line": 23,
        "col": 8
      },
      "label": "&mut",
      "color": "yellow",
      "stroke": "thick"
    },
    {
      "id": "a11",
      "step": 5,
      "type": "box",
      "from_line": 23,
      "to_line": 25,
      "label": "borrows mutably: the caller sees the change",
      "color": "yellow"
    },
    {
      "id": "a12",
      "step": 6,
      "type": "note",
      "line": 12,
      "text": "u is now \"hey!\"",
      "color": "yellow"
    },
    {
      "id": "a13",
      "step": 1,
      "type": "arrow",
      "from": {
        "line": 2,
        "col": 12
      },
      "to": {
        "line": 2,
        "col": 8
      },
      "label": "s is born",
      "color": "purple"
    },
    {
      "id": "s1",
      "step": 2,
      "type": "spotlight",
      "from_line": 15,
      "to_line": 17,
      "ids": [
        "a1",
        "a2"
      ]
    },
    {
      "id": "s2",
      "step": 4,
      "type": "spotlight",
      "ids": [
        "a6",
        "a7",
        "a8"
      ]
    },
    {
      "id": "a14",
      "step": 4,
      "type": "box",
      "from_line": 19,
      "to_line": 21,
      "label": "borrows read-only: main keeps using t",
      "color": "cyan"
    },
    {
      "id": "a15",
      "step": 1,
      "type": "note",
      "line": 1,
      "text": "three ways to pass a value",
      "color": "cyan"
    }
  ]
}
```

</details>

For all the keys of the steps, see [Go to a step](reference/keys.md#go-to-a-step).

## 5. Add a note and a box

Press `:` to open the command line on the last row. Type the command, then press `Enter`. Do this for each command:

```text
:step 7
:note 20 counts the bytes, not the letters
:box 6-8 t is only borrowed
```

`20` is a line number, and `6-8` is lines 6 to 8. `:step 7` goes to a new step after the last one, so the note and the box go on step 7. When a command is wrong, the status bar shows a message. Type the command again. `Esc` closes the command line.

<!-- frame id=start-yours doc=docs/frames/getting-started.tiza step=7 -->

<img src="images/guides/start-yours.webp" width="598" alt="Step 7: the annotations of steps 1 to 6, a new box around lines 6 to 8 with the label: t is only borrowed. A new note under line 20: counts the bytes, not the letters.">

<details><summary>The document: <code>docs/frames/getting-started.tiza</code></summary>

```json
{
  "schema": 1,
  "title": "Ownership in Rust: move, borrow and &mut",
  "source": {
    "path": "ownership.rs",
    "language": "rs",
    "first_line": 1
  },
  "code": {
    "file": "ownership.rs"
  },
  "styles": {
    "borrow_mut": {
      "glyph": "✎▶",
      "legend": "&mut borrow",
      "color": "yellow"
    }
  },
  "annotations": [
    {
      "id": "a1",
      "step": 1,
      "color": "orange",
      "type": "mark",
      "kind": "move",
      "span": {
        "line": 3,
        "start": 10,
        "end": 11
      },
      "label": "into takes"
    },
    {
      "id": "a2",
      "step": 1,
      "color": "orange",
      "type": "arrow",
      "from": {
        "line": 3,
        "col": 10
      },
      "to": {
        "line": 15,
        "col": 9
      },
      "label": "move"
    },
    {
      "id": "a3",
      "step": 3,
      "color": "orange",
      "type": "note",
      "line": 4,
      "text": "s no longer owns it: using it here does not compile"
    },
    {
      "id": "a4",
      "step": 3,
      "color": "orange",
      "type": "box",
      "from_line": 15,
      "to_line": 17,
      "label": "takes owns s and frees it at the end"
    },
    {
      "id": "a5",
      "step": 3,
      "type": "mark",
      "kind": "drop",
      "span": {
        "line": 17,
        "start": 0,
        "end": 1
      },
      "label": "s dies here"
    },
    {
      "id": "a6",
      "step": 4,
      "color": "cyan",
      "type": "mark",
      "kind": "read",
      "span": {
        "line": 7,
        "start": 16,
        "end": 18
      },
      "label": "borrowed"
    },
    {
      "id": "a7",
      "step": 4,
      "color": "cyan",
      "type": "arrow",
      "from": {
        "line": 7,
        "col": 16
      },
      "to": {
        "line": 19,
        "col": 7
      },
      "label": "borrow &",
      "stroke": "dashed"
    },
    {
      "id": "a8",
      "step": 4,
      "color": "cyan",
      "type": "note",
      "line": 8,
      "text": "t is still valid"
    },
    {
      "id": "a9",
      "step": 5,
      "type": "mark",
      "kind": "borrow_mut",
      "span": {
        "line": 11,
        "start": 9,
        "end": 15
      }
    },
    {
      "id": "a10",
      "step": 5,
      "color": "yellow",
      "type": "arrow",
      "from": {
        "line": 11,
        "col": 9
      },
      "to": {
        "line": 23,
        "col": 8
      },
      "label": "&mut",
      "stroke": "thick"
    },
    {
      "id": "a11",
      "step": 5,
      "color": "yellow",
      "type": "box",
      "from_line": 23,
      "to_line": 25,
      "label": "borrows mutably: the caller sees the change"
    },
    {
      "id": "a12",
      "step": 6,
      "color": "yellow",
      "type": "note",
      "line": 12,
      "text": "u is now \"hey!\""
    },
    {
      "id": "a13",
      "step": 1,
      "color": "purple",
      "type": "arrow",
      "from": {
        "line": 2,
        "col": 12
      },
      "to": {
        "line": 2,
        "col": 8
      },
      "label": "s is born"
    },
    {
      "id": "s1",
      "step": 2,
      "type": "spotlight",
      "from_line": 15,
      "to_line": 17,
      "ids": [
        "a1",
        "a2"
      ]
    },
    {
      "id": "s2",
      "step": 4,
      "type": "spotlight",
      "ids": [
        "a6",
        "a7",
        "a8"
      ]
    },
    {
      "id": "a14",
      "step": 4,
      "color": "cyan",
      "type": "box",
      "from_line": 19,
      "to_line": 21,
      "label": "borrows read-only: main keeps using t"
    },
    {
      "id": "a15",
      "step": 1,
      "color": "cyan",
      "type": "note",
      "line": 1,
      "text": "three ways to pass a value"
    },
    {
      "id": "a18",
      "step": 7,
      "type": "note",
      "line": 20,
      "text": "counts the bytes, not the letters"
    },
    {
      "id": "a19",
      "step": 7,
      "type": "box",
      "from_line": 6,
      "to_line": 8,
      "label": "t is only borrowed"
    }
  ],
  "step": 7,
  "theme": "dark",
  "padding": [
    1,
    2
  ]
}
```

</details>


## 6. Quit

Press `q`.

With the default settings, Tiza saved each change in `ownership.rs.tiza` when you made it. Your copy now has a step 7. To get the original back, download it again. See [Saving](concepts/saving.md).

## 7. Export an image

Run:

```sh
tiza render ownership.rs.tiza --step 7 --out my-step.png
```

Open `my-step.png` in an image viewer. It is in the folder `tiza-lesson`, and it shows step 7, as in [Add a note and a box](#5-add-a-note-and-a-box).

You made a step of your own in `ownership.rs.tiza`, and an image of it in `my-step.png`. For the other formats, see [Exports](reference/exports.md).

## Where next

- To annotate your own code, run `tiza FILE`. See [Quick start](../README.md#quick-start).
- To annotate a selection with keys (`V`, then `n` for a note or `b` for a box), see [Annotate a selection](reference/keys.md#annotate-a-selection).
- To present the steps to people, see [Present a take](guides/present-a-take.md).
- To present several documents in a row, for example a talk of many scenes, see [Present a long talk](guides/present-a-long-talk.md).
- To make an explanation that plays by itself, see [Animate an explanation](guides/animate-an-explanation.md).
- Before you record, see [Check before you record](guides/check-before-you-record.md).
- For all the keys, see [Keys](reference/keys.md). For all the commands, see [Commands](reference/commands.md).
