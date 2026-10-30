//! The seam a front end implements: how to run one prompt, and the live line
//! back into the frame while it runs (ADR-0012). No provider type crosses it
//! (ADR-0003): the frame is told how a run ended in its own words.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use sindri_core::agent::Event;
use sindri_core::cancel::Cancel;

use crate::msg::Msg;
use crate::rail::Launch;

/// Which stream a line of live output came from.
pub use sindri_core::exec::Side;

/// How one command ended, in the frame's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallOver {
    /// The command ran and exited with this status.
    Exited(i32),
    /// Our own deadline fired and the process group was killed.
    TimedOut,
    /// The run was cancelled while the command was running.
    Cancelled,
    /// The sandbox never started, so no command ran.
    Refused(String),
}

/// What a command's output had to leave behind: the streams the model was given
/// were cut down, and the untruncated bytes went to these files (ADR-0007).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spilled {
    /// Whether either stream was cut down.
    pub truncated: bool,
    /// The files the full streams were written to.
    pub files: Vec<PathBuf>,
}

/// Why a run ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Stop {
    /// The model finished; the answer is final.
    #[default]
    Final,
    /// The turn cap was reached.
    MaxTurns,
    /// The run was cancelled with Ctrl-C.
    Cancelled,
    /// The model or its provider failed.
    Error,
}

impl Stop {
    /// The word the rail and the record use for it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Stop::Final => "final",
            Stop::MaxTurns => "max_turns",
            Stop::Cancelled => "cancelled",
            Stop::Error => "error",
        }
    }
}

/// What a run spent, as the provider reported it. Absent counters are zero: the
/// frame says a number or says nothing, and never says a wrong one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    /// Every input token, cached or not.
    pub input: u64,
    /// Every output token.
    pub output: u64,
    /// The input tokens the provider served from its cache.
    pub cached: u64,
}

/// One run, as the frame needs to hear about it.
#[derive(Debug, Clone, Default)]
pub struct Finished {
    /// Why it ended.
    pub stopped: Stop,
    /// How many turns it took.
    pub turns: u32,
    /// What it spent.
    pub tokens: Tokens,
    /// The provider's complaint, when it failed.
    pub error: Option<String>,
    /// Something the run could not do after it ended, in the frame's words: a
    /// record that could not be closed is evidence lost, and evidence nobody is
    /// told about is evidence dropped (ADR-0006, ADR-0015 §3).
    pub note: Option<String>,
}

/// What the frame asks a front end to do: one prompt, run to an answer.
pub trait Driver: Send {
    /// Runs `prompt`, feeding `tap` as it goes, and says how it ended.
    fn run(&mut self, prompt: &str, cancel: Cancel, tap: Tap) -> Finished;

    /// Starts a fresh session — a new GUID, a new record, the same frame — and
    /// says what the rail should now show. `/new` is the only caller. A session
    /// is proved before it is adopted, so `Err` is why this one did not exist:
    /// the thread, the transcript and the record being written all stay, and
    /// the reason is said where an error belongs (ADR-0015 §2).
    fn renew(&mut self) -> Result<Launch, String> {
        Err("this front end cannot start a new session".to_string())
    }

    /// Reads the config file again, so the next prompt runs on what was just
    /// saved — no restart, and no second source of truth. `/settings` is the
    /// only caller. `Ok` is what the rail should now show; `Err` is why the
    /// next run cannot start, said in the transcript where an error belongs.
    fn reload(&mut self) -> Result<Launch, String> {
        Err("this front end cannot read the config again".to_string())
    }
}

/// One thing the frame asks the front end's thread to do.
#[derive(Debug)]
pub enum Task {
    /// Run one prompt.
    Run(Job),
    /// Start a fresh session.
    Renew,
    /// Read the config file again.
    Reload,
}

/// One prompt on its way to a driver, with the flag that stops it.
#[derive(Debug)]
pub struct Job {
    /// The prompt as the composer had it.
    pub prompt: String,
    /// The flag this run checks; raising it cancels the run.
    pub cancel: Cancel,
}

/// The line back into the frame while a run is in flight. Cheap to clone, so
/// the loop and the executor can both hold one.
#[derive(Debug, Clone)]
pub struct Tap {
    tx: Sender<Msg>,
}

impl Tap {
    pub(crate) fn new(tx: Sender<Msg>) -> Self {
        Tap { tx }
    }

    /// A fragment of the run's event stream.
    pub fn event(&self, event: Event) {
        let _ = self.tx.send(Msg::Event(event));
    }

    /// One line of a running command's output, as it arrives.
    pub fn output(&self, side: Side, text: &str) {
        let _ = self.tx.send(Msg::Output(side, text.to_string()));
    }

    /// How one command ended, and what its output had to leave behind.
    pub fn over(&self, over: CallOver, spilled: Spilled) {
        let _ = self.tx.send(Msg::Over(over, spilled));
    }
}
