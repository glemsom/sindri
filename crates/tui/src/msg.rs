//! What reaches the frame from outside it: a key, a fragment of the run, and
//! the run's own end. One channel, so nothing overtakes anything else by more
//! than the thread that carried it.

use crossterm::event::{KeyEvent, MouseEvent};
use sindri_core::agent::Event;
use sindri_core::exec::ToolResult;

use crate::driver::{Finished, Invoked, Side};
use crate::picture::{Drawing, Refusal};
use crate::rail::Launch;

#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    Mouse(MouseEvent),
    /// A paste, whole and unasked for: a draft and never a keystroke, so no
    /// line break in it is an `Enter`.
    Paste(String),
    /// The terminal changed size. Nothing in the frame changes: the next
    /// frame asks the terminal the new one, and this message is what asks
    /// for that frame.
    Resize,
    Event(Event),
    Output(Side, String),
    /// One call's end: the result the executor produced, or nothing where the
    /// run was stopped under it.
    Over(Option<ToolResult>),
    Finished(Finished),
    Paths(u64, Vec<String>, bool),
    /// A fence's body drawn, or the parser's reason there is none: a layout is
    /// tens of milliseconds, so it is made on a thread of its own and arrives
    /// here like a line of output — the frame that laid its panel out waited
    /// for nothing.
    Drawn(String, Result<Drawing, Refusal>),
    /// What a skill the frame asked for came to: the load itself arrived as the
    /// `Event::SkillLoaded` before it, and this is the frame's answer that the
    /// task is done.
    Loaded(Result<Invoked, String>),
    /// The launch a `/new` or a save produced, boxed: it is a bag of a dozen
    /// facts and arrives twice in a session, where every key of the frame does
    /// not, so the box costs the two places that make one and nothing else.
    Renewed(Result<Box<Launch>, String>),
    Reloaded(Result<Box<Launch>, String>),
}
