use std::panic;
use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use rustix::termios::{LocalModes, Winsize};

use sindri::capability::{Answers, Asked, Capabilities, Question};
use sindri::composer::Key;
use sindri::facts::Facts;
use sindri::ink::{Ink, Look};
use sindri::link::{Link, Links};
use sindri::palette::Palette;
use sindri::pen::{self, Clip};
use sindri::size::{SCALE, Sized};
use sindri::view::Transcript;

use sindri_core::config::Config;
use sindri_core::open::Ground;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

use crate::term::pty::Pty;

use super::*;

/// The guard a taken screen is held in is one for the whole process, so the tests that take one
/// hold this too: two screens at once is one of them given back down the other's terminal.
pub(crate) static ONE_SCREEN: Mutex<()> = Mutex::new(());

#[test]
fn a_terminal_that_reports_its_pixels_is_asked_no_question() {
    let pty = Pty::open();
    pty.resize(80, 24, 800, 480);
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");

    assert_eq!(
        terminal.winsize(),
        Some(CellSize::new(10, 20)),
        "800 by 480 pixels over 80 by 24 cells is a 10 by 20 cell"
    );
    assert!(
        pty.silent(),
        "the terminal was asked nothing at all: no sequence went out"
    );
}

#[test]
fn a_winsize_is_a_cell_only_when_it_carries_pixels_for_every_cell() {
    let sized = |ws_row: u16, ws_col: u16, ws_xpixel: u16, ws_ypixel: u16| {
        of_winsize(Winsize {
            ws_row,
            ws_col,
            ws_xpixel,
            ws_ypixel,
        })
    };

    assert_eq!(
        sized(0, 0, 800, 480),
        None,
        "a terminal with no cells has no cell"
    );
    assert_eq!(
        sized(24, 80, 8, 480),
        None,
        "fewer pixels across than cells is no answer"
    );
    assert_eq!(
        sized(24, 80, 800, 20),
        None,
        "and neither is fewer pixels down"
    );
    assert_eq!(
        sized(24, 80, 800, 480),
        Some(CellSize::new(10, 20)),
        "800 by 480 pixels over 80 by 24 cells is a 10 by 20 cell"
    );
}

#[test]
fn a_terminal_that_says_nothing_is_asked_its_cell_and_answers_nothing() {
    let pty = Pty::open();
    pty.resize(80, 24, 0, 0);
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");

    assert_eq!(terminal.winsize(), None, "no pixels, so no cell is said");

    let answered = terminal.ask(Question::Cell);

    assert_eq!(
        answered, None,
        "and a terminal that answers nothing says nothing"
    );
    assert!(
        pty.drawn().contains("\u{1b}[16t"),
        "the question really was put to it"
    );
}

#[test]
fn a_terminal_that_says_nothing_answers_nothing_to_the_sizing_question() {
    let pty = Pty::open();
    pty.resize(80, 24, 0, 0);
    let terminal = Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to ask");

    let answered = terminal.ask(Question::Sizing);

    assert_eq!(answered, None, "a terminal that says nothing says nothing");
    assert!(
        pty.drawn().contains("\x1b]66;s=2; \x1b\\"),
        "the question really was put to it"
    );
}

fn composed(width: u16, height: u16, rows: &[(u16, &str)]) -> Buffer {
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
    buffer
}

fn drawn(width: u16, height: u16, rows: &[(u16, &str)], links: Links, places: bool) -> String {
    handed(composed(width, height, rows), links, places)
}

fn handed(buffer: Buffer, links: Links, places: bool) -> String {
    let area = *buffer.area();
    let pty = Pty::open();
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

fn spelled(buffer: Buffer, sizes: Sizes) -> String {
    let area = *buffer.area();
    let pty = Pty::open();
    let mut backend = Backend::new(
        pty.slave
            .try_clone()
            .expect("a second handle on the screen"),
        false,
    );
    backend.sizes(&sizes);
    backend
        .draw(Buffer::empty(area).diff(&buffer).into_iter())
        .expect("a drawn frame");
    backend.flush().expect("a written frame");
    drop(backend);
    pty.drawn()
}

fn ground() -> Ground {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the tree the product is built in");
    let boundary = Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
        .expect("the boundary holds");
    Ground::new(&boundary, workspace, workspace).expect("the ground resolves")
}

fn ways(links: &[Link]) -> Links {
    let mut ways = Links::none();
    ways.lay(0, 0, links);
    ways
}

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

fn written(pty: &Pty, launch: bool) {
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to write to");
    let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
    let mut view = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::none(),
    );
    match launch {
        true => {
            paint(&mut canvas, &mut view).expect("a painted frame");
        }
        false => {
            canvas.backend_mut().open_frame().expect("a bracket");
            canvas
                .draw(|frame| {
                    let told = view.paint(frame.area(), frame.buffer_mut());
                    if let Some(caret) = told.caret() {
                        frame.set_cursor_position(caret);
                    }
                })
                .expect("a drawn frame");
        }
    }
}

