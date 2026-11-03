//! The model, behind one seam: ask, and get an answer or a failure that is
//! safe to print. Retries and redaction live here; the provider library lives
//! only in [`rig_wire`], so a library rewrite is one file's problem.

mod asked;
mod rig_wire;
mod window;

pub use asked::{Asked, Target, reads};
pub use window::context_window;

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::Config;

const MAX_ATTEMPTS: u32 = 3;
const BASE_BACKOFF: Duration = Duration::from_millis(200);
pub const REDACTED: &str = "[redacted]";
/// The words a call no tool could answer opens with, for the model to read. The
/// reason after them is a fact as well — [`ToolFacts::reason`] — so a front end
/// that says one in its own words reads it as a value rather than cutting this
/// prefix back off the prose.
pub const MALFORMED: &str = "tool_error: the call was not run: ";

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub id: String,
    pub name: String,
    /// The arguments if they were an object, else exactly the text that was
    /// sent in their place, so the turn can be replayed as the model wrote it.
    arguments: Result<serde_json::Map<String, serde_json::Value>, String>,
}

impl Call {
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

    pub(crate) fn sent(&self) -> Result<&serde_json::Map<String, serde_json::Value>, &String> {
        self.arguments.as_ref()
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutcome {
    pub call_id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
    /// The same facts `content` states, as fields. Absent ones mean the call
    /// never reached the sandbox, which is not the same as an outcome of zero.
    pub facts: ToolFacts,
}

impl ToolOutcome {
    pub fn new(call: &Call, content: String, is_error: bool) -> Self {
        ToolOutcome {
            call_id: call.id.clone(),
            name: call.name.clone(),
            content,
            is_error,
            facts: ToolFacts::default(),
        }
    }

    /// A call no tool answered: the reason is written into the text the model
    /// reads and kept beside it as a fact, from the one string, so the two
    /// spellings of it cannot disagree.
    pub fn malformed(call: &Call, reason: &str) -> Self {
        ToolOutcome {
            call_id: call.id.clone(),
            name: call.name.clone(),
            content: format!("{MALFORMED}{reason}"),
            is_error: true,
            facts: ToolFacts {
                reason: Some(reason.to_string()),
                ..ToolFacts::default()
            },
        }
    }

    /// The facts the content was rendered from: given here, where both are
    /// built from one result, so the two cannot disagree.
    pub fn with_facts(mut self, facts: ToolFacts) -> Self {
        self.facts = facts;
        self
    }
}

/// A tool result's facts as fields rather than prose: what the record keeps
/// beside the text the model was shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolFacts {
    /// `exited`, `timed_out` or `sandbox_error`; absent when no command ran.
    pub outcome: Option<String>,
    /// Why a refused call never ran — a name no tool answers, arguments no tool
    /// takes, a target the guard refuses — in the words the content was written
    /// with. Absent when a tool answered the call, so it is never a recovered
    /// prefix but the reason itself.
    pub reason: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub truncated: bool,
    pub spill: Vec<String>,
}

/// The token counters a provider reports. Absent means unreported, never zero:
/// the field order is the JSON contract's own, and a counter nobody reported is
/// left out rather than sent as null.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

impl Usage {
    pub fn is_empty(&self) -> bool {
        *self == Usage::default()
    }
    /// Adds one turn's counters to the run's: a counter nobody reported stays
    /// unreported, and one reported once stays reported.
    pub fn accumulate(&mut self, turn: &Usage) {
        self.input_tokens = add(self.input_tokens, turn.input_tokens);
        self.output_tokens = add(self.output_tokens, turn.output_tokens);
        self.cached_input_tokens = add(self.cached_input_tokens, turn.cached_input_tokens);
        self.reasoning_tokens = add(self.reasoning_tokens, turn.reasoning_tokens);
        self.total_tokens = add(self.total_tokens, turn.total_tokens);
    }

    /// A counter nobody reported, as the zero a figure is drawn with. A report
    /// that says nothing is not a report of nothing: `is_empty` is the question
    /// the record asks, and this is the question a meter asks.
    pub fn input(&self) -> u64 {
        self.input_tokens.unwrap_or_default()
    }

    /// See [`Usage::input`]: the output figure a meter draws.
    pub fn output(&self) -> u64 {
        self.output_tokens.unwrap_or_default()
    }

