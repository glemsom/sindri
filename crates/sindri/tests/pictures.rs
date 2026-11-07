//! What a picture an answer points at comes to: a source resolved inside the workspace is
//! measured into the cells it takes and placed over them through the Kitty protocol, and one
//! that leaves the workspace — or is not there at all — is refused with the reason the
//! caption prints in its place (§12, #55).
//!
//! Nothing here has a terminal. A placement is asserted as the glyphs it puts in the cells of a
//! buffer ratatui's headless backend hands back — the placeholders of a picture the terminal
//! holds, and the protocol's transmission header in the cell that sends one — and a cost, a
//! raster made, is read off the counter the placement keeps, so the three rules ADR-0008 laid
//! down are numbers a test can hold rather than a reading of the code. What the header count is
//! not is a wire: no byte leaves this process, and the count says only that no cell of a frame
//! carries a transmission.

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::composer::{Key, Outcome};
use sindri::graphics::CellSize;
use sindri::mark::Cell;
use sindri::markdown;
use sindri::picture::{self, Surface};
use sindri::pictures::Pictures;
use sindri::rows::{Row, Scroll};
use sindri::view::View;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

/// A directory with a workspace inside it, and one picture in that workspace: the file a
/// run's own commands could have read, and so the file an answer may point the reader at.
/// What is outside it — the directory above — is what a refusal is about.
fn workspace(width: u32, height: u32) -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("a directory to stand in");
    let inside = root.path().join("ws");
    std::fs::create_dir(&inside).expect("a workspace inside it");
    shoot(&inside.join("shot.png"), width, height);
    (root, inside)
}

/// One picture written at `path`: a plain field of colour, because nothing here reads its
/// pixels — what a picture looks like is `graphics`' business.
fn shoot(path: &Path, width: u32, height: u32) {
    let picture = RgbaImage::from_pixel(width, height, Rgba([120, 80, 200, 255]));
    picture.save(path).expect("a picture on disk");
}

/// The surface a picture is measured on: the workspace a source is resolved inside, the cell
/// it is drawn at, and whether the terminal places what it is handed.
fn surface(workspace: &Path) -> Surface {
    Surface::of(Some(workspace), CellSize::new(10, 20), true)
}

/// §12's first picture: a source inside the workspace is found, and measured into the cells
/// the band's width leaves it. The source as it was written is what the picture is kept by,
/// so the same words in two answers are one picture (ADR-0008).
#[test]
fn a_picture_inside_the_workspace_is_measured_as_the_cells_it_takes() {
    let (_root, workspace) = workspace(40, 40);

    let measured = picture::measure("shot.png", 80, &surface(&workspace))
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

/// A source wider than the band is the ordinary case, and it is measured into the cells the
/// band has rather than refused: a block wider than its row would be clipped by the paint
/// and lost from whatever copies it (§12).
#[test]
fn a_picture_wider_than_the_band_is_measured_into_the_columns_the_band_has() {
    let (_root, workspace) = workspace(400, 200);

    let measured = picture::measure("shot.png", 20, &surface(&workspace))
        .expect("a wide picture is fitted, not refused");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (20, 5),
        "40 by 10 cells fitted into 20 columns is 20 by 5, and keeps its shape"
    );
}

/// An SVG is measured by the size it declares, with no renderer asked for it: the same
/// promise ADR-0003 makes of a mermaid fence holds for a drawing an answer points at.
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

    let measured = picture::measure("plan.svg", 80, &surface(&workspace))
        .expect("a drawing inside the workspace is a picture");

    assert_eq!(
        (measured.cols(), measured.rows()),
        (20, 5),
        "200 by 100 pixels at a 10 by 20 cell is twenty cells by five"
    );
}

/// The refusal §12 asks for by name: a picture is drawn from a file the run could read, and
/// a file outside the workspace is not one. It is said as a reason and not as a silence,
/// because a picture that vanished is the failure the caption exists for.
#[test]
fn a_picture_outside_the_workspace_is_refused_with_the_reason_the_caption_prints() {
    let (root, workspace) = workspace(40, 40);
    shoot(&root.path().join("outside.png"), 40, 40);
    std::os::unix::fs::symlink(root.path().join("outside.png"), workspace.join("link.png"))
        .expect("a link out of the workspace");

    for source in ["/etc/hostname", "../outside.png", "link.png"] {
        let refusal = picture::measure(source, 80, &surface(&workspace))
            .expect_err("a picture outside the workspace is not drawn");

        assert!(
            refusal.message().contains("outside the workspace"),
            "{source} is refused as outside the workspace, and the caption says so: {}",
            refusal.message()
        );
    }
}

