//! What the seam between the run loop and a wire promises: a turn's own words are what the wire is
//! asked, the answer's text is what the run answers with, a refusal stops the run in the one line
//! safe to print, and thinking is asked for only when the run asked for it.

use serde_json::{Value, json};
use sindri_core::asking::Asking;
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Ask, Client, Deltas, Usage};
use sindri_core::redact::{Redacted, SecretSet};
use sindri_core::run::{Acted, Call, Exchange, Executor, Loaded, Run, State, Stop};
use sindri_core::wire::Wire;

const ANTHROPIC: &str = include_str!("fixtures/providers/anthropic_stream.sse");

const BAD_REQUEST: &str = r#"{"error":{"message":"unknown model","type":"invalid_request_error"}}"#;

fn client_over(replay: &Replay) -> Client {
    Client::over(replay.clone(), SecretSet::default()).waits(|_| {})
}

fn ask() -> Ask {
    Ask::new(Wire::Anthropic, "claude-sonnet-4-6")
        .key("sk-ant-recorded")
        .preamble("## Persona\n\nYou are Sindri.")
}

struct Unreached;

impl Executor for Unreached {
    fn execute(&self, call: &Call, _loaded: &mut Loaded) -> Result<Acted, Redacted> {
        panic!(
            "no call was asked for, so `{}` cannot have been run",
            call.name
        )
    }
}

#[derive(Debug, Default)]
struct Transcript {
    text: String,
}

impl Deltas for Transcript {
    fn text(&mut self, fragment: &str) {
        self.text.push_str(fragment);
    }

    fn reasoning(&mut self, _fragment: &str) {}
}

fn turned(turn: u32, prompt: &str, answer: &str) -> Exchange {
    Exchange {
        turn,
        prompt: String::from(prompt),
        text: String::from(answer),
        calls: Vec::new(),
        usage: Usage::default(),
    }
}

fn asked(replay: &Replay) -> Value {
    let requests = replay.requests();
    serde_json::from_str(&requests[0].body).expect("a wire's body is JSON")
}

fn body_for(asking: Asking, replay: &Replay) -> String {
    let mut run = Run::new("triage this", asking, Unreached);
    let mut transcript = Transcript::default();
    run.drive(&mut transcript);
    let requests = replay.requests();
    assert_eq!(requests.len(), 1, "one turn asks the wire once");
    requests[0].body.clone()
}

#[test]
fn a_turns_own_words_and_the_preamble_are_what_the_wire_is_asked() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);

    let body = body_for(Asking::new(client_over(&replay), ask()), &replay);

    assert!(
        body.contains("triage this"),
        "the human's own words are the prompt: {body}"
    );
    assert!(
        body.contains("You are Sindri."),
        "the preamble rides beside the prompt as the system message: {body}"
    );
    assert!(
        body.contains("claude-sonnet-4-6"),
        "the served name the config named, and no other: {body}"
    );
    assert_eq!(
        replay.requests()[0].url,
        "https://api.anthropic.com/v1/messages",
        "the dialect's own endpoint, because no base URL was given"
    );
}

#[test]
fn the_answers_text_is_what_the_run_answers_with() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);
    let mut run = Run::new(
        "triage this",
        Asking::new(client_over(&replay), ask()),
        Unreached,
    );
    let mut transcript = Transcript::default();

    let state = run.drive(&mut transcript);

    assert_eq!(state, State::Stopped(Stop::Final));
    assert_eq!(
        run.answer(),
        Some("Hello world"),
        "the fragments the wire streamed, folded into one answer"
    );
    assert_eq!(
        transcript.text, "Hello world",
        "and the same text reached the front end as it landed (§4)"
    );
    assert_eq!(run.turns(), 1, "one ask, one turn");
}

#[test]
fn a_reply_carries_what_its_ask_spent_and_the_total_stays_the_runs() {
    let replay = Replay::new([Canned::stream(ANTHROPIC), Canned::stream(ANTHROPIC)]);
    let asking = Asking::new(client_over(&replay), ask());
    let mut first = Run::new("triage this", asking.clone(), Unreached);
    let mut second = Run::new("triage this", asking.clone(), Unreached);

    first.drive(&mut Transcript::default());
    second.drive(&mut Transcript::default());

    assert_eq!(
        first.history()[0].usage.input,
        Some(11),
        "the turn's exchange says what that turn's ask was handed"
    );
    assert_eq!(
        second.history()[0].usage.input,
        Some(11),
        "a second ask of the same history is the same size, not twice it"
    );
    assert_eq!(
        asking.spent().input,
        Some(22),
        "while the total the record and the outcome read is still every ask summed"
    );
}

