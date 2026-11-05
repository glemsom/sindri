//! The only module that knows the provider library exists. A turn is sent over
//! one of three dialects — Anthropic Messages, OpenAI-compatible Chat
//! Completions, and Responses, which a Copilot GPT-6 model is routed to by its
//! own name — and the rest is ours: an HTTP client with explicit timeouts, and
//! the translation of the library's vocabulary into ours.

use std::borrow::Cow;
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
use rig_core::providers::copilot::{
    self,
    auth::{AuthSource, Authenticator, DeviceCodeHandler, FileTokenStore},
};
use rig_core::providers::openai::{OpenAIConfig, Route, wire::Auth};
use rig_core::streaming::{CompletionStream, Item, StreamEvent};
use rig_reqwest::ReqwestClient;
use rig_reqwest::reqwest;
use rig_reqwest::reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::config::Provider;

use super::{Call, Client, Delta, DeltaSink, Error, Message, SESSION_TOKEN, ToolSpec, Turn, Usage};

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
pub(super) fn turn(
    client: &Client,
    history: &[Message],
    tools: &[ToolSpec],
    thinking: bool,
    deltas: &mut dyn DeltaSink,
) -> Result<Turn, Error> {
    let (api_key, base_url, model) = client.parts();

    let http = http(client, Ask::Turn(client.session()))?;

    let request = request(
        history,
        tools,
        thinking,
        client.provider(),
        client.preamble(),
    )?;
    let runtime = runtime()?;

    let response = match client.provider() {
        Provider::Anthropic => {
            let config = anthropic_config(api_key, base_url);
            let model = config.connect(ReqwestClient::from(http)).completion(model);
            let stream = model.stream(request).map_err(convert)?;
            consume(&runtime, stream, deltas)?
        }
        // Every OpenAI-shaped server serves Chat Completions, and only some
        // serve `/responses`, so the wire is named rather than inferred.
        Provider::Copilot => {
            let config = copilot_config(&runtime, &http, Some(model))?;
            let model = config.connect(ReqwestClient::from(http)).completion(model);
            let stream = model.stream(request).map_err(convert)?;
            consume(&runtime, stream, deltas).map_err(refused_tools)?
        }
        Provider::Openai => {
            let config = openai_config(api_key, base_url);
            let model = config.connect(ReqwestClient::from(http)).completion(model);
            let stream = model.stream(request).map_err(convert)?;
            consume(&runtime, stream, deltas).map_err(refused_tools)?
        }
    };

    Ok(finish(response))
}

/// The names the wire says it serves, in the order it named them, asked with
/// the credential a turn is asked with. Nothing here is invented and nothing is
/// renamed: a name is the identifier the wire wrote, because that is the string
/// a later turn must send back. A wire that named nothing is a failure rather
/// than an empty list, so an empty list is never read as an answer.
pub(super) fn models(client: &Client) -> Result<Vec<String>, Error> {
    let (api_key, base_url, _) = client.parts();
    let http = http(client, Ask::Listing)?;
    let runtime = runtime()?;

    let listed = match client.provider() {
        Provider::Anthropic => {
            let config = anthropic_config(api_key, base_url);
            let wire = config.connect(ReqwestClient::from(http));
            runtime.block_on(wire.list_models()).map_err(convert)?
        }
        // The signed-in wire, asked with the session a turn is asked with. No
        // route is named: a listing is the wire's, not a model's.
        Provider::Copilot => {
            let config = copilot_config(&runtime, &http, None)?;
            let wire = config.connect(ReqwestClient::from(http));
            runtime.block_on(wire.list_models()).map_err(convert)?
        }
        Provider::Openai => {
            let config = openai_config(api_key, base_url);
            let wire = config.connect(ReqwestClient::from(http));
            runtime.block_on(wire.list_models()).map_err(convert)?
        }
    };

    let names: Vec<String> = listed.into_iter().map(|model| model.id).collect();
    if names.is_empty() {
        return Err(Error::answered(format!(
            "the {} wire named no models",
            client.provider().as_str()
        )));
    }
    Ok(names)
}

/// The runtime one call to the wire needs, on this thread and no other.
fn runtime() -> Result<tokio::runtime::Runtime, Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::transport(format!("could not start the runtime: {error}")))
}

/// The Messages wire, on the credential and the endpoint the run speaks to.
fn anthropic_config(api_key: &str, base_url: Option<&str>) -> AnthropicConfig {
    let mut config = AnthropicConfig::new(api_key);
    if let Some(base_url) = base_url {
        config = config.with_base_url(base_url);
    }
    config
}

