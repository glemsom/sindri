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
pub struct Assistant(Option<AssistantMessage>, Option<ReplayTarget>);

impl Eq for Assistant {}

#[derive(Clone, Debug, PartialEq)]
pub struct Responses(
    AssistantMessage,
    Option<ReplayTarget>,
    Option<String>,
    Option<serde_json::Value>,
);

impl Eq for Responses {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayTarget {
    endpoint: Endpoint,
    route: Option<Route>,
    model: String,
    storage: StoragePolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoragePolicy {
    Inherited,
    Stored,
    Ephemeral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayCapabilities {
    pub continuation: bool,
    pub compaction: bool,
}

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
    continuation: bool,
    storage: Option<bool>,
    verified_continuation: bool,
    native_compaction: bool,
    verified_compaction: bool,
    capacity: Option<u64>,
    output_headroom: u64,
    tool_result_headroom: u64,
    compaction_usage: Usage,
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
            continuation: false,
            storage: None,
            verified_continuation: false,
            native_compaction: false,
            verified_compaction: false,
            capacity: None,
            output_headroom: 4096,
            tool_result_headroom: 4096,
            compaction_usage: Usage::default(),
        }
    }

    pub fn continuation(mut self, enabled: bool, storage: Option<bool>, verified: bool) -> Self {
        self.continuation = enabled;
        self.storage = storage;
        self.verified_continuation = verified;
        self
    }

    pub fn native_compaction(mut self, enabled: bool, verified: bool) -> Self {
        self.native_compaction = enabled;
        self.verified_compaction = verified;
        self
    }

    fn handle(&self) -> Option<&str> {
        if !self.continuation {
            return None;
        }
        let target = self.replay_target();
        let mut handle = None;
        for turn in self.earlier.iter().chain(&self.turns) {
            let items = turn.responses.as_ref()?;
            if items.1.as_ref() != Some(&target) {
                return None;
            }
            handle = Some(items.2.as_deref()?);
        }
        handle
    }

    pub fn capacity(mut self, window: Option<u64>, output: u64, tool_results: u64) -> Self {
        self.capacity = window;
        self.output_headroom = output;
        self.tool_result_headroom = tool_results;
        self
    }

    fn capacity_bytes(&self) -> Result<u64, Failure> {
        let mut replay = self.clone();
        replay.continuation = false;
        serde_json::to_vec(&replay.request())
            .map(|bytes| bytes.len() as u64)
            .map_err(|_| self.capacity_failure())
    }

    pub fn guard_capacity(&self) -> Result<(), Failure> {
        let Some(window) = self.capacity else {
            return Ok(());
        };
        // Serialized bytes deliberately overcount tokenized text and include protocol overhead.
        let bytes = self.capacity_bytes()?;
        let required = bytes
            .saturating_add(self.output_headroom)
            .saturating_add(self.tool_result_headroom);
        if required > window {
            return Err(self.capacity_failure());
        }
        Ok(())
    }

    fn capacity_failure(&self) -> Failure {
        Failure {
            verdict: Verdict::Refused, retryable: false, streamed: false,
            message: SecretSet::default().redact("Insufficient configured context capacity: increase context_window, reduce output_headroom/tool_result_headroom, or start a new conversation. Retained material was not truncated."),
        }
    }

    pub fn retained(&self) -> &[Turn] {
        &self.earlier
    }

    pub fn compaction_usage(&self) -> Usage {
        self.compaction_usage
    }

