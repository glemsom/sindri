mod auth; // the one command that signs in rather than runs
mod batch; // is the Unix filter: a prompt in, the answer and one newline out
mod cell; // the cell-size question and the bounded wait for its answer
mod cli; // the command line as clap sees it
mod doctor; // the honest window: it proves the sandbox
mod launch; // one launch's run, assembled the same way whichever front end asked for it
// Nothing in this binary calls the opener yet: the tool-call rows that will are not built.
#[allow(dead_code)]
mod open; // the host spawns the opener, because the sandbox has no display
mod osc; // sequences that carry words out of the terminal
mod paste; // the terminal's echo is off while the key is typed
mod reports; // the threads a run and a wire listing land their reports on, and the pass that takes them
mod sessions; // reads past runs back
mod signal; // the one door an outside `SIGINT` comes through
mod term; // the terminal as a resource: raw mode, the alternate screen, the panic restore, the canvas, and the telling
mod tui; // the launch: the loop, a key's outcome, where a prompt starts a run, and the endings
mod window; // the terminal outside the frame: its title, its mark, its notification and its focus

use std::process::ExitCode;

use clap::Parser;
use sindri_core::config::{Config, Flags, Process};
use sindri_core::first_run::FirstRun;

use crate::cli::{Cli, Command};

/// The exit code for a command line, a prompt or an answer that cannot be used.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    let mut cli = Cli::parse();
    let asked = batch::Job::asked(&cli);

    if let Some(command) = cli.command.take() {
        return match command {
            Command::Auth { action } => auth::run(action),
            Command::Doctor => doctor::run(&cli),
            Command::Sessions { action } => sessions::run(action),
        };
    }

    let mut first_run = match FirstRun::resolve(&Process, cli.flags()) {
        Ok(first_run) => first_run,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };

    // On stderr, because stdout is the answer's alone.
    if asked.is_none() || !first_run.is_ready() || cli.is_verbose() {
        eprintln!("{}", first_run.api_key_row());
    }

    if first_run.missing().contains(&"api_key") {
        // A pipe is words to ask and never a credential: a key is pasted only at a terminal.
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
        // The flag layer is dropped after the save: the file is the only copy of the key.
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
        None => tui::run(&cli, first_run.config()),
    }
}

fn short_of(first_run: &FirstRun) -> String {
    let missing = first_run.missing();
    format!(
        "a run needs {}: set {} in {} or in the environment",
        missing.join(", "),
        if missing.len() == 1 { "it" } else { "them" },
        first_run.path().display()
    )
}

pub(crate) fn refuse(message: &str, code: u8) -> ExitCode {
    eprintln!("sindri: {message}");
    ExitCode::from(code)
}
