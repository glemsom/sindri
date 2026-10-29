//! What reaches the frame from outside it: a key, a fragment of the run, and the
//! run's own end. One channel, so nothing overtakes anything else
//! by more than the thread that carried it.

use crossterm::event::{KeyEvent, MouseEvent};
use sindri_core::agent::Event;

use crate::driver::{CallOver, Finished, Side, Spilled};
use crate::rail::Launch;

/// One thing that happened, on its way to the frame.
#[derive(Debug)]
pub enum Msg {
    /// A key the terminal took out of raw mode.
    Key(KeyEvent),
    /// A mouse event, when capture is on and the gesture is not Shift'd.
    Mouse(MouseEvent),
    /// A fragment of the run's event stream, forwarded from the core.
    Event(Event),
    /// One line of a running command's output.
    Output(Side, String),
    /// How one command ended, and where its untruncated output went.
    Over(CallOver, Spilled),
    /// The run is over.
    Finished(Finished),
    /// What one background walk of the workspace found — and whether it hit the
    /// cap before the tree ended — for the completion it was started for.
    Paths(u64, Vec<String>, bool),
    /// A fresh session, when a front end could start one.
    Renewed(Option<Launch>),
}
