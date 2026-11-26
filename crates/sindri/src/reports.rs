use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sindri::composer::Prompt;
use sindri::ledger::Facts;
use sindri_core::config::Config;
use sindri_core::launch::{Handed, Launch, Machine, Turns, served};
use sindri_core::provider::Deltas;
use sindri_core::provider::served::Listing;
use sindri_core::records::{Entry, SessionDocument, Told};
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
            let stop = match launch.drive(
                prompt.text(),
                &earlier,
                &mut reports,
                &mut turns,
                interrupted,
            ) {
                Ok(outcome) => outcome.stopped().clone(),
                Err(refusal) => Stop::Error(SecretSet::default().redact(refusal.message())),
            };
            // Read here, where the run stopped, and carried to the frame: a frame that read the
            // clock when it painted would put a different number beside the same verdict.
            let elapsed_ms = millis(started.elapsed());
            reports.push(Report::Stopped { stop, elapsed_ms });
            launch
        });
        Running {
            cancel,
            pending,
            thread,
            started,
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancel.now();
    }

    pub(crate) fn elapsed_ms(&self) -> u64 {
        millis(self.started.elapsed())
    }

    pub(crate) fn landed(&mut self) -> Vec<Report> {
        std::mem::take(&mut *lock(&self.pending))
    }

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
    ReplayBoundary,
    Text(String),
    Reasoning(String),
    Told(Told),
    Facts(Facts),
    Record {
        document: SessionDocument,
        transcript: Vec<Entry>,
    },
    Retained(Vec<Turn>),
    Stopped {
        stop: Stop,
        elapsed_ms: u64,
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
        self.wake.ping();
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
    fn turn(&mut self, _ended: &Turn) {}

    fn told(&mut self, told: Told) {
        self.push(Report::Told(told));
    }

    fn facts(&mut self, handed: Handed<'_>) {
        self.push(Report::Facts(
            Facts::of(handed.session, handed.document)
                .loading(handed.loaded.clone())
                .measured(&handed.measured, handed.window),
        ));
        self.push(Report::Record {
            document: handed.document.clone(),
            transcript: handed.transcript.to_vec(),
        });
    }

    fn replay_boundary(&mut self) {
        self.push(Report::ReplayBoundary);
    }

    fn retained(&mut self, retained: &[Turn]) {
        self.push(Report::Retained(retained.to_vec()));
    }
}

pub(crate) struct Asking {
    landing: Arc<Mutex<Option<Listing>>>,
    thread: JoinHandle<()>,
}

pub(crate) fn ask(config: &Config, wake: &Arc<Wake>) -> Option<Asking> {
    let config = config.clone();
    let landing = Arc::new(Mutex::new(None));
    let answer = Arc::clone(&landing);
    let wake = Arc::clone(wake);
    let thread = thread::spawn(move || {
        if let Some(listing) = served(&Machine::this(), &config) {
            *lock(&answer) = Some(listing);
            wake.ping();
        }
    });
    Some(Asking { landing, thread })
}

impl Asking {
    pub(crate) fn landed(&mut self) -> Option<Listing> {
        lock(&self.landing).take()
    }

    pub(crate) fn joined(self) {
        let _ = self.thread.join();
    }
}
