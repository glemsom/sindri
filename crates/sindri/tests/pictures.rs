//! Nothing here has a terminal: a placement is asserted as the glyphs in a buffer ratatui's
//! headless backend hands back, and a transmission count is a reading of that buffer and not a
//! wire — no byte leaves this process.

mod caps; // the terminals these frames are drawn on

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::face;
use sindri::facts;
use sindri::graphics::{Ceiling, CellSize};
use sindri::ledger::{self, Boot, Ctx, Facts, Rail};
use sindri::mark::Cell;
use sindri::markdown;
use sindri::palette::{Band, Edge, Palette, channels};
use sindri::picture::{self, Surface};
use sindri::pictures::{PLACEHOLDER, Pictures};
use sindri::plane::{Plane, PlaneId, SEAM};
use sindri::rows::Row;
use sindri::view::{Transcript, View};
use sindri_core::policy::Enforcement;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::wire::Wire;

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
    Surface::of(Some(workspace), CellSize::new(10, 20), true, false)
}

/// A surface with no workspace in it, which is all a picture the binary carries is measured
/// against: the pixels in a cell, and whether this terminal places one at all.
fn carried(draws: bool) -> Surface {
    Surface::of(None, cell(), draws, false)
}

/// Whether every alpha on the boundary of `drawn` is nothing, which is where its footprint is
/// decided: a pixel left there is the plate the band used to read as.
fn boundary_is_clear(drawn: &RgbaImage) -> bool {
    let (width, height) = drawn.dimensions();
    let top_and_bottom =
        (0..width).all(|x| drawn.get_pixel(x, 0)[3] == 0 && drawn.get_pixel(x, height - 1)[3] == 0);
    let sides =
        (0..height).all(|y| drawn.get_pixel(0, y)[3] == 0 && drawn.get_pixel(width - 1, y)[3] == 0);
    top_and_bottom && sides
}

