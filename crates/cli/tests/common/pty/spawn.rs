//! Starting the real `sindri` on a pty: its streams on the terminal, its stderr
//! either there too or in a file beside it, and the two deadlines a test reads by
//! — `expect` for a phrase in the stream, `type_line` for a whole prompt.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

use std::fs::File;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use super::{DEADLINE, ENTER, Pty};
use crate::common::Env;

/// What a frame said without drawing it: the stderr beside the screen. A
/// failure said before the frame takes the terminal has nowhere else to go, so
/// a test that reads it must keep it out of the frame's escape sequences.
pub struct Said {
    /// The directory the file lives in, kept alive for the test's length.
    _root: tempfile::TempDir,
    path: PathBuf,
}

impl Said {
    pub fn new() -> Said {
        let root = tempfile::TempDir::new().expect("a directory for the frame's stderr");
        let path = root.path().join("stderr");
        Said { _root: root, path }
    }

    /// The file to hand `spawn_with_stderr`.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Everything the frame said on stderr, read once it has said it.
    pub fn words(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }
}

impl Default for Said {
    fn default() -> Said {
        Said::new()
    }
}

/// Waits for the process to be gone, and gives the code a shell would read —
/// `None` when it would not go, so a stuck front end fails rather than hangs.
pub fn exit(child: &mut Child) -> Option<i32> {
    let deadline = Instant::now() + DEADLINE;
    while Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            return status.code();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

/// A pty test's own environment: the mock provider's base URL, and nothing of
/// the host's own settings.
pub fn spawn(env: &Env, pty: &Pty, vars: &[(&str, &str)]) -> Child {
    opened(env, pty, vars, None)
}

/// The same, with the process's stderr kept in a file rather than drawn on the
/// terminal: the cold path's one note is a line on stderr, and a test that reads
/// it must not have the frame's own escape sequences in the way.
pub fn spawn_with_stderr(env: &Env, pty: &Pty, vars: &[(&str, &str)], file: &Path) -> Child {
    opened(env, pty, vars, Some(file))
}

/// The one spawn both doors do: a terminal for the frame, and a place for what
/// the frame does not draw.
fn opened(env: &Env, pty: &Pty, vars: &[(&str, &str)], stderr: Option<&Path>) -> Child {
    let mut command = env.command();
    let said = match stderr {
        Some(file) => Stdio::from(File::create(file).expect("a file for the frame's stderr")),
        None => Stdio::from(pty.slave.try_clone().expect("a pty slave")),
    };
    // The frame asks the terminal it is attached to how big it is, and a process
    // without a controlling terminal is told about whatever terminal the test
    // runner happens to have. A terminal emulator attaches its child to the pty
    // it opened, so this does too: the pty's size is the frame's size.
    // SAFETY: `setsid` and `TIOCSCTTY` touch nothing but the child's own
    // session, and fd 0 is the pty's slave by the time they run.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
        .env("TERM", "xterm-256color")
        .envs(vars.iter().copied())
        .stdin(Stdio::from(pty.slave.try_clone().expect("a pty slave")))
        .stdout(Stdio::from(pty.slave.try_clone().expect("a pty slave")))
        .stderr(said);
    command.spawn().expect("spawn sindri on a terminal")
}

/// A drawn frame that does not say `want` is a failure, not a hang: kills the
/// run and reports what it did draw.
pub fn expect(pty: &Pty, child: &mut Child, want: &str) {
    let drawn = pty.wait_for_text(child, want);
    if !drawn.contains(want) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the frame never said {want:?}, drew: {drawn:?}");
    }
}

/// Types a whole prompt: the characters, then Enter.
pub fn type_line(pty: &Pty, text: &str) {
    for byte in text.bytes() {
        pty.type_key(byte);
    }
    pty.type_key(ENTER);
}
