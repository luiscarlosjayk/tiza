//! The signals that end the TUI in order.
//!
//! - SIGHUP: the terminal went away (a pane or a tab closed, an SSH session
//!   dropped). Tiza has nothing left to do without its terminal, also when
//!   SIGHUP came in ignored (`nohup`).
//! - SIGTERM and SIGINT from outside (`kill`, a service manager; in raw
//!   mode Ctrl-C is a key, not a signal): the loop ends as with Ctrl-C, the
//!   terminal comes back, the socket goes, and the process exits with
//!   128 + the signal, as a process that a signal ends.
//!
//! Each one is a flag that the loop reads, so the process does not die
//! where it stands. A second SIGTERM or SIGINT ends the process at once. If
//! the main thread cannot end the loop (it is blocked in a write to a
//! terminal that nobody reads), a watchdog removes the socket and ends the
//! process [`SIGNAL_EXIT_WAIT`] after the first signal, or at the second.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

/// How long the watchdog lets the process end in order after SIGTERM or
/// SIGINT, before it removes the socket and ends the process.
pub const SIGNAL_EXIT_WAIT: Duration = Duration::from_secs(5);

/// The flags of the signals, shared by the binary and the TUI.
pub struct Signals {
    hup: Arc<AtomicBool>,
    /// The number of the SIGTERM or SIGINT that came, or 0.
    which: Arc<AtomicUsize>,
}

static INSTALLED: OnceLock<Signals> = OnceLock::new();

/// Catches the signals, once for the process. Call it before the socket
/// exists, so that a signal never leaves the socket behind.
pub fn install() -> &'static Signals {
    INSTALLED.get_or_init(|| {
        let hup = Arc::new(AtomicBool::new(false));
        let _ = signal_hook::flag::register(SIGHUP, Arc::clone(&hup));
        let which = Arc::new(AtomicUsize::new(0));
        let armed = Arc::new(AtomicBool::new(false));
        for sig in [SIGTERM, SIGINT] {
            // In this order: the shutdown reads `armed` before the first
            // signal sets it, so only a second signal ends at once. The two
            // actions are not one atomic step: two signals that come at the
            // same time (one on another thread, or one inside the handler
            // of the other) can both read `armed` before either sets it.
            // Then the watchdog ends the process at the second signal.
            let _ = signal_hook::flag::register_conditional_shutdown(
                sig,
                128 + sig,
                Arc::clone(&armed),
            );
            let _ = signal_hook::flag::register(sig, Arc::clone(&armed));
            let _ = signal_hook::flag::register_usize(sig, Arc::clone(&which), sig as usize);
        }
        watchdog();
        Signals { hup, which }
    })
}

impl Signals {
    /// The SIGTERM or SIGINT that came, if one did.
    pub fn ended(&self) -> Option<i32> {
        match self.which.load(Ordering::SeqCst) {
            0 => None,
            n => i32::try_from(n).ok(),
        }
    }

    /// Whether the terminal went away: SIGHUP came, or stdout is a hung-up
    /// terminal (a pty that is not the controlling terminal gets no SIGHUP).
    pub fn hung_up(&self) -> bool {
        self.hup.load(Ordering::Relaxed) || crate::hung_up(std::io::stdout())
    }
}

/// The thread that ends the process when the main thread cannot: it waits
/// for the first SIGTERM or SIGINT, then for [`SIGNAL_EXIT_WAIT`] or a
/// second signal, whichever comes first. If the process is still there
/// after the wait, the main thread is blocked (signal-hook restarts a write
/// that the signal broke); a second signal ends the process at once in any
/// case. The watchdog removes the socket of this process, if it is still
/// there, and ends the process with 128 + the first signal that it reads,
/// without the at-exit hooks: they lock stdout, which the blocked thread
/// holds. When two signals come before it reads, it reads SIGINT first.
///
/// A second signal comes here also when the handlers in [`install`] miss
/// it, because it came at the same time as the first. Limit: the kernel and
/// signal-hook keep one pending signal of each kind, so two signals of the
/// same kind at the same time can be one signal. Then only the wait ends
/// the process.
fn watchdog() {
    let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGTERM, SIGINT]) else {
        return;
    };
    // The iterator blocks with no timeout, so one thread reads the signals
    // and the watchdog waits on them with its timeout.
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::Builder::new()
        .name("tiza-signal-reader".into())
        .spawn(move || {
            for sig in signals.forever() {
                if tx.send(sig).is_err() {
                    return;
                }
            }
        });
    if reader.is_err() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("tiza-signals".into())
        .spawn(move || {
            let Ok(first) = rx.recv() else {
                return;
            };
            let end = std::time::Instant::now() + SIGNAL_EXIT_WAIT;
            if let Err(std::sync::mpsc::RecvTimeoutError::Disconnected) =
                rx.recv_timeout(SIGNAL_EXIT_WAIT)
            {
                // The reader is gone: no second signal can come.
                std::thread::sleep(end.saturating_duration_since(std::time::Instant::now()));
            }
            crate::ipc::remove_own_socket();
            signal_hook::low_level::exit(128 + first);
        });
}
