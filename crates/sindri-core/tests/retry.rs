//! What §4's retry policy does, proved without a socket and without sleeping:
//! the waits it takes are recorded by a fake backoff, so the sequence of delays
//! and the number of attempts are assertions rather than elapsed time, and the
//! replies it retries are the bytes a fixture recorded.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sindri_core::provider::retry::{BACKOFF, Backoff, until_final};
use sindri_core::provider::transport::{Canned, Http, Replay};
use sindri_core::provider::{Ask, Client, Deltas, Failure, Verdict};
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

/// An Anthropic Messages reply, as the wire streamed it.
const ANTHROPIC: &str = include_str!("fixtures/providers/anthropic_stream.sse");
/// The same dialect, cut off after its first text fragment: no `message_stop`.
const ANTHROPIC_TRUNCATED: &str = include_str!("fixtures/providers/anthropic_truncated.sse");
/// What a wire says when it wants fewer requests in flight.
const RATE_LIMITED: &str = r#"{"error":{"message":"slow down","type":"rate_limit_error"}}"#;
/// What a wire says when its own side faulted.
const OVERLOADED: &str = r#"{"error":{"message":"overloaded","type":"overloaded_error"}}"#;
/// What a wire says when the request itself was wrong.
const BAD_REQUEST: &str = r#"{"error":{"message":"unknown model","type":"invalid_request_error"}}"#;

/// A backoff that only remembers what it was asked to wait, so a test asserts
/// the sequence of delays instead of spending them.
#[derive(Clone, Default)]
struct Recorded(Arc<Mutex<Vec<Duration>>>);

impl Backoff for Recorded {
    fn wait(&self, delay: Duration) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(delay);
    }
}

impl Recorded {
    /// Every delay this backoff was handed, in order.
    fn delays(&self) -> Vec<Duration> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// A failure as a wire reports one: the verdict it classified, the wire's own
/// retry answer, and whether anything had streamed by then.
fn failure(verdict: Verdict, retryable: bool, streamed: bool) -> Failure {
    Failure {
        verdict,
        retryable,
        streamed,
        message: SecretSet::default().redact(format!("{verdict:?} as the wire told it")),
    }
}

#[test]
fn a_retryable_verdict_is_tried_three_times_and_then_given_up() {
    let pause = Recorded::default();
    let mut attempts = 0;

    let failure = until_final(&pause, || {
        attempts += 1;
        Err::<(), _>(failure(Verdict::Timeout, true, false))
    })
    .expect_err("every attempt timed out");

    assert_eq!(attempts, 3, "three attempts, the first included");
    assert_eq!(
        failure.verdict,
        Verdict::Timeout,
        "the last failure is the one reported"
    );
    assert_eq!(
        pause.delays(),
        [Duration::ZERO, BACKOFF, BACKOFF * 2],
        "nothing before the first attempt, then 200 ms doubling"
    );
}

/// A sink that keeps what arrived, so a test sees the fragments and not only the
/// folded answer.
#[derive(Debug, Default)]
struct Transcript {
    /// Every text fragment, joined.
    text: String,
}

impl Deltas for Transcript {
    fn text(&mut self, fragment: &str) {
        self.text.push_str(fragment);
    }

