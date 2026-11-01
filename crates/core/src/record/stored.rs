//! Reading a record back: `sindri sessions show` and, later, a TUI's session
//! restore, from the same files the run wrote. A reader may name a record by a
//! whole id or by a leading prefix of one, and either way it is the same guard
//! that decides a name can be a record at all.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use super::entry::{Entry, Meta};
use crate::paths::{SessionDir, state_dir};

#[derive(Debug)]
pub enum ShowError {
    /// The id is not a session GUID: never a path, so a lookup cannot escape.
    BadId {
        id: String,
    },
    /// More than one record starts with the prefix the reader typed: which one
    /// they meant is a question only they can answer, and the candidates are
    /// the answer's shape.
    Ambiguous {
        prefix: String,
        candidates: Vec<String>,
    },
    Missing {
        path: PathBuf,
    },
    Unreadable {
        path: PathBuf,
        source: io::Error,
    },
    /// Only ever the metadata: without `session.json` there is no record at
    /// all. A transcript line that does not parse is counted in
    /// [`Stored::unread`] instead, because what came before it is still
    /// evidence.
    Corrupt {
        path: PathBuf,
        detail: String,
    },
}

impl fmt::Display for ShowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShowError::BadId { id } => write!(f, "`{id}` is not a session id"),
            ShowError::Ambiguous { prefix, candidates } => write!(
                f,
                "`{prefix}` names more than one session: {}",
                candidates.join(", ")
            ),
            ShowError::Missing { path } => write!(f, "no session recorded in {}", path.display()),
            ShowError::Unreadable { path, source } => {
                write!(f, "{} cannot be read: {source}", path.display())
            }
            ShowError::Corrupt { path, detail } => {
                write!(f, "{} is not a record: {detail}", path.display())
            }
        }
    }
}

impl std::error::Error for ShowError {}

pub struct Stored {
    pub meta: Meta,
    pub entries: Vec<Entry>,
    /// Transcript lines that could not be read: a crash cuts the line it was
    /// writing, and a record short of its tail is evidence of everything up to
    /// the cut. Counted rather than fatal, said rather than silent.
    pub unread: usize,
    /// `status.json`'s lines verbatim: bubblewrap's own documents are not ours
    /// to type, and a post-mortem wants what bwrap actually said.
    pub status: Vec<String>,
}

impl Stored {
    /// The record a reader asked for: the newest one when they named none, the
    /// one `name` starts when they did. Reading is not resuming — nothing here
    /// writes, so a record is never scolded for being looked at.
    pub fn resolve(name: Option<&str>) -> Result<Self, ShowError> {
        let root = sessions_root()?;
        match name {
            Some(name) => Self::by_name(&root, name),
            None => Self::newest(&root),
        }
    }

    /// The run you just did. It comes from the listing `sessions list` prints,
    /// so the two commands cannot disagree about which record came last — and
    /// it asks that listing for one row, since an id is all a lookup needs and
    /// the prompts of the rows it never shows are reads nobody asked for.
    fn newest(root: &Path) -> Result<Self, ShowError> {
        let rows =
            super::listing_limited(root, Some(1)).map_err(|source| ShowError::Unreadable {
                path: root.to_path_buf(),
                source,
            })?;
        let Some(row) = rows.into_iter().next() else {
            return Err(ShowError::Missing {
                path: root.to_path_buf(),
            });
        };
        Self::load_in(&SessionDir::at(root.join(row.id)))
    }

    /// A whole id or a leading prefix of one, git-style: the guard stays the
    /// only door, so a prefix can no more leave the state directory than an id
    /// can, and an ambiguous one names its candidates rather than guessing.
    fn by_name(root: &Path, name: &str) -> Result<Self, ShowError> {
        if !is_session_id(name) {
            return Err(ShowError::BadId {
                id: name.to_string(),
            });
        }
        let mut ids = candidates(root, name)?;
        match ids.len() {
            0 => Err(ShowError::Missing {
                path: root.join(name),
            }),
            1 => Self::load_in(&SessionDir::at(root.join(ids.remove(0)))),
            _ => Err(ShowError::Ambiguous {
                prefix: name.to_string(),
                candidates: ids,
            }),
        }
    }

    pub fn load_in(session: &SessionDir) -> Result<Self, ShowError> {
        let root = session.root();
        let meta = read(&root.join("session.json"))?;
        let meta: Meta = serde_json::from_str(&meta).map_err(|error| ShowError::Corrupt {
            path: root.join("session.json"),
            detail: error.to_string(),
        })?;

        let path = root.join("transcript.jsonl");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(source) => return Err(ShowError::Unreadable { path, source }),
        };
        let mut entries = Vec::new();
        let mut unread = 0;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            match serde_json::from_str(line) {
                Ok(entry) => entries.push(entry),
                Err(_) => unread += 1,
            }
        }
        let status = read_lines(&root.join("status.json"))?;
        Ok(Stored {
            meta,
            entries,
            unread,
            status,
        })
    }
}

/// A file's lines, or none at all when the file was never written: a record
/// without a `status.json` is a record whose run had nothing to say.
fn read_lines(path: &Path) -> Result<Vec<String>, ShowError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text.lines().map(str::to_string).collect()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(ShowError::Unreadable {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read(path: &Path) -> Result<String, ShowError> {
    std::fs::read_to_string(path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            ShowError::Missing {
                path: path.to_path_buf(),
            }
        } else {
            ShowError::Unreadable {
                path: path.to_path_buf(),
                source,
            }
        }
    })
}

/// The directory every run writes into, and the one thing a lookup cannot do
/// without: a reader who asked for a record is owed the reason rather than an
/// empty answer.
fn sessions_root() -> Result<PathBuf, ShowError> {
    state_dir()
        .map(|state| state.join("sessions"))
        .map_err(|source| ShowError::Unreadable {
            path: PathBuf::from("<state dir>"),
            source,
        })
}

/// Every record directory whose name `name` starts, sorted so the candidates an
/// ambiguous prefix prints read the same twice. A directory is the only thing
/// that can be a record, so a stray file cannot shadow a session, and a state
/// directory no run has written to holds no candidates rather than being a
/// failure.
fn candidates(root: &Path, name: &str) -> Result<Vec<String>, ShowError> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(ShowError::Unreadable {
                path: root.to_path_buf(),
                source,
            });
        }
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| Some(entry.file_name().to_str()?.to_string()))
        .filter(|id| id.starts_with(name) && is_session_id(id))
        .collect();
    ids.sort();
    Ok(ids)
}

/// Whether `id` is a session GUID: hex and dashes only, so a lookup can never be
/// a path and `sessions show ../../etc` can never leave the state directory.
fn is_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}
