//! One run's setup: the session directory a record lives in, and the policy the
//! sandbox is built from. One place, so `doctor`, the debug subcommand and the
//! turn loop cannot drift apart.

use std::io;
use std::path::PathBuf;

use crate::paths::SessionDir;
use crate::sandbox::{
    Enforcement, FALLBACK_PATH, NetworkPosture, PolicyError, SandboxPolicy, WritableSet,
};

/// The policy every run starts from: the workspace writable, everything else
/// read-only, network shared, bubblewrap unless asked otherwise.
pub fn policy_for(
    workspace: &std::path::Path,
    session_temp: PathBuf,
    search_path: String,
    enforcement: Enforcement,
) -> Result<SandboxPolicy, PolicyError> {
    let writable = WritableSet::new([workspace.to_path_buf()])?;
    SandboxPolicy::new(
        writable,
        session_temp,
        search_path,
        NetworkPosture::Shared,
        enforcement,
    )
}

/// The `PATH` the sandbox is given: the user's own, so tools in a nix profile or
/// `~/.local/bin` keep working.
pub fn search_path() -> String {
    std::env::var("PATH")
        .ok()
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| FALLBACK_PATH.to_string())
}

/// A fresh session directory with its temp and HOME created.
pub fn begin() -> io::Result<SessionDir> {
    let session = SessionDir::create()?;
    std::fs::create_dir_all(session.temp().join("home"))?;
    Ok(session)
}
