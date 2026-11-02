//! A repository's own rules: its `AGENTS.md`, read once at boot from the
//! repository root — never an ancestor's, never a nested one — and handed to
//! the model beside the preamble, so the transcript stays the user's words.
//!
//! The read is capped and the cap is stated here and nowhere else: the preamble
//! is re-sent on every turn, so a file with no limit is a bill with no limit,
//! and a file past it arrives cut with a marker rather than silently short.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::workspace;

/// The most of an `AGENTS.md` a run hands the model. A byte cap rather than a
/// word or line count, because bytes are what the request pays for; the marker
/// that says the file was cut is ours and is not counted against it.
pub const CAP: usize = 8 * 1024;

/// The file a repository's rules live in, at its root and nowhere else.
pub const FILE: &str = "AGENTS.md";

/// What a run was handed: the path, how many of its bytes, and the two texts a
/// reader of the run needs — the file's own words, and the block the model
/// received with the lead-in and any marker around them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    /// The bytes of the file handed over: the whole file, or [`CAP`] when it
    /// was longer.
    pub bytes: usize,
    pub text: String,
}

/// What was handed over, as a record and a diagnostic may say it: a path and a
/// count. Serialized as those two fields, so a reader can find the file and
/// compare it with what the run was given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handed {
    pub path: String,
    pub bytes: usize,
}

impl Handed {
    /// The one sentence `-v`, the record and `sindri doctor` all say, so one
    /// file is never spelled two ways.
    pub fn line(&self) -> String {
        format!("instructions: {} ({} bytes)", self.path, self.bytes)
    }
}

impl From<&Found> for Handed {
    fn from(found: &Found) -> Self {
        Handed {
            path: found.path.display().to_string(),
            bytes: found.bytes,
        }
    }
}

/// What a boot found when it looked. `Absent` is the ordinary case and says
/// nothing anywhere; `Unreadable` is a diagnostic that never stops a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Taken {
    Absent,
    Found(Found),
    Unreadable { path: PathBuf, reason: String },
}

impl Taken {
    /// The one sentence `sindri doctor` prints, so it cannot drift into a
    /// second description of the file a run reads. `None` when there is nothing
    /// to say, which is what an absent `AGENTS.md` is.
    pub fn line(&self) -> Option<String> {
        match self {
            Taken::Absent => None,
            Taken::Found(found) => Some(Handed::from(found).line()),
            Taken::Unreadable { path, reason } => Some(format!(
                "instructions: {} cannot be read: {reason}",
                path.display()
            )),
        }
    }

    /// The sentence a run says without being asked: only an unreadable file is
    /// worth a word nobody asked for, and the run it could not be read for is
    /// still started.
    pub fn diagnostic(&self) -> Option<String> {
        match self {
            Taken::Unreadable { .. } => self.line(),
            Taken::Absent | Taken::Found(_) => None,
        }
    }

    /// What the model is handed beside the preamble, or nothing at all: the
    /// file named in one line, then its words, exactly as the file holds them.
    pub fn block(&self) -> Option<String> {
        let Taken::Found(found) = self else {
            return None;
        };
        Some(format!(
            "The repository you are working in carries its house rules in {}; they \
             take precedence where they are more specific than the guidance above.\n\n{}",
            found.path.display(),
            found.text
        ))
    }

    /// What was handed over, for the record and for `-v`: absent for a file
    /// that never reached the model, which includes one that could not be read —
    /// that one was already said as a diagnostic and is not said twice.
    pub fn handed(&self) -> Option<Handed> {
        match self {
            Taken::Found(found) => Some(Handed::from(found)),
            Taken::Absent | Taken::Unreadable { .. } => None,
        }
    }
}

/// Reads the rules of the repository `workspace` stands in: its root's
/// `AGENTS.md`, or the workspace's own when the directory is no repository.
///
/// One file and one level, deliberately: a walk above the root would read
/// somebody else's repository, and a nested file would be a second source of
/// the same truth.
pub fn read(workspace: &Path) -> Taken {
    let root = workspace::root(workspace).unwrap_or_else(|| workspace.to_path_buf());
    let path = root.join(FILE);
    let raw = match std::fs::read(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Taken::Absent,
        Err(error) => {
            return Taken::Unreadable {
                path,
                reason: error.to_string(),
            };
        }
    };
    // A file that is not UTF-8 is read lossily rather than refused: a rule in
    // the wrong encoding is still a rule, and half of one is worse than a
    // replacement character.
    let whole = String::from_utf8_lossy(&raw).into_owned();
    let bytes = whole.len().min(CAP);
    let text = cut(&whole).unwrap_or(whole);
    Taken::Found(Found { path, bytes, text })
}

/// The file's text cut at [`CAP`], marker and all, or `None` when it fits.
/// The cut lands on a character boundary, because half a character is not text.
fn cut(text: &str) -> Option<String> {
    if text.len() <= CAP {
        return None;
    }
    let mut at = CAP;
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    Some(format!(
        "{}\n\n[sindri: this file was truncated at the {CAP}-byte cap; {} bytes of it were not \
         read]",
        &text[..at],
        text.len() - at
    ))
}
