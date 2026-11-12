//! What the provider seam promises, asked through the real wire implementations and the real
//! event-stream decoding, against replies recorded from the two providers. Nothing reaches the
//! network: the transport hands back recorded bytes, so only the socket is missing.

use serde_json::{Value, json};
use sindri_core::provider::transport::{Canned, Http, Replay};
use sindri_core::provider::{Ask, Client, Credential, Deltas, Earlier, Route, Verdict};
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

const ANTHROPIC: &str = include_str!("fixtures/providers/anthropic_stream.sse");
const ANTHROPIC_TRUNCATED: &str = include_str!("fixtures/providers/anthropic_truncated.sse");
const OPENAI_RESPONSES: &str = include_str!("fixtures/providers/openai_responses_stream.sse");
const CHAT: &str = include_str!("fixtures/providers/copilot_chat_stream.sse");
const BAD_REQUEST: &str = r#"{"error":{"message":"unknown model","type":"invalid_request_error"}}"#;

#[derive(Debug, Default)]
struct Transcript {
    text: String,
    reasoning: String,
}

impl Deltas for Transcript {
    fn text(&mut self, fragment: &str) {
        self.text.push_str(fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.reasoning.push_str(fragment);
    }
}

fn anthropic_ask() -> Ask {
    Ask::new(Wire::Anthropic, "claude-sonnet-4-6")
        .key("sk-ant-recorded")
        .prompt("Say hello")
}

fn client_over(replay: &Replay) -> Client {
    Client::over(replay.clone(), SecretSet::default())
}

#[test]
fn an_anthropic_reply_streams_its_text_and_reasoning_and_reports_usage() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);
    let mut transcript = Transcript::default();

    let answer = client_over(&replay)
        .ask(&anthropic_ask(), &mut transcript)
        .expect("a recorded reply answers");

    assert_eq!(
        transcript.reasoning, "Let me think.",
        "the thought streamed"
    );
    assert_eq!(transcript.text, "Hello world", "the answer streamed");
    assert_eq!(answer.text, "Hello world");
    assert_eq!(answer.reasoning.as_deref(), Some("Let me think."));
    assert_eq!(answer.usage.input, Some(11));
    assert_eq!(answer.usage.output, Some(7));
    assert_eq!(answer.usage.total, Some(18));
    assert_eq!(answer.usage.reasoning, None, "this reply reported no split");
}

#[test]
fn an_openai_reply_streams_its_text_and_reasoning_and_reports_usage() {
    let replay = Replay::new([Canned::stream(OPENAI_RESPONSES)]);
    let mut transcript = Transcript::default();
    let ask = Ask::new(Wire::Openai, "gpt-5")
        .key("sk-recorded")
        .prompt("Say hi");

    let answer = client_over(&replay)
        .ask(&ask, &mut transcript)
        .expect("a recorded reply answers");

    assert_eq!(transcript.reasoning, "Weighing it.");
    assert_eq!(transcript.text, "Hi there");
    assert_eq!(answer.text, "Hi there");
    assert_eq!(answer.reasoning.as_deref(), Some("Weighing it."));
    assert_eq!(answer.usage.input, Some(9));
    assert_eq!(answer.usage.output, Some(4));
    assert_eq!(answer.usage.total, Some(13));
    assert_eq!(answer.usage.reasoning, Some(2), "the thought is counted");
    assert_eq!(
        answer.usage.cached_input,
        Some(3),
        "the cache read is counted"
    );
    assert_eq!(
        replay.requests()[0].url,
        "https://api.openai.com/v1/responses",
        "the dialect chose the route the name demands"
    );
}

#[test]
fn the_wire_is_addressed_by_its_own_endpoint_and_sends_the_ask() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);

    client_over(&replay)
        .ask(&anthropic_ask(), &mut Transcript::default())
        .expect("a recorded reply answers");

    let sent = replay.requests();
    let [request] = sent.as_slice() else {
        panic!("one request was sent, got {sent:?}");
    };
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
    assert!(
        request.body.contains(r#""model":"claude-sonnet-4-6""#),
        "the ask names its model: {}",
        request.body
    );
}

#[test]
fn the_transport_carries_the_runs_extra_headers_to_the_wire() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);
    let headers = vec![("x-sindri-session".to_owned(), "s-1".to_owned())];
    let client = Client::over(
        Http::new(headers).over(replay.clone()),
        SecretSet::default(),
    );

    client
        .ask(&anthropic_ask(), &mut Transcript::default())
        .expect("a recorded reply answers");

    let request = &replay.requests()[0];
    assert!(
        request
            .headers
            .contains(&("x-sindri-session".to_owned(), "s-1".to_owned())),
        "the header reached the request: {:?}",
        request.headers
    );
}

