//! The provider seam: an ask goes out in Sindri's own words and a reply comes
//! back, or a failure safe to print, so nothing above this module knows which
//! client library carried it, how a reply was framed, or which dialect spoke.
//!
//! One ask is one request to one model on one wire: [`Ask`] holds the dialect,
//! the served name, the endpoint, the credential, the preamble and the prompt.
//! What comes back is [`Answer`] — text, the reasoning that preceded it, and the
//! usage the wire reported — arriving in fragments through [`Deltas`] as they
//! land, because §4 streams text and reasoning as they stream.
//!
//! A failure is [`Failure`], never a client library's error type: it carries the
//! verdict the wire classified (§4), whether the wire says another attempt could
//! succeed, and whether text or reasoning had already streamed when it failed.
//! Those three fields are all §4's retry policy reads, and [`retry`] reads them:
//! a reply that already streamed is never retried, however retryable its verdict.
//!
//! How a request's bytes travel is [`transport`], and a test replaces it with
//! recorded bytes, which is what keeps this seam testable with no network. The
//! one place a request leaves the process is also the one place it is sent
//! again: [`retry`] tries it at most three times, on the verdict and never on a
//! status code.
//!
//! A failure's one line is a [`Redacted`], built through the client's
//! [`SecretSet`]: a provider that refuses a key by quoting it back is the one
//! place a secret would otherwise ride a diagnostic into a transcript, so the
//! text is scrubbed where it is born rather than by every window after it
//! (ADR-0011). Nothing here writes a record: that is ticket #17.

pub mod retry; // §4's retry policy in one place: an ask is tried three times
pub mod served; // what a wire says it serves, read off the wire and kept beside the (wire, base_url) pair that produced them
pub mod transport; // how a request's bytes travel, and the fake a test substitutes

use std::fmt;
use std::sync::Arc;

use futures::StreamExt as _;
use rig_core::Model;
use rig_core::completion::message::AssistantContent;
use rig_core::completion::{
    CompletionRequest, CompletionResponse, Effort, GenerationOptions, Reasoning,
};
use rig_core::error::{ErrorKind, ProviderError};
use rig_core::http_client::{DynHttpClient, HttpClientExt};
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::openai::OpenAIConfig;
use rig_core::streaming::{Item, StreamEvent};
use rig_core::wire::{Encoded, Wire as RigWire, WireFrame};

use self::retry::{Backoff, Sleep, until_final};
use self::transport::Http;
use crate::redact::{MASK, Redacted, SecretSet};
use crate::wire::Wire;

/// The credential a wire is asked with, kept opaque so every printed form of it
/// is masked rather than trusted.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Credential(Option<String>);

impl Credential {
    /// A credential whose value is `key`.
    pub fn new(key: impl Into<String>) -> Self {
        Self(Some(key.into()))
    }

    /// No credential, which is a wire that needs none.
    pub fn none() -> Self {
        Self(None)
    }

    /// The key itself, for the one place that hands it to a wire; `None` when
    /// there is no key or it is empty, which is how an unset variable reads.
    fn expose(&self) -> Option<&str> {
        self.0.as_deref().filter(|key| !key.is_empty())
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.expose() {
            Some(_) => formatter.write_str(MASK),
            None => formatter.write_str("none"),
        }
    }
}

/// One question put to one wire: everything §10 needs besides the transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    /// The dialect to speak.
    wire: Wire,
    /// The served name the wire answered with, never invented here.
    model: String,
    /// Where the endpoint is, when it is not the dialect's own.
    base_url: Option<String>,
    /// What the request is sent with.
    credential: Credential,
    /// The system message, re-sent every turn and never part of the prompt.
    preamble: Option<String>,
    /// What the human asked.
    prompt: String,
    /// Whether the model should reason before it answers.
    thinking: bool,
}

impl Ask {
    /// An ask of `model` on `wire` with an empty prompt.
    pub fn new(wire: Wire, model: impl Into<String>) -> Self {
        Self {
            wire,
            model: model.into(),
            base_url: None,
            credential: Credential::none(),
            preamble: None,
            prompt: String::new(),
            thinking: false,
        }
    }

    /// The ask's prompt: the human's own words.
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    /// The preamble, sent as the system message ahead of the prompt.
    pub fn preamble(mut self, preamble: impl Into<String>) -> Self {
        self.preamble = Some(preamble.into());
        self
    }

