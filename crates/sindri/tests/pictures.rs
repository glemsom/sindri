//! Nothing here has a terminal: a placement is asserted as the glyphs in a buffer ratatui's
//! headless backend hands back, and a transmission count is a reading of that buffer and not a
//! wire — no byte leaves this process.

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::composer::{Key, Outcome};
use sindri::face;
use sindri::graphics::{Ceiling, CellSize};
use sindri::mark::Cell;
use sindri::markdown;
use sindri::picture::{self, Surface};
use sindri::pictures::Pictures;
use sindri::rows::{Row, Scroll};
use sindri::view::View;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

fn workspace(width: u32, height: u32) -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("a directory to stand in");
    let inside = root.path().join("ws");
    std::fs::create_dir(&inside).expect("a workspace inside it");
    shoot(&inside.join("shot.png"), width, height);
    (root, inside)
}

fn shoot(path: &Path, width: u32, height: u32) {
    let picture = RgbaImage::from_pixel(width, height, Rgba([120, 80, 200, 255]));
    picture.save(path).expect("a picture on disk");
}

fn surface(workspace: &Path) -> Surface {
    Surface::of(Some(workspace), CellSize::new(10, 20), true)
}

#[test]
fn a_face_that_travels_in_the_binary_is_measured_from_its_own_header() {
    let places_pictures = Surface::of(None, CellSize::new(10, 20), true);

    let measured = face::measure(Ceiling::across(80), &places_pictures)
        .expect("a terminal that places pictures is given the face");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (80, 50),
        "795 by 989 pixels at a 10 by 20 cell is eighty cells by fifty, read off its header \
         with no workspace and no file to look in"
    );
    assert_eq!(
        measured.pixels(CellSize::new(10, 20)),
        (800, 1000),
        "and the pixels it is drawn at are the footprint it was measured into"
    );
    assert_eq!(
        measured.source(),
        "face.png",
        "the face is filed under a name of its own, which is what a placement is held by"
    );
}

#[test]
fn a_face_too_tall_for_its_band_gives_way_on_rows_and_never_on_shape() {
    let places_pictures = Surface::of(None, CellSize::new(10, 20), true);

    let measured = face::measure(Ceiling::new(40, 12), &places_pictures)
        .expect("a band too short for the face's own height is still a band it is drawn in");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (19, 11),
        "eighty cells by fifty fitted under forty columns and twelve rows is nineteen by eleven"
    );
    assert!(
        measured.cols() < 80,
        "the columns give way with the rows, because one scale is spent on both axes"
    );
    let (width, height) = measured.pixels(CellSize::new(10, 20));
    let shape = f64::from(width) / f64::from(height);
    assert!(
        (shape - 795.0 / 989.0).abs() < 0.1,
        "and what is drawn keeps the face's shape of {}, not the band's: {shape}",
        795.0 / 989.0
    );
}

#[test]
fn a_terminal_that_places_no_picture_is_given_no_cells_and_no_words_for_the_face() {
    let words_only = Surface::of(None, cell(), false);

    assert!(
        face::measure(Ceiling::new(40, 12), &words_only).is_none(),
        "the face is decoration, so a frame that places no picture reserves no cell for it and \
         has no caption to say it is missing"
    );
}

#[test]
fn a_picture_inside_the_workspace_is_measured_as_the_cells_it_takes() {
    let (_root, workspace) = workspace(40, 40);

    let measured = picture::measure("shot.png", Ceiling::across(80), &surface(&workspace))
        .expect("a picture inside the workspace is one the reader may point at");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (4, 2),
        "40 by 40 pixels at a 10 by 20 cell is four cells by two"
    );
    assert_eq!(
        measured.source(),
        "shot.png",
        "and it is kept by the words the answer wrote, not by the path they resolved to"
    );
    assert_eq!(
        measured.pixels(CellSize::new(10, 20)),
        (40, 40),
        "the pixels it is drawn at are the cells it takes times the cell"
    );
}

#[test]
fn a_picture_wider_than_the_band_is_measured_into_the_columns_the_band_has() {
    let (_root, workspace) = workspace(400, 200);

    let measured = picture::measure("shot.png", Ceiling::across(20), &surface(&workspace))
        .expect("a wide picture is fitted, not refused");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (20, 5),
        "40 by 10 cells fitted into 20 columns is 20 by 5, and keeps its shape"
    );
}

