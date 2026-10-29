//! Where Sindri keeps its state: the state directory and the session
//! identifiers that name a record (ADR-0006).

use std::io;
use std::path::PathBuf;

/// `$XDG_STATE_HOME/sindri`, falling back to `$HOME/.local/state/sindri`.
pub fn state_dir() -> io::Result<PathBuf> {
    match std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => Ok(PathBuf::from(dir).join("sindri")),
        None => {
            let home = std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .ok_or_else(|| io::Error::other("neither XDG_STATE_HOME nor HOME is set"))?;
            Ok(PathBuf::from(home).join(".local/state/sindri"))
        }
    }
}

/// A fresh session GUID, taken from the kernel rather than guessed at.
pub fn new_session_id() -> io::Result<String> {
    let uuid = std::fs::read_to_string("/proc/sys/kernel/random/uuid")?;
    Ok(uuid.trim().to_string())
}