    pub fn compact(&self, client: &Client) -> Result<Option<Self>, Failure> {
        if self.native_compaction && (!self.replay_capabilities().compaction || self.continuation) {
            return Err(self.native_failure("Native compaction requires native_compaction_verified=true for the selected endpoint/model. Standalone compaction cannot prune a Responses continuation chain; disable continuation or native compaction."));
        }
        if self.continuation {
            return Ok(None);
        }
        let Some(window) = self.capacity else {
            return Ok(None);
        };
        let bytes = self.capacity_bytes()?;
        if bytes
            .saturating_add(self.output_headroom)
            .saturating_add(self.tool_result_headroom)
            < window.saturating_mul(3) / 4
        {
            return Ok(None);
        }
        if self.native_compaction {
            return self.compact_native(client);
        }
        let (older, protected) = self.compaction_partition(false);
        if older.is_empty() {
            return Ok(None);
        }
        let mut summary = self.clone();
        summary.earlier = self.transition(&older).0;
        summary.turns.clear();
        summary.tools.clear();
        summary.thinking = false;
        summary.prompt = "Summarize this conversation for continuation. Preserve task objectives, constraints, exact important identifiers and evidence, completed side effects (never repeat them), unresolved errors, recent work and next actions. Distinguish plans from actual outcomes. Return only the faithful summary, not a final answer.".into();
        summary.guard_capacity()?;
        let answer = client.ask(&summary, &mut ())?;
        if answer.text.trim().is_empty() || !answer.calls.is_empty() {
            return Err(self.capacity_failure());
        }
        let mut compacted = self.clone();
        compacted.compaction_usage = answer.usage;
        compacted.compaction_usage.client_compactions = Some(1);
        compacted.earlier = vec![Turn::answered(
            0,
            "Client-managed conversation summary",
            answer.text,
        )];
        compacted.earlier.extend(protected);
        Ok(Some(compacted))
    }

    fn native_failure(&self, message: &str) -> Failure {
        Failure {
            verdict: Verdict::Refused,
            retryable: false,
            streamed: false,
            message: SecretSet::default().redact(message),
        }
    }

    fn serving(&self, key: &str, native_beta: bool) -> Serving {
        match self.endpoint.serving(key) {
            Serving::Messages(config) if native_beta => {
                Serving::Messages(config.with_beta("compact-2026-09-04"))
            }
            serving => serving,
        }
    }

    fn encoded(&self, key: &str) -> Result<Encoded, Failure> {
        let request = self.request();
        let encoded = match self.endpoint.serving(key) {
            Serving::OpenAi(config) => {
                rig_core::providers::openai::wire::OpenAiWire::new(config, &self.model)
                    .encode(request, rig_core::wire::Mode::Unary)
            }
            Serving::Messages(config) => config
                .completion(&self.model)
                .encode(request, rig_core::wire::Mode::Unary),
        };
        encoded.map_err(|_| self.native_failure("Cannot encode native compaction input"))
    }

    fn compaction_partition(&self, native: bool) -> (Vec<Turn>, Vec<Turn>) {
        let boundary = self.earlier.len().saturating_sub(2);
        let mut input = Vec::new();
        let mut protected = Vec::new();
        for (index, turn) in self.earlier.iter().enumerate() {
            let incomplete = turn.calls.len() != turn.results.len();
            let instructions = turn.calls.iter().any(|call| call.name == "skill")
                || (turn.turn == 0
                    && turn.prompt.is_empty()
                    && (!native || turn.responses.is_none()));
            if index >= boundary || incomplete || instructions {
                protected.push(turn.clone());
            }
            if index < boundary && !incomplete {
                input.push(turn.clone());
            }
        }
        (input, protected)
    }

    fn compact_native(&self, client: &Client) -> Result<Option<Self>, Failure> {
        if self.earlier.is_empty() || !self.turns.is_empty() {
            return Ok(None);
        }
        if self.endpoint.wire() == Wire::Anthropic
            && (self.thinking || self.earlier.iter().any(|turn| turn.reasoning.is_some()))
        {
            return Err(self.native_failure("Anthropic native compaction with preserved thinking requires a verified prefix contract not implemented here; disable native compaction to use client summarization."));
        }
        let mut secrets = client.secrets.clone();
        let key = client
            .credential(&self.credential, &mut secrets)?
            .ok_or_else(|| Failure::unconfigured(self.endpoint.wire()))?;
        let mut input = self.clone();
        let (older, protected) = self.compaction_partition(true);
        input.earlier = older;
        if input.earlier.is_empty() {
            return Ok(None);
        }
        input.prompt = "Native compaction boundary".into();
        input.guard_capacity()?;
        let encoded = input.encoded(&key)?;
        let rig_core::wire::Body::Bytes(bytes) = encoded.request.body() else {
            return Err(self.native_failure("Unsupported compaction body"));
        };
        let mut body: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|_| self.native_failure("Invalid compaction input"))?;
        let anthropic = self.endpoint.wire() == Wire::Anthropic;
        let field = if anthropic { "messages" } else { "input" };
        body[field]
            .as_array_mut()
            .ok_or_else(|| self.native_failure("Missing compaction input"))?
            .pop();
        body.as_object_mut().unwrap().remove("stream");