/// The OpenAI-shaped wire as a turn speaks it: Chat Completions named rather
/// than inferred, and an optional bearer, so a server that wants no credential
/// is asked without one and is never sent a blank one.
fn openai_config(api_key: &str, base_url: Option<&str>) -> OpenAIConfig {
    let mut config = OpenAIConfig::new(api_key).with_route(Route::Chat);
    config.auth = Auth::OptionalBearer;
    if let Some(base_url) = base_url {
        config = config.with_base_url(base_url);
    }
    config
}

/// The Copilot wire, signed in from the stored session. The model is named when
/// a turn is what is wanted, because its identifier is what picks the route;
/// a listing names none.
fn copilot_config(
    runtime: &tokio::runtime::Runtime,
    http: &reqwest::Client,
    model: Option<&str>,
) -> Result<OpenAIConfig, Error> {
    let authenticator = Authenticator::new(
        AuthSource::OAuth,
        FileTokenStore::new(
            crate::paths::state_dir()
                .map_err(|error| {
                    Error::transport(format!("could not locate Copilot credentials: {error}"))
                })?
                .join("copilot"),
        ),
        DeviceCodeHandler::default(),
        false,
    );
    let api_client = ReqwestClient::from(http.clone());
    let auth = runtime
        .block_on(authenticator.auth_context(&api_client))
        .map_err(|error| {
            Error::transport(format!(
                "GitHub Copilot authentication failed: {error}; run `sindri auth login copilot`"
            ))
        })?;
    let mut config = OpenAIConfig::with_key(&copilot::DIALECT, auth.api_key.expose());
    // Copilot's session token is scoped to the endpoint Rig derives from its
    // auth response; never route it to a configured gateway.
    if let Some(base_url) = auth.api_base.as_deref() {
        config = config.with_base_url(base_url);
    }
    Ok(match model.and_then(copilot_route) {
        Some(route) => config.with_route(route),
        None => config,
    })
}

