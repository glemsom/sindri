//! The Anthropic event-stream frames a streamed reply is made of, built as
//! documents about that wire rather than literals buried in a test. A frame is
//! one SSE dispatch: a `data:` line carrying the event, then a blank line.

use serde_json::{Value, json};

pub fn frame(value: Value) -> String {
    format!(
        "data: {}\n\n",
        serde_json::to_string(&value).expect("serialize frame")
    )
}

pub fn text(fragments: &[&str]) -> Vec<String> {
    let mut frames = start();
    block(
        &mut frames,
        0,
        json!({"type": "text", "text": ""}),
        &text_deltas(fragments),
    );
    close(&mut frames, "end_turn");
    frames
}

pub fn truncated_text(fragments: &[&str]) -> Vec<String> {
    let mut frames = start();
    block(
        &mut frames,
        0,
        json!({"type": "text", "text": ""}),
        &text_deltas(fragments),
    );
    frames
}

pub fn text_then_error(fragments: &[&str], kind: &str, message: &str) -> Vec<String> {
    let mut frames = truncated_text(fragments);
    frames.push(frame(json!({
        "type": "error",
        "error": {"type": kind, "message": message},
    })));
    frames
}

pub fn reasoning_then_text(reasoning: &[&str], text: &[&str]) -> Vec<String> {
    let mut frames = start();
    block(
        &mut frames,
        0,
        json!({"type": "thinking", "thinking": ""}),
        &reasoning
            .iter()
            .map(|fragment| json!({"type": "thinking_delta", "thinking": fragment}))
            .collect::<Vec<_>>(),
    );
    block(
        &mut frames,
        1,
        json!({"type": "text", "text": ""}),
        &text_deltas(text),
    );
    close(&mut frames, "end_turn");
    frames
}

pub fn tool_call(id: &str, arguments: &[&str]) -> Vec<String> {
    let mut frames = start();
    block(
        &mut frames,
        0,
        json!({"type": "tool_use", "id": id, "name": "bash", "input": {}}),
        &arguments
            .iter()
            .map(|fragment| json!({"type": "input_json_delta", "partial_json": fragment}))
            .collect::<Vec<_>>(),
    );
    close(&mut frames, "tool_use");
    frames
}

fn start() -> Vec<String> {
    vec![frame(json!({
        "type": "message_start",
        "message": {
            "id": "msg_mock",
            "type": "message",
            "role": "assistant",
            "model": "claude-mock",
            "content": [],
            "stop_reason": null,
            "stop_sequence": null,
            "usage": {"input_tokens": 11, "output_tokens": 1},
        },
    }))]
}

fn block(frames: &mut Vec<String>, index: usize, start: Value, deltas: &[Value]) {
    frames.push(frame(json!({
        "type": "content_block_start",
        "index": index,
        "content_block": start,
    })));
    for delta in deltas {
        frames.push(frame(json!({
            "type": "content_block_delta",
            "index": index,
            "delta": delta,
        })));
    }
    frames.push(frame(json!({
        "type": "content_block_stop",
        "index": index,
    })));
}

fn close(frames: &mut Vec<String>, stop_reason: &str) {
    frames.push(frame(json!({
        "type": "message_delta",
        "delta": {"stop_reason": stop_reason, "stop_sequence": null},
        "usage": {"output_tokens": 7},
    })));
    frames.push(frame(json!({"type": "message_stop"})));
}

fn text_deltas(fragments: &[&str]) -> Vec<Value> {
    fragments
        .iter()
        .map(|fragment| json!({"type": "text_delta", "text": fragment}))
        .collect()
}