#[test]
fn a_face_that_travels_in_the_binary_is_measured_from_its_own_header() {
    let measured = face::measure(Ceiling::across(80), &carried(true))
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
    let measured = face::measure(Ceiling::new(40, 12), &carried(true))
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
fn a_decoded_picture_is_transparent_on_every_side_of_its_footprint() {
    let measured = face::measure(Ceiling::new(40, 12), &carried(true))
        .expect("a terminal that places pictures is given the face");
    let (width, height) = measured.pixels(cell());

    let face = measured
        .rasterise(cell())
        .expect("the face travels in the binary, so it decodes");

    assert_eq!(
        face.dimensions(),
        (width, height),
        "the pixels of the picture are still the footprint it was measured into"
    );
    assert!(
        boundary_is_clear(&face),
        "no pixel on the boundary of the footprint keeps any alpha"
    );
    assert!(
        face.pixels().any(|pixel| pixel[3] > 0),
        "and the picture still carries light inside that boundary"
    );
}

#[test]
fn a_picture_loses_the_plate_it_was_made_on_and_keeps_the_light_it_carries() {
    let root = tempfile::tempdir().expect("a directory to stand in");
    let shape = (400, 200);
    let mut lit = RgbaImage::from_pixel(shape.0, shape.1, Rgba([120, 80, 200, 5]));
    for y in 60..140 {
        for x in 100..300 {
            lit.put_pixel(x, y, Rgba([120, 80, 200, 200]));
        }
    }
    lit.save(root.path().join("shot.png"))
        .expect("a picture on disk");

    let measured = picture::measure("shot.png", Ceiling::across(80), &surface(root.path()))
        .expect("a picture inside the workspace is one the reader may point at");
    let drawn = measured
        .rasterise(cell())
        .expect("a png inside the workspace decodes");

    assert_eq!(
        drawn.dimensions(),
        shape,
        "a picture already at the pixels of its footprint is not scaled"
    );
    assert_eq!(
        drawn.get_pixel(200, 100)[3],
        200,
        "the light of the block is exactly what the file held, however faint the plate beside it"
    );
    assert_eq!(
        drawn.get_pixel(20, 20)[3],
        0,
        "and the plate of alpha 5 is floored away wherever it lies"
    );
    assert!(
        boundary_is_clear(&drawn),
        "so the band shows through all four sides of the footprint"
    );
}

#[test]
fn a_terminal_that_places_no_picture_is_given_no_cells_and_no_words_for_the_face() {
    assert!(
        face::measure(Ceiling::new(40, 12), &carried(false)).is_none(),
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
    let words_only = Surface::of(Some(&workspace), CellSize::new(10, 20), false, false);

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
        let on = Surface::of(Some(&taken), CellSize::new(10, 20), draws, false);
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

fn frame(caps: Capabilities, workspace: &Path, text: &str) -> View {
    View::of(
        caps,
        sindri::facts::Facts::default()
            .asked()
            .working(workspace.to_path_buf(), None),
        Transcript::of(vec![Entry::Assistant {
            turn: 1,
            text: SecretSet::default().redact(text),
        }]),
    )
}

fn painted_at(view: &mut View, size: (u16, u16)) -> Buffer {
    let (cols, rows) = size;
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    let completed = terminal
        .draw(|frame| {
            view.paint(frame.area(), frame.buffer_mut());
        })
        .expect("the frame is painted");
    completed.buffer.clone()
}

/// The first chunk of the protocol's transmission, which a terminal is told once and never
/// again for one picture: the glyph a cell holds when the placement sends the picture rather
/// than pointing at what the terminal already holds.
const TRANSMIT: &str = "_Gq=2,";

fn pictured_rows(buffer: &Buffer) -> Vec<u16> {
    let area = buffer.area;
    (area.y..area.bottom())
        .filter(|y| !pictured_cols(buffer, *y).is_empty())
        .collect()
}

/// The columns of row `y` a placement covers: the cell the protocol writes its sequence into, and
/// the cells it skips behind that one. It is the picture's whole width and no cell more.
fn pictured_cols(buffer: &Buffer, y: u16) -> Vec<u16> {
    let area = buffer.area;
    (area.x..area.right())
        .filter(|x| {
            buffer
                .cell((*x, y))
                .is_some_and(|cell| cell.skip || cell.symbol().contains(PLACEHOLDER))
        })
        .collect()
}

/// The rail's columns of row `y`, as a frame draws them.
fn rail_row(buffer: &Buffer, rail: Rect, y: u16) -> String {
    (rail.x..rail.right())
        .map(|x| {
            buffer
                .cell((x, y))
                .unwrap_or_else(|| panic!("column {x} of the rail, row {y}, is outside the frame"))
                .symbol()
                .to_owned()
        })
        .collect()
}

/// The twelve facts a rail draws, in the six blocks it reads them in, and no run's own spend: the
/// facts that stand under the face, whole.
fn railed_facts() -> Facts {
    Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet-4-5-20250929")),
        think: Some(String::from("high")),
        turns: Some((6, 250)),
        ctx: Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        cache: Some(88),
        workspace: Some(PathBuf::from(
            "/home/glemsom/Documents/git/sindri/.worktrees/46-rail",
        )),
        branch: Some(String::from("spec/1-capability")),
        loaded: vec![String::from("bash"), String::from("records")],
        sandbox: Some(Enforcement::Bubblewrap),
        session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
        temp: Some(PathBuf::from(
            "/home/glemsom/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
        )),
        ..Facts::default()
    }
}

/// The facts a launch already holds beside the splash screen: a session and a temp are a run's own,
/// so the rail draws an em dash for each.
fn boot_facts() -> facts::Facts {
    facts::Facts::boot(
        Boot {
            wire: Some(Wire::Anthropic),
            model: Some(String::from("claude-sonnet-4-5")),
            workspace: PathBuf::from("/home/glemsom/Documents/git/sindri"),
            branch: Some(String::from("spec/1-capability")),
            enforcement: Enforcement::Bubblewrap,
        },
        None,
    )
}

/// The splash screen a launch opens on, drawn on a terminal that places pictures or on one that
/// places none.
fn splash(draws: bool) -> View {
    View::of(
        match draws {
            true => caps::at(cell()),
            false => Capabilities::default(),
        },
        boot_facts(),
        Transcript::none(),
    )
}

/// A frame whose rail carries the twelve facts, and a transcript of nothing: the rail is the picture
/// a face is drawn over. A terminal that places none has no face to turn off, so `face` is no fact of
/// its frame.
fn railed(draws: bool, face: bool) -> View {
    View::of(
        match draws {
            true => caps::faced(face),
            false => Capabilities::default(),
        },
        sindri::facts::Facts::default()
            .asked()
            .railed(railed_facts()),
        Transcript::none(),
    )
}

/// The band the transcript and the rail share in a `cols` by `rows` frame: everything above the
/// two rows the composer rests at and the one the footer takes.
fn shared_band(cols: u16, rows: u16) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: cols,
        height: rows - 3,
    }
}

/// The one row a frame narrower than the ledger's floor draws its strip in: the last row of the
/// band it shares with the transcript.
fn strip_band(cols: u16, rows: u16) -> Rect {
    Rect {
        y: shared_band(cols, rows).bottom() - 1,
        height: 1,
        ..shared_band(cols, rows)
    }
}