#[test]
fn a_drawing_inside_the_workspace_is_measured_by_the_size_it_declares() {
    let (_root, workspace) = workspace(40, 40);
    std::fs::write(
        workspace.join("plan.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
             <rect x="0" y="0" width="200" height="100" fill="#ffffff"/>
           </svg>"##,
    )
    .expect("a drawing on disk");

    let measured = picture::measure("plan.svg", Ceiling::across(80), &surface(&workspace))
        .expect("a drawing inside the workspace is a picture");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (20, 5),
        "200 by 100 pixels at a 10 by 20 cell is twenty cells by five"
    );
}

#[test]
fn a_picture_outside_the_workspace_is_refused_with_the_reason_the_caption_prints() {
    let (root, workspace) = workspace(40, 40);
    shoot(&root.path().join("outside.png"), 40, 40);
    std::os::unix::fs::symlink(root.path().join("outside.png"), workspace.join("link.png"))
        .expect("a link out of the workspace");

    for source in ["/etc/hostname", "../outside.png", "link.png"] {
        let refusal = picture::measure(source, Ceiling::across(80), &surface(&workspace))
            .expect_err("a picture outside the workspace is not drawn");

        assert!(
            refusal.message().contains("outside the workspace"),
            "{source} is refused as outside the workspace, and the caption says so: {}",
            refusal.message()
        );
    }
}

#[test]
fn a_picture_that_is_not_there_is_refused_with_the_reason_the_read_gave() {
    let (_root, workspace) = workspace(40, 40);

    let refusal = picture::measure("absent.png", Ceiling::across(80), &surface(&workspace))
        .expect_err("a picture that is not there is not drawn");

    assert!(
        refusal.message().contains("could not be read"),
        "the caption says the picture could not be read, and what was wrong: {}",
        refusal.message()
    );
}

#[test]
fn a_frame_with_no_workspace_refuses_the_picture_and_says_what_it_lacks() {
    let refusal = picture::measure("shot.png", Ceiling::across(80), &Surface::none())
        .expect_err("a frame with no workspace has nowhere to look");

    assert!(
        refusal.message().contains("no workspace"),
        "the caption says there is nowhere to look: {}",
        refusal.message()
    );
}

#[test]
fn a_picture_is_spoken_of_as_its_alternate_text_or_as_its_source() {
    assert_eq!(
        picture::words("shot.png", "a purple field"),
        "a purple field",
        "the words the answer wrote over the picture are what it is spoken of as"
    );
    assert_eq!(
        picture::words("shot.png", "  "),
        "shot.png",
        "and a picture written with no words is spoken of as its source"
    );
}

#[test]
fn a_picture_is_laid_out_as_the_rows_it_takes_with_its_words_at_the_head() {
    let (_root, workspace) = workspace(40, 40);

    let rows = markdown::rows_on(
        "before ![a purple field](shot.png) after",
        80,
        0,
        &surface(&workspace),
    );
    let at = rows
        .iter()
        .position(|row| row.picture().is_some())
        .expect("the picture is a row of the answer");

    assert_eq!(
        rows[at].glyphs(),
        "a purple field",
        "the row the picture stands on is its words, which is what a copy of it takes"
    );
    let measured = rows[at].picture().expect("the picture of that row");
    assert_eq!(
        (measured.cols(), measured.rows()),
        (4, 2),
        "and the cells of it are the cells it was measured into"
    );
    assert_eq!(
        rows[at + 1].glyphs(),
        "",
        "the second of its two rows is the picture's own and holds no words"
    );
    assert!(
        rows.iter().any(|row| row.glyphs().contains("after")),
        "and what the answer said after the picture is still in the answer: {:?}",
        rows.iter().map(Row::glyphs).collect::<Vec<_>>()
    );
    assert!(
        rows.iter().all(|row| row.width() <= 80),
        "no row of the answer is wider than the band it was laid out at"
    );
}

#[test]
fn a_picture_written_with_no_alternate_text_is_spoken_of_as_its_source() {
    let (_root, workspace) = workspace(40, 40);

    let rows = markdown::rows_on("![](shot.png)", 80, 0, &surface(&workspace));

    assert_eq!(
        rows.iter()
            .find(|row| row.picture().is_some())
            .expect("the picture is a row")
            .glyphs(),
        "shot.png"
    );
}

