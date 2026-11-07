//! What the seam between the run loop and a wire promises: a turn's own words are
//! what the wire is asked, the answer's text is what the run answers with, a
//! refusal stops the run in the one line safe to print, and thinking is asked for
//! only when the run asked for it.
//!
//! The wire is the recorded bytes a fixture holds and the backoff waits for
//! nothing, so a test here reaches no network and spends no time.

use sindri_core::asking::Asking;
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Ask, Client, Deltas};
use sindri_core::redact::{Redacted, SecretSet};
use sindri_core::run::{Acted, Call, Executor, Loaded, Run, State, Stop};
use sindri_core::wire::Wire;

/// An Anthropic Messages reply, as the wire streamed it: two text fragments that
/// make one answer.
const ANTHROPIC: &str = include_str!("fixtures/providers/anthropic_stream.sse");

/// What a wire says when the request itself was wrong, so no retry can help.
const BAD_REQUEST: &str = r#"{"error":{"message":"unknown model","type":"invalid_request_error"}}"#;

/// A client over `replay` that waits for nothing.
fn client_over(replay: &Replay) -> Client {
    Client::over(replay.clone(), SecretSet::default()).waits(|_| {})
}

/// The ask a front end builds from its config: a wire, a served name, a
/// credential and the preamble that is re-sent every turn.
fn ask() -> Ask {
    Ask::new(Wire::Anthropic, "claude-sonnet-4-6")
        .key("sk-ant-recorded")
        .preamble("## Persona\n\nYou are Sindri.")
}

/// A tool the loop never reaches in these tests: a reply that asks for no call
/// is a complete answer, so nothing is executed.
struct Unreached;

impl Executor for Unreached {
    fn execute(&self, call: &Call, _loaded: &mut Loaded) -> Result<Acted, Redacted> {
        panic!(
            "no call was asked for, so `{}` cannot have been run",
            call.name
        )
    }
}

/// A sink that keeps what streamed, so a test sees the fragments and not only the
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

/// The request body the wire was sent, as the replay read it.
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
