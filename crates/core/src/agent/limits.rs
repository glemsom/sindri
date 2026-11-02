//! The two figures that end a run — the command timeout the executor enforces
//! and the budget the loop stops on — formatted in one place, so the
//! description the model reads is derived from the values in force and cannot
//! drift from the boundary it describes. Both are facts about this program, not
//! about bash: no amount of model knowledge substitutes for them, and a model
//! that has never been told cannot tell a slow machine from its own limit.

use std::time::Duration;

use super::budget::Budget;
use crate::exec::DEFAULT_TIMEOUT_SECS;

/// What will stop the work, as the executor and the loop hold it. Passing it to
/// the tool is what makes the description true rather than aspirational.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub command_timeout: Duration,
    pub budget: Budget,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            command_timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            budget: Budget::default(),
        }
    }
}

impl Limits {
    /// The limits a run that changes only the command timeout has: the budget
    /// is not a setting yet, so it stays the default the loop stops on.
    pub fn for_timeout(command_timeout: Duration) -> Self {
        Limits {
            command_timeout,
            ..Limits::default()
        }
    }

    /// The sentences the description carries. One derivation of both figures,
    /// so a config that changes either changes what the model reads.
    pub fn describe(self) -> String {
        format!(
            "A command is killed after {}; the run gets {} of command time and {} of output in \
             all, and `timed_out` means one of those limits: report, do not retry.",
            span(self.command_timeout),
            span(self.budget.wall),
            bytes(self.budget.bytes),
        )
    }
}

/// A duration as the sentence reads it: whole hours in their own unit, anything
/// else in seconds, which is the unit the timeout itself is set in.
fn span(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 3600 && secs.is_multiple_of(3600) {
        match secs / 3600 {
            1 => "one hour".to_string(),
            hours => format!("{hours} hours"),
        }
    } else {
        format!("{secs} s")
    }
}

/// A byte count in the largest unit that divides it exactly, because 8388608
/// tells a model nothing that 8 MiB does not.
fn bytes(count: u64) -> String {
    match count {
        count if count >= 1 << 20 && count.is_multiple_of(1 << 20) => {
            format!("{} MiB", count >> 20)
        }
        count if count >= 1 << 10 && count.is_multiple_of(1 << 10) => {
            format!("{} KiB", count >> 10)
        }
        count => format!("{count} bytes"),
    }
}