#[test]
fn a_terminal_that_places_no_picture_is_given_the_words_and_no_rows_for_it() {
    let (_root, workspace) = workspace(40, 40);
    let words_only = Surface::of(Some(&workspace), CellSize::new(10, 20), false);

    let rows = markdown::rows_on(
        "before ![a purple field](shot.png) after",
        80,
        0,
        &words_only,
    );

    assert!(
        rows.iter().all(|row| row.picture().is_none()),
        "nothing here claims a picture will be drawn"
    );
    assert_eq!(
        rows.iter().map(Row::glyphs).collect::<Vec<_>>().join("\n"),
        "before\n\na purple field\n\nafter",
        "the words the picture was pointed at with stand in one row of their own"
    );
}

#[test]
fn a_refused_picture_is_a_row_saying_why_it_was_not_drawn() {
    let (root, workspace) = workspace(40, 40);
    shoot(&root.path().join("outside.png"), 40, 40);

    let rows = markdown::rows_on("![a field](../outside.png)", 80, 0, &surface(&workspace));
    let said = rows.iter().map(Row::glyphs).collect::<Vec<_>>().join("\n");

    assert!(
        said.contains("a field") && said.contains("outside the workspace"),
        "the caption says what the picture was of and why it was not drawn: {said:?}"
    );
    assert!(
        rows.iter().all(|row| row.picture().is_none()),
        "and nothing claims a picture will be drawn"
    );
}

#[test]
fn a_picture_whose_words_are_wider_than_the_band_wraps_and_is_never_cut() {
    let (_root, taken) = workspace(40, 40);
    let alt = "a purple field photographed at dusk from the far side of the car park with a \
               long lens and no tripod at all";

    for draws in [true, false] {
        let on = Surface::of(Some(&taken), CellSize::new(10, 20), draws);
        let rows = markdown::rows_on(&format!("![{alt}](shot.png)"), 30, 0, &on);
        let said = rows.iter().map(Row::glyphs).collect::<Vec<_>>().join(" ");

        assert!(
            rows.iter().all(|row| row.width() <= 30),
            "every row of the block is inside the band it was laid out at: {said:?}"
        );
        assert!(
            said.contains("no tripod at all"),
            "and every word of the alt text is on the screen, draws the picture or not: \
             {said:?}"
        );
    }

    let (above, band) = workspace(40, 40);
    shoot(&above.path().join("outside.png"), 40, 40);
    let rows = markdown::rows_on("![a field](../outside.png)", 24, 0, &surface(&band));
    let said = rows.iter().map(Row::glyphs).collect::<Vec<_>>().join(" ");

    assert!(
        rows.iter().all(|row| row.width() <= 24),
        "no row of the caption runs over its band: {said:?}"
    );
    assert!(
        said.contains("outside the workspace"),
        "and the reason the caption exists to print is printed: {said:?}"
    );
}

const SCREEN: (u16, u16) = (100, 30);

fn cell() -> CellSize {
    CellSize::new(10, 20)
}

fn frame(workspace: &Path, text: &str, draws: bool) -> View {
    let mut view = View::of(
        vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(text),
        }],
        Scroll::Tail,
    );
    view.set_workspace(workspace.to_path_buf());
    view.set_cell_size(cell());
    if draws {
        view.set_pictures(Pictures::kitty(cell()));
    }
    view
}

fn painted_at(view: &mut View, size: (u16, u16)) -> Buffer {
    let (cols, rows) = size;
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    let completed = terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("the frame is painted");
    completed.buffer.clone()
}

/// The glyph a Kitty placement writes into every cell it covers: the one thing on a screen
/// that says a picture is placed there rather than written about.
const PLACEHOLDER: char = '\u{10EEEE}';

/// The first chunk of the protocol's transmission, which a terminal is told once and never
/// again for one picture: the glyph a cell holds when the placement sends the picture rather
/// than pointing at what the terminal already holds.
const TRANSMIT: &str = "_Gq=2,";

fn pictured_rows(buffer: &Buffer) -> Vec<u16> {
    let area = buffer.area;
    (area.y..area.bottom())
        .filter(|y| {
            (area.x..area.right()).any(|x| {
                buffer
                    .cell((x, *y))
                    .is_some_and(|cell| cell.symbol().contains(PLACEHOLDER))
            })
        })
        .collect()
}

