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

/// `$XDG_CONFIG_HOME/sindri`, falling back to `$HOME/.config/sindri`.
pub fn config_dir() -> io::Result<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => Ok(PathBuf::from(dir).join("sindri")),
        None => {
            let home = std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .ok_or_else(|| io::Error::other("neither XDG_CONFIG_HOME nor HOME is set"))?;
            Ok(PathBuf::from(home).join(".config/sindri"))
        }
    }
}

/// The one config file a run reads (ADR-0005). Never a repository-local one: a
/// checked-in file that grants writable paths turns cloning into privilege
/// escalation.
pub fn config_file() -> io::Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

/// One run's directory: `<state>/sindri/sessions/<guid>` (ADR-0006).
#[derive(Debug, Clone)]
pub struct SessionDir {
    root: PathBuf,
}

impl SessionDir {
    /// Creates the directory for a fresh session.
    pub fn create() -> io::Result<Self> {
        let root = state_dir()?.join("sessions").join(new_session_id()?);
        std::fs::create_dir_all(&root)?;
        Ok(SessionDir { root })
    }

    /// The session temp: writable scratch, and the parent of the sandbox HOME.
    pub fn temp(&self) -> PathBuf {
        self.root.join("tmp")
    }
}

/// A fresh session directory under the state directory.
pub fn new_session_dir() -> io::Result<SessionDir> {
    SessionDir::create()
}

/// A fresh session GUID, taken from the kernel rather than guessed at.
pub fn new_session_id() -> io::Result<String> {
    let uuid = std::fs::read_to_string("/proc/sys/kernel/random/uuid")?;
    Ok(uuid.trim().to_string())
}
