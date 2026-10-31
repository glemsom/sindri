//! The machine-readable output contract: one run as exactly one
//! compact JSON object on one line. The field order below *is* the contract —
//! later fields append and this order never changes, so a parser written
//! against today's shape keeps working tomorrow. A field nobody reported is
//! absent, never null, so a script can tell "not available" from "empty".

use serde::Serialize;

use crate::agent::Report;
use crate::provider::Usage;
use crate::sandbox::Enforcement;

/// One run, shaped for the contract. Serde writes a struct's fields in the
/// order they are declared here, which is how the order is fixed.
#[derive(Serialize)]
struct Run<'a> {
    answer: &'a str,
    session: &'a str,
    turns: u32,
    stopped: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    duration_ms: u64,
    tool_calls: u32,
    #[serde(skip_serializing_if = "Usage::is_empty")]
    usage: &'a Usage,
    enforcement: &'static str,
}

/// Infallible in practice, because every field is a string or an integer and
/// `serde_json` cannot refuse those; the empty fallback exists only so a
/// finished run never panics on an impossible failure.
pub fn to_json(report: &Report, session: &str, enforcement: Enforcement) -> String {
    let run = Run {
        answer: report.answer.as_str(),
        session,
        turns: report.turns,
        stopped: report.stopped.as_str(),
        model: report.model.as_deref(),
        duration_ms: u64::try_from(report.duration.as_millis()).unwrap_or(u64::MAX),
        tool_calls: report.tool_calls,
        usage: &report.usage,
        enforcement: enforcement.as_str(),
    };
    serde_json::to_string(&run).unwrap_or_default()
}
