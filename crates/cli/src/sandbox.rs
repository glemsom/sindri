//! `sindri sandbox exec`: run one command through the real sandbox and print the
//! structured outcome. A diagnostic, not a front end: no model is involved, so
//! a broken sandbox is visible before an agent can be confused by it.
//! Its exit status reports whether the *sandbox* worked: 0 whenever an outcome
//! was produced, 3 when the sandbox itself failed. The command's own status is
//! in the JSON, and only there — that is the whole point of the outcome.
//! The timeout it runs under is the resolved setting and never a number of this
//! front end's own, so the command is bounded by the one a run would be given.

use std::time::{Duration, Instant};

use sindri_core::agent::Stopped;
use sindri_core::cancel::Cancel;
use sindri_core::config::Overrides;
use sindri_core::exec::{BashExecutor, Outcome, ToolResult};
use sindri_core::provider::Error;
use sindri_core::session::Session;
use sindri_types::ExitCode;

use crate::boot;
use crate::{failure, notice, signal};

/// Runs the command and prints one JSON object describing what happened. A
/// session is opened the one way a run opens one, so a diagnostic cannot probe a
/// different sandbox than the run it is diagnosing.
pub fn exec(overrides: Overrides, command: &[String]) -> i32 {
    let configured = match boot::configure(overrides) {
        Ok(configured) => configured,
        Err(failure) => return failure.report(),
    };
    notice::unsafe_run(configured.config.enforcement);
    let boot = match configured.start() {
        Ok(boot) => boot,
        Err(failure) => return failure.report(),
    };
    let session = boot.session;

    let script = shell_join(command);
    let cancel = Cancel::new();
    signal::install(cancel.clone());
    let executor = session.executor(boot.config.timeout, cancel);
    let started = Instant::now();
    let result = executor.execute(&script);
    // The session ends whatever the command did: a record left saying `running`
    // could not be told from a diagnostic that is still going.
    end(&session, result.as_ref(), started.elapsed());

    let Some(result) = result else {
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

/// Ends the session on the three facts a command leaves: it failed, it was
/// cancelled, or it ran. No model was asked here, so the record is written
/// without an answer rather than around an empty `Report`.
fn end(session: &Session, result: Option<&ToolResult>, took: Duration) {
    let written = match result.map(|result| &result.outcome) {
        None => session.finish_command(Stopped::Cancelled, 0, took),
        Some(Outcome::SandboxError { detail }) => session.finish(&Err(Error::transport(detail))),
        Some(_) => session.finish_command(Stopped::Final, 1, took),
    };
    if let Err(error) = written {
        failure::record_failed(session, &error);
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
