//! A session is one recorded run's directory, named by a GUID, holding the temp
//! a run's calls share and the spill a budget emptied.
//!
//! The temp is the scratch §6 maps to `TMPDIR`: one directory per session, alive
//! as long as the session, so a call sees the file the call before it wrote and
//! no run sees another's. It is created here, named here and emptied here, and
//! [`crate::bash`] is handed the path — which is why what confines a command and
//! the ground it writes scratch on cannot disagree.
//!
//! The directory is `$XDG_STATE_HOME/sindri/sessions/<guid>/`, and its layout is
//! this module's to make: `tmp/` for the session temp and `spill/` beside it for
//! output a budget could not hold. The record files §13 puts in that same
//! directory — `session.json`, `transcript.jsonl`, `status.json` — are written by
//! whoever records the run and never here, so a session can exist before the
//! first entry does.
//!
//! A GUID is generated once and exposed, so the record's `session` field and the
//! last line of `-v` read one value for the life of the session.
//!
//! Nothing is cleaned away at the end but the temp: the directory and `spill/`
//! are kept, because a cancelled or killed run still has to read.

use std::error::Error;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{Environment, spoken};

/// The sub-directory of the state home that Sindri owns.
const APPLICATION: &str = "sindri";

/// The directory of sessions inside it, one per GUID.
const SESSIONS: &str = "sessions";

/// The session temp, which the sandbox maps to `TMPDIR`.
const TEMP: &str = "tmp";

/// Where command output a budget could not hold is written, beside the temp.
const SPILL: &str = "spill";

/// `$XDG_STATE_HOME/sindri/sessions`, falling back to `$HOME/.local/state`.
///
/// The variables are handed in rather than read, so a test holds them as data
/// and never touches a real state home.
pub fn sessions_root(environment: &impl Environment) -> Result<PathBuf, SessionError> {
    let base = match spoken(environment, "XDG_STATE_HOME") {
        Some(state) => PathBuf::from(state),
        None => match spoken(environment, "HOME") {
            Some(home) => PathBuf::from(home).join(".local").join("state"),
            None => return Err(SessionError::NoStateHome),
        },
    };
    Ok(base.join(APPLICATION).join(SESSIONS))
}

/// One session's directory: the GUID that names it, the temp inside it and the
/// spill beside it.
///
/// The three paths are made once, in [`Session::create`], and read from then on:
/// no accessor joins anything a caller could get wrong, and no call site spells
/// the layout a second time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// The GUID the directory is named by, and the id every record field uses.
    guid: String,
    /// `…/sessions/<guid>`.
    directory: PathBuf,
    /// `…/sessions/<guid>/tmp`, which the sandbox maps to `TMPDIR`.
    temp: PathBuf,
    /// `…/sessions/<guid>/spill`, where output a budget could not hold is kept.
    spill: PathBuf,
    /// How many spills this session has written, so the next is numbered after
    /// the last rather than over it.
    spilled: u32,
}

impl Session {
    /// Creates a session under `state_home`: the directory named by a fresh
    /// GUID, the temp inside it and the spill beside it.
    ///
    /// `state_home` is resolved through the filesystem, so the paths a command
    /// is handed are canonical and two spellings of one state home are one.
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

    /// The GUID this session is named by: the record's `session` field and the
    /// last line of `-v`.
    pub fn guid(&self) -> &str {
        &self.guid
    }

    /// `…/sessions/<guid>`, where the record files go.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The session temp, which the sandbox maps to `TMPDIR`.
    pub fn temp(&self) -> &Path {
        &self.temp
    }

    /// The spill directory, beside the temp.
    pub fn spill_dir(&self) -> &Path {
        &self.spill
    }

    /// Writes `bytes` to a numbered file under `spill/` and hands back a
    /// reference the caller can print.
    ///
    /// This is the mechanism and not the policy: what a run's budget is, and
    /// when it is exhausted, is the caller's decision — a spill is what the
    /// caller does with output it will not feed back whole, so nothing here
    /// counts bytes against a limit or decides that one has been reached.
    ///
    /// Each spill is a file of its own, created new and never over an earlier
    /// one, so a later spill cannot erase what an earlier one kept.
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

    /// Ends the session: empties the temp, and keeps everything else.
    ///
    /// The temp is scratch for the life of the session and nothing after it, so
    /// what a run left there goes; the directory, `spill/` and anything in it
    /// stay, because a spilled file is the record of output a budget dropped and
    /// a later reader still has to reach it. Ending twice is not a failure.
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
            // A symlink is removed by its own name and never by its target's:
            // a link a call left pointing out of the temp is not followed.
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

/// Output a budget could not hold, kept under `spill/`: the file it is in and
/// how many bytes that file holds.
///
/// It is what the caller prints in place of the output it dropped, so the model
/// reads where the rest went. Whether that path is reachable from inside the
/// sandbox is the writable set's decision and not this value's: `spill/` sits
/// beside the temp, and only the temp is mapped to `TMPDIR`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpillRef {
    /// The file the output was written to, inside `spill/`.
    path: PathBuf,
    /// How many bytes it holds.
    bytes: u64,
}

impl SpillRef {
    /// The file the output was written to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How many bytes that file holds.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl fmt::Display for SpillRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.path.display())
    }
}

/// A fresh GUID: 16 bytes from the kernel's entropy pool, marked version 4 and
/// RFC 4122 variant, spelled 8-4-4-4-12 in lowercase hex.
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

/// Why a session could not be created or ended. Every one of them names the path
/// that failed, so a run that cannot record says which directory to fix.
#[derive(Debug)]
pub enum SessionError {
    /// Neither `XDG_STATE_HOME` nor `HOME` is set, so there is nowhere to write.
    NoStateHome,
    /// The kernel gave no entropy, so no GUID can name a session.
    NoEntropy {
        /// Why it gave none.
        reason: String,
    },
    /// A directory of the session's layout could not be created.
    Uncreatable {
        /// The path that would not be made.
        path: PathBuf,
        /// Why it would not.
        reason: String,
    },
    /// A directory of the session's layout could not be read.
    Unreadable {
        /// The path that would not be read.
        path: PathBuf,
        /// Why it would not.
        reason: String,
    },
    /// An entry of the session temp could not be removed.
    Unremovable {
        /// The path that would not be removed.
        path: PathBuf,
        /// Why it would not.
        reason: String,
    },
    /// A spill file could not be written.
    Unspillable {
        /// The path that would not be written.
        path: PathBuf,
        /// Why it would not.
        reason: String,
    },
}

impl SessionError {
    /// The exit code a front end reports: nowhere to write is a usage error, and
    /// a machine that will not make the directory is the run failing.
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
