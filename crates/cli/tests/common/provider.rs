//! A mock provider on loopback: canned replies in order, and every request
//! recorded, so a process test can prove what the binary actually sent. No
//! network, no credential, no live model.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

/// One request the binary made, as text: what a wire-level assertion needs.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    /// A request header, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The request body as JSON.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("request body is JSON")
    }
}

/// One canned reply.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub body: String,
}

impl Reply {
    /// A finished Anthropic message answering `text`.
    pub fn answer(text: &str) -> Self {
        Reply {
            status: 200,
            body: json(serde_json::json!({
                "id": "msg_mock",
                "type": "message",
                "role": "assistant",
                "model": "claude-mock",
                "content": [{"type": "text", "text": text}],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 11, "output_tokens": 7},
            })),
        }
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
        Reply {
            status: 200,
            body: json(serde_json::json!({
                "id": "msg_mock",
                "type": "message",
                "role": "assistant",
                "model": "claude-mock",
                "content": content,
                "stop_reason": "tool_use",
                "usage": {"input_tokens": 20, "output_tokens": 5, "cache_read_input_tokens": 4},
            })),
        }
    }

    /// A finished Anthropic message that shows `reasoning` before answering
    /// `text`.
    pub fn thinking_answer(reasoning: &str, text: &str) -> Self {
        Reply {
            status: 200,
            body: json(serde_json::json!({
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
        }
    }

    /// A tool call whose arguments are not a JSON object.
    pub fn malformed_tool_call(id: &str) -> Self {
        Reply {
            status: 200,
            body: json(serde_json::json!({
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
        }
    }

    /// A provider failure with `status` and `message`, in the provider's own
    /// error envelope.
    pub fn error(status: u16, kind: &str, message: &str) -> Self {
        Reply {
            status,
            body: json(serde_json::json!({
                "type": "error",
                "error": {"type": kind, "message": message},
            })),
        }
    }
}

/// A body as the wire sends it: one line, escaped by serde.
fn json(value: Value) -> String {
    serde_json::to_string(&value).expect("serialize canned reply")
}

/// A running mock provider.
pub struct MockProvider {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stopped: Arc<Mutex<bool>>,
}

impl MockProvider {
    /// Starts a server that answers the `replies` in order; the last one
    /// repeats, so a test that retries needs only the endpoints it cares about.
    pub fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
        let port = listener.local_addr().expect("local addr").port();
        listener
            .set_nonblocking(true)
            .expect("non-blocking listener");
        let requests: Arc<Mutex<Vec<Request>>> = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(Mutex::new(false));

        let recorded = Arc::clone(&requests);
        let stop = Arc::clone(&stopped);
        std::thread::spawn(move || {
            let mut served = 0;
            loop {
                if *stop.lock().expect("stop flag") {
                    return;
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        let reply = replies
                            .get(served)
                            .or_else(|| replies.last())
                            .expect("at least one canned reply")
                            .clone();
                        served += 1;
                        if let Some(request) = serve(stream, &reply) {
                            recorded.lock().expect("record request").push(request);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => return,
                }
            }
        });

        MockProvider {
            url: format!("http://127.0.0.1:{port}"),
            requests,
            stopped,
        }
    }

    /// The base URL the binary should be pointed at.
    pub fn base_url(&self) -> String {
        self.url.clone()
    }

    /// Every request served so far, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().expect("requests").clone()
    }
}

impl Drop for MockProvider {
    fn drop(&mut self) {
        *self.stopped.lock().expect("stop flag") = true;
    }
}

/// Reads one request off `stream` and writes `reply`, then closes.
fn serve(mut stream: TcpStream, reply: &Reply) -> Option<Request> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(position) = find(&buffer, b"\r\n\r\n") {
            break position;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
    };

    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut request_line = request_line.split(' ');
    let method = request_line.next().unwrap_or_default().to_string();
    let path = request_line.next().unwrap_or_default().to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);

    let mut body = buffer[header_end + 4..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => body.extend_from_slice(&chunk[..read]),
        }
    }

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        reason(reply.status),
        reply.body.len(),
        reply.body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    Some(Request {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        529 => "Overloaded",
        _ => "Status",
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
