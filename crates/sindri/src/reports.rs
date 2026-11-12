//! The threads a run in flight and a wire listing run on, the reports they land, and the pass
//! that takes them: a flight pushes a fragment and raises the pipe a signal uses, and the pass
//! puts it on the frame when the loop wakes. Nothing here reads a key.

use std::fs::File;
use std::io;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sindri::composer::{Prompt, State};
use sindri::ledger::Facts;
use sindri::pulse::Pulse;
use sindri::view::View;
use sindri_core::config::Config;
use sindri_core::provider::Deltas;
use sindri_core::provider::served::Listing;
use sindri_core::records::{SessionDocument, Told};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Cancel, Exchange, Stop};

use crate::launch::{Launch, Turns};
use crate::osc::Teller;
use crate::signal::Wake;
use crate::term::{Canvas, lock, paint};

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
        earlier: Vec<Exchange>,
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
}

fn millis(span: Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}

enum Report {
    Text(String),
    Reasoning(String),
    Told(Told),
    Facts(Facts),
    Record(SessionDocument),
    Kept(Exchange),
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
    fn turn(&mut self, ended: &Exchange) {
        if !ended.calls.is_empty() {
            self.push(Report::Told(Told::Asking {
                turn: ended.turn + 1,
            }));
        }
    }

    fn facts(&mut self, facts: Facts) {
        self.push(Report::Facts(facts));
    }

    fn record(&mut self, document: &SessionDocument) {
        self.push(Report::Record(document.clone()));
    }

    fn kept(&mut self, kept: &Exchange) {
        self.push(Report::Kept(kept.clone()));
    }
}

/// Takes everything the run left and puts it on the frame, and says whether the run ended. A run
/// that ended leaves the composer idle and its thread is joined rather than left for the process to
/// exit under — which is where the launch the run held is taken back, for the prompt after it.
pub(crate) fn take(
    canvas: &mut Canvas<'_>,
    view: &mut View,
    running: &mut Option<Running>,
    telling: &mut Teller<File>,
    held: &mut Option<Launch>,
) -> io::Result<bool> {
    let Some(flight) = running.as_ref() else {
        return Ok(false);
    };
    let left = std::mem::take(&mut *lock(&flight.pending));
    let mut painted = false;
    let mut stopped = None;
    for report in left {
        match report {
            Report::Text(fragment) => {
                view.text(&fragment);
                painted = true;
            }
            Report::Reasoning(fragment) => {
                view.reasoning(&fragment);
                painted = true;
            }
            // A status changes no cell: it is written by the one thing here that owns a tty.
            Report::Told(told) => telling.say(told)?,
            Report::Facts(facts) => {
                view.set_facts(facts);
                painted = true;
            }
            Report::Record(document) => {
                view.set_record(&document);
                painted = true;
            }
            // No cell: the transcript already drew the exchange it keeps.
            Report::Kept(kept) => view.kept(&kept),
            Report::Stopped {
                stop,
                elapsed_ms,
                turns,
            } => stopped = Some((stop, elapsed_ms, turns)),
        }
    }
    if let Some((ref stop, elapsed_ms, turns)) = stopped {
        // Said at once, whatever the pace says about a turn: a stop reason is a change of state.
        telling.say(Told::Stopped {
            turn: turns,
            stop: stop.clone(),
        })?;
        // The last frame the run itself paints: once it is gone the wait goes back to forever,
        // and the settled marker stays on the frame for every later paint to draw.
        view.set_pulse(Some(Pulse::settled(stop, elapsed_ms)));
        view.set_state(State::Idle);
        painted = true;
        if let Some(flight) = running.take()
            && let Ok(launch) = flight.thread.join()
        {
            // A run that panicked took its launch with it, so the next prompt opens a session of its
            // own rather than running on one nothing knows the state of.
            *held = Some(launch);
        }
    }
    if painted {
        paint(canvas, view)?;
    }
    Ok(stopped.is_some())
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
        if let Some(listing) = crate::launch::served(&config) {
            *lock(&answer) = Some(listing);
            wake.raise();
        }
    });
    // Joined by `served`, not here: nothing blocks, and the frame paints the screen asking
    // until the answer lands.
    Some(Asking { landing, thread })
}

/// Puts the names a wire answered with on the frame, and drops the thread that asked them: the
/// `/model` screen is painted once with the listing it was waiting for.
pub(crate) fn served(
    canvas: &mut Canvas<'_>,
    view: &mut View,
    asking: &mut Option<Asking>,
) -> io::Result<()> {
    let landed = match asking.as_ref() {
        Some(flight) => lock(&flight.landing).take(),
        None => None,
    };
    let Some(listing) = landed else {
        return Ok(());
    };
    view.set_served(listing);
    paint(canvas, view)?;
    if let Some(flight) = asking.take() {
        let _ = flight.thread.join();
    }
    Ok(())
}
