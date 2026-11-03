//! Fixtures shared by the core's process-free tests: a sandbox policy that
//! points at a temporary workspace, and an executor that answers from a script
//! so a test about the loop or the tool never needs bubblewrap.

#![allow(
    dead_code,
    unused_imports,
    reason = "each test binary uses a different slice of the fixtures"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sindri_core::agent::{Activated, Agent, Event, Report, SkillTool, Tools};
use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, Provider};
use sindri_core::exec::{BashExecutor, Outcome, ToolResult};
use sindri_core::provider::{Call, Delta, DeltaSink, Error, Message, Model, ToolSpec, Turn, Usage};
use sindri_core::record::{Recorder, Start};
use sindri_core::sandbox::{Enforcement, Extras, SandboxPolicy, WritableSet};
use sindri_core::session::Env;
use sindri_core::skills::Catalog;
use tempfile::TempDir;

mod batch;
mod git;
pub mod open;
mod replies;
pub mod skills;

pub use replies::{ScriptedModel, answer, call, calls, usage};

pub use batch::Echoing;
pub use git::commit;

/// A record's start block for a test that is not a run: the run's identity, and
/// every field only a live session can probe left empty.
pub fn start(provider: &str, model: &str, enforcement: Enforcement) -> Start {
    Start {
        provider: provider.to_string(),
        model: model.to_string(),
        enforcement,
        policy: None,
        workspace: String::new(),
        git: None,
        instructions: None,
        max_turns: 0,
        thinking: false,
    }
}

pub fn fixtures() -> (TempDir, PathBuf, SandboxPolicy) {
    let workspace = TempDir::new().expect("workspace");
    let session_temp = workspace.path().join("session/tmp");
    std::fs::create_dir_all(&session_temp).expect("session temp");
    let policy = policy(&workspace, &session_temp);
    (workspace, session_temp, policy)
}

pub fn policy(workspace: &TempDir, session_temp: &Path) -> SandboxPolicy {
    let writable = WritableSet::new([workspace.path().to_path_buf()]).expect("writable set");
    SandboxPolicy::new(
        writable,
        session_temp.to_path_buf(),
        sindri_core::paths::home(),
        "/usr/bin:/bin".to_string(),
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
        status: true,
        extras: Extras::Named(extras.iter().map(|path| path.to_path_buf()).collect()),
        enforcement: Enforcement::Bubblewrap,
        extra_headers: Vec::new(),
        refused_repo_config: None,
    }
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

pub struct Harness {
    agent: Agent,
    pub seen: Arc<Mutex<Vec<Vec<Message>>>>,
    seen_scripts: Arc<Mutex<Vec<String>>>,
    pub thinking: Arc<Mutex<Vec<bool>>>,
    events: Receiver<Event>,
    /// Every event taken off `events` so far, so a test that reads the run's
    /// events and one that reads the loaded set see the same stream rather than
    /// whichever of them drained it first.
    heard: Mutex<Vec<Event>>,
    /// The names this conversation has been told, kept beside the history it
    /// belongs to and cleared with it, as a frame's ledger is.
    loaded: Mutex<BTreeSet<String>>,
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

    /// A harness whose tools include the `skill` tool a catalog describes, so a
    /// turn can load a skill the way a run does.
    pub fn skilled(replies: Vec<Turn>, commands: Vec<ToolResult>, catalog: &Catalog) -> Self {
        let mut harness = Harness::built(
            replies,
            commands,
            8,
            false,
            Interrupt::Never,
            false,
            SkillTool::new(catalog),
        );
        // The half of the catalog a human reaches for is handed in as the boot
        // hands it: a load by hand is what `/skill` asks for.
        harness
            .agent
            .set_hand(sindri_core::agent::UserSkills::new(catalog));
        harness
    }

    fn build(
        replies: Vec<Turn>,
        commands: Vec<ToolResult>,
        max_turns: u32,
        thinking: bool,
        interrupt: Interrupt,
        stream: bool,
    ) -> Self {
        Harness::built(
            replies, commands, max_turns, thinking, interrupt, stream, None,
        )
    }

    fn built(
        replies: Vec<Turn>,
        commands: Vec<ToolResult>,
        max_turns: u32,
        thinking: bool,
        interrupt: Interrupt,
        stream: bool,
        skill: Option<SkillTool>,
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
        let bash = sindri_core::agent::BashTool::new(
            &policy,
            sindri_core::agent::Limits::default(),
            executor,
        );
        let tool = Tools::new(bash, skill, None);
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
            heard: Mutex::new(Vec::new()),
            loaded: Mutex::new(BTreeSet::new()),
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

    pub fn set_budget(&mut self, budget: sindri_core::agent::Budget) {
        self.agent.set_budget(budget);
    }

    pub fn forget(&mut self) {
        self.agent.forget();
        self.loaded.lock().expect("the loaded set").clear();
    }

    /// A skill loaded by hand, as the frame asks for one: no turn, and the body
    /// in the conversation from here on.
    pub fn activate(&mut self, name: &str) -> Activated {
        let activated = self.agent.activate(name);
        // A load by hand runs no turn, so nothing is emitted for it: the call's
        // own answer is where the frame reads it, and where this reads it too.
        if activated.loaded {
            self.loaded
                .lock()
                .expect("the loaded set")
                .insert(activated.name.clone());
        }
        activated
    }

    /// The skills this conversation has been told, in name order: a frame's
    /// ledger, built as a frame builds one, from the `SkillLoaded` events a turn
    /// emits and the answer a load by hand comes back with.
    pub fn loaded(&self) -> Vec<String> {
        self.drain();
        self.loaded
            .lock()
            .expect("the loaded set")
            .iter()
            .cloned()
            .collect()
    }

    pub fn events(&self) -> Vec<Event> {
        self.drain();
        self.heard.lock().expect("events").clone()
    }

    /// Every event the agent has emitted so far, kept rather than dropped, so
    /// `events` and `loaded` are two readings of one stream; and a name a turn
    /// put in force accrues to the loaded set on the way past. Only the word
    /// `loaded` counts, because a second call for a skill already in force is
    /// answered `already_loaded` and adds nothing — the rail's chip reads the
    /// same word.
    fn drain(&self) {
        let mut heard = self.heard.lock().expect("events");
        for event in self.events.try_iter() {
            if let Event::SkillLoaded {
                name,
                outcome: Some(outcome),
                ..
            } = &event
                && outcome == "loaded"
            {
                self.loaded
                    .lock()
                    .expect("the loaded set")
                    .insert(name.clone());
            }
            heard.push(event);
        }
    }

    pub fn scripts(&self) -> Vec<String> {
        self.seen_scripts.lock().expect("scripts").clone()
    }

    pub fn conversations(&self) -> Vec<Vec<Message>> {
        self.seen.lock().expect("seen").clone()
    }
}