    /// The credential the wire is asked with.
    pub fn credential(mut self, credential: Credential) -> Self {
        self.credential = credential;
        self
    }

    /// The credential a key names, for the common case.
    pub fn key(self, key: impl Into<String>) -> Self {
        self.credential(Credential::new(key))
    }

    /// Another endpoint than the dialect's own, as a gateway or a proxy gives.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Whether the model reasons before it answers. `false` asks for nothing
    /// rather than forbidding thought: a model that cannot stop thinking must
    /// still answer.
    pub fn thinking(mut self, thinking: bool) -> Self {
        self.thinking = thinking;
        self
    }

    /// The request this ask is, in the wording the wire's encoders take.
    fn request(&self) -> CompletionRequest {
        let mut request = CompletionRequest::new(self.prompt.clone());
        if let Some(preamble) = &self.preamble {
            request = request.preamble(preamble.clone());
        }
        let mut options = GenerationOptions::default();
        options.reasoning = self.thinking.then_some(Reasoning::Effort(Effort::High));
        request.options(options)
    }
}

/// What a reply says as it arrives, one fragment at a time.
pub trait Deltas {
    /// One fragment of answer text landed.
    fn text(&mut self, fragment: &str);

    /// One fragment of reasoning landed, before or between text fragments.
    fn reasoning(&mut self, fragment: &str);
}

/// A sink for a caller with nothing to show while it waits.
impl Deltas for () {
    fn text(&mut self, _fragment: &str) {}

    fn reasoning(&mut self, _fragment: &str) {}
}

/// What one ask spent, as much of it as the wire reported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Every input token, cache reads included.
    pub input: Option<u64>,
    /// Every output token, reasoning included.
    pub output: Option<u64>,
    /// `input + output`, when both were reported.
    pub total: Option<u64>,
    /// The part of `input` the provider served from its own cache.
    pub cached_input: Option<u64>,
    /// The part of `output` spent reasoning.
    pub reasoning: Option<u64>,
}

/// What one ask produced, folded from the reply the wire finished.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    /// The text the model answered with.
    pub text: String,
    /// The reasoning the model showed, when it showed any.
    pub reasoning: Option<String>,
    /// The tokens this ask spent.
    pub usage: Usage,
}

impl From<&CompletionResponse> for Answer {
    fn from(response: &CompletionResponse) -> Self {
        let text: String = response
            .choice
            .iter()
            .filter_map(|content| match content {
                AssistantContent::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect();
        let reasoning = response.reasoning();
        Self {
            text,
            reasoning: (!reasoning.is_empty()).then_some(reasoning),
            usage: Usage {
                input: response.usage.input_tokens,
                output: response.usage.output_tokens,
                total: response.usage.total_tokens,
                cached_input: response.usage.cached_input_tokens,
                reasoning: response.usage.reasoning_tokens,
            },
        }
    }
}

/// Why an ask failed, in the closed set §4's retry rule reads. The name says
/// which kind of failure it was; [`Failure::retryable`] says whether the wire
/// expects another attempt to differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing was sent: the wire needs a credential this ask does not carry.
    Unconfigured,
    /// The reply outlived its deadline.
    Timeout,
    /// The provider asked for fewer requests in flight.
    RateLimited,
    /// The provider faulted on its own side.
    ServerFault,
    /// The reply never arrived, or stopped before the provider ended it.
    Transport,
    /// The request itself was wrong, so resending it changes nothing.
    Client,
    /// The provider declined in words of its own.
    Refused,
    /// The caller stopped the ask.
    Cancelled,
    /// What arrived could not be read as the dialect's own reply.
    Broken,
}

impl Verdict {
    /// One phrase naming this verdict, short enough to show where a list would
    /// have gone and carrying no reply's own words.
    fn phrase(self) -> &'static str {
        match self {
            Verdict::Unconfigured => "nothing was sent: the wire needs a credential",
            Verdict::Timeout => "the reply outlived its deadline",
            Verdict::RateLimited => "it asked for fewer requests in flight",
            Verdict::ServerFault => "it faulted on its own side",
            Verdict::Transport => "the reply never arrived",
            Verdict::Client => "it does not offer a listing here",
            Verdict::Refused => "it declined in words of its own",
            Verdict::Cancelled => "the asking was cancelled",
            Verdict::Broken => "what arrived was not a listing",
        }
    }
}

