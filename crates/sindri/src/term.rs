use base64::Engine;
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::panic::{self, PanicHookInfo};
use std::sync::{Mutex, MutexGuard, Once, OnceLock};
use std::time::Duration;

use ratatui::backend::{Backend as RatatuiBackend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Rect, Size};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::termios::{
    InputModes, OptionalActions, Termios, Winsize, tcgetattr, tcgetwinsize, tcsetattr,
};
use sindri::capability::{Asked, Question};
use sindri::face::{self, Motion};
use sindri::graphics::CellSize;
use sindri::link::Links;
use sindri::picture::Picture;
use sindri::plane::{self, Placement, Plane, PlaneId, Planes, Shade};
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

pub(crate) struct Screen;

impl Screen {
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

pub(crate) type Canvas<'a> = CanvasOver<&'a File>;

pub(crate) type CanvasOver<W> = ratatui::Terminal<Backend<W>>;

const FRAME_HELD: usize = 8 * 1024;

/// `CSI ? 25 l`: the caret is given up for as long as a frame too big to be held whole is written,
/// because a terminal repaints between its writes and would show the caret in a cell that frame has
/// not placed it in.
const GIVE_UP_CARET: &[u8] = b"\x1b[?25l";

/// `CSI ? 2026 h` and `CSI ? 2026 l`: one frame's cells, grounds and placements are written
/// between them, so a Reader is shown the frame whole or not at all.
const OPEN_FRAME: &[u8] = b"\x1b[?2026h";
const CLOSE_FRAME: &[u8] = b"\x1b[?2026l";

#[derive(Default)]
struct Writing {
    bytes: Vec<u8>,
    given_up: bool,
}

struct Held<'a, W: Write> {
    screen: &'a mut W,
    writing: &'a mut Writing,
}

impl<W: Write> Held<'_, W> {
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

    fn tell(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.spill()?;
        self.screen.write_all(bytes)
    }

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

fn painting<'a, W: Write>(
    screen: &'a mut W,
    writing: &'a mut Writing,
) -> CrosstermBackend<Held<'a, W>> {
    CrosstermBackend::new(holding(screen, writing))
}

pub(crate) struct Backend<W: Write> {
    screen: CrosstermBackend<W>,
    writing: Writing,
    links: Links,
    sizes: Sizes,
    places: bool,
    shades: Vec<Standing>,
    faced: Animated,
    framed: bool,
}

/// An image the terminal holds and the cells of the frame it was last put on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Standing {
    shade: Shade,
    at: Placement,
}

impl<W: Write> Backend<W> {
    fn new(screen: W, places: bool) -> Backend<W> {
        Backend {
            screen: CrosstermBackend::new(screen),
            writing: Writing::default(),
            links: Links::none(),
            sizes: Sizes::none(),
            places,
            shades: Vec::new(),
            faced: Animated::default(),
            framed: false,
        }
    }

    fn links(&mut self, links: &Links) {
        self.links.clone_from(links);
    }

    fn sizes(&mut self, sizes: &Sizes) {
        self.sizes.clone_from(sizes);
    }

    /// Opens the frame: no cell, ground or placement of it reaches the terminal outside the
    /// bracket its own close ends.
    fn open_frame(&mut self) -> io::Result<()> {
        self.framed = true;
        holding(&mut self.screen, &mut self.writing).write_all(OPEN_FRAME)
    }

    fn shades(&mut self, shades: &[Shade], images: &Planes) -> io::Result<()> {
        let standing: Vec<Standing> = shades
            .iter()
            .filter_map(|shade| shade.place().map(|at| Standing { shade: *shade, at }))
            .collect();
        if self.shades == standing {
            return Ok(());
        }
        let before = std::mem::take(&mut self.shades);
        for held in &before {
            let gone = standing
                .iter()
                .all(|next| next.shade.image() != held.shade.image());
            if gone && let Some(id) = held.shade.id() {
                holding(&mut self.screen, &mut self.writing).tell(&plane::delete(id))?;
            }
        }
        for next in &standing {
            match before
                .iter()
                .find(|held| held.shade.image() == next.shade.image())
            {
                None => {
                    if let Some(plane) = images.image(&next.shade) {
                        self.upload(plane, next.at)?;
                    }
                }
                Some(held) if held.at != next.at => self.replace(next)?,
                Some(_) => {}
            }
        }
        self.shades = standing;
        Ok(())
    }

