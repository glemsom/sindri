use crate::config::{Environment, spoken};
use serde::Serialize;
use std::error::Error;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
const APPLICATION: &str = "sindri";
const SESSIONS: &str = "sessions";
const TEMP: &str = "tmp";
const SPILL: &str = "spill";
pub fn state_home(environment: &impl Environment) -> Result<PathBuf, SessionError> {
    match spoken(environment, "XDG_STATE_HOME") {
        Some(state) => Ok(PathBuf::from(state)),
        None => match spoken(environment, "HOME") {
            Some(home) => Ok(PathBuf::from(home).join(".local").join("state")),
            None => Err(SessionError::NoStateHome),
        },
    }
}
pub fn sessions_root(environment: &impl Environment) -> Result<PathBuf, SessionError> {
    Ok(state_home(environment)?.join(APPLICATION).join(SESSIONS))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    guid: String,
    directory: PathBuf,
    temp: PathBuf,
    spill: PathBuf,
    spilled: u32,
}
impl Session {
    pub fn create(state_home: &Path) -> Result<Session, SessionError> {
        std::fs::create_dir_all(state_home).map_err(|error| SessionError::Uncreatable {
            path: state_home.to_path_buf(),
            reason: error.to_string(),
        })?;
        let root =
            std::fs::canonicalize(state_home).map_err(|error| SessionError::Uncreatable {
                path: state_home.to_path_buf(),
                reason: error.to_string(),
            })?;
        let guid = fresh_guid()?;
        let directory = root.join(APPLICATION).join(SESSIONS).join(&guid);
        let temp = directory.join(TEMP);
        let spill = directory.join(SPILL);
        for path in [&temp, &spill] {
            std::fs::create_dir_all(path).map_err(|error| SessionError::Uncreatable {
                path: path.to_path_buf(),
                reason: error.to_string(),
            })?;
        }
        Ok(Session {
            guid,
            directory,
            temp,
            spill,
            spilled: 0,
        })
    }
    pub fn guid(&self) -> &str {
        &self.guid
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn temp(&self) -> &Path {
        &self.temp
    }
    pub fn spill_dir(&self) -> &Path {
        &self.spill
    }
    pub fn spill(&mut self, bytes: &[u8]) -> Result<SpillRef, SessionError> {
        let mut sequence = self.spilled + 1;
        loop {
            let path = self.spill.join(format!("{sequence:04}.out"));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(bytes)
                        .map_err(|error| SessionError::Unspillable {
                            path: path.clone(),
                            reason: error.to_string(),
                        })?;
                    self.spilled = sequence;
                    return Ok(SpillRef {
                        path,
                        bytes: bytes.len() as u64,
                    });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => sequence += 1,
                Err(error) => {
                    return Err(SessionError::Unspillable {
                        path,
                        reason: error.to_string(),
                    });
                }
            }
        }
    }
    pub fn finish(&mut self) -> Result<(), SessionError> {
        let entries = std::fs::read_dir(&self.temp).map_err(|error| SessionError::Unreadable {
            path: self.temp.clone(),
            reason: error.to_string(),
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| SessionError::Unreadable {
                path: self.temp.clone(),
                reason: error.to_string(),
            })?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|error| SessionError::Unreadable {
                    path: path.clone(),
                    reason: error.to_string(),
                })?;
            let removed = match kind.is_dir() {
                true => std::fs::remove_dir_all(&path),
                false => std::fs::remove_file(&path),
            };
            removed.map_err(|error| SessionError::Unremovable {
                path,
                reason: error.to_string(),
            })?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpillRef {
    path: PathBuf,
    bytes: u64,
}
impl SpillRef {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}
impl fmt::Display for SpillRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.path.display())
    }
}
fn fresh_guid() -> Result<String, SessionError> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| SessionError::NoEntropy {
            reason: error.to_string(),
        })?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}
#[derive(Debug)]
pub enum SessionError {
    NoStateHome,
    NoEntropy { reason: String },
    Uncreatable { path: PathBuf, reason: String },
    Unreadable { path: PathBuf, reason: String },
    Unremovable { path: PathBuf, reason: String },
    Unspillable { path: PathBuf, reason: String },
}
impl SessionError {
    pub fn exit_code(&self) -> u8 {
        match self {
            SessionError::NoStateHome => 2,
            SessionError::NoEntropy { .. }
            | SessionError::Uncreatable { .. }
            | SessionError::Unreadable { .. }
            | SessionError::Unremovable { .. }
            | SessionError::Unspillable { .. } => 3,
        }
    }
}
impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::NoStateHome => write!(
                formatter,
                "neither XDG_STATE_HOME nor HOME is set, so there is nowhere to record a session"
            ),
            SessionError::NoEntropy { reason } => {
                write!(formatter, "cannot read a GUID from /dev/urandom: {reason}")
            }
            SessionError::Uncreatable { path, reason } => {
                write!(formatter, "cannot create {}: {reason}", path.display())
            }
            SessionError::Unreadable { path, reason } => {
                write!(formatter, "cannot read {}: {reason}", path.display())
            }
            SessionError::Unremovable { path, reason } => {
                write!(formatter, "cannot remove {}: {reason}", path.display())
            }
            SessionError::Unspillable { path, reason } => {
                write!(formatter, "cannot spill to {}: {reason}", path.display())
            }
        }
    }
}
impl Error for SessionError {}
