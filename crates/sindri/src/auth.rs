use std::process::ExitCode;

use sindri_core::config::Process;
use sindri_core::provider::copilot::{self, Store};

use crate::cli::{AuthAction, LoginWire};
use crate::refuse;

/// The exit code for a sign-in that did not finish.
const USAGE: u8 = 2;

pub fn run(action: AuthAction) -> ExitCode {
    match action {
        AuthAction::Login {
            wire: LoginWire::Copilot,
        } => login(),
    }
}

fn login() -> ExitCode {
    let store = match Store::resolve(&Process) {
        Ok(store) => store,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };
    let signed_in = store.login(|code| eprintln!("sindri: to sign in, {code}"));

    match signed_in {
        Ok(()) => {
            eprintln!("{}", copilot::store_row(&store.path()));
            ExitCode::SUCCESS
        }
        Err(reason) => refuse(&reason.message().to_string(), USAGE),
    }
}