    /// One plane sent as the picture a terminal keeps under a band: its pixels as a PNG, in the
    /// chunks the protocol allows, under its identity, its placement and the frame's depth. `C=1`
    /// keeps a tall placement from scrolling, and `q=2` leaves no reply to read as a keystroke.
    fn upload(&mut self, plane: &Plane, at: Placement) -> io::Result<()> {
        let head = format!(
            "a=T,f=100,i={},p={},q=2,z={},C=1,c={},r={},x={},y={},w={},h={}",
            plane.id().image(),
            plane::PLACEMENT,
            plane::DEPTH,
            at.cols,
            at.rows,
            at.from,
            at.down,
            at.width,
            at.height
        );
        let mut sent = Vec::new();
        write!(sent, "\x1b7\x1b[{};{}H", at.top + 1, at.left + 1)?;
        transmit(&mut sent, &head, &encoded(plane.pixels())?)?;
        sent.extend_from_slice(b"\x1b8");
        holding(&mut self.screen, &mut self.writing).tell(&sent)
    }

    /// The image the terminal already holds, put on the cells it stands on now: no byte of it is
    /// sent again, and the placement it is put under is the one it was held by.
    fn replace(&mut self, standing: &Standing) -> io::Result<()> {
        let Some(id) = standing.shade.id() else {
            return Ok(());
        };
        let at = standing.at;
        let mut sent = Vec::new();
        write!(sent, "\x1b7\x1b[{};{}H", at.top + 1, at.left + 1)?;
        write!(
            sent,
            "\x1b_Ga=p,i={},p={},q=2,z={},C=1,c={},r={},x={},y={},w={},h={}\x1b\\",
            id.image(),
            plane::PLACEMENT,
            plane::DEPTH,
            at.cols,
            at.rows,
            at.from,
            at.down,
            at.width,
            at.height
        )?;
        sent.extend_from_slice(b"\x1b8");
        holding(&mut self.screen, &mut self.writing).tell(&sent)
    }

    fn face(&mut self, face: Option<&face::Face>) -> io::Result<()> {
        let sent = self.faced.hand(face)?;
        if sent.is_empty() {
            return Ok(());
        }
        holding(&mut self.screen, &mut self.writing).tell(&sent)
    }

    fn large<S: RatatuiBackend + Write>(
        screen: &mut S,
        links: &Links,
        laid: &Laid,
        ink: &Cell,
        places: bool,
    ) -> io::Result<()> {
        let run = laid.run();
        let mut sequence = ink.clone();
        let bytes = if run.fraction().is_none() && run.words().is_ascii() {
            osc::sized(run.scale(), run.words())
        } else {
            run.sequence()
        };
        sequence.set_symbol(&String::from_utf8_lossy(&bytes));
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

/// What the terminal holds of the face: what it was made from, where it stands, how many frames of
/// it the terminal keeps, and the motion it was last handed.
struct Facing {
    made: Made,
    area: Rect,
    frames: usize,
    motion: Motion,
}

/// The picture a face was made from: its source, the cells it takes and the cell it is drawn at, so
/// a picture of the same source at another size is another face.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Made {
    source: String,
    cols: u16,
    rows: u16,
    cell: CellSize,
}

impl Made {
    fn of(picture: &Picture, cell: CellSize) -> Made {
        Made {
            source: picture.source().to_owned(),
            cols: picture.cols(),
            rows: picture.rows(),
            cell,
        }
    }
}

#[derive(Default)]
struct Animated {
    holding: Option<Facing>,
}

impl Animated {
    /// The bytes the terminal needs to hold the face this frame asks for, and none at all where it
    /// already holds it where the frame stands.
    fn hand(&mut self, face: Option<&face::Face>) -> io::Result<Vec<u8>> {
        let Some(face) = face else {
            return Ok(self.take());
        };
        let made = Made::of(face.picture(), face.cell());
        if self
            .holding
            .as_ref()
            .is_some_and(|facing| facing.made == made)
        {
            return self.again(face);
        }
        let mut sent = self.take();
        sent.extend(self.transmit(made, face)?);
        Ok(sent)
    }