/// The rail's own columns of that band: the ones the frame's width gives it, down its right-hand
/// edge.
fn rail_band(cols: u16, rows: u16) -> Rect {
    let columns = Rail::columns(cols);
    Rect {
        x: cols - columns,
        width: columns,
        ..shared_band(cols, rows)
    }
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
    let measured = face::measure(Ceiling::new(40, 12), &carried(true))
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
    let mut view = frame(
        caps::at(cell()),
        &workspace,
        "before ![a purple field](shot.png) after",
    );

    let painted = painted_at(&mut view, SCREEN);

    let rows = pictured_rows(&painted);
    assert_eq!(
        rows.len(),
        10,
        "40 by 10 cells of picture is ten rows of placeholders, and the whole block is on the \
         screen: {rows:?}"
    );
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
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
    let mut view = frame(caps::at(cell()), &workspace, "![a purple field](shot.png)");

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
        view.capabilities().pictures().rasterises("shot.png"),
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
        caps::at(cell()),
        sindri::facts::Facts::default()
            .asked()
            .working(workspace.clone(), None),
        Transcript::of(vec![
            Entry::Assistant {
                turn: 1,
                text: SecretSet::default().redact(format!("![first](old.png) {}", tall.repeat(6))),
            },
            Entry::Assistant {
                turn: 2,
                text: SecretSet::default().redact("![second](shot.png) the newest answer"),
            },
        ]),
    );

    painted_at(&mut view, SCREEN);
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        1,
        "the picture the band shows is made"
    );
    assert_eq!(
        view.capabilities().pictures().rasterises("old.png"),
        0,
        "and the one scrolled above the band is never touched, because no row of it was laid \
         out to draw"
    );

    painted_at(&mut view, (60, SCREEN.1));
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        2,
        "a narrower band fits the visible picture into fewer columns, so it is made again at \
         the size it is now drawn at"
    );
    assert_eq!(
        view.capabilities().pictures().rasterises("old.png"),
        0,
        "and the picture off the screen keeps the raster it never had, because a resize is not \
         what makes a picture: the band showing it is"
    );
}

#[test]
fn a_resize_keeps_the_picture_the_band_shows_and_drops_the_size_it_has_left() {
    let (_root, workspace) = workspace(800, 200);
    let mut view = frame(caps::at(cell()), &workspace, "![a purple field](shot.png)");

    let wide = painted_at(&mut view, SCREEN);
    assert_eq!(
        transmission_headers(&wide),
        1,
        "the picture is sent once at the size the wide band fits it into"
    );
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        1,
        "and made once for that size"
    );

    let narrow = painted_at(&mut view, (60, SCREEN.1));
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
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
        view.capabilities().pictures().rasterises("shot.png"),
        2,
        "and makes no second raster of it"
    );

    let back = painted_at(&mut view, SCREEN);
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
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
    let mut view = frame(caps::at(cell()), &workspace, "![a purple field](shot.png)");
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
        Capabilities::default(),
        &workspace,
        "before ![a purple field](shot.png) after",
    );

    assert!(
        !view.capabilities().pictures().draws(),
        "this frame places no picture"
    );
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
        caps::at(cell()),
        &workspace,
        "![a purple field](shot.png)\n\n```mermaid\nflowchart TD\n  A-->B\n```",
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

#[test]
fn the_face_is_placed_over_the_top_rows_of_the_rail_above_the_first_block() {
    for cols in [100, 120] {
        let mut view = railed(true, true);
        let painted = painted_at(&mut view, (cols, 40));
        let rail = rail_band(cols, 40);
        let face = face::over_rail(rail.width, &carried(true)).expect("the face a rail holds");

        assert_eq!(
            pictured_rows(&painted),
            (rail.y..rail.y + face.rows()).collect::<Vec<u16>>(),
            "the face is drawn in the {cols}-column frame's top rows, {} of them, from the rail's first \
             row",
            face.rows()
        );
        for y in rail.y..rail.y + face.rows() {
            let columns = pictured_cols(&painted, y);
            assert_eq!(
                (columns.first().copied(), columns.len()),
                (
                    Some(rail.x + (rail.width - face.cols()) / 2),
                    usize::from(face.cols())
                ),
                "row {y} of the face is {} columns centred in the rail's own columns: {columns:?}",
                face.cols()
            );
            assert!(
                !rail_row(&painted, rail, y).contains('─'),
                "row {y} of the face has a hairline drawn through it: {:?}",
                rail_row(&painted, rail, y)
            );
        }

        for y in rail.y + face.rows()..rail.bottom() {
            assert!(
                pictured_cols(&painted, y).is_empty(),
                "row {y} of the rail stands under the face and holds a cell of it: {:?}",
                pictured_cols(&painted, y)
            );
        }

        let rule = rail.y + face.rows();
        assert_eq!(
            rail_row(&painted, rail, rule).trim_end(),
            "─".repeat(usize::from(rail.width)),
            "the face is closed by the one rule the rail draws in its body"
        );
        let first = rule + 1;
        assert_eq!(
            rail_row(&painted, rail, first).trim_end(),
            "wire      anthropic",
            "and the first block stands on the row under that rule, whole"
        );
        assert_eq!(
            rail_row(&painted, rail, first + 1).trim_end(),
            format!(
                "model     {}",
                match cols {
                    100 => "claude-sonnet-4-5…",
                    _ => "claude-sonnet-4-5-20250929",
                }
            ),
            "and the block under it reads in the contract's order, under the face and not over \
             it, with the served name whole where the wider frame's rail holds it"
        );
    }
}

