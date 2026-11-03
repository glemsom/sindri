//! The only module that knows the provider library exists: the
//! Anthropic Messages wire, the OpenAI-compatible Chat Completions wire, our
//! own HTTP client with explicit timeouts, and the translation of the
//! library's vocabulary into ours.

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
/// arrives with the `bash` tool's definition, and so does the batching rule:
/// what a result costs the conversation is only legible beside the result, so
/// this prompt states it once, there, and never here.
///
/// The sentence about reading is not decoration: told to search and open ranges,
/// a model spent 0.42x the input tokens of the same model told nothing, on the
/// same four tasks and with the same answers right — measured, and reproducible
/// with `scripts/bench-system-prompt`.
const PREAMBLE: &str = "You are Sindri, answering a developer at a Unix terminal. Reply with the answer \
    itself: no preamble, no closing offer of further help. Run commands rather than guessing. Read \
    the fewest bytes that answer: search first (`rg -n`), then open only the lines you need \
    (`sed -n 'A,Bp'`); never `cat` a whole file, and never run a search or a listing twice. A fence \
    marked `mermaid` is drawn as a picture: draw what prose cannot.";

pub(super) fn turn(
    client: &Client,
    history: &[Message],
    tools: &[ToolSpec],
    thinking: bool,
    deltas: &mut dyn DeltaSink,
) -> Result<Turn, Error> {
    let (api_key, base_url, model) = client.parts();

    let http = http(client)?;

    let request = request(
        history,
        tools,
        thinking,
        client.provider(),
        client.instructions(),
    )?;
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
    instructions: Option<&str>,
) -> Result<CompletionRequest, Error> {
    let mut request = CompletionRequest::new(WireMessage::user(String::new()));
    request.chat_history = history
        .iter()
        .map(|message| wire_message(message).map(Arc::new))
        .collect::<Result<Vec<_>, Error>>()?;
    let preamble = preamble(instructions);
    let mut request = request
        .preamble(preamble.as_ref())
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

/// The system message every turn carries: the preamble, and the repository's
/// own rules after it when the run stands in a repository that has any.
///
/// They are one message rather than a turn of their own, so the transcript the
/// user reads is their own words and the preamble's measured words are the
/// preamble's: what follows is a file, and its size is the cap's business, never
/// the budget's.
fn preamble(instructions: Option<&str>) -> Cow<'_, str> {
    match instructions {
        None => Cow::Borrowed(PREAMBLE),
        Some(text) => Cow::Owned(format!("{PREAMBLE}\n\n{text}")),
    }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::PREAMBLE;
    use crate::agent::{BashTool, Limits};
    use crate::exec::{BashExecutor, ToolResult};
    use crate::sandbox::{Enforcement, SandboxPolicy, WritableSet};

    /// The two sentences the preamble keeps whatever else it sheds: the reading
    /// rule, measured at 0.42x the tokens of the same model told nothing, and the
    /// renderer fact no model can know — quoted from the shipped text.
    const READING_RULE: &str = "Read the fewest bytes that answer: search first (`rg -n`), then open \
        only the lines you need (`sed -n 'A,Bp'`); never `cat` a whole file, and never run a search \
        or a listing twice.";
    const MERMAID: &str = "A fence marked `mermaid` is drawn as a picture: draw what prose cannot.";

    /// No command is run to read a description.
    struct Silent;

    impl BashExecutor for Silent {
        fn execute(&self, _script: &str) -> Option<ToolResult> {
            None
        }
    }

    #[test]
    fn the_script_sentence_is_stated_once_and_in_the_tool_description() {
        // Every phrase below was paid for twice while both copies shipped: the
        // preamble and the tool description are re-sent on every turn of every
        // run, so the doctrine belongs in the one that sits beside the cost.
        for moved in [
            "put the steps of one task",
            "$TMPDIR",
            "using the context after it",
            "check the exit code",
        ] {
            assert!(
                !PREAMBLE.contains(moved),
                "the preamble no longer carries `{moved}`: {PREAMBLE}"
            );
        }
        let words = PREAMBLE.split_whitespace().count();
        assert!(
            words <= 80,
            "the preamble stays at or under 80 words, not {words}: {PREAMBLE}"
        );

        let description = tool_description();
        for carried in ["put the steps of one task in one script", "$TMPDIR"] {
            assert!(
                description.contains(carried),
                "the tool description carries `{carried}`: {description}"
            );
        }
        for kept in [READING_RULE, MERMAID] {
            assert!(
                PREAMBLE.contains(kept),
                "the preamble still carries `{kept}`: {PREAMBLE}"
            );
        }
    }

    #[test]
    fn rules_beside_the_preamble_leave_its_words_alone() {
        let rules = "The repository you are working in carries its house rules.\n\nBe brief.\n";

        let alone = super::preamble(None);
        let together = super::preamble(Some(rules));

        assert_eq!(alone, PREAMBLE, "no rules is the preamble and no more");
        assert!(
            together.starts_with(PREAMBLE) && together.ends_with(rules),
            "the rules follow the preamble, whole: {together}"
        );
        // The measurement is the preamble's own, and a file beside it is not
        // folded into it: what follows is a
        // repository's words, measured against the cap and never this budget.
        assert_eq!(
            together[..PREAMBLE.len()].split_whitespace().count(),
            PREAMBLE.split_whitespace().count(),
            "the preamble's own words are unchanged by what follows it"
        );
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
        let writable = WritableSet::new([root.to_path_buf()]).expect("writable set");
        let policy = SandboxPolicy::new(
            writable,
            root.join("session/tmp"),
            crate::paths::home(),
            None,
            "/usr/bin:/bin".to_string(),
            Enforcement::Bubblewrap,
        )
        .expect("policy");
        let bash = BashTool::new(&policy, Limits::default(), Arc::new(Silent));
        let tools = crate::agent::Tools::new(bash, crate::agent::SkillTool::new(catalog), None);
        let request = super::request(
            &[],
            &tools.specs(),
            false,
            crate::config::Provider::Anthropic,
            None,
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
            format!("{:?}", super::WireMessage::system(PREAMBLE.to_string())),
            "the message is the preamble and nothing else: {preamble_without}"
        );
    }

    /// The description as the model receives it: a policy refuses a writable path
    /// that is not there, so the test makes one.
    fn tool_description() -> String {
        let workspace = tempfile::TempDir::new().expect("workspace");
        let writable = WritableSet::new([workspace.path().to_path_buf()]).expect("writable set");
        let policy = SandboxPolicy::new(
            writable,
            workspace.path().join("session/tmp"),
            crate::paths::home(),
            None,
            "/usr/bin:/bin".to_string(),
            Enforcement::Bubblewrap,
        )
        .expect("policy");
        let tool = BashTool::new(&policy, Limits::default(), Arc::new(Silent));
        tool.spec().description.clone()
    }
}
