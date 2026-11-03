//! One run's session: the value a front end holds to have a run.
//!
//! `Session::open` is the only place in the core where the order a run is
//! started in exists — the session directory with its `HOME`, the record, the
//! sandbox policy — and the per-prompt parts are built from it rather than
//! owned by it: a policy can change between prompts, and a front end keeps one
//! agent for a whole thread.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::agent::{Agent, Dials, Report, SkillTool, Stopped, UserSkills};
use crate::cancel::Cancel;
use crate::config::Config;
use crate::exec::SandboxExecutor;
use crate::instructions::{self, Taken};
use crate::paths::{self, SessionDir};
use crate::provider::Error;
use crate::record::{Recorder, Redaction, Start};
use crate::sandbox::{Enforcement, Extras, FALLBACK_PATH, PolicyError, SandboxPolicy, WritableSet};
use crate::skills;
use crate::workspace::{self, Git};

pub type LoopError = Error;

#[derive(Debug, Clone)]
pub struct Env {
    pub workspace: PathBuf,
    pub search_path: String,
    pub state: PathBuf,
}

impl Env {
    pub fn ambient() -> io::Result<Env> {
        Ok(Env {
            workspace: std::env::current_dir()?.canonicalize()?,
            search_path: search_path(),
            state: paths::state_dir()?,
        })
    }
}

/// Why a run could not be started: one error, naming what failed and carrying
/// the path where there is one. The core prints nothing and maps no exit code —
/// a front end owns those words.
#[derive(Debug)]
pub enum StartError {
    Sandbox(PolicyError),
    Directory { path: PathBuf, source: io::Error },
    Record { path: PathBuf, source: io::Error },
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartError::Sandbox(source) => write!(f, "{source}"),
            StartError::Directory { path, source } => write!(
                f,
                "the session directory {} cannot be created: {source}",
                path.display()
            ),
            StartError::Record { path, source } => write!(
                f,
                "the record in {} cannot be started: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for StartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StartError::Sandbox(source) => Some(source),
            StartError::Directory { source, .. } | StartError::Record { source, .. } => {
                Some(source)
            }
        }
    }
}

impl From<PolicyError> for StartError {
    fn from(source: PolicyError) -> Self {
        StartError::Sandbox(source)
    }
}

pub struct Session {
    /// The environment it was opened in, kept so `rebase` re-derives the policy
    /// from the same workspace and `PATH` without reading the process again.
    env: Env,
    dir: SessionDir,
    policy: SandboxPolicy,
    /// The tree the run was opened in, probed once: the rail and the record
    /// read this one value, so they cannot name different branches.
    git: Option<Git>,
    recorder: Arc<Recorder>,
    /// The tool that carries the model-invocable ones, built with the catalog
    /// it describes: a prompt is handed this value, so the description and the
    /// schema are made once rather than derived again on every turn.
    skill: Option<SkillTool>,
    /// The half of the catalog a human may reach for, built once from the same
    /// scan: the composer's names, and the skills a load by hand is read from.
    hand: UserSkills,
    /// The repository's own rules, read once here and handed to the model
    /// beside the preamble on every turn of the thread.
    instructions: Taken,
}

impl Session {
    pub fn open(env: &Env, config: &Config) -> Result<Session, StartError> {
        let dir = directory(env)?;
        // The policy is derived before the record is started: a path the
        // sandbox refuses must not leave a record saying `running`, which no
        // reader could tell from a run that is still going.
        let policy = derive(env, &dir, &config.extras, config.enforcement)?;
        // One probe of the tree for both facts boot needs from it: the git facts
        // the record and the rail read, and the root the rules and the skill
        // scopes are found from. Asking twice would fork git a third time and
        // let the rules and the skills be read from two different repositories.
        let probe = workspace::probe(&env.workspace);
        let git = probe.git;
        let root = probe.root;
        // Before the record, because the start block says what the model was
        // handed: a file read after the metadata was written would be missing
        // from the one document a reader looks for it in.
        let instructions = instructions::read_in(root.as_deref(), &env.workspace);
        let recorder = Recorder::start(
            &dir,
            Start {
                provider: config.provider.as_str().to_string(),
                model: config.model.clone(),
                enforcement: config.enforcement,
                policy: Some(policy.describe()),
                workspace: env.workspace.display().to_string(),
                git: git.clone(),
                instructions: instructions.handed(),
                max_turns: config.max_turns,
                thinking: config.thinking,
            },
            Redaction::new([config.api_key.clone()]),
        )
        .map_err(|source| StartError::Record {
            path: dir.root().to_path_buf(),
            source,
        })?;

        let catalog =
            skills::discover_in(&env.workspace, root.as_deref(), paths::home().as_deref());
        let skill = SkillTool::new(&catalog);
        let hand = UserSkills::new(&catalog);
        Ok(Session {
            env: env.clone(),
            dir,
            policy,
            git,
            recorder: Arc::new(recorder),
            skill,
            hand,
            instructions,
        })
    }

