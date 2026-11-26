pub mod copilot;
pub mod endpoint;
pub mod retry;
pub mod route;
pub mod served;
pub mod transport;

use std::fmt;
use std::sync::Arc;

use futures::StreamExt as _;
use rig_core::Model;
use rig_core::completion::ToolDefinition;
use rig_core::completion::message::{
    AssistantContent, AssistantMessage, CallId, Message, ToolCall, ToolFunction, ToolName,
    ToolResult, ToolResultContent,
};
use rig_core::completion::{
    CompletionRequest, CompletionResponse, Effort, GenerationOptions, Reasoning,
};
use rig_core::error::{ErrorKind, ProviderError};
use rig_core::http_client::{DynHttpClient, HttpClientExt};
use rig_core::streaming::{Item, StreamEvent};
use rig_core::wire::{Encoded, Wire as RigWire, WireFrame};

use self::endpoint::{Endpoint, Serving};
use self::retry::{Backoff, Sleep, until_final};
pub use self::route::Route;
use self::transport::Http;
use crate::config;
use crate::redact::{MASK, Redacted, SecretSet};
use crate::tools::Tool;
use crate::wire::Wire;

pub use crate::turn::{Call, Turn, Usage};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assistant(Option<AssistantMessage>);

impl Eq for Assistant {}

#[derive(Clone, Debug, PartialEq)]
pub struct Responses(AssistantMessage);

impl Eq for Responses {}


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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    endpoint: Endpoint,
    model: String,
    credential: Credential,
    preamble: Option<String>,
    earlier: Vec<Turn>,
    turns: Vec<Turn>,
    tools: Vec<ToolDefinition>,
    prompt: String,
    thinking: bool,
}

impl Ask {
    pub fn new(wire: Wire, model: impl Into<String>) -> Self {
        Self::of(Endpoint::own(wire), model)
    }

