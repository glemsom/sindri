//! The terminal as a resource: raw mode with flow control off, without which `Ctrl-Q` never
//! arrives, the alternate screen taken and given back on every path out including a panic, the
//! canvas frames are painted through, and the telling a sequence is written out by.

use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::panic::{self, PanicHookInfo};
use std::sync::{Mutex, MutexGuard, Once, OnceLock};

use ratatui::backend::CrosstermBackend;
use rustix::termios::{InputModes, OptionalActions, Termios, tcgetattr, tcsetattr};
use sindri::view::View;

/// Takes the alternate screen with the cursor hidden, so the first frame is not drawn
/// around a caret the last shell left, and asks for bracketed paste so a paste is one edit.
const TAKE_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?2004h";

/// Gives the screen back with bracketed paste off first and the cursor last, so a shell
/// gets the terminal it had.
const GIVE_BACK_SCREEN: &[u8] = b"\x1b[?2004l\x1b[?25h\x1b[?1049l";

pub(crate) struct Terminal {
    pub(crate) keys: File,
    pub(crate) screen: File,
}

impl Terminal {
    pub(crate) fn stdio() -> io::Result<Terminal> {
        Terminal::pair(io::stdin().as_fd(), io::stdout().as_fd())
    }

    pub(crate) fn pair(keys: BorrowedFd<'_>, screen: BorrowedFd<'_>) -> io::Result<Terminal> {
        Ok(Terminal {
            keys: File::from(keys.try_clone_to_owned()?),
            screen: File::from(screen.try_clone_to_owned()?),
        })
    }
}

/// The screen a launch holds: taking one puts the terminal in raw mode and dropping it gives
/// the terminal back. A guard, so no `?`, early return or panic can forget half of it.
pub(crate) struct Screen;

impl Screen {
    pub(crate) fn take(terminal: &Terminal) -> io::Result<Screen> {
        guard_panics();
        let saved = tcgetattr(&terminal.keys)?;
        let mut raw = saved.clone();
        raw.make_raw();
        // `make_raw` clears flow control too, and this stays because the day raw mode is spelled
        // differently is the day `Ctrl-Q` stops arriving.
        raw.input_modes.remove(InputModes::IXON);
        tcsetattr(&terminal.keys, OptionalActions::Now, &raw)?;

        // Handed to the panic hook as well, so the screen is given back even when the unwinding
        // that follows runs no guard.
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

/// The screen frames are painted through, taken once: a canvas that remembered nothing would
/// rewrite every cell of the frame on every key.
pub(crate) type Canvas<'a> = ratatui::Terminal<CrosstermBackend<&'a File>>;

pub(crate) fn canvas(terminal: &Terminal) -> io::Result<Canvas<'_>> {
    ratatui::Terminal::new(CrosstermBackend::new(&terminal.screen))
}

/// The same canvas at a size already known, so nothing is asked of the operating system.
/// crossterm answers a size question by opening `/dev/tty` and, with none to open, by running
/// `tput`, whose `fork` a suite's parallel threads can have refused for want of process slots.
#[cfg(test)]
pub(crate) fn canvas_at(
    terminal: &Terminal,
    area: ratatui::layout::Rect,
) -> io::Result<Canvas<'_>> {
    ratatui::Terminal::with_options(
        CrosstermBackend::new(&terminal.screen),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        },
    )
}

/// Paints the frame the view holds now, and puts the terminal's cursor where the view says.
pub(crate) fn paint(canvas: &mut Canvas<'_>, view: &mut View) -> io::Result<()> {
    canvas.draw(|frame| {
        view.paint(frame.area(), frame.buffer_mut());
        if let Some(at) = view.cursor(frame.area()) {
            frame.set_cursor_position(at);
        }
    })?;
    Ok(())
}

/// What the terminal is to be given back as: the state raw mode replaced and the two fds it is
/// put back through, held where a panic hook can reach them, because an unwinding path runs no
/// guard this module owns.
static GIVE_BACK: Mutex<Option<Restore>> = Mutex::new(None);

struct Restore {
    keys: File,
    screen: File,
    saved: Termios,
}

impl Restore {
    /// Puts the terminal back: the screen is left first, so nothing that follows — a panic message
    /// included — is drawn on a screen that is about to be somebody else's, and then the state raw
    /// mode replaced is restored.
    fn put_back(self) {
        let _ = tell(&self.screen, GIVE_BACK_SCREEN);
        let _ = tcsetattr(&self.keys, OptionalActions::Now, &self.saved);
    }
}

/// Gives the terminal back, exactly once, whichever of the guard and the panic hook asks first:
/// the state is taken out of the static, so the second ask finds nothing and a shell is never
/// handed a half-restored terminal twice.
fn give_back() {
    let taken = lock(&GIVE_BACK).take();
    if let Some(restore) = taken {
        restore.put_back();
    }
}

