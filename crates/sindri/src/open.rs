//! The opener is this host's, because the sandbox has no display: the core has
//! already judged the target, and it arrives as one argv word and never through a
//! shell. The child outlives the frame, so it is reaped rather than waited for.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::ErrorKind;
use std::process::{Command, Stdio};
use std::thread;

use sindri_core::open::Opening;

/// The program a Linux host opens a target with. A front end on another machine
/// builds its own [`Opener`] with that machine's opener instead.
pub const HOST_OPENER: &str = "xdg-open";

/// This machine's opener, and the one argv word it is spawned with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opener {
    /// The program to spawn, as the host names it.
    program: OsString,
}

impl Opener {
    /// An opener running `program`, which is what a test hands a recording
    /// script and what a front end hands its platform's opener.
    pub fn new(program: impl Into<OsString>) -> Opener {
        Opener {
            program: program.into(),
        }
    }

    /// This machine's opener: the one that knows which browser the user picked.
    pub fn host() -> Opener {
        Opener::new(HOST_OPENER)
    }

    /// The program this opener spawns.
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    /// Hand a judged target to the opener — one argv word, both streams closed,
    /// and a thread that reaps the child so nothing is left behind.
    pub fn open(&self, opening: &Opening) -> Result<(), OpenError> {
        let mut child = Command::new(&self.program)
            .arg(opening.word())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| OpenError {
                program: self.program.clone(),
                kind: error.kind(),
                reason: error.to_string(),
            })?;

        // A browser outlives the frame, so the wait is a thread's: that is what reaps the
        // child instead of leaving a zombie a long session never collects.
        thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// Why the opener could not be spawned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenError {
    /// The program that would not run.
    program: OsString,
    /// What kind of failure it was, so a front end can tell a missing opener from
    /// one that is there and would not start.
    kind: ErrorKind,
    /// Why it would not run, in the operating system's own words.
    reason: String,
}

impl OpenError {
    /// The program that would not run.
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    /// What kind of failure it was.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot run {}: {}",
            self.program.to_string_lossy(),
            self.reason
        )
    }
}

impl std::error::Error for OpenError {}

#[cfg(test)]
mod tests;