        let mut request = encoded.request.map(|_| Vec::<u8>::new());
        if anthropic {
            body["compaction"] = serde_json::json!({"type":"summarize"});

            let beta = request
                .headers()
                .get("anthropic-beta")
                .and_then(|value| value.to_str().ok())
                .map_or_else(
                    || "compact-2026-09-04".to_owned(),
                    |existing| format!("{existing},compact-2026-09-04"),
                );
            request.headers_mut().insert(
                "anthropic-beta",
                http::HeaderValue::from_str(&beta)
                    .map_err(|_| self.native_failure("Invalid compaction beta header"))?,
            );
        } else {
            body.as_object_mut()
                .unwrap()
                .retain(|key, _| matches!(key.as_str(), "model" | "input" | "instructions"));
            *request.uri_mut() = format!("{}/compact", request.uri())
                .parse()
                .map_err(|_| self.native_failure("Invalid compaction endpoint"))?;
        }
        *request.body_mut() = serde_json::to_vec(&body).unwrap();
        let started = std::time::Instant::now();
        let returned = futures::executor::block_on(async {
            let response = client
                .transport
                .send::<Vec<u8>, Vec<u8>>(request)
                .await
                .map_err(|_| {
                    self.native_failure(
                        "Native compaction transport failed; original material retained",
                    )
                })?;
            if !response.status().is_success() {
                return Err(
                    self.native_failure("Native compaction rejected; original material retained")
                );
            }
            let bytes = response.into_body().await.map_err(|_| {
                self.native_failure("Native compaction response failed; original material retained")
            })?;
            serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|_| {
                self.native_failure(
                    "Invalid native compaction response; original material retained",
                )
            })
        })?;
        let window = if anthropic {
            if returned["stop_reason"] != "compaction" {
                return Err(self.native_failure(
                    "Native compaction did not complete; original material retained",
                ));
            }
            serde_json::json!([{"role":"assistant", "content":returned["content"]}])
        } else {
            returned["output"].clone()
        };
        let items = window
            .as_array()
            .filter(|items| !items.is_empty())
            .ok_or_else(|| {
                self.native_failure("Empty native compaction window; original material retained")
            })?;
        if !items.iter().any(|item| {
            item["type"] == "compaction"
                || item["content"]
                    .as_array()
                    .is_some_and(|blocks| blocks.iter().any(|block| block["type"] == "compaction"))
        }) {
            return Err(
                self.native_failure("Missing native compaction item; original material retained")
            );
        }
        let mut compacted = self.clone();
        let usage = &returned["usage"];
        compacted.compaction_usage = Usage {
            input: usage["input_tokens"].as_u64(),
            output: usage["output_tokens"].as_u64(),
            total: usage["total_tokens"].as_u64(),
            cached_input: usage["cache_read_input_tokens"]
                .as_u64()
                .or_else(|| usage["input_tokens_details"]["cached_tokens"].as_u64()),
            cache_read: usage["cache_read_input_tokens"]
                .as_u64()
                .or_else(|| usage["input_tokens_details"]["cached_tokens"].as_u64()),
            cache_write: usage["cache_creation_input_tokens"].as_u64(),
            reasoning: usage["output_tokens_details"]["reasoning_tokens"].as_u64(),
            request_ms: Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)),
            native_compactions: Some(1),
            ..Usage::default()
        };
        let mut boundary = Turn::answered(
            0,
            "",
            "Native compacted conversation: opaque context requires the original endpoint and model.",
        );
        boundary.responses = Some(Responses(
            AssistantMessage::new(Vec::new()),
            Some(self.replay_target()),
            None,
            Some(window),
        ));
        compacted.earlier = vec![boundary];
        compacted.earlier.extend(protected);
        Ok(Some(compacted))
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

    pub fn replay_target(&self) -> ReplayTarget {
        ReplayTarget {
            endpoint: self.endpoint.clone(),
            route: self.endpoint.route_of(&self.model),
            model: self.model.clone(),
            storage: match self.storage {
                None => StoragePolicy::Inherited,
                Some(true) => StoragePolicy::Stored,
                Some(false) => StoragePolicy::Ephemeral,
            },
        }
    }

    pub fn replay_capabilities(&self) -> ReplayCapabilities {
        ReplayCapabilities {
            continuation: self.endpoint.wire() == Wire::Openai
                && self.endpoint.route_of(&self.model) == Some(Route::Responses)
                && (self.endpoint.base_url().is_none() || self.verified_continuation),
            compaction: self.verified_compaction
                && (self.endpoint.wire() == Wire::Anthropic
                    || (self.endpoint.wire() == Wire::Openai
                        && self.endpoint.route_of(&self.model) == Some(Route::Responses))),
        }
    }

    pub fn transition(&self, earlier: &[Turn]) -> (Vec<Turn>, bool) {
        let target = self.replay_target();
        let mut lost = false;
        let turns = earlier
            .iter()
            .cloned()
            .map(|mut turn| {
                let incompatible = turn
                    .assistant
                    .1
                    .as_ref()
                    .is_some_and(|origin| origin != &target)
                    || turn
                        .responses
                        .as_ref()
                        .and_then(|items| items.1.as_ref())
                        .is_some_and(|origin| origin != &target);
                if incompatible {
                    turn.assistant = Assistant::default();
                    turn.responses = None;
                    turn.reasoning = None;
                    lost = true;
                }
                turn
            })
            .collect();
        (turns, lost)
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
            let (compatible, _) = self.transition(std::slice::from_ref(ended));
            messages.extend(turn_messages(&compatible[0]));
        }
        messages
    }

    fn request(&self) -> CompletionRequest {
        let handle = self.handle();
        let mut conversation = if handle.is_some() {
            Vec::new()
        } else {
            self.replayed()
        };
        if handle.is_some() && !self.turns.is_empty() {
            conversation.extend(
                turn_messages(self.turns.last().unwrap())
                    .into_iter()
                    .filter(|message| matches!(message, Message::User { .. })),
            );
        } else {
            if handle.is_some()
                && let Some(turn) = self.earlier.last()
            {
                conversation.extend(
                    turn_messages(turn)
                        .into_iter()
                        .filter(|message| matches!(message, Message::User { .. })),
                );
            }
            conversation.push(Message::user(self.prompt.clone()));
            conversation.extend(self.turns.iter().flat_map(turn_messages));
        }
        let last = conversation.pop().expect("the prompt was pushed");
        let mut request = CompletionRequest::new(last)
            .messages(conversation)
            .tools(self.tools.iter().cloned());
        if let Some(preamble) = &self.preamble {
            request = request.preamble(preamble.clone());
        }
        let mut options = GenerationOptions::default();
        options.reasoning = self.thinking.then_some(Reasoning::Effort(Effort::High));
        if self.capacity.is_some() {
            request = request.max_tokens(self.output_headroom);
        }
        if self.endpoint.route_of(&self.model) == Some(Route::Responses) {
            let mut params = serde_json::Map::new();
            if let Some(store) = self.storage {
                params.insert("store".into(), store.into());
            }
            if let Some(handle) = handle {
                params.insert("previous_response_id".into(), handle.into());
            }
            if !params.is_empty() {
                request = request.additional_params(serde_json::Value::Object(params));
            }
        }
        if let Some(window) = self.earlier.iter().find_map(|turn| {
            turn.responses
                .as_ref()
                .and_then(|items| items.3.as_ref())
                .filter(|_| {
                    turn.responses.as_ref().unwrap().1.as_ref() == Some(&self.replay_target())
                })
        }) {
            let field = if self.endpoint.wire() == Wire::Anthropic {
                "messages"
            } else {
                "input"
            };
            let mut baseline = self.clone();
            baseline.earlier.retain(|turn| {
                !turn
                    .responses
                    .as_ref()
                    .is_some_and(|items| items.3.is_some())
            });
            if let Ok(encoded) = baseline.encoded("")
                && let rig_core::wire::Body::Bytes(bytes) = encoded.request.body()
                && let Ok(body) = serde_json::from_slice::<serde_json::Value>(bytes)
            {
                let mut items = window.as_array().unwrap().clone();
                items.extend(body[field].as_array().into_iter().flatten().cloned());
                let mut params = request
                    .additional_params
                    .clone()
                    .unwrap_or_else(|| serde_json::json!({}));
                params[field] = items.into();
                request = request.additional_params(params);
            }
        }
        request.options(options)
    }
}

