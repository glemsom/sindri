//! Batch mode is the Unix filter: a prompt in, the answer and one newline out,
//! and every word of chatter on stderr instead of on stdout.
//!
//! What stdout carries is §15's contract and nothing else — the answer and exactly
//! one newline, or nothing at all when the run failed or was cancelled, or §15's
//! one compact object under `--format json` — and the exit code is the stop
//! reason's own, read off `Outcome::exit_code` so this front end cannot hold a
//! second opinion about how a run ended (ADR-0006).
//!
//! The prompt is the command line's, with a pipe appended after it as context; a
//! pipe with no prompt *is* the prompt, and a pipe with no `-b` is neither — a
//! terminal UI off a terminal is a refusal and not a guess (§3). A pipe is never
//! read as a credential: in batch mode a key is pasted from a terminal or refused.
//!
//! The run stands where the process does: the session is created under the state
//! home, the boundary grants the workspace, the catalog is what the workspace and
//! the home offer, the preamble is the workspace's, the tools are a batch run's
//! two, and the record is opened before the first turn so a killed run still
//! reads (§13). Nothing here decides a fact the core could decide: this module
//! assembles the values and writes the answer.

use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use sindri_core::asking::Asking;
use sindri_core::config::{Config, Process};
use sindri_core::outcome::{Facts as Reported, Outcome};
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::preamble::Preamble;
use sindri_core::provider::Deltas;
use sindri_core::provider::{Ask, Client, Credential};
use sindri_core::records::{Facts, Record};
use sindri_core::run::{Cancel, Limits, Run, Shell, Stop};
use sindri_core::session::{self, Session};
use sindri_core::skills::Catalog;
use sindri_core::tools::{FrontEnd, ToolSet};
use sindri_core::wire::Wire;

use crate::cli::{Cli, Format};

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

