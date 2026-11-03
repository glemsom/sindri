//! What a batch run tells the terminal it draws in, on stderr and in one
//! `OSC 7501` report at a time: the encoder is the core's, and so is the channel
//! that says it whole — the title, the gate on saying one and the writer
//! together (`status::Channel`). What lives here is the run's own bookkeeping —
//! when it began, the thread that watches the loop's events, and the two moments
//! a run announces without one.
//!
//! stdout is the answer, and a script parsing it must not find an escape in it,
//! so a report goes to stderr — which the terminal parses all the same, because
//! it reads the pty and not one descriptor — and only where stderr is a
//! terminal: an escape written into `2>log` is bytes in a log, and bytes in a
//! log are the one thing this feature must never add. Whether stderr is a
//! terminal is this front end's fact and is decided here, once, and handed to
//! the channel as a writer; the setting (`status`, and `SINDRI_STATUS` above it)
//! turns the whole thing off from one place both front ends read.

use std::io::IsTerminal;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Instant;

use sindri_core::agent::{Event, Stopped};
use sindri_core::status::{Channel, Doing, State, title};

/// The reports one run makes where there is a terminal to tell, and nothing at
/// all where there is not or the setting silences them.
pub struct Reports {
    /// The title every report carries: the workspace's own name.
    title: String,
    /// When the run began, which is what a message's seconds count from.
    start: Instant,
    /// The channel a run says its own moments through: the one before its first
    /// turn, and the one it stopped in.
    channel: Channel,
    /// Whether a report goes anywhere at all — the setting and the terminal —
    /// kept because the thread that watches the events builds a channel of its
    /// own from it.
    told: bool,
}

impl Reports {
    /// The reports a run standing in `workspace` makes, when the setting asks
    /// for them and stderr is a terminal: the setting is the config's, so every
    /// front end reads one value, and the writer is this front end's own.
    pub fn new(workspace: &Path, wanted: bool) -> Reports {
        let title = title(workspace).into_owned();
        let told = wanted && std::io::stderr().is_terminal();
        Reports {
            channel: Channel::new(title.clone(), told, Box::new(std::io::stderr())),
            title,
            start: Instant::now(),
            told,
        }
    }

    /// The report a run makes before its first turn: nothing has been asked of
    /// it yet, and thinking is what the first turn is doing. The channel's first
    /// word, and so one the gate has nothing to withhold.
    pub fn begun(&mut self) {
        self.channel.say(
            &State::Working {
                doing: Doing::Thinking,
                turn: 1,
                elapsing: self.start.elapsed(),
            },
            Instant::now(),
        );
    }

    /// The report a run ends in, which is `Stopped`'s mapping and no second one:
    /// a cap or a budget delivered an answer and says `done`, an interrupt
    /// leaves the terminal `idle`, and a run that could not deliver says so.
    pub fn settled(&mut self, stopped: Stopped) {
        self.channel.last(&State::settled(stopped));
    }

    /// The one word for a run that never became a report: the provider or the
    /// transport failed, so the loop had no stop reason to hand back.
    pub fn failed(&mut self) {
        self.channel.last(&State::Error);
    }

    /// Watches the loop's events on a thread of its own, saying what the run is
    /// doing as the channel allows: a change of `Doing` at once, an unchanged one
    /// only once the second is out. The handle ends where the loop closes the
    /// channel, and hands back the events it kept, so `-v` still has the whole
    /// stream.
    pub fn watch(&self, events: Receiver<Event>, keep: bool) -> Watching {
        let mut channel = Channel::new(self.title.clone(), self.told, Box::new(std::io::stderr()));
        let start = self.start;
        let thread = std::thread::spawn(move || {
            let mut kept = Vec::new();
            for event in events {
                let (turn, doing) = event.doing();
                if keep {
                    kept.push(event);
                }
                let state = State::Working {
                    doing,
                    turn,
                    elapsing: start.elapsed(),
                };
                channel.say(&state, Instant::now());
            }
            kept
        });
        Watching { thread }
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