/// And a source that resolves nowhere is refused with what the read said, because a mistyped
/// path and a file the run may not read are two things a reader acts on differently.
#[test]
fn a_picture_that_is_not_there_is_refused_with_the_reason_the_read_gave() {
    let (_root, workspace) = workspace(40, 40);

    let refusal = picture::measure("absent.png", 80, &surface(&workspace))
        .expect_err("a picture that is not there is not drawn");

    assert!(
        refusal.message().contains("could not be read"),
        "the caption says the picture could not be read, and what was wrong: {}",
        refusal.message()
    );
}

/// A frame nobody told where it stands draws no picture rather than one from a directory
/// nobody named: the workspace is the whole of what makes a source a file the run could
/// read (§12).
#[test]
fn a_frame_with_no_workspace_refuses_the_picture_and_says_what_it_lacks() {
    let refusal = picture::measure("shot.png", 80, &Surface::none())
        .expect_err("a frame with no workspace has nowhere to look");

    assert!(
        refusal.message().contains("no workspace"),
        "the caption says there is nowhere to look: {}",
        refusal.message()
    );
}

/// §12's block: a picture is spoken of as the words an answer wrote over it, and as its
/// source when it wrote none — a picture with no words would copy as nothing at all.
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

/// §12's block: a picture takes the cells it was measured into, and the words it is spoken of
/// as stand at the head of them — so a reader of a terminal that places the picture sees the
/// picture, and a reader of one that does not sees what it was of.
///
/// The words are laid out as prose and not one run, so an alt text longer than the band wraps
/// onto the block's own rows instead of running over its edge: a row is whole or it is not
/// written (§12).
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

/// A picture's words are its own even when the answer wrote none: the source is what it is
/// spoken of as, because a picture with no words at all would copy as nothing.
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

/// A terminal that places no picture is given the words and no rows reserved for a picture it
/// cannot draw: a block of rows nothing fills is a hole in the transcript, and a hole is not
/// the honest way to say that a picture is not drawn (§12).
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

/// And a picture that is refused says so where it would have stood, in the words the refusal
/// was measured in: a picture that vanished is the failure the caption exists for (§12).
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

/// A picture's words are held to the band like any other words: an alt text wider than it
/// wraps instead of running over the edge, and a refusal's caption — which is longer than any
/// alt text a writer would type — does the same, so a copy of the block never takes half a
/// word (§12).
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

/// The screen every frame below is painted at: wide enough for the rail beside the band, and
/// tall enough for an answer holding a picture's whole block.
const SCREEN: (u16, u16) = (100, 30);

/// The cell a launch probes once and keeps for the length of the launch (§12, ADR-0003).
fn cell() -> CellSize {
    CellSize::new(10, 20)
}

/// The frame an answer is drawn on: the workspace it stands in, the cell the launch measured,
/// and whether its terminal places a picture at all — the three facts a picture's rows are
/// measured against.
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

/// The frame `view` paints at `size`, as the buffer a terminal would be handed.
fn painted_at(view: &mut View, size: (u16, u16)) -> Buffer {
    let (cols, rows) = size;
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    let completed = terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("the frame is painted");
    completed.buffer.clone()
}

/// The glyph a Kitty placement writes into every cell it covers, and the only thing on a
/// screen that says a picture is placed there rather than written about.
const PLACEHOLDER: char = '\u{10EEEE}';

/// The first chunk of the protocol's transmission, which is what a terminal is told once and
/// never again for one picture (ADR-0008): the glyphs a cell holds when the placement is the
/// one that sends the picture rather than one that points at what the terminal already holds.
const TRANSMIT: &str = "_Gq=2,";

/// The rows of `buffer` a picture is drawn over, top to bottom.
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

