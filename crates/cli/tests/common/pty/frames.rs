//! Waiting for what a frame says. A frame reaches the pty as it is written, so a
//! read can catch a row that has not finished arriving; the cursor's own
//! sequence is the frame's last act, and a wait that reads a whole frame ends
//! there. Every wait has a deadline, so a frame that never draws fails instead
//! of hanging.

use std::io::{ErrorKind, Read};
use std::process::Child;
use std::time::{Duration, Instant};

use super::{DEADLINE, Pty, SHOW_CURSOR};
use crate::common::screen::says;

/// How long a pipe stays quiet before a watchful wait accepts that a frame
/// whose cursor arrived with its words has nothing more to say.
const QUIET: Duration = Duration::from_millis(150);
/// What a wait sleeps between reads it found nothing in.
const POLL: Duration = Duration::from_millis(10);

impl Pty {
    pub fn wait_for_screen(&self, child: &mut Child, want: &str) -> String {
        self.wait_until(child, &|screen: &str| screen.contains(want))
    }

    pub fn wait_for_words(&self, child: &mut Child, want: &str) -> String {
        self.wait_until(child, &|screen: &str| says(screen, want))
    }

    /// Waits for the next frame to be drawn whole. A frame ends by showing the
    /// terminal's cursor, and an idle frame draws nothing, so the next end
    /// after a keystroke is the frame that keystroke drew. A test that
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

    /// Waits for a screen that says `want`, and for the frame that said it to be
    /// written whole. The words alone are not enough: a row arrives a cell at a
    /// time, so `want` can match a row whose rest — the reason on the row below,
    /// the path a row was cut after — is still on its way. A test that reads a
    /// screen beyond the words it waited for comes here.
    pub fn wait_for_whole_screen(&self, child: &mut Child, want: &str) -> String {
        self.wait_for_whole(child, &|screen: &str| screen.contains(want))
    }

    /// The same, for words a row may wrap: see `says`.
    pub fn wait_for_whole_words(&self, child: &mut Child, want: &str) -> String {
        self.wait_for_whole(child, &|screen: &str| says(screen, want))
    }

    /// The words, then the frame's end. The frame ends by showing the terminal's
    /// cursor, so a cursor that arrives after the words is the frame whole. A
    /// frame that was already whole when the words appeared has its cursor
    /// behind them, where it cannot be told apart from the frame before: there
    /// the pipe's silence is the only end there is, and it is short.
    ///
    /// The bytes are searched as they accumulate, never a read at a time: a read
    /// can end in the middle of the cursor's own sequence.
    fn wait_for_whole(&self, child: &mut Child, reached: &dyn Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + DEADLINE;
        let mut port = self.port();
        let mut buffer = [0u8; 4096];
        let mut since_words = String::new();
        let mut said = false;
        let mut quiet = Instant::now();
        while Instant::now() < deadline {
            match port.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    quiet = Instant::now();
                    let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                    self.look(&text);
                    if said {
                        since_words.push_str(&text);
                        if since_words.contains(SHOW_CURSOR) {
                            return self.screen();
                        }
                    }
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    if said && quiet.elapsed() >= QUIET {
                        return self.screen();
                    }
                    std::thread::sleep(POLL);
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => std::thread::sleep(POLL),
                Err(_) => break,
            }
            if !said && reached(&self.screen()) {
                said = true;
                quiet = Instant::now();
            }
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                break;
            }
        }
        self.screen()
    }
}
