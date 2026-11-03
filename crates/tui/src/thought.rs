//! A thought the run is having: the turn it belongs to, the text so far and the
//! moment it began. It is held beside the thread and never in it, because a
//! thought is a state the run is in and not something that was said — the
//! transcript is a document of what was said, and a thought is what happened
//! first. `render::pane` is the one surface it is drawn on, and the one row it
//! leaves in the thread belongs to `entry::Entry`'s scar.

use std::time::Duration;

/// One thought, as it arrives. `started` is the run's own clock when the first
/// fragment landed, which is what the pane's head row counts from; `text` is
/// every fragment of it, appended in the order the provider sent them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thought {
    pub turn: u32,
    pub text: String,
    pub started: Duration,
}
