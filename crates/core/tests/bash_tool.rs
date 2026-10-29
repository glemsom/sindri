#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

//! The `bash` tool's contract with the model: one tool, a description derived
//! from the sandbox policy rather than written by hand (ADR-0002), and a
//! structured result rendered with every field ADR-0007 promises.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sindri_core::agent::BashTool;
use sindri_core::exec::{BashExecutor, Outcome, ToolResult};
use sindri_core::provider::Call;
use sindri_core::sandbox::{Enforcement, NetworkPosture, SandboxPolicy, WritableSet};
use tempfile::TempDir;

/// A port implementation that answers from a script and remembers what it saw,
/// so a test about the tool never needs bubblewrap.
struct Scripted {
    replies: Mutex<Vec<ToolResult>>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Scripted {
    fn new(replies: Vec<ToolResult>, seen: Arc<Mutex<Vec<String>>>) -> Self {
        Scripted {
            replies: Mutex::new(replies),
            seen,
        }
    }

    fn silent(seen: Arc<Mutex<Vec<String>>>) -> Arc<dyn BashExecutor> {
        Arc::new(Scripted::new(Vec::new(), seen))
    }
}

impl BashExecutor for Scripted {
    fn execute(&self, script: &str) -> ToolResult {
        self.seen.lock().expect("seen").push(script.to_string());
        self.replies.lock().expect("replies").remove(0)
    }
}

fn policy(workspace: &TempDir, session_temp: &Path) -> SandboxPolicy {
    let writable = WritableSet::new([workspace.path().to_path_buf()]).expect("writable set");
    SandboxPolicy::new(
        writable,
        session_temp.to_path_buf(),
        "/usr/bin:/bin".to_string(),
        NetworkPosture::Shared,
        Enforcement::Bubblewrap,
    )
    .expect("policy")
}

fn fixtures() -> (TempDir, PathBuf, SandboxPolicy) {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(session_temp.join("home")).expect("session temp");
    let policy = policy(&workspace, &session_temp);
    (workspace, session_temp, policy)
}

#[test]
fn the_model_gets_exactly_one_tool_named_bash_described_by_the_policy() {
    let (workspace, _session_temp, policy) = fixtures();
    let tool = BashTool::new(&policy, Scripted::silent(Arc::new(Mutex::new(Vec::new()))));

    let spec = tool.spec();
    assert_eq!(spec.name, "bash", "the only tool is bash");
    let description = &spec.description;
    assert!(
        description.contains(&policy.describe()),
        "the description is generated from the policy, not written by hand: {description}"
    );
    assert!(
        description.contains(&workspace.path().display().to_string()),
        "the model is told which paths it may write: {description}"
    );
    assert_eq!(spec.parameters["type"], "object");
    assert_eq!(spec.parameters["required"][0], "command");
}

#[test]
fn a_tool_result_reaches_the_model_with_every_field_it_needs() {
    let (workspace, session_temp, policy) = fixtures();
    let spill = session_temp.join("spill/bash-1.out");
    std::fs::create_dir_all(spill.parent().expect("spill dir")).expect("spill dir");
    std::fs::write(&spill, "the whole story").expect("spill");

    let seen = Arc::new(Mutex::new(Vec::new()));
    let executor = Arc::new(Scripted::new(
        vec![ToolResult {
            outcome: Outcome::Exited { exit_code: 7 },
            stdout: "twelve files".to_string(),
            stderr: "a warning".to_string(),
            truncated: true,
            spill: vec![spill.clone()],
            duration_ms: 42,
        }],
        Arc::clone(&seen),
    ));
    let tool = BashTool::new(&policy, executor);

    let call = Call::new(
        "call_1",
        "bash",
        Ok(serde_json::Map::from_iter([(
            "command".to_string(),
            serde_json::json!("ls | wc -l"),
        )])),
    );
    let outcome = tool.call(&call);

    assert_eq!(*seen.lock().expect("seen"), vec!["ls | wc -l"]);
    assert_eq!(outcome.call_id, "call_1");
    assert!(
        !outcome.is_error,
        "a non-zero exit is a result, not a tool error"
    );
    let content = &outcome.content;
    for expected in [
        "outcome: exited",
        "exit_code: 7",
        "duration_ms: 42",
        "truncated: true",
        "stdout:\ntwelve files",
        "stderr:\na warning",
    ] {
        assert!(
            content.contains(expected),
            "the model is told `{expected}`: {content}"
        );
    }
    assert!(
        content.contains(&spill.display().to_string()),
        "the model is told where the untruncated text went: {content}"
    );
    let _ = workspace;
}