#[test]
fn every_fact_the_rail_drew_without_a_face_is_drawn_whole_below_one() {
    for cols in [100, 120] {
        let rail = rail_band(cols, 40);
        let mut bare = railed(false, true);
        let without = painted_at(&mut bare, (cols, 40));
        let mut faced = railed(true, true);
        let with = painted_at(&mut faced, (cols, 40));

        let stated = |buffer: &Buffer, from: u16| -> Vec<String> {
            (from..rail.bottom())
                .map(|y| rail_row(buffer, rail, y).trim_end().to_owned())
                .filter(|row| !row.is_empty())
                .collect()
        };
        // the rows the face takes, and the one under them that holds the rule closing it
        let under = u16::try_from(pictured_rows(&with).len()).expect("the band's rows") + 1;

        assert_eq!(
            stated(&with, rail.y + under),
            stated(&without, rail.y),
            "the {cols}-column frame draws the same twelve facts under the face that it drew \
             from the rail's first row: none is given up to the face"
        );
        assert!(
            !stated(&with, rail.y + under)
                .iter()
                .any(|row| row.contains("more")),
            "and none is counted as gone, so the rail grows no row saying one went: {:?}",
            stated(&with, rail.y + under)
        );
    }
}

#[test]
fn below_a_hundred_columns_no_face_is_drawn_and_the_strip_is_the_row_it_was() {
    for cols in [40, 80, 99] {
        let mut faced = railed(true, true);
        let with = painted_at(&mut faced, (cols, 24));
        let mut bare = railed(false, true);
        let without = painted_at(&mut bare, (cols, 24));

        assert!(
            pictured_rows(&with).is_empty(),
            "a {cols}-column frame draws one row of facts and no face: it has no band to reserve"
        );
        same_cells_outside_ground(&with, &without, strip_band(cols, 24), 8);
    }
}

#[test]
fn a_frame_the_face_is_turned_off_for_is_the_frame_a_terminal_that_places_no_picture_draws() {
    for cols in [100, 120] {
        let rail = rail_band(cols, 40);
        let mut turned_off = railed(true, false);
        let off = painted_at(&mut turned_off, (cols, 40));
        let mut placeless = railed(false, true);
        let placeless = painted_at(&mut placeless, (cols, 40));

        assert!(
            pictured_rows(&off).is_empty(),
            "a frame the config turned the face off for is given no cell of it"
        );
        assert_eq!(
            rail_row(&off, rail, rail.y).trim_end(),
            "wire      anthropic",
            "the facts stand from the rail's first row, and the row under them is the second \
             fact: no band is reserved for a face that is not drawn, and no rule closes one"
        );
        same_cells_outside_ground(&off, &placeless, rail, 8);

        let mut back_on = railed(true, true);
        let back_on = painted_at(&mut back_on, (cols, 40));
        let face = face::over_rail(rail.width, &carried(true)).expect("a face the rail holds");
        assert_eq!(
            pictured_rows(&back_on).len(),
            usize::from(face.rows()),
            "a frame the setting is turned back on for draws the face again, over the rows it \
             reserved before"
        );
    }
}

#[test]
fn the_rail_beside_the_splash_screen_loses_no_fact_to_the_face_it_carries() {
    let mut pictured = splash(true);
    let with = painted_at(&mut pictured, (120, 40));
    let mut placeless = splash(false);
    let without = painted_at(&mut placeless, (120, 40));
    let rail = rail_band(120, 40);
    let face = face::over_rail(rail.width, &carried(true)).expect("the face a rail holds");

    let pictured_in_rail = |y: u16| -> Vec<u16> {
        pictured_cols(&with, y)
            .into_iter()
            .filter(|x| (rail.x..rail.right()).contains(x))
            .collect()
    };
    let rows: Vec<u16> = (rail.y..rail.bottom())
        .filter(|y| !pictured_in_rail(*y).is_empty())
        .collect();
    assert_eq!(
        rows,
        (rail.y..rail.y + face.rows()).collect::<Vec<u16>>(),
        "the face is drawn in the 120-column frame's top rows, {} of them, from the rail's first \
         row",
        face.rows()
    );
    for y in rail.y..rail.y + face.rows() {
        let columns = pictured_in_rail(y);
        assert_eq!(
            (columns.first().copied(), columns.len()),
            (
                Some(rail.x + (rail.width - face.cols()) / 2),
                usize::from(face.cols())
            ),
            "row {y} of the face is {} columns centred in the rail's own columns: {columns:?}",
            face.cols()
        );
    }

    let hairline = rail.y + face.rows();
    assert_eq!(
        rail_row(&with, rail, hairline).trim_end(),
        "─".repeat(usize::from(rail.width)),
        "the face the rail carries is closed by the one rule the rail draws in its body"
    );
    let first = hairline + 1;
    assert_eq!(
        rail_row(&with, rail, first).trim_end(),
        "wire      anthropic",
        "and the first fact stands on the row under that rule, whole"
    );

    let stated = |buffer: &Buffer, from: u16| -> Vec<String> {
        (from..rail.bottom())
            .map(|y| rail_row(buffer, rail, y).trim_end().to_owned())
            .filter(|row| !row.is_empty())
            .collect()
    };
    assert_eq!(
        stated(&with, first),
        stated(&without, rail.y),
        "the splash screen's rail draws the facts a terminal that places no picture draws from \
         its first row, so the face it carries takes no fact of one"
    );
    assert!(
        !stated(&with, first).iter().any(|row| row.contains("more")),
        "and none is counted as gone, so the rail grows no row saying one went: {:?}",
        stated(&with, first)
    );
}