/// The transmission headers `buffer` carries, counted as the cells whose glyphs hold the
/// protocol's first chunk: the bytes a terminal is sent when this buffer is written out, and
/// the count a scroll and a repaint must leave where it was (ADR-0008).
///
/// It is a reading of the painted buffer and not of a wire — nothing here has a terminal to
/// write one to. What it holds is that no cell of the frame carries a transmission, which is
/// the whole of what a frame can do about a picture the terminal already holds; it does not
/// count the bytes of one, and it does not say what a terminal does with them.
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

/// §12's picture, drawn: a source inside the workspace reaches the terminal as the Kitty
/// protocol's placeholders over the rows its block reserved, and the paint that first shows it
/// is the one that carries its transmission header.
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

/// ADR-0008's second rule as a cost: a picture already placed is never transmitted again. A
/// repaint of the same frame carries the placeholders and no transmission header, because the
/// protocol's own flag says the terminal holds what the first paint sent it.
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

/// The same rule where a scroll puts it: a placement moved down the band is the picture the
/// terminal holds, written again at another row — no header, no raster.
#[test]
fn a_picture_placed_again_after_a_scroll_carries_no_transmission_header() {
    let (_root, workspace) = workspace(400, 200);
    let measured = picture::measure("shot.png", 60, &surface(&workspace))
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

/// ADR-0008's third rule: a resize re-rasterises only what is visible. A width change drops
/// every row the cache held, so the next paint lays out the entries the band reaches and no
/// others — and a picture becomes pixels only when its block is on the screen.
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

/// §12's third rule as the cost it is: the cache holds the pictures the band shows and drops
/// the ones it has left, so dragging a width across a range does not keep a full raster per
/// size for ever. The size the band no longer shows is gone — going back to it makes the
/// picture again — while the size it does show is still held, so a repaint of it transmits
/// nothing and #55's "a scroll never re-transmits" survives the bound.
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

/// §12's copy rule: a picture's block copies as its words and never as the bytes it is drawn
/// from. `Ctrl-Y` hands out the rows the block was laid out as, so the cells a reader marked
/// give `a purple field` and no escape sequence, no placeholder and no base64.
#[test]
fn a_picture_block_copied_out_of_the_band_is_its_words_and_never_its_bytes() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(&workspace, "![a purple field](shot.png)", true);
    let painted = painted_at(&mut view, SCREEN);
    let drawn = *pictured_rows(&painted)
        .first()
        .expect("the picture is on the screen");

    view.drag(Cell::new(drawn, 0), Cell::new(drawn, SCREEN.0 - 1));
    // Painted again, because a copy reads the span the last paint drew: the mark and the
    // cells `Ctrl-Y` hands out are one value, one paint apart at the most (§12, #50).
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

/// §12's absence over refusal: a terminal that speaks no Kitty is given the words, whole, and
/// no row reserved for a picture it cannot draw. The frame is the frame a reader without
/// pictures would have had — nothing truncated, nothing half-drawn, no transmission header.
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

/// The terminal is asked what it is by its own name, once, before the first frame: a terminal
/// that speaks Kitty is handed the pictures its frames draw from, and one that does not is
/// handed none. `KITTY_WINDOW_ID` is not the question — the terminal that names itself in
/// `TERM` is the terminal that speaks the protocol.
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

/// §12's other picture, and the one the milestone is judged on: a mermaid fence that draws is
/// a diagram, and the diagram is placed over the source its block stands on — the source being
/// what the block copies as, and what a reader whose terminal places no picture is left with
/// (ADR-0003).
#[test]
fn a_mermaid_fence_that_draws_is_a_block_the_diagram_is_placed_over() {
    let (_root, workspace) = workspace(40, 40);
    // Laid out again until the diagram is placed: `mermaid::BOUND` is a stall a reader should
    // not feel, and a machine running the whole suite at once can spend it on scheduling.
    let source = "```mermaid\nflowchart TD\n  A-->B\n```";
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let rows = loop {
        let rows = markdown::rows_on(source, 200, 0, &surface(&workspace));
        if rows.iter().any(|row| row.picture().is_some()) || std::time::Instant::now() > deadline {
            break rows;
        }
    };
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

/// ADR-0008's rule, held from the other side: a fence whose closing line has not arrived is
/// not a diagram, so nothing is measured for it and no cell is claimed by a picture that does
/// not exist yet.
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

/// The two pictures a frame draws are placed and kept apart: a diagram and an image in one
/// answer are two cache entries, each drawn where its own block is, and neither is mistaken for
/// the other by the cells it takes.
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
