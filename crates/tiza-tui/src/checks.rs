//! The live `check` off the TUI thread.
//!
//! A check of a long tutorial takes seconds (the search for the
//! narrowest clean width lays every step out at many widths). Run where the
//! socket requests are answered, it froze the screen, the keys and every
//! other request until it ended. Here it runs on a thread of its own, on a
//! copy of the document, and that thread answers each request on the
//! channel it came with, with the report and the revision it checked.
//!
//! At most one check runs and one waits. A request for the same width and
//! revision as the one running joins it and shares its report. Any other
//! takes the place that waits: a request for the same width as the one
//! waiting joins it (the copy becomes the newest one asked for, and every
//! reply of that batch names that revision); one for another width replaces
//! it, and the requests it replaced are answered `busy` at once. The width
//! compared is the one the check uses (`{}` is `{"width": 100}`). The thread
//! ends when nothing waits: nothing waits for it on the way out, so quitting
//! is never delayed by a check.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::Value;
use tiza_core::Document;
use tiza_core::check::{CheckOptions, check_document};
use tiza_core::protocol::{LiveCheck, Response};

/// Requests waiting for one computation.
struct Batch {
    width: usize,
    revision: u64,
    /// Each request's id and the channel its reply goes back on.
    waiters: Vec<(Value, Sender<Response>)>,
}

/// What a check needs besides its batch: the copy of the document and the
/// directory linked files are read from.
struct Job {
    doc: Document,
    base: Option<PathBuf>,
}

#[derive(Default)]
struct State {
    running: Option<Batch>,
    waiting: Option<(Batch, Job)>,
}

