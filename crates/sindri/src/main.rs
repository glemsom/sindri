//! The Sindri front ends: the terminal UI and batch mode, both driving one core
//! that owns every decision either of them makes.
//!
//! Today the front end reaches one path and no other: a first run. It resolves the
//! config with no assumption of a file, prints the API-key row that says where a
//! key comes from and where it will be written, and asks for the key when nothing
//! supplied one. The run itself arrives with the run loop (#17) and the batch front
//! end (#19); until then a configured first run says so rather than pretending to
//! have answered.

mod batch; // a prompt in, the answer and one newline out
mod cli; // the command line as clap sees it
mod osc; // sequences that carry words out of the terminal
mod paste; // the terminal's echo is off while the key is typed
mod tui; // the frame, the rail, the composer and the queue

use std::process::ExitCode;

use clap::Parser;
use sindri_core::config::{Config, Flags, Process};
use sindri_core::first_run::FirstRun;

use crate::cli::Cli;

/// Spec §15's setup failure: the front end is configured and has nothing to run
/// with, and a zero here would claim an answer nobody produced.
const NOTHING_TO_RUN: u8 = 3;

/// Spec §15's usage error: a command line or an answer that cannot be used.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    let cli = Cli::parse();
    // `--format` and `-v` are global so one launch of either front end accepts
    // them (§3). There is nothing for either to change until a run exists.
    let _ = (&cli.format, cli.verbose);

    let mut first_run = match FirstRun::resolve(&Process, cli.flags()) {
        Ok(first_run) => first_run,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };

    // The row goes to stderr, not stdout: stdout is the answer's alone (§15), and
    // a fact about where a secret will live is not part of an answer.
    eprintln!("{}", first_run.api_key_row());

    if first_run.missing().contains(&"api_key") {
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

    // #17 and #19 own the run and the batch front end. Naming them is the honest
    // answer, and this line goes the day either lands.
    eprintln!("sindri: configured; the run itself arrives with tickets #17 and #19");
    ExitCode::from(NOTHING_TO_RUN)
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
fn refuse(message: &str, code: u8) -> ExitCode {
    eprintln!("sindri: {message}");
    ExitCode::from(code)
}
