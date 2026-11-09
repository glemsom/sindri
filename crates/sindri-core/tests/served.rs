//! What a wire says it serves, asked through the real listing wires of every dialect, against
//! catalogues recorded from the two vendors. Nothing reaches the network: the transport hands back
//! recorded bytes, so the parsing and the request are a live run's and only the socket is missing.

use std::sync::{Arc, Mutex};

use rig_core::http_client::{
    Bytes, Error, HttpClientExt, LazyBody, MultipartForm, Request, Response, StreamingResponse,
};
use sindri_core::provider::served::{Listing, Where};
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Ask, Client, Credential};
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

const ANTHROPIC_MODELS: &str = include_str!("fixtures/providers/anthropic_models.json");
const OPENAI_MODELS: &str = include_str!("fixtures/providers/openai_models.json");
const ANTHROPIC_STREAM: &str = include_str!("fixtures/providers/anthropic_stream.sse");
const NO_LISTING: &str =
    r#"{"error":{"message":"this gateway has no model catalogue","type":"not_implemented"}}"#;

const SONNET: &str = "claude-sonnet-4-6";
const HAIKU: &str = "claude-haiku-4-5";
const GPT_5: &str = "gpt-5";

fn client_over(replay: &Replay) -> Client {
    Client::over(replay.clone(), SecretSet::default()).waits(|_| {})
}

#[test]
fn a_listing_is_read_off_the_wire_and_kept_beside_the_pair_that_produced_it() {
    let replay = Replay::new([Canned::reply(200, ANTHROPIC_MODELS)]);
    let at = Where::own(Wire::Anthropic);

    let listing = client_over(&replay).served(&at, &Credential::new("sk-ant-recorded"), &at);

    let names = listing.names().expect("the wire answered with a catalogue");
    assert_eq!(names.len(), 2, "both entries the wire listed, and no more");
    assert_eq!(names[0].id, SONNET, "the identifier is the wire's own");
    assert_eq!(names[0].label, "Claude Sonnet 4.6", "and so is its prose");
    assert_eq!(names[1].id, HAIKU, "the wire's order is kept");
    assert_eq!(listing.at(), &at, "kept beside the pair that produced it");
    assert_eq!(
        replay.requests()[0].url,
        "https://api.anthropic.com/v1/models",
        "the catalogue was read off the wire, not remembered"
    );
    assert!(
        listing.hint().is_none(),
        "an answered listing needs no hint"
    );
}

#[test]
fn an_openai_catalogue_is_read_at_its_own_endpoint() {
    let replay = Replay::new([Canned::reply(200, OPENAI_MODELS)]);
    let at = Where::own(Wire::Openai);

    let listing = client_over(&replay).served(&at, &Credential::new("sk-recorded"), &at);

    let names = listing.names().expect("the wire answered with a catalogue");
    assert_eq!(names.first().map(|served| served.id.as_str()), Some(GPT_5));
    assert_eq!(
        replay.requests()[0].url,
        "https://api.openai.com/v1/models",
        "the dialect's own listing route"
    );
}

#[test]
fn a_gateway_base_url_is_where_the_listing_is_read() {
    let replay = Replay::new([Canned::reply(200, OPENAI_MODELS)]);
    let at = Where::at(Wire::Openai, "https://gateway.example/v1");

    let listing = client_over(&replay).served(&at, &Credential::new("sk-recorded"), &at);

    assert!(listing.names().is_some(), "the gateway answered");
    assert_eq!(
        replay.requests()[0].url,
        "https://gateway.example/v1/models",
        "the pair names the endpoint, so the endpoint is where it was read"
    );
    assert_eq!(listing.at().base_url(), Some("https://gateway.example/v1"));
}

/// A transport that lets the wire move while a request is in flight: the reply is the recorded one,
/// but the pair in force changes on the way past, which is what a provider row edited during a
/// fetch does.
#[derive(Clone)]
struct Moving {
    inner: Replay,
    meanwhile: Arc<dyn Fn() + Send + Sync>,
}

impl HttpClientExt for Moving {
    fn send<T, U>(
        &self,
        request: Request<T>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        T: Into<Bytes> + Send,
        U: From<Bytes> + Send + 'static,
    {
        (self.meanwhile)();
        self.inner.send::<T, U>(request)
    }

    fn send_multipart<U>(
        &self,
        request: Request<MultipartForm>,
    ) -> impl Future<Output = Result<Response<LazyBody<U>>, Error>> + Send + 'static
    where
        U: From<Bytes> + Send + 'static,
    {
        (self.meanwhile)();
        self.inner.send_multipart::<U>(request)
    }

    fn send_streaming<T>(
        &self,
        request: Request<T>,
    ) -> impl Future<Output = Result<StreamingResponse, Error>> + Send
    where
        T: Into<Bytes> + Send,
    {
        (self.meanwhile)();
        self.inner.send_streaming::<T>(request)
    }
}

#[derive(Clone)]
struct Cell(Arc<Mutex<Where>>);

impl Cell {
    fn holding(pair: &Where) -> Self {
        Self(Arc::new(Mutex::new(pair.clone())))
    }

