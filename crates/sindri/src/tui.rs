//! The terminal UI's contract with the terminal it draws on: the alternate screen it
//! takes and gives back, raw mode with flow control off, the keys it reads and the
//! sequences it writes back, and a refusal at exit 2 when there is no terminal to do
//! any of that on (§3).
//!
//! The terminal itself is the one thing this crate owns and the core may not name
//! (ADR-0005): the alternate screen, raw mode, the keys read off the tty and the
//! sequences written back to it all live in this module, and what a frame holds is
//! the core's business rather than the terminal's.
//!
//! The loop is a loop that waits. One frame is painted when the screen is taken — the
//! boot frame, empty transcript and all (§12) — and after it a key arrives as a byte
//! from `read`, so a launch with nobody at the keyboard writes nothing further: there
//! is no timer, no tick and no repaint, which is what §12's "an idle frame is never
//! drawn" means as code.

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::panic::{self, PanicHookInfo};
use std::process::ExitCode;
use std::sync::{Mutex, MutexGuard, Once, OnceLock};

use ratatui::backend::CrosstermBackend;
use rustix::termios::{InputModes, OptionalActions, Termios, tcgetattr, tcsetattr};
use sindri::view::View;

/// Spec §15's usage error: a launch that cannot be a run, a screen a launch cannot
/// draw on.
const USAGE: u8 = 2;

/// Spec §15's setup failure: a terminal that will not take the screen or will not
/// give a key back.
const SETUP: u8 = 3;

/// `Ctrl-Q`, the byte that quits: `0x11`, which is why flow control has to be off —
/// a terminal left with `IXON` eats this byte instead of sending it.
const QUIT: u8 = 0x11;

/// Take the alternate screen, and hide the cursor so a frame is not drawn around a
/// caret sitting wherever the last shell left it.
const TAKE_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l";

/// Give the alternate screen back, cursor first, so a shell gets a terminal that
/// looks the way it did.
const GIVE_BACK_SCREEN: &[u8] = b"\x1b[?25h\x1b[?1049l";

/// The terminal UI: takes the screen, holds it until `Ctrl-Q`, gives it back.
pub fn run() -> ExitCode {
    if let Some(reason) = missing_terminal() {
        return crate::refuse(&reason, USAGE);
    }
    let terminal = match Terminal::stdio() {
        Ok(terminal) => terminal,
        Err(error) => {
            return crate::refuse(&format!("the terminal could not be taken: {error}"), SETUP);
        }
    };
    let screen = match Screen::take(&terminal) {
        Ok(screen) => screen,
        Err(error) => {
            return crate::refuse(&format!("the terminal would not go raw: {error}"), SETUP);
        }
    };
    if let Err(error) = paint(&terminal) {
        return crate::refuse(&format!("the frame could not be painted: {error}"), SETUP);
    }
    let keys = keys(&terminal);
    drop(screen);
    match keys {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => crate::refuse(&format!("the terminal could not be read: {error}"), SETUP),
    }
}

/// What a launch with no prompt is short of when it is short of a terminal: the
/// stream that is not one, and the thing a person can do about it.
fn missing_terminal() -> Option<String> {
    if !io::stdout().is_terminal() {
        return Some(String::from(
            "the terminal UI needs a terminal to draw on, and stdout is not one: \
             give a prompt with -b to run as a filter",
        ));
    }
    if !io::stdin().is_terminal() {
        return Some(String::from(
            "the terminal UI needs a terminal to read keys from, and stdin is not one: \
             give a prompt with -b to run as a filter",
        ));
    }
    None
}

/// The terminal this front end draws on, held as the two fds it really is — the one
/// keys arrive on and the one frames leave by — so a test can hand in a
/// pseudo-terminal and read what a person would have seen.
struct Terminal {
    /// Where keys come from: the terminal's input side, whose termios is raw mode.
    keys: File,
    /// Where frames go: the terminal's output side, the alternate screen.
    screen: File,
}

impl Terminal {
    /// This process's own standard input and output, which `run` has already
    /// refused to build a UI on unless both are terminals.
    fn stdio() -> io::Result<Terminal> {
        Terminal::pair(io::stdin().as_fd(), io::stdout().as_fd())
    }

    /// A terminal from the two ends a caller names, taken as copies so this value
    /// owns no descriptor the rest of the process still reads and writes.
    fn pair(keys: BorrowedFd<'_>, screen: BorrowedFd<'_>) -> io::Result<Terminal> {
        Ok(Terminal {
            keys: File::from(keys.try_clone_to_owned()?),
            screen: File::from(screen.try_clone_to_owned()?),
        })
    }
}