    /// See [`Usage::input`]: the cached figure a cache hit is a share of.
    pub fn cached(&self) -> u64 {
        self.cached_input_tokens.unwrap_or_default()
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
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn status(&self) -> Option<u16> {
        self.status
    }

    /// Whether another attempt could succeed. Only rate limits and server
    /// faults qualify: a client error repeats identically forever.
    fn retryable(&self) -> bool {
        matches!(self.status, Some(429) | Some(500..=599))
    }

    /// A transport failure: no reply, so no status. Public because the one
    /// [`Model`] implementation outside this crate — the scripted double in
    /// `tests/support` — has to report one.
    pub fn transport(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            status: None,
        }
    }

    fn answered(status: Option<u16>, message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            status,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    User(String),
    Assistant { text: String, calls: Vec<Call> },
    ToolResults(Vec<ToolOutcome>),
}

#[derive(Debug, Clone)]
pub struct Turn {
    pub text: String,
    pub reasoning: Option<String>,
    pub calls: Vec<Call>,
    pub model: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    Text(String),
    Reasoning(String),
}

/// Where a streamed turn reports its fragments as they arrive. Returning
/// `false` stops the stream: the loop does that when a cancel is raised.
pub trait DeltaSink {
    fn delta(&mut self, delta: Delta) -> bool;
}

/// `Send`, because the agent that holds one outlives a single prompt: a front
/// end keeps its conversation on the thread that runs prompts.
pub trait Model: Send {
    fn turn(
        &self,
        history: &[Message],
        tools: &[ToolSpec],
        thinking: bool,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Turn, Error>;
}

#[derive(Debug, Clone)]
pub struct Client {
    provider: crate::config::Provider,
    api_key: String,
    base_url: Option<String>,
    model: String,
    extra_headers: Vec<(String, String)>,
    /// The preamble every turn carries, composed once by the session
    /// (`crate::preamble`): the stance, the workspace this run stands in — a
    /// fact only the run has — and the repository's instructions after them.
    /// Settled when the client is built, because neither a file read once at
    /// boot nor a path the run started from changes on a later turn.
    preamble: crate::preamble::Preamble,
}

impl Client {
    pub fn new(config: &Config, preamble: crate::preamble::Preamble) -> Client {
        Client {
            provider: config.provider,
            api_key: config.api_key.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            extra_headers: config.extra_headers.clone(),
            preamble,
        }
    }

    pub(crate) fn preamble(&self) -> &str {
        self.preamble.as_str()
    }

    pub fn provider(&self) -> crate::config::Provider {
        self.provider
    }

    fn redact(&self, error: Error) -> Error {
        if self.api_key.is_empty() {
            return error;
        }
        Error {
            message: error.message.replace(&self.api_key, REDACTED),
            status: error.status,
        }
    }

    pub(crate) fn parts(&self) -> (&str, Option<&str>, &str) {
        (&self.api_key, self.base_url.as_deref(), &self.model)
    }

    pub(crate) fn extra_headers(&self) -> &[(String, String)] {
        &self.extra_headers
    }
}

/// A sink that remembers whether any fragment reached it, so a retry never
/// repeats fragments a front end has already shown.
struct RetryGuard<'a> {
    inner: &'a mut dyn DeltaSink,
    streamed: bool,
}

impl DeltaSink for RetryGuard<'_> {
    fn delta(&mut self, delta: Delta) -> bool {
        self.streamed = true;
        self.inner.delta(delta)
    }
}

impl Model for Client {
    fn turn(
        &self,
        history: &[Message],
        tools: &[ToolSpec],
        thinking: bool,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Turn, Error> {
        let mut attempt = 1;
        loop {
            let mut guard = RetryGuard {
                inner: &mut *deltas,
                streamed: false,
            };
            let outcome = rig_wire::turn(self, history, tools, thinking, &mut guard);
            match outcome {
                Ok(turn) => return Ok(turn),
                // Fragments already shown are not shown twice: a retry after
                // the stream opened would repeat them.
                Err(error) if error.retryable() && attempt < MAX_ATTEMPTS && !guard.streamed => {
                    std::thread::sleep(BASE_BACKOFF * 2u32.pow(attempt - 1));
                    attempt += 1;
                }
                Err(error) => return Err(self.redact(error)),
            }
        }
    }
}