    fn reasoning(&mut self, _fragment: &str) {}
}

/// An Anthropic ask with a credential, as §10 sends one.
fn anthropic_ask() -> Ask {
    Ask::new(Wire::Anthropic, "claude-sonnet-4-6")
        .key("sk-ant-recorded")
        .prompt("Say hello")
}

#[test]
fn a_rate_limited_reply_and_a_server_fault_are_retried_until_the_wire_answers() {
    let replay = Replay::new([
        Canned::reply(429, RATE_LIMITED),
        Canned::reply(503, OVERLOADED),
        Canned::stream(ANTHROPIC),
    ]);
    let pause = Recorded::default();
    let client = Client::over(replay.clone(), SecretSet::default()).waits(pause.clone());

    let answer = client
        .ask(&anthropic_ask(), &mut Transcript::default())
        .expect("the third attempt answers");

    assert_eq!(answer.text, "Hello world", "the answer that finally came");
    assert_eq!(replay.requests().len(), 3, "one request per attempt");
    assert_eq!(
        pause.delays(),
        [Duration::ZERO, BACKOFF, BACKOFF * 2],
        "the waits double, and the first attempt waits for nothing"
    );
}

#[test]
fn a_client_error_reaches_the_wire_once() {
    let replay = Replay::new([Canned::reply(400, BAD_REQUEST), Canned::stream(ANTHROPIC)]);
    let pause = Recorded::default();
    let client = Client::over(replay.clone(), SecretSet::default()).waits(pause.clone());

    let failure = client
        .ask(&anthropic_ask(), &mut Transcript::default())
        .expect_err("a 400 is not an answer");

    assert_eq!(failure.verdict, Verdict::Client);
    assert_eq!(replay.requests().len(), 1, "a client error is not resent");
    assert_eq!(
        pause.delays(),
        [Duration::ZERO],
        "it waits for no second attempt"
    );
}

#[test]
fn a_reply_that_streamed_a_delta_is_tried_once() {
    let replay = Replay::new([
        Canned::stream(ANTHROPIC_TRUNCATED),
        Canned::stream(ANTHROPIC),
    ]);
    let pause = Recorded::default();
    let client = Client::over(replay.clone(), SecretSet::default()).waits(pause.clone());
    let mut transcript = Transcript::default();

    let failure = client
        .ask(&anthropic_ask(), &mut transcript)
        .expect_err("a reply without its end is not an answer");

    assert_eq!(transcript.text, "Hello", "the fragment reached the caller");
    assert!(failure.streamed, "text had already gone out");
    assert_eq!(
        replay.requests().len(),
        1,
        "a second reply was recorded and never asked for"
    );
    assert_eq!(pause.delays(), [Duration::ZERO]);
}

#[test]
fn a_transport_failure_is_tried_three_times_and_then_given_up() {
    let replay = Replay::new(std::iter::empty::<Canned>());
    let pause = Recorded::default();
    let client = Client::over(replay.clone(), SecretSet::default()).waits(pause.clone());

    let failure = client
        .ask(&anthropic_ask(), &mut Transcript::default())
        .expect_err("no reply was ever recorded");

    assert_eq!(failure.verdict, Verdict::Transport);
    assert!(failure.retryable, "the wire called it transient");
    assert!(!failure.streamed);
    assert_eq!(
        replay.requests().len(),
        3,
        "three attempts, the first included"
    );
    assert_eq!(pause.delays(), [Duration::ZERO, BACKOFF, BACKOFF * 2]);
}

#[test]
fn a_cancel_is_tried_once_however_retryable_the_wire_calls_it() {
    let pause = Recorded::default();
    let mut attempts = 0;

    let failure = until_final(&pause, || {
        attempts += 1;
        Err::<(), _>(failure(Verdict::Cancelled, true, false))
    })
    .expect_err("a cancel is never an answer");

    assert_eq!(failure.verdict, Verdict::Cancelled);
    assert_eq!(attempts, 1, "§4 forbids retrying a cancel");
    assert_eq!(pause.delays(), [Duration::ZERO]);
}

#[test]
fn a_status_code_never_decides_because_the_wire_s_verdict_does() {
    let final_wait = Recorded::default();
    let mut final_attempts = 0;
    until_final(&final_wait, || {
        final_attempts += 1;
        Err::<(), _>(failure(Verdict::ServerFault, false, false))
    })
    .expect_err("a fault the wire called final");

    let transient_wait = Recorded::default();
    let mut transient_attempts = 0;
    until_final(&transient_wait, || {
        transient_attempts += 1;
        Err::<(), _>(failure(Verdict::Client, true, false))
    })
    .expect_err("a request the wire called transient");

    assert_eq!(
        final_attempts, 1,
        "a 5xx-shaped verdict the wire called final is not resent"
    );
    assert_eq!(
        transient_attempts, 3,
        "a 4xx-shaped verdict the wire called transient is"
    );
}

#[test]
fn an_extra_header_reaches_the_wire_with_the_session_expanded() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);
    let headers = vec![
        ("x-sindri-session".to_owned(), "{session}".to_owned()),
        (
            "x-sindri-gateway".to_owned(),
            "cache-per-session-{session}".to_owned(),
        ),
    ];
    let client = Client::over(
        Http::new(headers).session("7f3c-a1").over(replay.clone()),
        SecretSet::default(),
    );

    client
        .ask(&anthropic_ask(), &mut Transcript::default())
        .expect("a recorded reply answers");

    let sent = replay.requests();
    let headers = &sent[0].headers;
    assert!(
        headers.contains(&("x-sindri-session".to_owned(), "7f3c-a1".to_owned())),
        "the placeholder became the session id: {headers:?}"
    );
    assert!(
        headers.contains(&(
            "x-sindri-gateway".to_owned(),
            "cache-per-session-7f3c-a1".to_owned()
        )),
        "every placeholder in a value is expanded: {headers:?}"
    );
}
