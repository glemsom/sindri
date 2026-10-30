//! `sindri sessions show <id>`: the record one run left behind, read back.
//! Reading only — nothing here writes, deletes or prunes.

use sindri_core::record::{ShowError, Stored};
use sindri_types::ExitCode;

/// Prints one session's transcript. A wrong id is a usage error; a record that
/// cannot be read is the environment's failure, not the user's.
pub fn show(id: &str) -> i32 {
    match Stored::load(id) {
        Ok(stored) => {
            print!("{}", stored.render());
            ExitCode::Success.code()
        }
        Err(error) => {
            eprintln!("sindri: sessions: {error}");
            match error {
                ShowError::BadId { .. } | ShowError::Missing { .. } => ExitCode::UsageError.code(),
                ShowError::Unreadable { .. } | ShowError::Corrupt { .. } => {
                    ExitCode::EnvironmentFailure.code()
                }
            }
        }
    }
}
