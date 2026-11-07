//! The outcome is what a batch run hands whatever scripted it: §15's exit code,
//! and — with `--format json` — one compact object on one line whose fields
//! arrive in a fixed order and are only ever appended to.
//!
//! It is built from the run and from the handful of facts a run does not hold
//! itself: the session it was recorded under, the model it asked, how its
//! commands were confined, how long it took, and the tokens the wire reported.
//! Nothing here decides anything a run has already decided — the stop reason is
//! read off the run, and the exit code is [`Stop::exit_code`] and never a second
//! mapping — so a script cannot read two opinions about how a run ended (§15,
//! ADR-0006).
//!
//! Absence is the rule for a field the run knows nothing about: a failed run has
//! no answer and no `answer` field, and a wire that reported no tokens leaves
//! `usage` out entirely. `null` is never written, because a script cannot tell
//! "nothing" from "nothing known" once both are the same value.

use std::time::Duration;

use serde::{Serialize, Serializer};

use crate::policy::Enforcement;
use crate::records::Usage;
use crate::run::{Executor, Responder, Run, Stop};

/// The facts a report is given that the run itself does not hold: the session, the
/// model, the enforcement, the wall clock and the tokens.
///
/// Every one of them is an input, so a report is written in a test with no state
/// home, no clock and no wire.
pub struct Facts<'a> {
    /// The GUID the session directory is named by, which `-v` ends with too.
    pub session: &'a str,
    /// The served name the run asked for.
    pub model: &'a str,
    /// How the run's commands were confined.
    pub enforcement: Enforcement,
    /// How long the run took, from the first request to the stop reason.
    pub duration: Duration,
    /// What the run's asks spent, as the record summed it.
    pub usage: Usage,
}

/// What one run left behind, as §15's object holds it.
///
/// The fields are declared in the order the spec names them, because `Serialize`
/// writes a struct in declaration order and that order is the contract: a later
/// field is appended and never inserted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Outcome {
    /// The text the run answered with, and nothing at all when it did not answer:
    /// the one field a failed or cancelled run has no value for.
    #[serde(skip_serializing_if = "Option::is_none")]
    answer: Option<String>,
    /// The GUID the session directory is named by.
    session: String,
    /// How many turns the run started.
    turns: u32,
    /// Why it ended, from the closed set of five.
    #[serde(serialize_with = "stopped_word")]
    stopped: Stop,
    /// The served name it asked.
    model: String,
    /// How long it took, in milliseconds.
    duration_ms: u64,
    /// How many tool calls it made, counted rather than guessed at.
    tool_calls: u64,
    /// What its asks spent, and nothing at all when no wire reported a token.
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Usage>,
    /// How its commands were confined: `bubblewrap` or `unsafe`.
    enforcement: Enforcement,
    /// The command wall clock the run spent, in milliseconds.
    command_ms: u64,
    /// The bytes of command output the run fed back.
    output_bytes: u64,
}

impl Outcome {
    /// The report `run` leaves, or `None` when it is still running: a report is
    /// what a stopped run leaves, and a running one has no word to say about how
    /// it will end (§4).
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

    /// The answer a batch front end prints in text mode: the run's own text, and
    /// nothing at all for every other stop reason (§15).
    pub fn answer(&self) -> Option<&str> {
        self.answer.as_deref()
    }

    /// Why the run ended.
    pub fn stopped(&self) -> &Stop {
        &self.stopped
    }

    /// How many turns it started.
    pub fn turns(&self) -> u32 {
        self.turns
    }

    /// How many tool calls it made.
    pub fn tool_calls(&self) -> u64 {
        self.tool_calls
    }

    /// The GUID the session directory is named by.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// The exit code §15 gives this run: the stop reason's own, from the one
    /// mapping table, so a script never gets a second opinion (ADR-0006).
    pub fn exit_code(&self) -> u8 {
        self.stopped.exit_code()
    }

    /// The report as the one compact line `--format json` writes, with no newline
    /// of its own: the newline belongs to whoever writes the object out.
    pub fn json(&self) -> String {
        serde_json::to_string(self).expect("a report of plain facts serializes")
    }
}

/// The stop reason, as the word the JSON and the record both use.
fn stopped_word<S: Serializer>(stop: &Stop, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(stop.word())
}

/// The usage a report can carry, and nothing when no wire reported a token: a
/// count nobody gave is left out rather than written as zero (§15).
fn reported(usage: Usage) -> Option<Usage> {
    let reported = usage.input.is_some()
        || usage.output.is_some()
        || usage.total.is_some()
        || usage.cached_input.is_some()
        || usage.reasoning.is_some();
    reported.then_some(usage)
}

/// A span as the whole milliseconds a report counts in, saturating rather than
/// wrapping: no report may say a run was quicker than it was.
fn millis(span: Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}
