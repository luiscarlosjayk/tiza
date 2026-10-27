//! Tests whose result depends on time: how fast a child process answers,
//! or when file events arrive. They are in a crate of their own, so that
//! no other test of the gate runs beside them, and they take one lock, so
//! that they run one at a time.

mod burst_reload;
mod busy_file_events;
mod config_noise;
mod export_sizes;
mod live_check;

/// Taken by every test of this crate: each one times a child process, so it
/// must not share the machine with another test of this crate.
static ALONE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn alone() -> std::sync::MutexGuard<'static, ()> {
    ALONE.lock().unwrap_or_else(|e| e.into_inner())
}
