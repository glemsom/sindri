//! An ask goes out in Sindri's own words and an [`Answer`] or a [`Failure`] comes back,
//! never a client library's own error type: nothing above this module knows which client
//! library carried it, how a reply was framed, or which dialect spoke.

pub mod copilot;
pub mod retry;
pub mod route;
pub mod served;
pub mod transport;

use std::fmt;
use std::sync::Arc;

use futures::StreamExt as _;
use rig_core::Model;
use rig_core::completion::message::{AssistantContent, Message};
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
pub use self::route::Route;
use self::transport::Http;
use crate::redact::{MASK, Redacted, SecretSet};
use crate::wire::Wire;

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Credential(Option<String>);

impl Credential {
    pub fn new(key: impl Into<String>) -> Self {
        Self(Some(key.into()))
    }

    pub fn none() -> Self {
        Self(None)
    }

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

/// A prompt beside the answer it got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Earlier {
    pub prompt: String,
    pub answer: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    wire: Wire,
    model: String,
    base_url: Option<String>,
    credential: Credential,
    preamble: Option<String>,
    earlier: Vec<Earlier>,
    prompt: String,
    thinking: bool,
    route: Option<Route>,
}

impl Ask {
    pub fn new(wire: Wire, model: impl Into<String>) -> Self {
        Self {
            wire,
            model: model.into(),
            base_url: None,
            credential: Credential::none(),
            preamble: None,
            earlier: Vec::new(),
            prompt: String::new(),
            thinking: false,
            route: None,
        }
    }

    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    pub fn preamble(mut self, preamble: impl Into<String>) -> Self {
        self.preamble = Some(preamble.into());
        self
    }

    pub fn earlier(mut self, earlier: impl IntoIterator<Item = Earlier>) -> Self {
        self.earlier = earlier.into_iter().collect();
        self
    }

    pub fn credential(mut self, credential: Credential) -> Self {
        self.credential = credential;
        self
    }

    pub fn key(self, key: impl Into<String>) -> Self {
        self.credential(Credential::new(key))
    }

    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Whether the model reasons before it answers. `false` asks for nothing rather than
    /// forbidding thought: a model that cannot stop thinking must still answer.
    pub fn thinking(mut self, thinking: bool) -> Self {
        self.thinking = thinking;
        self
    }

    pub fn thinks(&self) -> bool {
        self.thinking
    }

    pub fn route(mut self, route: Route) -> Self {
        self.route = Some(route);
        self
    }

    fn endpoint(&self, key: &str) -> OpenAIConfig {
        let mut endpoint = match self.wire {
            Wire::Copilot => OpenAIConfig::with_key(&rig_core::providers::copilot::DIALECT, key),
            Wire::Anthropic | Wire::Openai => OpenAIConfig::new(key),
        };
        if let Some(base_url) = &self.base_url {
            endpoint = endpoint.with_base_url(base_url);
        }
        if let Some(route) = self.route {
            endpoint = endpoint.with_route(route.chosen());
        }
        endpoint
    }

    /// Every dialect refuses an empty message, so a prompt that got no answer travels alone: the
    /// words that were asked are not given up with the answer that never came.
    fn replayed(&self) -> Vec<Message> {
        let mut messages = Vec::new();
        for ended in &self.earlier {
            messages.push(Message::user(ended.prompt.clone()));
            if !ended.answer.trim().is_empty() {
                messages.push(Message::assistant(ended.answer.clone()));
            }
        }
        messages
    }

    fn request(&self) -> CompletionRequest {
        let mut request = CompletionRequest::new(self.prompt.clone()).messages(self.replayed());
        if let Some(preamble) = &self.preamble {
            request = request.preamble(preamble.clone());
        }
        let mut options = GenerationOptions::default();
        options.reasoning = self.thinking.then_some(Reasoning::Effort(Effort::High));
        request.options(options)
    }
}

pub trait Deltas {
    fn text(&mut self, fragment: &str);

    fn reasoning(&mut self, fragment: &str);
}

impl Deltas for () {
    fn text(&mut self, _fragment: &str) {}

    fn reasoning(&mut self, _fragment: &str) {}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub total: Option<u64>,
    pub cached_input: Option<u64>,
    pub reasoning: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    pub text: String,
    pub reasoning: Option<String>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Unconfigured,
    Timeout,
    RateLimited,
    ServerFault,
    Transport,
    Client,
    Refused,
    Cancelled,
    Broken,
}

