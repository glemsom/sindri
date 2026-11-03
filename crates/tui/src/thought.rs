//! A thought the run is having: the turn it belongs to, the text so far and the
//! moment it began — and, once it is over, the moment the pane began to fold
//! back into the scar the thread keeps of it.
//! It is held beside the thread and never in it, because a thought is a state
//! the run is in and not something that was said — the transcript is a document
//! of what was said, and a thought is what happened first. `render::pane` is the
//! one surface it is drawn on, and the one row it leaves in the thread belongs
//! to `entry::Entry`'s scar.

use std::time::Duration;

/// One thought, as it arrives. `started` is the run's own clock when the first
/// fragment landed, which is what the pane's head row counts from and what its
/// unfurl is measured against; `text` is every fragment of it, appended in the
/// order the provider sent them.
///
/// `closed` is the moment the final reasoning event landed, and the clock the
/// fold runs on: the pane gives a row back a tick until it is gone, and the scar
/// is written on the tick it is one row tall. `scarred` says that row has been
/// written, so a fold that runs past it writes one scar and not several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thought {
    pub turn: u32,
    pub text: String,
    pub started: Duration,
    pub closed: Option<Duration>,
    pub scarred: bool,
}

impl Thought {
    /// A thought arriving: one fragment in, and the clock it began at.
    pub fn new(turn: u32, text: String, started: Duration) -> Self {
        Thought {
            turn,
            text,
            started,
            closed: None,
            scarred: false,
        }
    }

    /// How long the thought has been closed at `now`: the fold's own clock, and
    /// `None` while the thought is still arriving, which is a pane that has no
    /// folding to do.
    pub fn closed_for(&self, now: Duration) -> Option<Duration> {
        self.closed.map(|at| now.saturating_sub(at))
    }
}
