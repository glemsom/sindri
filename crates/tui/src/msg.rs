//! What reaches the frame from outside it: a key, a fragment of the run, and
//! the run's own end. One channel, so nothing overtakes anything else by more
//! than the thread that carried it.

use crossterm::event::{KeyEvent, MouseEvent};
use sindri_core::agent::Event;

use crate::driver::{CallOver, Finished, Side, Spilled};
use crate::picture::{Drawing, Refusal};
use crate::rail::Launch;

#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    Mouse(MouseEvent),
    /// The terminal changed size. Nothing in the frame changes: the next
    /// frame asks the terminal the new one, and this message is what asks
    /// for that frame.
    Resize,
    Event(Event),
    Output(Side, String),
    Over(CallOver, Spilled),
    Finished(Finished),
    Paths(u64, Vec<String>, bool),
    /// A fence's body drawn, or the parser's reason there is none: a layout is
    /// tens of milliseconds, so it is made on a thread of its own and arrives
    /// here like a line of output — the frame that laid its panel out waited
    /// for nothing.
    Drawn(String, Result<Drawing, Refusal>),
    Renewed(Result<Launch, String>),
    Reloaded(Result<Launch, String>),
}