#[test]
fn a_refused_ask_stops_the_run_in_the_one_line_safe_to_print() {
    let replay = Replay::new([Canned::reply(400, BAD_REQUEST)]);
    let mut run = Run::new(
        "triage this",
        Asking::new(client_over(&replay), ask()),
        Unreached,
    );
    let mut transcript = Transcript::default();

    let state = run.drive(&mut transcript);

    let Stop::Error(message) = state.stop().cloned().expect("a stop reason") else {
        panic!("a refused request ends the run as an error, not as an answer");
    };
    assert!(
        message.to_string().contains("unknown model"),
        "the wire's own words, so a person can fix them: {message}"
    );
    assert_eq!(run.answer(), None, "a failed run answers nothing (§15)");
    assert_eq!(replay.requests().len(), 1, "a 400 is not retried (§4)");
}

#[test]
fn thinking_is_asked_for_when_the_run_asked_for_it_and_never_otherwise() {
    let asking = Replay::new([Canned::stream(ANTHROPIC)]);
    let quiet = body_for(Asking::new(client_over(&asking), ask()), &asking);

    let thinking = Replay::new([Canned::stream(ANTHROPIC)]);
    let thought = body_for(
        Asking::new(client_over(&thinking), ask().thinking(true)),
        &thinking,
    );

    assert!(
        !quiet.contains("thinking"),
        "a run that asked for no reasoning asks the wire for none: {quiet}"
    );
    assert!(
        thought.contains("thinking"),
        "and a run that asked for it says so in the request: {thought}"
    );
}

#[test]
fn the_turns_that_ended_before_a_run_are_asked_ahead_of_its_prompt() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);
    let mut run = Run::new(
        "what is my name?",
        Asking::new(client_over(&replay), ask()),
        Unreached,
    )
    .earlier(vec![
        turned(1, "hi my name is Glenn", "nice to meet you, Glenn"),
        turned(2, "is it raining?", "it is raining"),
    ]);

    run.drive(&mut Transcript::default());

    let body = asked(&replay);
    assert_eq!(
        body["messages"],
        json!([
            {"role": "user", "content": [{"type": "text", "text": "hi my name is Glenn"}]},
            {
                "role": "assistant",
                "content": [{"type": "text", "text": "nice to meet you, Glenn"}]
            },
            {"role": "user", "content": [{"type": "text", "text": "is it raining?"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "it is raining"}]},
            {"role": "user", "content": [{"type": "text", "text": "what is my name?"}]},
        ]),
        "each prompt rides beside the answer it got, in the order they ended, with the new \
         prompt last"
    );
    assert_eq!(
        body["system"].as_array().map(Vec::len),
        Some(1),
        "the preamble stays one system message, not one per turn: {}",
        body["system"]
    );
}

#[test]
fn a_turn_that_ended_with_no_answer_is_not_sent_as_an_empty_message() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);
    let mut run = Run::new(
        "what is my name?",
        Asking::new(client_over(&replay), ask()),
        Unreached,
    )
    .earlier(vec![
        turned(1, "hi my name is Glenn", "nice to meet you, Glenn"),
        turned(2, "what is the weather?", ""),
    ]);

    run.drive(&mut Transcript::default());

    assert_eq!(
        asked(&replay)["messages"],
        json!([
            {"role": "user", "content": [{"type": "text", "text": "hi my name is Glenn"}]},
            {
                "role": "assistant",
                "content": [{"type": "text", "text": "nice to meet you, Glenn"}]
            },
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "what is the weather?"},
                    {"type": "text", "text": "what is my name?"},
                ]
            },
        ]),
        "a prompt that got no answer travels alone — no empty message, because every dialect \
         refuses one — and the dialect merges the two user turns it leaves side by side"
    );
}

#[test]
fn a_run_handed_nothing_asks_exactly_what_it_asked_before() {
    let replay = Replay::new([Canned::stream(ANTHROPIC)]);

    let body = body_for(Asking::new(client_over(&replay), ask()), &replay);

    assert_eq!(
        asked(&replay)["messages"],
        json!([
            {"role": "user", "content": [{"type": "text", "text": "triage this"}]},
        ]),
        "one prompt and nothing ahead of it: {body}"
    );
}