/// The screen a launch holds: taking one is what puts the terminal in raw mode, and
/// dropping it is what gives the terminal back. A guard rather than two calls, so
/// there is no path — a `?`, an early return, a panic — that can forget half.
struct Screen;

impl Screen {
    /// Takes the alternate screen and raw mode together, remembering the state that
    /// replacing them is a change to.
    fn take(terminal: &Terminal) -> io::Result<Screen> {
        guard_panics();
        let saved = tcgetattr(&terminal.keys)?;
        let mut raw = saved.clone();
        raw.make_raw();
        // `make_raw` clears flow control already, and this line stays because the
        // day raw mode is spelled differently is the day `Ctrl-Q` stops arriving.
        raw.input_modes.remove(InputModes::IXON);
        tcsetattr(&terminal.keys, OptionalActions::Now, &raw)?;

        // Both halves are handed to the one place a panic hook can reach, so the
        // screen is given back even when the unwinding that follows runs no guard.
        let restore = Restore {
            keys: terminal.keys.try_clone()?,
            screen: terminal.screen.try_clone()?,
            saved,
        };
        *lock(&GIVE_BACK) = Some(restore);
        if let Err(error) = tell(&terminal.screen, TAKE_SCREEN) {
            give_back();
            return Err(error);
        }
        Ok(Screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        give_back();
    }
}

/// Paints the frame a launch owes, and only that one: the boot frame.
///
/// It goes through `View::paint` — the seam the fixture harness renders through
/// (ADR-0012) — onto this terminal's own output side, so what a reader sees at launch
/// and what a golden frame holds are one picture and not two. After this the loop
/// blocks in `read`, and nothing is painted until a key or a fact asks for it, because
/// §12's idle frame is never drawn at all.
fn paint(terminal: &Terminal) -> io::Result<()> {
    let view = View::boot();
    let mut canvas = ratatui::Terminal::new(CrosstermBackend::new(&terminal.screen))?;
    canvas.draw(|frame| view.paint(frame.area(), frame.buffer_mut()))?;
    Ok(())
}

/// Reads the terminal a key at a time until `Ctrl-Q`.
///
/// The loop blocks in `read` and paints nothing while it waits: a launch with
/// nobody at the keyboard asks the kernel for a byte and sleeps, so an idle frame
/// is not skipped, it is never drawn.
fn keys(terminal: &Terminal) -> io::Result<()> {
    let mut input = &terminal.keys;
    let mut key = [0u8; 1];
    loop {
        match input.read(&mut key) {
            // The terminal went away: there is nothing left to quit from, and the
            // screen is still given back by the guard.
            Ok(0) => return Ok(()),
            Ok(_) if key[0] == QUIT => return Ok(()),
            Ok(_) => {}
            Err(error) => return Err(error),
        }
    }
}

/// What the terminal is to be given back as: the state raw mode replaced and the two
/// fds it is put back through, held where a panic hook can reach them, because an
/// unwinding path runs no guard this module owns.
static GIVE_BACK: Mutex<Option<Restore>> = Mutex::new(None);

/// The state to put back, and the terminal it goes to.
struct Restore {
    /// The terminal's input side, whose termios raw mode changed.
    keys: File,
    /// The terminal's output side, where the alternate screen was taken.
    screen: File,
    /// The terminal's state before raw mode.
    saved: Termios,
}

impl Restore {
    /// Puts the terminal back: the screen is left first, so nothing that follows —
    /// a panic message included — is drawn on a screen that is about to be somebody
    /// else's, and then the state raw mode replaced is restored.
    fn put_back(self) {
        let _ = tell(&self.screen, GIVE_BACK_SCREEN);
        let _ = tcsetattr(&self.keys, OptionalActions::Now, &self.saved);
    }
}

/// Gives the terminal back, exactly once, whichever of the guard and the panic hook
/// asks first: the state is taken out of the static, so the second ask finds nothing
/// and a shell is never handed a half-restored terminal twice.
fn give_back() {
    let taken = lock(&GIVE_BACK).take();
    if let Some(restore) = taken {
        restore.put_back();
    }
}

/// A panic hook, in the shape `std::panic` hands out and takes back: what the hook
/// this module installs keeps of the one it replaced, so a panic still gets reported.
type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

/// Installs, once, the hook that gives the terminal back when a panic unwinds past
/// the guard: a terminal left raw and blank is a shell nobody can type in, and an
/// unwinding path runs no guard at all.
fn guard_panics() {
    /// Whether this process has a hook of this module's yet.
    static INSTALLED: Once = Once::new();
    /// The hook this one replaced, which is still what reports a panic.
    static REPLACED: OnceLock<Hook> = OnceLock::new();

    INSTALLED.call_once(|| {
        // The replaced hook is called rather than dropped: a terminal given back is
        // no reason for a panic to go unreported, and a panic nobody can read is the
        // bug this hook is least able to catch.
        let _ = REPLACED.set(panic::take_hook());
        panic::set_hook(Box::new(|raised| {
            give_back();
            if let Some(replaced) = REPLACED.get() {
                replaced(raised);
            }
        }));
    });
}

/// Writes a sequence out and flushes it: a frame that sat in a buffer is a frame
/// nobody saw, and a screen given back only at exit is a terminal left blank when
/// the process dies.
fn tell(out: &File, sequence: &[u8]) -> io::Result<()> {
    let mut out = out;
    out.write_all(sequence)?;
    out.flush()
}

/// A lock held for the length of one statement: a panic hook that blocked on a
/// poisoned lock would nest the panic it was called to clean up after.
fn lock<T>(held: &Mutex<T>) -> MutexGuard<'_, T> {
    held.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    //! The one contract an integration test cannot reach from outside the process: a
    //! panic unwinds past no guard this module owns, so the hook is proved here, on a
    //! pseudo-terminal of this process's own, where a panic can be raised and caught
    //! on purpose and the terminal's state read back afterwards.