/// A panic hook, in the shape `std::panic` hands out and takes back: what the hook this module
/// installs keeps of the one it replaced, so a panic still gets reported.
type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

/// Installs, once, the hook that gives the terminal back when a panic unwinds past the guard: a
/// terminal left raw and blank is a shell nobody can type in.
fn guard_panics() {
    static INSTALLED: Once = Once::new();
    static REPLACED: OnceLock<Hook> = OnceLock::new();

    INSTALLED.call_once(|| {
        // The replaced hook is called rather than dropped: a terminal given back is no reason for a
        // panic to go unreported.
        let _ = REPLACED.set(panic::take_hook());
        panic::set_hook(Box::new(|raised| {
            give_back();
            if let Some(replaced) = REPLACED.get() {
                replaced(raised);
            }
        }));
    });
}

/// Writes a sequence out and flushes it: a frame that sat in a buffer is a frame nobody saw, and
/// a screen given back only at exit is a terminal left blank when the process dies.
pub(crate) fn tell(out: &File, sequence: &[u8]) -> io::Result<()> {
    let mut out = out;
    out.write_all(sequence)?;
    out.flush()
}

/// A lock held for the length of one statement: a panic hook that blocked on a poisoned lock
/// would nest the panic it was called to clean up after.
pub(crate) fn lock<T>(held: &Mutex<T>) -> MutexGuard<'_, T> {
    held.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The area a test's canvas is told it has: the shape the test gave it, never the shape the
/// terminal reports, so nothing is asked of the operating system.
#[cfg(test)]
pub(crate) fn painted_at() -> ratatui::layout::Rect {
    ratatui::layout::Rect::new(0, 0, 80, 24)
}

#[cfg(test)]
pub(crate) mod pty {
    //! A pseudo-terminal of this process's own, for the contracts only a real terminal proves.

    use std::fs::File;
    use std::io::Read;
    use std::path::PathBuf;
    use std::time::Duration;

    use rustix::event::{PollFd, PollFlags, Timespec, poll};
    use rustix::fs::{Mode, OFlags, open};
    use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
    use rustix::termios::{Termios, Winsize, tcgetattr, tcsetwinsize};

    pub(crate) struct Pty {
        pub(crate) master: File,
        pub(crate) slave: File,
    }

    impl Pty {
        pub(crate) fn open() -> Pty {
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

        pub(crate) fn drawn(&mut self) -> String {
            let mut drawn = [0u8; 4096];
            let mut master = &self.master;
            let count = master.read(&mut drawn).expect("the terminal's screen");
            String::from_utf8_lossy(&drawn[..count]).into_owned()
        }

        pub(crate) fn resize(&self, cols: u16, rows: u16, xpixel: u16, ypixel: u16) {
            tcsetwinsize(
                &self.slave,
                Winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: xpixel,
                    ws_ypixel: ypixel,
                },
            )
            .expect("the terminal takes its size");
        }

        /// Whether the terminal has been written nothing at all: a master with no byte waiting
        /// is a terminal nobody spoke to, and asking must not block the way a read would.
        pub(crate) fn silent(&mut self) -> bool {
            let mut fds = [PollFd::new(&self.master, PollFlags::IN)];
            let wait = Timespec::try_from(Duration::from_millis(20)).expect("a span");
            poll(&mut fds, Some(&wait)).is_ok() && !fds[0].revents().contains(PollFlags::IN)
        }

        pub(crate) fn attrs(&self) -> Termios {
            tcgetattr(&self.slave).expect("the terminal's state")
        }
    }
}

#[cfg(test)]
mod tests {
    //! The contracts an integration test cannot reach from outside the process: the screen is a
    //! guard and a panic hook, and only a panic can prove which of them gives a terminal back.

    use std::panic;

    use rustix::termios::LocalModes;

    use crate::term::pty::Pty;

    use super::*;

    /// A canvas a test paints through is the shape the test gave it, not the shape the terminal
    /// reports: no `/dev/tty` is opened and no `tput` is spawned to answer a question a
    /// pseudo-terminal was never going to answer anyway.
    #[test]
    fn a_canvas_told_its_size_is_the_shape_it_was_told() {
        let pty = Pty::open();
        pty.resize(100, 30, 1000, 600);
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");

        let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
        let mut drawn = None;
        canvas
            .draw(|frame| drawn = Some(frame.area()))
            .expect("a frame");

        assert_eq!(
            drawn,
            Some(painted_at()),
            "the frame is the shape the test gave it, and the terminal's own 100 by 30 is not"
        );
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

        // The guard is dropped after the panic has already been cleaned up after, and a terminal
        // handed back twice is a shell with its state overwritten.
        drop(screen);
        assert_eq!(
            pty.attrs().input_modes.bits(),
            before.input_modes.bits(),
            "the second give-back changes nothing"
        );
    }
}
