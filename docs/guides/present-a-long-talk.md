# Present a long talk

A talk is a presentation of several scenes, one after the other. Each scene is a `.tiza` document, with its own code, steps and layout. A master `.tiza` file gives the order of the scenes. Tiza plays the scenes in a row: `n` and `p` go from one scene to the next, and the audience does not see the change.

Tiza is a terminal tool that draws annotations on code. To install it, see [Install](install.md).

Use a talk when one document is not sufficient: for example, title slides without line numbers, slides in Markdown, code, and a diagram of memory, each with its own layout. For one document with a few files, a `code.link` move is sufficient. See [Several files](../reference/document-format.md#several-files).

## 1. Plan the scenes

- Make one `.tiza` file for each scene. A scene is a usual document: you open it, edit it, check it and export it alone. To make one, see [Animate an explanation](animate-an-explanation.md).
- A scene keeps its own layout: arrow lanes, margin, line numbers, `center`, padding, theme and language. A title slide without line numbers can come before a scene of code with line numbers.
- A title slide can have `"center": true`: it stays in the middle at each terminal size.
- Keep the scenes in one folder next to the master. The paths of a scene (`code`, `code.link`, `views`) are relative to its own `.tiza` file.
- The ids of annotations are for each scene. Two scenes can each have an `h1`.

For example:

```
talk.tiza
scenes/01-cover.txt.tiza     a title slide that goes on by itself
scenes/02-memory.md.tiza     a slide in Markdown
scenes/03-move.rs.tiza       code, with its error message by a code.link move
scenes/04-extra.rs.tiza      a scene that the talk leaves out
scenes/05-thanks.txt.tiza
```

## 2. Write the master

A master is a `.tiza` file with `talk` in place of `code`:

```json
{
  "schema": 2,
  "talk": {
    "title": "Ownership in Rust",
    "scenes": [
      "scenes/01-cover.txt.tiza",
      "scenes/02-memory.md.tiza",
      {"file": "scenes/03-move.rs.tiza", "name": "move"},
      {"file": "scenes/04-extra.rs.tiza", "skip": true},
      "scenes/05-thanks.txt.tiza"
    ]
  }
}
```

- An entry is a path, or an object with `file`, `name` and `skip`. A path is relative to the master.
- Each scene has a position: the number of its entry, from 1. A skipped entry has a position too, so `skip` does not change the positions of the other scenes.
- Each scene has a name: its `name`, else its file name up to the first dot (`scenes/02-memory.md.tiza` is `02-memory`). A file name of digits only, for example `07.tiza`, gives no name: use the position of the scene. Two scenes can have the same name.
- `"skip": true` leaves the scene out of the talk. Tiza does not read its file, and the file stays.
- To change the order, move a line.
- A master cannot hold another master, and a file shows once in a talk.

Tiza writes `at` into the master when it closes: you do not write it. All the fields are in [Talk](../reference/document-format.md#talk).

## 3. Check and export the talk

Run `tiza check` on the master, at the width of the terminal that you record in:

```sh
tiza check talk.tiza --width 120
```

- Tiza checks each scene that shows as `tiza check SCENE` does, with the padding and the layout of that scene. Each finding starts with the file of its scene.
- Tiza also gives each problem of the master: a scene file that is not there, a scene that is a master, a bad entry. The other scenes are checked all the same.
- The last line gives the narrowest terminal at which every scene shows its labels whole.
- `--scene X` checks one scene. See [Check before you record](check-before-you-record.md).

To make one video of the whole talk, give the master to `tiza render`:

```sh
tiza render talk.tiza --out talk.mp4
```

- The scenes that show play in a row, each as its own export: its theme, its typing, its effects and its `auto_advance_ms`.
- Every frame has the width of the export and the height of the tallest scene. A lower scene is at the top. An export does not center a scene down, also with `center`.
- The last step stays twice as long only at the end of the talk. In the talk, a scene goes on to the next one after `--interval`, or after the `auto_advance_ms` of its last step.
- A scene with `loop` plays one time.
- A GIF and a WebP work too. For an image, a text export or a `.cast` of one scene, give `--scene X`, and `--step N` for an image. See [Exports](../reference/exports.md#record-a-talk).

## 4. Present the talk

```sh
tiza talk.tiza --present
```

The first time, the talk opens at step 0 of the first scene that shows. Then go through the talk with the keys of a document:

| Key or command | What it does in a talk |
|---|---|
| `n` | Goes to the next step. On the last step of a scene, it goes to step 0 of the next scene. |
| `p` | Goes to the previous step. On step 0 of a scene, it goes to the last step of the scene before. |
| `:scene X` | Goes to scene `X`: a position (`:scene 3`) or a name (`:scene move`). |
| `:scene X STEP` | Goes to scene `X`, at a step number or a named step. |

- When a step types code, `n` finishes the typing, as in a document. The next `n` goes to the next scene.
- A step with `auto_advance_ms` goes on by itself, also from the last step of a scene to the next scene.
- In a scene with `"loop": true`, `n` on the last step goes to the next scene. The auto-advance of the last step goes back to step 0, so a title slide waits for you. In the last scene, `n` does not go back to step 0.
- `p` goes back over the steps that go on by themselves. Thus `p` on a title slide goes to the scene before. When `p` comes back to such a scene, the scene waits on its last step for your next key. In the first scene, there is no scene before: `p` goes one step back, and that step waits for your next key.
- On the last step of the last scene, `n` does not move. On step 0 of the first scene, `p` does not move.
- These rules are the same in normal mode and in presentation mode.
- With a `title` in the master, the terminal tab shows the title while you present. The tab gets its old title back when presentation mode stops or when you quit.
- `q` quits. Autosave stays on during the talk. With changes that are not saved in the scene on screen, `q` does not quit: type `:q!`. See [Present](../reference/keys.md#present).

The example talk [`examples/ownership-talk.tiza`](../../examples/ownership-talk.tiza) has three scenes. The title slide has no line numbers, and `center` puts it in the middle across. Then come four lines of code, with their line numbers, in two steps. The last slide comes after the last step of the code.

<!-- frame id=talk-cross doc=examples/ownership-talk.tiza animate=0 -->
```json
{
  "schema": 2,
  "talk": {
    "title": "Ownership in Rust",
    "scenes": [
      "talk/cover.txt.tiza",
      "talk/move.rs.tiza",
      "talk/thanks.txt.tiza"
    ]
  }
}
```

<img src="../images/guides/talk-cross.webp" width="0" alt="An animation of a talk with three scenes. First the title slide Ownership in Rust, with no line numbers. Then lines 1 to 4 of ownership.rs, with line numbers: at step 1, a move mark on s comes in, and at step 2, a note under line 4. Then the last slide, Thank you.">

## 5. Start at a scene

```sh
tiza talk.tiza --present --scene move --step 9
```

- `--scene X` takes a position or a name. Digits only are a position: `--scene 06` is position 6. A name goes to the first scene that shows with that name.
- `--step N` is the step of that scene.
- To present only a part of the talk, start at its first scene, and give `"skip": true` to the scenes that you do not show.

## 6. Stop and continue later

When Tiza closes, it writes the place in the talk into the master, as `talk.at`. The next `tiza talk.tiza` opens there. Tiza changes only `talk.at`. The other keys and the form of each entry stay, but Tiza writes the file in its own layout: `schema`, then `talk` with `title`, `scenes` and `at`, then the other keys. The first time, git thus shows a change of the layout.

- With `--no-save`, Tiza does not write the master.
- When the list of scenes changed on disk while the talk was open, Tiza does not write the master. It tells you on the terminal when it closes.
- After you move or rename a scene, the talk opens at the first scene with the name of `at`, else at the first scene.

## 7. Edit a scene while you rehearse

- A change in a scene goes to the file of that scene, with the autosave rules of a document. The master keeps only the order, the title and the place in the talk.
- Going from step to step does not change the `step` of a scene file. Thus a rehearsal does not change your scene files in git.
- Tiza reads a scene from disk again when you go to it. A change from your editor or from an agent shows when you get to that scene.
- Undo is for each scene. When you come back to a scene, `u` undoes the changes that you made there, if its files did not change on disk.
- When autosave cannot save a change, for example a read-only file or `:autosave off`, `n` and `p` do not leave the scene. The status bar tells you to type `:w`. `:next!`, `:prev!` and `:scene! X` leave the scene without the change.
- With `--no-save`, `n` and `p` always leave a scene. Its changes and its undo wait in memory: when you come back, the scene shows them. Tiza does not write them. `q` names the scenes that hold changes and does not quit. `:q!` and Ctrl-C quit without them. When the files of the scene change on disk while it waits, the scene opens from the disk, and the status bar tells you.
- The flags of the command line apply to each scene: `--theme`, `--padding`, `--no-save`, `--follow` and `--mouse`. `--step` and `--focus` apply only to the scene that the talk opens at.

## Special cases

- **`--theme-file` and `--lang`:** they do not work with a talk. Put the theme and the language in each scene.
- **`:saveas`:** it does not work in a talk, because the master names the file of each scene. `:w PATH` writes a copy. With `--no-save`, the status bar and `:q` in normal mode tell you to use `:w PATH`. `:w` with no path writes the file of the scene on screen, as in a document.
- **`tiza render` of a whole talk:** it writes a GIF, a WebP, an MP4 or a WebM. A still image, a text export, `--all-steps` and a `.cast` need `--scene X`. A `.tape` needs the file of the scene: `tiza render SCENE.tiza --out demo.tape`.
- **`tiza next`, `tiza prev` and `tiza goto ID`:** they act on the live talk as `n`, `p` and `:goto ID` do. Thus `tiza next` on the last step of a scene goes to the next scene. `tiza goto ID` stays in the scene on screen. When you type a `:` command or have a panel open, `tiza next` and `tiza prev` do not change the scene: they give an error, and the agent asks again later. Changes that are not saved also stop them, with the error `unsaved_changes`.
- **A scene file is not there, or Tiza cannot read it:** `tiza talk.tiza` does not open, and the message names the scene. Tiza does not read the file of a skipped scene, so that file can be missing.
- **A scene file breaks while the talk is open:** when you go to that scene, the status bar tells why, and the scene on screen stays.
- **A `scene put` of another document:** it turns autosave off for that scene only. The next scene has autosave on.
- **An agent change that waits for your review:** it gets `unavailable` when you go to another scene. Tiza does not apply it.
- **A session has one instance:** a second `tiza talk.tiza` in the same session does not start. See [Close an instance](../reference/cli.md#close-an-instance).

## See also

- [Present a take](present-a-take.md)
- [Check before you record](check-before-you-record.md)
- [Animate an explanation](animate-an-explanation.md)
- [Talk](../reference/document-format.md#talk)
