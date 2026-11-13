//! The terminal as a resource: raw mode with flow control off, without which `Ctrl-Q` never
//! arrives, the alternate screen taken and given back on every path out including a panic, the
//! canvas frames are painted through, and the telling a sequence is written out by.

use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::panic::{self, PanicHookInfo};
use std::sync::{Mutex, MutexGuard, Once, OnceLock};

use ratatui::backend::{Backend as RatatuiBackend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use rustix::termios::{InputModes, OptionalActions, Termios, tcgetattr, tcsetattr};
use sindri::graphics::CellSize;
use sindri::link::Links;
use sindri::picture;
use sindri::pictures::Pictures;
use sindri::view::View;

use crate::osc;

/// Takes the alternate screen with the cursor hidden, so the first frame is not drawn around a
/// caret the last shell left, asks for bracketed paste so a paste is one edit, and pushes the key
/// protocol's disambiguation so `Shift+Enter` is told from `Enter`, on the terminal's own stack.
const TAKE_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?2004h\x1b[>1u";

/// Gives the screen back: the key protocol popped first, then bracketed paste off, then the
/// cursor, so a shell gets the terminal it had.
const GIVE_BACK_SCREEN: &[u8] = b"\x1b[<u\x1b[?2004l\x1b[?25h\x1b[?1049l";

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
pub(crate) type Canvas<'a> = ratatui::Terminal<Backend<&'a File>>;

/// Writes a frame to a terminal, with each destination a run of its cells names opened before the
/// run's first cell and closed after its last. A cell is a glyph and a look and has no room for a
/// destination, so the frame's ways travel beside it and are written here.
pub(crate) struct Backend<W: Write> {
    screen: CrosstermBackend<W>,
    links: Links,
    places: bool,
}

impl<W: Write> Backend<W> {
    /// A terminal that places no hyperlink is handed the words and no sequence: `places` is what
    /// the launch was told, and nothing here asks again.
    fn new(screen: W, places: bool) -> Backend<W> {
        Backend {
            screen: CrosstermBackend::new(screen),
            links: Links::none(),
            places,
        }
    }

    /// The destinations the frame about to be written names.
    fn links(&mut self, links: &Links) {
        self.links.clone_from(links);
    }

    /// One run of cells and the destination it names: opened before the first of them and closed
    /// after the last, so no cell past the run is a way to anywhere.
    fn run(&mut self, destination: Option<&str>, cells: &[(u16, u16, &Cell)]) -> io::Result<()> {
        if let Some(destination) = destination {
            self.screen.write_all(&osc::link(destination))?;
        }
        self.screen.draw(cells.iter().copied())?;
        if destination.is_some() {
            self.screen.write_all(&osc::link(""))?;
        }
        Ok(())
    }
}

impl<W: Write> RatatuiBackend for Backend<W> {
    /// The cells are written in runs of one destination, so two runs naming one destination are
    /// one link and a run naming none closes the one before it. A frame never leaves a destination
    /// open: what follows the frame would be a way to it.
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut cells: Vec<(u16, u16, &Cell)> = Vec::new();
        let mut naming: Option<String> = None;
        for (x, y, cell) in content {
            let destination = match self.places {
                true => self.links.at(y, x),
                false => None,
            };
            if destination != naming.as_deref() {
                let next = destination.map(String::from);
                let run = std::mem::take(&mut cells);
                self.run(naming.as_deref(), &run)?;
                naming = next;
            }
            cells.push((x, y, cell));
        }
        let run = std::mem::take(&mut cells);
        self.run(naming.as_deref(), &run)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.screen.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.screen.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.screen.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.screen.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.screen.clear()
    }

    fn size(&self) -> io::Result<Size> {
        self.screen.size()
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.screen.window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        RatatuiBackend::flush(&mut self.screen)
    }

    fn append_lines(&mut self, count: u16) -> io::Result<()> {
        self.screen.append_lines(count)
    }

    fn clear_region(&mut self, clear: ClearType) -> io::Result<()> {
        self.screen.clear_region(clear)
    }
}

pub(crate) fn canvas(terminal: &Terminal, places: bool) -> io::Result<Canvas<'_>> {
    ratatui::Terminal::new(Backend::new(&terminal.screen, places))
}

