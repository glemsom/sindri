//! `--format json` writes one compact object on one line whose fields arrive in a fixed
//! order and are only ever appended to. A field the run knows nothing about is left out
//! rather than written as `null`: a script cannot tell the two apart.

use std::time::Duration;

use serde::{Serialize, Serializer};

use crate::policy::Enforcement;
use crate::records::Usage;
use crate::run::{Executor, Responder, Run, Stop};

pub struct Facts<'a> {
    pub session: &'a str,
    pub model: &'a str,
    pub enforcement: Enforcement,
    pub duration: Duration,
    pub usage: Usage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Outcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    answer: Option<String>,
    session: String,
    turns: u32,
    #[serde(serialize_with = "stopped_word")]
    stopped: Stop,
    model: String,
    duration_ms: u64,
    tool_calls: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Usage>,
    enforcement: Enforcement,
    command_ms: u64,
    output_bytes: u64,
}

impl Outcome {
    pub fn of<R: Responder, A: Executor>(run: &Run<R, A>, facts: Facts<'_>) -> Option<Outcome> {
        let stopped = run.state().stop()?.clone();
        let tool_calls = run
            .history()
            .iter()
            .map(|exchange| u64::try_from(exchange.calls.len()).unwrap_or(u64::MAX))
            .fold(0u64, u64::saturating_add);
        Some(Outcome {
            answer: run.answer().map(str::to_owned),
            session: facts.session.to_owned(),
            turns: run.turns(),
            stopped,
            model: facts.model.to_owned(),
            duration_ms: millis(facts.duration),
            tool_calls,
            usage: reported(facts.usage),
            enforcement: facts.enforcement,
            command_ms: millis(run.spent().clock()),
            output_bytes: run.spent().output(),
        })
    }

    pub fn answer(&self) -> Option<&str> {
        self.answer.as_deref()
    }

    pub fn stopped(&self) -> &Stop {
        &self.stopped
    }

    pub fn turns(&self) -> u32 {
        self.turns
    }

    pub fn tool_calls(&self) -> u64 {
        self.tool_calls
    }

    pub fn session(&self) -> &str {
        &self.session
    }

    pub fn exit_code(&self) -> u8 {
        self.stopped.exit_code()
    }

    /// The one compact line `--format json` writes, with no newline: that belongs to whoever
    /// writes it out.
    pub fn json(&self) -> String {
        serde_json::to_string(self).expect("a report of plain facts serializes")
    }
}

fn stopped_word<S: Serializer>(stop: &Stop, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(stop.word())
}

fn reported(usage: Usage) -> Option<Usage> {
    let reported = usage.input.is_some()
        || usage.output.is_some()
        || usage.total.is_some()
        || usage.cached_input.is_some()
        || usage.reasoning.is_some();
    reported.then_some(usage)
}

/// Saturating rather than wrapping: no report may say a run was quicker than it was.
fn millis(span: Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}