#[test]
fn a_thinking_run_tells_the_wire_to_think_and_a_plain_run_does_not() {
    let thinking = Replay::new([Canned::stream(ANTHROPIC)]);
    client_over(&thinking)
        .ask(&anthropic_ask().thinking(true), &mut Transcript::default())
        .expect("a recorded reply answers");

    let plain = Replay::new([Canned::stream(ANTHROPIC)]);
    client_over(&plain)
        .ask(&anthropic_ask(), &mut Transcript::default())
        .expect("a recorded reply answers");

    assert!(
        thinking.requests()[0].body.contains(r#""thinking""#),
        "a thinking ask asks for it: {}",
        thinking.requests()[0].body
    );
    assert!(
        !plain.requests()[0].body.contains(r#""thinking""#),
        "a plain ask leaves it to the model: {}",
        plain.requests()[0].body
    );
}

#[test]
fn a_client_error_comes_back_with_a_verdict_that_forbids_retrying() {
    let replay = Replay::new([Canned::reply(400, BAD_REQUEST)]);

    let failure = client_over(&replay)
        .ask(&anthropic_ask(), &mut Transcript::default())
        .expect_err("a 400 is not an answer");

    assert_eq!(failure.verdict, Verdict::Client);
    assert!(!failure.retryable, "a client error retries nowhere");
    assert!(!failure.streamed, "nothing had streamed");
    assert!(
        failure.to_string().contains("400"),
        "the failure names what the wire said: {failure}"
    );
}

#[test]
fn a_rate_limited_ask_is_reported_as_retryable() {
    let replay = Replay::new(std::iter::repeat_n(Canned::reply(429, BAD_REQUEST), 3));

    let failure = Client::over(replay.clone(), SecretSet::default())
        .waits(|_| {})
        .ask(&anthropic_ask(), &mut Transcript::default())
        .expect_err("a 429 is not an answer");

    assert_eq!(failure.verdict, Verdict::RateLimited);
    assert!(failure.retryable, "a rate limit is worth another attempt");
    assert!(!failure.streamed);
    assert_eq!(
        replay.requests().len(),
        3,
        "every attempt was rate limited, so all three were spent"
    );
}

#[test]
fn a_reply_cut_short_after_it_streamed_text_is_marked_as_streamed() {
    let replay = Replay::new([Canned::stream(ANTHROPIC_TRUNCATED)]);
    let mut transcript = Transcript::default();

    let failure = client_over(&replay)
        .ask(&anthropic_ask(), &mut transcript)
        .expect_err("a reply without its end is not an answer");

    assert_eq!(transcript.text, "Hello", "what arrived, arrived");
    assert_eq!(failure.verdict, Verdict::Transport);
    assert!(
        failure.streamed,
        "text had already streamed, so §4 forbids a retry"
    );
}

#[test]
fn a_wire_that_needs_a_credential_never_sends_without_one() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);
    let ask = Ask::new(Wire::Anthropic, "claude-sonnet-4-6").prompt("Say hello");

    let failure = client_over(&replay)
        .ask(&ask, &mut ())
        .expect_err("nothing can be sent uncredentialed");

    assert_eq!(failure.verdict, Verdict::Unconfigured);
    assert!(!failure.retryable);
    assert!(
        replay.requests().is_empty(),
        "the wire was never asked: {:?}",
        replay.requests()
    );
}

#[test]
fn a_credential_is_never_printed() {
    let ask = Ask::new(Wire::Openai, "gpt-5").key("sk-do-not-print-me");

    let printed = format!("{ask:?}");

    assert!(
        !printed.contains("sk-do-not-print-me"),
        "the Debug of an ask hides its credential: {printed}"
    );
    assert_eq!(
        Credential::new("sk-do-not-print-me"),
        Credential::new("sk-do-not-print-me")
    );
}

fn sent_body(replay: &Replay) -> Value {
    serde_json::from_str(&replay.requests()[0].body).expect("a wire's body is JSON")
}

#[test]
fn an_openai_ask_on_the_responses_route_replays_the_turns_that_ended_before_it() {
    let replay = Replay::new([Canned::stream(OPENAI_RESPONSES)]);
    let ask = Ask::new(Wire::Openai, "gpt-5")
        .key("sk-recorded")
        .prompt("what is my name?")
        .earlier([Earlier {
            prompt: String::from("hi my name is Glenn"),
            answer: String::from("nice to meet you, Glenn"),
        }]);

    client_over(&replay)
        .ask(&ask, &mut Transcript::default())
        .expect("a recorded reply answers");

    let body = sent_body(&replay);
    assert_eq!(
        body["input"],
        json!([
            {
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "hi my name is Glenn"}]
            },
            {
                "type": "message",
                "role": "assistant",
                "id": "msg_rig_1",
                "status": "completed",
                "content": [
                    {
                        "type": "output_text",
                        "text": "nice to meet you, Glenn",
                        "annotations": []
                    }
                ]
            },
            {
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "what is my name?"}]
            },
        ]),
        "the earlier prompt and its answer are the route's own messages, ahead of the new prompt"
    );
}

#[test]
fn an_openai_ask_on_the_chat_route_replays_them_the_way_that_route_spells_them() {
    let replay = Replay::new([Canned::stream(CHAT)]);
    let ask = Ask::new(Wire::Openai, "gpt-4o")
        .key("sk-recorded")
        .prompt("what is my name?")
        .route(Route::Chat)
        .earlier([Earlier {
            prompt: String::from("hi my name is Glenn"),
            answer: String::from("nice to meet you, Glenn"),
        }]);

    client_over(&replay)
        .ask(&ask, &mut Transcript::default())
        .expect("a recorded reply answers");

    assert_eq!(
        sent_body(&replay)["messages"],
        json!([
            {"role": "user", "content": "hi my name is Glenn"},
            {"role": "assistant", "content": "nice to meet you, Glenn"},
            {"role": "user", "content": "what is my name?"},
        ]),
        "the same prompts read the same on either route"
    );
}
