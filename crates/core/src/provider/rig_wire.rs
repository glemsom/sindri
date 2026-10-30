//! The only module that knows the provider library exists: the
//! Anthropic Messages wire, the OpenAI-compatible Chat Completions wire, our
//! own HTTP client with explicit timeouts, and the translation of the
//! library's vocabulary into ours.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use rig_core::completion::message::{
    AssistantContent, AssistantMessage, CallId, Message as WireMessage, ToolCall, ToolFunction,
    ToolName, ToolResult, ToolResultContent,
};
use rig_core::completion::{
    CompletionRequest, CompletionResponse, Effort, GenerationOptions, Reasoning, ToolDefinition,
};
use rig_core::error::ProviderError;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::openai::{OpenAIConfig, Route, wire::Auth};
use rig_core::streaming::{CompletionStream, Item, StreamEvent};
use rig_reqwest::ReqwestClient;
use rig_reqwest::reqwest;
use rig_reqwest::reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::config::Provider;

use super::{Call, Client, Delta, DeltaSink, Error, Message, ToolSpec, Turn, Usage};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
/// The most tokens one answer may use. A model the provider library does not
/// know has no default, and an unbounded answer is an unbounded bill.
const MAX_OUTPUT_TOKENS: u64 = 8192;
/// The most tokens the model may spend reasoning when the run pays for it.
const THINKING_BUDGET: u32 = 4096;
/// The effort an OpenAI-compatible server is asked for when the run pays for
/// reasoning: that wire names a level, not a budget.
const THINKING_EFFORT: Effort = Effort::Medium;
/// The instructions every batch turn carries. The sandbox's own description
/// arrives with the `bash` tool's definition.
const PREAMBLE: &str = "You are Sindri, an assistant answering a developer at a Unix \
terminal. Answer the question you were given, using the context after it when there is \
some. You have one tool, `bash`: run commands with it rather than guessing, and check \
the exit code it reports before believing the output. Reply with the answer itself: no \
preamble, no closing offer of further help.";

pub(super) fn turn(
    client: &Client,
    history: &[Message],
    tools: &[ToolSpec],
    thinking: bool,
    deltas: &mut dyn DeltaSink,
) -> Result<Turn, Error> {
    let (api_key, base_url, model) = client.parts();

    let http = http(client)?;

    let request = request(history, tools, thinking, client.provider())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::transport(format!("could not start the runtime: {error}")))?;

    let response = match client.provider() {
        Provider::Anthropic => {
            let mut config = AnthropicConfig::new(api_key);
            if let Some(base_url) = base_url {
                config = config.with_base_url(base_url);
            }
            let model = config.connect(ReqwestClient::from(http)).completion(model);
            let stream = model.stream(request).map_err(convert)?;
            consume(&runtime, stream, deltas)?
        }
        // Every OpenAI-shaped server serves Chat Completions, and only some
        // serve `/responses`, so the wire is named rather than inferred.
        Provider::Openai => {
            let mut config = OpenAIConfig::new(api_key).with_route(Route::Chat);
            // A server that wants no credential must not be sent a blank one.
            config.auth = Auth::OptionalBearer;
            if let Some(base_url) = base_url {
                config = config.with_base_url(base_url);
            }
            let model = config.connect(ReqwestClient::from(http)).completion(model);
            let stream = model.stream(request).map_err(convert)?;
            consume(&runtime, stream, deltas).map_err(refused_tools)?
        }
    };

    Ok(finish(response))
}

/// Reads the reply to its end, handing every fragment to `deltas` as it
/// arrives, and folds it into the whole response. A stream that stops before
/// the provider ended it, or that carries the provider's own failure, is an
/// error here: neither may pass for an answer.
fn consume(
    runtime: &tokio::runtime::Runtime,
    stream: CompletionStream,
    deltas: &mut dyn DeltaSink,
) -> Result<CompletionResponse, Error> {
    runtime.block_on(async {
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            match item.map_err(convert)? {
                Item::Event(StreamEvent::Text { text, .. }) => {
                    if !deltas.delta(Delta::Text(text)) {
                        return Err(stopped());
                    }
                }
                Item::Event(StreamEvent::Reasoning { text, .. }) => {
                    if !deltas.delta(Delta::Reasoning(text)) {
                        return Err(stopped());
                    }
                }
                // A part opening or ending carries no fragment: the answer's
                // text is what grew, not the envelope around it.
                Item::Event(_) | Item::Unknown(_) => {}
            }
        }
        stream.finish().await.map_err(convert)
    })
}

/// The sink asked for the stream to stop, which is how a cancel reaches the
/// adapter. The loop maps it to a cancellation because its flag is already up.
fn stopped() -> Error {
    Error::transport("the stream was stopped before it finished")
}

/// The HTTP client one run speaks through: the deadlines, and the headers the
/// config asked for. A gateway that routes on a header of its own — a session
/// id, a referer — has no other way to be told, and the request cannot be
/// routed without it.
fn http(client: &Client) -> Result<reqwest::Client, Error> {
    let mut headers = HeaderMap::with_capacity(client.extra_headers().len());
    for (name, value) in client.extra_headers() {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| Error::transport(format!("`{name}` is not an HTTP header name")))?;
        let value = HeaderValue::from_str(value).map_err(|_| {
            Error::transport(format!("the value of `{name}` is not a header value"))
        })?;
        headers.insert(name, value);
    }
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .default_headers(headers)
        .build()
        .map_err(|error| Error::transport(format!("could not build the HTTP client: {error}")))
}

