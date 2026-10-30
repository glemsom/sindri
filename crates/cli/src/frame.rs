//! The terminal UI's launch: what a bare `sindri` on a terminal does. Boot
//! proves the sandbox and opens a session before the first frame, the rail
//! carries the facts the filter used to keep to stderr, and the driver runs one
//! prompt at a time through the same session batch mode uses (ADR-0011).

use std::sync::Arc;
use std::sync::mpsc;

use sindri_core::agent::{Agent, BashTool, Dials, Event, Report, Stopped};
use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, ConfigError, Overrides};
use sindri_core::doctor::{self, DoctorError};
use sindri_core::exec::{BashExecutor, Outcome, OutputSink, SandboxExecutor, Side, ToolResult};
use sindri_core::provider::{self, Client, Usage};
use sindri_core::sandbox::{Enforcement, SandboxPolicy};
use sindri_core::session::{LoopError, Session};
use sindri_tui::driver::{CallOver, Driver, Finished, Spilled, Stop, Tap, Tokens};
use sindri_tui::run::{self, Ending};
use sindri_tui::settings::Missing;
use sindri_tui::{Card, Launch};
use sindri_types::ExitCode;

use crate::boot::{self, Boot};
use crate::{failure, signal};

pub fn run(overrides: Overrides) -> i32 {
    let (configured, missing) = match boot::prepare(overrides.clone()) {
        Ok(configured) => (configured, None),
        // A value the run cannot do without is a line and an exit everywhere
        // else; the frame is the one door that opens on it, because a card has
        // no field to type a key into (ADR-0004). What is absent is read from
        // the config that was resolved anyway, so the modal knows which row it
        // is for. Every other failure still leaves by `report`.
        Err(failure::Failure::Config(ConfigError::Missing(_))) => {
            match boot::configure(overrides.clone()) {
                Ok(configured) => {
                    let missing = Missing::of(
                        configured.config.provider,
                        configured.config.provider.needs_credential()
                            && configured.config.api_key.trim().is_empty(),
                        configured.config.model.trim().is_empty(),
                    );
                    (configured, missing)
                }
                Err(failure) => return failure.report(),
            }
        }
        Err(failure) => return failure.report(),
    };
    // The sandbox is the precondition for the agent, so it is proven before the
    // first frame: a broken sandbox is a card, never a half-drawn interface
    if let Err(error) = doctor::probe(&configured.env, &configured.config.extra_writable) {
        let card = Card {
            title: "sindri cannot start: the sandbox does not work here".to_string(),
            check: failing_check(&error),
            detail: error.to_string(),
        };
        let _ = run::refuse(card);
        return ExitCode::EnvironmentFailure.code();
    }
    let boot = match configured.start() {
        Ok(boot) => boot,
        Err(failure) => return failure.report(),
    };

    // One flag for the launch, raised by Ctrl-C and by a signal alike, and put
    // down between prompts by the frame (ADR-0015).
    let cancel = Cancel::new();
    signal::install(cancel.clone());

    let launch = launch(&boot);
    let driver = Front {
        boot,
        overrides,
        agent: None,
    };
    match run::open(launch, Box::new(driver), cancel, missing) {
        Ending::Left => ExitCode::Success.code(),
        Ending::BootFailed => ExitCode::EnvironmentFailure.code(),
    }
}

fn launch(boot: &Boot) -> Launch {
    Launch {
        session: boot.session.dir().id().to_string(),
        temp: boot.session.dir().temp().display().to_string(),
        branch: git_branch(),
        sandbox: posture(boot.session.policy()),
        provider: boot.config.provider.as_str().to_string(),
        model: boot.config.model.clone(),
        workspace: workspace(&boot.env.workspace),
        max_turns: boot.config.max_turns,
        // The meter's denominator is the family's advertised window, where the
        // adapter knows it; where it does not, the rail counts up without one.
        context_limit: provider::context_window(&boot.config.model),
    }
}

struct Front {
    boot: Boot,
    overrides: Overrides,
    agent: Option<Agent>,
}