/// The cells whose glyphs hold the protocol's first chunk, counted: what a buffer sends when it is
/// written out, and the count a scroll and a repaint must leave where it was.
fn transmission_headers(buffer: &Buffer) -> usize {
    let area = buffer.area;
    let mut count = 0;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buffer.cell((x, y))
                && cell.symbol().contains(TRANSMIT)
            {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn a_face_is_placed_over_the_footprint_it_was_measured_into_and_made_once_per_size() {
    let measured = face::measure(Ceiling::new(40, 12), &Surface::of(None, cell(), true))
        .expect("the face is measured with no workspace and no file on disk");
    let mut pictures = Pictures::kitty(cell());
    let block = Rect {
        x: 0,
        y: 0,
        width: measured.cols(),
        height: measured.rows(),
    };

    let mut first = Buffer::empty(Rect::new(0, 0, 60, 30));
    pictures.place(&measured, cell(), block, &mut first);
    let mut again = Buffer::empty(Rect::new(0, 0, 60, 30));
    pictures.place(&measured, cell(), block, &mut again);

    assert_eq!(
        pictured_rows(&first).len(),
        usize::from(measured.rows()),
        "the placement covers the rows of the footprint it was measured into, {} of them",
        measured.rows()
    );
    assert_eq!(
        transmission_headers(&first),
        1,
        "and the terminal is told about it once, from the first cell of the block"
    );
    assert_eq!(
        transmission_headers(&again),
        0,
        "a repaint writes the placeholders of what the terminal already holds"
    );
    assert_eq!(
        pictures.rasterises("face.png"),
        1,
        "the face is made into pixels once for the size it was measured into"
    );
}

#[test]
fn a_picture_inside_the_workspace_is_placed_over_the_rows_it_takes() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(&workspace, "before ![a purple field](shot.png) after", true);

    let painted = painted_at(&mut view, SCREEN);

    let rows = pictured_rows(&painted);
    assert_eq!(
        rows.len(),
        10,
        "40 by 10 cells of picture is ten rows of placeholders, and the whole block is on the \
         screen: {rows:?}"
    );
    assert_eq!(
        view.pictures().rasterises("shot.png"),
        1,
        "the picture is made once for the size it was measured into"
    );
    assert_eq!(
        transmission_headers(&painted),
        1,
        "and the header that sends it is on the screen once, in the first cell of its block"
    );
}

#[test]
fn a_picture_painted_a_second_time_carries_no_transmission_header() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(&workspace, "![a purple field](shot.png)", true);

    let first = painted_at(&mut view, SCREEN);
    let again = painted_at(&mut view, SCREEN);

    assert_eq!(
        transmission_headers(&first),
        1,
        "the first paint is what tells the terminal about the picture"
    );
    assert_eq!(
        transmission_headers(&again),
        0,
        "and the second paint writes the placeholders of a picture the terminal already holds"
    );
    assert_eq!(
        pictured_rows(&again).len(),
        10,
        "the picture is where it was, ten rows of placeholders: a repaint moves neither its \
         rows nor its transmission"
    );
    assert_eq!(
        view.pictures().rasterises("shot.png"),
        1,
        "nor is it made a second time at a size it was already made at"
    );
}

#[test]
fn a_picture_placed_again_after_a_scroll_carries_no_transmission_header() {
    let (_root, workspace) = workspace(400, 200);
    let measured = picture::measure("shot.png", Ceiling::across(60), &surface(&workspace))
        .expect("a picture inside the workspace");
    let mut pictures = Pictures::kitty(cell());
    let block = Rect {
        x: 0,
        y: 2,
        width: measured.cols(),
        height: measured.rows(),
    };

    let mut first = Buffer::empty(Rect::new(0, 0, 80, 30));
    pictures.place(&measured, cell(), block, &mut first);
    let mut scrolled = Buffer::empty(Rect::new(0, 0, 80, 30));
    pictures.place(&measured, cell(), Rect { y: 8, ..block }, &mut scrolled);

    assert_eq!(
        transmission_headers(&first),
        1,
        "the first placement sends the picture"
    );
    assert_eq!(
        transmission_headers(&scrolled),
        0,
        "and a scroll is a placement of what was sent, not a second sending"
    );
    assert_eq!(
        pictured_rows(&scrolled),
        (8..8 + measured.rows()).collect::<Vec<_>>(),
        "the block is drawn where the reader scrolled to"
    );
    assert_eq!(
        pictures.rasterises("shot.png"),
        1,
        "and the picture is not made again at a size it was already made at"
    );
}

