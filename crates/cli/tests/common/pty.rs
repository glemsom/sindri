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
use std::process::Child;
use std::time::{Duration, Instant};

use super::screen::{Screen, says};

mod spawn;

// Each test binary uses a different slice of the harness, so a door a binary
// does not open is not an unused import; it is a door that binary has no key to.
#[allow(
    unused_imports,
    reason = "each test binary uses a different slice of the harness"
)]
pub use spawn::{Said, exit, expect, spawn, spawn_with_stderr, type_line};

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
/// The sequence that shows the terminal's cursor: a frame ends with it, so one
/// of these is one frame drawn.
pub const SHOW_CURSOR: &str = "\u{1b}[?25h";
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

    pub fn listening(&self, child: &mut Child, window: Duration) -> String {
        let mut port = self.port();
        let mut drawn = String::new();
        let mut buffer = [0u8; 4096];
        let deadline = Instant::now() + window;
        while Instant::now() < deadline {
            match port.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                    self.look(&text);
                    drawn.push_str(&text);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if child.try_wait().is_ok_and(|status| status.is_some()) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        drawn
    }

    pub fn resize(&self, child: &Child, rows: u16, cols: u16) {
        let size = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: the master descriptor is valid and owned here, and
        // `TIOCSWINSZ` reads a `winsize` this frame owns.
        let sized = unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &size) };
        assert_eq!(sized, 0, "the terminal takes the new size");
        // SAFETY: `kill` on a pid this test owns; a child already gone is harmless.
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGWINCH) };
    }

    fn look(&self, text: &str) {
        if let Ok(mut screen) = self.screen.lock() {
            screen.feed(text);
        }
    }

    pub fn written_while_visible(&self) -> Vec<(u16, u16)> {
        self.screen
            .lock()
            .map(|screen| screen.seen_while_visible().to_vec())
            .unwrap_or_default()
    }

    pub fn screen(&self) -> String {
        self.screen
            .lock()
            .map(|screen| screen.text())
            .unwrap_or_default()
    }

    pub fn wait_for_screen(&self, child: &mut Child, want: &str) -> String {
        self.wait_until(child, &|screen: &str| screen.contains(want))
    }

    pub fn wait_for_words(&self, child: &mut Child, want: &str) -> String {
        self.wait_until(child, &|screen: &str| says(screen, want))
    }

    /// Waits for the next frame to be drawn whole. A frame ends by showing the
    /// terminal's cursor, and an idle frame draws nothing (#42), so the next
    /// end after a keystroke is the frame that keystroke drew. A test that
    /// reads the screen after a key comes here first: a frame arrives as it is
    /// written, so a row it has not reached yet still says what was there
    /// before.
    pub fn wait_for_frame(&self, child: &mut Child) -> String {
        let deadline = Instant::now() + DEADLINE;
        let mut port = self.port();
        let mut drawn = String::new();
        let mut buffer = [0u8; 4096];
        while Instant::now() < deadline {
            match port.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                    self.look(&text);
                    drawn.push_str(&text);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
            if drawn.matches(SHOW_CURSOR).count() >= 1 {
                // The cursor's own move trails the sequence that shows it: the
                // frame is done, and what is left is not a row.
                self.drain(&mut drawn);
                return self.screen();
            }
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.screen()
    }

    fn wait_until(&self, child: &mut Child, reached: &dyn Fn(&str) -> bool) -> String {
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
            let screen = self.screen();
            if reached(&screen) {
                return screen;
            }
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.screen()
    }

    pub fn type_key(&self, byte: u8) {
        let _ = self.port().write_all(&[byte]);
    }

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
