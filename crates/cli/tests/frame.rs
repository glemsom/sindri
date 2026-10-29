//! The process seam for the terminal UI: bare `sindri` on a real terminal opens
//! the frame, and a sandbox that cannot exist is a card before anything else
//! draws. Driven through a pseudo-terminal, because "is it a terminal" is
//! exactly the question the front door answers — and every wait has a deadline,
//! so a frame that never draws fails instead of hanging (ADR-0011).

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use common::{Env, TEST_KEY, host_path, path_without, required_binaries};

/// The escape sequence that enters the alternate screen: the frame's first act.
const ALTERNATE_SCREEN: &str = "\u{1b}[?1049h";
/// Ctrl-Q, the key that leaves the frame.
const QUIT: u8 = 0x11;
/// How long anything is given before it counts as a failure.
const DEADLINE: Duration = Duration::from_secs(20);

/// A pseudo-terminal. `None` where the machine has none to offer: a container
/// without `/dev/ptmx` cannot test a frame, and skipping says so.
struct Pty {
    master: OwnedFd,
    slave: OwnedFd,
}

impl Pty {
    /// The master end, as a file to read and write.
    fn port(&self) -> File {
        File::from(self.master.try_clone().expect("the pty's master"))
    }
}

impl Pty {
    fn new() -> Option<Pty> {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        // SAFETY: `openpty` writes exactly two descriptors into the pointers it
        // is given; both are wrapped in an `OwnedFd` exactly once below, and
        // closed on every path that returns early.
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if opened != 0 || master < 0 || slave < 0 {
            return None;
        }
        // SAFETY: both descriptors are fresh and owned here, each wrapped once.
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
        // A pty starts at 0×0 cells, which is no frame at all: say the size.
        let size = libc::winsize {
            ws_row: 30,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: the master descriptor is valid and owned here, and
        // `TIOCSWINSZ` reads a `winsize` this frame owns.
        if unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &size) } != 0 {
            return None;
        }
        // Reading must never block: every read below polls a deadline.
        // SAFETY: `fcntl` on a descriptor this frame owns.
        unsafe {
            let flags = libc::fcntl(master.as_raw_fd(), libc::F_GETFL);
            if flags < 0
                || libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) != 0
            {
                return None;
            }
        }
        Some(Pty { master, slave })
    }

    /// Everything the frame has drawn so far, read until `want` appears or the
    /// deadline passes.
    fn wait_for(&self, child: &mut Child, want: &str) -> String {
        let mut port = self.port();
        let mut seen = String::new();
        let deadline = Instant::now() + DEADLINE;
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline {
            match port.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => seen.push_str(&String::from_utf8_lossy(&buffer[..read])),
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                // The child is gone: what it drew is all there is.
                Err(_) => break,
            }
            if seen.contains(want) {
                return seen;
            }
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        seen
    }

    /// Types one byte at the terminal, as a keyboard would.
    fn type_key(&self, byte: u8) {
        let _ = self.port().write_all(&[byte]);
    }

    /// Asks the frame to leave, the way a user would: pressing the key again if
    /// the first press landed while the frame was still opening. Returns the
    /// exit status and everything the frame drew on its way out — the terminal
    /// is handed back before the process goes — or `None` when it would not go.
    fn quit(&self, child: &mut Child) -> (Option<i32>, String) {
        let deadline = Instant::now() + DEADLINE;
        let mut leaving = String::new();
        while Instant::now() < deadline {
            if let Ok(Some(status)) = child.try_wait() {
                return (status.code(), self.drain(&mut leaving));
            }
            self.type_key(QUIT);
            self.drain(&mut leaving);
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
        (None, self.drain(&mut leaving))
    }

    /// Everything still waiting at the master end, appended to `seen`.
    fn drain(&self, seen: &mut String) -> String {
        let mut port = self.port();
        let mut buffer = [0u8; 4096];
        loop {
            match port.read(&mut buffer) {
                Ok(0) | Err(_) => return seen.clone(),
                Ok(read) => seen.push_str(&String::from_utf8_lossy(&buffer[..read])),
            }
        }
    }
}

