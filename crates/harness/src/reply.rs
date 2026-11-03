//! One canned reply: a unary body the wire folds whole, or a streamed one the
//! mock writes chunk by chunk. The constructors are the vocabulary a test
//! states its provider answer in.

use std::time::Duration;

use serde_json::Value;

use crate::HARNESS_FIXTURES;

/// One tool ask: the id the result is answered to, the tool's name, and the
/// input it takes.
type Ask<'a> = (&'a str, &'a str, Value);

#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub body: String,
    /// Raw chunks written one at a time as a `text/event-stream` reply. Empty
    /// means `body` is served whole.
    pub chunks: Vec<String>,
    /// How long the server waits after writing each chunk, so a cancel can
    /// land mid-stream.
    pub pause: Duration,
}

impl Reply {
    fn whole(status: u16, body: String) -> Self {
        Reply {
            status,
            body,
            chunks: Vec::new(),
            pause: Duration::ZERO,
        }
    }

    pub fn answer(text: &str) -> Self {
        Reply::whole(
            200,
            json(serde_json::json!({
                "id": "msg_mock",
                "type": "message",
                "role": "assistant",
                "model": "claude-mock",
                "content": [{"type": "text", "text": text}],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 11, "output_tokens": 7},
            })),
        )
    }

    pub fn tool_call(id: &str, command: &str, text: &str) -> Self {
        Reply::tool_calls(&[(id, command)], text)
    }

    /// Several commands in one turn: the wave a run answers together.
    pub fn tool_calls(calls: &[(&str, &str)], text: &str) -> Self {
        let asks: Vec<Ask> = calls
            .iter()
            .map(|(id, command)| (*id, "bash", serde_json::json!({"command": command})))
            .collect();
        Reply::asks(&asks, text)
    }

    /// One turn asking for one tool by name, with the input that tool takes:
    /// `skill` and `open` are this, and so is a single `bash`.
    pub fn one_tool(id: &str, name: &str, input: Value, text: &str) -> Self {
        Reply::asks(&[(id, name, input)], text)
    }

    /// The assistant message every tool ask is: the text, where there is any,
    /// then one `tool_use` per call, and the usage a tool turn reports. One
    /// body for all of them, because a tool turn's usage is a fact about a tool
    /// turn rather than about which tool was asked — the drift a second copy
    /// buys is one constructor reporting a cached prefix and another not.
    fn asks(calls: &[Ask], text: &str) -> Self {
        let mut content = Vec::new();
        if !text.is_empty() {
            content.push(serde_json::json!({"type": "text", "text": text}));
        }
        for (id, name, input) in calls {
            content.push(serde_json::json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input,
            }));
        }
        Reply::whole(
            200,
            json(serde_json::json!({
                "id": "msg_mock",
                "type": "message",
                "role": "assistant",
                "model": "claude-mock",
                "content": content,
                "stop_reason": "tool_use",
                "usage": {"input_tokens": 20, "output_tokens": 5, "cache_read_input_tokens": 4},
            })),
        )
    }

    pub fn thinking_answer(reasoning: &str, text: &str) -> Self {
        Reply::whole(
            200,
            json(serde_json::json!({
                "id": "msg_mock",
                "type": "message",
                "role": "assistant",
                "model": "claude-mock",
                "content": [
                    {"type": "thinking", "thinking": reasoning, "signature": "sig"},
                    {"type": "text", "text": text},
                ],
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 11,
                    "output_tokens": 7,
                    "cache_read_input_tokens": 3,
                    "output_tokens_details": {"reasoning_tokens": 2},
                },
            })),
        )
    }

    pub fn malformed_tool_call(id: &str) -> Self {
        Reply::whole(
            200,
            json(serde_json::json!({
                "id": "msg_mock",
                "type": "message",
                "role": "assistant",
                "model": "claude-mock",
                "content": [{
                    "type": "tool_use",
                    "id": id,
                    "name": "bash",
                    "input": "{oops",
                }],
                "stop_reason": "tool_use",
                "usage": {"input_tokens": 20, "output_tokens": 5},
            })),
        )
    }

    pub fn recorded(name: &str, status: u16) -> Self {
        Reply::from_file(HARNESS_FIXTURES, name, status)
    }

    pub fn from_file(dir: &str, name: &str, status: u16) -> Self {
        let path = format!("{dir}/{name}.json");
        Reply::whole(
            status,
            std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read fixture {path}: {error}"))
                .trim()
                .to_string(),
        )
    }

    pub fn error(status: u16, kind: &str, message: &str) -> Self {
        Reply::whole(
            status,
            json(serde_json::json!({
                "type": "error",
                "error": {"type": kind, "message": message},
            })),
        )
    }

    pub fn streamed(chunks: Vec<String>) -> Self {
        Reply {
            status: 200,
            body: String::new(),
            chunks,
            pause: Duration::ZERO,
        }
    }

    pub fn streamed_paced(chunks: Vec<String>, pause: Duration) -> Self {
        Reply {
            pause,
            ..Reply::streamed(chunks)
        }
    }
}

pub(crate) fn json(value: Value) -> String {
    serde_json::to_string(&value).expect("serialize canned reply")
}
