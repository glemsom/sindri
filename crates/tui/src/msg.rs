//! What reaches the frame from outside it: a key, a fragment of the run, and the
//! run's own end. One channel, so nothing overtakes anything else
//! by more than the thread that carried it.

use crossterm::event::KeyEvent;
use sindri_core::agent::Event;

use crate::driver::{CallOver, Finished, Side};

/// One thing that happened, on its way to the frame.
#[derive(Debug)]
pub enum Msg {
    /// A key the terminal took out of raw mode.
    Key(KeyEvent),
    /// A fragment of the run's event stream, forwarded from the core.
    Event(Event),
    /// One line of a running command's output.
    Output(Side, String),
    /// How one command ended.
    Over(CallOver),
    /// The run is over.
    Finished(Finished),
}
