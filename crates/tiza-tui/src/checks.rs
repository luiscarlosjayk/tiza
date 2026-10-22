//! The live `check` off the TUI thread (t-210).
//!
//! A check of a long tutorial takes seconds (t-200's search for the
//! narrowest clean width lays every step out at many widths). Run where the
//! socket requests are answered, it froze the screen, the keys and every
//! other request until it ended. Here it runs on a thread of its own, on a
//! copy of the document taken when it was asked, and that thread answers
//! each request on the channel it came with, with the report and the
//! revision it checked.
//!
//! At most one check runs at a time. A request for the same width and
//! revision as the one running waits for it and shares its report; any
//! other waits in a queue that keeps one entry per width, holding the
//! newest copy of the document asked for at that width (its replies then
//! name that revision). The thread ends when the queue is empty: nothing
//! waits for it on the way out, so quitting is never delayed by a check.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::Value;
use tiza_core::Document;
use tiza_core::check::{CheckOptions, check_document};
use tiza_core::protocol::{LiveCheck, Response};

/// Requests waiting for one computation.
struct Batch {
    width: Option<usize>,
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
    queued: Vec<(Batch, Job)>,
}

/// The live checks of one instance (see the module documentation).
#[derive(Default)]
pub struct Checks(Arc<Mutex<State>>);

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    // A panic never happens under this lock (the check runs outside it);
    // a poisoned one still holds consistent data.
    state.lock().unwrap_or_else(|e| e.into_inner())
}

impl Checks {
    /// Answers request `id` on `reply` with the check of the document at
    /// `revision`. `snapshot` copies the document; it is called only when
    /// no running or queued check holds a copy of that revision already.
    pub fn ask(
        &self,
        id: Value,
        reply: Sender<Response>,
        width: Option<usize>,
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
            if let Some((b, job)) = st.queued.iter_mut().find(|(b, _)| b.width == width) {
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
            st.queued.push((
                batch,
                Job {
                    doc: snapshot(),
                    base,
                },
            ));
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
        let spawned = std::thread::Builder::new()
            .name("tiza-check".into())
            .spawn(move || run(&state, job, width));
        if let Err(e) = spawned {
            // Nothing will run them: every waiting request is told.
            let mut st = lock(&self.0);
            let msg = format!("check: could not start the check: {e}");
            let mut batches: Vec<Batch> = st.queued.drain(..).map(|(b, _)| b).collect();
            batches.extend(st.running.take());
            drop(st);
            for (id, tx) in batches.into_iter().flat_map(|b| b.waiters) {
                let _ = tx.send(Response::err(id, "unavailable", msg.clone()));
            }
        }
    }
}

/// The check thread: the running batch, then each queued one, until none
/// is left.
fn run(state: &Mutex<State>, mut job: Job, mut width: Option<usize>) {
    loop {
        let result = check(&job, width);
        let (done, next) = {
            let mut st = lock(state);
            let done = st.running.take();
            let next = (!st.queued.is_empty()).then(|| st.queued.remove(0));
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
fn check(job: &Job, width: Option<usize>) -> Result<tiza_core::check::Report, String> {
    crate::app::catch_request_panic("check", || {
        let report = check_document(&job.doc, job.base.as_deref(), &CheckOptions { width });
        // Test hook (t-166), only with the `test-hooks` feature: a panic
        // after the check did its work.
        #[cfg(feature = "test-hooks")]
        if std::env::var("TIZA_TEST_HOOK").is_ok_and(|v| v == "request:check") {
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
