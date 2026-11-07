//! Proves the route an OpenAI-shaped wire serves is rig's choice from the
//! model's own name — `chat` for `POST /chat/completions`, `responses` for
//! `POST /responses` — that the Copilot dialect answers on either with streamed
//! text, usage and the editor identity rig stamps, and that a person can make a
//! run take the other route when one misbehaves (§10).
//!
//! Nothing here reaches the network: the transport hands back bytes a fixture
//! recorded, so the request the wire really encodes is the one asserted on, and
//! the address it went to is how the route is proved rather than a field read
//! off an internal.

use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Ask, Client, Deltas, Route};
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

/// A Copilot Chat Completions reply, as the dialect streamed it for a name that
/// asks for no other route.
const CHAT: &str = include_str!("fixtures/providers/copilot_chat_stream.sse");
/// The same dialect's Responses reply, for a name whose identifier demands it.
const RESPONSES: &str = include_str!("fixtures/providers/openai_responses_stream.sse");

/// A sink that keeps every fragment that landed, so a test proves the answer
/// streamed rather than only that it folded.
#[derive(Debug, Default)]
struct Streamed {
    /// Every text fragment, joined in the order it arrived.
    text: String,
}

impl Deltas for Streamed {
    fn text(&mut self, fragment: &str) {
        self.text.push_str(fragment);
    }

    fn reasoning(&mut self, _fragment: &str) {}
}

/// A Copilot ask of `model`, carrying a session of its own: the seam's own
/// route, with the sign-in store left out of the picture.
fn copilot(model: &str) -> Ask {
    Ask::new(Wire::Copilot, model)
        .key("tid=recorded_copilot_session")
        .prompt("Say hello")
}

/// The address the one request `replay` was given went to.
fn addressed(replay: &Replay) -> String {
    let sent = replay.requests();
    let [request] = sent.as_slice() else {
        panic!("one request was sent, got {sent:?}");
    };
    request.url.clone()
}

#[test]
fn a_copilot_ask_on_a_plain_name_answers_from_chat_completions() {
    let replay = Replay::new([Canned::stream(CHAT)]);
    let mut streamed = Streamed::default();

    let answer = Client::over(replay.clone(), SecretSet::default())
        .ask(&copilot("gpt-4o"), &mut streamed)
        .expect("a recorded reply answers");

    assert_eq!(
        streamed.text, "Hello from Copilot",
        "the answer streamed as it landed"
    );
    assert_eq!(answer.text, "Hello from Copilot");
    assert_eq!(
        answer.usage.input,
        Some(12),
        "the prompt tokens are reported"
    );
    assert_eq!(answer.usage.output, Some(5), "the answer tokens are too");
    assert_eq!(answer.usage.total, Some(17));
    assert_eq!(answer.usage.cached_input, Some(4), "and the cache read");
    assert_eq!(
        addressed(&replay),
        "https://api.githubcopilot.com/chat/completions",
        "a name that asks for no other route is served by Chat Completions"
    );
}

#[test]
fn a_copilot_ask_on_a_codex_name_answers_from_the_responses_route() {
    let replay = Replay::new([Canned::stream(RESPONSES)]);
    let mut streamed = Streamed::default();

    let answer = Client::over(replay.clone(), SecretSet::default())
        .ask(&copilot("gpt-5.3-codex"), &mut streamed)
        .expect("a recorded reply answers");

    assert_eq!(streamed.text, "Hi there");
    assert_eq!(answer.usage.total, Some(13));
    assert_eq!(
        addressed(&replay),
        "https://api.githubcopilot.com/responses",
        "the identifier demands the other route, and rig chose it"
    );
}

#[test]
fn a_copilot_ask_carries_the_editor_identity_rig_stamps() {
    let replay = Replay::new([Canned::stream(CHAT)]);

    Client::over(replay.clone(), SecretSet::default())
        .ask(&copilot("gpt-4o"), &mut Streamed::default())
        .expect("a recorded reply answers");

    let sent = &replay.requests()[0];
    for name in [
        "editor-version",
        "editor-plugin-version",
        "copilot-integration-id",
        "user-agent",
        "openai-intent",
        "x-github-api-version",
        "x-initiator",
    ] {
        assert!(
            sent.headers.iter().any(|(header, _)| header == name),
            "the editor envelope carries {name}: {:?}",
            sent.headers
        );
    }
    assert!(
        sent.body.contains(r#""model":"gpt-4o""#),
        "the ask names the served name it was given: {}",
        sent.body
    );
}

#[test]
fn an_override_takes_a_codex_name_to_chat_completions() {
    let replay = Replay::new([Canned::stream(CHAT)]);
    let ask = copilot("gpt-5.3-codex").route(Route::Chat);

    let answer = Client::over(replay.clone(), SecretSet::default())
        .ask(&ask, &mut Streamed::default())
        .expect("a recorded reply answers");

    assert_eq!(answer.text, "Hello from Copilot");
    assert_eq!(
        addressed(&replay),
        "https://api.githubcopilot.com/chat/completions",
        "a route that misbehaves is overridden by name"
    );
    assert_eq!(
        ask.route_of(),
        Some(Route::Chat),
        "and the override is the route the record keeps"
    );
}

#[test]
fn an_override_takes_a_plain_name_to_the_responses_route() {
    let replay = Replay::new([Canned::stream(RESPONSES)]);
    let ask = copilot("gpt-4o").route(Route::Responses);

    Client::over(replay.clone(), SecretSet::default())
        .ask(&ask, &mut Streamed::default())
        .expect("a recorded reply answers");

    assert_eq!(
        addressed(&replay),
        "https://api.githubcopilot.com/responses",
        "the override names the route, not the model's own identifier"
    );
}

#[test]
fn a_route_is_chosen_by_the_name_unless_the_ask_overrides_it() {
    assert_eq!(copilot("gpt-4o").route_of(), Some(Route::Chat));
    assert_eq!(copilot("gpt-5.3-codex").route_of(), Some(Route::Responses));
    assert_eq!(
        Ask::new(Wire::Anthropic, "claude-sonnet-4-6").route_of(),
        None,
        "a dialect that serves one endpoint names no route"
    );
    assert_eq!(
        Ask::new(Wire::Openai, "gpt-5")
            .route(Route::Chat)
            .route_of(),
        Some(Route::Chat),
        "the other OpenAI-shaped wire takes the same dial"
    );
}

#[test]
fn a_route_is_named_by_the_word_the_config_holds() {
    assert_eq!(Route::Chat.word(), "chat");
    assert_eq!(Route::Responses.word(), "responses");
    assert_eq!(Route::Chat.to_string(), "chat");
    assert_eq!(Route::from_word("responses"), Some(Route::Responses));
    assert_eq!(Route::from_word("chat"), Some(Route::Chat));
    assert_eq!(
        Route::from_word("completions"),
        None,
        "a route this build cannot send is no route"
    );
    assert_eq!(
        Route::WORDS,
        ["chat", "responses"],
        "and the words are the two endpoints, in the order a listing offers them"
    );
}
