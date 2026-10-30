//! Batch mode: the Unix filter front end. It reads the prompt and the stdin
//! context, runs the agent to an answer and prints it plus exactly one newline;
//! everything else is a decision made in `sindri-core`. Exit codes, not prose,
//! tell a script what happened.

use std::io::{IsTerminal, Read};
use std::sync::Arc;
use std::sync::mpsc;

use clap::ValueEnum;
use sindri_core::agent::{BashTool, Dials, Event, Stopped};
use sindri_core::cancel::Cancel;
use sindri_core::config::Overrides;
use sindri_core::output;
use sindri_core::provider::{Client, Error};
use sindri_types::ExitCode;

use crate::boot::{self, Boot};
use crate::{failure, signal, verbose};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Default)]
pub struct Flags {
    pub format: OutputFormat,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub max_turns: Option<u32>,
    pub timeout: Option<u64>,
    pub thinking: Option<bool>,
    pub writable: Option<Vec<String>>,
    pub extra_headers: Option<Vec<String>>,
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

pub fn run(prompt: Option<String>, flags: Flags) -> i32 {
    let prompt = match composed_prompt(prompt) {
        Ok(prompt) => prompt,
        Err(message) => return failure::usage_error(&message),
    };

    let (format, verbose) = (flags.format, flags.verbose);
    // One door starts the run: the process read once, the config resolved once,
    // the session opened once, in the one order. Boot has already said a refused
    // repository config, and a run that cannot be evidenced is not a run.
    let Boot {
        config, session, ..
    } = match boot::open(flags.into()) {
        Ok(boot) => boot,
        Err(failure) => return failure.report(),
    };
    // One flag for the whole run: Ctrl-C raises it, the loop stops on it between
    // turns, and the executor kills a running command's group on it.
    let cancel = Cancel::new();
    signal::install(cancel.clone());

    let executor = Arc::new(session.executor(config.timeout, cancel.clone()));
    let tool = BashTool::new(session.policy(), executor);
    let mut agent = session.agent(
        Dials {
            model: Box::new(Client::new(&config)),
            tool,
            max_turns: config.max_turns,
            thinking: config.thinking,
        },
        cancel.clone(),
    );
    let (sender, events) = mpsc::channel::<Event>();
    if verbose {
        agent.set_events(sender);
    }

    let outcome = agent.run(&prompt);
    // The record's end is written before anything is printed, so the session the
    // output names is already on disk and complete.
    if let Err(error) = session.finish(&outcome) {
        eprintln!(
            "sindri: session {}: the record could not be finished: {error}",
            session.dir().root().display()
        );
    }

    match outcome {
        Ok(report) => {
            // A cancelled run writes nothing: stdout must never let a
            // half-finished run be mistaken for an answer. `-v` still
            // says what happened and where the record is, which is the whole
            // point of recording the run that went wrong.
            if report.stopped != Stopped::Cancelled {
                match format {
                    OutputFormat::Text => println!("{}", report.answer.trim_end()),
                    OutputFormat::Json => {
                        println!("{}", output::to_json(&report, session.dir().id()))
                    }
                }
            }
            if verbose {
                for line in verbose::lines(events.try_iter().collect(), session.dir().id()) {
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

/// A provider or transport failure: the provider's message, one line, exit 1.
fn model_error(error: &Error) -> i32 {
    eprintln!("sindri: {}", failure::provider_message(error));
    ExitCode::ModelError.code()
}
