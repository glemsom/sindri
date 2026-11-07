//! Reading a key off a terminal: the prompt is written first, the terminal's echo
//! is off while the key is typed, and the value leaves by this function's return
//! and by nothing else, so it cannot reach a scrollback or a log line.
//!
//! A reader that is not a terminal — a pipe, a file, `/dev/null` — has no echo to
//! turn off, and is read exactly as it is: a paste and a pipe are the same bytes.

use std::io::{self, BufRead, IsTerminal, Write};

use rustix::termios::{LocalModes, OptionalActions, Termios, tcgetattr, tcsetattr};

/// What the terminal is asked with, before it is asked to stop echoing.
const PROMPT: &str = "api key   paste it, then press Enter: ";

/// Asks for the key on stderr and reads one line of it with the terminal's echo
/// turned off, so the value is not left where anyone can read it over a shoulder.
/// A terminal echo cannot be turned off on is an error rather than a read: a key
/// typed where it can be seen is the one thing this function exists to avoid.
///
/// The line comes back trimmed: a paste carries the newline that ended it and the
/// spaces around it, and neither is part of the key.
pub fn ask() -> io::Result<String> {
    let terminal = io::stdin().is_terminal();
    let mut stderr = io::stderr();
    write!(stderr, "{PROMPT}")?;
    stderr.flush()?;

    let echo = Echo::off(terminal)?;
    let mut line = String::new();
    let read = io::stdin().lock().read_line(&mut line);
    drop(echo);
    read?;

    // The Enter that ended the paste is not on the screen when the terminal was
    // silenced, and a pipe shows nothing in the first place, so the prompt's own
    // line is ended here rather than where the person typed.
    writeln!(stderr)?;
    Ok(line.trim().to_owned())
}

/// The terminal's echo, off for as long as this guard lives and back on when it is
/// dropped — on a panic too, which is a terminal left silent being a bug nobody
/// can see.
struct Echo {
    /// The state to put back, or nothing when there was no terminal to silence.
    restore: Option<Termios>,
}

impl Echo {
    /// Turns echo off when the reader is a terminal, and does nothing at all when
    /// it is not: a pipe echoes nothing in the first place.
    fn off(terminal: bool) -> io::Result<Echo> {
        if !terminal {
            return Ok(Echo { restore: None });
        }
        let stdin = io::stdin();
        let before = tcgetattr(&stdin)?;
        let mut silent = before.clone();
        silent.local_modes.remove(LocalModes::ECHO);
        tcsetattr(&stdin, OptionalActions::Now, &silent)?;
        Ok(Echo {
            restore: Some(before),
        })
    }
}

impl Drop for Echo {
    fn drop(&mut self) {
        if let Some(before) = &self.restore {
            let _ = tcsetattr(io::stdin(), OptionalActions::Now, before);
        }
    }
}
