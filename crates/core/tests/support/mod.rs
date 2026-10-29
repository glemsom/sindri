//! Fixtures shared by the core's process-free tests: a sandbox policy that
//! points at a temporary workspace, and an executor that answers from a script
//! so a test about the loop or the tool never needs bubblewrap.

#![allow(
    dead_code,
    reason = "each test binary uses a different slice of the fixtures"
)]

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use sindri_core::agent::{Agent, Event, Report};
use sindri_core::exec::{BashExecutor, Outcome, ToolResult};
use sindri_core::provider::{Call, Error, Message, Model, ToolSpec, Turn, Usage};
use sindri_core::sandbox::{Enforcement, NetworkPosture, SandboxPolicy, WritableSet};
use tempfile::TempDir;

/// A workspace with a session temp, and the policy a run would build from it.
pub fn fixtures() -> (TempDir, PathBuf, SandboxPolicy) {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(session_temp.join("home")).expect("session temp");
    let policy = policy(&workspace, &session_temp);
    (workspace, session_temp, policy)
}

/// The policy a run builds: the workspace writable, everything else read-only.
pub fn policy(workspace: &TempDir, session_temp: &Path) -> SandboxPolicy {
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

/// An executor that answers from a script and remembers what it was asked to
/// run. An exhausted script answers a bare success.
pub struct Scripted {
    replies: Mutex<Vec<ToolResult>>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Scripted {
    pub fn new(replies: Vec<ToolResult>, seen: Arc<Mutex<Vec<String>>>) -> Self {
        Scripted {
            replies: Mutex::new(replies),
            seen,
        }
    }

    /// An executor that was never asked to run anything.
    pub fn silent() -> Arc<dyn BashExecutor> {
        Arc::new(Scripted::new(Vec::new(), Arc::new(Mutex::new(Vec::new()))))
    }
}

impl BashExecutor for Scripted {
    fn execute(&self, script: &str) -> ToolResult {
        self.seen.lock().expect("seen").push(script.to_string());
        let mut replies = self.replies.lock().expect("replies");
        if replies.is_empty() {
            return exited("");
        }
        replies.remove(0)
    }
}

/// A result for a command that ran and exited successfully.
pub fn exited(stdout: &str) -> ToolResult {
    ToolResult {
        outcome: Outcome::Exited { exit_code: 0 },
        stdout: stdout.to_string(),
        stderr: String::new(),
        truncated: false,
        spill: Vec::new(),
        duration_ms: 1,
    }
}

/// A model that answers from a script and records every conversation it was
/// shown, so a test can prove what the loop fed back.
pub struct ScriptedModel {
    replies: Mutex<Vec<Turn>>,
    seen: Arc<Mutex<Vec<Vec<Message>>>>,
    /// Whether the run paid for reasoning, one entry per turn.
    pub thinking: Arc<Mutex<Vec<bool>>>,
}

impl ScriptedModel {
    /// A model scripted with `replies`, writing what it saw into `seen` and
    /// what it was told about reasoning into `thinking`.
    pub fn new(
        replies: Vec<Turn>,
        seen: Arc<Mutex<Vec<Vec<Message>>>>,
        thinking: Arc<Mutex<Vec<bool>>>,
    ) -> Self {
        ScriptedModel {
            replies: Mutex::new(replies),
            seen,
            thinking,
        }
    }
}

impl Model for ScriptedModel {
    fn turn(
        &self,
        history: &[Message],
        _tools: &[ToolSpec],
        thinking: bool,
    ) -> Result<Turn, Error> {
        self.seen.lock().expect("seen").push(history.to_vec());
        self.thinking.lock().expect("thinking").push(thinking);
        let mut replies = self.replies.lock().expect("replies");
        assert!(
            !replies.is_empty(),
            "the loop asked for more turns than the script holds"
        );
        Ok(replies.remove(0))
    }
}

/// The loop under test, with a model that answers `replies`, an executor that
/// runs `commands`, and the channel its events land on. The workspace lives as
/// long as the harness, because the policy points at it.
pub struct Harness {
    agent: Agent,
    /// Every conversation the model was shown, in order.
    pub seen: Arc<Mutex<Vec<Vec<Message>>>>,
    seen_scripts: Arc<Mutex<Vec<String>>>,
    /// Whether the run paid for reasoning, one entry per turn.
    pub thinking: Arc<Mutex<Vec<bool>>>,
    events: Receiver<Event>,
    _workspace: TempDir,
}

impl Harness {
    pub fn new(
        replies: Vec<Turn>,
        commands: Vec<ToolResult>,
        max_turns: u32,
        thinking: bool,
    ) -> Self {
        let (workspace, _session_temp, policy) = fixtures();
        let seen_scripts = Arc::new(Mutex::new(Vec::new()));
        let executor = Arc::new(Scripted::new(commands, Arc::clone(&seen_scripts)));
        let tool = sindri_core::agent::BashTool::new(&policy, executor);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let told = Arc::new(Mutex::new(Vec::new()));
        let model = ScriptedModel::new(replies, Arc::clone(&seen), Arc::clone(&told));
        let (sender, events) = std::sync::mpsc::channel();

        let mut agent = Agent::new(Box::new(model), tool, max_turns, thinking);
        agent.set_events(sender);
        Harness {
            agent,
            seen,
            seen_scripts,
            thinking: told,
            events,
            _workspace: workspace,
        }
    }

    pub fn run(&mut self, prompt: &str) -> Report {
        self.agent.run(prompt).expect("the loop completed")
    }

    pub fn events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    pub fn scripts(&self) -> Vec<String> {
        self.seen_scripts.lock().expect("scripts").clone()
    }

    pub fn conversations(&self) -> Vec<Vec<Message>> {
        self.seen.lock().expect("seen").clone()
    }
}

pub fn usage(input: u64, output: u64) -> Usage {
    Usage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: None,
        reasoning_tokens: None,
        total_tokens: None,
    }
}

pub fn answer(text: &str) -> Turn {
    Turn {
        text: text.to_string(),
        reasoning: None,
        calls: Vec::new(),
        model: Some("claude-mock".to_string()),
        usage: usage(11, 7),
    }
}

pub fn call(id: &str, command: &str) -> Call {
    Call::new(
        id,
        "bash",
        Ok(serde_json::Map::from_iter([(
            "command".to_string(),
            serde_json::json!(command),
        )])),
    )
}

/// A turn that asks for tool calls and says nothing else.
pub fn calls(calls: Vec<Call>) -> Turn {
    Turn {
        text: String::new(),
        reasoning: None,
        calls,
        model: Some("claude-mock".to_string()),
        usage: usage(20, 5),
    }
}