#[test]
fn the_frame_the_launch_paints_is_the_frame_the_canvas_draws() {
    let launch = Pty::open();
    let canvas = Pty::open();
    written(&launch, true);
    written(&canvas, false);

    assert_eq!(
        launch.drawn(),
        canvas.drawn(),
        "the launch paints the frame and asks the terminal for no cursor of its own: the frame \
         beside it is the frame"
    );
}

#[test]
fn the_launch_places_the_cursors_cell_after_the_frames_cells() {
    let pty = Pty::open();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to write to");
    let mut canvas = canvas_at(&terminal, painted_at()).expect("a canvas over it");
    let mut view = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::none(),
    );
    view.paste("a draft of mine");

    let painted = paint(&mut canvas, &mut view).expect("a painted frame");
    let drawn = pty.drawn();
    let caret = painted
        .caret()
        .expect("a draft takes the keys, so the frame names a cell to place the cursor in");
    // The terminal counts cells from one, and the pass counts them from none.
    let placement = format!("\u{1b}[{};{}H", caret.y + 1, caret.x + 1);

    assert!(
        drawn.contains("\u{1b}[?25h"),
        "the cursor is shown before it is placed, or the cell is one nobody sees: {drawn:?}"
    );
    assert!(
        drawn.find("\u{1b}[?25h") < drawn.rfind(&placement),
        "the show comes first, and then the move: {drawn:?}"
    );
    assert!(
        drawn.ends_with(&then_closed(&placement)),
        "the placement is the last thing written inside the frame, and the bracket closes after \
         it, so it lands on the frame just painted and never on a stale one: {drawn:?}"
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
            id: None,
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
            &mut Sizes::none(),
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
fn a_run_drawn_at_twice_the_cell_is_the_sequence_that_draws_it_and_no_cell_of_it_is_written() {
    let area = Rect::new(0, 0, 20, 2);
    let run = Sized::new(0, SCALE, "The rule", Look::plain(Ink::Plain));
    let mut buffer = Buffer::empty(area);
    pen::write(
        &mut buffer,
        Rect { height: 1, ..area },
        &[(run.spelling(), Ink::Plain)],
        Clip::Edge,
        &Palette::TRUECOLOR,
    );
    pen::write(
        &mut buffer,
        Rect {
            y: 1,
            height: 1,
            ..area
        },
        &[("nowhere", Ink::Plain)],
        Clip::Edge,
        &Palette::TRUECOLOR,
    );
    let mut sizes = Sizes::none();
    sizes.lay(0, 0, std::slice::from_ref(&run));

    let drawn = spelled(buffer, sizes);

    assert!(
        drawn.contains("\x1b]66;s=2;The rule\x1b\\"),
        "the words go out as the one sequence that draws them large: {drawn:?}"
    );
    assert!(
        !words(&drawn).contains("T h e"),
        "and no cell of the run is written as the cells it leaves: {drawn:?}"
    );
    assert!(
        !words(&drawn).contains("nowhere"),
        "a cell on the row under a block is the block's and no cell of the frame: {drawn:?}"
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
fn a_terminal_that_takes_the_window_sequences_is_asked_for_its_focus_and_gives_it_up() {
    let _one_screen = lock(&ONE_SCREEN);
    let pty = Pty::open();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");
    let screen = Screen::take(&terminal, true, false).expect("the screen is taken");

    let asked = pty.drawn();
    assert!(
        asked.contains("\x1b[?1004h"),
        "the terminal is asked to report which window is being read: {asked:?}"
    );

    drop(screen);
    let given = pty.drawn();
    assert!(
        given.contains("\x1b[?1004l"),
        "and gives the report up on the way out, or a shell is left sending reports: {given:?}"
    );
    assert!(
        given.contains("\x1b]9;4;0\x1b\\"),
        "and the mark is taken away with it, because a mark a killed run left is a mark \
             nobody can clear: {given:?}"
    );
}

#[test]
fn a_terminal_that_takes_none_of_the_window_sequences_is_asked_for_nothing() {
    let _one_screen = lock(&ONE_SCREEN);
    let pty = Pty::open();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");
    let screen = Screen::take(&terminal, false, false).expect("the screen is taken");
    let asked = pty.drawn();

    drop(screen);
    let given = pty.drawn();

    for drawn in [asked, given] {
        assert!(
            !drawn.contains("1004") && !drawn.contains("\x1b]9;"),
            "a terminal nobody can vouch for is handed no sequence but the screen's own: \
                 {drawn:?}"
        );
    }
}

#[test]
fn a_terminal_that_reports_a_pointer_is_asked_for_one_and_gives_it_up() {
    let _one_screen = lock(&ONE_SCREEN);
    let pty = Pty::open();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");
    let screen = Screen::take(&terminal, false, true).expect("the screen is taken");

    let asked = pty.drawn();
    assert!(
        asked.contains(std::str::from_utf8(TAKE_POINTER).expect("a printable sequence")),
        "the terminal reports the press and the wheel in the spelling that names a cell: \
             {asked:?}"
    );

    drop(screen);
    let given = pty.drawn();
    assert!(
        given.contains(std::str::from_utf8(GIVE_UP_POINTER).expect("a printable sequence")),
        "and gives the report up on the way out, or a shell is left sending presses: \
             {given:?}"
    );
}

#[test]
fn a_terminal_that_reports_no_pointer_is_asked_for_none_and_has_none_to_give_up() {
    let _one_screen = lock(&ONE_SCREEN);
    let pty = Pty::open();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");
    let screen = Screen::take(&terminal, true, false).expect("the screen is taken");
    let asked = pty.drawn();

    drop(screen);
    let given = pty.drawn();

    for drawn in [asked, given] {
        assert!(
            !drawn.contains("1000") && !drawn.contains("1006"),
            "a terminal nobody can vouch for is handed no pointer sequence: {drawn:?}"
        );
    }
}

#[test]
fn a_panic_gives_the_terminal_back_before_the_guard_can() {
    let _one_screen = lock(&ONE_SCREEN);
    let pty = Pty::open();
    let before = pty.attrs();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");
    let screen = Screen::take(&terminal, false, false).expect("the screen is taken");

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
        drawn.contains("\u{1b}[?25h"),
        "and the cursor is shown, so the shell gets a caret it can see: {drawn:?}"
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

#[derive(Clone, Default)]
struct Wire(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

impl Wire {
    fn drawn(&self) -> String {
        String::from_utf8(self.0.borrow().clone()).expect("a printable wire")
    }
}

impl Write for Wire {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Writes(std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>);

impl Writes {
    fn kept(&self) -> Vec<Vec<u8>> {
        self.0.borrow().clone()
    }

    fn joined(&self) -> String {
        String::from_utf8(self.0.borrow().concat()).expect("a printable wire")
    }
}

impl Write for Writes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().push(bytes.to_vec());
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const A_ROW_OF_WORDS: &str = "the frame the launch paints at the pace the pulse paints it";

const A_ROW_PAST_THE_BOUND: &str = concat!(
    "the frame the launch paints at the pace the pulse paints it, and the caret stands in the ",
    "draft the whole time the frame is written: the frame the launch paints at the pace the ",
    "pulse paints it, and the caret stands in the draft the whole time it is written",
);

fn a_full_frame(width: u16, height: u16) -> Buffer {
    let rows: Vec<(u16, &str)> = (0..height).map(|y| (y, A_ROW_OF_WORDS)).collect();
    composed(width, height, &rows)
}

fn draw<W: Write>(backend: &mut Backend<W>, frame: &Buffer) {
    backend
        .draw(Buffer::empty(*frame.area()).diff(frame).into_iter())
        .expect("a drawn frame");
    backend.flush().expect("a written frame");
}

#[test]
fn a_frame_reaches_the_terminal_in_one_write() {
    let frame = a_full_frame(80, 24);
    let writes = Writes::default();
    let mut backend = Backend::new(writes.clone(), false);
    draw(&mut backend, &frame);
    drop(backend);

    assert_eq!(
        writes.kept().len(),
        1,
        "a frame is the terminal's whole or not at all, so the caret cannot be drawn into a cell \
         the frame has not placed it in"
    );
    assert_eq!(
        writes.joined(),
        handed(frame, Links::none(), false),
        "and the bytes are the ones the frame drew, joined and no more"
    );
}

#[test]
fn the_cell_the_caret_is_placed_in_travels_in_the_one_write_the_cells_do() {
    let writes = Writes::default();
    let mut canvas = canvas_over(&writes, painted_at());
    let mut view = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::none(),
    );
    view.paste("a draft of mine");
    let mut caret = None;
    canvas
        .draw(|frame| {
            caret = view.paint(frame.area(), frame.buffer_mut()).caret();
            if let Some(at) = caret {
                frame.set_cursor_position(at);
            }
        })
        .expect("a painted frame");

    let caret =
        caret.expect("a draft takes the keys, so the frame names a cell to place the caret in");
    // The terminal counts cells from one, and the pass counts them from none.
    let placement = format!("\u{1b}[{};{}H", caret.y + 1, caret.x + 1);
    let made = writes.kept();
    assert_eq!(
        made.len(),
        1,
        "the cells, the cursor shown and the cell it is placed in are one write, so no repaint \
         finds the caret in a cell this frame has not placed it in"
    );
    let frame = String::from_utf8(made[0].clone()).expect("a printable frame");
    let shown = frame.find("\u{1b}[?25h").expect("the cursor is shown");
    let placed = frame.rfind(&placement).expect("and placed");
    assert!(
        shown < placed,
        "the show comes first, and then the move, inside the write the cells are in"
    );
    assert!(
        frame.ends_with(&placement),
        "the placement is the last thing written, so it lands on the frame just painted: {frame:?}"
    );
}

fn canvas_over(writes: &Writes, area: Rect) -> CanvasOver<Writes> {
    ratatui::Terminal::with_options(
        Backend::new(writes.clone(), false),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Fixed(area),
        },
    )
    .expect("a canvas over the wire")
}

/// The writes as the words they carry: a glyph split between two of them reads as no sequence, and
/// every sequence a frame is written with is ASCII.
fn carried(writes: &[Vec<u8>]) -> Vec<String> {
    writes
        .iter()
        .map(|write| String::from_utf8_lossy(write).into_owned())
        .collect()
}

/// What the frame's last write ends on: the caret shown and placed, and the bracket closed
/// after them.
fn then_closed(placement: &str) -> String {
    format!(
        "{placement}{}",
        std::str::from_utf8(CLOSE_FRAME).expect("a printable close")
    )
}

fn placed_in(painted: Painted) -> String {
    let caret = painted
        .caret()
        .expect("a draft takes the keys, so the frame names a cell to place the caret in");
    format!("\u{1b}[{};{}H", caret.y + 1, caret.x + 1)
}

fn gives_up_across(frame: &[String], placement: &str) {
    assert!(
        frame.len() > 1,
        "the frame spans more than one write: {} write(s)",
        frame.len()
    );
    let whole = frame.concat();
    let open = std::str::from_utf8(OPEN_FRAME).expect("a printable open");
    let given = std::str::from_utf8(GIVE_UP_CARET).expect("a printable give up");
    assert!(
        whole.starts_with(given),
        "the caret is given up before the frame's first byte: {whole:?}"
    );
    assert!(
        whole
            .strip_prefix(given)
            .unwrap_or(&whole)
            .starts_with(open),
        "and the bracket is opened before the frame's first cell, so the cells, the grounds and \
         the placements of it are written inside: {whole:?}"
    );
    let given_up: usize = frame
        .iter()
        .map(|write| write.matches("\u{1b}[?25l").count())
        .sum();
    assert_eq!(
        given_up, 1,
        "and given up once for the whole frame, never once per write: {frame:?}"
    );
    let (last, before) = frame.split_last().expect("a frame is more than no write");
    assert!(
        before.iter().all(|write| !write.contains("\u{1b}[?25h")),
        "no write the frame spans shows the caret: {before:?}"
    );
    assert!(
        last.contains("\u{1b}[?25h") && last.ends_with(&then_closed(placement)),
        "and the frame's end shows it, places it on the cell the frame names, and closes the \
         bracket after them: {last:?}"
    );
}

#[test]
fn a_frame_too_big_to_hold_gives_the_caret_up_before_its_first_byte_and_places_it_at_its_end() {
    let writes = Writes::default();
    let mut canvas = canvas_over(&writes, Rect::new(0, 0, 300, 100));
    let mut view = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::none(),
    );
    view.paste("a draft of mine");
    let placement = placed_in(paint(&mut canvas, &mut view).expect("a painted frame"));
    drop(canvas);

    gives_up_across(&carried(&writes.kept()), &placement);
}

#[test]
fn a_frame_that_fits_in_one_write_places_the_caret_and_never_gives_it_up() {
    let writes = Writes::default();
    let mut canvas = canvas_over(&writes, painted_at());
    let mut view = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::none(),
    );
    view.paste("a draft of mine");
    let placement = placed_in(paint(&mut canvas, &mut view).expect("a painted frame"));
    drop(canvas);

    let made = carried(&writes.kept());
    assert_eq!(
        made.len(),
        1,
        "a frame the buffer holds is the terminal's in one write: {} write(s)",
        made.len()
    );
    let frame = &made[0];
    assert!(
        !frame.contains("\u{1b}[?25l"),
        "and the caret is never given up where it is placed by the one write: {frame:?}"
    );
    assert!(
        frame.starts_with(std::str::from_utf8(OPEN_FRAME).expect("a printable open")),
        "the frame opens the terminal's synchronised output before its first cell: {frame:?}"
    );
    assert!(
        frame.contains("\u{1b}[?25h") && frame.ends_with(&then_closed(&placement)),
        "the show and the placement are the last things inside the frame, and the bracket closes \
         after them: {frame:?}"
    );
}

#[test]
fn a_frame_that_names_no_caret_leaves_it_hidden_however_many_writes_it_spans() {
    let writes = Writes::default();
    let mut canvas = canvas_over(&writes, Rect::new(0, 0, 160, 50));
    let mut view = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::none(),
    );
    view.key(Key::Char('?'));
    let painted = paint(&mut canvas, &mut view).expect("a painted frame");
    drop(canvas);

    assert!(
        painted.caret().is_none(),
        "the keys card draws no query row, so the frame names no cell for the caret"
    );
    let made = carried(&writes.kept());
    assert!(
        made.len() > 1,
        "and a frame this big is the terminal's in more than one write: {} write(s)",
        made.len()
    );
    assert!(
        made.iter().all(|write| !write.contains("\u{1b}[?25h")),
        "no write it spans shows the caret: {made:?}"
    );
    assert!(
        made.last().is_some_and(|last| last.contains("\u{1b}[?25l")),
        "the frame's own last bytes leave it hidden: {made:?}"
    );
}

#[test]
fn the_launchs_first_frame_and_the_frame_a_resize_paints_give_the_caret_up_across_their_writes() {
    let writes = Writes::default();
    let mut canvas = canvas_over(&writes, Rect::new(0, 0, 160, 50));
    let mut view = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::none(),
    );
    view.paste("a draft of mine");

    let launch = placed_in(paint(&mut canvas, &mut view).expect("a painted frame"));
    let launched = carried(&writes.kept()).len();
    canvas
        .resize(Rect::new(0, 0, 200, 60))
        .expect("a canvas the size the window became");
    canvas
        .backend_mut()
        .flush()
        .expect("the clear the resize writes is no frame's");
    let cleared = carried(&writes.kept()).len();
    let resized = placed_in(paint(&mut canvas, &mut view).expect("a painted frame"));
    drop(canvas);

    let made = carried(&writes.kept());
    let (first, after) = made.split_at(launched);
    let (_, resized_frame) = after.split_at(cleared - launched);
    gives_up_across(first, &launch);
    gives_up_across(resized_frame, &resized);
}

