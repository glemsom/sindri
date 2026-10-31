//! The one tool the model gets: `bash`. Its description is derived from the
//! sandbox policy, never written by hand, so the model is told exactly the
//! boundary that is enforced.

use std::sync::Arc;

use crate::exec::{BashExecutor, Outcome, ToolResult};
use crate::provider::{Call, ToolFacts, ToolOutcome, ToolSpec};
use crate::sandbox::SandboxPolicy;

pub struct BashTool {
    spec: ToolSpec,
    executor: Arc<dyn BashExecutor>,
}

impl BashTool {
    pub fn new(policy: &SandboxPolicy, executor: Arc<dyn BashExecutor>) -> Self {
        BashTool {
            spec: describe(policy),
            executor,
        }
    }

    pub fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    pub fn call(&self, call: &Call) -> Option<ToolOutcome> {
        let script = match call.command() {
            Ok(script) => script,
            Err(reason) => return Some(ToolOutcome::malformed(call, &reason)),
        };
        let result = self.executor.execute(&script)?;
        let failed = matches!(result.outcome, Outcome::SandboxError { .. });
        let content = render(&result);
        Some(ToolOutcome::new(call, content, failed).with_facts(facts(&result)))
    }
}

/// The policy's own description, plus how a result comes back: one derivation,
/// so the model is never told a comfortable lie about its boundary.
fn describe(policy: &SandboxPolicy) -> ToolSpec {
    let description = format!(
        "Run a bash command on the user's machine. It is the only tool, so pipelines and small \
         scripts are how you explore and act.\n\n{}\n\
         How a result comes back: the outcome (`exited`, `timed_out` or `sandbox_error`), the \
         `exit_code` when the command exited, `duration_ms`, a `truncated` flag, then `stdout` \
         and `stderr`. Over-long output is cut head and tail; the full text is left in a spill \
         file whose path the result names. `sandbox_error` means the sandbox never started, so \
         no command ran. The status of the last command in the script is the status you get; \
         `set -e` and `set -o pipefail` work as in any bash script.",
        policy.describe()
    );
    ToolSpec {
        name: "bash".to_string(),
        description,
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash script to run.",
                },
            },
            "required": ["command"],
        }),
    }
}

/// The structured result as text, every field named so the
/// model can act on it rather than guess. A field with nothing to say is said
/// absent rather than left out, so the shape never depends on the outcome.
fn render(result: &ToolResult) -> String {
    let facts = facts(result);
    let mut out = format!(
        "outcome: {}\n",
        facts.outcome.as_deref().unwrap_or_default()
    );
    if let Some(exit_code) = facts.exit_code {
        out.push_str(&format!("exit_code: {exit_code}\n"));
    }
    // The sandbox's own words, which are prose and so have no field of their
    // own: the one thing `facts` does not carry.
    if let Outcome::SandboxError { detail } = &result.outcome {
        out.push_str(&format!("detail: {detail}\n"));
    }
    out.push_str(&format!("duration_ms: {}\n", result.duration_ms));
    out.push_str(&format!("truncated: {}\n", result.truncated));
    match facts.spill.as_slice() {
        [] => out.push_str("spill: none\n"),
        paths => {
            for path in paths {
                out.push_str(&format!("spill: {path}\n"));
            }
        }
    }
    out.push_str(&format!("stdout:\n{}", result.stdout));
    if !result.stdout.is_empty() && !result.stdout.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!("stderr:\n{}", result.stderr));
    if !result.stderr.is_empty() && !result.stderr.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The same facts `render` puts into words, as fields: one result, two
/// spellings, derived once here so a reader of the record never parses the text
/// a model was shown, and the two cannot disagree.
fn facts(result: &ToolResult) -> ToolFacts {
    let (outcome, exit_code) = spelled(&result.outcome);
    ToolFacts {
        outcome: Some(outcome.to_string()),
        exit_code,
        duration_ms: Some(result.duration_ms),
        truncated: result.truncated,
        // The paths the sandbox named, not a host path invented at read time.
        spill: result
            .spill
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
    }
}

/// An outcome as its one word and its exit status: the single exhaustive match,
/// so a fourth outcome is a compile error here rather than a fact the record
/// quietly stops carrying.
fn spelled(outcome: &Outcome) -> (&'static str, Option<i32>) {
    match outcome {
        Outcome::Exited { exit_code } => ("exited", Some(*exit_code)),
        Outcome::TimedOut => ("timed_out", None),
        Outcome::SandboxError { .. } => ("sandbox_error", None),
    }
}
