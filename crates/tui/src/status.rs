//! The frame's status: what it says its terminal while it owns the alternate
//! screen. A record belongs to a terminal and not to a screen, so this is the
//! one channel that reaches a human who is looking at another tab.
//!
//! The bytes are the core's (`sindri_core::status`), and so is the gate on
//! saying them (`status::Told`): this module holds the title, the writer and the
//! place in the frame's loop where the writing happens. That place is after a
//! draw has come back and never inside one — a sequence an emulator has consumed
//! is one ratatui's diff would repaint cells around — which is why the frame
//! calls [`Channel::say`] where it calls it, beside the OSC 52 copy
//! `crates/tui/src/copy.rs` writes to the same stdout.

use std::io::Write;
use std::time::Instant;

use sindri_core::status::{self, State, Told};

/// What the frame has told its terminal, and where the bytes go: the title the
/// reports are labelled by, the gate that keeps them from being said twice, and
/// the stdout the frame drew on.
pub(crate) struct Channel {
    title: String,
    told: Told,
    out: Box<dyn Write + Send>,
}

impl Channel {
    /// The frame's own stdout: the screen the reports are about is on it.
    pub(crate) fn stdout(title: String) -> Channel {
        Channel {
            title,
            told: Told::new(),
            out: Box::new(std::io::stdout()),
        }
    }

    /// Says what the frame is doing at `now`, or nothing where nothing has
    /// changed since the last report: `state` is the frame's own reading of
    /// itself, made once per frame.
    pub(crate) fn say(&mut self, state: &State, now: Instant) {
        if let Some(bytes) = self.told.next(state, &self.title, now) {
            self.write(&bytes);
        }
    }

    /// Whether a report is owed and only the second is withholding it. A frame
    /// with nothing running is otherwise still, and a state that settled inside
    /// the last report's second would never be said: this is what keeps the
    /// clock running for it.
    pub(crate) fn owes(&self, state: &State) -> bool {
        self.told.owed(state)
    }

    /// Takes the frame's record away. `done` and `error` survive a shell prompt
    /// by design; the frame's own record must not outlive the frame in a tab
    /// that has moved on, so this goes out before the alternate screen is left.
    pub(crate) fn clear(&mut self) {
        if let Some(bytes) = status::clear(&self.title) {
            self.write(&bytes);
        }
    }

    /// A terminal that has gone is not a failure to report: the frame is ending
    /// either way, and a report is a courtesy to a screen, never the product.
    fn write(&mut self, bytes: &[u8]) {
        let _ = self.out.write_all(bytes);
        let _ = self.out.flush();
    }
}
