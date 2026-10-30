//! The vocabulary of a failure: the words that follow `sindri: ` and the exit
//! code the shell reads (ADR-0004), in one module, so a front end never learns
//! its failure words from another front end. `batch`, `frame`, `doctor` and
//! `sandbox` all print failures; none of them owns what a failure says.

use std::fmt;
use std::io;

use sindri_core::config::ConfigError;
use sindri_core::provider::Error;
use sindri_core::session::StartError;
use sindri_types::ExitCode;

/// A run that could not start, in the two families the batch contract has
/// always printed: `sandbox: <why a session could not start>` and
/// `session <path>: <why the record could not>` (ADR-0004, ADR-0015 §5).
#[derive(Debug)]
pub(crate) enum Failure {
    /// The three ambient facts a session is opened in could not be read from
    /// the process (ADR-0015 §6).
    Environment(io::Error),
    /// A configuration value is wrong, or absent (ADR-0005).
    Config(ConfigError),
    /// The session could not be started: the environment's failure, whichever
    /// of its parts refused (ADR-0006, ADR-0015 §5).
    Start(StartError),
}

impl Failure {
    /// The code a shell reads: an environment or session failure is 3, a wrong
    /// config value is 2, an absent one is 1.
    pub(crate) fn code(&self) -> i32 {
        match self {
            Failure::Environment(_) | Failure::Start(_) => ExitCode::EnvironmentFailure.code(),
            Failure::Config(ConfigError::Usage(_)) => ExitCode::UsageError.code(),
            Failure::Config(ConfigError::Missing(_)) => ExitCode::ModelError.code(),
        }
    }

    /// Prints the one line and hands the code back: the whole of what a front
    /// end does with a failure.
    pub(crate) fn report(&self) -> i32 {
        eprintln!("sindri: {self}");
        self.code()
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Environment(error) => {
                write!(f, "the run environment cannot be read: {error}")
            }
            // The config's own words: it is the module that knows what is wrong
            // with a value, and its message carries the hint.
            Failure::Config(error) => f.write_str(error.message()),
            // A record is named by its own path, because a record that cannot be
            // written is not a run (ADR-0006).
            Failure::Start(StartError::Record { path, source }) => {
                write!(f, "session {}: {source}", path.display())
            }
            // The sandbox and the session directory are the environment's
            // failures, and the shell reads both as one: `sindri: sandbox:` is
            // the line the batch contract has always printed for either.
            Failure::Start(error) => write!(f, "sandbox: {error}"),
        }
    }
}

/// A wrong command line or a wrong value: exit 2. A misuse travels as a
/// `ConfigError::Usage` because it is the same failure to the shell — a value
/// as written cannot be honoured — and so earns the same one line and the same
/// code through the same `report`.
pub(crate) fn usage_error(message: &str) -> i32 {
    Failure::Config(ConfigError::Usage(message.to_string())).report()
}

/// A configuration value that is wrong is a usage error; one that is absent is
/// a model error with a hint, as the batch contract promises. One wording for
/// every front end.
pub(crate) fn config_error(error: ConfigError) -> i32 {
    Failure::Config(error).report()
}

/// The provider's complaint as one line, so a shell and a transcript read the
/// same words.
pub(crate) fn provider_message(error: &Error) -> String {
    match error.status() {
        Some(status) => format!("provider error (HTTP {status}): {}", error.message()),
        None => format!("provider error: {}", error.message()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken mapping is reported"
    )]

    use super::*;
    use std::path::PathBuf;

    /// The words and the code for one failure, as a shell would see them.
    fn said(failure: &Failure) -> (String, i32) {
        (failure.to_string(), failure.code())
    }

    fn io_error(why: &str) -> io::Error {
        io::Error::other(why)
    }

    #[test]
    fn an_unreadable_run_environment_is_named_as_one_and_exit_three() {
        let (line, code) = said(&Failure::Environment(io_error("no HOME")));

        assert_eq!(line, "the run environment cannot be read: no HOME");
        assert_eq!(code, 3, "the environment is the environment's failure");
    }

    #[test]
    fn a_wrong_config_value_is_its_own_message_and_exit_two() {
        let (line, code) = said(&Failure::Config(ConfigError::Usage(
            "max_turns must be at least 1".to_string(),
        )));

        assert_eq!(line, "max_turns must be at least 1");
        assert_eq!(code, 2, "a value that is wrong is a usage error");
    }

    #[test]
    fn an_absent_config_value_is_its_own_message_and_exit_one() {
        let (line, code) = said(&Failure::Config(ConfigError::Missing(
            "no model: set one with --model or in the config file".to_string(),
        )));

        assert_eq!(
            line, "no model: set one with --model or in the config file",
            "the hint travels with the message"
        );
        assert_eq!(code, 1, "a value that is absent is a model error");
    }

    #[test]
    fn a_record_that_cannot_be_started_names_its_path_and_exit_three() {
        let (line, code) = said(&Failure::Start(StartError::Record {
            path: PathBuf::from("/state/sessions/4f1c8f2e"),
            source: io_error("no space left on device"),
        }));

        assert_eq!(
            line, "session /state/sessions/4f1c8f2e: no space left on device",
            "a record is named by its own path, not by the sandbox"
        );
        assert_eq!(code, 3, "a run that cannot be evidenced is not a run");
    }

    #[test]
    fn a_session_that_cannot_start_is_the_sandbox_family_and_exit_three() {
        let (line, code) = said(&Failure::Start(StartError::Directory {
            path: PathBuf::from("/state/sessions"),
            source: io_error("Not a directory"),
        }));

        assert_eq!(
            line,
            "sandbox: the session directory /state/sessions cannot be created: Not a directory",
            "the sandbox and the session directory are one family"
        );
        assert_eq!(code, 3);
    }
}
