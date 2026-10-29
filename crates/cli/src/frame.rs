//! The terminal UI's launch: what a bare `sindri` on a terminal does. Boot
//! proves the sandbox before the first frame, the rail carries the facts the
//! filter used to keep to stderr, and the driver runs one prompt at a time
//! through the same core batch mode uses (ADR-0011, ADR-0012).

use std::sync::Arc;
use std::sync::mpsc;

use sindri_core::agent::{Agent, BashTool, Event};
use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, ConfigError, Overrides};
use sindri_core::doctor::{self, DoctorError};
use sindri_core::exec::{BashExecutor, Outcome, OutputSink, SandboxExecutor, Side, ToolResult};
use sindri_core::paths::SessionDir;
use sindri_core::provider::{Client, Error as ProviderError, Usage};
use sindri_core::record::{Recorder, Redaction, Start};
use sindri_core::sandbox::{Enforcement, SandboxPolicy};
use sindri_core::session;
use sindri_tui::driver::{CallOver, Driver, Finished, Stop, Tap, Tokens};
use sindri_tui::run::{self, Boot, Ending};
use sindri_tui::{Card, Launch};
use sindri_types::ExitCode;

/// Runs the frame for one launch, and says how the process should end.
pub fn run(overrides: Overrides) -> i32 {
    let config = match Config::resolve(overrides) {
        Ok(config) => config,
        Err(error) => return config_error(&error),
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
        let _ = run::run(Boot::Failed(card), Box::new(Nowhere));
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

    let launch = Launch {
        session: session.id().to_string(),
        temp: session.temp().display().to_string(),
        branch: git_branch(),
        sandbox: posture(&policy),
        max_turns: config.max_turns,
        // The provider reports what a turn spent, not how much window it has
        // left, so the meter counts up and says so rather than guessing a
        // denominator for a trust fact.
        context_limit: None,
    };
    let driver = Front {
        config,
        policy,
        session,
        recorder,
    };
    match run::run(Boot::Ready(launch), Box::new(driver)) {
        Ending::Left => ExitCode::Success.code(),
        Ending::BootFailed => ExitCode::EnvironmentFailure.code(),
    }
}

/// A launch that never reaches a driver: the frame drew a card and left.
struct Nowhere;

impl Driver for Nowhere {
    fn run(&mut self, _prompt: &str, _cancel: Cancel, _tap: Tap) -> Finished {
        Finished::default()
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
                let message = provider_message(&error);
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
        self.tap
            .over(over(result.as_ref().map(|result| &result.outcome)));
        result
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

/// The provider's complaint, as one line the transcript can hold.
fn provider_message(error: &ProviderError) -> String {
    match error.status() {
        Some(status) => format!("provider error (HTTP {status}): {}", error.message()),
        None => format!("provider error: {}", error.message()),
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

/// A configuration value that is wrong is a usage error; one that is absent is
/// a model error with a hint, exactly as batch says it.
fn config_error(error: &ConfigError) -> i32 {
    eprintln!("sindri: {}", error.message());
    match error {
        ConfigError::Usage(_) => ExitCode::UsageError.code(),
        ConfigError::Missing(_) => ExitCode::ModelError.code(),
    }
}
