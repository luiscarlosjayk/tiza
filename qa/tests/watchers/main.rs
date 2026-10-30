//! Tests that run a TUI with follow on in a pseudo-terminal. macOS makes a
//! new file watcher wait while a process starts and drops others, so these
//! tests are in a crate of their own, away from the tests that start watchers
//! in the test process.

mod ctrl_c_over_follow;
mod follow_with_moves;
mod pause_while_the_watch_starts;
mod reopen_after_follow_off;
mod rollback_while_paused;