/// `sindri` with its streams on the pty, and the pty's own settings.
fn spawn(env: &Env, pty: &Pty, vars: &[(&str, &str)]) -> Child {
    let mut command = env.command();
    command
        .env("TERM", "xterm-256color")
        .env("SINDRI_API_KEY", TEST_KEY)
        .env("SINDRI_MODEL", "claude-sonnet-4-6")
        .envs(vars.iter().copied())
        .stdin(Stdio::from(pty.slave.try_clone().expect("a pty slave")))
        .stdout(Stdio::from(pty.slave.try_clone().expect("a pty slave")))
        .stderr(Stdio::from(pty.slave.try_clone().expect("a pty slave")));
    command.spawn().expect("spawn sindri on a terminal")
}

#[test]
fn bare_sindri_on_a_terminal_opens_the_frame() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the frame's launch is not tested here");
        return;
    };
    let env = Env::new();
    let mut child = spawn(&env, &pty, &[]);

    let drawn = pty.wait_for(&mut child, ALTERNATE_SCREEN);
    if !drawn.contains(ALTERNATE_SCREEN) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("bare `sindri` on a terminal must open the frame, drew: {drawn:?}");
    }

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_sandbox_that_cannot_exist_is_a_card_before_anything_else_draws() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the card's launch is not tested here");
        return;
    };
    let env = Env::new();
    // A PATH without bwrap is a machine that cannot have a sandbox at all.
    let hidden = path_without(&required_binaries(), &["bwrap"], &host_path());
    let mut child = spawn(
        &env,
        &pty,
        &[("PATH", &hidden.path().display().to_string())],
    );

    let drawn = pty.wait_for(&mut child, "sindri cannot start");
    if !drawn.contains("sindri cannot start") {
        let _ = child.kill();
        let _ = child.wait();
        panic!("a broken sandbox must say so on the whole screen, drew: {drawn:?}");
    }
    assert!(
        drawn.contains(ALTERNATE_SCREEN),
        "the card is drawn in the alternate screen, not over the shell"
    );
    assert!(
        drawn.contains("bwrap"),
        "the card names the check that failed: {drawn:?}"
    );

    // Any key dismisses it, and the run that could not start is an environment
    // failure — not a success.
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(3), "a sandbox that cannot start exits 3");
}

/// The escape sequence that hands the mouse back when the frame closes.
const MOUSE_OFF: &str = "\u{1b}[?1000l";
/// The escape sequence that turns mouse reporting on: the frame draws its own
/// selection, so the terminal's must be off (ADR-0011 §7).
const MOUSE_ON: &str = "\u{1b}[?1000h";
#[test]
fn the_mouse_is_the_frames_while_it_draws_and_the_terminals_again_after() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: mouse capture is not tested here");
        return;
    };
    let env = Env::new();
    let mut child = spawn(&env, &pty, &[]);

    let drawn = pty.wait_for(&mut child, MOUSE_ON);
    if !drawn.contains(MOUSE_ON) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the frame takes the mouse when it opens, drew: {drawn:?}");
    }
    let (code, leaving) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert!(
        leaving.contains(MOUSE_OFF),
        "and the mouse is the terminal's again: {leaving:?}"
    );
}

#[test]
fn the_frame_advertises_the_hands_and_leaves_when_asked() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the frame's keys are not tested here");
        return;
    };
    let env = Env::new();
    let mut child = spawn(&env, &pty, &[]);

    // The footer carries the keys a user needs before they know them: scrolling,
    // `@` paths, `/` commands and `?`. Everything else is behind `?`.
    let drawn = pty.wait_for(&mut child, "@ paths");
    for wanted in ["PgUp/PgDn scroll", "@ paths", "/ commands", "? keys"] {
        assert!(
            drawn.contains(wanted),
            "the frame advertises {wanted:?} before anything is typed: {drawn:?}"
        );
    }

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}
