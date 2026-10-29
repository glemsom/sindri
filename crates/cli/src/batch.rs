//! Batch mode: the Unix filter front end. It reads the prompt and the stdin
//! context, runs the agent to an answer and prints it plus exactly one newline;
//! everything else is a decision made in `sindri-core`. Exit codes, not prose,
//! tell a script what happened (ADR-0004).

use std::io::{IsTerminal, Read};
use std::sync::Arc;
use std::sync::mpsc;

use clap::ValueEnum;
use sindri_core::agent::{Agent, BashTool, Event, Stopped};
use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, ConfigError, Overrides};
use sindri_core::exec::SandboxExecutor;
use sindri_core::output;
use sindri_core::provider::{Client, Error};
use sindri_core::record::{Recorder, Redaction, Start};
use sindri_core::sandbox::Enforcement;
use sindri_core::session;
use sindri_types::ExitCode;

use crate::{signal, verbose};

/// How a finished run reaches stdout (ADR-0004): the answer plus one newline,
/// which is what a shell pipeline wants, or one compact object, which is what a
/// parser wants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// The answer, then exactly one newline.
    #[default]
    Text,
    /// One compact object with a fixed field order.
    Json,
}

/// The flag overrides `sindri -b` accepts, taken from the command line as
/// written.
#[derive(Debug, Default)]
pub struct Flags {
    /// The shape stdout takes.
    pub format: OutputFormat,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub max_turns: Option<u32>,
    pub timeout: Option<u64>,
    /// Pay for the model's reasoning, or refuse to: both override the file
    /// and the environment (ADR-0005).
    pub thinking: Option<bool>,
    /// `--writable`, as written.
    pub writable: Option<Vec<String>>,
    /// `--header`, as written: `Name: value` lines.
    pub extra_headers: Option<Vec<String>>,
    /// Report the run on stderr when it is over.
    pub verbose: bool,
}

impl From<Flags> for Overrides {
    fn from(flags: Flags) -> Self {
        Overrides {
            provider: flags.provider,
            model: flags.model,
            base_url: flags.base_url,
            max_turns: flags.max_turns,
            timeout: flags.timeout,
            thinking: flags.thinking,
            writable: flags.writable,
            extra_headers: flags.extra_headers,
        }
    }
}

