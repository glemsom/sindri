//! The seam a front end implements: how to run one prompt, and the live line
//! back into the frame while it runs (ADR-0012). No provider type crosses it:
//! the frame is told how a run ended in its own words.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use sindri_core::agent::Event;
use sindri_core::cancel::Cancel;

use crate::msg::Msg;
use crate::rail::Launch;

pub use sindri_core::exec::Side;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallOver {
    Exited(i32),
    TimedOut,
    Cancelled,
    Refused(String),
}

/// What a command's output had to leave behind: the streams the model
/// was given were cut down, and the untruncated bytes went to these
/// files (ADR-0007).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spilled {
    pub truncated: bool,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Stop {
    #[default]
    Final,
    MaxTurns,
    Cancelled,
    Error,
}

impl Stop {
    pub const fn as_str(self) -> &'static str {
        match self {
            Stop::Final => "final",
            Stop::MaxTurns => "max_turns",
            Stop::Cancelled => "cancelled",
            Stop::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cached: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Finished {
    pub stopped: Stop,
    pub turns: u32,
    pub tokens: Tokens,
    pub error: Option<String>,
    /// Something the run could not do after it ended, in the frame's words: a
    /// record that could not be closed is evidence lost, and evidence nobody
    /// is told about is evidence dropped (ADR-0006).
    pub note: Option<String>,
}

pub trait Driver: Send {
    fn run(&mut self, prompt: &str, cancel: Cancel, tap: Tap) -> Finished;

    /// Starts a fresh session — a new GUID, a new record, the same frame —
    /// and says what the rail should now show. `/new` is the only caller. A
    /// session is proved before it is adopted, so `Err` is why this one did
    /// not exist: the thread, the transcript and the record being written all
    /// stay, and the reason is said where an error belongs (ADR-0015).
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

#[derive(Debug)]
pub enum Task {
    Run(Job),
    Renew,
    Reload,
}

#[derive(Debug)]
pub struct Job {
    pub prompt: String,
    pub cancel: Cancel,
}

#[derive(Debug, Clone)]
pub struct Tap {
    tx: Sender<Msg>,
}

impl Tap {
    pub(crate) fn new(tx: Sender<Msg>) -> Self {
        Tap { tx }
    }

    pub fn event(&self, event: Event) {
        let _ = self.tx.send(Msg::Event(event));
    }

    pub fn output(&self, side: Side, text: &str) {
        let _ = self.tx.send(Msg::Output(side, text.to_string()));
    }

    pub fn over(&self, over: CallOver, spilled: Spilled) {
        let _ = self.tx.send(Msg::Over(over, spilled));
    }
}