#[test]
fn a_resize_rasterises_only_the_picture_the_band_goes_on_to_show() {
    let (_root, workspace) = workspace(800, 200);
    shoot(&workspace.join("old.png"), 800, 200);
    let tall = "a paragraph of an answer written out long enough to fill more rows than the \
                band has, so that the entry above the newest one is off the screen and the \
                picture in it is never measured";
    let mut view = View::of(
        vec![
            Entry::Assistant {
                turn: 1,
                text: SecretSet::default().redact(format!("![first](old.png) {}", tall.repeat(6))),
            },
            Entry::Assistant {
                turn: 2,
                text: SecretSet::default().redact("![second](shot.png) the newest answer"),
            },
        ],
        Scroll::Tail,
    );
    view.set_workspace(workspace.clone());
    view.set_cell_size(cell());
    view.set_pictures(Pictures::kitty(cell()));

    painted_at(&mut view, SCREEN);
    assert_eq!(
        view.pictures().rasterises("shot.png"),
        1,
        "the picture the band shows is made"
    );
    assert_eq!(
        view.pictures().rasterises("old.png"),
        0,
        "and the one scrolled above the band is never touched, because no row of it was laid \
         out to draw"
    );

    painted_at(&mut view, (60, SCREEN.1));
    assert_eq!(
        view.pictures().rasterises("shot.png"),
        2,
        "a narrower band fits the visible picture into fewer columns, so it is made again at \
         the size it is now drawn at"
    );
    assert_eq!(
        view.pictures().rasterises("old.png"),
        0,
        "and the picture off the screen keeps the raster it never had, because a resize is not \
         what makes a picture: the band showing it is (ADR-0008)"
    );
}

#[test]
fn a_resize_keeps_the_picture_the_band_shows_and_drops_the_size_it_has_left() {
    let (_root, workspace) = workspace(800, 200);
    let mut view = frame(&workspace, "![a purple field](shot.png)", true);

    let wide = painted_at(&mut view, SCREEN);
    assert_eq!(
        transmission_headers(&wide),
        1,
        "the picture is sent once at the size the wide band fits it into"
    );
    assert_eq!(
        view.pictures().rasterises("shot.png"),
        1,
        "and made once for that size"
    );

    let narrow = painted_at(&mut view, (60, SCREEN.1));
    assert_eq!(
        view.pictures().rasterises("shot.png"),
        2,
        "the narrower band fits the picture into fewer columns, so it is another picture"
    );
    assert_eq!(
        transmission_headers(&narrow),
        1,
        "and the terminal is told about that size once"
    );

    let again = painted_at(&mut view, (60, SCREEN.1));
    assert_eq!(
        transmission_headers(&again),
        0,
        "the size the band shows is still held: a repaint of it transmits nothing"
    );
    assert_eq!(
        view.pictures().rasterises("shot.png"),
        2,
        "and makes no second raster of it"
    );

    let back = painted_at(&mut view, SCREEN);
    assert_eq!(
        view.pictures().rasterises("shot.png"),
        3,
        "the size the band has left is held no longer, so going back to it makes the picture \
         again"
    );
    assert_eq!(
        transmission_headers(&back),
        1,
        "and sends it again, which is the price of a cache that is bounded"
    );
}

#[test]
fn a_picture_block_copied_out_of_the_band_is_its_words_and_never_its_bytes() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(&workspace, "![a purple field](shot.png)", true);
    let painted = painted_at(&mut view, SCREEN);
    let drawn = *pictured_rows(&painted)
        .first()
        .expect("the picture is on the screen");

    view.drag(Cell::new(drawn, 0), Cell::new(drawn, SCREEN.0 - 1));
    painted_at(&mut view, SCREEN);
    let taken = match view.key(Key::Copy) {
        Outcome::Copy(words) => words,
        other => panic!("Ctrl-Y is a copy, and this frame answered it {other:?}"),
    };

    assert_eq!(
        taken.trim(),
        "a purple field",
        "the marked cells hand out what the picture is of — never the bytes that drew it"
    );
    assert!(
        !taken.contains(PLACEHOLDER) && !taken.contains('\u{1b}'),
        "and nothing of the terminal's own protocol is in what leaves: {taken:?}"
    );
}