/// One batch run. Returns the exit code the shell sees.
pub fn run(prompt: Option<String>, flags: Flags) -> i32 {
    let prompt = match composed_prompt(prompt) {
        Ok(prompt) => prompt,
        Err(message) => return usage_error(&message),
    };

    let (format, verbose) = (flags.format, flags.verbose);
    let config = match Config::resolve(flags.into()) {
        Ok(config) => config,
        Err(error) => return config_error(&error),
    };
    // Nothing written by a clone may configure the agent, and staying silent
    // about it would leave a user editing a file that is read by nobody.
    if let Some(path) = &config.refused_repo_config {
        eprintln!(
            "sindri: ignoring {}: a config committed in a repository is never honoured (ADR-0005)",
            path.display()
        );
    }

    let (session, policy) = match session::prepare(&config.extra_writable, Enforcement::Bubblewrap)
    {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("sindri: sandbox: {error}");
            return ExitCode::EnvironmentFailure.code();
        }
    };

    // Every run leaves a record (ADR-0006), and a run that cannot write one is
    // not a run: the evidence is what makes the agent's actions reviewable.
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

    // One flag for the whole run: Ctrl-C raises it, the loop stops on it between
    // turns, and the executor kills a running command's group on it.
    let cancel = Cancel::new();
    signal::install(cancel.clone());

    let executor = Arc::new(
        SandboxExecutor::new(
            policy.clone(),
            config.timeout,
            session.spill(),
            cancel.clone(),
        )
        // bubblewrap's own account of each command goes into the same record,
        // document by document, so a killed run still says how far the sandbox got.
        .with_status_sink(recorder.clone()),
    );
    let tool = BashTool::new(&policy, executor);
    let mut agent = Agent::new(
        Box::new(Client::new(&config)),
        tool,
        config.max_turns,
        config.thinking,
    );
    // Events are buffered only for `-v`: a silent run should not carry a
    // transcript nobody asked for.
    let (sender, events) = mpsc::channel::<Event>();
    if verbose {
        agent.set_events(sender);
    }
    agent.set_cancel(cancel);
    agent.set_recorder(Arc::clone(&recorder));

    let outcome = agent.run(&prompt);
    // The record's end is written before anything is printed, so the session the
    // output names is already on disk and complete.
    let written = match &outcome {
        Ok(report) => recorder.finish(report),
        Err(error) => recorder.failed(error.message()),
    };
    if let Err(error) = written {
        eprintln!(
            "sindri: session {}: the record could not be finished: {error}",
            recorder.root().display()
        );
    }

    match outcome {
        Ok(report) => {
            // A cancelled run writes nothing: stdout must never let a
            // half-finished run be mistaken for an answer (ADR-0004). `-v` still
            // says what happened and where the record is, which is the whole
            // point of recording the run that went wrong.
            if report.stopped != Stopped::Cancelled {
                match format {
                    // The answer and exactly one newline, so `| wc -l` counts
                    // answers.
                    OutputFormat::Text => println!("{}", report.answer.trim_end()),
                    // One compact object on one line, so `| jq` reads fields.
                    OutputFormat::Json => println!("{}", output::to_json(&report, session.id())),
                }
            }
            if verbose {
                for line in verbose::lines(events.try_iter().collect(), session.id()) {
                    eprintln!("{line}");
                }
            }
            match report.stopped {
                Stopped::Final => ExitCode::Success.code(),
                Stopped::MaxTurns => ExitCode::TurnCap.code(),
                Stopped::Cancelled => ExitCode::Cancelled.code(),
                Stopped::Error => ExitCode::ModelError.code(),
            }
        }
        Err(error) => model_error(&error),
    }
}

/// The question actually asked: the prompt argument, with piped stdin appended
/// after it as context, or stdin itself when there is no argument (ADR-0004).
fn composed_prompt(prompt: Option<String>) -> Result<String, String> {
    let piped = read_stdin()?;
    let prompt = prompt.filter(|prompt| !prompt.trim().is_empty());
    let context = piped.filter(|context| !context.trim().is_empty());

    match (prompt, context) {
        (Some(prompt), Some(context)) => {
            Ok(format!("{}\n\n{}", prompt.trim_end(), context.trim_end()))
        }
        (Some(prompt), None) => Ok(prompt),
        (None, Some(context)) => Ok(context.trim_end().to_string()),
        (None, None) => Err(r#"no prompt: pass one with -b "…" or pipe one in"#.to_string()),
    }
}

/// Piped stdin, read only when there is a pipe to read: a terminal is not
/// drained, and neither is stdin when the run never reaches batch mode.
fn read_stdin() -> Result<Option<String>, String> {
    if std::io::stdin().is_terminal() {
        return Ok(None);
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| format!("could not read stdin: {error}"))?;
    Ok(Some(text))
}

/// A wrong command line or a wrong value: exit 2.
pub fn usage_error(message: &str) -> i32 {
    eprintln!("sindri: {message}");
    ExitCode::UsageError.code()
}

/// A configuration value that is wrong is a usage error; one that is absent is
/// a model error with a hint, as the batch contract promises. One wording for
/// both front ends.
pub fn config_error(error: &ConfigError) -> i32 {
    eprintln!("sindri: {}", error.message());
    match error {
        ConfigError::Usage(_) => ExitCode::UsageError.code(),
        ConfigError::Missing(_) => ExitCode::ModelError.code(),
    }
}

/// A provider or transport failure: the provider's message, one line, exit 1.
fn model_error(error: &Error) -> i32 {
    eprintln!("sindri: {}", provider_message(error));
    ExitCode::ModelError.code()
}

/// The provider's complaint as one line, so a shell and a transcript read the
/// same words.
pub fn provider_message(error: &Error) -> String {
    match error.status() {
        Some(status) => format!("provider error (HTTP {status}): {}", error.message()),
        None => format!("provider error: {}", error.message()),
    }
}