    pub fn of(endpoint: Endpoint, model: impl Into<String>) -> Self {
        Self {
            endpoint,
            model: model.into(),
            credential: Credential::none(),
            preamble: None,
            earlier: Vec::new(),
            turns: Vec::new(),
            tools: Vec::new(),
            prompt: String::new(),
            thinking: false,
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

    pub fn earlier(mut self, earlier: impl IntoIterator<Item = Turn>) -> Self {
        self.earlier = earlier.into_iter().collect();
        self
    }

    pub fn tools(mut self, tools: &[Tool]) -> Self {
        self.tools = tools
            .iter()
            .filter_map(|tool| {
                ToolName::new(tool.name()).ok().map(|name| {
                    ToolDefinition::new(name, tool.description(), tool.parameters().clone())
                })
            })
            .collect();
        self
    }

    pub fn turns(mut self, turns: impl IntoIterator<Item = Turn>) -> Self {
        self.turns = turns.into_iter().collect();
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
        self.endpoint = self.endpoint.based(base_url);
        self
    }

    pub fn thinking(mut self, thinking: bool) -> Self {
        self.thinking = thinking;
        self
    }

    pub fn thinks(&self) -> bool {
        self.thinking
    }

    pub fn route(mut self, route: Route) -> Self {
        self.endpoint = self.endpoint.routed(Some(route));
        self
    }

    fn replayed(&self) -> Vec<Message> {
        let mut messages = Vec::new();
        for ended in &self.earlier {
            if !ended.prompt.is_empty() {
                messages.push(Message::user(ended.prompt.clone()));
            }
            messages.extend(turn_messages(ended));
        }
        messages
    }

    fn request(&self) -> CompletionRequest {
        let mut conversation = self.replayed();
        conversation.push(Message::user(self.prompt.clone()));
        conversation.extend(self.turns.iter().flat_map(turn_messages));
        let last = conversation.pop().expect("the prompt was pushed");
        let mut request = CompletionRequest::new(last)
            .messages(conversation)
            .tools(self.tools.iter().cloned());
        if let Some(preamble) = &self.preamble {
            request = request.preamble(preamble.clone());
        }
        let mut options = GenerationOptions::default();
        options.reasoning = self.thinking.then_some(Reasoning::Effort(Effort::High));
        if self.endpoint.route_of(&self.model) == Some(Route::Responses) {
            use rig_core::completion::ProviderOptions;
            use rig_core::providers::openai::extension::{OpenAiExt, OpenAiOptions, OpenAiShared};
            request = request.provider_options(
                ProviderOptions::new()
                    .with::<OpenAiExt>(
                        &OpenAiOptions::default().shared(OpenAiShared::default().store(false)),
                    )
                    .expect("typed OpenAI options"),
            );
        }
        request.options(options)
    }
}

fn turn_messages(turn: &Turn) -> Vec<Message> {
    let mut content = Vec::new();
    if !turn.text.trim().is_empty() {
        content.push(AssistantContent::text(turn.text.clone()));
    }
    let mut results = Vec::new();
    for (index, call) in turn.calls.iter().enumerate() {
        let Ok(name) = ToolName::new(call.name.clone()) else {
            continue;
        };
        let id = CallId::from_wire(call.id.clone());
        content.push(AssistantContent::ToolCall(ToolCall::new(
            id.clone(),
            ToolFunction::parse(name.clone(), &call.arguments),
        )));
        let acted = turn.results.get(index);
        let text = acted.map_or_else(
            || {
                format!(
                    "not executed: the run stopped before this call; arguments: {}",
                    call.arguments
                )
            },
            |acted| acted.text.clone(),
        );
        results.push(ToolResult {
            call: id,
            name,
            content: vec![ToolResultContent::text(text)],
            is_error: acted.is_none_or(|acted| !acted.exit.is_ok()),
        });
    }
    if content.is_empty() && turn.assistant.0.is_none() && turn.responses.is_none() {

        return Vec::new();
    }
    let assistant = turn
        .responses
        .as_ref()
        .map(|message| message.0.clone())
        .or_else(|| turn.assistant.0.clone())
        .unwrap_or_else(|| AssistantMessage::new(content));
    let mut messages = vec![Message::Assistant(assistant)];

    if !results.is_empty() {
        messages.push(Message::tool_results(results));
    }
    messages
}

pub trait Deltas {
    fn text(&mut self, fragment: &str);

    fn reasoning(&mut self, fragment: &str);
}

impl Deltas for () {
    fn text(&mut self, _fragment: &str) {}

    fn reasoning(&mut self, _fragment: &str) {}
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    pub assistant: Assistant,
    pub text: String,
    pub reasoning: Option<String>,
    pub responses: Option<Responses>,
    pub calls: Vec<Call>,
    pub usage: Usage,
}

impl From<&CompletionResponse> for Answer {
    fn from(response: &CompletionResponse) -> Self {
        let mut text = String::new();
        let mut calls = Vec::new();
        for content in &response.choice {
            match content {
                AssistantContent::Text(fragment) => text.push_str(&fragment.text),
                AssistantContent::ToolCall(call) => calls.push(Call {
                    id: call.id.wire().into_owned(),
                    name: call.function.name.to_string(),
                    arguments: call.function.invalid_arguments.clone().unwrap_or_else(|| {
                        serde_json::to_string(&call.function.arguments).unwrap_or_default()
                    }),
                }),
                _ => {}
            }
        }
        let reasoning = response.reasoning();
        Self {
            assistant: Assistant(response.message().and_then(|message| match message {
                Message::Assistant(assistant) => Some(assistant),
                _ => None,
            })),
            text,
            reasoning: (!reasoning.is_empty()).then_some(reasoning),
            responses: if response.origin.api.as_str() == "openai.responses" {
                response.message().and_then(|message| match message {
                    Message::Assistant(assistant) => Some(Responses(assistant)),
                    _ => None,
                })
            } else {
                None
            },
            calls,
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
    match wire.credential_variable() {
        Some(variable) => format!("set {variable}"),
        None => format!("run `{}`", copilot::LOGIN),
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
    signing: Option<copilot::Signing>,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("transport", &self.transport)
            .field("secrets", &self.secrets.len())
            .field("signing", &self.signing)
            .finish_non_exhaustive()
    }
}

impl Client {
    pub fn over(transport: impl HttpClientExt + Clone + 'static, secrets: SecretSet) -> Self {
        Self {
            transport: DynHttpClient::new(transport),
            backoff: Arc::new(Sleep),
            secrets,
            signing: None,
        }
    }

    pub fn secrets(&self) -> &SecretSet {
        &self.secrets
    }

    pub fn sign_in(&self) -> Option<&copilot::Signing> {
        self.signing.as_ref()
    }

    pub fn signing(mut self, signing: Option<copilot::Signing>) -> Self {
        if let Some(copilot::Signing::Store(store)) = &signing {
            self.secrets = self.secrets.union(store.secrets());
        }
        self.signing = signing;
        self
    }

    pub fn waits(mut self, backoff: impl Backoff + Send + Sync + 'static) -> Self {
        self.backoff = Arc::new(backoff);
        self
    }

    pub fn for_endpoint(
        endpoint: &Endpoint,
        headers: Vec<(String, String)>,
        session: impl Into<String>,
        secrets: SecretSet,
        environment: &impl config::Environment,
    ) -> Self {
        Self::over(
            Http::new(headers)
                .session(session)
                .over(rig_reqwest::ReqwestClient::default()),
            secrets,
        )
        .signing(copilot::signing(endpoint.wire(), environment))
    }

    pub fn ask(&self, ask: &Ask, deltas: &mut dyn Deltas) -> Result<Answer, Failure> {
        let request = ask.request();
        // A session resolved here joins the set this ask is scrubbed with, so a wire
        // that echoes it back is echoed back masked.
        let mut secrets = self.secrets.clone();
        let credential = self.credential(&ask.credential, &mut secrets)?;
        let Some(key) = credential else {
            return Err(Failure::unconfigured(ask.endpoint.wire()));
        };
        let mut answer = self.carry(
            ask.endpoint.serving(&key),
            &ask.model,
            request,
            deltas,
            &secrets,
        )?;
        if ask.endpoint.wire() != Wire::Anthropic {
            answer.assistant.0 = None;
        }
        Ok(answer)
    }

    fn credential(
        &self,
        given: &Credential,
        secrets: &mut SecretSet,
    ) -> Result<Option<String>, Failure> {
        match given.expose() {
            Some(key) => Ok(Some(key.to_owned())),
            None => self.session(secrets),
        }
    }

    fn session(&self, secrets: &mut SecretSet) -> Result<Option<String>, Failure> {
        let session = match &self.signing {
            None => return Ok(None),
            Some(copilot::Signing::Store(store)) => store.session().map_err(Failure::signed_out)?,
            Some(copilot::Signing::Unreadable(reason)) => {
                return Err(Failure::signed_out(reason.clone()));
            }
        };
        Ok(session.expose().map(|token| {
            secrets.add(token);
            token.to_owned()
        }))
    }

    fn carry(
        &self,
        serving: Serving,
        model: &str,
        request: CompletionRequest,
        deltas: &mut dyn Deltas,
        secrets: &SecretSet,
    ) -> Result<Answer, Failure> {
        let transport = self.transport.clone();
        match serving {
            Serving::Messages(config) => self.drive(
                config.connect(transport).completion(model),
                request,
                deltas,
                secrets,
            ),
            Serving::OpenAi(config) => self.drive(
                config.connect(transport).completion(model),
                request,
                deltas,
                secrets,
            ),
        }
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
            if finished.stop().is_failure() {
                return Err(Failure {
                    verdict: Verdict::Broken,
                    retryable: false,
                    streamed,
                    message: secrets.redact("the wire did not complete its response items"),
                });
            }
            Ok(Answer::from(&finished))
        })
    }
}
