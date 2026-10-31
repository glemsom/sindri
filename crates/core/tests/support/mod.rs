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
use std::time::Duration;

use sindri_core::agent::{Agent, Event, Report};
use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, Provider};
use sindri_core::exec::{BashExecutor, Outcome, ToolResult};
use sindri_core::provider::{Call, Delta, DeltaSink, Error, Message, Model, ToolSpec, Turn, Usage};
use sindri_core::record::Recorder;
use sindri_core::sandbox::{Enforcement, NetworkPosture, SandboxPolicy, WritableSet};
use sindri_core::session::Env;
use tempfile::TempDir;

pub fn fixtures() -> (TempDir, PathBuf, SandboxPolicy) {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(session_temp.join("home")).expect("session temp");
    let policy = policy(&workspace, &session_temp);
    (workspace, session_temp, policy)
}

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

/// A credential shaped like a real one but belonging to nothing.
pub const KEY: &str = "sk-ant-test-not-a-real-key";

pub fn host() -> (TempDir, Env) {
    let root = TempDir::new().expect("host");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let env = Env {
        workspace: workspace.canonicalize().expect("canonical workspace"),
        search_path: "/usr/bin:/bin".to_string(),
        state: root.path().join("state"),
    };
    (root, env)
}

pub fn config(extras: &[&Path]) -> Config {
    Config {
        provider: Provider::Anthropic,
        model: "claude-mock".to_string(),
        api_key: KEY.to_string(),
        base_url: None,
        max_turns: 8,
        timeout: Duration::from_secs(30),
        thinking: false,
        extra_writable: extras.iter().map(|path| path.to_path_buf()).collect(),
        enforcement: Enforcement::Bubblewrap,
        extra_headers: Vec::new(),
        refused_repo_config: None,
    }
}

