//! The bash executor: one port, two implementations. The turn loop talks to the
//! port, so loop tests need no bubblewrap; the sandbox is proven where it lives.

mod result;
mod sandboxed;
mod status;

pub use result::{Cut, Outcome, OutputLimit, ToolResult, truncate};
pub use sandboxed::{DEFAULT_TIMEOUT_SECS, MAX_TIMEOUT_SECS, SandboxExecutor};

use std::time::Duration;

/// The single seam between the turn loop and the sandbox.
pub trait BashExecutor: Send + Sync {
    /// Runs one `bash` script and reports a structured outcome.
    fn execute(&self, script: &str) -> ToolResult;
}

/// Reads the configured command timeout, refusing anything outside the contract.
pub fn timeout_from_secs(secs: u64) -> Result<Duration, String> {
    if secs == 0 || secs > MAX_TIMEOUT_SECS {
        return Err(format!(
            "timeout must be between 1 and {MAX_TIMEOUT_SECS} seconds, got {secs}"
        ));
    }
    Ok(Duration::from_secs(secs))
}
