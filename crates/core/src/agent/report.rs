//! What one run did: the answer, why the loop stopped, and what it cost.

use std::time::Duration;

use crate::provider::Usage;

/// Why the loop ended (the glossary's `stopped`). Every word below appears in
/// the JSON output, and the set is closed: a run ends for one of these reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// The model stopped asking for tools: the answer is final.
    Final,
    /// The turn cap was reached; the answer is whatever the last turn said.
    MaxTurns,
    /// The run was cancelled with SIGINT; stdout stays empty (ADR-0004).
    Cancelled,
    /// The model or its provider failed; stdout stays empty (ADR-0004).
    Error,
}

impl Stopped {
    /// The word the output contract uses for it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Stopped::Final => "final",
            Stopped::MaxTurns => "max_turns",
            Stopped::Cancelled => "cancelled",
            Stopped::Error => "error",
        }
    }
}

/// Everything a finished run reports, before any front end shapes it.
#[derive(Debug, Clone)]
pub struct Report {
    /// The final assistant text of the last turn: the product.
    pub answer: String,
    /// How many round trips the run took.
    pub turns: u32,
    /// Why the loop ended.
    pub stopped: Stopped,
    /// The model the provider says answered, when it said.
    pub model: Option<String>,
    /// How many tool calls ran, or were answered as unusable.
    pub tool_calls: u32,
    /// Every turn's counters, summed.
    pub usage: Usage,
    /// How long the whole run took.
    pub duration: Duration,
}
