//! The tool that runs things: `bash`. Its description is derived from the
//! sandbox policy and the limits in force, never written by hand, so the model
//! is told exactly the boundary that is enforced and the two figures that end
//! its work.

use std::sync::Arc;

use super::Limits;
use crate::exec::{BashExecutor, Outcome, ToolResult};
use crate::provider::{Call, ToolFacts, ToolOutcome, ToolSpec};
use crate::sandbox::SandboxPolicy;

pub struct BashTool {
    spec: ToolSpec,
    executor: Arc<dyn BashExecutor>,
}

impl BashTool {
    pub fn new(policy: &SandboxPolicy, limits: Limits, executor: Arc<dyn BashExecutor>) -> Self {
        BashTool {
            spec: describe(policy, limits),
            executor,
        }
    }

    pub fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    /// The call run. The script is the one the parse already took out of the
    /// arguments, so a call that never parsed never reaches the sandbox.
    pub fn call(&self, call: &Call, script: &str) -> Option<ToolOutcome> {
        let result = self.executor.execute(script)?;
        let failed = matches!(result.outcome, Outcome::SandboxError { .. });
        let content = render(&result);
        Some(ToolOutcome::new(call, content, failed).with_facts(facts(&result)))
    }
}

/// The policy's own description, the limits that end the run and how a result
/// comes back: one derivation, so the model is never told a comfortable lie
/// about its boundary.
fn describe(policy: &SandboxPolicy, limits: Limits) -> ToolSpec {
    // Every word here is re-sent on every turn, so it says only what the
    // model cannot read off a field name or a result it has already seen.
    let description = format!(
        "It is the only tool that runs anything, so pipelines and small scripts are how you \
         explore and act: put the steps of one task in one script, not in three calls.\n\n{}\n\n{}\n\
         Every result is carried in the conversation for the rest of the run and sent again with \
         every later turn, so filter in the script and print only what you concluded; keep \
         intermediates in `$TMPDIR`, which outlives the call.\n\
         How a result comes back: fields are named for what they hold, `exit_code` is absent \
         when the command did not exit, and `retained_bytes` is what the result costs the \
         conversation. Over-long output is cut head and tail and the whole text is in the \
         `spill` file. `sandbox_error` means the sandbox never started, so no command ran.",
        limits.describe(),
        policy.describe()
    );
    ToolSpec {
        name: "bash".to_string(),
        description,
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" },
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
    out.push_str(&format!(
        "retained_bytes: {}\n",
        result.stdout.len() + result.stderr.len()
    ));
    match facts.spill.as_slice() {
        [] => out.push_str("spill: none\n"),
        paths => {
            for path in paths {
                out.push_str(&format!("spill: {path} (grep it from the next script)\n"));
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