/// The live checks of one instance (see the module documentation).
#[derive(Default)]
pub struct Checks(Arc<Mutex<State>>, #[cfg(feature = "test-hooks")] Arc<Hold>);

/// Test hook, only with the `test-hooks` feature: while it is on, a check
/// thread that starts waits before its first computation, with its batch
/// already the one that runs. A test asks for more checks then, and they
/// join or wait as they would beside a long check, whatever the speed of
/// the machine. The batch that waits is computed when the first one ends,
/// with no hold.
///
/// A test sees that a check waits ([`Hold::wait_held`]; `status.checks` on
/// a pty). A hold ends by itself after 10 s and says so
/// ([`Hold::expired`]): a test that keeps it longer fails when it lets it
/// go, and never hangs.
#[cfg(feature = "test-hooks")]
#[derive(Default)]
pub struct Hold {
    state: Mutex<HoldState>,
    changed: std::sync::Condvar,
}

#[cfg(feature = "test-hooks")]
#[derive(Default)]
struct HoldState {
    on: bool,
    /// The check threads that wait now, in the hold or for the file.
    held: usize,
    expired: bool,
}

#[cfg(feature = "test-hooks")]
impl Hold {
    /// How long a check waits while the hold is on.
    const LIMIT: std::time::Duration = std::time::Duration::from_secs(10);
    /// How long a check waits for the file `check-hold`: longer, as the
    /// scripts on a pty start a process for each request. A check that
    /// waits blocks neither the TUI nor its exit.
    const FILE_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

    fn state(&self) -> MutexGuard<'_, HoldState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Holds the checks (`true`) or lets them go (`false`).
    pub fn set(&self, on: bool) {
        self.state().on = on;
        self.changed.notify_all();
    }

    /// The check threads that wait now.
    pub fn held(&self) -> usize {
        self.state().held
    }

    /// Whether a check stopped waiting because the hold lasted 10 s.
    pub fn expired(&self) -> bool {
        self.state().expired
    }

    /// Waits until a check thread waits, at most `limit`; `false` if none
    /// did.
    pub fn wait_held(&self, limit: std::time::Duration) -> bool {
        let st = self.state();
        let (st, _) = self
            .changed
            .wait_timeout_while(st, limit, |st| st.held == 0)
            .unwrap_or_else(|e| e.into_inner());
        st.held > 0
    }

    /// Waits while the hold is on. Then, for a TUI on a pty with
    /// `TIZA_TEST_HOOK=check:hold`, waits while the file `check-hold` is in
    /// the runtime folder of Tiza.
    fn wait(&self) {
        let mut st = self.state();
        st.held += 1;
        self.changed.notify_all();
        let (mut st, waited) = self
            .changed
            .wait_timeout_while(st, Self::LIMIT, |st| st.on)
            .unwrap_or_else(|e| e.into_inner());
        if waited.timed_out() {
            st.expired = true;
        }
        drop(st);
        if crate::test_hook("check:hold") {
            let file = tiza_core::socket::runtime_dir().join("check-hold");
            let start = std::time::Instant::now();
            while file.exists() && start.elapsed() < Self::FILE_LIMIT {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        self.state().held -= 1;
        self.changed.notify_all();
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    // A panic never happens under this lock (the check runs outside it);
    // a poisoned one still holds consistent data.
    state.lock().unwrap_or_else(|e| e.into_inner())
}

impl Checks {
    /// Test hook, only with the `test-hooks` feature: the hold of these
    /// checks ([`Hold`]).
    #[cfg(feature = "test-hooks")]
    pub fn hold(&self) -> Arc<Hold> {
        Arc::clone(&self.1)
    }

    /// Test hook, only with the `test-hooks` feature: the requests that
    /// wait for the check that runs.
    #[cfg(feature = "test-hooks")]
    pub fn running_waiters(&self) -> usize {
        lock(&self.0)
            .running
            .as_ref()
            .map_or(0, |b| b.waiters.len())
    }

    /// Answers request `id` on `reply` with the check of the document at
    /// `revision`, at `width` (the one the check uses, never "the
    /// default"). `snapshot` copies the document; it is called only when no
    /// running or waiting check holds a copy of that revision already.
    pub fn ask(
        &self,
        id: Value,
        reply: Sender<Response>,
        width: usize,
        revision: u64,
        snapshot: impl FnOnce() -> Document,
        base: Option<PathBuf>,
    ) {
        let mut st = lock(&self.0);
        if let Some(r) = &mut st.running {
            if r.width == width && r.revision == revision {
                r.waiters.push((id, reply));
                return;
            }
            if let Some((b, job)) = &mut st.waiting
                && b.width == width
            {
                if b.revision != revision {
                    *job = Job {
                        doc: snapshot(),
                        base,
                    };
                    b.revision = revision;
                }
                b.waiters.push((id, reply));
                return;
            }
            let batch = Batch {
                width,
                revision,
                waiters: vec![(id, reply)],
            };
            let replaced = st.waiting.replace((
                batch,
                Job {
                    doc: snapshot(),
                    base,
                },
            ));
            drop(st);
            if let Some((b, _)) = replaced {
                let msg = format!(
                    "check: replaced by a newer check (width {width}) before it ran; ask again"
                );
                for (id, tx) in b.waiters {
                    let _ = tx.send(Response::err(id, "busy", msg.clone()));
                }
            }
            return;
        }
        st.running = Some(Batch {
            width,
            revision,
            waiters: vec![(id, reply)],
        });
        let job = Job {
            doc: snapshot(),
            base,
        };
        drop(st);
        let state = Arc::clone(&self.0);
        #[cfg(feature = "test-hooks")]
        let hold = Arc::clone(&self.1);
        let spawned = std::thread::Builder::new()
            .name("tiza-check".into())
            .spawn(move || {
                #[cfg(feature = "test-hooks")]
                hold.wait();
                run(&state, job, width)
            });
        if let Err(e) = spawned {
            // Nothing will run them: every waiting request is told.
            let mut st = lock(&self.0);
            let msg = format!("check: could not start the check: {e}");
            let mut batches: Vec<Batch> = st.waiting.take().map(|(b, _)| b).into_iter().collect();
            batches.extend(st.running.take());
            drop(st);
            for (id, tx) in batches.into_iter().flat_map(|b| b.waiters) {
                let _ = tx.send(Response::err(id, "unavailable", msg.clone()));
            }
        }
    }
}

/// The check thread: the running batch, then the waiting one, until none
/// is left.
fn run(state: &Mutex<State>, mut job: Job, mut width: usize) {
    loop {
        let result = check(&job, width);
        let (done, next) = {
            let mut st = lock(state);
            let done = st.running.take();
            let next = st.waiting.take();
            (
                done,
                next.map(|(b, j)| {
                    width = b.width;
                    st.running = Some(b);
                    j
                }),
            )
        };
        answer(done, &result);
        match next {
            Some(j) => job = j,
            None => return,
        }
    }
}

/// The report, or the message of a bug that stopped it.
fn check(job: &Job, width: usize) -> Result<tiza_core::check::Report, String> {
    crate::app::catch_request_panic("check", || {
        let report = check_document(
            &job.doc,
            job.base.as_deref(),
            &CheckOptions {
                width: Some(width),
                numbers: None,
            },
        );
        // Test hook, only with the `test-hooks` feature: a panic
        // after the check did its work.
        #[cfg(feature = "test-hooks")]
        if crate::test_hook("request:check") {
            panic!("TIZA_TEST_HOOK at check");
        }
        report
    })
    .map_err(|log| {
        format!(
            "check: a bug in Tiza stopped this request (a panic); please report it{}",
            match log {
                Some(p) => format!(" (details in {})", p.display()),
                None => String::new(),
            }
        )
    })
}

/// Sends the result to every request of `batch`; one whose client is gone
/// is skipped.
fn answer(batch: Option<Batch>, result: &Result<tiza_core::check::Report, String>) {
    let Some(batch) = batch else { return };
    let value = result.as_ref().map(|report| {
        serde_json::to_value(LiveCheck {
            report: report.clone(),
            revision: batch.revision,
        })
    });
    for (id, tx) in batch.waiters {
        let resp = match &value {
            Ok(Ok(v)) => Response::ok(id, v.clone()),
            Ok(Err(e)) => Response::err(id, "internal", e.to_string()),
            Err(msg) => Response::err(id, "internal", msg.to_string()),
        };
        let _ = tx.send(resp);
    }
}
