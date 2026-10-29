//! What one line of the transcript says, and what `session.json` holds. Both are
//! written by a run and read back by `sindri sessions show`, so one type serves
//! both directions and the two cannot drift.

use serde::{Deserialize, Serialize};

/// The metadata document: one run, from the moment it started to the reason it
/// stopped (ADR-0006). A record still saying `running` is a run that crashed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    /// The GUID this run is recorded under: the `session` field of the output.
    pub id: String,
    /// The binary that wrote it, so a record identifies its own source.
    pub version: String,
    /// The provider that was asked.
    pub provider: String,
    /// The model that was configured, whether or not it answered.
    pub model: String,
    /// `bubblewrap` when every command was sandboxed, `unsafe` under
    /// `--yolo-unsafe`: a record must never claim a sandbox a run did not have.
    pub enforcement: String,
    /// The pid of the front end that wrote it.
    pub pid: u32,
    /// When the run started, in milliseconds since the Unix epoch.
    pub started_ms: u64,
    /// When it ended, or absent when it never did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<u64>,
    /// How long it ran, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// The model the provider said answered, when it named one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered_model: Option<String>,
    /// How many round trips the run took.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns: Option<u32>,
    /// How many tool calls ran, or were answered as unusable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<u32>,
    /// Why the run ended: `final`, `max_turns`, `cancelled`, `error` — or
    /// `running` while it has not ended at all.
    pub stopped: String,
    /// What the run was billed for; absent when the provider reported nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::provider::Usage>,
    /// The failure that ended the run, when one did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One thing that happened, in order, as one line of `transcript.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Entry {
    /// The question the run was asked.
    User {
        /// The composed prompt: the argument, with any piped context after it.
        text: String,
    },
    /// One turn's reply from the model.
    Assistant {
        /// The turn it belongs to, counted from 1.
        turn: u32,
        /// The assistant text, empty when the turn only asked for tools.
        text: String,
        /// The model's reasoning, where the provider showed it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
    },
    /// One tool call the model asked for, named before it ran.
    ToolCall {
        /// The turn it belongs to, counted from 1.
        turn: u32,
        /// The provider's id for the call, echoed back with its result.
        call_id: String,
        /// The tool that was asked for.
        tool: String,
        /// The script, when the call's arguments were usable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        /// Why nothing ran, when the arguments were unusable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        problem: Option<String>,
    },
    /// What the tool answered the model.
    ToolResult {
        /// The turn it belongs to, counted from 1.
        turn: u32,
        /// The call this answers.
        call_id: String,
        /// What the model read: the structured outcome and both streams.
        content: String,
        /// Whether the tool never ran, or failed to.
        is_error: bool,
    },
}
