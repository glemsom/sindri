//! The terminal as a resource: raw mode with flow control off, without which `Ctrl-Q` never
//! arrives, the alternate screen taken and given back on every path out including a panic, the
//! canvas frames are painted through, and the telling a sequence is written out by.

use base64::Engine;
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::panic::{self, PanicHookInfo};
use std::sync::{Mutex, MutexGuard, Once, OnceLock};
use std::time::Duration;

use ratatui::backend::{Backend as RatatuiBackend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::termios::{
    InputModes, OptionalActions, Termios, Winsize, tcgetattr, tcgetwinsize, tcsetattr,
};
use sindri::capability::{Asked, Question};
use sindri::graphics::CellSize;
use sindri::link::Links;
use sindri::plane::{self, Plane, PlaneId, Planes, Shade};
use sindri::size::{Laid, Sizes};
use sindri::view::{Painted, View};

use crate::osc;

/// Takes the alternate screen with the cursor hidden, so the first frame is not drawn around a
/// caret the last shell left, asks for bracketed paste so a paste is one edit, and pushes the key
/// protocol's disambiguation so `Shift+Enter` is told from `Enter`, on the terminal's own stack.
const TAKE_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?2004h\x1b[>1u";

/// `CSI ? 1004 h`: the terminal reports which of its windows is the one being read, which is the
/// only way a frame knows its accent edge has nobody looking at it. Asked of a terminal that takes
/// the window's sequences, and of no other.
const ASK_FOCUS: &[u8] = b"\x1b[?1004h";

/// Gives the screen back: the key protocol popped first, then bracketed paste off, then the
/// cursor, so a shell gets the terminal it had.
const GIVE_BACK_SCREEN: &[u8] = b"\x1b[<u\x1b[?2004l\x1b[?25h\x1b[?1049l";

/// `CSI ? 1004 l`: the report is given up, or a shell is left sending one.
const GIVE_UP_FOCUS: &[u8] = b"\x1b[?1004l";

/// `CSI ? 1000 h` and `CSI ? 1006 h`: the terminal reports a press and the wheel in the `SGR`
/// spelling, which is the one that names the cell a press landed on. Asked of a terminal that
/// reports a pointer, and of no other.
const TAKE_POINTER: &[u8] = b"\x1b[?1000h\x1b[?1006h";

/// `CSI ? 1000 l` and `CSI ? 1006 l`: the report is given up, or a shell is left sending presses.
const GIVE_UP_POINTER: &[u8] = b"\x1b[?1000l\x1b[?1006l";

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
    /// `window` is whether the terminal takes the sequences a window is written with, and `pointer`
    /// whether it reports one: the screen is taken either way, and a terminal nobody can vouch for
    /// is asked for no report at all.
    pub(crate) fn take(terminal: &Terminal, window: bool, pointer: bool) -> io::Result<Screen> {
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
            window,
            pointer,
        };
        *lock(&GIVE_BACK) = Some(restore);
        let mut taking = TAKE_SCREEN.to_vec();
        if window {
            taking.extend_from_slice(ASK_FOCUS);
        }
        if pointer {
            taking.extend_from_slice(TAKE_POINTER);
        }
        if let Err(error) = tell(&terminal.screen, &taking) {
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
pub(crate) type Canvas<'a> = CanvasOver<&'a File>;

/// A canvas over any writer: the terminal's own in a launch, and one a test can read in pieces.
pub(crate) type CanvasOver<W> = ratatui::Terminal<Backend<W>>;

/// The most of one frame held before it spills: a frame bigger than this reaches the terminal as it
/// is written, in more than one write.
const FRAME_HELD: usize = 8 * 1024;

/// `CSI ? 25 l`: the caret is given up for as long as a frame too big to be held whole is written,
/// because a terminal repaints between its writes and would show the caret in a cell that frame has
/// not placed it in.
const GIVE_UP_CARET: &[u8] = b"\x1b[?25l";

/// The bytes of the frame being written before they are the terminal's, and whether its caret has
/// been given up.
#[derive(Default)]
struct Writing {
    bytes: Vec<u8>,
    /// Set by the frame's first spill, and cleared only when the frame is the terminal's: a frame
    /// that spans several writes gives the caret up once.
    given_up: bool,
}

/// The screen one frame is written to, with what it has drawn so far held until the frame is a
/// whole one: the cells and the cell the caret is placed in then reach the terminal together.
struct Held<'a, W: Write> {
    screen: &'a mut W,
    writing: &'a mut Writing,
}

impl<W: Write> Held<'_, W> {
    /// Hands the screen what is held so far, so what is written next follows it.
    fn spill(&mut self) -> io::Result<()> {
        let held = std::mem::take(&mut self.writing.bytes);
        if held.is_empty() {
            return Ok(());
        }
        self.screen.write_all(&held)
    }

    fn give_up_the_caret(&mut self) -> io::Result<()> {
        if self.writing.given_up {
            return Ok(());
        }
        self.screen.write_all(GIVE_UP_CARET)?;
        self.writing.given_up = true;
        Ok(())
    }

    /// Says what is no part of a frame — a plane's bytes, a clear's deletes: what is held leaves
    /// first, so a word said between two frames never arrives inside one.
    fn tell(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.spill()?;
        self.screen.write_all(bytes)
    }

    /// The frame is the terminal's whole here: what is held leaves in one write, and the screen
    /// under it is flushed.
    fn hand_over(&mut self) -> io::Result<()> {
        self.spill()?;
        self.writing.given_up = false;
        self.screen.flush()
    }
}

impl<W: Write> Write for Held<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.writing.bytes.len() + bytes.len() > FRAME_HELD {
            self.give_up_the_caret()?;
            self.spill()?;
            if bytes.len() >= FRAME_HELD {
                return self.screen.write(bytes);
            }
        }
        self.writing.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// Nothing leaves here: crossterm flushes between the cells and the shown and placed caret, and
    /// the frame is the terminal's when the backend hands it over.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn holding<'a, W: Write>(screen: &'a mut W, writing: &'a mut Writing) -> Held<'a, W> {
    Held { screen, writing }
}