    use std::panic;
    use std::path::PathBuf;

    use rustix::fs::{Mode, OFlags, open};
    use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
    use rustix::termios::LocalModes;

    use super::*;

    /// A pseudo-terminal the test holds both ends of: the slave stands in for both
    /// standard streams a launch would be handed, and the master is where the
    /// sequences a launch writes come back.
    struct Pty {
        /// The end nobody in the program reads, where what it drew lands.
        master: File,
        /// The end the program takes as its terminal.
        slave: File,
    }

    impl Pty {
        /// A fresh pseudo-terminal, nobody's controlling terminal.
        fn open() -> Pty {
            let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("a pseudoterminal");
            grantpt(&master).expect("the slave is granted");
            unlockpt(&master).expect("the slave is unlocked");
            let name = ptsname(&master, Vec::new()).expect("the slave has a name");
            let path = PathBuf::from(name.to_str().expect("a printable name"));
            let slave = open(path.as_path(), OFlags::RDWR | OFlags::NOCTTY, Mode::empty())
                .expect("the slave opens");
            Pty {
                master: File::from(master),
                slave: File::from(slave),
            }
        }

        /// Everything written to the terminal since the last read of it.
        fn drawn(&mut self) -> String {
            let mut drawn = [0u8; 4096];
            let mut master = &self.master;
            let count = master.read(&mut drawn).expect("the terminal's screen");
            String::from_utf8_lossy(&drawn[..count]).into_owned()
        }

        /// The terminal's state, as the program left it.
        fn attrs(&self) -> Termios {
            tcgetattr(&self.slave).expect("the terminal's state")
        }
    }

    #[test]
    fn a_panic_gives_the_terminal_back_before_the_guard_can() {
        let mut pty = Pty::open();
        let before = pty.attrs();
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");
        let screen = Screen::take(&terminal).expect("the screen is taken");

        assert!(
            !pty.attrs().local_modes.contains(LocalModes::ICANON),
            "raw mode is in force, so there is something for a panic to undo"
        );

        let caught = panic::catch_unwind(|| panic!("a panic the hook has to clean up after"));

        assert!(caught.is_err(), "the panic really was raised");
        let after = pty.attrs();
        assert_eq!(
            after.local_modes.bits(),
            before.local_modes.bits(),
            "the terminal's local modes are the ones raw mode replaced"
        );
        assert_eq!(
            after.input_modes.bits(),
            before.input_modes.bits(),
            "and its input modes too, flow control included"
        );
        let drawn = pty.drawn();
        assert!(
            drawn.contains(std::str::from_utf8(GIVE_BACK_SCREEN).expect("a printable sequence")),
            "the alternate screen was given back while the guard was still alive: {drawn:?}"
        );

        // The guard is dropped after the panic has already been cleaned up after, and
        // a terminal handed back twice is a shell with its state overwritten.
        drop(screen);
        assert_eq!(
            pty.attrs().input_modes.bits(),
            before.input_modes.bits(),
            "the second give-back changes nothing"
        );
    }
}
