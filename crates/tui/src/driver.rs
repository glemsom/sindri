//! The seam a front end implements: how to run one prompt, and the live line
//! back into the frame while it runs. Nothing of the frame's own crosses it:
//! what a run ended on crosses as the core's own report, its stop reason
//! re-exported as [`Stop`], and a call's end as the result the executor
//! produced, so no fact of the core's is spelled twice. The client does not
//! cross it at all — `rig_wire` stays the one file that knows it.

use std::sync::mpsc::Sender;

use sindri_core::agent::{Event, Report};
use sindri_core::cancel::Cancel;
use sindri_core::exec::ToolResult;

use crate::msg::Msg;
use crate::rail::Launch;
use crate::settings::Serving;

pub use sindri_core::agent::Stopped as Stop;
pub use sindri_core::exec::Side;

/// A run that ended, and everything it did: the core's own report, handed on
/// as it is. The one shape a report cannot hold is a run that failed before it
/// had one, so that crosses as the failure in the frame's own words.
#[derive(Debug, Clone)]
pub struct Finished {
    /// Why the run ended and what it cost. `Err` is the failure in the frame's
    /// own words: a run that asked no model has no report to hand over.
    pub run: Result<Report, String>,
    /// Something the run could not do after it ended, in the frame's words: a
    /// record that could not be closed is evidence lost, and evidence nobody
    /// is told about is evidence dropped.
    pub note: Option<String>,
}

/// What a front end made of a skill a human asked for: the body is in the
/// conversation, or it was already there and nothing moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invoked {
    /// The skill went into the conversation, and the row that says so arrived as
    /// an `Event::SkillLoaded` through the tap.
    Loaded(String),
    /// It was already in force: the conversation was told nothing a second time,
    /// because a result is sent again on every turn that follows it.
    Already(String),
}

pub trait Driver: Send {
    fn run(&mut self, prompt: &str, cancel: Cancel, tap: Tap) -> Finished;

    /// Loads one skill by hand, without a turn: the conversation the body goes
    /// into is the front end's, so a frame can ask for a load and not make one.
    /// The load is announced as an `Event::SkillLoaded` on `tap`, which is how
    /// the transcript's row and the rail's chip are drawn; `Err` is why it could
    /// not be, said in the transcript where an error belongs.
    fn load(&mut self, skill: &str, tap: Tap) -> Result<Invoked, String> {
        let _ = (skill, tap);
        Err("this front end cannot load a skill".to_string())
    }

    /// Starts a fresh session — a new GUID, a new record, the same frame —
    /// and says what the rail should now show. `/new` is the only caller. A
    /// session is proved before it is adopted, so `Err` is why this one did
    /// not exist: the thread, the transcript and the record being written all
    /// stay, and the reason is said where an error belongs.
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

    /// The names the wire `serving` names serves: asked on the thread the tasks
    /// run on rather than the frame's, so the modal draws and takes every key
    /// while it is out. The ask carries the wire and the address together
    /// because a listing answers for one of each — the row a human has just
    /// moved, not the one the launch was built on. `Err` is why the wire would
    /// not say — a hint the modal and the model screen state, never a refusal,
    /// because the run this front end cannot ask is the one with no key yet —
    /// and it is read in a row, so it is a handful of words and never a
    /// server's body.
    fn models(&mut self, _serving: &Serving) -> Result<Vec<String>, String> {
        Err("this front end cannot ask the wire".to_string())
    }
}

#[derive(Debug)]
pub enum Task {
    Run(Job),
    Renew,
    Reload,
    /// One ask about the names the wire serves, and what they answer for:
    /// nothing waits on it, so it is the queue's like any other task and the
    /// frame draws while a thread is out with it.
    Ask(Serving),
    /// One skill, loaded by hand. It runs before any prompt behind it in the
    /// queue, because the driver's thread is one thread and its tasks are
    /// answered in the order they were sent.
    Load(String),
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

    /// One call's end: the result the executor produced, or nothing at all
    /// where the run was stopped under it. One value saying what ran, what it
    /// left behind and where the untruncated bytes went.
    pub fn over(&self, result: Option<ToolResult>) {
        let _ = self.tx.send(Msg::Over(result));
    }
}
