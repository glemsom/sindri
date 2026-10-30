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

use crate::agent::{Agent, Dials, Report};
use crate::cancel::Cancel;
use crate::config::Config;
use crate::exec::SandboxExecutor;
use crate::paths::{self, SessionDir};
use crate::provider::Error;
use crate::record::{Recorder, Redaction, Start};
use crate::sandbox::{
    Enforcement, FALLBACK_PATH, NetworkPosture, PolicyError, SandboxPolicy, WritableSet,
};

pub type LoopError = Error;

/// How every run is enforced: bubblewrap, always. Running commands with the
/// user's own privileges is not a setting a run may carry yet, so the session
/// states the one answer rather than leaving its callers to choose it.
const RUN_ENFORCEMENT: Enforcement = Enforcement::Bubblewrap;

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
    recorder: Arc<Recorder>,
}

impl Session {
    pub fn open(env: &Env, config: &Config) -> Result<Session, StartError> {
        let dir = directory(env)?;
        // The policy is derived before the record is started: a path the
        // sandbox refuses must not leave a record saying `running`, which no
        // reader could tell from a run that is still going.
        let policy = derive(env, &dir, &config.extra_writable, RUN_ENFORCEMENT)?;
        let recorder = Recorder::start(
            &dir,
            Start {
                provider: config.provider.as_str().to_string(),
                model: config.model.clone(),
                enforcement: RUN_ENFORCEMENT,
            },
            Redaction::new([config.api_key.clone()]),
        )
        .map_err(|source| StartError::Record {
            path: dir.root().to_path_buf(),
            source,
        })?;

        Ok(Session {
            env: env.clone(),
            dir,
            policy,
            recorder: Arc::new(recorder),
        })
    }

    pub fn rebase(&mut self, config: &Config) -> Result<(), StartError> {
        self.policy = derive(
            &self.env,
            &self.dir,
            &config.extra_writable,
            RUN_ENFORCEMENT,
        )?;
        Ok(())
    }

    pub fn executor(&self, timeout: Duration, cancel: Cancel) -> SandboxExecutor {
        SandboxExecutor::new(self.policy.clone(), timeout, self.dir.spill(), cancel)
            .with_status_sink(self.recorder.clone())
    }

    /// One prompt's loop, wired to the session: the dials the front end decided
    /// on, the cancel flag it owns, and this record. The caller keeps the agent,
    /// because the frame's thread outlives a prompt.
    pub fn agent(&self, dials: Dials, cancel: Cancel) -> Agent {
        let mut agent = Agent::new(dials.model, dials.tool, dials.max_turns, dials.thinking);
        agent.set_cancel(cancel);
        agent.set_recorder(self.recorder.clone());
        agent
    }

    pub fn finish(&self, outcome: &Result<Report, LoopError>) -> io::Result<()> {
        match outcome {
            Ok(report) => self.recorder.finish(report),
            Err(error) => self.recorder.failed(error.message()),
        }
    }

    pub fn dir(&self) -> &SessionDir {
        &self.dir
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    pub fn recorder(&self) -> Arc<Recorder> {
        self.recorder.clone()
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
    extras: &[PathBuf],
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

pub fn policy_for(
    workspace: &Path,
    extras: &[PathBuf],
    session_temp: PathBuf,
    search_path: String,
    enforcement: Enforcement,
) -> Result<SandboxPolicy, PolicyError> {
    let writable =
        WritableSet::new(std::iter::once(workspace.to_path_buf()).chain(extras.iter().cloned()))?;
    SandboxPolicy::new(
        writable,
        session_temp,
        search_path,
        NetworkPosture::Shared,
        enforcement,
    )
}

pub fn search_path() -> String {
    std::env::var("PATH")
        .ok()
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| FALLBACK_PATH.to_string())
}
