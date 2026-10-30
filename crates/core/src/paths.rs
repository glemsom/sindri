//! Where Sindri keeps its state: the state directory and the session
//! identifiers that name a record (ADR-0006).

use std::io;
use std::path::{Path, PathBuf};

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
    /// Creates the directory for a fresh session under `state`, the root the
    /// run environment names: the session's caller decides where state lives,
    /// so nothing below `Session::open` reads the process (ADR-0015).
    pub fn create_in(state: &Path) -> io::Result<Self> {
        let root = state.join("sessions").join(new_session_id()?);
        std::fs::create_dir_all(&root)?;
        Ok(SessionDir { root })
    }

    /// The session temp: writable scratch, and the parent of the sandbox HOME.
    pub fn temp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// Where over-long command output is written, one file per stream.
    pub fn spill(&self) -> PathBuf {
        self.temp().join("spill")
    }

    /// One session directory as it already exists on disk: what a reader opens,
    /// and what a test points at a temporary state directory.
    pub fn at(root: PathBuf) -> Self {
        SessionDir { root }
    }

    /// The directory itself, where the record's documents live (ADR-0006).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The GUID this run is recorded under (ADR-0006).
    pub fn id(&self) -> &str {
        self.root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    }
}

/// A fresh session GUID, taken from the kernel rather than guessed at.
pub fn new_session_id() -> io::Result<String> {
    let uuid = std::fs::read_to_string("/proc/sys/kernel/random/uuid")?;
    Ok(uuid.trim().to_string())
}
