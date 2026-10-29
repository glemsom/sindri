//! What the loop tells its front end while it runs. Events, not printing: the
//! core holds no terminal concerns, so a TUI renders this stream instead of
//! reimplementing the loop.

/// One thing that happened during a run, as it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A fragment of the model's answer, as it arrives.
    TextDelta {
        /// The turn it belongs to, counted from 1.
        turn: u32,
        /// The fragment, to be appended to the deltas before it.
        text: String,
    },
    /// A fragment of the model's reasoning, as it arrives.
    ReasoningDelta {
        /// The turn it belongs to, counted from 1.
        turn: u32,
        /// The fragment, to be appended to the deltas before it.
        text: String,
    },
    /// The model's reasoning on one turn, where the provider shows it.
    Reasoning {
        /// The turn it belongs to, counted from 1.
        turn: u32,
        /// The reasoning text as the provider sent it.
        text: String,
    },
    /// One tool call the model asked for, named but not yet run.
    ToolCall {
        /// The turn it belongs to, counted from 1.
        turn: u32,
        /// The script that will run, or `None` when the call's arguments were
        /// unusable and nothing ran.
        command: Option<String>,
    },
}
