//! One canned reply: a unary body the wire folds whole, or a streamed one the
//! mock writes chunk by chunk. The constructors are the vocabulary a test
//! states its provider answer in.

use std::time::Duration;

use serde_json::Value;

use crate::HARNESS_FIXTURES;

/// One canned reply.
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
    /// A whole body, served in one write.
    fn whole(status: u16, body: String) -> Self {
        Reply {
            status,
            body,
            chunks: Vec::new(),
            pause: Duration::ZERO,
        }
    }

    /// A finished Anthropic message answering `text`.
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

    /// A finished Anthropic message asking for one `bash` call with `command`,
    /// after `text`.
    pub fn tool_call(id: &str, command: &str, text: &str) -> Self {
        Reply::tool_calls(&[(id, command)], text)
    }

    /// A finished Anthropic message asking for several `bash` calls in one
    /// turn, after `text`.
    pub fn tool_calls(calls: &[(&str, &str)], text: &str) -> Self {
        let mut content = Vec::new();
        if !text.is_empty() {
            content.push(serde_json::json!({"type": "text", "text": text}));
        }
        for (id, command) in calls {
            content.push(serde_json::json!({
                "type": "tool_use",
                "id": id,
                "name": "bash",
                "input": {"command": command},
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

    /// A finished Anthropic message that shows `reasoning` before answering
    /// `text`.
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

    /// A tool call whose arguments are not a JSON object.
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

    /// A body recorded from an OpenAI-compatible server, served at `status`:
    /// the fixture names a file under `fixtures/openai`, so the wire format the
    /// test proves is a document about that format, not a literal buried in a
    /// test.
    pub fn recorded(name: &str, status: u16) -> Self {
        Reply::from_file(HARNESS_FIXTURES, name, status)
    }

    /// A body read from a file in `dir`, served at `status`: how the eval suite
    /// replays a recorded conversation instead of asking a live model.
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

    /// A provider failure with `status` and `message`, in the provider's own
    /// error envelope.
    pub fn error(status: u16, kind: &str, message: &str) -> Self {
        Reply::whole(
            status,
            json(serde_json::json!({
                "type": "error",
                "error": {"type": kind, "message": message},
            })),
        )
    }

    /// A streamed reply: `chunks` already framed as `text/event-stream`, written
    /// one at a time.
    pub fn streamed(chunks: Vec<String>) -> Self {
        Reply {
            status: 200,
            body: String::new(),
            chunks,
            pause: Duration::ZERO,
        }
    }

    /// [`Self::streamed`], waiting `pause` after each chunk so the test can act
    /// while the stream is still open.
    pub fn streamed_paced(chunks: Vec<String>, pause: Duration) -> Self {
        Reply {
            pause,
            ..Reply::streamed(chunks)
        }
    }
}

/// A body as the wire sends it: one line, escaped by serde.
pub(crate) fn json(value: Value) -> String {
    serde_json::to_string(&value).expect("serialize canned reply")
}
