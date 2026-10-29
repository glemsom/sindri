//! The model, behind one seam: ask, and get an answer or a failure that is
//! safe to print. Retries and redaction live here; the provider library lives
//! only in [`rig_wire`], so a library rewrite is one file's problem (ADR-0003).

mod rig_wire;

use std::time::Duration;

use serde::Serialize;

use crate::config::Config;

/// How many times one request is attempted before the failure is reported.
const MAX_ATTEMPTS: u32 = 3;
/// The first wait between attempts; each retry doubles it.
const BASE_BACKOFF: Duration = Duration::from_millis(200);
/// What a redacted credential is replaced by, everywhere.
pub const REDACTED: &str = "[redacted]";

/// One tool the model may call, as the wire sends it and the loop names it.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// The tool's name on the wire.
    pub name: String,
    /// The description the model reads: derived from the sandbox policy.
    pub description: String,
    /// The JSON Schema of the call's arguments.
    pub parameters: serde_json::Value,
}

/// One tool call the model asked for, with its arguments as they arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The provider's id for the call, echoed back with the result.
    pub id: String,
    /// The tool the model named.
    pub name: String,
    /// The arguments if they were an object, else exactly the text that was
    /// sent in their place, so the turn can be replayed as the model wrote it.
    arguments: Result<serde_json::Map<String, serde_json::Value>, String>,
}

impl Call {
    /// A call whose arguments are an object, or the text sent in their place.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: Result<serde_json::Map<String, serde_json::Value>, String>,
    ) -> Self {
        Call {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    /// The arguments as the model sent them, for replaying its own turn: the
    /// object it wrote, or the text it wrote instead of one.
    pub(crate) fn sent(&self) -> Result<&serde_json::Map<String, serde_json::Value>, &String> {
        self.arguments.as_ref()
    }

    /// The script this call asks for, or why the call cannot be run: malformed
    /// arguments are answered, never dropped, so the model can correct itself.
    pub fn command(&self) -> Result<String, String> {
        let arguments = self
            .arguments
            .as_ref()
            .map_err(|sent| format!("the arguments were not a JSON object: `{sent}`"))?;
        match arguments.get("command") {
            Some(serde_json::Value::String(command)) if !command.trim().is_empty() => {
                Ok(command.clone())
            }
            Some(serde_json::Value::String(_)) | None => {
                Err("the call has no `command`: send {\"command\": \"<bash script>\"}".to_string())
            }
            Some(other) => Err(format!(
                "`command` must be a string, not {}",
                json_kind(other)
            )),
        }
    }
}

/// The JSON type of a value, as a message names it.
fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// One tool call's result, ready to send back to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutcome {
    /// The call this answers.
    pub call_id: String,
    /// The tool that ran, so the wire can name it.
    pub name: String,
    /// What the model reads.
    pub content: String,
    /// Whether the tool never ran, or failed to: true for a malformed call and
    /// for a sandbox that would not start.
    pub is_error: bool,
}

impl ToolOutcome {
    /// A result with what the model reads, and whether the tool itself failed.
    pub fn new(call: &Call, content: String, is_error: bool) -> Self {
        ToolOutcome {
            call_id: call.id.clone(),
            name: call.name.clone(),
            content,
            is_error,
        }
    }

    /// A call that could not be run, said so the model can correct it.
    pub fn malformed(call: &Call, reason: &str) -> Self {
        ToolOutcome {
            call_id: call.id.clone(),
            name: call.name.clone(),
            content: format!("tool_error: the call was not run: {reason}"),
            is_error: true,
        }
    }
}

/// The token counters a provider reports. Absent means unreported, never zero.
///
/// The field order is the JSON output contract's own (ADR-0004), and a counter
/// nobody reported is left out of the object rather than sent as null.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    /// Every input token, cached or not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Every output token, reasoning included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// The input tokens a provider served from its cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    /// The output tokens the model spent reasoning, where the provider says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// Input plus output, when the provider reports both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

impl Usage {
    /// Whether the provider reported nothing at all: such a `usage` object
    /// carries no information, so the contract leaves it out.
    pub fn is_empty(&self) -> bool {
        *self == Usage::default()
    }
    /// Adds one turn's counters to the run's: a counter nobody reported stays
    /// unreported, and one reported once stays reported (ADR-0004).
    pub fn accumulate(&mut self, turn: &Usage) {
        self.input_tokens = add(self.input_tokens, turn.input_tokens);
        self.output_tokens = add(self.output_tokens, turn.output_tokens);
        self.cached_input_tokens = add(self.cached_input_tokens, turn.cached_input_tokens);
        self.reasoning_tokens = add(self.reasoning_tokens, turn.reasoning_tokens);
        self.total_tokens = add(self.total_tokens, turn.total_tokens);
    }
}

