use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;
use std::time::Instant;

use sindri_core::config::Config;
use sindri_core::outcome::Outcome;
use sindri_core::provider::Deltas;
use sindri_core::records::Told;
use sindri_core::run::{Cancel, Exchange, Stop};
use sindri_core::tools::FrontEnd;

use crate::cli::{Cli, Format};
use crate::launch::{Launch, Turns};
use crate::osc::Teller;

/// The exit code for a run that had nothing to run *with*, or an answer it could not write.
const SETUP: u8 = 3;

/// The exit code for a command line or a prompt that cannot be used.
const USAGE: u8 = 2;

pub struct Job {
    prompt: Option<String>,
}

impl Job {
    pub fn asked(cli: &Cli) -> Option<Job> {
        cli.batch.as_ref().map(|prompt| Job {
            prompt: prompt.clone(),
        })
    }

    fn prompt(self) -> Result<String, String> {
        let piped = piped_words();
        let words = match (self.prompt, piped) {
            (Some(prompt), Some(context)) => format!("{prompt}\n\n{context}"),
            (Some(prompt), None) => prompt,
            (None, Some(prompt)) => prompt,
            (None, None) => String::new(),
        };
        if words.trim().is_empty() {
            return Err(String::from(
                "a batch run needs a prompt: write one after -b, or pipe one in",
            ));
        }
        Ok(words)
    }
}

/// Batch mode's answer is written once it is whole, so the fragments land and go no further.
struct Quiet;

impl Deltas for Quiet {
    fn text(&mut self, _fragment: &str) {}

    fn reasoning(&mut self, _fragment: &str) {}
}

pub fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}

/// A terminal is never read here: a run waiting on a keyboard nobody is at would hang.
fn piped_words() -> Option<String> {
    if stdin_is_terminal() {
        return None;
    }
    let mut words = String::new();
    io::stdin().read_to_string(&mut words).ok()?;
    let words = words.trim_end().to_owned();
    (!words.is_empty()).then_some(words)
}

pub fn run(job: Job, cli: &Cli, config: &Config) -> ExitCode {
    let started = Instant::now();
    let prompt = match job.prompt() {
        Ok(prompt) => prompt,
        Err(usage) => return refuse(&usage, USAGE),
    };
    let launch = match Launch::open(cli, config, FrontEnd::Batch) {
        Ok(launch) => launch,
        Err(refusal) => return refuse(refusal.message(), refusal.code()),
    };
    let figure = launch.figure();
    let max_turns = launch.max_turns();
    let mut telling = Teller::new(io::stderr(), tells(config), &figure.workspace);
    quietly(telling.say(Told::Asking { turn: 1 }));

    narrate(
        cli,
        &format!(
            "wire {}, model {}, enforcement {}",
            figure.wire.word(),
            figure.model,
            figure.enforcement.word()
        ),
    );
    if let Some(route) = figure.route {
        narrate(cli, &format!("route {route}"));
    }
    narrate(cli, &format!("workspace {}", figure.workspace.display()));
    narrate(cli, &format!("session {}", figure.session.display()));
    narrate(cli, &figure.proof);

    let cancel = Cancel::on_sigint();
    let mut quiet = Quiet;
    let outcome = {
        let mut spoken = Says {
            cli,
            turns: 0,
            telling: &mut telling,
        };
        match launch.drive(&prompt, &[], &mut quiet, &mut spoken, cancel) {
            Ok(outcome) => outcome,
            Err(refusal) => return refuse(refusal.message(), refusal.code()),
        }
    };
    // Said at once, so a run that ended inside a second of starting still says it ended.
    quietly(telling.say(Told::Stopped {
        turn: outcome.turns(),
        stop: outcome.stopped().clone(),
    }));

    narrate(
        cli,
        &format!(
            "stopped {} in {} ms, {} turns, {} tool calls",
            outcome.stopped(),
            started.elapsed().as_millis(),
            outcome.turns(),
            outcome.tool_calls()
        ),
    );
    // On stderr, where a script reads it: stdout is the answer's alone.
    if let Some(reason) = why(&outcome, max_turns) {
        eprintln!("sindri: {reason}");
    }

    let written = match cli.format {
        Format::Text => match outcome.answer() {
            Some(answer) => write_line(answer),
            None => Ok(()),
        },
        Format::Json => write_line(&outcome.json()),
    };
    if let Err(error) = written {
        return refuse(&format!("cannot write the answer: {error}"), SETUP);
    }

    // The last line of the verbose stream, whatever the run's stop reason.
    if cli.is_verbose() {
        eprintln!("{}", outcome.session());
    }
    ExitCode::from(outcome.exit_code())
}

struct Says<'a> {
    cli: &'a Cli,
    turns: u32,
    telling: &'a mut Teller<io::Stderr>,
}

impl Turns for Says<'_> {
    fn turn(&mut self, ended: &Exchange) {
        self.turns = ended.turn;
        narrate(self.cli, &format!("turn {}", self.turns));
        // Before this turn's own word: a filter nobody watches has no other clock to go by.
        quietly(self.telling.due());
        // A turn that asked for no call has ended the run, so the stop reason comes next.
        if !ended.calls.is_empty() {
            quietly(self.telling.say(Told::Asking {
                turn: self.turns + 1,
            }));
        }
    }
}

/// A model that ended its answer with a newline does not get a second one.
fn write_line(line: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(line.trim_end_matches('\n').as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

/// Only where stderr is a terminal: a sequence written into a file or a pipe is bytes a
/// script did not ask for.
fn tells(config: &Config) -> bool {
    config.tells_status() && io::stderr().is_terminal()
}

/// A status nobody could write does not fail a run that is answering: a terminal that has
/// gone away is one there is nothing left to tell.
fn quietly(written: io::Result<()>) {
    let _ = written;
}

fn narrate(cli: &Cli, line: &str) {
    if cli.is_verbose() {
        eprintln!("sindri: {line}");
    }
}

fn why(outcome: &Outcome, max_turns: u32) -> Option<String> {
    match outcome.stopped() {
        Stop::Final => None,
        Stop::MaxTurns => Some(format!("the run reached its cap of {max_turns} turns")),
        Stop::Budget(budget) => Some(format!("the run spent its {budget} budget")),
        Stop::Cancelled => Some(String::from("the run was cancelled")),
        Stop::Error(message) => Some(message.to_string()),
    }
}

fn refuse(message: &str, code: u8) -> ExitCode {
    eprintln!("sindri: {message}");
    ExitCode::from(code)
}