#[test]
fn a_terminal_that_speaks_no_kitty_is_given_the_words_and_an_uncorrupted_frame() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(
        &workspace,
        "before ![a purple field](shot.png) after",
        false,
    );

    assert!(!view.pictures().draws(), "this frame places no picture");
    let painted = painted_at(&mut view, SCREEN);
    let words: String = (painted.area.y..painted.area.bottom())
        .map(|y| {
            (painted.area.x..painted.area.right())
                .filter_map(|x| painted.cell((x, y)).map(|cell| cell.symbol().to_owned()))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        words.contains("a purple field"),
        "the picture's words are on the screen, and are the only thing it is said as: {words:?}"
    );
    assert_eq!(
        transmission_headers(&painted),
        0,
        "and no byte of a picture protocol is in a frame that places none"
    );
    assert!(
        pictured_rows(&painted).is_empty(),
        "nor is a cell claimed for a picture that will not be drawn"
    );
}

#[test]
fn a_terminal_that_names_itself_as_one_that_places_pictures_is_told_so() {
    for term in [
        "xterm-kitty",
        "xterm-ghostty",
        "xterm-256color-with-wezterm",
    ] {
        assert!(
            picture::speaks_kitty(Some(term), None),
            "{term} is a terminal that places pictures"
        );
    }
    assert!(
        picture::speaks_kitty(None, Some("ghostty")),
        "and a terminal that names itself by its program is one too"
    );

    for quiet in [
        (Some("xterm-256color"), None),
        (Some("screen-256color"), Some("tmux")),
        (Some("dumb"), None),
        (None, None),
    ] {
        assert!(
            !picture::speaks_kitty(quiet.0, quiet.1),
            "{quiet:?} is a terminal that would be handed a picture it cannot place"
        );
    }
}

#[test]
fn a_mermaid_fence_that_draws_is_a_block_the_diagram_is_placed_over() {
    let (_root, workspace) = workspace(40, 40);
    let source = "```mermaid\nflowchart TD\n  A-->B\n```";
    let rows = markdown::rows_on(source, 200, 0, &surface(&workspace));
    let at = rows
        .iter()
        .position(|row| row.picture().is_some())
        .expect("the diagram is the picture of its block");
    let diagram = rows[at].picture().expect("the picture of that row");

    assert_eq!(
        rows[at].glyphs(),
        "▏ mermaid",
        "the block's first row is the one it opened with, and the picture is placed over it"
    );
    assert_eq!(
        rows[at + 1].glyphs(),
        "▏ flowchart TD",
        "and the fence's own words stand under it, whole"
    );
    assert!(
        diagram.rows() > 3,
        "a flowchart is taller than the three rows its source was written on: {} rows",
        diagram.rows()
    );
    assert!(
        rows.len() >= at + usize::from(diagram.rows()),
        "so the block is as tall as the picture placed over it: {} rows of the answer, {} of \
         the picture",
        rows.len() - at,
        diagram.rows()
    );
}

#[test]
fn a_mermaid_fence_still_streaming_claims_no_cells_for_a_picture() {
    let (_root, workspace) = workspace(40, 40);

    let rows = markdown::rows_on(
        "```mermaid\nflowchart TD\n  A-->B",
        200,
        0,
        &surface(&workspace),
    );

    assert!(
        rows.iter().all(|row| row.picture().is_none()),
        "a fence still being written is the code block it opened and never a picture: {:?}",
        rows.iter().map(Row::glyphs).collect::<Vec<_>>()
    );
    assert!(
        rows.iter().any(|row| row.glyphs().contains("A-->B")),
        "and its source is drawn all the same, so the answer loses nothing that was written"
    );
}

#[test]
fn a_diagram_and_an_image_in_one_answer_are_placed_each_over_its_own_block() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(
        &workspace,
        "![a purple field](shot.png)\n\n```mermaid\nflowchart TD\n  A-->B\n```",
        true,
    );

    let painted = painted_at(&mut view, SCREEN);

    assert!(
        pictured_rows(&painted).len() >= 10,
        "both pictures are on the screen: {:?}",
        pictured_rows(&painted)
    );
    assert_eq!(
        transmission_headers(&painted),
        2,
        "and each was transmitted once, from its own first cell"
    );
}