/// One batch run: the session, the boundary, the tools, the record and the loop,
/// driven to a stop reason, and stdout written from what it left.
pub fn run(job: Job, cli: &Cli, config: &Config) -> ExitCode {
    let started = Instant::now();
    let prompt = match job.prompt() {
        Ok(prompt) => prompt,
        Err(usage) => return refuse(&usage, USAGE),
    };
    let (Some(wire), Some(model)) = (config.provider, config.model.clone()) else {
        return refuse(
            "a run needs a provider and a model: set them in the config or in the environment",
            USAGE,
        );
    };

    let workspace = match std::env::current_dir() {
        Ok(workspace) => workspace,
        Err(error) => {
            return refuse(
                &format!("cannot read the working directory: {error}"),
                SETUP,
            );
        }
    };
    let state_home = match session::state_home(&Process) {
        Ok(path) => path,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };
    let session = match Session::create(&state_home) {
        Ok(session) => session,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };
    let boundary = match Boundary::from_config(config, &workspace, Enforcement::Bubblewrap) {
        Ok(boundary) => boundary,
        Err(error) => return refuse(&error.to_string(), SETUP),
    };
    let limits = Limits::from_config(config, &boundary);
    let catalog = Catalog::discover(&workspace, home().as_deref());
    let tools = ToolSet::new(&boundary, &catalog, FrontEnd::Batch);
    let secrets = config.secrets();
    let client = Client::http(headers(config), session.guid(), secrets.clone());
    let asking = Asking::new(client, ask(config, wire, model.clone(), &workspace));
    let executor = Shell::new(boundary.clone(), session.temp(), catalog);

    let mut record = match Record::open(
        &session,
        Facts {
            wire,
            model: &model,
            boundary: &boundary,
            limits: &limits,
            workspace: &workspace,
            secrets: &secrets,
        },
    ) {
        Ok(record) => record,
        Err(error) => return refuse(&error.to_string(), SETUP),
    };

    narrate(
        cli,
        &format!(
            "wire {}, model {model}, enforcement {}",
            wire.word(),
            boundary.enforcement().word()
        ),
    );
    narrate(cli, &format!("workspace {}", workspace.display()));
    narrate(cli, &format!("session {}", session.directory().display()));

    let mut run = Run::new(prompt, asking.clone(), executor)
        .limits(limits)
        .tools(&tools)
        .cancel(Cancel::on_sigint());
    let mut quiet = Quiet;
    while run.state().is_running() {
        run.turn(&mut quiet);
        // The loop hands a front end no hook per entry, so a turn is written whole
        // as it ends: the turn it was, and what the assistant said. A call and its
        // result need the loop to say when they happen, which is #17's seam to
        // grow; nothing in this build's wire can ask for one (ADR-0016).
        if let Some(exchange) = run.history().last() {
            note(record.turn_started(exchange.turn));
            note(record.answered(exchange.turn, &exchange.text));
            narrate(cli, &format!("turn {}", exchange.turn));
        }
    }

    let state = run.state();
    let usage = asking.spent();
    note(record.spent(usage));
    note(record.stopped(&state));
    let outcome = match Outcome::of(
        &run,
        Reported {
            session: session.guid(),
            model: &model,
            enforcement: boundary.enforcement(),
            duration: started.elapsed(),
            usage: record.document().usage,
        },
    ) {
        Some(outcome) => outcome,
        None => return refuse("the run ended with no stop reason to report", SETUP),
    };

    narrate(
        cli,
        &format!(
            "stopped {} in {} ms, {} turns, {} tool calls",
            outcome.stopped(),
            started.elapsed().as_millis(),
            outcome.turns(),
            outcome.tool_calls(),
        ),
    );
    // A run that did not answer says why on stderr: stdout is the answer's alone,
    // so the reason a script cannot see in stdout is the one it reads here (§15).
    if let Some(reason) = why(&outcome, limits.max_turns()) {
        eprintln!("sindri: {reason}");
    }
    // The temp is emptied and everything else kept, so a session that ended still
    // reads beside one that was killed (§13).
    note(finish(record));

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

    // §15 ends the verbose stream with the session GUID, whatever the exit code.
    if cli.is_verbose() {
        eprintln!("{}", outcome.session());
    }
    ExitCode::from(outcome.exit_code())
}

/// The ask a run is built from: the dialect the config named, the served name, the
/// endpoint when one was set, the credential, and the preamble the workspace
/// stands in front of, re-sent every turn.
fn ask(config: &Config, wire: Wire, model: String, workspace: &Path) -> Ask {
    let mut ask = Ask::new(wire, model).preamble(Preamble::for_workspace(workspace).render());
    if let Some(base_url) = &config.base_url {
        ask = ask.base_url(base_url);
    }
    ask = match &config.api_key {
        Some(key) => ask.credential(Credential::new(key)),
        None => ask.credential(Credential::none()),
    };
    ask.thinking(config.thinking.unwrap_or(false))
}

/// The run's extra request headers, as the config writes them: `Name: value`,
/// split at the first colon. A line with no colon is dropped rather than sent as a
/// name with an empty value.
fn headers(config: &Config) -> Vec<(String, String)> {
    config
        .extra_headers
        .iter()
        .flatten()
        .filter_map(|header| {
            let (name, value) = header.split_once(':')?;
            Some((name.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// The home the user-scope skills come from, when the environment names one.
fn home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
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

/// A record that would not write is a diagnostic and not a reason to stop a run
/// that is already doing what it was asked: the session that remains still reads.
fn note(written: Result<(), impl std::fmt::Display>) {
    if let Err(error) = written {
        eprintln!("sindri: {error}");
    }
}

/// Ends the session: the temp is emptied and the directory is kept.
fn finish(record: Record) -> Result<(), impl std::fmt::Display> {
    record.finish()
}

/// The one way this front end says no: the reason on stderr, the code §15 gives
/// it, and nothing at all on stdout.
fn refuse(message: &str, code: u8) -> ExitCode {
    eprintln!("sindri: {message}");
    ExitCode::from(code)
}
