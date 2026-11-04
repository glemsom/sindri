//! The names a wire offers, read at the seam: one call asks the wire what it
//! serves and answers with the names it named, in the order it named them — or
//! with a failure safe to print, holding no credential and never an empty list
//! dressed as an answer. Every test here speaks to a wire on a port of its own,
//! so what is proven is what a server said rather than what our code believes.
//! Copilot is asked where its stored session points and nowhere a human's
//! configured gateway does, so the last tests here stand on a state root of
//! their own rather than on a base URL the test chose.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken seam reports itself"
)]

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use sindri_core::config::{Config, Provider};
use sindri_core::provider::{Client, Delta, DeltaSink, Model};
use sindri_core::sandbox::{Enforcement, Extras};
use tempfile::TempDir;

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

    /// The config a run would have been given, pointed at this wire.
    fn config(&self, provider: Provider, api_key: &str) -> Config {
        config(provider, api_key, Some(self.base_url.clone()))
    }
}

/// The config a run would have been given: `base_url` as the human configured
/// it, which for Copilot is not necessarily where the run is asked.
fn config(provider: Provider, api_key: &str, base_url: Option<String>) -> Config {
    Config {
        provider,
        model: "model-one".to_string(),
        api_key: api_key.to_string(),
        base_url,
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
    composed(&wire.config(provider, api_key), SESSION)
}

/// The id of the session the clients here are composed for. A listing is asked
/// with it in hand and does not send it, which one test here proves.
const SESSION: &str = "8f14e45f-a-session";

/// A client composed from a config and a session's id, the way a run composes
/// one: the preamble of a workspace with nothing in it, and the id a header
/// value's `{session}` would become.
fn composed(config: &Config, session: &str) -> Client {
    Client::new(
        config,
        sindri_core::preamble::of(Path::new("/workspace"), None),
        session,
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

/// The state root a Copilot test signs in against, and the lock that keeps two
/// of them from writing the process's `XDG_STATE_HOME` at once: the Copilot
/// wire reads its session through the process, so one test at a time owns the
/// variable. The root is gone when the store is dropped, and the next test
/// points the variable at its own.
struct Store {
    _lock: MutexGuard<'static, ()>,
    _state: TempDir,
}

/// The credential the stored record carries: a signed-in Copilot's key, and the
/// string no configured gateway may ever be sent.
const COPILOT_KEY: &str = "tid=session-token-7734";

static ENV: Mutex<()> = Mutex::new(());

impl Store {
    /// A state root whose only Copilot record is a live session scoped to
    /// `endpoint`, the base a signed-in Copilot names.
    fn signed_in(endpoint: &str) -> Store {
        Store::holding(Some(&format!(
            r#"{{"token":"{COPILOT_KEY}","expires_at":{},"endpoints":{{"api":"{endpoint}"}}}}"#,
            expires_at()
        )))
    }

    /// A state root with no Copilot record at all: nobody has signed in.
    fn signed_out() -> Store {
        Store::holding(None)
    }

    /// A state root of this test's own, `record` written where Rig keeps a
    /// Copilot session when there is one. The lock is held for the store's
    /// whole life, because the path is read from the process's environment.
    fn holding(record: Option<&str>) -> Store {
        let store = Store {
            _lock: ENV
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            _state: TempDir::new().expect("state directory"),
        };
        // SAFETY: this test is run alone against the process environment: the
        // lock above is held while the variable names this test's own state
        // root, and no other test in this binary reads or writes it.
        unsafe {
            std::env::set_var("XDG_STATE_HOME", store._state.path());
        }
        if let Some(record) = record {
            let dir = store._state.path().join("sindri/copilot");
            std::fs::create_dir_all(&dir).expect("the Copilot store's directory");
            std::fs::write(dir.join("copilot.json"), record).expect("the stored session");
        }
        store
    }
}

/// A session's expiry, an hour from now: live, so reading it never asks GitHub
/// to exchange a token, and no test reaches a network it did not open.
fn expires_at() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after the epoch")
        .as_secs() as i64
        + 3600
}

/// A sink no fragment reaches: a Copilot test fails before the stream opens.
struct Silent;

impl DeltaSink for Silent {
    fn delta(&mut self, _delta: Delta) -> bool {
        true
    }
}

/// The one Copilot client a test asks, over the config a run would be built
/// with and whatever base URL the human configured. The `Store` the caller
/// holds is what the call is signed in with, so it must outlive it.
fn copilot(base_url: Option<String>) -> Client {
    composed(&config(Provider::Copilot, "", base_url), SESSION)
}

#[test]
fn a_stored_session_names_the_models_the_endpoint_it_names_serves() {
    let wire = Wire::answering(
        "200 OK",
        r#"{"data":[{"id":"gpt-4o"},{"id":"claude-sonnet-4.6"}]}"#,
        1,
    );
    let _store = Store::signed_in(&wire.base_url);

    let names = copilot(None).models().expect("the session's wire listed");

    assert_eq!(names, ["gpt-4o", "claude-sonnet-4.6"]);
}

#[test]
fn a_configured_gateway_is_never_asked_nor_handed_the_session_token() {
    // A gateway on a port that is open to nobody: routing the session there
    // would fail the call rather than pass unnoticed.
    let gateway = Wire::answering("200 OK", r#"{"data":[{"id":"gateway-model"}]}"#, 0);
    let session = Wire::answering("200 OK", r#"{"data":[{"id":"gpt-4o"}]}"#, 1);
    let _store = Store::signed_in(&session.base_url);

    let names = copilot(Some(gateway.base_url.clone()))
        .models()
        .expect("the session's own endpoint listed");

    assert_eq!(names, ["gpt-4o"]);
    assert!(
        gateway.requests().is_empty(),
        "the configured gateway was asked: {:?}",
        gateway.requests()
    );
    let asked = session.requests().remove(0);
    assert!(
        asked.contains(&format!("Bearer {COPILOT_KEY}")),
        "the list is asked with the session a turn is asked with: {asked}"
    );
}

#[test]
fn with_no_session_the_list_fails_in_the_sentence_a_turn_fails_in() {
    let _store = Store::signed_out();
    let client = copilot(None);

    let listed = client.models().expect_err("no session, no list");
    let turned = client
        .turn(&[], &[], false, &mut Silent)
        .expect_err("no session, no turn");

    assert!(
        listed.message().contains("sindri auth login copilot"),
        "the failure names the command that signs in: {}",
        listed.message()
    );
    assert_eq!(
        listed.message(),
        turned.message(),
        "one sentence for a missing session, whichever call asked"
    );
}

#[test]
fn a_listing_is_asked_without_a_header_value_that_names_the_session() {
    let wire = Wire::answering("200 OK", r#"{"object":"list","data":[{"id":"alpha"}]}"#, 1);
    let mut config = wire.config(Provider::Openai, "");
    config.extra_headers = vec![
        ("x-opencode-session".to_string(), "{session}".to_string()),
        ("x-gateway-hint".to_string(), "hint".to_string()),
    ];

    let names = composed(&config, SESSION)
        .models()
        .expect("the wire named its models");

    assert_eq!(names, ["alpha"]);
    let request = wire.requests().remove(0).to_ascii_lowercase();
    assert!(
        !request.contains("x-opencode-session"),
        "a listing is not a turn, so a value naming the run's session is left off it \
         rather than sent as a placeholder: {request}"
    );
    assert!(
        request.contains("x-gateway-hint: hint"),
        "and a value with no token goes on every request, the listing's included: {request}"
    );
}
