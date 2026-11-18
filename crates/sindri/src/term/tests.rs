//! The contracts an integration test cannot reach from outside the process: a canvas's shape
//! is a test's decision, and the guard and the panic hook both give the terminal back.

use std::panic;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use rustix::termios::{LocalModes, Winsize};

use sindri::capability::{Asked, Capabilities, Question};
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
static ONE_SCREEN: Mutex<()> = Mutex::new(());

/// A terminal that fills its `winsize`'s pixel fields is asked nothing, because the kernel's
/// own answer is the whole of what a picture needs.
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

/// The bytes a terminal at the far end of a pseudo-terminal is handed for a frame and the runs
/// it draws at more than one cell.
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

/// The ground a run judges a target from in this test: the tree the product is built in, whose
/// `GLOSSARY.md` is a file a call can name and be judged against.
fn ground() -> Ground {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the tree the product is built in");
    let boundary = Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
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
            canvas
                .draw(|frame| {
                    view.paint(frame.area(), frame.buffer_mut());
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
fn a_frame_whose_terminal_sizes_text_hands_it_the_heading_as_the_sequence_that_draws_it() {
    let pty = Pty::open();
    let terminal =
        Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).expect("a terminal to draw on");
    let area = painted_at();
    let mut canvas = canvas_at(&terminal, area).expect("a canvas over it");
    let mut view = View::of(
        crate::caps::sized(),
        Facts::default().asked(),
        Transcript::of(vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact("# A gate rule"),
        }]),
    );

    paint(&mut canvas, &mut view).expect("a painted frame");
    drop(canvas);

    let drawn = pty.drawn();
    assert!(
        drawn.contains("\x1b]66;s=2;A gate rule\x1b\\"),
        "the heading is handed the terminal as the one sequence that draws it: {drawn:?}"
    );
    assert!(
        !words(&drawn).contains("A   g a t e"),
        "and no cell of it is written as the cells the sizing leaves: {drawn:?}"
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
    // A cell under the block that no row of the frame wrote: a block is two rows of cells, and
    // the terminal draws the second of them.
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

#[test]
fn a_pixel_ground_is_placed_and_deleted_on_clear() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let pixels = image::RgbaImage::from_pixel(20, 20, image::Rgba([20, 30, 40, 255]));
    backend
        .ground(Some(&(Rect::new(2, 3, 2, 1), pixels)))
        .unwrap();
    backend.ground(None).unwrap();
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();
    assert!(wire.contains("_Ga=T"));
    assert!(wire.contains("_Ga=d"));
    assert!(wire.find("_Ga=T").unwrap() < wire.find("_Ga=d").unwrap());
}

#[test]
fn a_resized_ground_replaces_the_old_placement_and_a_moved_caret_refreshes_it() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let first = (
        Rect::new(2, 3, 2, 1),
        image::RgbaImage::from_pixel(20, 20, image::Rgba([20, 30, 40, 255])),
    );
    let moved = (
        first.0,
        image::RgbaImage::from_pixel(20, 20, image::Rgba([40, 30, 20, 255])),
    );
    let resized = (Rect::new(1, 5, 3, 2), moved.1.clone());
    backend.ground(Some(&first)).unwrap();
    backend.ground(Some(&first)).unwrap();
    backend.ground(Some(&moved)).unwrap();
    backend.ground(Some(&resized)).unwrap();
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();
    assert_eq!(wire.matches("_Ga=T").count(), 3);
    assert_eq!(wire.matches("_Ga=d").count(), 2);
    let first_delete = wire.find("_Ga=d").unwrap();
    let second_upload = wire.match_indices("_Ga=T").nth(1).unwrap().0;
    let second_delete = wire.match_indices("_Ga=d").nth(1).unwrap().0;
    let third_upload = wire.match_indices("_Ga=T").nth(2).unwrap().0;
    assert!(
        first_delete < second_upload
            && second_upload < second_delete
            && second_delete < third_upload
    );
    assert!(wire.contains("\x1b[6;2H"));
}

#[test]
fn a_whole_frame_clear_deletes_and_repaints_the_ground() {
    let mut wire = Vec::new();
    let mut backend = Backend::new(&mut wire, false);
    let ground = (
        Rect::new(2, 3, 2, 1),
        image::RgbaImage::from_pixel(20, 20, image::Rgba([20, 30, 40, 255])),
    );
    backend.ground(Some(&ground)).unwrap();
    RatatuiBackend::clear(&mut backend).unwrap();
    backend.ground(Some(&ground)).unwrap();
    drop(backend);
    let wire = String::from_utf8(wire).unwrap();
    assert_eq!(wire.matches("_Ga=T").count(), 2);
    assert_eq!(wire.matches("_Ga=d").count(), 1);
    assert!(wire.find("_Ga=d").unwrap() < wire.match_indices("_Ga=T").nth(1).unwrap().0);
}

#[test]
fn giving_the_screen_back_deletes_the_ground_before_the_shell_returns() {
    let _one_screen = lock(&ONE_SCREEN);
    let pty = Pty::open();
    let terminal = Terminal::pair(pty.slave.as_fd(), pty.slave.as_fd()).unwrap();
    let screen = Screen::take(&terminal, false, false).unwrap();
    drop(screen);
    let drawn = pty.drawn();
    assert!(drawn.find("_Ga=d,d=i,i=333").unwrap() < drawn.find("\x1b[?1049l").unwrap());
}