/// The bytes a terminal takes in while a frame is written to it, read off a thread of this test's
/// own: the few kilobytes a terminal nobody reads will hold stop a spill that outgrows them.
fn taken(pty: Pty, draw: impl FnOnce(&File)) -> String {
    let mut master = pty
        .master
        .try_clone()
        .expect("a second handle on the terminal");
    let read = Arc::new(Mutex::new(Vec::new()));
    let landing = Arc::clone(&read);
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(count) = master.read(&mut chunk) {
            if count == 0 {
                return;
            }
            landing
                .lock()
                .expect("the frame")
                .extend_from_slice(&chunk[..count]);
        }
    });
    draw(&pty.slave);
    let mut landed = read.lock().expect("the frame").len();
    loop {
        std::thread::sleep(Duration::from_millis(20));
        let now = read.lock().expect("the frame").len();
        if now == landed {
            return String::from_utf8_lossy(&read.lock().expect("the frame")).into_owned();
        }
        landed = now;
    }
}

#[test]
fn a_frame_past_the_bound_spills_and_every_byte_of_it_still_arrives_in_order() {
    let rows: Vec<(u16, &str)> = (0..60).map(|y| (y, A_ROW_PAST_THE_BOUND)).collect();
    let frame = composed(200, 60, &rows);
    let writes = Writes::default();
    let mut backend = Backend::new(writes.clone(), false);
    draw(&mut backend, &frame);
    drop(backend);

    let made = writes.kept();
    assert!(
        made.len() > 1,
        "a frame too big to hold is the terminal's as it is written, not a launch left with no \
         room to draw in"
    );
    assert!(
        made.iter().all(|write| write.len() <= FRAME_HELD),
        "and no write of it is past the bound either: {:?}",
        made.iter().map(Vec::len).collect::<Vec<usize>>()
    );
    assert_eq!(
        writes.joined(),
        taken(Pty::open(), |screen| {
            draw(&mut Backend::new(screen, false), &frame);
        }),
        "and every byte of it arrives, in the order it was drawn"
    );
}

