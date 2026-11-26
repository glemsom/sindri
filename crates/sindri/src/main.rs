mod auth;
mod batch;
#[cfg(test)]
mod caps;
mod cli;
mod doctor;
mod open;
mod osc;
mod paste;
mod reports;
mod sessions;
mod signal;
mod term;
mod tui;
mod window;

use std::process::ExitCode;

use clap::Parser;
use sindri_core::config::{Config, Process};
use sindri_core::first_run::FirstRun;

use crate::cli::{Cli, Command};

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

    if asked.is_none() || !first_run.config().is_ready() || cli.is_verbose() {
        eprintln!("{}", first_run.api_key_row());
    }

    if asked.is_none() && batch::stdin_is_terminal() {
        return tui::run(&cli, &first_run);
    }

    if first_run.config().missing().contains(&"api_key") {
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
        let pasted_key = Config {
            api_key: Some(pasted),
            ..Config::default()
        };
        if let Err(error) = Config::save(&Process, &pasted_key) {
            return refuse(&error.to_string(), error.exit_code());
        }
        first_run = match FirstRun::resolve(&Process, Config::default()) {
            Ok(first_run) => first_run,
            Err(error) => return refuse(&error.to_string(), error.exit_code()),
        };
    }

    if !first_run.config().is_ready() {
        return refuse(&short_of(&first_run), USAGE);
    }

    match asked {
        Some(job) => batch::run(job, &cli, &first_run),
        None if !batch::stdin_is_terminal() => refuse(
            "a pipe is context for a prompt, and a prompt is what -b is for",
            USAGE,
        ),
        None => tui::run(&cli, &first_run),
    }
}

fn short_of(first_run: &FirstRun) -> String {
    let missing = first_run.config().missing();
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