/// The screen a frame is drawn through, and the bytes it draws with: borrowed apart from the backend
/// so a frame's own links and sizes can be read while it is written.
fn painting<'a, W: Write>(
    screen: &'a mut W,
    writing: &'a mut Writing,
) -> CrosstermBackend<Held<'a, W>> {
    CrosstermBackend::new(holding(screen, writing))
}

/// Writes a frame to a terminal, with each destination a run of its cells names opened before the
/// run's first cell and closed after its last, and each run of cells drawn at more than one cell
/// written as the sequence that draws it: a cell has room for neither, so both travel beside it.
pub(crate) struct Backend<W: Write> {
    screen: CrosstermBackend<W>,
    writing: Writing,
    links: Links,
    sizes: Sizes,
    places: bool,
    shades: Vec<Shade>,
}

impl<W: Write> Backend<W> {
    /// A terminal that places no hyperlink is handed the words and no sequence: `places` is what
    /// the launch was told, and nothing here asks again.
    fn new(screen: W, places: bool) -> Backend<W> {
        Backend {
            screen: CrosstermBackend::new(screen),
            writing: Writing::default(),
            links: Links::none(),
            sizes: Sizes::none(),
            places,
            shades: Vec::new(),
        }
    }

    /// The destinations the frame about to be written names.
    fn links(&mut self, links: &Links) {
        self.links.clone_from(links);
    }

    /// The runs the frame about to be written draws at more than one cell.
    fn sizes(&mut self, sizes: &Sizes) {
        self.sizes.clone_from(sizes);
    }

    /// The bands the frame shades and the images it holds of them: a shading that left is deleted
    /// by its identity, and one that arrived is uploaded as the frame's image of it. One with no
    /// image is no band the terminal is told of, and the next pass names it again.
    fn shades(&mut self, shades: &[Shade], images: &Planes) -> io::Result<()> {
        if self.shades == shades {
            return Ok(());
        }
        let before = std::mem::take(&mut self.shades);
        for shade in &before {
            if !shades.contains(shade)
                && let Some(id) = shade.id()
            {
                holding(&mut self.screen, &mut self.writing).tell(&plane::delete(id))?;
            }
        }
        for shade in shades {
            if before.contains(shade) {
                self.shades.push(*shade);
                continue;
            }
            if let Some(plane) = images.image(shade) {
                self.upload(plane)?;
                self.shades.push(*shade);
            }
        }
        Ok(())
    }

    /// One plane sent as the picture a terminal keeps under a band: the pixels as a PNG, in the
    /// chunks the protocol allows, under the identity, the rect and the depth the placement is made
    /// of, and `C=1`: a placement tall as the frame would otherwise scroll the screen it shades.
    fn upload(&mut self, plane: &Plane) -> io::Result<()> {
        let area = plane.area();
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(plane.pixels().clone())
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(io::Error::other)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        let mut sent = Vec::new();
        write!(sent, "\x1b7\x1b[{};{}H", area.y + 1, area.x + 1)?;
        for (index, chunk) in encoded.as_bytes().chunks(4096).enumerate() {
            let more = if (index + 1) * 4096 < encoded.len() {
                1
            } else {
                0
            };
            if index == 0 {
                write!(
                    sent,
                    "\x1b_Ga=T,f=100,i={},q=2,z=-1,C=1,c={},r={},m={more};",
                    plane.id().image(),
                    area.width,
                    area.height
                )?;
            } else {
                write!(sent, "\x1b_Gm={more};")?;
            }
            sent.extend_from_slice(chunk);
            sent.extend_from_slice(b"\x1b\\");
        }
        sent.extend_from_slice(b"\x1b8");
        holding(&mut self.screen, &mut self.writing).tell(&sent)
    }

