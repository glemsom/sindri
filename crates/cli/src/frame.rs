//! The terminal UI's launch: what a bare `sindri` on a terminal does. Boot
//! proves the sandbox before the first frame, the rail carries the facts the
//! filter used to keep to stderr, and the driver runs one prompt at a time
//! through the same core batch mode uses (ADR-0011, ADR-0012).

use std::sync::Arc;
use std::sync::mpsc;

use sindri_core::agent::{Agent, BashTool, Event};
use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, Overrides};
use sindri_core::doctor::{self, DoctorError};
use sindri_core::exec::{BashExecutor, Outcome, OutputSink, SandboxExecutor, Side, ToolResult};
use sindri_core::paths::SessionDir;
use sindri_core::provider::{self, Client, Usage};
use sindri_core::record::{Recorder, Redaction, Start};
use sindri_core::sandbox::{Enforcement, SandboxPolicy};
use sindri_core::session;
use sindri_tui::driver::{CallOver, Driver, Finished, Spilled, Stop, Tap, Tokens};
use sindri_tui::run::{self, Ending};
use sindri_tui::{Card, Launch};
use sindri_types::ExitCode;

use crate::batch;

/// Runs the frame for one launch, and says how the process should end.
pub fn run(overrides: Overrides) -> i32 {
    let config = match Config::resolve(overrides) {
        Ok(config) => config,
        Err(error) => return batch::config_error(&error),
    };
    // The sandbox is the precondition for the agent, so it is proven before the
    // first frame: a broken sandbox is a card, never a half-drawn interface
    // (ADR-0011 §15).
    if let Err(error) = doctor::probe(&config.extra_writable) {
        let card = Card {
            title: "sindri cannot start: the sandbox does not work here".to_string(),
            check: failing_check(&error),
            detail: error.to_string(),
        };
        let _ = run::refuse(card);
        return ExitCode::EnvironmentFailure.code();
    }

    let (session, policy) = match session::prepare(&config.extra_writable, Enforcement::Bubblewrap)
    {
        Ok(prepared) => prepared,
        Err(message) => {
            eprintln!("sindri: sandbox: {message}");
            return ExitCode::EnvironmentFailure.code();
        }
    };
    let recorder = match Recorder::start(
        &session,
        Start {
            provider: config.provider.as_str().to_string(),
            model: config.model.clone(),
            enforcement: Enforcement::Bubblewrap,
        },
        Redaction::new([config.api_key.clone()]),
    ) {
        Ok(recorder) => Arc::new(recorder),
        Err(error) => {
            eprintln!("sindri: session {}: {error}", session.root().display());
            return ExitCode::EnvironmentFailure.code();
        }
    };

    let launch = launch(&session, &policy, &config);
    let driver = Front {
        config,
        policy,
        session,
        recorder,
    };
    match run::open(launch, Box::new(driver)) {
        Ending::Left => ExitCode::Success.code(),
        Ending::BootFailed => ExitCode::EnvironmentFailure.code(),
    }
}

/// What the rail is shown at the start of a launch, and again after `/new`.
fn launch(session: &SessionDir, policy: &SandboxPolicy, config: &Config) -> Launch {
    Launch {
        session: session.id().to_string(),
        temp: session.temp().display().to_string(),
        branch: git_branch(),
        sandbox: posture(policy),
        max_turns: config.max_turns,
        // The meter's denominator is the family's advertised window, where the
        // adapter knows it; where it does not, the rail counts up without one.
        context_limit: provider::context_window(&config.model),
    }
}

/// One launch's runs: the executor, the tool and the agent batch mode builds,
/// wired to the frame instead of to stdout.
struct Front {
    config: Config,
    policy: SandboxPolicy,
    session: SessionDir,
    recorder: Arc<Recorder>,
}

