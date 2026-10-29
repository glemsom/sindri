//! One run's setup: the session directory a record lives in, and the policy the
//! sandbox is built from. One place, so `doctor`, the debug subcommand and the
//! turn loop cannot drift apart.

use std::io;
use std::path::PathBuf;

use crate::paths::SessionDir;
use crate::sandbox::{
    Enforcement, FALLBACK_PATH, NetworkPosture, PolicyError, SandboxPolicy, WritableSet,
};

/// The policy every run starts from: the workspace and the extras writable,
/// everything else read-only, network shared, bubblewrap unless asked otherwise.
pub fn policy_for(
    workspace: &std::path::Path,
    extras: &[PathBuf],
    session_temp: PathBuf,
    search_path: String,
    enforcement: Enforcement,
) -> Result<SandboxPolicy, PolicyError> {
    let writable =
        WritableSet::new(std::iter::once(workspace.to_path_buf()).chain(extras.iter().cloned()))?;
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

/// The policy a session that is already running would be bounded by with these
/// extras: the same workspace and the same temp, so a saved setting is in force
/// for the very next command without a new session (ADR-0014).
pub fn rebounded(session: &SessionDir, extras: &[PathBuf]) -> Result<SandboxPolicy, String> {
    let workspace = std::env::current_dir()
        .and_then(|dir| dir.canonicalize())
        .map_err(|error| error.to_string())?;
    policy_for(
        &workspace,
        extras,
        session.temp(),
        search_path(),
        Enforcement::Bubblewrap,
    )
    .map_err(|error| error.to_string())
}

/// A fresh session directory with its temp and HOME created.
pub fn begin() -> io::Result<SessionDir> {
    let session = SessionDir::create()?;
    std::fs::create_dir_all(session.temp().join("home"))?;
    Ok(session)
}

/// One run's session and the policy its commands are bounded by: the workspace
/// and the configured extras writable, everything else read-only. The one place
/// every front end builds it, so `doctor`, the debug subcommand and the turn
/// loop cannot drift apart.
pub fn prepare(
    extras: &[PathBuf],
    enforcement: Enforcement,
) -> Result<(SessionDir, SandboxPolicy), String> {
    let session = begin().map_err(|error| error.to_string())?;
    let workspace = std::env::current_dir()
        .and_then(|dir| dir.canonicalize())
        .map_err(|error| error.to_string())?;
    let policy = policy_for(
        &workspace,
        extras,
        session.temp(),
        search_path(),
        enforcement,
    )
    .map_err(|error| error.to_string())?;
    Ok((session, policy))
}
