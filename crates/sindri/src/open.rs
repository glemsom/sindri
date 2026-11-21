//! The opener is this host's, because the sandbox has no display: the core has
//! already judged the target, and it arrives as one argv word and never through a
//! shell. The call settles on what the opener did, and a browser outliving it is still reaped.

#[cfg(test)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use sindri_core::open::{Handover, Opener, Opening};
use sindri_core::pipe;
use sindri_core::turn::Exit;

/// The program a Linux host opens a target with. A front end on another machine
/// builds its own [`HostOpener`] with that machine's opener instead.
pub const HOST_OPENER: &str = "xdg-open";

/// How long an opener is given to end before the target is settled as handed over.
const GRACE: Duration = Duration::from_secs(2);

const NAP: Duration = Duration::from_millis(25);

/// This machine's opener, and the one argv word it is spawned with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostOpener {
    /// The program to spawn, as the host names it.
    program: OsString,
    /// How long a call waits for it before settling as handed.
    grace: Duration,
}

impl HostOpener {
    /// An opener running `program`, which is what a test hands a recording
    /// script and what a front end hands its platform's opener.
    pub fn new(program: impl Into<OsString>) -> HostOpener {
        HostOpener {
            program: program.into(),
            grace: GRACE,
        }
    }

    /// The same opener, waiting at most `grace` for it to end.
    #[cfg(test)]
    pub fn within(mut self, grace: Duration) -> HostOpener {
        self.grace = grace;
        self
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
    /// Hand a judged target to the opener — one argv word, and no shell — and settle on what
    /// that opener did with it.
    fn open(&self, opening: &Opening) -> Handover {
        let program = self.program.to_string_lossy().into_owned();
        let mut child = match Command::new(&self.program)
            .arg(opening.word())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                return Handover::Failed {
                    exit: Exit::Code(1),
                    words: format!("cannot run {program}: {error}"),
                };
            }
        };

        let said = pipe::Stream::drain(child.stderr.take());
        let deadline = Instant::now() + self.grace;
        let mut nap = Duration::from_millis(1);
        let exit = loop {
            match child.try_wait() {
                Ok(Some(status)) => break exit_of(status),
                Ok(None) => {}
                Err(error) => {
                    reap(child);
                    return Handover::Failed {
                        exit: Exit::Code(1),
                        words: format!("cannot wait for {program}: {error}"),
                    };
                }
            }
            if Instant::now() >= deadline {
                reap(child);
                return Handover::Handed;
            }
            thread::sleep(nap);
            nap = (nap * 2).min(NAP);
        };
        if exit.is_ok() {
            return Handover::Opened;
        }
        let words = pipe::first_line(&said.finish());
        Handover::Failed {
            exit,
            words: match words.is_empty() {
                true => format!("{program} said nothing"),
                false => words,
            },
        }
    }
}

/// The child is the browser's now: a thread waits for it, so a process outliving the call is
/// neither killed nor left a zombie.
fn reap(mut child: Child) {
    thread::spawn(move || {
        let _ = child.wait();
    });
}

fn exit_of(status: ExitStatus) -> Exit {
    match status.code() {
        Some(code) => Exit::Code(code),
        None => Exit::Signalled,
    }
}

#[cfg(test)]
mod tests;
