//! The one command that signs in rather than runs: it shows the code the device
//! flow asks a person to type, waits for GitHub as long as the flow says to, and
//! then says where the session was kept — a file of its own, owner-only, that is
//! never read back as an API key (§10).
//!
//! Both lines go to stderr, because stdout is the answer's alone (§15) and a
//! sign-in answers nothing; the row naming the file is the core's own value, so
//! the terminal UI can paint the same bytes and a test can assert on them.
//!
//! Nothing here decides what a failure means: the core hands back one reason
//! already in words a person can act on, and this prints it and exits 2, because
//! a launch that could not sign in has not run anything either.

use std::process::ExitCode;

use sindri_core::config::Process;
use sindri_core::provider::copilot::{self, Store};

use crate::cli::{AuthAction, LoginWire};
use crate::refuse;

/// Spec §15's usage error: a sign-in that did not finish is not an answer.
const USAGE: u8 = 2;

/// Runs `sindri auth …`, which today is one thing only.
pub fn run(action: AuthAction) -> ExitCode {
    match action {
        AuthAction::Login {
            wire: LoginWire::Copilot,
        } => login(),
    }
}

/// Signs in to Copilot: the code as the flow shows it, then the file the session
/// was written to and that nothing ever shows it again.
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
