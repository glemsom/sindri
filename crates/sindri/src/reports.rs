//! The threads a run in flight and a wire listing run on, and the reports they land: a flight
//! pushes a fragment and raises the pipe a signal uses, and paints no cell of its own. Nothing
//! here reads a key.

use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sindri::composer::Prompt;
use sindri::ledger::Facts;
use sindri_core::config::Config;
use sindri_core::launch::{Handed, Launch, Turns, served};
use sindri_core::provider::Deltas;
use sindri_core::provider::served::Listing;
use sindri_core::records::{SessionDocument, Told};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Cancel, Stop, Turn};

use crate::signal::Wake;
use crate::term::lock;

pub(crate) struct Running {
    cancel: Cancel,
    pending: Arc<Mutex<Vec<Report>>>,
    thread: JoinHandle<Launch>,
    started: Instant,
}

impl Running {
    /// Drives `prompt` in its own thread: the fragment a wire lands is pushed where the loop takes
    /// it, and the loop is pinged through the pipe a signal uses, so nothing polls for it. The thread
    /// hands the launch back when the run stops, so the prompt after it runs in the same session.
    pub(crate) fn start(
        launch: Launch,
        prompt: Prompt,
        earlier: Vec<Turn>,
        wake: Arc<Wake>,
    ) -> Running {
        let cancel = Cancel::new();
        let pending = Arc::new(Mutex::new(Vec::new()));
        let mut reports = Reports {
            pending: Arc::clone(&pending),
            wake,
        };
        let mut turns = reports.clone();
        let interrupted = cancel.clone();
        let started = Instant::now();
        let thread = thread::spawn(move || {
            let mut launch = launch;
            // `drive` failing leaves no stop reason, which the loop cannot do: the marker settles
            // on the refusal rather than turning for a run nobody is driving.
            let (turns, stop) = match launch.drive(
                prompt.text(),
                &earlier,
                &mut reports,
                &mut turns,
                interrupted,
            ) {
                Ok(outcome) => (outcome.turns(), outcome.stopped().clone()),
                Err(refusal) => (
                    0,
                    Stop::Error(SecretSet::default().redact(refusal.message())),
                ),
            };
            // Read here, where the run stopped, and carried to the frame: a frame that read the
            // clock when it painted would put a different number beside the same verdict.
            let elapsed_ms = millis(started.elapsed());
            reports.push(Report::Stopped {
                stop,
                elapsed_ms,
                turns,
            });
            launch
        });
        Running {
            cancel,
            pending,
            thread,
            started,
        }
    }

    /// The user's word: the run stops when the request in flight does, and the command it is in
    /// flight with is killed now.
    pub(crate) fn cancel(&self) {
        self.cancel.now();
    }

    pub(crate) fn elapsed_ms(&self) -> u64 {
        millis(self.started.elapsed())
    }

    /// Everything the run landed since the last pass. The queue is drained, so each report is
    /// handed out once.
    pub(crate) fn landed(&mut self) -> Vec<Report> {
        std::mem::take(&mut *lock(&self.pending))
    }

    /// Whether the run's thread ended within `grace`: a wait rather than a join, because a request
    /// the wire never answers must not hold a launch that is ending.
    pub(crate) fn finished_within(&self, grace: Duration) -> bool {
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if self.thread.is_finished() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        self.thread.is_finished()
    }

    /// The launch the run handed back, or nothing if its thread panicked. Only ever called on a
    /// thread that has ended: `join` on a live one blocks until the run does.
    pub(crate) fn handed_back(self) -> Option<Launch> {
        self.thread.join().ok()
    }
}

fn millis(span: Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}

pub(crate) enum Report {
    Text(String),
    Reasoning(String),
    Told(Told),
    Facts(Facts),
    Record(SessionDocument),
    Kept(Turn),
    Stopped {
        stop: Stop,
        elapsed_ms: u64,
        turns: u32,
    },
}

#[derive(Clone)]
struct Reports {
    pending: Arc<Mutex<Vec<Report>>>,
    wake: Arc<Wake>,
}

impl Reports {
    fn push(&mut self, report: Report) {
        lock(&self.pending).push(report);
        self.wake.raise();
    }
}

impl Deltas for Reports {
    fn text(&mut self, fragment: &str) {
        self.push(Report::Text(fragment.to_owned()));
    }

    fn reasoning(&mut self, fragment: &str) {
        self.push(Report::Reasoning(fragment.to_owned()));
    }
}

impl Turns for Reports {
    /// A turn that asked for calls keeps the run going, so the count a status bar wants moves on; a
    /// turn that asked for none ended the run, and the stop reason is the word a terminal is owed,
    /// since "asking the model" would be a status about a turn this run will never start.
    fn turn(&mut self, ended: &Turn) {
        if !ended.calls.is_empty() {
            self.push(Report::Told(Told::Asking {
                turn: ended.turn + 1,
            }));
        }
    }

    fn facts(&mut self, handed: Handed<'_>) {
        self.push(Report::Facts(
            Facts::of(handed.session, handed.document).measured(&handed.measured, handed.window),
        ));
        self.push(Report::Record(handed.document.clone()));
    }

    fn kept(&mut self, kept: &Turn) {
        self.push(Report::Kept(kept.clone()));
    }
}

/// A wire being asked what it serves, on a thread of its own: `/model` opens its screen at once
/// and the names land when they land, because the loop owes a key its answer now.
pub(crate) struct Asking {
    landing: Arc<Mutex<Option<Listing>>>,
    thread: JoinHandle<()>,
}

/// Asks the wire the config names what it serves, and pings the loop through the pipe a signal
/// uses when the answer is in: one more fd the loop waits on, and no timer.
pub(crate) fn ask(config: &Config, wake: &Arc<Wake>) -> Option<Asking> {
    let config = config.clone();
    let landing = Arc::new(Mutex::new(None));
    let answer = Arc::clone(&landing);
    let wake = Arc::clone(wake);
    let thread = thread::spawn(move || {
        // A launch with no wire is one `main` refused before the screen was taken, so the only
        // answer this thread can leave is a listing.
        if let Some(listing) = served(&config) {
            *lock(&answer) = Some(listing);
            wake.raise();
        }
    });
    // The loop joins it when the listing lands: nothing blocks here, and the frame paints the
    // screen asking until then.
    Some(Asking { landing, thread })
}

impl Asking {
    /// The listing the wire answered with, once.
    pub(crate) fn landed(&mut self) -> Option<Listing> {
        lock(&self.landing).take()
    }

    /// Joins the thread that asked, and keeps no word of it: the listing was taken before, so a
    /// thread that panicked has nothing left to say.
    pub(crate) fn joined(self) {
        let _ = self.thread.join();
    }
}
