//! What the loop tells its front end while it runs. Events, not printing: the
//! core holds no terminal concerns, so a TUI renders this stream instead of
//! reimplementing the loop.

use crate::provider::Asked;

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
        /// What the call asks for, as the loop read it: the script, the skill,
        /// or the reason nothing ran. The variant is the tool, so a frame that
        /// draws one call draws every kind of call.
        asked: Asked,
    },
}
