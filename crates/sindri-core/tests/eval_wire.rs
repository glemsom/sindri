//! The bytes each wire sends, pinned: one ask per dialect and route goes out
//! through the replay transport, the request the transport was handed is read
//! back, and its method, endpoint, headers and body are compared against the
//! golden checked in beside this file (§10, §16).
//!
//! What a request says is the wire library's prose and not this crate's, so a
//! library that quietly renames a field or drops a header is exactly the change
//! no reader finds in a changelog: pinning the payload turns a dependency bump
//! into a diff that names itself. The credential is scrubbed out of what is
//! compared, so no golden holds a key — and a request that moved the key to
//! another header still shows up as a diff rather than passing in silence.
//!
//! `SINDRI_BLESS=1` rewrites a golden instead of comparing it, which is the only
//! way one moves: deliberately, in the commit that changed the wire.
//!
//! An eval is judged by `make evals` and never by `make test`, which is why every
//! test here is ignored: the stage that owns a failure is the stage that prints
//! it, and a `make test` that failed first would hide the eval behind itself.

use serde_json::{Map, Value, json};
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Ask, Client, Route};
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

/// The literals the asks below are credentialed with. It is not a real key and it is
/// planted on purpose: the golden must hold the mask where the key rode.
const KEY: &str = "sk-eval-recorded";

/// Where the accepted payloads live: beside the test that compares them, one file
/// per case.
const GOLDENS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/evals/wire");

/// The variable that rewrites a golden instead of comparing it, as the mermaid
/// corpus's bless is spelled: one word for every eval in the tree.
const BLESS: &str = "SINDRI_BLESS";

/// An Anthropic Messages reply, recorded mid-stream.
const ANTHROPIC: &str = include_str!("fixtures/providers/anthropic_stream.sse");
/// An OpenAI Responses reply, reasoning summarized before the answer.
const OPENAI_RESPONSES: &str = include_str!("fixtures/providers/openai_responses_stream.sse");
/// A chat-completions reply, recorded from the wire that serves that route.
const CHAT: &str = include_str!("fixtures/providers/copilot_chat_stream.sse");

/// Every payload this crate sends, named as its golden is: the ask that produces a
/// request, and the recorded reply that lets the ask finish. One case per dialect and
/// route the run can choose, because a route is a different body and not a different
/// word for the same one (§10, Appendix A10).
fn cases() -> Vec<(&'static str, Ask, &'static str)> {
    vec![
        (
            "anthropic-messages",
            Ask::new(Wire::Anthropic, "claude-sonnet-4-6")
                .key(KEY)
                .preamble("You are a shell.")
                .prompt("Say hello"),
            ANTHROPIC,
        ),
        (
            "anthropic-thinking",
            Ask::new(Wire::Anthropic, "claude-sonnet-4-6")
                .key(KEY)
                .prompt("Say hello")
                .thinking(true),
            ANTHROPIC,
        ),
        (
            "openai-responses",
            Ask::new(Wire::Openai, "gpt-5")
                .key(KEY)
                .preamble("You are a shell.")
                .prompt("Say hello")
                .thinking(true),
            OPENAI_RESPONSES,
        ),
        (
            "openai-chat",
            Ask::new(Wire::Openai, "gpt-4o")
                .key(KEY)
                .prompt("Say hello")
                .route(Route::Chat),
            CHAT,
        ),
        (
            "copilot-chat",
            Ask::new(Wire::Copilot, "gpt-5")
                .key(KEY)
                .preamble("You are a shell.")
                .prompt("Say hello"),
            CHAT,
        ),
        (
            "gateway-responses",
            Ask::new(Wire::Openai, "gpt-5")
                .key(KEY)
                .prompt("Say hello")
                .base_url("https://gateway.example/v1"),
            OPENAI_RESPONSES,
        ),
    ]
}

/// The request `ask` sends, as the artefact a golden holds: the method, the endpoint
/// the bytes went to, every header with the credential scrubbed, and the body parsed
/// so it compares as the fields it is rather than as one long line.
///
/// `x-request-id` is left out: the library mints one per request, so it is a fact
/// about a request and never about the payload, and a golden that held it would fail
/// on every run.
fn artefact(ask: &Ask, reply: &'static str, secrets: &SecretSet) -> Value {
    let replay = Replay::new([Canned::stream(reply)]);
    let client = Client::over(replay.clone(), secrets.clone());
    client
        .ask(ask, &mut ())
        .expect("the recorded reply answers the ask");

    let requests = replay.requests();
    assert_eq!(
        requests.len(),
        1,
        "one ask is one request: a retry here would mean the recorded reply was refused, \
         and the payload a golden holds would be the second attempt's"
    );
    let seen = &requests[0];

    let mut headers = Map::new();
    for (name, value) in &seen.headers {
        if name == "x-request-id" {
            continue;
        }
        headers.insert(
            name.clone(),
            Value::String(secrets.redact(value.as_str()).to_string()),
        );
    }

    json!({
        "method": seen.method,
        "url": seen.url,
        "headers": headers,
        "body": serde_json::from_str::<Value>(&seen.body).unwrap_or_else(|error| {
            panic!(
                "a wire's body is JSON, and this one is not: {error}\n{}",
                seen.body
            )
        }),
    })
}

/// The first line two payloads disagree on, with its number and both sides, because a
/// diff nobody can read is a test nobody fixes.
fn diff(accepted: &str, sent: &str) -> String {
    for (at, (left, right)) in accepted
        .lines()
        .zip(sent.lines())
        .enumerate()
        .map(|(at, pair)| (at + 1, pair))
    {
        if left != right {
            return format!("line {at} differs:\n  accepted: {left}\n  sent:     {right}");
        }
    }
    format!(
        "the accepted payload has {} lines and the sent one has {}",
        accepted.lines().count(),
        sent.lines().count()
    )
}

/// Holds `wrote` to the golden `case` names, or rewrites that golden when `bless` says
/// so.
fn hold(case: &str, wrote: &str) {
    let path = format!("{GOLDENS}/{case}.json");
    if std::env::var_os(BLESS).is_some() {
        std::fs::write(&path, wrote).unwrap_or_else(|error| panic!("{path} is written: {error}"));
        return;
    }
    let accepted = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "the golden {path} is readable: {error}; run `SINDRI_BLESS=1 make evals` to write it"
        )
    });
    assert!(
        accepted == wrote,
        "wire payload `{case}`: {}\nrun `SINDRI_BLESS=1 make evals` to move the golden \
         deliberately",
        diff(&accepted, wrote)
    );
}

#[test]
#[ignore = "an eval is judged by `make evals`, not by `make test`"]
fn every_wire_sends_the_payload_the_golden_holds() {
    let secrets = SecretSet::of([KEY]);
    let mut moved = Vec::new();
    for (case, ask, reply) in cases() {
        let wrote = format!(
            "{}\n",
            serde_json::to_string_pretty(&artefact(&ask, reply, &secrets))
                .expect("a payload is JSON and serialises")
        );
        assert!(
            !wrote.contains(KEY),
            "wire payload `{case}`: the credential rode into the artefact a golden holds, \
             so the one place it belongs is the mask:\n{wrote}"
        );
        hold(case, &wrote);
        moved.push(case);
    }
    println!(
        "\ncheck: eval wire-payloads ... ok ({} of them)",
        moved.len()
    );
}