/// Where a wire's credential comes from, as §11 has the user set it: the one
/// wording a failure and a listing's hint both print.
fn credential_source(wire: Wire) -> &'static str {
    match wire {
        Wire::Anthropic => "set ANTHROPIC_API_KEY",
        Wire::Openai => "set OPENAI_API_KEY",
        Wire::Copilot => "run `sindri auth login copilot`",
    }
}

/// One failure, carrying as much as a caller needs to print it and to decide
/// whether a second attempt is allowed. The policy itself is #13's; this is
/// only the evidence it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// What kind of failure the wire classified.
    pub verdict: Verdict,
    /// Whether the wire says another attempt could succeed.
    pub retryable: bool,
    /// Whether text or reasoning had already streamed when this failed. §4
    /// forbids retrying such a reply however retryable its verdict.
    pub streamed: bool,
    /// One line, safe to print: a `Redacted`, so a wire's refusal cannot carry
    /// a key back into a diagnostic, a verbose line or a record.
    pub message: Redacted,
}

impl Failure {
    /// A failure that never reached a wire, naming the wire and where its
    /// credential comes from (§11).
    pub fn unconfigured(wire: Wire) -> Self {
        Self {
            verdict: Verdict::Unconfigured,
            retryable: false,
            streamed: false,
            message: SecretSet::default().redact(format!(
                "{} needs a credential: {}",
                wire.word(),
                credential_source(wire)
            )),
        }
    }

    /// The failure `error` is, given whether anything had streamed by then and
    /// the secrets to scrub out of what it says.
    fn from_wire(error: ProviderError, streamed: bool, secrets: &SecretSet) -> Self {
        Self {
            verdict: verdict_of(&error),
            retryable: error.is_retryable(),
            streamed,
            message: secrets.redact(error.to_string()),
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message.to_string())
    }
}

/// Which [`Verdict`] a wire's error is. The classification is the wire's own
/// ([`ProviderError::is_retryable`] answers the retry half); the status only
/// separates a rate limit from a server fault and from a client error.
fn verdict_of(error: &ProviderError) -> Verdict {
    // A reply cut short or dropped mid-stream is a transport's failure, not the
    // dialect's, even though the library reports both as a response failure.
    match error {
        ProviderError::Truncated => return Verdict::Transport,
        ProviderError::Stalled(_) => return Verdict::Timeout,
        _ => {}
    }
    let report = error.report();
    match report.kind {
        ErrorKind::Timeout => Verdict::Timeout,
        ErrorKind::Cancelled => Verdict::Cancelled,
        ErrorKind::Http => Verdict::Transport,
        ErrorKind::ProviderResponse => match report.http_status {
            Some(429) => Verdict::RateLimited,
            Some(status) if (500..600).contains(&status) => Verdict::ServerFault,
            _ => Verdict::Client,
        },
        ErrorKind::Provider => Verdict::Refused,
        _ => Verdict::Broken,
    }
}

/// One way to reach every wire: the seam the rest of core asks through, holding
/// a transport and the wait between attempts, so a fake transport replaces the
/// network and a fake backoff replaces the clock.
#[derive(Clone)]
pub struct Client {
    /// How a request's bytes travel.
    transport: DynHttpClient,
    /// How long to wait before each attempt (§4), injectable for tests.
    backoff: Arc<dyn Backoff + Send + Sync>,
    /// The secrets a wire's own words are scrubbed of before they are kept.
    secrets: SecretSet,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("transport", &self.transport)
            .field("secrets", &self.secrets.len())
            .finish_non_exhaustive()
    }
}

impl Client {
    /// A client over `transport`, whatever sends it: the real network, or a
    /// fixture replaying recorded replies.
    ///
    /// `secrets` is not optional, and that is the point: a client cannot be
    /// built without deciding what a wire's own words are scrubbed of, so the
    /// choice to know nothing is written down as `SecretSet::default()` at the
    /// one call site that made it rather than being the default everywhere
    /// (ADR-0011).
    pub fn over(transport: impl HttpClientExt + Clone + 'static, secrets: SecretSet) -> Self {
        Self {
            transport: DynHttpClient::new(transport),
            backoff: Arc::new(Sleep),
            secrets,
        }
    }

    /// Wait through `backoff` between attempts, so a test proves the delays of
    /// §4's 200 ms doubling without spending them.
    pub fn waits(mut self, backoff: impl Backoff + Send + Sync + 'static) -> Self {
        self.backoff = Arc::new(backoff);
        self
    }

