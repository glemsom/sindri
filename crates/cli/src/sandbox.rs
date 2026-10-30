//! `sindri sandbox exec`: run one command through the real sandbox and print the
//! structured outcome. A diagnostic, not a front end: no model is involved, so
//! a broken sandbox is visible before an agent can be confused by it.
//!
//! Its exit status reports whether the *sandbox* worked: 0 whenever an outcome
//! was produced, 3 when the sandbox itself failed. The command's own status is
//! in the JSON, and only there — that is the whole point of the outcome.

use std::time::{Duration, Instant};

use sindri_core::agent::{Report, Stopped};
use sindri_core::cancel::Cancel;
use sindri_core::config::{Config, Overrides};
use sindri_core::exec::{BashExecutor, Outcome, ToolResult};
use sindri_core::provider::{Error, Usage};
use sindri_core::session::{Env, LoopError, Session, StartError};
use sindri_types::ExitCode;

use crate::signal;

/// Runs the command and prints one JSON object describing what happened. A
/// session is opened the one way a run opens one, so a diagnostic cannot probe a
/// different sandbox than the run it is diagnosing (ADR-0015 §7).
pub fn exec(timeout: Duration, overrides: Overrides, command: &[String]) -> i32 {
    // A diagnostic asks no model, so it needs none of a model's settings: the
    // extras a run would be granted, and nothing a run would spend.
    let config = match Config::for_command(overrides) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("sindri: {}", error.message());
            return ExitCode::UsageError.code();
        }
    };
    let env = match Env::ambient() {
        Ok(env) => env,
        Err(error) => {
            eprintln!("sindri: sandbox exec: the run environment cannot be read: {error}");
            return ExitCode::EnvironmentFailure.code();
        }
    };
    let session = match Session::open(&env, &config) {
        Ok(session) => session,
        Err(StartError::Record { path, source }) => {
            eprintln!("sindri: session {}: {source}", path.display());
            return ExitCode::EnvironmentFailure.code();
        }
        Err(error) => {
            eprintln!("sindri: sandbox exec: {error}");
            return ExitCode::EnvironmentFailure.code();
        }
    };

    let script = shell_join(command);
    // Ctrl-C here kills the command's group too, so a diagnostic cannot leave a
    // process behind either.
    let cancel = Cancel::new();
    signal::install(cancel.clone());
    let executor = session.executor(timeout, cancel);
    let started = Instant::now();
    let result = executor.execute(&script);
    // The session ends whatever the command did: a record left saying `running`
    // could not be told from a diagnostic that is still going (ADR-0006).
    end(&session, &command_over(result.as_ref(), started.elapsed()));

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

/// One command's result as the session's end: no model was asked, so there were
/// no turns, and a sandbox that never started is the failure it was.
fn command_over(result: Option<&ToolResult>, took: Duration) -> Result<Report, LoopError> {
    let (stopped, tool_calls) = match result.map(|result| &result.outcome) {
        // Cancelled before anything ran.
        None => (Stopped::Cancelled, 0),
        Some(Outcome::SandboxError { detail }) => return Err(Error::transport(detail)),
        Some(_) => (Stopped::Final, 1),
    };
    Ok(Report {
        answer: String::new(),
        turns: 0,
        stopped,
        model: None,
        tool_calls,
        usage: Usage::default(),
        duration: took,
    })
}

/// Writes the session's end, and says a write that failed: a record nobody is
/// told about is a record dropped (ADR-0006).
fn end(session: &Session, outcome: &Result<Report, LoopError>) {
    if let Err(error) = session.finish(outcome) {
        eprintln!(
            "sindri: session {}: the record could not be finished: {error}",
            session.dir().root().display()
        );
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