fn finish(response: CompletionResponse) -> Turn {
    let reasoning = response.reasoning();
    Turn {
        text: response.text(),
        reasoning: (!reasoning.trim().is_empty()).then_some(reasoning),
        calls: response
            .tool_calls()
            .map(|call| {
                Call::new(
                    call.id.to_string(),
                    call.function.name.as_str(),
                    arguments(&call.function),
                )
            })
            .collect(),
        model: response.model().map(str::to_owned),
        usage: Usage {
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            cached_input_tokens: response.usage.cached_input_tokens,
            reasoning_tokens: response.usage.reasoning_tokens,
            total_tokens: response.usage.total_tokens,
        },
    }
}

fn request(
    history: &[Message],
    tools: &[ToolSpec],
    thinking: bool,
    provider: Provider,
) -> Result<CompletionRequest, Error> {
    let mut request = CompletionRequest::new(WireMessage::user(String::new()));
    request.chat_history = history
        .iter()
        .map(|message| wire_message(message).map(Arc::new))
        .collect::<Result<Vec<_>, Error>>()?;
    let mut request = request
        .preamble(PREAMBLE)
        .max_tokens(MAX_OUTPUT_TOKENS)
        .tools(
            tools
                .iter()
                .map(tool_definition)
                .collect::<Result<Vec<_>, Error>>()?,
        );
    if thinking {
        let reasoning = match provider {
            Provider::Anthropic => Reasoning::Budget {
                tokens: THINKING_BUDGET,
            },
            Provider::Openai => Reasoning::Effort(THINKING_EFFORT),
        };
        request.options = GenerationOptions::default().reasoning(reasoning);
    }
    Ok(request)
}

fn tool_definition(spec: &ToolSpec) -> Result<ToolDefinition, Error> {
    Ok(ToolDefinition::new(
        tool_name(&spec.name)?,
        spec.description.clone(),
        spec.parameters.clone(),
    ))
}

fn wire_message(message: &Message) -> Result<WireMessage, Error> {
    Ok(match message {
        Message::User(text) => WireMessage::user(text.clone()),
        Message::Assistant { text, calls } => {
            let mut content: Vec<AssistantContent> = Vec::with_capacity(calls.len() + 1);
            if !text.is_empty() {
                content.push(AssistantContent::text(text.clone()));
            }
            for call in calls {
                content.push(AssistantContent::ToolCall(ToolCall::new(
                    CallId::from_wire(call.id.clone()),
                    tool_function(call)?,
                )));
            }
            WireMessage::Assistant(AssistantMessage::new(content))
        }
        Message::ToolResults(outcomes) => WireMessage::tool_results(
            outcomes
                .iter()
                .map(|outcome| {
                    Ok(ToolResult {
                        call: CallId::from_wire(outcome.call_id.clone()),
                        name: tool_name(&outcome.name)?,
                        content: vec![ToolResultContent::text(outcome.content.clone())],
                        is_error: outcome.is_error,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?,
        ),
    })
}

fn tool_function(call: &Call) -> Result<ToolFunction, Error> {
    let name = tool_name(&call.name)?;
    Ok(match call.sent() {
        Ok(arguments) => ToolFunction::new(name, serde_json::Value::Object(arguments.clone())),
        Err(sent) => ToolFunction::parse(name, sent),
    })
}

fn tool_name(name: &str) -> Result<ToolName, Error> {
    ToolName::new(name).map_err(|_| Error::transport("the model sent a call to no tool"))
}

fn arguments(
    function: &ToolFunction,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    match &function.invalid_arguments {
        Some(text) => Err(format!("the arguments were not a JSON object: `{text}`")),
        None => Ok(function.arguments.clone()),
    }
}

fn convert(error: ProviderError) -> Error {
    let report = error.report();
    Error::answered(report.http_status, report.message)
}

/// A failure, said plainly when what the server refused was the `bash` tool.
/// An OpenAI-compatible server without function calling rejects the request
/// the moment it is asked to describe one, and its own words are usually a
/// field name; a user needs to know it is the server's capability, not their
/// configuration, that is missing.
fn refused_tools(error: Error) -> Error {
    let status_refused = matches!(error.status(), Some(400 | 404 | 422 | 501));
    if !status_refused || !refuses_capability(error.message()) {
        return error;
    }
    Error::answered(
        error.status(),
        format!(
            "the model server does not support tool calling, which `bash` needs: {} \
             (use a model or server that serves function calls)",
            error.message()
        ),
    )
}

/// What a server says when the capability, not the request, is what it lacks.
/// Naming a field it will not take is not enough: a malformed tool call is
/// also a 400 naming tools, and telling that user their server cannot call
/// functions would send them shopping for a server they already have.
fn refuses_capability(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    const REFUSALS: [&str; 5] = [
        "does not support tool",
        "doesn't support tool",
        "tools are not supported",
        "tool calling is not supported",
        "function calling is not supported",
    ];
    REFUSALS.iter().any(|refusal| message.contains(refusal))
        || (message.contains("tool") && message.contains("not supported"))
}
