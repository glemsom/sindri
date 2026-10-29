//! `sindri sandbox exec`: run one command through the real sandbox and print the
//! structured outcome. A diagnostic, not a front end: no model is involved, so
//! a broken sandbox is visible before an agent can be confused by it.
//!
//! Its exit status reports whether the *sandbox* worked: 0 whenever an outcome
//! was produced, 3 when the sandbox itself failed. The command's own status is
//! in the JSON, and only there — that is the whole point of the outcome.

use std::path::PathBuf;
use std::time::Duration;

use sindri_core::exec::{BashExecutor, SandboxExecutor};
use sindri_core::sandbox::Enforcement;
use sindri_core::session;
use sindri_types::ExitCode;

/// Runs the command and prints one JSON object describing what happened.
pub fn exec(timeout: Duration, command: &[String]) -> i32 {
    let prepared = session::begin().and_then(|session| {
        let session_temp = session.temp();
        let policy = session::policy_for(
            &workspace()?,
            session_temp.clone(),
            session::search_path(),
            Enforcement::Bubblewrap,
        )
        .map_err(|error| std::io::Error::other(error.to_string()))?;
        Ok((policy, session_temp))
    });

    let (policy, session_temp) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("sindri: sandbox exec: {error}");
            return ExitCode::EnvironmentFailure.code();
        }
    };

    let script = shell_join(command);
    let executor = SandboxExecutor::new(policy, timeout, session_temp.join("spill"));
    let result = executor.execute(&script);

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

fn workspace() -> std::io::Result<PathBuf> {
    std::env::current_dir()?.canonicalize()
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
