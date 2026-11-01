//! Shared values with no dependency on the rest of Sindri: the exit-code
//! taxonomy that the batch contract promises, and the build version.

/// The build version, derived from `git describe` at build time.
pub const VERSION: &str = env!("SINDRI_VERSION");
/// The commit date of the build, `YYYY-MM-DD`, never the wall clock.
pub const COMMIT_DATE: &str = env!("SINDRI_COMMIT_DATE");
/// `git describe` and the commit date as one line, `{version} ({date})`: the
/// string `--version` prints, and so the one a user sees. Injected by
/// `crates/types/build.rs`, which is where the two halves are cut.
pub const VERSION_LONG: &str = env!("SINDRI_VERSION_LONG");

/// Every way a `sindri` run can end, numbered as the batch contract promises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success,
    ModelError,
    UsageError,
    EnvironmentFailure,
    /// The turn cap was reached; a partial answer is allowed.
    TurnCap,
    /// The run was cancelled with SIGINT; stdout stays empty.
    Cancelled,
}

impl ExitCode {
    pub const fn code(self) -> i32 {
        match self {
            ExitCode::Success => 0,
            ExitCode::ModelError => 1,
            ExitCode::UsageError => 2,
            ExitCode::EnvironmentFailure => 3,
            ExitCode::TurnCap => 100,
            ExitCode::Cancelled => 130,
        }
    }
}