    fn again(&mut self, face: &face::Face) -> io::Result<Vec<u8>> {
        let Some(facing) = self.holding.as_mut() else {
            return Ok(Vec::new());
        };
        let mut sent = Vec::new();
        if facing.area != face.area() {
            facing.area = face.area();
            let at = face.area();
            write!(sent, "\x1b7\x1b[{};{}H", at.y + 1, at.x + 1)?;
            write!(
                sent,
                "\x1b_Ga=p,i={},p={},q=2,z={},C=1,c={},r={}\x1b\\",
                face::ID,
                plane::PLACEMENT,
                plane::DEPTH,
                at.width,
                at.height
            )?;
            sent.extend_from_slice(b"\x1b8");
        }
        if facing.motion != face.motion() {
            facing.motion = face.motion();
            sent.extend(turning(face.motion(), facing.frames)?);
        }
        Ok(sent)
    }

    /// The face's picture sent under its own identity, the frames past it composed against the
    /// root, and the loop the motion asks for read off the frames the terminal keeps.
    fn transmit(&mut self, made: Made, face: &face::Face) -> io::Result<Vec<u8>> {
        let root = face
            .picture()
            .rasterise(face.cell())
            .ok_or_else(|| io::Error::other("the face's picture could not be made into pixels"))?;
        let frames = face::frames(&root);
        let at = face.area();
        let mut sent = Vec::new();
        write!(sent, "\x1b7\x1b[{};{}H", at.y + 1, at.x + 1)?;
        let head = format!(
            "a=T,f=100,i={},p={},q=2,z={},C=1,c={},r={}",
            face::ID,
            plane::PLACEMENT,
            plane::DEPTH,
            at.width,
            at.height
        );
        transmit(&mut sent, &head, &encoded(&root)?)?;
        sent.extend_from_slice(b"\x1b8");
        let mut kept = 1;
        if matches!(face.motion(), Motion::Turning(_)) {
            for frame in &frames[1..] {
                let Some((left, top, width, height)) = differing(&root, frame) else {
                    continue;
                };
                let head = format!(
                    "a=f,f=100,i={},q=2,c=1,X=1,x={left},y={top},s={width},v={height}",
                    face::ID
                );
                let shown = image::imageops::crop_imm(frame, left, top, width, height).to_image();
                transmit(&mut sent, &head, &encoded(&shown)?)?;
                kept += 1;
            }
        }
        sent.extend(turning(face.motion(), kept)?);
        self.holding = Some(Facing {
            made,
            area: at,
            frames: kept,
            motion: face.motion(),
        });
        Ok(sent)
    }

    /// The bytes that give back whatever image the terminal holds of the face, and none where it
    /// holds none.
    fn take(&mut self) -> Vec<u8> {
        match self.holding.take() {
            Some(_) => face::delete(),
            None => Vec::new(),
        }
    }
}

/// The loop the terminal turns the face in, or the one that stops it on the frame it was given,
/// over the frames the terminal is holding.
fn turning(motion: Motion, frames: usize) -> io::Result<Vec<u8>> {
    let mut sent = Vec::new();
    match motion {
        Motion::Still => {}
        Motion::Turning(None) => {
            write!(sent, "\x1b_Ga=a,i={},s=1,c=1,q=2\x1b\\", face::ID)?;
        }
        Motion::Turning(Some(turning)) => {
            for frame in 1..=frames {
                write!(
                    sent,
                    "\x1b_Ga=a,i={},r={frame},z={},q=2\x1b\\",
                    face::ID,
                    turning.gap_ms()
                )?;
            }
            let cycles = turning.cycles().unwrap_or(1);
            write!(sent, "\x1b_Ga=a,i={},s=3,v={cycles},q=2\x1b\\", face::ID)?;
        }
    }
    Ok(sent)
}

/// The rectangle of pixels a frame differs from the root in, and none where it differs in nothing.
fn differing(root: &image::RgbaImage, frame: &image::RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let width = root.width();
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (at, (lit, dimmed)) in root.pixels().zip(frame.pixels()).enumerate() {
        if lit == dimmed {
            continue;
        }
        let at = u32::try_from(at).ok()?;
        let (x, y) = (at % width, at / width);
        bounds = Some(match bounds {
            None => (x, y, x + 1, y + 1),
            Some(bounds) => (
                bounds.0.min(x),
                bounds.1.min(y),
                bounds.2.max(x + 1),
                bounds.3.max(y + 1),
            ),
        });
    }
    bounds.map(|(left, top, right, bottom)| (left, top, right - left, bottom - top))
}

fn encoded(pixels: &image::RgbaImage) -> io::Result<String> {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(pixels.clone())
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(io::Error::other)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png))
}