impl Verdict {
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

fn credential_source(wire: Wire) -> String {
    match wire {
        Wire::Anthropic => "set ANTHROPIC_API_KEY".to_owned(),
        Wire::Openai => "set OPENAI_API_KEY".to_owned(),
        Wire::Copilot => format!("run `{}`", copilot::LOGIN),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub verdict: Verdict,
    pub retryable: bool,
    /// Whether text or reasoning had already streamed when this failed: such a reply
    /// is never retried, however retryable its verdict.
    pub streamed: bool,
    pub message: Redacted,
}

impl Failure {
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

    pub fn signed_out(reason: copilot::SignIn) -> Self {
        Self {
            verdict: Verdict::Unconfigured,
            retryable: false,
            streamed: false,
            message: reason.message(),
        }
    }

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

fn verdict_of(error: &ProviderError) -> Verdict {
    // A reply cut short mid-stream is the transport's failure and not the dialect's: rig
    // calls it a response failure, and calls a reply that stalled a timeout.
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

#[derive(Clone)]
pub struct Client {
    transport: DynHttpClient,
    backoff: Arc<dyn Backoff + Send + Sync>,
    secrets: SecretSet,
    store: Option<copilot::Store>,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("transport", &self.transport)
            .field("secrets", &self.secrets.len())
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

impl Client {
    pub fn over(transport: impl HttpClientExt + Clone + 'static, secrets: SecretSet) -> Self {
        Self {
            transport: DynHttpClient::new(transport),
            backoff: Arc::new(Sleep),
            secrets,
            store: None,
        }
    }

    pub fn store(mut self, store: copilot::Store) -> Self {
        self.store = Some(store);
        self
    }

    pub fn waits(mut self, backoff: impl Backoff + Send + Sync + 'static) -> Self {
        self.backoff = Arc::new(backoff);
        self
    }

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

    pub fn ask(&self, ask: &Ask, deltas: &mut dyn Deltas) -> Result<Answer, Failure> {
        let request = ask.request();
        // A session resolved here joins the set this ask is scrubbed with, so a wire
        // that echoes it back is echoed back masked.
        let mut secrets = self.secrets.clone();
        let credential = match ask.credential.expose() {
            Some(key) => Some(key.to_owned()),
            None => self.session(ask.wire, &mut secrets)?,
        };
        let credential = credential.as_deref();
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
                self.drive(model, request, deltas, &secrets)
            }
            Wire::Openai | Wire::Copilot => {
                let Some(key) = credential else {
                    return Err(Failure::unconfigured(ask.wire));
                };
                let model = ask
                    .endpoint(key)
                    .connect(self.transport.clone())
                    .completion(ask.model.clone());
                self.drive(model, request, deltas, &secrets)
            }
        }
    }

    fn session(&self, wire: Wire, secrets: &mut SecretSet) -> Result<Option<String>, Failure> {
        let (Wire::Copilot, Some(store)) = (wire, self.store.as_ref()) else {
            return Ok(None);
        };
        let session = store.session().map_err(Failure::signed_out)?;
        Ok(session.expose().map(|token| {
            secrets.add(token);
            token.to_owned()
        }))
    }

    fn drive<W>(
        &self,
        model: Model<W, DynHttpClient>,
        request: CompletionRequest,
        deltas: &mut dyn Deltas,
        secrets: &SecretSet,
    ) -> Result<Answer, Failure>
    where
        W: RigWire<Op = rig_core::operation::Completion, Payload = Encoded, Frame = WireFrame>,
    {
        until_final(self.backoff.as_ref(), || {
            self.attempt(&model, &request, deltas, secrets)
        })
    }

    fn attempt<W>(
        &self,
        model: &Model<W, DynHttpClient>,
        request: &CompletionRequest,
        deltas: &mut dyn Deltas,
        secrets: &SecretSet,
    ) -> Result<Answer, Failure>
    where
        W: RigWire<Op = rig_core::operation::Completion, Payload = Encoded, Frame = WireFrame>,
    {
        futures::executor::block_on(async move {
            let mut streaming = model
                .stream(request.clone())
                .map_err(|error| Failure::from_wire(error, false, secrets))?;
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
                        return Err(Failure::from_wire(error, streamed, secrets));
                    }
                }
            }
            let finished = streaming
                .finish()
                .await
                .map_err(|error| Failure::from_wire(error, streamed, secrets))?;
            Ok(Answer::from(&finished))
        })
    }
}
