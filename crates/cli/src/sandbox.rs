//! `sindri sandbox exec`: run one command through the real sandbox and print the
//! structured outcome. A diagnostic, not a front end: no model is involved, so
//! a broken sandbox is visible before an agent can be confused by it.
//!
//! Its exit status reports whether the *sandbox* worked: 0 whenever an outcome
//! was produced, 3 when the sandbox itself failed. The command's own status is
//! in the JSON, and only there — that is the whole point of the outcome.

use std::time::Duration;

use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, Overrides};
use sindri_core::exec::{BashExecutor, SandboxExecutor};
use sindri_core::sandbox::Enforcement;
use sindri_core::session;
use sindri_types::ExitCode;

/// Runs the command and prints one JSON object describing what happened. The
/// extras a run would be granted are granted here too, so a diagnostic cannot
/// probe a different sandbox than the run it is diagnosing.
pub fn exec(timeout: Duration, overrides: Overrides, command: &[String]) -> i32 {
    let extras = match Config::configured_extras(overrides) {
        Ok(extras) => extras,
        Err(error) => {
            eprintln!("sindri: {}", error.message());
            return ExitCode::UsageError.code();
        }
    };
    let (policy, session) = match session::prepare(extras.as_slice(), Enforcement::Bubblewrap) {
        Ok((session, policy)) => (policy, session),
        Err(error) => {
            eprintln!("sindri: sandbox exec: {error}");
            return ExitCode::EnvironmentFailure.code();
        }
    };

    let script = shell_join(command);
    // Ctrl-C here kills the command's group too, so a diagnostic cannot leave a
    // process behind either.
    let cancel = Cancel::new();
    crate::signal::install(cancel.clone());
    let executor = SandboxExecutor::new(policy, timeout, session.spill(), cancel);
    let Some(result) = executor.execute(&script) else {
        return ExitCode::Cancelled.code();
    };

    match serde_json::to_string(&result) {
        Ok(json) => {
            println!("{json}");
            if matches!(
                result.outcome,
                sindri_core::exec::Outcome::SandboxError { .. }
            ) {
                ExitCode::EnvironmentFailure.code()
            } else {
                ExitCode::Success.code()
            }
        }
        Err(error) => {
            eprintln!("sindri: sandbox exec: {error}");
            ExitCode::EnvironmentFailure.code()
        }
    }
}

/// Turns an argv into one script, quoting each word so the shell sees exactly
/// the arguments the caller wrote.
fn shell_join(command: &[String]) -> String {
    command
        .iter()
        .map(|word| format!("'{}'", word.replace('\'', r"'\''")))
        .collect::<Vec<_>>()
        .join(" ")
}