#[test]
fn the_words_a_launch_says_between_two_frames_keep_the_order_they_were_said_in() {
    let frame = a_full_frame(80, 24);
    let writes = Writes::default();
    let mut backend = Backend::new(writes.clone(), false);
    draw(&mut backend, &frame);

    let mut launch = writes.clone();
    launch
        .write_all(b"\x1b[?1049h")
        .expect("the screen is taken");
    launch.flush().expect("and said");

    draw(&mut backend, &frame);
    drop(backend);

    let made = writes.kept();
    assert_eq!(
        made.len(),
        3,
        "a frame, then the words said between, then the next frame, and the frame before the \
         words is no part of them"
    );
    assert_eq!(
        made[1],
        b"\x1b[?1049h".to_vec(),
        "the words are their own write, whole"
    );
    assert_eq!(
        made[0], made[2],
        "and the frame after them is the frame that came before them"
    );
}

fn held(shades: &[Shade]) -> Planes {
    let mut images = Planes::default();
    images.hold(shades);
    images
}

fn composer_shade(area: Rect) -> Shade {
    Shade::composer(area, Some(CellSize::NOMINAL), &Palette::TRUECOLOR).expect("a band to shade")
}

fn pictured() -> Capabilities {
    let environment: std::collections::BTreeMap<String, String> =
        [("TERM", "xterm-kitty"), ("COLORTERM", "truecolor")]
            .map(|(key, value)| (String::from(key), String::from(value)))
            .into_iter()
            .collect();
    Capabilities::probed(&environment, &Answers::at(CellSize::NOMINAL), false).0
}

