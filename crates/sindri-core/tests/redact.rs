//! Proves redaction is a type rather than a habit: a `SecretSet` of literal
//! strings and a `Redacted` newtype whose `Display`, `Debug` and `Serialize`
//! all scrub, so a secret cannot leave the core by being printed.
//!
//! The four windows §11 names are the record, the JSON, the verbose stream and
//! the provider's own error text. Three of them are not written yet, so what is
//! proved here is the type's guarantee directly: bytes in, scrubbed bytes out.

use serde::Serialize;
use sindri_core::config::Config;
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Ask, Client, Verdict};
use sindri_core::redact::{Redacted, SecretSet};
use sindri_core::wire::Wire;

/// What a secret is written as wherever a value leaves core, spelled out here
/// rather than imported, because a test that read the constant could not tell
/// a mask from a mask that never changed.
const MASK: &str = "«redacted»";

/// One entry as a window would serialize it: a kind, and a body that has to
/// scrub itself because the type will not hold a secret.
#[derive(Debug, Serialize)]
struct Entry {
    /// What the entry is.
    kind: String,
    /// What it says, which is where a secret would otherwise be.
    body: Redacted,
}

#[test]
fn a_secret_that_is_a_prefix_of_another_is_scrubbed_whole() {
    let secrets = SecretSet::of(["sk-abc", "sk-abcdef"]);

    let printed = secrets.redact("the key is sk-abcdef").to_string();

    assert_eq!(
        printed,
        format!("the key is {MASK}"),
        "the longer secret is replaced first, so no fragment of it survives"
    );
}

#[test]
fn a_printed_value_carrying_no_secret_is_left_as_it_was() {
    let secrets = SecretSet::of(["sk-abc"]);

    assert_eq!(secrets.redact("sk-abc").to_string(), MASK);
    assert_eq!(secrets.redact("nothing here").to_string(), "nothing here");
}

#[test]
fn a_serialized_entry_never_holds_the_secret() {
    let secrets = SecretSet::of(["ghp-token-value"]);
    let entry = Entry {
        kind: String::from("tool_call"),
        body: secrets.redact("Authorization: ghp-token-value"),
    };

    let json = serde_json::to_string(&entry).expect("an entry serializes");

    assert_eq!(
        json,
        format!(r#"{{"kind":"tool_call","body":"Authorization: {MASK}"}}"#),
        "the JSON window writes the scrubbed form, not the secret"
    );
    assert!(!json.contains("ghp-token-value"));
}

#[test]
fn a_debugged_entry_never_holds_the_secret() {
    let secrets = SecretSet::of(["ghp-token-value"]);
    let entry = Entry {
        kind: String::from("tool_call"),
        body: secrets.redact("Authorization: ghp-token-value"),
    };

    let printed = format!("{entry:?}");

    assert!(
        !printed.contains("ghp-token-value"),
        "the verbose stream cannot print what Debug does not hold: {printed}"
    );
    assert!(
        printed.contains(MASK),
        "it says a secret was there: {printed}"
    );
}

#[test]
fn a_secret_set_prints_how_many_it_holds_and_never_which() {
    let secrets = SecretSet::of(["ghp-token-value", "sk-another"]);

    let printed = format!("{secrets:?}");

    assert!(!printed.contains("ghp-token-value"), "{printed}");
    assert!(!printed.contains("sk-another"), "{printed}");
}

#[test]
fn a_set_of_no_secrets_redacts_nothing_at_all() {
    let empty = SecretSet::of([] as [&str; 0]);

    assert!(empty.is_empty());
    assert_eq!(
        empty.redact("the key is sk-abc").to_string(),
        "the key is sk-abc",
        "an empty set changes nothing"
    );
    assert_eq!(
        SecretSet::of([""]).redact("sk-abc").to_string(),
        "sk-abc",
        "an empty string is not a secret"
    );
}

#[test]
fn the_configs_two_doors_are_the_key_and_every_header_value() {
    let config = Config {
        api_key: Some(String::from("sk-live-key")),
        extra_headers: Some(vec![
            String::from("Authorization: Bearer ghp-gateway"),
            String::from("X-Plain: nothing-secret"),
        ]),
        ..Config::default()
    };

    let secrets = config.secrets();

    assert_eq!(
        secrets.redact("the key sk-live-key is wrong").to_string(),
        format!("the key {MASK} is wrong")
    );
    assert_eq!(
        secrets.redact("Bearer ghp-gateway").to_string(),
        MASK,
        "a header's value is a secret on its own, wherever it shows up"
    );
    assert_eq!(
        secrets
            .redact("Authorization: Bearer ghp-gateway")
            .to_string(),
        MASK,
        "the whole header line is a secret too"
    );
    assert_eq!(
        secrets.redact("nothing-secret").to_string(),
        MASK,
        "every header value is a door, not only the credential-looking one"
    );
}

#[test]
fn a_config_holding_no_secret_redacts_nothing() {
    let secrets = Config::default().secrets();

    assert!(secrets.is_empty());
    assert_eq!(
        secrets.redact("sk-abc").to_string(),
        "sk-abc",
        "an empty set changes nothing"
    );
}

#[test]
fn a_refusal_that_echoes_the_key_comes_back_scrubbed() {
    let key = "sk-ant-echoed-back";
    let refusal = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key: sk-ant-echoed-back"}}"#;
    let replay = Replay::new([Canned::reply(401, refusal)]);
    let ask = Ask::new(Wire::Anthropic, "claude-sonnet-4-6")
        .key(key)
        .prompt("Say hello");

    let failure = Client::over(replay, SecretSet::of([key]))
        .ask(&ask, &mut ())
        .expect_err("a 401 is not an answer");

    assert_eq!(failure.verdict, Verdict::Client);
    assert!(
        !failure.to_string().contains(key),
        "the provider's own words came back with the key in them: {failure}"
    );
    assert!(
        !format!("{failure:?}").contains(key),
        "the verbose stream reads the same failure: {failure:?}"
    );
    assert!(
        failure.to_string().contains(MASK),
        "it still says a key was there: {failure}"
    );
    assert!(
        failure.to_string().contains("401"),
        "and still says what the wire said: {failure}"
    );
}
