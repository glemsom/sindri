//! What a run may spend without the turn cap noticing: the wall clock its
//! commands take, and the bytes of their output it feeds back to the model.
//! Batching moves work inside one script, where `max_turns` cannot see it, so
//! the bound has to be stated in the currency a script spends — otherwise a
//! single call that loops for an hour is supervision-free time the cap was
//! never able to count.

use std::time::Duration;

/// An hour of command time in one prompt. Generous past any interactive task,
/// and far short of the eight hours `max_turns` times the command timeout would
/// otherwise allow.
pub const DEFAULT_WALL: Duration = Duration::from_secs(3600);

/// 8 MiB of output fed back: five hundred results at the head-and-tail cap, and
/// every one of them re-sent with every later turn.
pub const DEFAULT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub wall: Duration,
    pub bytes: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            wall: DEFAULT_WALL,
            bytes: DEFAULT_BYTES,
        }
    }
}

impl Budget {
    /// Whether a run that has spent this much has spent its way to a stop.
    pub fn reached(self, spent: Spent) -> bool {
        spent.wall >= self.wall || spent.bytes >= self.bytes
    }
}

/// What one run has spent, and the two figures its report and JSON carry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Spent {
    pub wall: Duration,
    pub bytes: u64,
}

impl Spent {
    /// Counts one result as the model is charged for it: the command's own
    /// duration, and the text fed back, which is what the run pays for again on
    /// every later turn.
    pub fn add(&mut self, command_ms: u64, fed_back: usize) {
        self.wall += Duration::from_millis(command_ms);
        self.bytes += fed_back as u64;
    }
}