fn light_shade(band: Rect, caret: u16) -> Shade {
    Shade::caret(band, Some(CellSize::NOMINAL), caret, &Palette::TRUECOLOR)
        .expect("the light the caret casts")
}

fn rail_shade() -> Shade {
    Shade::rail(
        sindri::ledger::Rail::Ledger,
        sindri::ledger::RailBand::of(Rect::new(6, 3, 3, 4)),
        Some(CellSize::NOMINAL),
        &Palette::TRUECOLOR,
    )
    .expect("a band to shade")
}

fn transcript_shade() -> Shade {
    Shade::transcript(
        Rect::new(0, 0, 6, 4),
        Some(CellSize::NOMINAL),
        &Palette::TRUECOLOR,
    )
    .expect("a band to shade")
}

#[test]
fn a_plane_is_sent_once_and_the_plane_that_leaves_is_deleted_by_its_own_id() {
    let wire = Wire::default();
    let mut backend = Backend::new(wire.clone(), false);
    let shades = [
        rail_shade(),
        transcript_shade(),
        composer_shade(Rect::new(2, 3, 2, 1)),
    ];
    let images = held(&shades);
    backend.shades(&shades, &images).unwrap();
    let placed = wire.drawn().len();
    backend.shades(&shades, &images).unwrap();
    assert_eq!(
        placed,
        wire.drawn().len(),
        "a paint that changes no band writes no bytes at all"
    );
    let rest = &shades[1..];
    backend.shades(rest, &held(rest)).unwrap();
    drop(backend);
    let wire = wire.drawn();
    assert_eq!(
        wire.matches("_Ga=T").count(),
        3,
        "three planes, and each placed once: {wire:?}"
    );
    for id in [333, 334, 335] {
        assert!(
            wire.contains(&format!("_Ga=T,f=100,i={id},p=1,q=2,z={}", plane::DEPTH)),
            "a plane is uploaded under no identity and no placement of its own, and each band's \
             image is its own: {wire:?}"
        );
    }
    let deleted = wire.matches("_Ga=d").count();
    assert_eq!(
        deleted, 1,
        "the plane that left is deleted and the two that stand are not: {wire:?}"
    );
    assert!(
        wire.contains("_Ga=d,d=i,i=334"),
        "and it is deleted by the identity it was placed under, leaving the transcript band's \
         and the composer's standing: {wire:?}"
    );
}

