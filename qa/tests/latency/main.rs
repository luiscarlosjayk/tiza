//! Tests that measure how fast a child process answers. They are in a crate
//! of their own, so that no other test of the gate runs beside them.

mod busy_file_events;
mod export_sizes;
mod live_check;
