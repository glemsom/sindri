//! The names a wire offers, read at the seam: one call asks the wire what it
//! serves and answers with the names it named, in the order it named them — or
//! with a failure safe to print, holding no credential and never an empty list
//! dressed as an answer. Every test here speaks to a wire on a port of its own,
//! so what is proven is what a server said rather than what our code believes.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken seam reports itself"
)]

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sindri_core::config::{Config, Provider};
use sindri_core::provider::{Client, Model};
use sindri_core::sandbox::{Enforcement, Extras};

/// A wire on an ephemeral port: it answers each request with the status and
/// body it was given, and keeps every request it read so a test can ask what
/// was sent — the credential's header included. The thread is left to the test
/// process, because a wire nobody asked blocks on its port until then.
struct Wire {
    base_url: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Wire {
    /// A wire that answers `requests` requests with `status` and `body`.
    fn answering(status: &'static str, body: impl Into<String>, requests: usize) -> Wire {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port of our own");
        let base_url = format!("http://{}", listener.local_addr().expect("an address"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&seen);
        let body = body.into();
        std::thread::spawn(move || {
            for _ in 0..requests {
                let (stream, _) = listener.accept().expect("a request");
                let request = request_head(&stream);
                kept.lock().expect("seen").push(request);
                write_reply(&stream, status, &body);
            }
        });
        Wire { base_url, seen }
    }

    /// A wire whose port was closed before the call: asked, and never reached.
    fn nobody_answers() -> Wire {
        let port = TcpListener::bind("127.0.0.1:0").expect("a port of our own");
        let address = port.local_addr().expect("an address");
        drop(port);
        Wire {
            base_url: format!("http://{address}"),
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The requests the wire read, in the order it read them.
    fn requests(&self) -> Vec<String> {
        self.seen.lock().expect("seen").clone()
    }

    fn config(&self, provider: Provider, api_key: &str) -> Config {
        Config {
            provider,
            model: "model-one".to_string(),
            api_key: api_key.to_string(),
            base_url: Some(self.base_url.clone()),
            max_turns: 8,
            timeout: Duration::from_secs(10),
            thinking: false,
            status: false,
            ssh_agent: false,
            extras: Extras::Named(Vec::new()),
            enforcement: Enforcement::Bubblewrap,
            extra_headers: Vec::new(),
            refused_repo_config: None,
        }
    }
}

/// One request's head, up to the blank line that ends it.
fn request_head(stream: &TcpStream) -> String {
    let mut head = String::new();
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).expect("a request line") == 0 {
            return head;
        }
        if line == "\r\n" || line == "\n" {
            return head;
        }
        head.push_str(&line);
    }
}

fn write_reply(mut stream: &TcpStream, status: &str, body: &str) {
    let reply = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(reply.as_bytes())
        .expect("a reply to the request");
    stream.flush().expect("a flushed reply");
}

/// The names a client reads from a wire: the seam's one call, asked with the
/// config a run would have been given.
fn names(wire: &Wire, provider: Provider, api_key: &str) -> Vec<String> {
    client(wire, provider, api_key)
        .models()
        .expect("the wire named its models")
}

fn client(wire: &Wire, provider: Provider, api_key: &str) -> Client {
    Client::new(
        &wire.config(provider, api_key),
        sindri_core::preamble::of(Path::new("/workspace"), None),
    )
}

#[test]
fn the_openai_shaped_wire_names_its_models_in_the_order_it_listed_them() {
    let wire = Wire::answering(
        "200 OK",
        r#"{"object":"list","data":[{"id":"zulu"},{"id":"alpha"}]}"#,
        1,
    );

    assert_eq!(names(&wire, Provider::Openai, ""), ["zulu", "alpha"]);
}

#[test]
fn a_wire_that_names_no_models_is_a_failure_and_not_an_empty_answer() {
    let wire = Wire::answering("200 OK", r#"{"object":"list","data":[]}"#, 1);

    let error = client(&wire, Provider::Openai, "")
        .models()
        .expect_err("a wire with nothing to name is no answer");

    assert_eq!(
        error.message(),
        "the openai wire named no models",
        "the failure says what happened rather than standing in for a list"
    );
}

#[test]
fn a_refusal_is_a_failure_that_carries_no_credential() {
    const KEY: &str = "sk-test-4242424242";
    // A hostile wire, echoing the credential it was sent back in its refusal:
    // the failure a human reads must not carry it.
    let wire = Wire::answering(
        "401 Unauthorized",
        format!(r#"{{"error":{{"message":"bad credential {KEY}"}}}}"#),
        1,
    );

    let error = client(&wire, Provider::Openai, KEY)
        .models()
        .expect_err("a refusal is no answer");

    assert!(
        !error.message().contains(KEY),
        "the credential is not in the failure: {}",
        error.message()
    );
    assert!(
        error.message().contains("[redacted]"),
        "the failure says where the credential was: {}",
        error.message()
    );
}

#[test]
fn a_wire_nobody_answers_is_a_failure_and_not_a_panic() {
    let wire = Wire::nobody_answers();

    let error = client(&wire, Provider::Openai, "")
        .models()
        .expect_err("a wire that was never reached is no answer");

    let address = wire.base_url.trim_start_matches("http://").to_string();
    assert!(
        error.message().contains(&address),
        "the failure names the wire that was not reached: {}",
        error.message()
    );
}

#[test]
fn the_openai_shaped_wire_with_no_credential_is_asked_without_one() {
    let wire = Wire::answering("200 OK", r#"{"object":"list","data":[{"id":"alpha"}]}"#, 1);

    names(&wire, Provider::Openai, "");

    let request = wire.requests().remove(0);
    assert!(
        !request.to_ascii_lowercase().contains("authorization"),
        "a local server with no key is not sent a blank one: {request}"
    );
}

#[test]
fn the_openai_shaped_wire_with_a_credential_is_asked_with_it() {
    const KEY: &str = "sk-test-8675309";
    let wire = Wire::answering("200 OK", r#"{"object":"list","data":[{"id":"alpha"}]}"#, 1);

    names(&wire, Provider::Openai, KEY);

    let request = wire.requests().remove(0);
    assert!(
        request.contains(&format!("authorization: Bearer {KEY}"))
            || request.contains(&format!("Authorization: Bearer {KEY}")),
        "the list is asked with the credential a turn is asked with: {request}"
    );
}

#[test]
fn the_anthropic_wire_names_its_models_in_the_order_it_listed_them() {
    let wire = Wire::answering(
        "200 OK",
        r#"{"data":[{"id":"claude-zulu","display_name":"Claude Zulu","type":"model"},{"id":"claude-alpha","display_name":"Claude Alpha","type":"model"}],"has_more":false}"#,
        1,
    );

    assert_eq!(
        names(&wire, Provider::Anthropic, "sk-ant-test"),
        ["claude-zulu", "claude-alpha"]
    );
}
