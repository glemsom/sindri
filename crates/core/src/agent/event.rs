//! What the loop tells its front end while it runs. Events, not printing: the
//! core holds no terminal concerns, so a TUI renders this stream instead of
//! reimplementing the loop.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    TextDelta {
        turn: u32,
        text: String,
    },
    ReasoningDelta {
        turn: u32,
        text: String,
    },
    Reasoning {
        turn: u32,
        text: String,
    },
    ToolCall {
        turn: u32,
        /// The script that will run, or `None` when the call's arguments were
        /// unusable and nothing ran.
        command: Option<String>,
    },
}
