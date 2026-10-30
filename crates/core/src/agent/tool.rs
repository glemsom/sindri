//! The one tool the model gets: `bash`. Its description is derived from the
//! sandbox policy, never written by hand, so the model is told exactly the
//! boundary that is enforced.

use std::sync::Arc;

use crate::exec::{BashExecutor, Outcome, ToolResult};
use crate::provider::{Call, ToolOutcome, ToolSpec};
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
        Some(ToolOutcome::new(call, render(&result), failed))
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
    let mut out = String::new();
    match &result.outcome {
        Outcome::Exited { exit_code } => {
            out.push_str("outcome: exited\n");
            out.push_str(&format!("exit_code: {exit_code}\n"));
        }
        Outcome::TimedOut => out.push_str("outcome: timed_out\n"),
        Outcome::SandboxError { detail } => {
            out.push_str("outcome: sandbox_error\n");
            out.push_str(&format!("detail: {detail}\n"));
        }
    }
    out.push_str(&format!("duration_ms: {}\n", result.duration_ms));
    out.push_str(&format!("truncated: {}\n", result.truncated));
    match result.spill.as_slice() {
        [] => out.push_str("spill: none\n"),
        spill => {
            for path in spill {
                out.push_str(&format!("spill: {}\n", path.display()));
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
