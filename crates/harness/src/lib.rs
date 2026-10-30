//! A mock provider on loopback: canned replies in order, and every request
//! recorded, so a process test can prove what the binary actually sent. No
//! network, no credential, no live model.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

pub mod frames;
mod reply;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

pub use reply::Reply;

/// Every variable that can point a run at a provider, or set its limits, so a
/// test or an eval states its own rather than inheriting the host's.
pub const PROVIDER_ENV: &[&str] = &[
    "SINDRI_PROVIDER",
    "SINDRI_MODEL",
    "SINDRI_API_KEY",
    "SINDRI_BASE_URL",
    "SINDRI_MAX_TURNS",
    "SINDRI_TIMEOUT",
    "SINDRI_THINKING",
    "SINDRI_EXTRA_WRITABLE_PATHS",
    "SINDRI_EXTRA_HEADERS",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_BASE_URL",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
];

pub const HARNESS_FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/openai");

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    headers: Vec<(String, String)>,
    pub body: String,
}

impl Request {
    pub fn literal(method: &str, path: &str, body: &str) -> Self {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            headers: Vec::new(),
            body: body.to_string(),
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("request body is JSON")
    }
}

pub struct MockProvider {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stopped: Arc<Mutex<bool>>,
}

impl MockProvider {
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
                        serve(stream, &reply, &recorded);
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

    pub fn base_url(&self) -> String {
        self.url.clone()
    }

    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().expect("requests").clone()
    }
}

impl Drop for MockProvider {
    fn drop(&mut self) {
        *self.stopped.lock().expect("stop flag") = true;
    }
}

/// Reads one request off `stream`, records it, and writes `reply`. The request
/// is recorded before the reply finishes, so a test can act while a paced
/// stream is still open.
fn serve(mut stream: TcpStream, reply: &Reply, recorded: &Arc<Mutex<Vec<Request>>>) {
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
            Ok(0) | Err(_) => return,
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

    let body = String::from_utf8_lossy(&body).to_string();
    let request = Request {
        method,
        path,
        headers,
        body,
    };
    let streaming = streams(reply, &request.body);
    recorded.lock().expect("record request").push(request);
    if streaming {
        write_stream(&mut stream, reply);
    } else {
        write_whole(&mut stream, reply);
    }
}

fn streams(reply: &Reply, body: &str) -> bool {
    if !reply.chunks.is_empty() {
        return true;
    }
    reply.status == 200
        && serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|body| body.get("stream").and_then(Value::as_bool))
            .unwrap_or(false)
}

fn write_whole(stream: &mut TcpStream, reply: &Reply) {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        reason(reply.status),
        reply.body.len(),
        reply.body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn write_stream(stream: &mut TcpStream, reply: &Reply) {
    let explicit = !reply.chunks.is_empty();
    let chunks = if explicit {
        reply.chunks.clone()
    } else {
        vec![sse(reply.body.as_str())]
    };
    let head = if explicit {
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_string()
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            chunks.iter().map(String::len).sum::<usize>()
        )
    };
    let _ = stream.write_all(head.as_bytes());
    for chunk in chunks {
        let written = if explicit {
            stream.write_all(format!("{:x}\r\n{chunk}\r\n", chunk.len()).as_bytes())
        } else {
            stream.write_all(chunk.as_bytes())
        };
        if written.is_err() || stream.flush().is_err() {
            return;
        }
        if !reply.pause.is_zero() {
            std::thread::sleep(reply.pause);
        }
    }
    if explicit {
        let _ = stream.write_all(b"0\r\n\r\n");
        let _ = stream.flush();
    }
}

/// A whole body as one SSE frame: every line of it is a `data:` line, so even a
/// pretty-printed document reassembles to exactly itself.
fn sse(body: &str) -> String {
    let mut frame = String::new();
    for line in body.lines() {
        frame.push_str("data: ");
        frame.push_str(line);
        frame.push('\n');
    }
    frame.push('\n');
    frame
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