/// One image, in the chunks the protocol allows: the first chunk carries the head, and each chunk
/// after it says only whether another follows.
fn transmit(sent: &mut Vec<u8>, head: &str, encoded: &str) -> io::Result<()> {
    for (index, chunk) in encoded.as_bytes().chunks(4096).enumerate() {
        let more = if (index + 1) * 4096 < encoded.len() {
            1
        } else {
            0
        };
        match index {
            0 => write!(sent, "\x1b_G{head},m={more};")?,
            _ => write!(sent, "\x1b_Gm={more};")?,
        }
        sent.extend_from_slice(chunk);
        sent.extend_from_slice(b"\x1b\\");
    }
    Ok(())
}

impl<W: Write> RatatuiBackend for Backend<W> {
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

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        holding(&mut self.screen, &mut self.writing).spill()?;
        self.screen.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        painting(&mut self.screen, &mut self.writing).set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        let giving = self.faced.take();
        if !giving.is_empty() {
            holding(&mut self.screen, &mut self.writing).tell(&giving)?;
        }
        for standing in std::mem::take(&mut self.shades) {
            if let Some(id) = standing.shade.id() {
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

    fn flush(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.framed) {
            holding(&mut self.screen, &mut self.writing).write_all(CLOSE_FRAME)?;
        }
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

pub(crate) fn paint<W: Write>(canvas: &mut CanvasOver<W>, view: &mut View) -> io::Result<Painted> {
    canvas.backend_mut().open_frame()?;
    canvas.autoresize()?;
    let area = canvas.current_buffer_mut().area;
    let painted = view.paint(area, canvas.current_buffer_mut());
    canvas.backend_mut().links(painted.links());
    canvas.backend_mut().sizes(painted.sizes());
    canvas
        .backend_mut()
        .shades(painted.shades(), view.planes())?;
    canvas.backend_mut().face(painted.face())?;
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
    /// Puts the terminal back: a frame that was interrupted closes its bracket first, then the key
    /// protocol is popped and the screen left, so nothing that follows — a panic message included —
    /// is drawn on a screen about to be somebody else's, and then raw mode and the signals return.
    fn put_back(self) {
        let mut giving = CLOSE_FRAME.to_vec();
        for id in PlaneId::ALL {
            giving.extend(plane::delete(id));
        }
        giving.extend(face::delete());
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
        crate::signal::give_signals_back();
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

type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

fn guard_panics() {
    static INSTALLED: Once = Once::new();
    static REPLACED: OnceLock<Hook> = OnceLock::new();

    INSTALLED.call_once(|| {
        let _ = REPLACED.set(panic::take_hook());
        panic::set_hook(Box::new(|raised| {
            give_back();
            if let Some(replaced) = REPLACED.get() {
                replaced(raised);
            }
        }));
    });
}

pub(crate) fn tell(out: &File, sequence: &[u8]) -> io::Result<()> {
    let mut out = out;
    out.write_all(sequence)?;
    out.flush()
}

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
        reply(self, &question.asked())
    }
}

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

#[cfg(test)]
pub(crate) fn painted_at() -> ratatui::layout::Rect {
    ratatui::layout::Rect::new(0, 0, 80, 24)
}

#[cfg(test)]
pub(crate) mod pty {

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
pub(crate) mod tests;
