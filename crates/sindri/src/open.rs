//! The opener is this host's, because the sandbox has no display: the core has
//! already judged the target, and it arrives as one argv word and never through a
//! shell. The call settles on what the opener did, and a browser outliving it is still reaped.

#[cfg(test)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::{ErrorKind, Read};
use std::process::{ChildStderr, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sindri_core::open::{Handover, Opener, Opening};
use sindri_core::turn::Exit;

/// The program a Linux host opens a target with. A front end on another machine
/// builds its own [`HostOpener`] with that machine's opener instead.
pub const HOST_OPENER: &str = "xdg-open";

/// How long an opener is given to end before the target is settled as handed over.
pub const GRACE: Duration = Duration::from_secs(2);

/// Short enough that an opener which ends at once is back at once.
const NAP: Duration = Duration::from_millis(25);

/// How long the opener's own words are given to arrive once it has exited: a grandchild still
/// holding the pipe cannot hold the call open.
const WORDS_GRACE: Duration = Duration::from_millis(250);

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

        let stderr = Drain::start(child.stderr.take());
        let deadline = Instant::now() + self.grace;
        let mut nap = Duration::from_millis(1);
        let exit = loop {
            match child.try_wait() {
                Ok(Some(status)) => break exit_of(status),
                Ok(None) => {}
                Err(error) => {
                    return Handover::Failed {
                        exit: Exit::Code(1),
                        words: format!("cannot wait for {program}: {error}"),
                    };
                }
            }
            if Instant::now() >= deadline {
                thread::spawn(move || {
                    let _ = child.wait();
                });
                return Handover::Handed;
            }
            thread::sleep(nap);
            nap = (nap * 2).min(NAP);
        };
        match exit {
            Exit::Code(0) => Handover::Opened,
            exit => Handover::Failed {
                exit,
                words: stderr.words(),
            },
        }
    }
}

/// A child's stream read by a thread of its own, so an opener that writes to it never blocks on
/// a pipe nobody reads, collected into a sink the call takes once the child has ended.
struct Drain {
    sink: Arc<Mutex<Vec<u8>>>,
    reader: JoinHandle<()>,
}

impl Drain {
    fn start(pipe: Option<ChildStderr>) -> Drain {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let collected = Arc::clone(&sink);
        let reader = thread::spawn(move || {
            let Some(mut pipe) = pipe else {
                return;
            };
            let mut chunk = [0u8; 8 * 1024];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => return,
                    Ok(read) => collected
                        .lock()
                        .expect("the sink is not poisoned")
                        .extend_from_slice(&chunk[..read]),
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => return,
                }
            }
        });
        Drain { sink, reader }
    }

    /// The first line the opener wrote that says anything, and none of the rest of it.
    fn words(self) -> String {
        let deadline = Instant::now() + WORDS_GRACE;
        while !self.reader.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        let sink = self.sink.lock().expect("the sink is not poisoned");
        String::from_utf8_lossy(&sink)
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_owned()
    }
}

fn exit_of(status: ExitStatus) -> Exit {
    match status.code() {
        Some(code) => Exit::Code(code),
        None => Exit::Signalled,
    }
}

#[cfg(test)]
mod tests;
