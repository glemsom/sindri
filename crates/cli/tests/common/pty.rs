//! A pseudo-terminal for the process seam: "is it a terminal" is exactly the
//! question the frame's front door answers, so a frame test drives the real
//! binary through a pty. Every wait has a deadline, so a frame that never draws
//! fails instead of hanging; `None` where the machine has no pty to offer means
//! the test skips and says so.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use super::Env;
use super::screen::Screen;

/// The escape sequence that enters the alternate screen: the frame's first act.
pub const ALTERNATE_SCREEN: &str = "\u{1b}[?1049h";
/// Ctrl-Q, the key that leaves the frame.
pub const QUIT: u8 = 0x11;
/// Ctrl-U, which empties the field the settings modal has the cursor in.
pub const CLEAR_FIELD: u8 = 0x15;
/// Enter, as a terminal sends it.
pub const ENTER: u8 = 0x0d;
/// Down, as a terminal sends it.
pub const DOWN: &[u8] = b"\x1b[B";
/// How long anything is given before it counts as a failure.
pub const DEADLINE: Duration = Duration::from_secs(20);

/// A pseudo-terminal. `None` where the machine has none to offer: a container
/// without `/dev/ptmx` cannot test a frame, and skipping says so.
pub struct Pty {
    master: OwnedFd,
    slave: OwnedFd,
    /// What the frame has drawn, as a terminal would remember it: the escapes
    /// applied rather than stripped, so a row a later frame overwrote says only
    /// what was drawn there last.
    screen: std::sync::Mutex<Screen>,
}

impl Pty {
    /// The master end, as a file to read and write.
    pub fn port(&self) -> File {
        File::from(self.master.try_clone().expect("the pty's master"))
    }
}

impl Pty {
    pub fn new() -> Option<Pty> {
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
        Some(Pty {
            master,
            slave,
            screen: std::sync::Mutex::default(),
        })
    }

    /// Everything the frame has drawn so far, read until `want` appears or the
    /// deadline passes.
    pub fn wait_for(&self, child: &mut Child, want: &str) -> String {
        let mut port = self.port();
        let mut seen = String::new();
        let deadline = Instant::now() + DEADLINE;
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline {
            match port.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                    self.look(&text);
                    seen.push_str(&text);
                }
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

    /// Everything the frame has drawn, read until the *visible* text says
    /// `want` — escapes taken out first, because a streamed answer arrives one
    /// cell per frame and its letters are interleaved with cursor moves.
    pub fn wait_for_text(&self, child: &mut Child, want: &str) -> String {
        let mut raw = String::new();
        let deadline = Instant::now() + DEADLINE;
        let mut port = self.port();
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline {
            match port.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                    self.look(&text);
                    raw.push_str(&text);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
            if visible(&raw).contains(want) {
                return visible(&raw);
            }
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        visible(&raw)
    }

    /// Feeds one read into the screen the frame is drawing on.
    fn look(&self, text: &str) {
        if let Ok(mut screen) = self.screen.lock() {
            screen.feed(text);
        }
    }

    /// What is on the screen now, as a reader would see it.
    pub fn screen(&self) -> String {
        self.screen
            .lock()
            .map(|screen| screen.text())
            .unwrap_or_default()
    }

    /// Reads until what is *on the screen* says `want`, and gives the screen
    /// back: unlike `wait_for_text`, a row redrawn after the first write is read
    /// as the frame last drew it.
    pub fn wait_for_screen(&self, child: &mut Child, want: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        let mut port = self.port();
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline {
            match port.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => self.look(&String::from_utf8_lossy(&buffer[..read])),
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
            if self.screen().contains(want) {
                return self.screen();
            }
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.screen()
    }

    /// Types one byte at the terminal, as a keyboard would.
    pub fn type_key(&self, byte: u8) {
        let _ = self.port().write_all(&[byte]);
    }

    /// Asks the frame to leave, the way a user would: pressing the key again if
    /// the first press landed while the frame was still opening. Returns the
    /// exit status and everything the frame drew on its way out — the terminal
    /// is handed back before the process goes — or `None` when it would not go.
    pub fn quit(&self, child: &mut Child) -> (Option<i32>, String) {
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
    pub fn drain(&self, seen: &mut String) -> String {
        let mut port = self.port();
        let mut buffer = [0u8; 4096];
        loop {
            match port.read(&mut buffer) {
                Ok(0) | Err(_) => return seen.clone(),
                Ok(read) => {
                    let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                    self.look(&text);
                    seen.push_str(&text);
                }
            }
        }
    }
}

/// A pty test's own environment: the mock provider's base URL, and nothing of
/// the host's own settings.
pub fn spawn(env: &Env, pty: &Pty, vars: &[(&str, &str)]) -> Child {
    let mut command = env.command();
    command
        .env("TERM", "xterm-256color")
        .envs(vars.iter().copied())
        .stdin(Stdio::from(pty.slave.try_clone().expect("a pty slave")))
        .stdout(Stdio::from(pty.slave.try_clone().expect("a pty slave")))
        .stderr(Stdio::from(pty.slave.try_clone().expect("a pty slave")));
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

/// What a user would read: every escape sequence taken out, so text a frame
/// drew one cell at a time reads as words again.
pub fn visible(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte.
            Some('[') => {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            // OSC: up to a BEL or a string terminator.
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
        // A cursor move is not a character, but a new row is where a reader
        // stops: a space keeps the words of one row together.
        out.push(' ');
    }
    out
}