#[test]
fn a_resized_plane_takes_the_old_one_away_and_places_the_new_one_after_it() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let first = composer_shade(Rect::new(2, 3, 2, 1));
    let resized = composer_shade(Rect::new(1, 5, 3, 2));
    for shade in [first, resized] {
        backend.shades(&[shade], &held(&[shade])).unwrap();
    }
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();
    assert_eq!(wire.matches("_Ga=T").count(), 2);
    assert_eq!(wire.matches("_Ga=d").count(), 1);
    let taken = wire.find("_Ga=d").unwrap();
    let placed = wire.match_indices("_Ga=T").nth(1).unwrap().0;
    assert!(
        taken < placed,
        "the image a resize made stale is taken away before the one that stands in its place is \
         written: {wire:?}"
    );
    assert!(wire.contains("\x1b[6;2H"));
}

#[test]
fn every_plane_is_placed_under_the_depth_a_cells_own_background_cannot_cover() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let band = Rect::new(2, 3, 30, 4);
    let shades = [
        rail_shade(),
        transcript_shade(),
        composer_shade(band),
        light_shade(band, 8),
    ];
    backend.shades(&shades, &held(&shades)).unwrap();
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();

    let depth: i64 = wire
        .split_once("z=")
        .and_then(|(_, rest)| rest.split(',').next())
        .and_then(|value| value.parse().ok())
        .expect("a depth on the ground the frame places");
    assert!(
        depth < -1_073_741_824,
        "a plane is placed below the depth at which a terminal draws an image under a cell that \
         carries a background, and the seam's edge ink is one such cell: {depth}"
    );
    for id in [333, 334, 335, 336] {
        assert!(
            wire.contains(&format!("i={id},p=1,q=2,z={},C=1", plane::DEPTH)),
            "the plane under {id} is placed at a depth a cell's background covers: {wire:?}"
        );
    }
    assert!(
        !wire.contains("z=-1,"),
        "no plane stands at the depth the seam's ink was painted over at: {wire:?}"
    );
}