impl Driver for Front {
    fn run(&mut self, prompt: &str, cancel: Cancel, tap: Tap) -> Finished {
        let executor = Arc::new(
            self.boot
                .session
                .executor(self.boot.config.timeout, cancel.clone())
                .with_output_sink(Arc::new(Told(tap.clone()))),
        );
        let tool = BashTool::new(
            self.boot.session.policy(),
            Arc::new(Watched {
                inner: executor,
                tap: tap.clone(),
            }),
        );
        let (max_turns, thinking) = (self.boot.config.max_turns, self.boot.config.thinking);
        let model = Box::new(Client::new(&self.boot.config));

        let (events, stream) = mpsc::channel::<Event>();
        let forward = std::thread::spawn(move || {
            for event in stream {
                tap.event(event);
            }
        });

        let dials = Dials {
            model,
            tool,
            max_turns,
            thinking,
        };
        let agent = match self.agent.as_mut() {
            Some(agent) => {
                agent.set_dials(dials);
                agent
            }
            None => self.agent.insert(self.boot.session.agent(dials, cancel)),
        };
        agent.set_events(events);
        // The agent stays: the thread it holds is the conversation's. Only its
        // channel is dropped, so the forwarding thread sees the stream end.
        let outcome = agent.run(prompt);
        agent.close_events();
        let _ = forward.join();

        // The record's end is written here, and a failure to write it is said in
        // the transcript rather than dropped (ADR-0015).
        let note = closing_note(&self.boot.session, &outcome);
        match outcome {
            Ok(report) => Finished {
                stopped: stop(report.stopped),
                turns: report.turns,
                tokens: tokens(&report.usage),
                error: None,
                note,
            },
            Err(error) => Finished {
                stopped: Stop::Error,
                turns: 0,
                tokens: Tokens::default(),
                error: Some(failure::provider_message(&error)),
                note,
            },
        }
    }

    fn renew(&mut self) -> Result<Launch, String> {
        // `/new` is boot's read again, in the environment this launch was booted
        // in — no second read of the process (ADR-0015) — and adopted only once
        // it has answered `Ok`. The session it replaces is superseded: left, not
        // stopped, so it gets no end written.
        let session = boot::renew(&self.boot)
            .map_err(|failure| format!("the new session could not start: {failure}"))?;
        self.boot.session = session;
        if let Some(agent) = self.agent.as_mut() {
            agent.forget();
        }
        Ok(launch(&self.boot))
    }

    fn reload(&mut self) -> Result<Launch, String> {
        // A save lands while the frame owns the screen, so it does not go through
        // boot: the failure is a value the transcript says, never a line on
        // stderr (ADR-0005).
        let config =
            Config::resolve(self.overrides.clone()).map_err(|error| error.message().to_string())?;
        self.boot
            .session
            .rebase(&config)
            .map_err(|error| error.to_string())?;
        self.boot.config = config;
        Ok(launch(&self.boot))
    }
}

fn closing_note(session: &Session, outcome: &Result<Report, LoopError>) -> Option<String> {
    session.finish(outcome).err().map(|error| {
        format!(
            "session {}: the record could not be finished: {error}",
            session.dir().id()
        )
    })
}

struct Told(Tap);

impl OutputSink for Told {
    fn output(&self, side: Side, text: &str) {
        self.0.output(side, text);
    }
}

struct Watched {
    inner: Arc<SandboxExecutor>,
    tap: Tap,
}

impl BashExecutor for Watched {
    fn execute(&self, script: &str) -> Option<ToolResult> {
        let result = self.inner.execute(script);
        self.tap.over(
            over(result.as_ref().map(|result| &result.outcome)),
            spilled(result.as_ref()),
        );
        result
    }
}

fn spilled(result: Option<&ToolResult>) -> Spilled {
    match result {
        Some(result) => Spilled {
            truncated: result.truncated,
            files: result.spill.clone(),
        },
        None => Spilled::default(),
    }
}

fn over(outcome: Option<&Outcome>) -> CallOver {
    match outcome {
        None => CallOver::Cancelled,
        Some(Outcome::Exited { exit_code }) => CallOver::Exited(*exit_code),
        Some(Outcome::TimedOut) => CallOver::TimedOut,
        Some(Outcome::SandboxError { detail }) => CallOver::Refused(detail.clone()),
    }
}

