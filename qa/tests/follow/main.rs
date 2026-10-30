//! Follow mode, a paste, a reload, an undo past a reload, and the moves that
//! the code changed under.

mod follow_by_hand;
mod follow_with_moves;
mod followed_file_after_ref_or_put;
mod paste_and_stdin;
mod paste_turns_follow_off;
mod pause_leftovers;
mod pause_with_patches;
mod put_while_paused;
mod reload_on_save;
mod reload_with_code_link;
mod reopen_after_follow_off;
mod save_with_moves_applied;
mod stale_moves;
mod undo_past_a_reload;

/// The follows of `live` from now on have no watcher: the test reads the
/// followed file by hand, with `App::read_save`, and no file event and no
/// read when follow starts come in between. A watcher puts load on
/// fseventsd for every process of the machine. The tests that check what
/// a watcher sees, or that save and let the watcher read the save, keep
/// one.
fn by_hand(live: &tiza_qa::Live) {
    live.with(|app| app.set_follow_by_hand());
}
