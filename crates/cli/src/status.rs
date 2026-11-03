//! What a batch run tells the terminal it draws in, on stderr and in one
//! `OSC 7501` report at a time: the encoder is the core's and so is the gate on
//! saying it (`status::Told`), because both front ends repeat states and a
//! second copy of "not twice, not within a second" is the copy that drifts. What
//! lives here is the run's own bookkeeping — the title, the writer on stderr,
//! the thread that watches the loop's events, and the two moments a run
//! announces without one.
//!
//! stdout is the answer, and a script parsing it must not find an escape in it,
//! so a report goes to stderr — which the terminal parses all the same, because
//! it reads the pty and not one descriptor — and only where stderr is a
//! terminal: an escape written into `2>log` is bytes in a log, and bytes in a
//! log are the one thing this feature must never add. The setting (`status`,
//! and `SINDRI_STATUS` above it) turns the whole channel off from one place both
//! front ends read, so a machine that finds the reports noisy silences them once.

use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Instant;

use sindri_core::agent::{Event, Stopped};
use sindri_core::status::{self, Doing, State, Told};

/// The reports one run makes where there is a terminal to tell, and nothing at
/// all where there is not or the setting silences them. Cheap to clone because
/// the thread that watches the events holds one of its own.
#[derive(Clone)]
pub struct Reports {
    /// The title every report carries: the workspace's own name.
    title: String,
    /// When the run began, which is what a message's seconds count from.
    start: Instant,
    /// Whether a report goes anywhere at all.
    told: bool,
}

impl Reports {
    /// The reports a run standing in `workspace` makes, when the setting asks
    /// for them and stderr is a terminal: the setting is the config's, so every
    /// front end reads one value, and the channel is this front end's own.
    pub fn new(workspace: &Path, wanted: bool) -> Reports {
        Reports {
            title: status::title(workspace).into_owned(),
            start: Instant::now(),
            told: wanted && std::io::stderr().is_terminal(),
        }
    }

    /// The report a run makes before its first turn: nothing has been asked of
    /// it yet, and thinking is what the first turn is doing.
    pub fn begun(&self) {
        self.say(State::Working {
            doing: Doing::Thinking,
            turn: 1,
            elapsing: self.start.elapsed(),
        });
    }

    /// The report a run ends in, which is `Stopped`'s mapping and no second one:
    /// a cap or a budget delivered an answer and says `done`, an interrupt
    /// leaves the terminal `idle`, and a run that could not deliver says so.
    pub fn settled(&self, stopped: Stopped) {
        self.say(State::settled(stopped));
    }

    /// The one word for a run that never became a report: the provider or the
    /// transport failed, so the loop had no stop reason to hand back.
    pub fn failed(&self) {
        self.say(State::Error);
    }

    /// Watches the loop's events on a thread of its own, saying what the run is
    /// doing as `Told` allows: a change of `Doing` at once, an unchanged one only
    /// once the second is out. The handle ends where the loop closes the channel,
    /// and hands back the events it kept, so `-v` still has the whole stream.
    pub fn watch(&self, events: Receiver<Event>, keep: bool) -> Watching {
        let reports = self.clone();
        let thread = std::thread::spawn(move || {
            let mut told = Told::new();
            let mut kept = Vec::new();
            for event in events {
                let (turn, doing) = of(&event);
                if keep {
                    kept.push(event);
                }
                let state = State::Working {
                    doing,
                    turn,
                    elapsing: reports.start.elapsed(),
                };
                if let Some(bytes) = told.next(&state, &reports.title, Instant::now()) {
                    reports.write(&bytes);
                }
            }
            kept
        });
        Watching { thread }
    }

    /// One report a run owes whatever the second says, because the run is over:
    /// the gate is for a stream of states, and the last one is not a repeat.
    fn say(&self, state: State) {
        // A report past a ceiling is not sent at all — the encoder refuses it
        // rather than shortening it — because a terminal discards the whole
        // sequence and a run that shortened it would think it had spoken.
        if let Some(bytes) = status::report(&state, &self.title) {
            self.write(&bytes);
        }
    }

    /// One report, written whole in one write: a terminal parses every byte it
    /// is handed, and half a sequence is a sequence it discards. The one place
    /// the channel is asked, so no caller of it can write without asking.
    fn write(&self, bytes: &[u8]) {
        if !self.told {
            return;
        }
        let mut out = std::io::stderr().lock();
        // A failure to write is a terminal that has gone: the report is a
        // courtesy and the run is not, so neither is the run's failure.
        let _ = out.write_all(bytes);
        let _ = out.flush();
    }
}

/// The thread that says what a run is doing, and the events it kept on the way.
pub struct Watching {
    thread: JoinHandle<Vec<Event>>,
}

impl Watching {
    /// Waits for the loop to close the channel and hands back every event it
    /// carried, in order. A thread that panicked kept nothing, which is a
    /// verbose run saying less rather than a run that failed.
    pub fn events(self) -> Vec<Event> {
        self.thread.join().unwrap_or_default()
    }
}

/// What one event says the run is doing, and in which turn: the stream the loop
/// already emits, read as `Doing`'s words rather than parsed by a front end.
fn of(event: &Event) -> (u32, Doing) {
    match event {
        Event::ToolCall { turn, asked } => (*turn, Doing::of(asked)),
        Event::SkillLoaded { turn, .. } => (*turn, Doing::Skill),
        Event::TextDelta { turn, .. } => (*turn, Doing::Answer),
        Event::Reasoning { turn, .. } | Event::ReasoningDelta { turn, .. } => {
            (*turn, Doing::Thinking)
        }
    }
}
