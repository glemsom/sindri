//! The Sindri front ends: the terminal UI and batch mode, both driving one core
//! that owns every decision either of them makes.
//!
//! Two paths leave this function. A launch with no prompt is the terminal UI's,
//! and off a terminal it is a refusal rather than a guess (§3); the frame itself
//! arrives with #36, so what a configured launch says today is that it is
//! configured. A launch with `-b` is batch mode (§3), which resolves the same
//! first run — an API-key row that says where a key comes from and where it will
//! be written, a key pasted from a terminal when none was supplied — and then
//! hands the prompt to the core and writes the answer, the JSON object or the
//! exit code that §15 gives the run's stop reason.

mod auth; // the one command that signs in rather than runs
mod batch; // is the Unix filter: a prompt in, the answer and one newline out
mod cli; // the command line as clap sees it
// The host spawns the opener and this binary does not open a tool call yet: #40's
// tool-call rows are what will call it, so until one lands the module is complete
// and unreached.
#[allow(dead_code)]
mod open; // the host spawns the opener, because the sandbox has no display
mod osc; // sequences that carry words out of the terminal
mod paste; // the terminal's echo is off while the key is typed
mod sessions; // reads past runs back
mod tui; // the frame, the rail, the composer and the queue

use std::process::ExitCode;

use clap::Parser;
use sindri_core::config::{Config, Flags, Process};
use sindri_core::first_run::FirstRun;

use crate::cli::{Cli, Command};

/// What a configured launch says while the terminal UI is not yet built: the
/// front end has everything a run needs and no run to give it to.
const NOTHING_TO_RUN: u8 = 3;

/// Spec §15's usage error: a command line, a prompt or an answer that cannot be
/// used.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    let cli = Cli::parse();
    // A prompt on the command line is batch mode; a launch with none is the
    // terminal UI's, which is asked for its own reasons below.
    let asked = batch::Job::asked(&cli);

    // A command that runs nothing is answered here, before a config is resolved
    // or a key is asked for: a sign-in needs neither.
    if let Some(command) = cli.command {
        return match command {
            Command::Auth { action } => auth::run(action),
            Command::Sessions { action } => sessions::run(action),
        };
    }

    let mut first_run = match FirstRun::resolve(&Process, cli.flags()) {
        Ok(first_run) => first_run,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };

    // The row goes to stderr, not stdout: stdout is the answer's alone (§15), and
    // a fact about where a secret will live is not part of an answer. A batch run
    // that is already configured keeps it for `-v`, because a filter's stderr is
    // the only thing a person reads when something goes wrong.
    if asked.is_none() || !first_run.is_ready() || cli.is_verbose() {
        eprintln!("{}", first_run.api_key_row());
    }

    if first_run.missing().contains(&"api_key") {
        // In batch mode a pipe is words to ask and never a credential, so a key is
        // only pasted when there is a terminal to paste it at.
        if asked.is_some() && !batch::stdin_is_terminal() {
            return refuse(&short_of(&first_run), USAGE);
        }
        let pasted = match paste::ask() {
            Ok(pasted) => pasted,
            Err(error) => return refuse(&format!("cannot read a pasted api key: {error}"), USAGE),
        };
        if pasted.is_empty() {
            return refuse(&short_of(&first_run), USAGE);
        }
        // The key is written where the row said it would be, owner-only, and is
        // not held in the flag layer afterwards: a file is the only copy.
        let touched = Flags {
            api_key: Some(pasted),
            ..Flags::default()
        };
        if let Err(error) = Config::save(&Process, &touched) {
            return refuse(&error.to_string(), error.exit_code());
        }
        first_run = match FirstRun::resolve(&Process, Flags::default()) {
            Ok(first_run) => first_run,
            Err(error) => return refuse(&error.to_string(), error.exit_code()),
        };
    }

    if !first_run.is_ready() {
        return refuse(&short_of(&first_run), USAGE);
    }

    match asked {
        Some(job) => batch::run(job, &cli, first_run.config()),
        None if !batch::stdin_is_terminal() => refuse(
            "a pipe is context for a prompt, and a prompt is what -b is for",
            USAGE,
        ),
        // #36 owns the frame. Naming it is the honest answer, and this line goes
        // the day the terminal UI lands.
        None => {
            eprintln!("sindri: configured; the terminal UI arrives with ticket #36");
            ExitCode::from(NOTHING_TO_RUN)
        }
    }
}

/// What a first run is still short of, said as the config keys it wants and the
/// two places they can come from: one message, whichever way a key was missing.
fn short_of(first_run: &FirstRun) -> String {
    let missing = first_run.missing();
    format!(
        "a run needs {}: set {} in {} or in the environment",
        missing.join(", "),
        if missing.len() == 1 { "it" } else { "them" },
        first_run.path().display()
    )
}

/// The one way this front end says no: the reason on stderr, the code spec §15
/// gives it, and nothing at all on stdout.
pub(crate) fn refuse(message: &str, code: u8) -> ExitCode {
    eprintln!("sindri: {message}");
    ExitCode::from(code)
}