#[test]
fn a_splash_screen_the_face_is_turned_off_for_is_the_one_a_placeless_terminal_draws() {
    let rail = rail_band(120, 40);
    let mut turned_off = View::of(caps::faced(false), boot_facts(), Transcript::none());
    let off = painted_at(&mut turned_off, (120, 40));
    let mut placeless = splash(false);
    let without = painted_at(&mut placeless, (120, 40));

    assert!(
        pictured_rows(&off).is_empty(),
        "a splash frame the config turned the face off for is given no cell of the one the rail \
         carries"
    );
    assert_eq!(
        rail_row(&off, rail, rail.y).trim_end(),
        "wire      anthropic",
        "the facts stand from the rail's first row: no band is reserved for a face that is not \
         drawn, and no rule closes one"
    );
    same_cells_outside_ground(&off, &without, rail, 5);
}

#[test]
fn an_eighty_column_splash_screen_holds_no_face_and_neither_does_a_runs_frame() {
    let mut launching = splash(true);
    let splash = painted_at(&mut launching, (80, 24));
    let mut live = railed(true, true);
    let run = painted_at(&mut live, (80, 24));

    assert!(
        pictured_rows(&splash).is_empty(),
        "80 columns leave no rail beside the splash screen, and the frame carries no other face: \
         {:?}",
        pictured_rows(&splash)
    );
    assert!(
        pictured_rows(&run).is_empty(),
        "and a conversation's 80-column frame is the strip it was drawn as, with no face on it \
         either: {:?}",
        pictured_rows(&run)
    );
}

#[test]
fn a_terminal_that_places_no_picture_draws_the_facts_from_the_rails_first_row() {
    for cols in [100, 120] {
        let mut view = railed(false, true);
        let painted = painted_at(&mut view, (cols, 40));
        let rail = rail_band(cols, 40);

        assert!(
            pictured_rows(&painted).is_empty(),
            "a terminal that places no picture is given no cell of the face"
        );
        assert_eq!(
            rail_row(&painted, rail, rail.y).trim_end(),
            "wire      anthropic",
            "the facts stand from the rail's first row, as they did before the face was drawn"
        );
    }
}

#[test]
fn the_face_is_capped_at_twelve_rows_and_keeps_its_shape() {
    let surface = carried(true);

    for cols in [32, 40, 60, 120] {
        let measured =
            face::over_rail(cols, &surface).expect("a rail a terminal places pictures in");
        assert_eq!(
            (measured.cols(), measured.rows()),
            (19, 11),
            "a rail {cols} columns wide draws the face nineteen cells by eleven: the twelve rows \
             it may take are what binds, so a wider rail draws it no larger"
        );
    }

    let narrow = face::over_rail(16, &surface).expect("a narrow rail draws it too");
    assert_eq!(
        (narrow.cols(), narrow.rows()),
        (16, 10),
        "and a rail narrower than the face fits it into the sixteen columns it has"
    );

    for cols in [8, 16, 32, 64, 200] {
        let measured = face::over_rail(cols, &surface).expect("a rail of any width draws the face");
        assert!(
            measured.rows() <= face::CAP,
            "a {cols}-column rail draws the face in {} rows, over the twelve it is capped at",
            measured.rows()
        );
        let (width, height) = measured.pixels(cell());
        let shape = f64::from(width) / f64::from(height);
        assert!(
            (shape - 795.0 / 989.0).abs() < 0.06,
            "and what is drawn keeps the face's shape of {}, not the rail's: {shape}",
            795.0 / 989.0
        );
    }
}

#[test]
fn the_placement_covers_the_band_the_ledger_reserved_for_it_and_no_cell_outside_it() {
    for cols in [100, 120] {
        let mut view = railed(true, true);
        let painted = painted_at(&mut view, (cols, 40));
        let rail = rail_band(cols, 40);
        let face = face::over_rail(rail.width, &carried(true)).expect("the face a rail holds");
        let reserved = ledger::split(&railed_facts(), shared_band(cols, 40))
            .rail
            .expect("a frame this wide holds a ledger")
            .1
            .with_picture(face.cols(), face.rows())
            .picture
            .expect("the cells the rail reserved for the face");

        for y in rail.y..rail.bottom() {
            for x in rail.x..rail.right() {
                let covered = pictured_cols(&painted, y).contains(&x);
                assert_eq!(
                    covered,
                    reserved.contains(Position::new(x, y)),
                    "row {y}, column {x} of the {cols}-column frame is covered by the placement \
                     {covered}, and the reserved band is {reserved:?}"
                );
            }
        }
    }
}

