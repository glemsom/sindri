//! Batch mode is the Unix filter: a prompt in, the answer and one newline out,
//! and every word of chatter on stderr instead of on stdout.
//!
//! What stdout carries is §15's contract and nothing else — the answer and exactly
//! one newline, or nothing at all when the run failed or was cancelled, or §15's
//! one compact object under `--format json` — and the exit code is the stop
//! reason's own, read off `Outcome::exit_code` so this front end cannot hold a
//! second opinion about how a run ended (ADR-0006).
//!
//! The run's status leaves here too, by OSC 7501 and on stderr, and only when stderr is a
//! terminal: a status sequence written into a file or a pipe is bytes a script did not ask
//! for and cannot use, so a filter on a pipe is told nothing at all (§14).
//!
//! The prompt is the command line's, with a pipe appended after it as context; a
//! pipe with no prompt *is* the prompt, and a pipe with no `-b` is neither — a
//! terminal UI off a terminal is a refusal and not a guess (§3). A pipe is never
//! read as a credential: in batch mode a key is pasted from a terminal or refused.
//!
//! Boot proves the sandbox once, before a turn or a record (§6): a sandbox that
//! cannot be created is a card on stderr naming the failing check, the code §15
//! gives it, and nothing on stdout. `--yolo-unsafe` is the one route past that
//! card, and it has to be on this command line to be on this line of code.
//!
//! The run stands where the process does: the session is created under the state
//! home, the boundary grants the workspace, the catalog is what the workspace and
//! the home offer, the preamble is the workspace's, the tools are a batch run's
//! two, and the record is opened before the first turn so a killed run still
//! reads (§13). Nothing here decides a fact the core could decide: this module
//! assembles the values and writes the answer.

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

/// §15's setup failure: nothing to run *with* — a session that cannot be made, a
/// boundary that cannot be held, an answer that cannot be written.
const SETUP: u8 = 3;

/// §15's usage error: a command line or a prompt that cannot be used.
const USAGE: u8 = 2;

/// One batch run, as the command line asked for it: the prompt, when the command
/// line named one, and nothing else until the config has been resolved.
pub struct Job {
    /// The prompt, when `-b` carried one: otherwise the pipe is the prompt.
    prompt: Option<String>,
}

impl Job {
    /// The batch run `cli` asks for, or `None` when this launch is not batch mode
    /// at all: a launch with no `-b` belongs to the terminal UI, whose refusal off
    /// a terminal is §3's and #36's.
    pub fn asked(cli: &Cli) -> Option<Job> {
        cli.batch.as_ref().map(|prompt| Job {
            prompt: prompt.clone(),
        })
    }

    /// The words to ask: the prompt with any piped context appended after it, or
    /// the pipe alone when the command line named no prompt (§3).
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

/// A sink for a run nobody is watching: batch mode's answer is written once it is
/// whole, so the fragments the wire streams land here and go no further.
struct Quiet;

impl Deltas for Quiet {
    fn text(&mut self, _fragment: &str) {}

    fn reasoning(&mut self, _fragment: &str) {}
}

/// Whether stdin is a terminal, which is what tells a paste from a pipeline: a
/// pipe is words to ask, and a terminal is somebody who can be asked for a key.
pub fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}

/// What a pipe carried, when stdin is not a terminal. A terminal is never read
/// here: a run that waited on a keyboard nobody is at would hang on a prompt it
/// already had.
fn piped_words() -> Option<String> {
    if stdin_is_terminal() {
        return None;
    }
    let mut words = String::new();
    io::stdin().read_to_string(&mut words).ok()?;
    let words = words.trim_end().to_owned();
    (!words.is_empty()).then_some(words)
}

/// One batch run: the launch, the loop and what stdout is handed when it stops.
///
/// Everything a run is made of is `launch`'s (one assembly both front ends share); this
/// function is batch mode's voice and its contract — the prompt resolved off the command
/// line and the pipe, the run narrated on stderr when `-v` asked for it, and §15's one
/// answer plus one newline, or nothing at all, on stdout.
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
        match launch.drive(&prompt, &mut quiet, &mut spoken, cancel) {
            Ok(outcome) => outcome,
            Err(refusal) => return refuse(refusal.message(), refusal.code()),
        }
    };
    // A stop reason is a new state and is said at once, so a run that ended inside a
    // second of starting still says it ended.
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
    // The reason a run gave no answer goes to stderr, which is where a script reads it:
    // stdout is the answer's alone.
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

    // The session GUID is the last line of the verbose stream, whatever the run's stop reason.
    if cli.is_verbose() {
        eprintln!("{}", outcome.session());
    }
    ExitCode::from(outcome.exit_code())
}

/// Batch mode's voice about a run that goes on: one line per turn, on stderr and only when
/// `-v` asked for it, and one status per turn for a terminal that asked for none of it. What
/// a turn said is the record's fact and the JSON's, so the narration is the turn's number
/// and nothing else.
struct Says<'a> {
    /// The command line, for the one question of whether to say anything at all.
    cli: &'a Cli,
    /// How many turns have been narrated, which is what a line counts.
    turns: u32,
    /// What the run is doing, said at §14's pace rather than a turn at a time.
    telling: &'a mut Teller<io::Stderr>,
}

impl Turns for Says<'_> {
    fn turn(&mut self, ended: &Exchange) {
        self.turns = ended.turn;
        narrate(self.cli, &format!("turn {}", self.turns));
        // A report the pace withheld is said here, before this turn's own: a filter
        // nobody watches has no other clock to go by.
        quietly(self.telling.due());
        // A turn that asked for no call has ended the run, so the stop reason is the
        // next word a terminal hears.
        if !ended.calls.is_empty() {
            quietly(self.telling.say(Told::Asking {
                turn: self.turns + 1,
            }));
        }
    }
}

/// Writes the answer, and exactly one newline after it: a model that ended its
/// answer with a newline does not get a second one, because stdout's contract is
/// the answer and one newline and nothing else (§15).
fn write_line(line: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(line.trim_end_matches('\n').as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

/// Whether this launch tells its terminal what the run is doing: §14's one switch, on by
/// default, and only where stderr is a terminal — a file or a pipe gets nothing at all,
/// because a sequence is a thing only a terminal can read.
fn tells(config: &Config) -> bool {
    config.tells_status() && io::stderr().is_terminal()
}

/// A status nobody could write is not a reason to fail a run that is answering: a terminal
/// that has gone away is a terminal there is nothing left to tell.
fn quietly(written: io::Result<()>) {
    let _ = written;
}

/// One line of the run, on stderr and only when `-v` asked for it: stdout is the
/// answer's alone, so every word about the run goes here.
fn narrate(cli: &Cli, line: &str) {
    if cli.is_verbose() {
        eprintln!("sindri: {line}");
    }
}

/// Why a run that produced no answer produced none, in one line: an answered run
/// says nothing here, because its answer is on stdout already.
fn why(outcome: &Outcome, max_turns: u32) -> Option<String> {
    match outcome.stopped() {
        Stop::Final => None,
        Stop::MaxTurns => Some(format!("the run reached its cap of {max_turns} turns")),
        Stop::Budget(budget) => Some(format!("the run spent its {budget} budget")),
        Stop::Cancelled => Some(String::from("the run was cancelled")),
        Stop::Error(message) => Some(message.to_string()),
    }
}

/// The one way this front end says no: the reason on stderr, the code §15 gives
/// it, and nothing at all on stdout.
fn refuse(message: &str, code: u8) -> ExitCode {
    eprintln!("sindri: {message}");
    ExitCode::from(code)
}