fn stop(stopped: Stopped) -> Stop {
    match stopped {
        Stopped::Final => Stop::Final,
        Stopped::MaxTurns => Stop::MaxTurns,
        Stopped::Cancelled => Stop::Cancelled,
        Stopped::Error => Stop::Error,
    }
}

fn tokens(usage: &Usage) -> Tokens {
    Tokens {
        input: usage.input_tokens.unwrap_or_default(),
        output: usage.output_tokens.unwrap_or_default(),
        cached: usage.cached_input_tokens.unwrap_or_default(),
    }
}

fn posture(policy: &SandboxPolicy) -> String {
    match policy.enforcement() {
        Enforcement::Bubblewrap => {
            format!("bwrap · {} writable", policy.writable().paths().len())
        }
        Enforcement::YoloUnsafe => "unsafe · your own privileges".to_string(),
    }
}

fn failing_check(error: &DoctorError) -> String {
    match error {
        DoctorError::BwrapMissing => "bwrap is not installed".to_string(),
        DoctorError::SandboxNotStarting { .. } => "bwrap cannot build a namespace".to_string(),
        DoctorError::BinariesMissing { missing } => {
            format!("binaries missing: {}", missing.join(", "))
        }
        DoctorError::Setup(_) => "the probe could not be set up".to_string(),
        DoctorError::Policy(_) => "the sandbox policy was refused".to_string(),
    }
}

/// The directory the rail states and a turn header names: the one the run was
/// booted in, which is the one the sandbox mounts, with the home shortened to
/// `~` because a home path is the one part a reader already knows.
fn workspace(dir: &std::path::Path) -> String {
    match std::env::var_os("HOME").map(std::path::PathBuf::from) {
        Some(home) => match dir.strip_prefix(&home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => dir.display().to_string(),
        },
        None => dir.display().to_string(),
    }
}

fn git_branch() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken seam is reported"
    )]

    use std::time::Duration;

    use sindri_core::config::Provider;
    use sindri_core::session::Env as RunEnv;
    use tempfile::TempDir;

    use super::*;

    fn config() -> Config {
        Config {
            provider: Provider::Anthropic,
            model: "model-one".to_string(),
            api_key: String::new(),
            base_url: None,
            max_turns: 3,
            timeout: Duration::from_secs(5),
            thinking: false,
            extra_writable: Vec::new(),
            extra_headers: Vec::new(),
            refused_repo_config: None,
        }
    }

    /// A run environment in a temporary directory, so nothing the launch says
    /// is the process's own.
    fn env(root: &TempDir) -> RunEnv {
        let workspace = root.path().join("workspace");
        let state = root.path().join("state");
        for dir in [&workspace, &state] {
            std::fs::create_dir_all(dir).expect("a temporary directory");
        }
        RunEnv {
            workspace: workspace.canonicalize().expect("canonical workspace"),
            search_path: "/usr/bin:/bin".to_string(),
            state,
        }
    }

    #[test]
    fn the_rail_names_the_workspace_the_sandbox_actually_mounts() {
        let root = TempDir::new().expect("temp root");
        let boot = boot::Configured {
            env: env(&root),
            config: config(),
        }
        .start()
        .expect("a session in the temporary environment");
        // Asked of the policy, not of the process.
        let mounted = boot
            .session
            .policy()
            .writable()
            .paths()
            .iter()
            .find(|path| path.ends_with("workspace"))
            .expect("the workspace is writable inside the sandbox")
            .clone();

        let launch = launch(&boot);

        // Shortened only by the home it is under: the mounted directory, not
        // the process's own.
        let shown = launch.workspace.trim_start_matches("~/");
        assert!(
            mounted.display().to_string().ends_with(shown),
            "the rail states the workspace the sandbox mounts: {shown:?} of {mounted:?}"
        );
        assert_eq!(
            launch.temp,
            boot.session.dir().temp().display().to_string(),
            "and the temp the session maps, beside it"
        );
        assert_eq!(launch.model, "model-one", "the model the run asks");
    }
}