    /// One run drawn as the sequence that makes it: written in the ink of the cell the diff handed
    /// over, because the sequence draws the words and no cell of them is written.
    fn large<S: RatatuiBackend + Write>(
        screen: &mut S,
        links: &Links,
        laid: &Laid,
        ink: &Cell,
        places: bool,
    ) -> io::Result<()> {
        let run = laid.run();
        let mut sequence = ink.clone();
        sequence.set_symbol(&String::from_utf8_lossy(&osc::sized(
            run.scale(),
            run.words(),
        )));
        let destination = match places {
            true => links.at(laid.row(), laid.left()).map(String::from),
            false => None,
        };
        if let Some(destination) = &destination {
            screen.write_all(&osc::link(destination))?;
        }
        screen.draw(std::iter::once((laid.left(), laid.row(), &sequence)))?;
        if destination.is_some() {
            screen.write_all(&osc::link(""))?;
        }
        Ok(())
    }

    /// One run of cells and the destination it names: opened before the first of them and closed
    /// after the last, so no cell past the run is a way to anywhere.
    fn run<S: RatatuiBackend + Write>(
        screen: &mut S,
        destination: Option<&str>,
        cells: &[(u16, u16, &Cell)],
    ) -> io::Result<()> {
        if let Some(destination) = destination {
            screen.write_all(&osc::link(destination))?;
        }
        screen.draw(cells.iter().copied())?;
        if destination.is_some() {
            screen.write_all(&osc::link(""))?;
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
        let mut screen = painting(&mut self.screen, &mut self.writing);
        let mut cells: Vec<(u16, u16, &Cell)> = Vec::new();
        let mut naming: Option<String> = None;
        let mut written: Vec<(u16, u16)> = Vec::new();
        for (x, y, cell) in content {
            let block = self
                .sizes
                .covers(y, x)
                .map(|laid| (laid.row(), laid.left()));
            if let Some((row, left)) = block {
                // The cells of a block are the sequence's on its first row, and the rows under it
                // are the block's and no cells of the frame at all.
                if row != y {
                    continue;
                }
                let run = std::mem::take(&mut cells);
                Backend::<W>::run(&mut screen, naming.as_deref(), &run)?;
                naming = None;
                if !written.contains(&(row, left))
                    && let Some(laid) = self.sizes.covers(y, x)
                {
                    written.push((row, left));
                    Backend::<W>::large(&mut screen, &self.links, laid, cell, self.places)?;
                }
                continue;
            }
            let destination = match self.places {
                true => self.links.at(y, x),
                false => None,
            };
            if destination != naming.as_deref() {
                let next = destination.map(String::from);
                let run = std::mem::take(&mut cells);
                Backend::<W>::run(&mut screen, naming.as_deref(), &run)?;
                naming = next;
            }
            cells.push((x, y, cell));
        }
        let run = std::mem::take(&mut cells);
        Backend::<W>::run(&mut screen, naming.as_deref(), &run)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        painting(&mut self.screen, &mut self.writing).hide_cursor()
    }

    /// The cells leave before the show, so the show and the placement that follows it are the
    /// frame's last write: a frame given up across its writes shows the caret in the cell it places
    /// and in no other.
    fn show_cursor(&mut self) -> io::Result<()> {
        if self.writing.given_up {
            holding(&mut self.screen, &mut self.writing).spill()?;
        }
        painting(&mut self.screen, &mut self.writing).show_cursor()
    }

    /// The question is put once the frame is the terminal's whole, so the cell it answers with is
    /// the one the frame placed.
    fn get_cursor_position(&mut self) -> io::Result<Position> {
        holding(&mut self.screen, &mut self.writing).spill()?;
        self.screen.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        painting(&mut self.screen, &mut self.writing).set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        for shade in std::mem::take(&mut self.shades) {
            if let Some(id) = shade.id() {
                holding(&mut self.screen, &mut self.writing).tell(&plane::delete(id))?;
            }
        }
        painting(&mut self.screen, &mut self.writing).clear()
    }

    fn size(&self) -> io::Result<Size> {
        self.screen.size()
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.screen.window_size()
    }

    /// The frame is the terminal's here, in the one write it was held for.
    fn flush(&mut self) -> io::Result<()> {
        holding(&mut self.screen, &mut self.writing).hand_over()
    }

    fn append_lines(&mut self, count: u16) -> io::Result<()> {
        painting(&mut self.screen, &mut self.writing).append_lines(count)
    }

    fn clear_region(&mut self, clear: ClearType) -> io::Result<()> {
        painting(&mut self.screen, &mut self.writing).clear_region(clear)
    }
}

#[cfg(not(test))]
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

/// Paints the frame the view holds now. The buffer is painted first, because the ways a frame names
/// are known only after it is painted and the backend must hold them before the frame is written.
/// The cursor is placed last, so the cell it lands in is the one this frame drew.
pub(crate) fn paint<W: Write>(canvas: &mut CanvasOver<W>, view: &mut View) -> io::Result<Painted> {
    canvas.autoresize()?;
    let area = canvas.current_buffer_mut().area;
    let painted = view.paint(area, canvas.current_buffer_mut());
    canvas.backend_mut().links(painted.links());
    canvas.backend_mut().sizes(painted.sizes());
    canvas
        .backend_mut()
        .shades(painted.shades(), view.planes())?;
    let caret = painted.caret();
    canvas.draw(|frame| {
        if let Some(caret) = caret {
            frame.set_cursor_position(caret);
        }
    })?;
    Ok(painted)
}

/// What the terminal is to be given back as: the state raw mode replaced and the two fds it is
/// put back through, held where a panic hook can reach them, because an unwinding path runs no
/// guard this module owns.
static GIVE_BACK: Mutex<Option<Restore>> = Mutex::new(None);

struct Restore {
    keys: File,
    screen: File,
    saved: Termios,
    window: bool,
    pointer: bool,
}

impl Restore {
    /// Puts the terminal back: the key protocol is popped and the screen left first, so nothing
    /// that follows — a panic message included — is drawn on a screen about to be somebody else's,
    /// and then the state raw mode replaced is restored.
    fn put_back(self) {
        let mut giving = Vec::new();
        for id in PlaneId::ALL {
            giving.extend(plane::delete(id));
        }
        giving.extend_from_slice(GIVE_BACK_SCREEN);
        if self.window {
            giving.extend_from_slice(GIVE_UP_FOCUS);
            // A mark outlives the process that set it, so the run it named is over whichever way
            // this launch ends: a panic that unwinds past the guard still takes it away.
            giving.extend(osc::progress(false));
        }
        if self.pointer {
            giving.extend_from_slice(GIVE_UP_POINTER);
        }
        let _ = tell(&self.screen, &giving);
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

/// The time a terminal that will answer a question takes to: one that cannot says nothing, and a
/// launch is never held open for it.
pub(crate) const REPLY_WAIT: Duration = Duration::from_millis(120);

/// Writes `ask` and reads whatever the terminal says back within the wait, or nothing at all when
/// it does not answer. The bytes are the terminal's, and a key typed while it thought is among them.
pub(crate) fn reply(terminal: &Terminal, ask: &[u8]) -> Option<Vec<u8>> {
    if tell(&terminal.screen, ask).is_err() {
        return None;
    }
    let wait = Timespec::try_from(REPLY_WAIT).ok()?;
    let mut fds = [PollFd::new(&terminal.keys, PollFlags::IN)];
    match poll(&mut fds, Some(&wait)) {
        Ok(_) if fds[0].revents().contains(PollFlags::IN) => {}
        _ => return None,
    }
    let mut read = [0u8; 64];
    let count = Read::read(&mut &terminal.keys, &mut read).ok()?;
    Some(read[..count].to_vec())
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

impl Asked for Terminal {
    fn winsize(&self) -> Option<CellSize> {
        tcgetwinsize(&self.screen).ok().and_then(of_winsize)
    }

    fn ask(&self, question: Question) -> Option<Vec<u8>> {
        reply(self, question.asked())
    }
}

/// The cell size a terminal's own `winsize` implies, when it carries pixels. A terminal that
/// reports no pixels, or fewer pixels than it has cells, has not said, and `None` says so.
fn of_winsize(size: Winsize) -> Option<CellSize> {
    let (cols, rows) = (size.ws_col, size.ws_row);
    let (xpixel, ypixel) = (size.ws_xpixel, size.ws_ypixel);
    if cols == 0 || rows == 0 || xpixel < cols || ypixel < rows {
        return None;
    }
    Some(CellSize::new(
        u32::from(xpixel / cols),
        u32::from(ypixel / rows),
    ))
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

        /// Every byte the terminal has been handed and nothing it says next: one read is as much as
        /// had arrived and a frame is several writes, so the reads go on until the terminal is
        /// silent — and never wait for a word it does not owe.
        pub(crate) fn drawn(&self) -> String {
            let mut drawn = Vec::new();
            while !self.silent() {
                let mut chunk = [0u8; 4096];
                let mut master = &self.master;
                let count = master.read(&mut chunk).expect("the terminal's screen");
                if count == 0 {
                    break;
                }
                drawn.extend_from_slice(&chunk[..count]);
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
        pub(crate) fn silent(&self) -> bool {
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
mod tests;
