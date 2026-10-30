//! What one line of the transcript says, and what `session.json` holds. Both are
//! written by a run and read back by `sindri sessions show`, so one type serves
//! both directions and the two cannot drift.

use serde::{Deserialize, Serialize};

/// The metadata document: one run, from the moment it started to the reason it
/// stopped (ADR-0006). A record still saying `running` is a run that crashed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub version: String,
    pub provider: String,
    pub model: String,
    /// `bubblewrap` when every command was sandboxed, `unsafe` under
    /// `--yolo-unsafe`: a record must never claim a sandbox a run did not have.
    pub enforcement: String,
    pub pid: u32,
    pub started_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answered_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<u32>,
    /// Why the run ended: `final`, `max_turns`, `cancelled`, `error` — or
    /// `running` while it has not ended at all.
    pub stopped: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::provider::Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Entry {
    User {
        text: String,
    },
    Assistant {
        turn: u32,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
    },
    ToolCall {
        turn: u32,
        call_id: String,
        tool: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        problem: Option<String>,
    },
    ToolResult {
        turn: u32,
        call_id: String,
        content: String,
        is_error: bool,
    },
}
