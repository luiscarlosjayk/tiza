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
//! process [`SIGNAL_EXIT_WAIT`] after the first signal.

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
            // signal sets it, so only a second signal ends at once.
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
/// for the first SIGTERM or SIGINT, then for [`SIGNAL_EXIT_WAIT`]. If the
/// process is still there, the main thread is blocked (signal-hook restarts
/// a write that the signal broke). The watchdog removes the socket of this
/// process, if it is still there, and ends the process without the at-exit
/// hooks: they lock stdout, which the blocked thread holds.
fn watchdog() {
    let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGTERM, SIGINT]) else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("tiza-signals".into())
        .spawn(move || {
            let Some(sig) = signals.forever().next() else {
                return;
            };
            std::thread::sleep(SIGNAL_EXIT_WAIT);
            crate::ipc::remove_own_socket();
            signal_hook::low_level::exit(128 + sig);
        });
}