/// The same canvas at a size already known, so nothing is asked of the operating system:
/// crossterm answers a size question by opening `/dev/tty` and, with none to open, by running
/// `tput`, whose `fork` a suite's threads can have refused. A test's canvas places no hyperlink.
#[cfg(test)]
pub(crate) fn canvas_at(
    terminal: &Terminal,
    area: ratatui::layout::Rect,
) -> io::Result<Canvas<'_>> {
    ratatui::Terminal::with_options(
        Backend::new(&terminal.screen, false),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        },
    )
}

/// Paints the frame the view holds now, and puts the terminal's cursor where the view says. The
/// buffer is painted first, because the ways a frame names are known only after it is painted and
/// the backend must hold them before the frame is written.
pub(crate) fn paint(canvas: &mut Canvas<'_>, view: &mut View) -> io::Result<()> {
    canvas.autoresize()?;
    let area = canvas.current_buffer_mut().area;
    view.paint(area, canvas.current_buffer_mut());
    let cursor = view.cursor(area);
    canvas.backend_mut().links(view.links());
    canvas
        .draw(|frame| {
            if let Some(at) = cursor {
                frame.set_cursor_position(at);
            }
        })
        .map(|_| ())
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
    /// Puts the terminal back: the key protocol is popped and the screen left first, so nothing
    /// that follows — a panic message included — is drawn on a screen about to be somebody else's,
    /// and then the state raw mode replaced is restored.
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

/// A launch with no terminal to read a key from or draw on, and the reason it has none: a run
/// from a pipe is a filter, and `-b` is how a Unix filter is asked for one.
pub(crate) fn missing_terminal() -> Option<String> {
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

/// Hands `view` the pictures its terminal places: a Kitty terminal gets the pictures its frames
/// draw from and any other gets none, which leaves it the words. `term` and `program` are
/// arguments rather than reads so a test need not set an environment.
pub(crate) fn hand_pictures(
    view: &mut View,
    cell: CellSize,
    term: Option<&str>,
    program: Option<&str>,
) {
    view.set_pictures(match picture::speaks_kitty(term, program) {
        true => Pictures::kitty(cell),
        false => Pictures::none(),
    });
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

        /// Every byte the terminal has been handed: what the first read waits for, and then
        /// everything else it says, because one read of a pseudo-terminal is as much as had
        /// arrived and a frame is several writes.
        pub(crate) fn drawn(&mut self) -> String {
            let mut drawn = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let mut master = &self.master;
                let count = master.read(&mut chunk).expect("the terminal's screen");
                if count == 0 {
                    break;
                }
                drawn.extend_from_slice(&chunk[..count]);
                if self.silent() {
                    break;
                }
            }
            String::from_utf8_lossy(&drawn).into_owned()
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
    //! The contracts an integration test cannot reach from outside the process: a canvas's shape
    //! is a test's decision, and the guard and the panic hook both give the terminal back.

    use std::panic;

    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use rustix::termios::LocalModes;
    use sindri::ink::Ink;
    use sindri::link::{Link, Links};
    use sindri::palette::Palette;
    use sindri::pen::{self, Clip};

    use sindri_core::config::Config;
    use sindri_core::open::Ground;
    use sindri_core::policy::{Boundary, Enforcement};
    use sindri_core::records::Entry;
    use sindri_core::redact::SecretSet;

    use crate::term::pty::Pty;

    use super::*;

    /// The two facts the terminal gives reach the view, so a terminal that speaks Kitty is handed
    /// the pictures its frames draw from and one that speaks no protocol is handed none — the frame
    /// it paints is the words of a picture, whole.
    #[test]
    fn a_terminal_that_speaks_kitty_is_handed_pictures_and_one_that_speaks_none_is_handed_none() {
        let cell = CellSize::new(10, 20);

        let mut speaks = View::boot();
        hand_pictures(&mut speaks, cell, Some("xterm-kitty"), None);
        assert!(
            speaks.pictures().draws(),
            "a terminal that names Kitty is handed the pictures its frames draw from"
        );

        let mut silent = View::boot();
        silent.set_pictures(Pictures::kitty(cell));
        hand_pictures(&mut silent, cell, Some("screen-256color"), Some("tmux"));
        assert!(
            !silent.pictures().draws(),
            "and a terminal that names no protocol is handed none, which is the words of a \
             picture and no placement"
        );
    }

    /// A frame of `width` by `height` cells, the words each of `rows` holds, and the destinations
    /// it names, drawn through a pseudo-terminal of this process's own: the bytes the terminal at
    /// the far end is handed.
    fn drawn(width: u16, height: u16, rows: &[(u16, &str)], links: Links, places: bool) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        for (y, words) in rows {
            pen::write(
                &mut buffer,
                Rect {
                    y: *y,
                    height: 1,
                    ..area
                },
                &[(*words, Ink::Plain)],
                Clip::Edge,
                &Palette::TRUECOLOR,
            );
        }
        handed(buffer, links, places)
    }

    /// The bytes a terminal at the far end of a pseudo-terminal is handed for a frame and the ways
    /// it names, told whether that terminal places a hyperlink.
    fn handed(buffer: Buffer, links: Links, places: bool) -> String {
        let area = *buffer.area();
        let mut pty = Pty::open();
        let mut backend = Backend::new(
            pty.slave
                .try_clone()
                .expect("a second handle on the screen"),
            places,
        );
        backend.links(&links);
        backend
            .draw(Buffer::empty(area).diff(&buffer).into_iter())
            .expect("a drawn row");
        backend.flush().expect("a written row");
        drop(backend);
        pty.drawn()
    }

    /// The ground a run judges a target from in this test: the tree the product is built in, whose
    /// `GLOSSARY.md` is a file a call can name and be judged against.
    fn ground() -> Ground {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("the tree the product is built in");
        let boundary =
            Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
                .expect("the boundary holds");
        Ground::new(&boundary, workspace, workspace).expect("the ground resolves")
    }

    /// The ways `links` name, laid at frame row 0: how a row hands the frame what it names.
    fn ways(links: &[Link]) -> Links {
        let mut ways = Links::none();
        ways.lay(0, 0, links);
        ways
    }

    /// What a terminal reads of a frame as the ways it opens and closes and the words it writes:
    /// every other sequence is dropped, because where a way begins and ends is what this proves.
    fn spoken(drawn: &str) -> Vec<String> {
        let mut said = Vec::new();
        let mut words = String::new();
        let mut rest = drawn;
        while let Some(at) = rest.find('\x1b') {
            words.push_str(&rest[..at]);
            rest = &rest[at..];
            let (sequence, tail) = escaped(rest);
            rest = tail;
            if let Some(destination) = sequence
                .strip_prefix("\x1b]8;;")
                .and_then(|said| said.strip_suffix("\x1b\\"))
            {
                if !words.is_empty() {
                    said.push(std::mem::take(&mut words));
                }
                said.push(match destination.is_empty() {
                    true => String::from("closes"),
                    false => format!("opens {destination}"),
                });
            }
        }
        words.push_str(rest);
        if !words.is_empty() {
            said.push(words);
        }
        said
    }

    /// What a terminal reads of a frame as words alone: the ways it opens and closes are not words.
    fn words(drawn: &str) -> String {
        spoken(drawn)
            .into_iter()
            .filter(|said| said != "closes" && !said.starts_with("opens "))
            .collect()
    }

    /// One escape sequence and the bytes after it: an OSC runs to its `ST`, a CSI to the byte that
    /// ends it, and anything else is the two bytes it was written as.
    fn escaped(bytes: &str) -> (&str, &str) {
        let end = match bytes.as_bytes().get(1) {
            Some(b']') => bytes.find("\x1b\\").map_or(bytes.len(), |at| at + 2),
            Some(b'[') => bytes
                .bytes()
                .position(|byte| (0x40..=0x7e).contains(&byte) && byte != b'[')
                .map_or(bytes.len(), |at| at + 1),
            _ => 2.min(bytes.len()),
        };
        bytes.split_at(end)
    }

    #[test]
    fn a_run_that_names_a_destination_is_written_around_the_cells_it_covers() {
        let links = ways(&[Link::new(4, 4, "https://example.test/spec")]);

        assert_eq!(
            spoken(&drawn(40, 1, &[(0, "the spec is here")], links, true)),
            [
                "the ",
                "opens https://example.test/spec",
                "spec",
                "closes",
                " is here"
            ],
            "the way opens before the run's first cell and closes after its last"
        );
    }

    #[test]
    fn two_runs_that_name_one_destination_are_one_link() {
        let links = ways(&[
            Link::new(0, 3, "https://example.test/spec"),
            Link::new(3, 5, "https://example.test/spec"),
        ]);

        assert_eq!(
            spoken(&drawn(40, 1, &[(0, "one spec")], links, true)),
            ["opens https://example.test/spec", "one spec", "closes"],
            "two runs of one frame naming one destination are one way to it"
        );
    }

    #[test]
    fn a_run_that_names_no_destination_closes_the_one_before_it() {
        let links = ways(&[Link::new(0, 3, "https://example.test/spec")]);

        assert_eq!(
            spoken(&drawn(40, 1, &[(0, "the spec is here")], links, true)),
            [
                "opens https://example.test/spec",
                "the",
                "closes",
                " spec is here"
            ],
            "a cell the run does not cover is no way to anywhere, so the way before it is closed"
        );
    }

    #[test]
    fn a_run_wider_than_the_row_it_was_laid_on_carries_nothing_to_the_row_below() {
        let mut links = Links::none();
        links.lay(0, 0, &[Link::new(0, 80, "https://example.test/spec")]);

        assert_eq!(
            spoken(&drawn(
                12,
                2,
                &[(0, "the spec"), (1, "its gate")],
                links,
                true
            )),
            [
                "opens https://example.test/spec",
                "the spec",
                "closes",
                "its gate"
            ],
            "a run is one row's: the row below it is drawn with no destination at all"
        );
    }

    /// A frame written through `paint`, and the same frame written through the canvas's own draw:
    /// what the launch paints is what a reader would have seen from the library it draws through.
    fn written(pty: &Pty, launch: bool) {
        let terminal =
            Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to write to");
        let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
        let mut view = View::boot();
        match launch {
            true => {
                paint(&mut canvas, &mut view).expect("a painted frame");
            }
            false => {
                canvas
                    .draw(|frame| {
                        view.paint(frame.area(), frame.buffer_mut());
                        if let Some(at) = view.cursor(frame.area()) {
                            frame.set_cursor_position(at);
                        }
                    })
                    .expect("a drawn frame");
            }
        }
    }

    #[test]
    fn the_frame_the_launch_paints_is_the_frame_the_canvas_draws() {
        let mut launch = Pty::open();
        let mut canvas = Pty::open();
        written(&launch, true);
        written(&canvas, false);

        assert_eq!(
            launch.drawn(),
            canvas.drawn(),
            "the cursor the view hands the canvas is placed, and the frame beside it is the frame"
        );
    }

    #[test]
    fn a_terminal_that_places_no_hyperlink_reads_a_link_and_a_path_as_their_words() {
        let area = Rect::new(0, 0, 60, 4);
        let mut frame = Buffer::empty(area);
        let mut links = Links::none();
        let answer = sindri::markdown::rows("see [the gate](https://example.test/gate)", 60);
        let call = sindri::rows::layout_on(
            &Entry::Call {
                turn: 1,
                name: SecretSet::default().redact("open"),
                arguments: SecretSet::default().redact(r#"{"path": "GLOSSARY.md"}"#),
            },
            60,
            &sindri::picture::Surface::none(),
            Some(&ground()),
        );
        for (y, row) in answer.iter().chain(call.iter()).enumerate() {
            row.paint(
                Rect {
                    y: u16::try_from(y).unwrap(),
                    height: 1,
                    ..area
                },
                &mut frame,
                &Palette::TRUECOLOR,
                &mut links,
            );
        }

        let places = handed(frame.clone(), links.clone(), true);
        let silent = handed(frame, links, false);

        assert_eq!(
            words(&silent),
            words(&places),
            "a terminal that places no hyperlink reads every one of these rows as the words the \
             frame drew, and a terminal that places one reads the same words"
        );
        assert!(
            !silent.contains("\x1b]8;"),
            "and it is handed no sequence at all: {silent:?}"
        );
        assert!(
            places.contains("\x1b]8;;https://example.test/gate\x1b\\"),
            "where the terminal that places one is handed the destination around the words"
        );
        assert!(
            places.contains("\x1b]8;;file://"),
            "and the path a call names is handed the file it was judged against"
        );
    }

    #[test]
    fn a_terminal_that_places_no_hyperlink_is_handed_the_words_and_no_sequence() {
        let links = ways(&[Link::new(0, 16, "https://example.test/spec")]);
        let handed = drawn(40, 1, &[(0, "the spec is here")], links, false);

        assert_eq!(spoken(&handed), ["the spec is here"]);
        assert!(
            !handed.contains("\x1b]8;"),
            "and no sequence at all: {handed:?}"
        );
    }

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
        assert!(
            drawn
                .find("\u{1b}[<u")
                .zip(drawn.find("\u{1b}[?1049l"))
                .is_some_and(|(keys, screen)| keys < screen),
            "and the key protocol was given back before the screen was, by the hook and not \
             only by the guard: {drawn:?}"
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
