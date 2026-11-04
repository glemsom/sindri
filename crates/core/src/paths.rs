//! Where Sindri keeps its state: the state directory and the session
//! identifiers that name a record.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// The home directory, when the process has one. `None` is a real answer — a
/// service or a container may have none — so a caller decides whether that is a
/// failure (`under_from`) or simply a scope that does not exist (skills).
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

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
    let home =
        home().ok_or_else(|| io::Error::other(format!("neither {variable} nor HOME is set")))?;
    Ok(home.join(fallback))
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

    /// The session temp: writable scratch, and nothing else. A state root that
    /// usually sits inside the home leaves this path inside a directory the
    /// sandbox has read-only, so the writable bind over it is mount order rather
    /// than accident.
    pub fn temp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn spill(&self) -> PathBuf {
        self.temp().join("spill")
    }

    /// The session temp emptied of the scratch a run left in it. Called when the
    /// session ends and never between calls: several `bash` calls of one session
    /// read what an earlier one wrote, and no call of the next session should
    /// read what this one did.
    ///
    /// `spill/` stays. It holds the output a truncated result names by path, and
    /// the record that names it outlives the run — a reader who follows that path
    /// finds the file the record promised. A failure to remove anything is not
    /// reported: a session that has ended can keep a file rather than gain a line
    /// no one could act on.
    pub fn clear_temp(&self) {
        let Ok(entries) = std::fs::read_dir(self.temp()) else {
            return;
        };
        let spill = self.spill();
        for entry in entries.flatten() {
            let path = entry.path();
            if path == spill {
                continue;
            }
            let removed = match entry.file_type() {
                Ok(kind) if kind.is_dir() => std::fs::remove_dir_all(&path),
                _ => std::fs::remove_file(&path),
            };
            let _ = removed;
        }
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

/// Whether `id` is a session GUID: hex and dashes only, so a lookup can never be
/// a path and `sessions show ../../etc` can never leave the state directory.
pub fn is_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}