/// A tree with one commit, so the probe has a branch and a commit to name.
pub fn commit(dir: &Path) -> String {
    let author = [
        "-c",
        "user.name=sindri",
        "-c",
        "user.email=sindri@example.invalid",
    ];
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(author)
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q", "--initial-branch=main"]);
    std::fs::write(dir.join("readme"), "one line\n").expect("a file to commit");
    git(&["add", "readme"]);
    git(&["commit", "-q", "-m", "the first commit"]);
    git(&["rev-parse", "HEAD"])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interrupt {
    #[default]
    Never,
    BeforeFirstTurn,
    DuringFirstTurn,
    DuringFirstCommand,
}

pub struct Scripted {
    replies: Mutex<Vec<ToolResult>>,
    seen: Arc<Mutex<Vec<String>>>,
    /// Raised when the first command runs, and no result is returned: exactly
    /// what a killed command group looks like to the loop.
    interrupt: Option<Cancel>,
}

impl Scripted {
    pub fn new(replies: Vec<ToolResult>, seen: Arc<Mutex<Vec<String>>>) -> Self {
        Scripted::interrupting(replies, seen, None)
    }

    fn interrupting(
        replies: Vec<ToolResult>,
        seen: Arc<Mutex<Vec<String>>>,
        interrupt: Option<Cancel>,
    ) -> Self {
        Scripted {
            replies: Mutex::new(replies),
            seen,
            interrupt,
        }
    }

    pub fn silent() -> Arc<dyn BashExecutor> {
        Arc::new(Scripted::new(Vec::new(), Arc::new(Mutex::new(Vec::new()))))
    }
}

impl BashExecutor for Scripted {
    fn execute(&self, script: &str) -> Option<ToolResult> {
        self.seen.lock().expect("seen").push(script.to_string());
        if let Some(cancel) = &self.interrupt {
            cancel.trigger();
            return None;
        }
        let mut replies = self.replies.lock().expect("replies");
        if replies.is_empty() {
            return Some(exited(""));
        }
        Some(replies.remove(0))
    }
}

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

pub struct ScriptedModel {
    replies: Mutex<Vec<Turn>>,
    seen: Arc<Mutex<Vec<Vec<Message>>>>,
    pub thinking: Arc<Mutex<Vec<bool>>>,
    /// Raised while the first request is in flight, which then fails: a request
    /// cut short by Ctrl-C looks exactly like this to the loop.
    interrupt: Option<Cancel>,
    stream: bool,
}

impl ScriptedModel {
    pub fn new(
        replies: Vec<Turn>,
        seen: Arc<Mutex<Vec<Vec<Message>>>>,
        thinking: Arc<Mutex<Vec<bool>>>,
    ) -> Self {
        ScriptedModel {
            replies: Mutex::new(replies),
            seen,
            thinking,
            interrupt: None,
            stream: false,
        }
    }

    fn interrupting(mut self, interrupt: Option<Cancel>) -> Self {
        self.interrupt = interrupt;
        self
    }

    fn streaming(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }
}

fn fragmented(text: &str) -> Vec<String> {
    text.split_inclusive(' ').map(str::to_string).collect()
}

impl Model for ScriptedModel {
    fn turn(
        &self,
        history: &[Message],
        _tools: &[ToolSpec],
        thinking: bool,
        deltas: &mut dyn DeltaSink,
    ) -> Result<Turn, Error> {
        self.seen.lock().expect("seen").push(history.to_vec());
        self.thinking.lock().expect("thinking").push(thinking);
        if let Some(cancel) = &self.interrupt {
            cancel.trigger();
            return Err(Error::transport("the request was cut short"));
        }
        let mut replies = self.replies.lock().expect("replies");
        assert!(
            !replies.is_empty(),
            "the loop asked for more turns than the script holds"
        );
        let reply = replies.remove(0);
        if self.stream {
            let fragments = reply
                .reasoning
                .iter()
                .flat_map(|reason| fragmented(reason))
                .map(Delta::Reasoning)
                .chain(fragmented(&reply.text).into_iter().map(Delta::Text));
            for delta in fragments {
                assert!(deltas.delta(delta), "the sink stayed open");
            }
        }
        Ok(reply)
    }
}

pub struct Harness {
    agent: Agent,
    pub seen: Arc<Mutex<Vec<Vec<Message>>>>,
    seen_scripts: Arc<Mutex<Vec<String>>>,
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
        Harness::build(
            replies,
            commands,
            max_turns,
            thinking,
            Interrupt::Never,
            false,
        )
    }

    pub fn streaming(
        replies: Vec<Turn>,
        commands: Vec<ToolResult>,
        max_turns: u32,
        thinking: bool,
    ) -> Self {
        Harness::build(
            replies,
            commands,
            max_turns,
            thinking,
            Interrupt::Never,
            true,
        )
    }

    pub fn interrupted(
        replies: Vec<Turn>,
        commands: Vec<ToolResult>,
        interrupt: Interrupt,
    ) -> Self {
        Harness::build(replies, commands, 8, false, interrupt, false)
    }

    fn build(
        replies: Vec<Turn>,
        commands: Vec<ToolResult>,
        max_turns: u32,
        thinking: bool,
        interrupt: Interrupt,
        stream: bool,
    ) -> Self {
        let (workspace, _session_temp, policy) = fixtures();
        let seen_scripts = Arc::new(Mutex::new(Vec::new()));
        let cancel = Cancel::new();
        if interrupt == Interrupt::BeforeFirstTurn {
            cancel.trigger();
        }
        let mid_command = (interrupt == Interrupt::DuringFirstCommand).then(|| cancel.clone());
        let executor = Arc::new(Scripted::interrupting(
            commands,
            Arc::clone(&seen_scripts),
            mid_command,
        ));
        let tool = sindri_core::agent::BashTool::new(&policy, executor);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let told = Arc::new(Mutex::new(Vec::new()));
        let model = ScriptedModel::new(replies, Arc::clone(&seen), Arc::clone(&told))
            .interrupting((interrupt == Interrupt::DuringFirstTurn).then(|| cancel.clone()))
            .streaming(stream);
        let (sender, events) = std::sync::mpsc::channel();

        let mut agent = Agent::new(Box::new(model), tool, max_turns, thinking);
        agent.set_events(sender);
        agent.set_cancel(cancel);
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

    /// Attaches a recorder, so the run writes the record a test reads back from
    /// the directory: the loop hands the recorder its facts and never reads it.
    pub fn set_recorder(&mut self, recorder: Arc<Recorder>) {
        self.agent.set_recorder(recorder);
    }

    pub fn forget(&mut self) {
        self.agent.forget();
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

pub fn calls(calls: Vec<Call>) -> Turn {
    Turn {
        text: String::new(),
        reasoning: None,
        calls,
        model: Some("claude-mock".to_string()),
        usage: usage(20, 5),
    }
}
