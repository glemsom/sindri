//! Where Sindri keeps its state: the state directory and the session
//! identifiers that name a record.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

pub fn state_dir() -> io::Result<PathBuf> {
    under_from(
        std::env::var_os("XDG_STATE_HOME"),
        "XDG_STATE_HOME",
        ".local/state/sindri",
    )
}

pub fn config_dir() -> io::Result<PathBuf> {
    under_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        "XDG_CONFIG_HOME",
        ".config/sindri",
    )
}

/// The one XDG rule: a value that names a directory roots a `sindri` leaf, and
/// an empty or absent one falls back under `$HOME`. It takes the value rather
/// than reading it, so the empty case is a test instead of an environment
/// mutation; `variable` names what was missing, so one message covers both.
fn under_from(value: Option<OsString>, variable: &str, fallback: &str) -> io::Result<PathBuf> {
    if let Some(dir) = value.filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir).join("sindri"));
    }
    let home = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| io::Error::other(format!("neither {variable} nor HOME is set")))?;
    Ok(PathBuf::from(home).join(fallback))
}

/// The one config file a run reads. Never a repository-local one: a
/// checked-in file that grants writable paths turns cloning into privilege
/// escalation.
pub fn config_file() -> io::Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

/// One run's directory: `<state>/sindri/sessions/<guid>`.
#[derive(Debug, Clone)]
pub struct SessionDir {
    root: PathBuf,
}

impl SessionDir {
    /// Creates the directory for a fresh session under `state`, the root the
    /// run environment names: the session's caller decides where state lives,
    /// so nothing below `Session::open` reads the process.
    pub fn create_in(state: &Path) -> io::Result<Self> {
        let root = state.join("sessions").join(new_session_id()?);
        std::fs::create_dir_all(&root)?;
        Ok(SessionDir { root })
    }

    /// The session temp: writable scratch, and the parent of the sandbox HOME.
    pub fn temp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn spill(&self) -> PathBuf {
        self.temp().join("spill")
    }

    pub fn at(root: PathBuf) -> Self {
        SessionDir { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

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
