# Drive a talk

A talk is a master `.tiza` file that lists other `.tiza` files, the scenes. The presenter opens it with `tiza talk.tiza --present`, and `n` goes from the last step of a scene to the next scene. See [Present a long talk](../guides/present-a-long-talk.md).

An agent can see the talk, go to a scene, and change the list, with no script.

## See the talk

- `status` has `talk` when the instance plays a talk: the master, and the `position` and the `name` of the scene on screen. A document alone has no `talk`.
- `talk_status` gives the same, and each entry of the list with its `entry`, `position`, `name`, `skip` and `steps`.
- The other tools act on the scene on screen: `get_code`, the add tools, `render_text`, `export_image`. An annotation id is unique in its scene only.

## Go through the scenes

- `next_step` after the last step of a scene opens the next scene that shows, at step 0. Its reply has `scene`. Look at it: step 0 of the next scene can look like the `loop` of a document.
- `goto_scene` with `{"scene": 6}` or `{"scene": "move", "step": "borrow"}` opens a scene at a step. A position counts every entry from 1, also a skipped entry.
- When the scene on screen has changes that Tiza could not save, these tools refuse with `unsaved_changes`. Ask the presenter to save with `:w`. Give `force: true` only when the presenter says that the changes can go.
- The requests of one batch go in order. A request after a change of scene acts on the new scene.

## Change the list

- `set_talk_scenes` replaces the whole list and writes it in the master. Take the `entry` values of `talk_status`, change them, and send them back. An entry that you leave out leaves the talk.
- Give `{"file": "x.tiza", "skip": true}` to keep a scene in the list and not show it. The scene on screen can get `skip`, but you cannot remove it: go to another scene first.
- `insert_scene` adds a scene at a position. With `scene`, a whole document with its code, it writes the new file first, in the folder of the master. It never writes over a file that exists.
- When the master changed on disk, for example because you edited it with a file tool, these tools refuse with `conflict`. Give `force: true` to write your list over it.
- With `--no-save`, Tiza does not write the master, and these tools refuse.

The rules and the fields are in [Talk methods](../reference/socket-api.md#talk-methods).
