//! Batch mode: the Unix filter front end. It reads the prompt and the stdin
//! context, runs the agent to an answer and prints it plus exactly one newline;
//! everything else is a decision made in `sindri-core`, and the stack it runs on
//! is composed in `stack`. Exit codes, not prose, tell a script what happened.

use std::io::{IsTerminal, Read};

use clap::ValueEnum;
use sindri_core::agent::Stopped;
use sindri_core::cancel::Cancel;
use sindri_core::config::Overrides;
use sindri_core::output;
use sindri_core::provider::Error;
use sindri_types::ExitCode;

use crate::boot::{self, Boot};
use crate::{failure, notice, signal, stack, status, verbose};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

/// What batch mode adds to the overrides: how the answer is shaped and whether
/// the run's events are said on stderr. The command line is translated into
/// overrides once, in `main`, so no field here repeats one of them.
#[derive(Debug, Default)]
pub struct Flags {
    pub format: OutputFormat,
    pub verbose: bool,
    pub overrides: Overrides,
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
    } = match boot::open(flags.overrides) {
        Ok(boot) => boot,
        Err(failure) => return failure.report(),
    };
    // One line, on stderr, before anything runs: stdout is the answer.
    notice::unsafe_run(config.enforcement);
    // A rules file that cannot be read is a diagnostic and never a failure to
    // start: the run goes on, and the model is given no rules rather than half
    // of them.
    if let Some(line) = session.instructions().diagnostic() {
        eprintln!("sindri: {line}");
    }
    // One flag for the whole run: Ctrl-C raises it, the loop stops on it between
    // turns, and the executor kills a running command's group on it.
    let cancel = Cancel::new();
    signal::install(cancel.clone());
    // The stack is composed in one place and this front end says only what
    // differs: a script has no window and nobody to look at it, so nothing is
    // told of a command's output as it arrives, and no `open` is offered at all
    // — the schema never mentions it, rather than refusing a call in a result
    // nobody reads.
    let bare = stack::Bare;
    let (mut agent, events) = stack::Stack {
        session: &session,
        config: &config,
        cancel,
        executor: &bare,
        opener: None,
    }
    .agent();
    // The tap is unconditional: the reports are read from it, and `-v` reads
    // what the same thread kept. A run whose stderr is no terminal still taps —
    // the channel and the setting are decided in one place, and a second
    // decision here would be a second answer.
    let mut reports = status::Reports::new(session.workspace(), config.status);
    reports.begun();
    let watching = reports.watch(events, verbose);

    let outcome = agent.run(&prompt);
    // The channel closes here, so the thread reading it knows the run it was
    // carrying is over — and the end report is written before the answer is
    // printed, so a terminal and a stdout reader agree on the order.
    agent.close_events();
    let seen = watching.events();
    match &outcome {
        Ok(report) => reports.settled(report.stopped),
        Err(_) => reports.failed(),
    }
    // The record's end is written before anything is printed, so the session the
    // output names is already on disk and complete.
    if let Err(error) = session.finish(&outcome) {
        failure::record_failed(&session, &error);
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
                        println!(
                            "{}",
                            output::to_json(&report, session.dir().id(), config.enforcement)
                        )
                    }
                }
            }
            if verbose {
                for line in verbose::lines(
                    seen,
                    session.dir().id(),
                    session.instructions().handed().map(|handed| handed.line()),
                ) {
                    eprintln!("{line}");
                }
            }
            match report.stopped {
                Stopped::Final => ExitCode::Success.code(),
                Stopped::MaxTurns => ExitCode::TurnCap.code(),
                Stopped::Budget => ExitCode::Budget.code(),
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
