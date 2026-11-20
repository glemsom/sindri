//! The opener is this host's, because the sandbox has no display: the core has
//! already judged the target, and it arrives as one argv word and never through a
//! shell. The child outlives the frame, so it is reaped rather than waited for.

#[cfg(test)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::process::{Command, Stdio};
use std::thread;

use sindri_core::open::{Opener, Opening};

/// The program a Linux host opens a target with. A front end on another machine
/// builds its own [`HostOpener`] with that machine's opener instead.
pub const HOST_OPENER: &str = "xdg-open";

/// This machine's opener, and the one argv word it is spawned with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostOpener {
    /// The program to spawn, as the host names it.
    program: OsString,
}

impl HostOpener {
    /// An opener running `program`, which is what a test hands a recording
    /// script and what a front end hands its platform's opener.
    pub fn new(program: impl Into<OsString>) -> HostOpener {
        HostOpener {
            program: program.into(),
        }
    }

    /// This machine's opener: the one that knows which browser the user picked.
    pub fn host() -> HostOpener {
        HostOpener::new(HOST_OPENER)
    }

    /// The program this opener spawns.
    #[cfg(test)]
    pub fn program(&self) -> &OsStr {
        &self.program
    }
}

impl Opener for HostOpener {
    /// Hand a judged target to the opener — one argv word, both streams closed,
    /// and a thread that reaps the child so nothing is left behind.
    fn open(&self, opening: &Opening) -> Result<(), String> {
        let mut child = Command::new(&self.program)
            .arg(opening.word())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("cannot run {}: {error}", self.program.to_string_lossy()))?;

        // A browser outlives the frame, so the wait is a thread's: that is what reaps the
        // child instead of leaving a zombie a long session never collects.
        thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
