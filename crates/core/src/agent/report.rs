//! What one run did: the answer, why the loop stopped, and what it cost.

use std::time::Duration;

use super::budget::Spent;
use crate::provider::Usage;

/// Why the loop ended (the glossary's `stopped`). Every word below appears in
/// the JSON output, and the set is closed: a run ends for one of these reasons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Stopped {
    /// The run answered: the default, so a front end that names no stop
    /// reason still ends the run finally.
    #[default]
    Final,
    MaxTurns,
    /// The run spent its command time or its output: what a batched script
    /// spends without the turn cap counting it.
    Budget,
    /// The run was cancelled with SIGINT; stdout stays empty.
    Cancelled,
    Error,
}

impl Stopped {
    pub const fn as_str(self) -> &'static str {
        match self {
            Stopped::Final => "final",
            Stopped::MaxTurns => "max_turns",
            Stopped::Budget => "budget",
            Stopped::Cancelled => "cancelled",
            Stopped::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub answer: String,
    pub turns: u32,
    pub stopped: Stopped,
    pub model: Option<String>,
    pub tool_calls: u32,
    /// What the commands cost, in the two currencies a script spends.
    pub spent: Spent,
    pub usage: Usage,
    pub duration: Duration,
}