    /// A client that reaches the dialects' own endpoints, sending `headers` on
    /// every request with `{session}` expanded to `session`, scrubbing
    /// `secrets` out of whatever a failed ask says.
    pub fn http(
        headers: Vec<(String, String)>,
        session: impl Into<String>,
        secrets: SecretSet,
    ) -> Self {
        Self::over(
            Http::new(headers)
                .session(session)
                .over(rig_reqwest::ReqwestClient::default()),
            secrets,
        )
    }

    /// Ask `ask`, handing every fragment to `deltas` as it lands, and fold the
    /// finished reply into an [`Answer`] or a [`Failure`]. A failure §4 allows
    /// another attempt for is tried again, as [`retry`] decides.
    ///
    /// The call blocks until the reply ends: the loop above owns turns, budgets
    /// and cancellation (#17), so a single ask is one bounded piece of work and
    /// needs no runtime of its own.
    pub fn ask(&self, ask: &Ask, deltas: &mut dyn Deltas) -> Result<Answer, Failure> {
        let request = ask.request();
        let credential = ask.credential.expose();
        match ask.wire {
            Wire::Anthropic => {
                let Some(key) = credential else {
                    return Err(Failure::unconfigured(Wire::Anthropic));
                };
                let mut endpoint = AnthropicConfig::new(key);
                if let Some(base_url) = &ask.base_url {
                    endpoint = endpoint.with_base_url(base_url);
                }
                let model = endpoint
                    .connect(self.transport.clone())
                    .completion(ask.model.clone());
                self.drive(model, request, deltas)
            }
            Wire::Openai => {
                let Some(key) = credential else {
                    return Err(Failure::unconfigured(Wire::Openai));
                };
                let mut endpoint = OpenAIConfig::new(key);
                if let Some(base_url) = &ask.base_url {
                    endpoint = endpoint.with_base_url(base_url);
                }
                let model = endpoint
                    .connect(self.transport.clone())
                    .completion(ask.model.clone());
                self.drive(model, request, deltas)
            }
            Wire::Copilot => {
                let Some(key) = credential else {
                    return Err(Failure::unconfigured(Wire::Copilot));
                };
                let mut endpoint =
                    OpenAIConfig::with_key(&rig_core::providers::copilot::DIALECT, key);
                if let Some(base_url) = &ask.base_url {
                    endpoint = endpoint.with_base_url(base_url);
                }
                let model = endpoint
                    .connect(self.transport.clone())
                    .completion(ask.model.clone());
                self.drive(model, request, deltas)
            }
        }
    }

    /// One wire bound to one transport, tried as §4 allows: the one place a
    /// request leaves the process, and so the one place it is sent again.
    fn drive<W>(
        &self,
        model: Model<W, DynHttpClient>,
        request: CompletionRequest,
        deltas: &mut dyn Deltas,
    ) -> Result<Answer, Failure>
    where
        W: RigWire<Op = rig_core::operation::Completion, Payload = Encoded, Frame = WireFrame>,
    {
        until_final(self.backoff.as_ref(), || {
            self.attempt(&model, &request, deltas)
        })
    }

    /// One attempt: the reply streamed to its end and folded, or the failure it
    /// was and whether anything had streamed by then.
    fn attempt<W>(
        &self,
        model: &Model<W, DynHttpClient>,
        request: &CompletionRequest,
        deltas: &mut dyn Deltas,
    ) -> Result<Answer, Failure>
    where
        W: RigWire<Op = rig_core::operation::Completion, Payload = Encoded, Frame = WireFrame>,
    {
        futures::executor::block_on(async move {
            let mut streaming = model
                .stream(request.clone())
                .map_err(|error| Failure::from_wire(error, false, &self.secrets))?;
            let mut streamed = false;
            while let Some(item) = streaming.next().await {
                match item {
                    Ok(Item::Event(StreamEvent::Text { text, .. })) => {
                        streamed |= !text.is_empty();
                        deltas.text(&text);
                    }
                    Ok(Item::Event(StreamEvent::Reasoning { text, .. })) => {
                        streamed |= !text.is_empty();
                        deltas.reasoning(&text);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        return Err(Failure::from_wire(error, streamed, &self.secrets));
                    }
                }
            }
            let finished = streaming
                .finish()
                .await
                .map_err(|error| Failure::from_wire(error, streamed, &self.secrets))?;
            Ok(Answer::from(&finished))
        })
    }
}