    /// The next prompt's policy, and the record's if it moved: a writable set
    /// that changed under a thread is announced where it changed.
    pub fn rebase(&mut self, config: &Config) -> Result<(), StartError> {
        self.policy = derive(&self.env, &self.dir, &config.extras, config.enforcement)?;
        self.recorder.rebase(&self.policy.describe());
        Ok(())
    }

    pub fn executor(&self, timeout: Duration, cancel: Cancel) -> SandboxExecutor {
        SandboxExecutor::new(self.policy.clone(), timeout, self.dir.spill(), cancel)
            .with_status_sink(self.recorder.clone())
    }

    /// One prompt's loop, wired to the session: the dials the front end decided
    /// on, the cancel flag it owns, and this session. The caller keeps the agent,
    /// because the frame's thread outlives a prompt.
    pub fn agent(&self, dials: Dials, cancel: Cancel) -> Agent {
        let mut agent = Agent::new(dials.model, dials.tool, dials.max_turns, dials.thinking);
        agent.set_cancel(cancel);
        self.adopt(&mut agent);
        agent
    }

    /// Takes a conversation a front end keeps onto this session: the record it
    /// writes and the skills a human may reach for are this session's. An agent
    /// made here has them once, so a front end that keeps one across prompts
    /// takes this session again before the next one — `/new` puts a new record
    /// and a new hand under the same conversation, and a prompt written to the
    /// record of the session it replaced is two records of one run.
    pub fn adopt(&self, agent: &mut Agent) {
        agent.set_recorder(self.recorder.clone());
        agent.set_hand(self.hand.clone());
    }

    pub fn finish(&self, outcome: &Result<Report, LoopError>) -> io::Result<()> {
        match outcome {
            Ok(report) => self.recorder.finish(report),
            Err(error) => self.recorder.failed(error.message()),
        }
    }

    /// Ends a session that ran a command and no model — `sandbox exec` has a
    /// stop reason, the commands that ran and a duration, and no answer to give,
    /// so it writes an end block without dressing one up as a `Report`.
    pub fn finish_command(
        &self,
        stopped: Stopped,
        tool_calls: u32,
        took: Duration,
    ) -> io::Result<()> {
        self.recorder
            .ran_command(stopped.as_str(), tool_calls, took)
    }

    pub fn dir(&self) -> &SessionDir {
        &self.dir
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// The tree the run was opened in: what a relative path in a call is read
    /// from, since it is the directory a command starts in inside the sandbox.
    pub fn workspace(&self) -> &Path {
        &self.env.workspace
    }

    /// The tree's git facts as the record holds them, so a front end states
    /// what the run recorded rather than shelling git a second time.
    pub fn git(&self) -> Option<&Git> {
        self.git.as_ref()
    }

    /// The `skill` tool, or none when this workspace holds no skill the model
    /// may see — in which case the model is handed `bash` and nothing else.
    pub fn skill_tool(&self) -> Option<SkillTool> {
        self.skill.clone()
    }

    /// The skills a human may reach for: the names a frame offers, and the ones a
    /// load by hand is read from.
    pub fn hand(&self) -> &UserSkills {
        &self.hand
    }

    /// The repository's own rules: what the model is handed beside the preamble,
    /// and what `-v` and `sindri doctor` name.
    pub fn instructions(&self) -> &Taken {
        &self.instructions
    }
}

fn directory(env: &Env) -> Result<SessionDir, StartError> {
    let dir = SessionDir::create_in(&env.state).map_err(|source| StartError::Directory {
        path: env.state.join("sessions"),
        source,
    })?;
    let home = dir.temp().join("home");
    std::fs::create_dir_all(&home).map_err(|source| StartError::Directory {
        path: home.clone(),
        source,
    })?;
    Ok(dir)
}

fn derive(
    env: &Env,
    dir: &SessionDir,
    extras: &Extras,
    enforcement: Enforcement,
) -> Result<SandboxPolicy, PolicyError> {
    policy_for(
        &env.workspace,
        extras,
        dir.temp(),
        env.search_path.clone(),
        enforcement,
    )
}

/// One policy from the run's three facts: the workspace, where the extras come
/// from, and the temp. The extras are resolved here — the floor against this
/// host's home directory and this workspace, a named list as it was given — so
/// what is mounted and what the description prints come from one value.
pub fn policy_for(
    workspace: &Path,
    extras: &Extras,
    session_temp: PathBuf,
    search_path: String,
    enforcement: Enforcement,
) -> Result<SandboxPolicy, PolicyError> {
    let resolved = extras.resolve(paths::home().as_deref(), workspace);
    let writable = WritableSet::new(std::iter::once(workspace.to_path_buf()).chain(resolved))?;
    SandboxPolicy::new(writable, session_temp, search_path, enforcement)
}

pub fn search_path() -> String {
    std::env::var("PATH")
        .ok()
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| FALLBACK_PATH.to_string())
}
