//! A paste and a pipe are the same bytes: a reader that is not a terminal — a pipe, a
//! file, `/dev/null` — has no echo to turn off and is read exactly as it is.

use std::io::{self, BufRead, IsTerminal, Write};

use rustix::termios::{LocalModes, OptionalActions, Termios, tcgetattr, tcsetattr};

const PROMPT: &str = "api key   paste it, then press Enter: ";

/// A terminal whose echo cannot be turned off is an error rather than a read: a key typed
/// where it can be seen is the one thing this function exists to avoid.
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

    // The Enter that ended the paste was never echoed, and a pipe shows nothing at all,
    // so the prompt ends its own line here.
    writeln!(stderr)?;
    Ok(line.trim().to_owned())
}

/// The terminal's echo, off for as long as this guard lives and back on when it is
/// dropped — on a panic too, a terminal left silent being a bug nobody can see.
struct Echo {
    restore: Option<Termios>,
}

impl Echo {
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