#[test]
fn a_picture_ground_follows_the_band_and_leaves_words_intact() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    view.paste("forge");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
    let painted = view.paint(buffer.area, &mut buffer);
    let band = painted.layout().expect("a laid out frame").composer;
    let ground = composer_plane(&painted);
    assert_eq!(ground.area(), band);
    assert_eq!(
        ground.pixels().dimensions(),
        (800, u32::from(band.height) * 20)
    );
    assert!(
        (band.y..band.bottom())
            .any(|y| { (band.x..band.right()).any(|x| buffer[(x, y)].symbol() == "f") })
    );
    assert!((band.y..band.bottom()).all(|y| {
        (band.x..band.right()).all(|x| !buffer[(x, y)].symbol().contains(PLACEHOLDER))
    }));
    let top = ground.pixels().get_pixel(400, 0);
    let bottom = ground.pixels().get_pixel(400, ground.pixels().height() - 1);
    assert!(top[0] > bottom[0], "the ground is lit from above");
    assert_ne!(
        ground.pixels().get_pixel(25, 30),
        ground.pixels().get_pixel(400, 30),
        "the caret glows"
    );
}

#[test]
fn the_frame_a_launch_opens_on_stands_on_the_bands_own_pixel_ground() {
    let mut launching = splash(true);
    let area = Rect::new(0, 0, 120, 40);
    let mut cells = Buffer::empty(area);
    let painted = launching.paint(area, &mut cells);
    let band = painted.layout().expect("a laid out frame").composer;
    let ground = composer_plane(&painted);
    assert_eq!(
        ground.area(),
        band,
        "the placement covers the band's own rect"
    );
    assert_eq!(
        ground.pixels().dimensions(),
        (u32::from(band.width) * 10, u32::from(band.height) * 20),
        "at the pixels of the cell it is drawn on"
    );
    let palette = launching.capabilities().palette();
    for row in 0..band.height {
        let (x, y) = (band.x + band.width / 2, band.y + row);
        assert_eq!(
            cells[(x, y)].bg,
            palette.ground(None).bg.unwrap(),
            "row {row} of the band is claimed as the ground the pixels are, and not ramped \
             cell by cell over it"
        );
    }
}

#[test]
fn a_256_colour_palette_draws_the_bands_ground_in_its_own_greys() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::greys(), &workspace, "hello");
    let area = Rect::new(0, 0, 80, 24);
    let mut cells = Buffer::empty(area);
    let painted = view.paint(area, &mut cells);
    let palette = view.capabilities().palette();
    assert_eq!(
        palette,
        Palette::ANSI,
        "a terminal that names no twenty-four-bit colour draws the design in its greys"
    );
    let band = painted.layout().expect("a laid out frame").composer;
    let ground = composer_plane(&painted);
    let (width, height) = ground.pixels().dimensions();
    let head = channels(palette.step(0, band.height)).expect("the band's head in pixels");
    let foot = channels(palette.step(band.height - 1, band.height)).expect("its foot in pixels");
    assert_eq!(
        (head, foot),
        ([48, 48, 48], [38, 38, 38]),
        "the band this palette spells runs from the grey of the composer's tone down to the \
             rail's own"
    );
    assert_eq!(
        ground.pixels().get_pixel(width - 1, 0).0[..3],
        head,
        "the plane's top row is the head the palette's raised band rises to"
    );
    assert_ne!(
        ground.pixels().get_pixel(width - 1, 0).0[..3],
        [25, 35, 47],
        "and not the head of the design's truecolour band"
    );
    for y in 0..height {
        let pixel = ground.pixels().get_pixel(width - 1, y);
        assert_eq!(
            (pixel[0], pixel[1], pixel[2]),
            (pixel[1], pixel[1], pixel[1]),
            "the plane's last column is grey at pixel row {y}, far from the caret's glow"
        );
    }
    let bottom = ground.pixels().get_pixel(width - 1, height - 1);
    for at in 0..3 {
        assert!(
            bottom[at].abs_diff(foot[at]) < bottom[at].abs_diff(head[at]),
            "the plane's bottom row {bottom:?} is nearer the band's foot {foot:?} than its head \
             {head:?}"
        );
    }
}

#[test]
fn a_truecolour_frames_band_is_drawn_at_the_two_ends_the_palette_gives_it() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    let area = Rect::new(0, 0, 80, 24);
    let mut cells = Buffer::empty(area);
    let painted = view.paint(area, &mut cells);
    let band = painted.layout().expect("a laid out frame").composer;
    let ground = composer_plane(&painted);
    let (width, height) = ground.pixels().dimensions();
    assert_eq!(
        (width, height),
        (
            u32::from(band.width) * cell().width(),
            u32::from(band.height) * cell().height()
        ),
        "the plane covers the band's own rect, at the pixels of the cell it is drawn on"
    );
    let (head, foot) = ([25, 35, 47], [19, 27, 37]);
    let column = width - 1;
    let top = ground.pixels().get_pixel(column, 0).0;
    assert_eq!(
        top[..3],
        head,
        "the plane's top row is the head the design's raised band rises to"
    );
    let bottom = ground.pixels().get_pixel(column, height - 1).0;
    for at in 0..3 {
        assert!(
            bottom[at].abs_diff(foot[at]) <= 1,
            "the plane's bottom row {bottom:?} is the foot {foot:?} the band comes down to, to \
             the pixel the plane samples it at"
        );
    }
    let mid = ground.pixels().get_pixel(column, height / 2).0;
    for at in 0..3 {
        assert!(
            mid[at] > bottom[at] && mid[at] < top[at],
            "and half way down the plane is a pixel strictly between the two ends: {mid:?}"
        );
    }
}

