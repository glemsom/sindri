//! What one run did: the answer, why the loop stopped, and what it cost.

use std::time::Duration;

use crate::provider::Usage;

/// Why the loop ended (the glossary's `stopped`). Every word below appears in
/// the JSON output, and the set is closed: a run ends for one of these reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    Final,
    MaxTurns,
    /// The run was cancelled with SIGINT; stdout stays empty.
    Cancelled,
    Error,
}

impl Stopped {
    pub const fn as_str(self) -> &'static str {
        match self {
            Stopped::Final => "final",
            Stopped::MaxTurns => "max_turns",
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
    pub usage: Usage,
    pub duration: Duration,
}