fn turn_messages(turn: &Turn) -> Vec<Message> {
    if turn
        .responses
        .as_ref()
        .is_some_and(|items| items.3.is_some())
    {
        return Vec::new();
    }
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
            |acted| {
                if acted.exit.is_ok() {
                    acted.text.clone()
                } else {
                    format!(
                        "Tool outcome: failed ({})\n{}",
                        acted.exit.word(),
                        acted.text
                    )
                }
            },
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
    fn completed_text(&mut self, _text: &str) {}
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
            assistant: Assistant(
                response.message().and_then(|message| match message {
                    Message::Assistant(assistant) => Some(assistant),
                    _ => None,
                }),
                None,
            ),
            text,
            reasoning: (!reasoning.is_empty()).then_some(reasoning),
            responses: if response.origin.api.as_str() == "openai.responses" {
                response.message().and_then(|message| match message {
                    Message::Assistant(assistant) => Some(Responses(
                        assistant,
                        None,
                        response.response_id().map(str::to_owned),
                        None,
                    )),
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
                cache_read: response.usage.cached_input_tokens,
                cache_write: response.usage.cache_creation_input_tokens,
                ..Usage::default()
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
        if ask.continuation
            && (!ask.replay_capabilities().continuation || ask.storage != Some(true))
        {
            return Err(Failure { verdict: Verdict::Refused, retryable: false, streamed: false,
                message: self.secrets.redact("Responses continuation requires a verified OpenAI Responses endpoint and explicit responses_store=true. Custom endpoints require responses_continuation_verified=true; Copilot is unsupported.") });
        }
        ask.guard_capacity()?;
        let request = ask.request();
        let native_beta = ask.earlier.iter().any(|turn| {
            turn.responses.as_ref().is_some_and(|items| {
                items.3.is_some() && items.1.as_ref() == Some(&ask.replay_target())
            })
        }) && ask.endpoint.wire() == Wire::Anthropic;
        // A session resolved here joins the set this ask is scrubbed with, so a wire
        // that echoes it back is echoed back masked.
        let mut secrets = self.secrets.clone();
        let credential = self.credential(&ask.credential, &mut secrets)?;
        let Some(key) = credential else {
            return Err(Failure::unconfigured(ask.endpoint.wire()));
        };
        let started = std::time::Instant::now();
        let carried = self.carry(
            ask.serving(&key, native_beta),
            &ask.model,
            request,
            deltas,
            &secrets,
        );
        let mut recovered = false;
        let mut answer = match carried {
            Err(failure)
                if ask.handle().is_some()
                    && !failure.streamed
                    && failure.verdict == Verdict::Client
                    && (failure.message.to_string().contains("previous_response_id")
                        || failure
                            .message
                            .to_string()
                            .contains("previous_response_not_found")) =>
            {
                let mut replay = ask.clone();
                replay.continuation = false;
                replay.guard_capacity()?;
                recovered = true;
                self.carry(
                    ask.serving(&key, native_beta),
                    &ask.model,
                    replay.request(),
                    deltas,
                    &secrets,
                )?
            }
            other => other?,
        };
        answer.usage.request_ms =
            Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        let target = ask.replay_target();
        if answer.assistant.0.is_some() {
            answer.assistant.1 = Some(target.clone());
        }
        if let Some(items) = &mut answer.responses {
            items.1 = Some(target);
            if !ask.continuation || recovered {
                items.2 = None;
            }
        }
        if ask.endpoint.wire() != Wire::Anthropic {
            answer.assistant = Assistant::default();
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
            let mut reasoning_parts = 0usize;
            let mut signed_parts = 0usize;
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
                    Ok(Item::Event(StreamEvent::Start {
                        kind: rig_core::streaming::PartKind::Reasoning,
                        ..
                    })) => reasoning_parts += 1,
                    Ok(Item::Event(StreamEvent::End { content, .. })) => {
                        if let AssistantContent::Text(text) = &content {
                            deltas.completed_text(&text.text);
                        }
                        if content.native_item().is_some_and(|item| {
                            (item["type"] == "thinking"
                                && item["signature"]
                                    .as_str()
                                    .is_some_and(|signature| !signature.is_empty()))
                                || (item["type"] == "redacted_thinking"
                                    && item["data"].as_str().is_some_and(|data| !data.is_empty()))
                        }) {
                            signed_parts += 1;
                        }
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
            if finished.stop().is_failure()
                || (finished.origin.api.as_str() == "anthropic.messages"
                    && reasoning_parts != signed_parts)
            {
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