#[test]
fn the_bands_glow_still_falls_off_under_the_caret() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    view.paste("forge");
    let area = Rect::new(0, 0, 80, 24);
    let mut cells = Buffer::empty(area);
    let painted = view.paint(area, &mut cells);
    let band = painted.layout().expect("a laid out frame").composer;
    let caret = (band.y..band.bottom())
        .flat_map(|y| (band.x..band.right()).map(move |x| (x, y)))
        .find(|(x, y)| cells[(*x, *y)].modifier.contains(Modifier::REVERSED))
        .expect("the caret's block stands in the band");
    let ground = composer_plane(&painted);
    let (width, height) = ground.pixels().dimensions();
    let row = height / 10;
    let at = u32::from(caret.0 - band.x) * cell().width() + cell().width() / 2;
    let lit = ground.pixels().get_pixel(at, row);
    let mut step = cell().width();
    while step < width {
        let further = ground.pixels().get_pixel((at + step).min(width - 1), row);
        for channel in 0..3 {
            assert!(
                lit[channel] >= further[channel],
                "pixel channel {channel} of the cell the caret waits in is darker than the one \
                 {step} pixels from it: {lit:?} against {further:?}"
            );
        }
        step += cell().width();
    }
    let far = ground.pixels().get_pixel(width - 1, row);
    for channel in 0..3 {
        assert!(
            lit[channel] > far[channel],
            "pixel channel {channel} of the cell the caret waits in is no lighter than the band's \
             far side: {lit:?} against {far:?}"
        );
    }
}

#[test]
fn a_rails_band_is_a_plane_of_its_own_beside_the_composers() {
    let mut view = railed(true, false);
    let area = Rect::new(0, 0, 120, 40);
    let mut cells = Buffer::empty(area);
    let painted = view.paint(area, &mut cells);
    let rail = painted
        .layout()
        .expect("a laid out frame")
        .rail
        .expect("a rail")
        .1
        .area;
    let palette = view.capabilities().palette();

    assert_eq!(
        painted
            .planes()
            .iter()
            .map(Plane::id)
            .collect::<Vec<PlaneId>>(),
        vec![PlaneId::Rail, PlaneId::Composer],
        "the frame hands back the rail's plane beside the composer's, each under its own identity"
    );
    let band = plane_of(&painted, PlaneId::Rail);
    assert_eq!(
        band.area(),
        Rect {
            x: rail.x - SEAM,
            width: rail.width + SEAM,
            ..rail
        },
        "the plane covers the rail's own rows and columns beside the {SEAM} columns of the \
         transcript the seam widens it across, so the rail comes out of the transcript's edge"
    );
    assert_eq!(
        band.pixels().dimensions(),
        (
            u32::from(band.area().width) * cell().width(),
            u32::from(rail.height) * cell().height()
        ),
        "at the pixels of the cell it is drawn on"
    );
    let (width, height) = band.pixels().dimensions();
    assert_eq!(
        band.pixels().get_pixel(0, 0).0[..3],
        channels(palette.band(Band::Well)).expect("a well in pixels"),
        "the plane's head is the well its seam opens out of, and no column of the rail stands \
         there"
    );
    assert_eq!(
        band.pixels().get_pixel(width - 1, height - 1).0[..3],
        channels(palette.edge(Edge::Shadow)).expect("a shadow in pixels"),
        "and its foot is the shadow colour, so the band falls into it in pixels rather than \
         into a hard dark row"
    );
    let middle = band.pixels().get_pixel(width - 1, height / 2);
    assert!(
        middle[0] > channels(palette.edge(Edge::Shadow)).expect("a shadow")[0]
            && middle[0] < channels(palette.band(Band::Rail)).expect("a band")[0],
        "and half way down it is a pixel between the two, and no whole cell of either: {middle:?}"
    );
    for y in rail.y..rail.bottom() {
        assert!(
            !rail_row(&cells, rail, y).contains(PLACEHOLDER),
            "row {y} of the rail's band is drawn by the plane the cells stand on, and no cell of \
             it is replaced by the placeholder glyph"
        );
    }
}

#[test]
fn a_palette_that_draws_no_colour_is_handed_no_plane_over_the_rail() {
    let mut view = View::of(
        caps::uncoloured(),
        sindri::facts::Facts::default()
            .asked()
            .railed(railed_facts()),
        Transcript::none(),
    );
    let area = Rect::new(0, 0, 120, 40);
    let mut cells = Buffer::empty(area);
    let painted = view.paint(area, &mut cells);
    assert_eq!(
        view.capabilities().palette(),
        Palette::SLOTS,
        "a terminal told NO_COLOR draws the reader's own slots"
    );
    assert!(
        painted
            .planes()
            .iter()
            .all(|plane| plane.id() != PlaneId::Rail),
        "a palette that draws no colour is handed no plane over the rail: its band is the flat \
         band it already had"
    );
}