#[test]
fn no_plane_the_frame_places_expects_a_reply_from_the_terminal() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let band = Rect::new(2, 3, 30, 4);
    let mut images = Planes::default();
    for caret in [8, 9] {
        let shades = [composer_shade(band), light_shade(band, caret)];
        images.hold(&shades);
        backend.shades(&shades, &images).unwrap();
    }
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();

    for command in wire.split("\u{1b}_G").skip(1) {
        let head = command.split(';').next().unwrap_or_default();
        if head.starts_with("a=d") {
            assert!(
                head.contains("q=2"),
                "an image given back asks the terminal a question, and an answer to it is read \
                 as the reader's next keystroke: {head:?}"
            );
            continue;
        }
        if !head.starts_with("a=T") && !head.starts_with("a=p") {
            continue;
        }
        assert!(
            head.contains("q=2") && head.contains("C=1"),
            "a placement asks the terminal a question, and an answer to it is read as the \
             reader's next keystroke: {head:?}"
        );
    }
    assert!(
        wire.contains("_Ga=p"),
        "and a placement is what the frame makes of a caret that moved: {wire:?}"
    );
}

#[test]
fn the_carets_light_is_given_back_when_the_band_under_it_loses_its_caret() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let band = Rect::new(2, 3, 30, 4);
    let lit = [composer_shade(band), light_shade(band, 8)];
    backend.shades(&lit, &held(&lit)).unwrap();
    let bare = [composer_shade(band)];
    backend.shades(&bare, &held(&bare)).unwrap();
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();

    assert!(
        wire.contains(&format!("_Ga=d,d=i,i={}", PlaneId::Caret.image())),
        "the light is given back when the band under it carries no caret, or the terminal is left \
         holding a light that stands nowhere: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=T").count(),
        2,
        "and neither the band nor the light is made again for it: {wire:?}"
    );
}

#[test]
fn a_frame_is_written_inside_one_synchronised_output_bracket() {
    let writes = Writes::default();
    let mut canvas = canvas_over(&writes, painted_at());
    let mut view = View::of(pictured(), Facts::default(), Transcript::none());
    view.paste("a draft of mine");
    paint(&mut canvas, &mut view).expect("a painted frame");
    drop(canvas);

    let whole = carried(&writes.kept()).concat();
    let open = std::str::from_utf8(OPEN_FRAME).expect("a printable open");
    let close = std::str::from_utf8(CLOSE_FRAME).expect("a printable close");
    assert_eq!(
        (whole.matches(open).count(), whole.matches(close).count()),
        (1, 1),
        "one frame is one bracket: {whole:?}"
    );
    let opened = whole.find(open).expect("the bracket opens");
    let closed = whole.find(close).expect("and closes");
    assert!(opened < closed, "in that order: {whole:?}");
    let inside = &whole[opened..closed];
    assert!(
        inside.contains("_Ga=T"),
        "the grounds the frame places are written inside the bracket, so no reader sees an old \
         ground under new cells: {whole:?}"
    );
    assert!(
        inside.contains("\u{1b}[38;2;"),
        "and so are the cells it draws over them: {whole:?}"
    );
    assert!(
        !whole[closed..].contains("_Ga="),
        "and nothing of the frame is written after the bracket closes: {whole:?}"
    );
}

#[test]
fn a_frame_whose_caret_moved_transmits_no_image_and_places_the_light_again() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let band = Rect::new(2, 3, 30, 4);
    let mut images = Planes::default();
    for caret in [6, 7, 8] {
        let shades = [composer_shade(band), light_shade(band, caret)];
        images.hold(&shades);
        backend.shades(&shades, &images).unwrap();
    }
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();
    assert_eq!(
        wire.matches("_Ga=T").count(),
        2,
        "the band and the light are transmitted once each, and a caret that moved twelve pixels \
         sends no pixel of either: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=p").count(),
        2,
        "and every caret after the first puts the image the terminal holds on the cells it stands \
         on now: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=d").count(),
        0,
        "nothing the frame still holds is taken away for a light that moved: {wire:?}"
    );
    assert!(
        wire.contains(&format!(
            "_Ga=p,i={},p={},q=2,z={},C=1",
            PlaneId::Caret.image(),
            plane::PLACEMENT,
            plane::DEPTH
        )),
        "the light is placed again under the identity and the placement it was held by, and the \
         command is quiet: {wire:?}"
    );
}