    fn now(&self) -> Where {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn client(&self, replay: &Replay, moved: &Where) -> Client {
        let cell = self.0.clone();
        let moved = moved.clone();
        let meanwhile = move || {
            let mut held = cell
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *held = moved.clone();
        };
        Client::over(
            Moving {
                inner: replay.clone(),
                meanwhile: Arc::new(meanwhile),
            },
            SecretSet::default(),
        )
        .waits(|_| {})
    }
}

#[test]
fn a_listing_that_lands_on_the_wire_it_started_from_is_kept() {
    let replay = Replay::new([Canned::reply(200, ANTHROPIC_MODELS)]);
    let at = Where::own(Wire::Anthropic);
    let cell = Cell::holding(&at);

    let listing =
        cell.client(&replay, &at)
            .served(&at, &Credential::new("sk-ant-recorded"), &|| cell.now());

    assert_eq!(
        listing.names().map(|names| names.len()),
        Some(2),
        "nothing moved, so the catalogue is kept"
    );
    assert!(listing.hint().is_none());
}

#[test]
fn a_listing_that_lands_after_the_wire_moved_is_discarded() {
    let replay = Replay::new([Canned::reply(200, ANTHROPIC_MODELS)]);
    let at = Where::own(Wire::Anthropic);
    let moved = Where::own(Wire::Openai);
    let cell = Cell::holding(&at);

    let listing =
        cell.client(&replay, &moved)
            .served(&at, &Credential::new("sk-ant-recorded"), &|| cell.now());

    assert_eq!(
        listing.names(),
        None,
        "names read from one wire are no evidence about another"
    );
    assert_eq!(
        listing.at(),
        &at,
        "and the pair it is kept beside is unmoved"
    );
    let hint = listing.hint().expect("a discarded listing says why");
    assert!(
        hint.contains("anthropic") && hint.contains("moved"),
        "the hint names the pair that moved: {hint}"
    );
}

#[test]
fn a_listing_that_lands_after_the_base_url_moved_is_discarded() {
    let replay = Replay::new([Canned::reply(200, OPENAI_MODELS)]);
    let at = Where::at(Wire::Openai, "https://gateway.example/v1");
    let moved = Where::at(Wire::Openai, "https://other.example/v1");
    let cell = Cell::holding(&at);

    let listing =
        cell.client(&replay, &moved)
            .served(&at, &Credential::new("sk-recorded"), &|| cell.now());

    assert_eq!(
        listing.names(),
        None,
        "the same wire at another endpoint is another wire's catalogue"
    );
    let hint = listing.hint().expect("a discarded listing says why");
    assert!(
        hint.contains("https://gateway.example/v1"),
        "the hint names the endpoint that moved: {hint}"
    );
}

fn listing_over(
    replies: impl IntoIterator<Item = Canned>,
    credential: Credential,
) -> (Listing, Replay) {
    let replay = Replay::new(replies);
    let at = Where::own(Wire::Anthropic);
    let listing = client_over(&replay).served(&at, &credential, &at);
    (listing, replay)
}

#[test]
fn a_wire_that_answers_with_an_error_hands_back_a_hint_instead_of_a_list() {
    let (listing, _) = listing_over(
        [Canned::reply(404, NO_LISTING)],
        Credential::new("sk-ant-recorded"),
    );

    assert_eq!(listing.names(), None, "a 404 is no catalogue");
    let hint = listing.hint().expect("and it says so in one line");
    assert!(
        hint.contains("anthropic") && hint.contains("name a model"),
        "the hint names the wire and what to do: {hint}"
    );
    assert!(
        !hint.contains('{') && !hint.contains('\n'),
        "a hint is a line, not the wire's reply: {hint}"
    );
}

#[test]
fn a_gateway_that_offers_no_listing_hands_back_a_hint_and_the_run_still_starts() {
    let replay = Replay::new([
        Canned::reply(501, NO_LISTING),
        Canned::stream(ANTHROPIC_STREAM),
    ]);
    let at = Where::own(Wire::Anthropic);
    let client = client_over(&replay);

    let listing = client.served(&at, &Credential::new("sk-ant-recorded"), &at);
    assert_eq!(
        listing.names(),
        None,
        "a gateway without a catalogue has none"
    );

    let answer = client
        .ask(
            &Ask::new(Wire::Anthropic, SONNET)
                .key("sk-ant-recorded")
                .prompt("Say hello"),
            &mut (),
        )
        .expect("no list is not a refusal: the run goes on");
    assert_eq!(answer.text, "Hello world");
}

#[test]
fn a_wire_that_never_answers_hands_back_a_hint_and_no_list() {
    let (listing, _) = listing_over([], Credential::new("sk-ant-recorded"));

    assert_eq!(
        listing.names(),
        None,
        "nothing arrived, so nothing is known"
    );
    let hint = listing.hint().expect("and it says so in one line");
    assert!(
        hint.contains("anthropic"),
        "the hint names the wire that was asked: {hint}"
    );
}

#[test]
fn a_wire_without_a_credential_is_never_asked_and_hands_back_a_hint() {
    let (listing, replay) =
        listing_over([Canned::reply(200, ANTHROPIC_MODELS)], Credential::none());

    assert_eq!(listing.names(), None, "an unasked wire has no catalogue");
    let hint = listing
        .hint()
        .expect("and it says where the key comes from");
    assert!(
        hint.contains("ANTHROPIC_API_KEY"),
        "the hint names the variable §11 reads: {hint}"
    );
    assert!(
        replay.requests().is_empty(),
        "nothing was sent without a credential: {:?}",
        replay.requests()
    );
}

#[test]
fn a_copilot_catalogue_is_read_at_the_dialects_own_route() {
    let replay = Replay::new([Canned::reply(200, OPENAI_MODELS)]);
    let at = Where::own(Wire::Copilot);

    let listing = client_over(&replay).served(&at, &Credential::new("tid=recorded"), &at);

    let names = listing
        .names()
        .expect("the dialect answered with a catalogue");
    assert_eq!(names.first().map(|served| served.id.as_str()), Some(GPT_5));
    let url = &replay.requests()[0].url;
    assert!(
        url.ends_with("/models") && url.contains("copilot"),
        "the copilot dialect lists at its own route: {url}"
    );
    assert!(
        listing.hint().is_none(),
        "an answered listing needs no hint"
    );
}