#[test]
fn a_palette_that_draws_no_colour_is_handed_no_plane_over_the_composer() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::uncoloured(), &workspace, "hello");
    let area = Rect::new(0, 0, 80, 24);
    let mut cells = Buffer::empty(area);
    let painted = view.paint(area, &mut cells);
    let palette = view.capabilities().palette();
    assert_eq!(
        palette,
        Palette::SLOTS,
        "a terminal told NO_COLOR draws the reader's own slots"
    );
    assert!(
        painted
            .planes()
            .iter()
            .all(|plane| plane.id() != PlaneId::Composer),
        "a palette that draws no colour is handed no plane over the composer: the band drawn in \
         its place is the flat one the frame already had"
    );
    let band = painted.layout().expect("a laid out frame").composer;
    for row in 0..band.height {
        let (x, y) = (band.x + band.width / 2, band.y + row);
        assert_eq!(
            cells[(x, y)].bg,
            palette.raised(row, band.height).bg.unwrap(),
            "row {row} of the band stands on the flat ground the reader's own slots are, and on \
             no ground a plane was claimed over"
        );
    }
}

#[test]
fn a_terminal_that_places_no_picture_keeps_the_cell_ramp_and_hands_back_no_plane() {
    let (_root, workspace) = workspace(10, 20);
    let mut plain = frame(Capabilities::default(), &workspace, "hello");
    let mut pictured = frame(caps::at(cell()), &workspace, "hello");
    let area = Rect::new(0, 0, 80, 24);
    let mut plain_cells = Buffer::empty(area);
    let mut picture_cells = Buffer::empty(area);
    let plain_frame = plain.paint(area, &mut plain_cells);
    let picture_frame = pictured.paint(area, &mut picture_cells);
    assert!(
        plain_frame.planes().is_empty(),
        "a terminal that places no picture is handed no plane at all: {:?}",
        plain_frame.planes()
    );
    let band = plain_frame.layout().unwrap().composer;
    assert_eq!(band, picture_frame.layout().unwrap().composer);
    let palette = plain.capabilities().palette();
    for row in 0..band.height {
        let x = band.x + band.width / 2;
        let y = band.y + row;
        assert_eq!(
            plain_cells[(x, y)].bg,
            palette.raised(row, band.height).bg.unwrap()
        );
        assert_eq!(picture_cells[(x, y)].bg, palette.ground(None).bg.unwrap());
    }
}

/// The plane a frame hands back under `id`: a frame hands back more than one, and each is asked
/// for by the identity it is placed under.
fn plane_of(painted: &sindri::view::Painted, id: PlaneId) -> &Plane {
    painted
        .planes()
        .iter()
        .find(|plane| plane.id() == id)
        .unwrap_or_else(|| {
            panic!(
                "the frame hands back no plane under {id:?}, and a terminal that places pictures is \
             handed its planes from every paint: {:?}",
                painted
                    .planes()
                    .iter()
                    .map(Plane::id)
                    .collect::<Vec<PlaneId>>()
            )
        })
}

/// The composer's plane, which is the one plane a frame's band has always carried.
fn composer_plane(painted: &sindri::view::Painted) -> &Plane {
    plane_of(painted, PlaneId::Composer)
}

/// The two frames compared cell by cell, on a terminal that places pictures and one that places
/// none: a band the frame draws a plane under is read through its cells, so only the glyphs on it
/// are the frame's, and every cell off both bands and off the seam is the frame's whole.
fn same_cells_outside_ground(
    pictured: &Buffer,
    placeless: &Buffer,
    rail: Rect,
    composer_rows: u16,
) {
    assert_eq!(pictured.area, placeless.area);
    for y in pictured.area.y..pictured.area.bottom() {
        for x in pictured.area.x..pictured.area.right() {
            let with = &pictured[(x, y)];
            let without = &placeless[(x, y)];
            let rows = (rail.y..rail.bottom()).contains(&y);
            let on_rail = rows && (rail.x..rail.right()).contains(&x);
            let on_seam = rows && (rail.x.saturating_sub(SEAM)..rail.x).contains(&x);
            if y < pictured.area.bottom() - composer_rows - 1 && !on_rail && !on_seam {
                assert_eq!(with, without, "outside the composer at ({x}, {y})");
            } else if on_rail || on_seam {
                assert_eq!(
                    (with.symbol(), with.fg, with.modifier),
                    (without.symbol(), without.fg, without.modifier),
                    "the words beside the seam at ({x}, {y})"
                );
            } else {
                assert_eq!(
                    with.symbol(),
                    without.symbol(),
                    "composer words at ({x}, {y})"
                );
            }
        }
    }
}