impl Driver for Front {
    fn run(&mut self, prompt: &str, cancel: Cancel, tap: Tap) -> Finished {
        let executor = Arc::new(
            SandboxExecutor::new(
                self.policy.clone(),
                self.config.timeout,
                self.session.spill(),
                cancel.clone(),
            )
            .with_status_sink(self.recorder.clone())
            .with_output_sink(Arc::new(Told(tap.clone()))),
        );
        let tool = BashTool::new(
            &self.policy,
            Arc::new(Watched {
                inner: executor,
                tap: tap.clone(),
            }),
        );
        let mut agent = Agent::new(
            Box::new(Client::new(&self.config)),
            tool,
            self.config.max_turns,
            self.config.thinking,
        );
        agent.set_cancel(cancel);
        agent.set_recorder(self.recorder.clone());

        // The core writes events to a channel of its own; one small thread
        // carries them to the frame, so a turn's fragments arrive in order.
        let (events, stream) = mpsc::channel::<Event>();
        agent.set_events(events);
        let forward = std::thread::spawn(move || {
            for event in stream {
                tap.event(event);
            }
        });
        let outcome = agent.run(prompt);
        drop(agent);
        let _ = forward.join();

        match outcome {
            Ok(report) => {
                let _ = self.recorder.finish(&report);
                Finished {
                    stopped: stop(report.stopped),
                    turns: report.turns,
                    tokens: tokens(&report.usage),
                    error: None,
                }
            }
            Err(error) => {
                let message = batch::provider_message(&error);
                let _ = self.recorder.failed(&message);
                Finished {
                    stopped: Stop::Error,
                    turns: 0,
                    tokens: Tokens::default(),
                    error: Some(message),
                }
            }
        }
    }

    fn renew(&mut self) -> Option<Launch> {
        self.fresh_session()
    }
}

impl Front {
    /// `/new`: a fresh session directory, a fresh record, the same frame. The
    /// old session keeps everything it already wrote (ADR-0006).
    fn fresh_session(&mut self) -> Option<Launch> {
        let (session, policy) =
            session::prepare(&self.config.extra_writable, Enforcement::Bubblewrap).ok()?;
        let recorder = Recorder::start(
            &session,
            Start {
                provider: self.config.provider.as_str().to_string(),
                model: self.config.model.clone(),
                enforcement: Enforcement::Bubblewrap,
            },
            Redaction::new([self.config.api_key.clone()]),
        )
        .ok()?;
        self.policy = policy;
        self.session = session;
        self.recorder = Arc::new(recorder);
        Some(launch(&self.session, &self.policy, &self.config))
    }
}

/// Hands each line of a running command's output to the frame as it arrives.
struct Told(Tap);

impl OutputSink for Told {
    fn output(&self, side: Side, text: &str) {
        self.0.output(side, text);
    }
}

/// Says how a command ended: the exit status is the executor's answer to its
/// caller, so only the caller can tell the frame.
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

/// What a command's output had to leave behind, in the frame's words: the
/// untruncated streams are files the frame offers rather than holds (ADR-0007).
fn spilled(result: Option<&ToolResult>) -> Spilled {
    match result {
        Some(result) => Spilled {
            truncated: result.truncated,
            files: result.spill.clone(),
        },
        None => Spilled::default(),
    }
}

/// The core's outcome, in the frame's words.
fn over(outcome: Option<&Outcome>) -> CallOver {
    match outcome {
        None => CallOver::Cancelled,
        Some(Outcome::Exited { exit_code }) => CallOver::Exited(*exit_code),
        Some(Outcome::TimedOut) => CallOver::TimedOut,
        Some(Outcome::SandboxError { detail }) => CallOver::Refused(detail.clone()),
    }
}

fn stop(stopped: sindri_core::agent::Stopped) -> Stop {
    match stopped {
        sindri_core::agent::Stopped::Final => Stop::Final,
        sindri_core::agent::Stopped::MaxTurns => Stop::MaxTurns,
        sindri_core::agent::Stopped::Cancelled => Stop::Cancelled,
        sindri_core::agent::Stopped::Error => Stop::Error,
    }
}

fn tokens(usage: &Usage) -> Tokens {
    Tokens {
        input: usage.input_tokens.unwrap_or_default(),
        output: usage.output_tokens.unwrap_or_default(),
        cached: usage.cached_input_tokens.unwrap_or_default(),
    }
}

/// The sandbox posture, in the one line the rail gives it.
fn posture(policy: &SandboxPolicy) -> String {
    match policy.enforcement() {
        Enforcement::Bubblewrap => {
            format!("bwrap · {} writable", policy.writable().paths().len())
        }
        Enforcement::YoloUnsafe => "unsafe · your own privileges".to_string(),
    }
}

/// The check that failed, named the way the card names it.
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

/// The branch of the workspace, when the workspace is a repository.
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
