//! The machine-readable output contract (ADR-0004): one run as exactly one
//! compact JSON object on one line. The field order below *is* the contract —
//! later fields append and this order never changes, so a parser written
//! against today's shape keeps working tomorrow. A field nobody reported is
//! absent, never null, so a script can tell "not available" from "empty".

use serde::Serialize;

use crate::agent::Report;
use crate::provider::Usage;

/// One run, shaped for the contract. Serde writes a struct's fields in the
/// order they are declared here, which is how the order is fixed.
#[derive(Serialize)]
struct Run<'a> {
    /// The final assistant text of the last turn.
    answer: &'a str,
    /// The session GUID the run was recorded under (ADR-0006).
    session: &'a str,
    /// How many round trips the run took.
    turns: u32,
    /// Why the loop ended: `final`, `max_turns`, `cancelled` or `error`.
    stopped: &'static str,
    /// The model the provider said answered, when it said.
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    /// How long the whole run took, in milliseconds.
    duration_ms: u64,
    /// How many tool calls ran, or were answered as unusable.
    tool_calls: u32,
    /// What the run was billed for; absent when the provider reported nothing.
    #[serde(skip_serializing_if = "Usage::is_empty")]
    usage: &'a Usage,
}

/// The run as the contract writes it: one compact object, no newline.
///
/// Infallible in practice, because every field is a string or an integer and
/// `serde_json` cannot refuse those; the empty fallback exists only so a
/// finished run never panics on an impossible failure.
pub fn to_json(report: &Report, session: &str) -> String {
    let run = Run {
        answer: report.answer.as_str(),
        session,
        turns: report.turns,
        stopped: report.stopped.as_str(),
        model: report.model.as_deref(),
        // A run cannot last longer than a `u64` of milliseconds.
        duration_ms: u64::try_from(report.duration.as_millis()).unwrap_or(u64::MAX),
        tool_calls: report.tool_calls,
        usage: &report.usage,
    };
    // Every field is a string or an integer, which `serde_json` cannot refuse.
    serde_json::to_string(&run).unwrap_or_default()
}