/// Two counters summed, with absence never standing in for zero.
fn add(total: Option<u64>, turn: Option<u64>) -> Option<u64> {
    match (total, turn) {
        (None, None) => None,
        (total, turn) => Some(total.unwrap_or(0) + turn.unwrap_or(0)),
    }
}

/// A provider or transport failure. Its message is the provider's own, with
/// the credential redacted, because that is what a user needs to act on.
#[derive(Debug, Clone)]
pub struct Error {
    message: String,
    status: Option<u16>,
}

impl Error {
    /// The failure as one line: no credential, and no stack.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The HTTP status the provider answered with, when it answered at all.
    pub fn status(&self) -> Option<u16> {
        self.status
    }

    /// Whether another attempt could succeed. Only rate limits and server
    /// faults qualify: a client error repeats identically forever (ADR-0003).
    fn retryable(&self) -> bool {
        matches!(self.status, Some(429) | Some(500..=599))
    }

    /// A transport failure: no reply, so no status.
    fn transport(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            status: None,
        }
    }

    /// A failure with the status the provider answered with.
    fn answered(status: Option<u16>, message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            status,
        }
    }
}

/// One item of the conversation the loop keeps: what the model is shown on the
/// next turn. Our own vocabulary, so nothing rig-shaped leaves the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// The question, or the piped context after it.
    User(String),
    /// A turn the model produced: its text and the calls it asked for.
    Assistant { text: String, calls: Vec<Call> },
    /// What the tools answered, in the order the calls were made.
    ToolResults(Vec<ToolOutcome>),
}

/// One turn's reply: what the model said, asked for, and cost.
#[derive(Debug, Clone)]
pub struct Turn {
    /// The assistant's text; empty when the turn only asks for tools.
    pub text: String,
    /// The model's reasoning, where the provider shows it.
    pub reasoning: Option<String>,
    /// The tool calls, in the order the model asked for them.
    pub calls: Vec<Call>,
    /// The model the provider says answered, when it says.
    pub model: Option<String>,
    /// What this turn was billed for.
    pub usage: Usage,
}

/// The model, behind one seam: ask one turn, and get a reply or a failure that
/// is safe to print.
pub trait Model {
    /// One round trip: `history` with `tools` available, and whether the run
    /// pays for reasoning.
    fn turn(&self, history: &[Message], tools: &[ToolSpec], thinking: bool) -> Result<Turn, Error>;
}

/// One configured provider endpoint: the wire, the credential and the model.
#[derive(Debug, Clone)]
pub struct Client {
    provider: crate::config::Provider,
    api_key: String,
    base_url: Option<String>,
    model: String,
}

impl Client {
    /// The client a resolved config describes.
    pub fn new(config: &Config) -> Client {
        Client {
            provider: config.provider,
            api_key: config.api_key.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
        }
    }

    /// The provider this client speaks to.
    pub fn provider(&self) -> crate::config::Provider {
        self.provider
    }

    /// The model this client asks.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The credential never leaves the process in a message or a record.
    fn redact(&self, error: Error) -> Error {
        if self.api_key.is_empty() {
            return error;
        }
        Error {
            message: error.message.replace(&self.api_key, REDACTED),
            status: error.status,
        }
    }

    /// The pieces the wire needs, named for it so `rig_wire` holds no policy.
    pub(crate) fn parts(&self) -> (&str, Option<&str>, &str) {
        (&self.api_key, self.base_url.as_deref(), &self.model)
    }
}

impl Model for Client {
    /// One turn, retried while retrying can help and reported otherwise.
    fn turn(&self, history: &[Message], tools: &[ToolSpec], thinking: bool) -> Result<Turn, Error> {
        let mut attempt = 1;
        loop {
            match rig_wire::turn(self, history, tools, thinking) {
                Ok(turn) => return Ok(turn),
                Err(error) if error.retryable() && attempt < MAX_ATTEMPTS => {
                    std::thread::sleep(BASE_BACKOFF * 2u32.pow(attempt - 1));
                    attempt += 1;
                }
                Err(error) => return Err(self.redact(error)),
            }
        }
    }
}
