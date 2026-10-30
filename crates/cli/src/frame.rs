//! The terminal UI's launch: what a bare `sindri` on a terminal does. Boot
//! proves the sandbox and opens a session before the first frame, the rail
//! carries the facts the filter used to keep to stderr, and the driver runs one
//! prompt at a time through the same session batch mode uses (ADR-0011,
//! ADR-0012, ADR-0015).

use std::sync::Arc;
use std::sync::mpsc;

use sindri_core::agent::{Agent, BashTool, Dials, Event, Report, Stopped};
use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, Overrides};
use sindri_core::doctor::{self, DoctorError};
use sindri_core::exec::{BashExecutor, Outcome, OutputSink, SandboxExecutor, Side, ToolResult};
use sindri_core::provider::{self, Client, Usage};
use sindri_core::sandbox::{Enforcement, SandboxPolicy};
use sindri_core::session::{LoopError, Session};
use sindri_tui::driver::{CallOver, Driver, Finished, Spilled, Stop, Tap, Tokens};
use sindri_tui::run::{self, Ending};
use sindri_tui::{Card, Launch};
use sindri_types::ExitCode;

use crate::boot::{self, Boot};
use crate::{failure, signal};

/// Runs the frame for one launch, and says how the process should end.
pub fn run(overrides: Overrides) -> i32 {
    // What boot reads, before anything exists on disk: the process is read once
    // and the config resolved once (ADR-0015 §6). The sandbox is proven on it
    // before the session is opened, so a machine that cannot have a sandbox is a
    // card and never a record left saying `running`.
    let configured = match boot::expect(overrides.clone()) {
        Ok(configured) => configured,
        Err(failure) => return failure.report(),
    };
    // The sandbox is the precondition for the agent, so it is proven before the
    // first frame: a broken sandbox is a card, never a half-drawn interface
    // (ADR-0011 §15).
    if let Err(error) = doctor::probe(&configured.config.extra_writable) {
        let card = Card {
            title: "sindri cannot start: the sandbox does not work here".to_string(),
            check: failing_check(&error),
            detail: error.to_string(),
        };
        let _ = run::refuse(card);
        return ExitCode::EnvironmentFailure.code();
    }
    // The run is started in the environment boot read, in the one order
    // (ADR-0015 §1-2): the frame builds no directory, no record and no policy by
    // hand. A run that cannot be evidenced is not a run, and the failure is said
    // the way batch mode says it (ADR-0006).
    let boot = match configured.start() {
        Ok(boot) => boot,
        Err(failure) => return failure.report(),
    };

    // One flag for the launch, raised by Ctrl-C and by a signal alike, and put
    // down between prompts by the frame (ADR-0015 §8).
    let cancel = Cancel::new();
    signal::install(cancel.clone());

    let launch = launch(&boot);
    let driver = Front {
        boot,
        overrides,
        agent: None,
    };
    match run::open(launch, Box::new(driver), cancel) {
        Ending::Left => ExitCode::Success.code(),
        Ending::BootFailed => ExitCode::EnvironmentFailure.code(),
    }
}

/// What the rail is shown at the start of a launch, and again after `/new`: the
/// session's own facts, and the two that are the rail's (ADR-0015 §9).
fn launch(boot: &Boot) -> Launch {
    Launch {
        session: boot.session.dir().id().to_string(),
        temp: boot.session.dir().temp().display().to_string(),
        branch: git_branch(),
        sandbox: posture(boot.session.policy()),
        max_turns: boot.config.max_turns,
        // The meter's denominator is the family's advertised window, where the
        // adapter knows it; where it does not, the rail counts up without one.
        context_limit: provider::context_window(&boot.config.model),
    }
}

/// One launch's runs: the session the launch holds, and the thread it keeps
/// across prompts.
struct Front {
    /// The run boot started: the environment it read, the config it resolved and
    /// the one session this launch is recording. `/new` opens the next session
    /// from these values, so the process is not read a second time (ADR-0015 §6).
    boot: Boot,
    /// The flags the launch was asked with. A reload resolves the config again
    /// from these, so a flag still wins over the file after a save (ADR-0005).
    overrides: Overrides,
    /// The conversation, kept across prompts: a second question is a follow-up,
    /// and a model changed in `/settings` answers it (ADR-0014).
    agent: Option<Agent>,
}

impl Driver for Front {
    fn run(&mut self, prompt: &str, cancel: Cancel, tap: Tap) -> Finished {
        // The per-prompt parts come from the session, so the sandbox, the record
        // and the flag are wired the one way (ADR-0015 §3).
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

        // The core writes events to a channel of its own; one small thread
        // carries them to the frame, so a turn's fragments arrive in order.
        let (events, stream) = mpsc::channel::<Event>();
        let forward = std::thread::spawn(move || {
            for event in stream {
                tap.event(event);
            }
        });

        // The thread is the conversation's, so a new model, tool, cap or
        // thinking toggle is a change of dials and not a change of subject.
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
            // The session builds the loop, wired to its own record and the
            // launch's flag.
            None => self.agent.insert(self.boot.session.agent(dials, cancel)),
        };
        agent.set_events(events);
        // The agent stays: the thread it holds is the conversation's. Only its
        // channel is dropped, so the forwarding thread sees the stream end.
        let outcome = agent.run(prompt);
        agent.close_events();
        let _ = forward.join();

        // The record's end is written here, and a failure to write it is said in
        // the transcript rather than dropped (ADR-0015 §3).
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
        // in — no second read of the process (ADR-0015 §6) — and adopted only
        // once it has answered `Ok`: the thread and the record being written
        // stay until a session exists to replace them, so a `/new` that cannot
        // start loses nothing (ADR-0015 §2). The session it replaces is
        // superseded: it was left, not stopped, so it gets no end written.
        let session = boot::renew(&self.boot)
            .map_err(|failure| format!("the new session could not start: {failure}"))?;
        self.boot.session = session;
        // A new session is a new conversation: the thread is forgotten only now.
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
        // The same session, bounded by the policy the new settings grant: a
        // saved extra path is in force for the very next command (ADR-0015 §4).
        self.boot
            .session
            .rebase(&config)
            .map_err(|error| error.to_string())?;
        self.boot.config = config;
        Ok(launch(&self.boot))
    }
}

/// The session's end, and what to say when it cannot be written: a record the
/// frame could not close is evidence lost, so it is said in the frame's own
/// words instead of dropped (ADR-0015 §3), the way batch mode says it.
fn closing_note(session: &Session, outcome: &Result<Report, LoopError>) -> Option<String> {
    session.finish(outcome).err().map(|error| {
        format!(
            "session {}: the record could not be finished: {error}",
            session.dir().id()
        )
    })
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
