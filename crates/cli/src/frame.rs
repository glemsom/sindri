//! The terminal UI's launch: what a bare `sindri` on a terminal does. Boot
//! proves the sandbox and opens a session before the first frame, the rail
//! carries the facts the filter used to keep to stderr, and the driver runs one
//! prompt at a time on the same stack batch mode runs on.

use std::sync::Arc;
use std::sync::mpsc::Receiver;

use sindri_core::agent::{Agent, Event, Report};
use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, ConfigError, Overrides};
use sindri_core::doctor::{self, DoctorError};
use sindri_core::exec::{BashExecutor, Outcome, OutputSink, SandboxExecutor, Side, ToolResult};
use sindri_core::provider::{self, Usage};
use sindri_core::sandbox::{Enforcement, SandboxPolicy};
use sindri_core::session::{LoopError, Session};
use sindri_tui::driver::{CallOver, Driver, Finished, Invoked, Spilled, Stop, Tap, Tokens};
use sindri_tui::run::{self, Ending};
use sindri_tui::settings::Missing;
use sindri_tui::{Card, Launch};
use sindri_types::ExitCode;

use crate::boot::{self, Boot};
use crate::stack::{self, Stack};
use crate::{failure, signal};

pub fn run(overrides: Overrides) -> i32 {
    let (configured, missing) = match boot::prepare(overrides.clone()) {
        Ok(configured) => (configured, None),
        // A value the run cannot do without is a line and an exit everywhere
        // else; the frame is the one door that opens on it, because a card has
        // no field to type a key into. What is absent is read from
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
    let (env, config) = (&configured.env, &configured.config);
    if let Err(error) = doctor::probe(env, &config.extra_writable, config.enforcement) {
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
    // down between prompts by the frame.
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
        // The session's own probe, not a second one: the rail states the
        // branch the record holds.
        branch: boot.session.git().map(|git| git.branch.clone()),
        sandbox: posture(boot.session.policy()),
        provider: boot.config.provider.as_str().to_string(),
        model: boot.config.model.clone(),
        workspace: workspace(&boot.env.workspace),
        // The half of the catalog a human may reach for: `/skill` offers these,
        // and a load by hand is read from the same catalog the fold built.
        skills: boot.session.hand().names(),
        max_turns: boot.config.max_turns,
        // The one switch, read where both front ends read it: a run whose
        // human asked for no reports says nothing and clears nothing.
        status: boot.config.status,
        // The meter's denominator is the family's advertised window, where the
        // adapter knows it; where it does not, the rail counts up without one.
        context_limit: provider::context_window(&boot.config.model),
    }
}

/// The stack this launch runs a prompt on: the frame is where a human is, so
/// commands are dressed for the tap and the third tool comes with the host's own
/// opener — outside the sandbox, where the display and the browser are.
fn stack_of<'a>(boot: &'a Boot, dressed: &'a Dressed, cancel: Cancel) -> Stack<'a> {
    Stack {
        session: &boot.session,
        config: &boot.config,
        cancel,
        executor: dressed,
        opener: Some(Arc::new(crate::opener::Desktop::new())),
    }
}

/// The conversation this launch is having, wired onto this prompt's stack. It is
/// made by the first prompt or the first load by hand, whichever comes first, and
/// kept after: the thread it holds is the conversation's.
fn conversing(stack: &Stack<'_>, kept: Option<Agent>) -> (Agent, Receiver<Event>) {
    match kept {
        Some(mut agent) => {
            let events = stack.wire(&mut agent);
            (agent, events)
        }
        None => stack.agent(),
    }
}

struct Front {
    boot: Boot,
    overrides: Overrides,
    agent: Option<Agent>,
}

impl Driver for Front {
    fn run(&mut self, prompt: &str, cancel: Cancel, tap: Tap) -> Finished {
        let dressed = Dressed(tap.clone());
        let stack = stack_of(&self.boot, &dressed, cancel);
        let (mut agent, events) = conversing(&stack, self.agent.take());
        let forwarding = tap.clone();
        let forward = std::thread::spawn(move || {
            for event in events {
                forwarding.event(event);
            }
        });

        // The agent stays: the thread it holds is the conversation's. Only its
        // channel is dropped, so the forwarding thread sees the stream end.
        let outcome = agent.run(prompt);
        agent.close_events();
        let _ = forward.join();
        self.agent = Some(agent);

        // The record's end is written here, and a failure to write it is said in
        // the transcript rather than dropped.
        let note = closing_note(&self.boot.session, &outcome);
        match outcome {
            Ok(report) => Finished {
                stopped: report.stopped,
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

    fn load(&mut self, skill: &str, tap: Tap) -> Result<Invoked, String> {
        // No prompt's clock and no prompt's flag: a load reads a file, and the
        // run behind it is given the frame's flag when it starts. Nothing emits
        // on a load, so the channel composed with the agent is dropped here.
        let dressed = Dressed(tap.clone());
        let stack = stack_of(&self.boot, &dressed, Cancel::new());
        let (mut agent, _) = conversing(&stack, self.agent.take());
        let activated = agent.activate(skill);
        // The row and the chip are drawn from this event, whether a human asked
        // or the model did: one spelling of a load, read by one handler.
        if let Some(word) = &activated.outcome.facts.outcome {
            tap.event(Event::SkillLoaded {
                // Turn zero: a load by hand happens between turns, so it belongs
                // to no turn of a run.
                turn: 0,
                name: activated.name.clone(),
                outcome: Some(word.clone()),
                body: activated.outcome.content.clone(),
            });
        }
        self.agent = Some(agent);
        if activated.outcome.is_error {
            // The reason the result carries, in its own words, and the whole
            // result where it carries none.
            return Err(activated
                .outcome
                .facts
                .reason
                .clone()
                .unwrap_or_else(|| activated.outcome.content.trim().to_string()));
        }
        Ok(if activated.loaded {
            Invoked::Loaded(activated.name)
        } else {
            Invoked::Already(activated.name)
        })
    }

    fn renew(&mut self) -> Result<Launch, String> {
        // `/new` is boot's read again, in the environment this launch was booted
        // in — no second read of the process — and adopted only once
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
        // stderr.
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
    session
        .finish(outcome)
        .err()
        .map(|error| failure::record_note(session.dir().root(), &error))
}

/// The frame's executor, dressed for the tap: every line a command writes is
/// drawn as it arrives, and every result is drawn as the call that ended.
struct Dressed(Tap);

impl stack::Dress for Dressed {
    fn dress(&self, executor: SandboxExecutor) -> Arc<dyn BashExecutor> {
        let output = Arc::new(Told(self.0.clone()));
        Arc::new(Watched {
            inner: Arc::new(executor.with_output_sink(output)),
            tap: self.0.clone(),
        })
    }
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
    match sindri_core::paths::home() {
        Some(home) => match dir.strip_prefix(&home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => dir.display().to_string(),
        },
        None => dir.display().to_string(),
    }
}

#[cfg(test)]
mod tests;
