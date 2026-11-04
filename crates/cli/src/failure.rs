//! The vocabulary of a failure: the words that follow `sindri: ` and the exit
//! code the shell reads, in one module, so a front end never learns
//! its failure words from another front end. `batch`, `frame`, `doctor` and
//! `sandbox` all print failures; none of them owns what a failure says.

use std::fmt;
use std::io;
use std::path::Path;

use sindri_core::config::ConfigError;
use sindri_core::provider::Error;
use sindri_core::session::{Session, StartError};
use sindri_types::ExitCode;

#[derive(Debug)]
pub(crate) enum Failure {
    Environment(io::Error),
    Config(ConfigError),
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
            Failure::Config(error) => f.write_str(error.message()),
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

/// The one sentence a run says when its record cannot be finished: the record's
/// own directory and the error that stopped it. Batch mode and `sandbox exec`
/// print it after `sindri: `, and the frame draws it in the transcript — none of
/// them spells it itself, so a reader who learns it once knows all three.
pub(crate) fn record_note(dir: &Path, error: &io::Error) -> String {
    format!(
        "session {}: the record could not be finished: {error}",
        dir.display()
    )
}

/// The same sentence, said on stderr by a front end whose contract is a stream:
/// one spelling and one printer, so batch mode and `sandbox exec` cannot drift
/// apart in the words a script sees.
pub(crate) fn record_failed(session: &Session, error: &io::Error) {
    eprintln!("sindri: {}", record_note(session.dir().root(), error));
}

pub(crate) fn provider_message(error: &Error) -> String {
    match error.status() {
        Some(status) => format!("provider error (HTTP {status}): {}", error.message()),
        None => format!("provider error: {}", error.message()),
    }
}

/// The same failure in the cells a settings row has: the status the wire
/// answered with, and never its body — a gateway that answers an unknown route
/// with a page of HTML would spend the whole row saying `<!DOCTYPE` — or the
/// one word a wire that said nothing at all earns, because a number is what a
/// refusal has and a timeout has none. The row is the room; the transcript is
/// where a run says a provider's own words.
pub(crate) fn listing_reason(status: Option<u16>) -> String {
    match status {
        Some(status) => format!("HTTP {status}"),
        None => "no answer".to_string(),
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

    fn said(failure: &Failure) -> (String, i32) {
        (failure.to_string(), failure.code())
    }

    fn io_error(why: &str) -> io::Error {
        io::Error::other(why)
    }

    #[test]
    fn a_failed_listing_is_a_status_and_never_a_body() {
        assert_eq!(
            listing_reason(None),
            "no answer",
            "a wire that said nothing at all is not a wire that said no"
        );
        // A gateway's unknown route answers with a page: the status is the
        // whole of it that fits the row a human reads.
        assert_eq!(listing_reason(Some(404)), "HTTP 404");
        assert_eq!(listing_reason(Some(401)), "HTTP 401");
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
    fn a_record_that_cannot_be_finished_is_one_sentence_naming_its_directory() {
        assert_eq!(
            record_note(
                Path::new("/state/sessions/4f1c8f2e"),
                &io_error("no space left on device")
            ),
            "session /state/sessions/4f1c8f2e: the record could not be finished: \
             no space left on device",
            "one sentence, the same in every front end that says it"
        );
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