#[test]
fn a_band_on_another_cell_or_in_another_palette_is_placed_again_over_the_same_area() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let area = Rect::new(2, 3, 2, 1);
    let first = composer_shade(area);
    let finer = Shade::composer(area, Some(CellSize::new(20, 40)), &Palette::TRUECOLOR)
        .expect("a band to shade");
    let greys =
        Shade::composer(area, Some(CellSize::NOMINAL), &Palette::ANSI).expect("a band to shade");
    assert_eq!(
        (finer.area(), greys.area()),
        (first.area(), first.area()),
        "the band's area stands where it did, and only the cell and the palette moved"
    );
    for shade in [first, finer, greys] {
        backend.shades(&[shade], &held(&[shade])).unwrap();
    }
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();
    assert_eq!(
        wire.matches("_Ga=T").count(),
        3,
        "a band drawn on another cell or in another palette is placed again: {wire:?}"
    );
    assert!(
        wire.contains(&format!(
            "_Ga=T,f=100,i=333,p=1,q=2,z={},C=1,c=2,r=1,x=0,w=20",
            plane::DEPTH
        )),
        "at the area the band stands on, which never moved: {wire:?}"
    );
}

#[test]
fn a_rail_whose_light_moved_is_placed_again_by_its_own_identity() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let area = Rect::new(6, 3, 3, 6);
    let rail = |face: u16| {
        Shade::rail(
            sindri::ledger::Rail::Ledger,
            sindri::ledger::RailBand::of(area).with_picture(3, face),
            Some(CellSize::NOMINAL),
            &Palette::TRUECOLOR,
        )
        .expect("a band to shade")
    };
    for shade in [rail(2), rail(3)] {
        backend.shades(&[shade], &held(&[shade])).unwrap();
    }
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();
    assert_eq!(
        wire.matches("_Ga=T").count(),
        2,
        "the light falls on the rows the face stands on, so a rail whose face moved is placed \
         once more: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=d").count(),
        1,
        "and the image it stood on is taken away rather than left under it: {wire:?}"
    );
    assert!(
        wire.contains("_Ga=d,d=i,i=334")
            && wire.contains(&format!("_Ga=T,f=100,i=334,p=1,q=2,z={}", plane::DEPTH)),
        "both the delete and the new placement are under the rail's own identity: {wire:?}"
    );
}

#[test]
fn a_whole_frame_clear_drops_every_plane_before_it_clears_the_screen() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let shades = [
        rail_shade(),
        transcript_shade(),
        composer_shade(Rect::new(2, 3, 2, 1)),
        light_shade(Rect::new(2, 3, 2, 1), 2),
    ];
    let images = held(&shades);
    backend.shades(&shades, &images).unwrap();
    RatatuiBackend::clear(&mut backend).unwrap();
    backend.shades(&shades, &images).unwrap();
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();
    assert_eq!(
        wire.matches("_Ga=T").count(),
        8,
        "four planes, placed twice"
    );
    assert_eq!(
        wire.matches("_Ga=d").count(),
        4,
        "a frame that clears drops every plane it placed: {wire:?}"
    );
    let dropped = wire.rfind("_Ga=d").unwrap();
    for id in [333, 334, 335, 336] {
        assert!(
            wire.contains(&format!("_Ga=d,d=i,i={id}")),
            "the plane under {id} is not dropped by its own identity: {wire:?}"
        );
    }
    assert!(
        dropped < wire.match_indices("_Ga=T").nth(4).unwrap().0,
        "and the frame after it places them again"
    );
}

#[test]
fn giving_the_screen_back_deletes_every_plane_before_the_shell_returns() {
    let _one_screen = lock(&ONE_SCREEN);
    let pty = Pty::open();
    let terminal = Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).unwrap();
    let screen = Screen::take(&terminal, false, false).unwrap();
    drop(screen);
    let drawn = pty.drawn();
    let screen_back = drawn.find("\x1b[?1049l").expect("the screen is given back");
    for id in [333, 334, 335, 336] {
        let deleted = drawn
            .find(&format!("_Ga=d,d=i,i={id}"))
            .unwrap_or_else(|| panic!("the plane under {id} is deleted: {drawn:?}"));
        assert!(
            deleted < screen_back,
            "a plane the launch placed is deleted before the alternate screen is given back: \
             {drawn:?}"
        );
    }
}