/// Copilot's GPT-6 models use Responses even though their identifiers do not
/// carry the `codex` marker Rig's dialect uses for route selection.
fn copilot_route(model: &str) -> Option<Route> {
    let model = model.to_ascii_lowercase();
    (model.starts_with("gpt-6-") || model.starts_with("gpt-6.")).then_some(Route::Responses)
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

/// The HTTP client one request speaks through: the deadlines, and the headers
/// the config asked for. A gateway that routes on a header of its own — a
/// session id, a referer — has no other way to be told, and the request cannot
/// be routed without it.
///
/// This is where a header value's [`SESSION_TOKEN`] becomes the run's session
/// id: resolved once, at the one place the client is built and the session is at
/// hand, so the file's table, `SINDRI_EXTRA_HEADERS` and `--header` all get it
/// and none of them has to be rewritten. Whether it is resolved at all is
/// [`Ask`]'s to say, and a listing's answer is that it is not: a value carrying
/// the token is left off that request whole, whatever else it holds, because
/// half a value is not one a gateway can route on and a placeholder it might
/// route on is worse than a header it does not see.
fn http(client: &Client, ask: Ask<'_>) -> Result<reqwest::Client, Error> {
    let mut headers = HeaderMap::with_capacity(client.extra_headers().len());
    for (name, value) in client.extra_headers() {
        let Some(value) = header_value(value, ask) else {
            continue;
        };
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| Error::transport(format!("`{name}` is not an HTTP header name")))?;
        let value = HeaderValue::from_str(&value).map_err(|_| {
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

/// What one request to the wire is, which is what decides whether a header
/// value's [`SESSION_TOKEN`] has anything to become. A turn is one request of the
/// run and has the run's session; a listing is not a turn — it writes nothing to
/// a record and belongs to no turn of the run — so it names no session.
#[derive(Clone, Copy)]
enum Ask<'a> {
    Turn(&'a str),
    Listing,
}

/// One configured header value as the request sends it: [`SESSION_TOKEN`]
/// replaced by the run's session id, a value with no token verbatim. `None` when
/// the value carries the token and there is no session to put in its place,
/// which is the listing's answer and never a turn's.
fn header_value<'a>(value: &'a str, ask: Ask<'_>) -> Option<Cow<'a, str>> {
    if !value.contains(SESSION_TOKEN) {
        return Some(Cow::Borrowed(value));
    }
    match ask {
        Ask::Turn(session) => Some(Cow::Owned(value.replace(SESSION_TOKEN, session))),
        Ask::Listing => None,
    }
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
    preamble: &str,
) -> Result<CompletionRequest, Error> {
    let mut request = CompletionRequest::new(WireMessage::user(String::new()));
    request.chat_history = history
        .iter()
        .map(|message| wire_message(message).map(Arc::new))
        .collect::<Result<Vec<_>, Error>>()?;
    let mut request = request
        .preamble(preamble)
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
            Provider::Openai | Provider::Copilot => Reasoning::Effort(THINKING_EFFORT),
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

/// The wire's own verdict, carried over: it decides whether asking again could
/// answer, and it decides it from what happened rather than from the status
/// alone — a request timeout, a request that arrived too early, a rate limit, a
/// server fault, a transport that never got a reply. Reading the status here
/// instead would answer a narrower question and lose the rest.
fn convert(error: ProviderError) -> Error {
    let report = error.report();
    Error::classified(report.http_status, report.retryable, report.message)
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
    let said = format!(
        "the model server does not support tool calling, which `bash` needs: {} \
         (use a model or server that serves function calls)",
        error.message()
    );
    error.restated(said)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::agent::{BashTool, Limits, Silent};
    use crate::sandbox::SandboxPolicy;

    #[test]
    fn a_failure_the_run_caused_is_never_sent_again() {
        let unsent = super::Error::transport("`x-bad header` is not an HTTP header name");
        let unresolved = super::Error::transport("GitHub Copilot authentication failed");

        assert!(
            !unsent.retryable() && !unresolved.retryable(),
            "a header this run could not send and a credential it could not resolve repeat"
        );
        assert!(
            !super::stopped().retryable(),
            "a stream the run stopped for a cancel is not a failure to send again"
        );
    }

    #[test]
    fn copilot_gpt_six_models_use_the_responses_route() {
        use rig_core::providers::openai::Route;

        for model in ["gpt-6-luna", "gpt-6-astra", "gpt-6-sol", "gpt-6.1-sol"] {
            assert_eq!(
                super::copilot_route(model),
                Some(Route::Responses),
                "{model}"
            );
        }
        assert_eq!(super::copilot_route("gpt-5.3-codex"), None);
        assert_eq!(super::copilot_route("gpt-4o"), None);
        assert_eq!(super::copilot_route("gpt-60-preview"), None);
    }

    /// The one sentence the bash description carries about the catalog, spelled
    /// out here rather than quoted from the source: this test exists to show
    /// what that sentence costs, so it must not read the cost off the code that
    /// writes it.
    const POINTER: &str = "Instructions for this environment live in `skill`: look there before \
                           your first command.";

    /// One request's tool descriptions and the system message beside them, for a
    /// catalog: built through the tools the agent hands over, so the text read
    /// here is the text that goes on the wire.
    fn wire(
        root: &std::path::Path,
        catalog: &crate::skills::Catalog,
    ) -> (Vec<(String, String)>, String) {
        // One root for both requests, so the policy's own words are the same
        // words and the difference read below is the catalog's alone.
        let policy = SandboxPolicy::fixture(root);
        let bash = BashTool::new(&policy, Limits::default(), Arc::new(Silent));
        let tools = crate::agent::Tools::new(bash, crate::agent::SkillTool::new(catalog), None);
        let preamble = crate::preamble::of(root, None);
        let request = super::request(
            &[],
            &tools.specs(),
            false,
            crate::config::Provider::Anthropic,
            preamble.as_str(),
        )
        .expect("a request");
        (
            request
                .tools
                .iter()
                .map(|tool| (tool.name.to_string(), tool.description.clone()))
                .collect(),
            format!("{:?}", request.chat_history[0]),
        )
    }

    #[test]
    fn the_wire_shows_what_the_catalog_costs_the_bash_description() {
        // A run with nothing the model may see: the description it sent before
        // the pointer existed, and no second tool to point at.
        let root = tempfile::TempDir::new().expect("workspace");
        let (without, preamble_without) = wire(root.path(), &crate::skills::Catalog::default());
        assert_eq!(without.len(), 1, "no catalog, no `skill` tool: {without:?}");
        assert_eq!(without[0].0, "bash");

        // A run with a built-in, which is the one every run carries: the same
        // description with one sentence after it, and the tool it names.
        let (with, preamble_with) = wire(
            root.path(),
            &crate::skills::fold([crate::skills::builtins()]),
        );
        assert_eq!(
            with.len(),
            2,
            "the pointed-at tool is on the wire: {with:?}"
        );
        assert_eq!(with[1].0, "skill", "{with:?}");
        assert_eq!(
            with[0].1,
            format!("{}\n{POINTER}", without[0].1),
            "the pointer is the whole of the difference the catalog makes to the bash description"
        );

        // The preamble is not where the pointer was paid for: it is the message
        // it always was, in both requests, byte for byte.
        assert_eq!(
            preamble_with, preamble_without,
            "the catalog leaves the system message alone"
        );
        assert_eq!(
            preamble_without,
            format!(
                "{:?}",
                super::WireMessage::system(
                    crate::preamble::of(root.path(), None).as_str().to_string()
                )
            ),
            "the message is the preamble and nothing else: {preamble_without}"
        );
    }
}
