mod caps;
mod frame;

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::face;
use sindri::facts;
use sindri::graphics::{Ceiling, CellSize};
use sindri::ledger::{Boot, Ctx, Facts, Rail};
use sindri::mark::Cell;
use sindri::markdown;
use sindri::palette::{Band, Palette, channels};
use sindri::picture::{self, Surface};
use sindri::pictures::{PLACEHOLDER, Pictures};
use sindri::plane::{Plane, PlaneId, Shade};
use sindri::pulse::Pulse;
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

fn carried(draws: bool) -> Surface {
    Surface::of(None, cell(), draws, false)
}

fn boundary_is_clear(drawn: &RgbaImage) -> bool {
    let (width, height) = drawn.dimensions();
    let top_and_bottom =
        (0..width).all(|x| drawn.get_pixel(x, 0)[3] == 0 && drawn.get_pixel(x, height - 1)[3] == 0);
    let sides =
        (0..height).all(|y| drawn.get_pixel(0, y)[3] == 0 && drawn.get_pixel(width - 1, y)[3] == 0);
    top_and_bottom && sides
}

const HEAD: u8 = 64;

fn head(drawn: &RgbaImage) -> Option<Rect> {
    let (width, height) = drawn.dimensions();
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..height {
        for x in 0..width {
            if drawn.get_pixel(x, y)[3] < HEAD {
                continue;
            }
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((left, top, right, bottom)) => {
                    (left.min(x), top.min(y), right.max(x), bottom.max(y))
                }
            });
        }
    }
    bounds.map(|(left, top, right, bottom)| {
        Rect::new(
            left as u16,
            top as u16,
            (right - left + 1) as u16,
            (bottom - top + 1) as u16,
        )
    })
}

fn field(pixel: &Rgba<u8>) -> bool {
    pixel[2] >= 250 && pixel[0] <= 64 && pixel[1] <= 64
}

fn margins_clear(drawn: &RgbaImage, head: Rect, cell: CellSize) -> bool {
    let (width, height) = drawn.dimensions();
    let across = cell.width() as u16;
    let down = cell.height() as u16;
    let grown = Rect::new(
        head.x.saturating_sub(across),
        head.y.saturating_sub(down),
        head.width.saturating_add(across * 2),
        head.height.saturating_add(down * 2),
    );
    (0..width).all(|x| {
        (0..height).all(|y| {
            grown.contains(Position::new(x as u16, y as u16)) || drawn.get_pixel(x, y)[3] == 0
        })
    })
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
fn a_face_cut_for_the_rail_carries_no_field_under_the_head() {
    let shipped = image::load_from_memory(include_bytes!("../face.png"))
        .expect("the face travels in the binary, so it decodes")
        .to_rgba8();

    assert_eq!(
        shipped.dimensions(),
        (795, 989),
        "the face keeps the extent it was cut at, so the rail reserves the footprint it reserved \
         before and the face draws at the size it drew at"
    );
    let head = head(&shipped).expect("the picture still keeps light inside the head");

    for (x, y, pixel) in shipped.enumerate_pixels() {
        assert!(
            pixel[3] == 0 || !field(pixel),
            "the pixel at column {x} of row {y} is the flat blue the picture was laid on, and it \
             keeps alpha {} of it: made into the pixels of its footprint, the whole block would \
             read a shade bluer than the band around it",
            pixel[3]
        );
    }
    assert!(
        margins_clear(&shipped, head, cell()),
        "the rows above the head and the columns beside it carry light, and the band's plane is \
         what stands there"
    );
}

#[test]
fn a_face_made_into_the_footprint_the_rail_gives_it_carries_its_light_in_the_head_alone() {
    let measured = face::over_rail(Rail::columns(SCREEN.0), &carried(true))
        .expect("a terminal that places pictures is given the face");
    let drawn = measured
        .rasterise(cell())
        .expect("the face travels in the binary, so it decodes");
    let (cols, rows) = (measured.cols(), measured.rows());
    let head = head(&drawn).expect("the picture still keeps light inside the head");

    assert!(
        margins_clear(&drawn, head, cell()),
        "the face drawn over {cols} by {rows} cells carries light more than a cell outside the \
         head, in the margin the field filled: the band's plane is what the frame has there"
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

const TRANSMIT: &str = "_Gq=2,";

fn pictured_rows(buffer: &Buffer) -> Vec<u16> {
    let area = buffer.area;
    (area.y..area.bottom())
        .filter(|y| !pictured_cols(buffer, *y).is_empty())
        .collect()
}

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
        loaded: ["bash", "records"].into_iter().collect(),
        sandbox: Some(Enforcement::Bubblewrap),
        session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
        temp: Some(PathBuf::from(
            "/home/glemsom/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
        )),
        ..Facts::default()
    }
}

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

fn shared_band(cols: u16, rows: u16) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: cols,
        height: rows - 3,
    }
}

fn strip_band(cols: u16, rows: u16) -> Rect {
    Rect {
        y: shared_band(cols, rows).bottom() - 1,
        height: 1,
        ..shared_band(cols, rows)
    }
}

fn transcript_band(framed: &frame::Frame) -> Rect {
    framed.told().layout().expect("a laid out frame").transcript
}

fn rail_band(framed: &frame::Frame) -> Rect {
    framed
        .told()
        .layout()
        .expect("a laid out frame")
        .rail
        .expect("a rail on a frame that knows a fact")
        .1
        .area
}

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

fn carried_face() -> (picture::Picture, Rect) {
    let measured = face::measure(Ceiling::new(40, 12), &carried(true))
        .expect("the face is measured with no workspace and no file on disk");
    let block = Rect {
        x: 0,
        y: 0,
        width: measured.cols(),
        height: measured.rows(),
    };
    (measured, block)
}

#[test]
fn a_cache_moved_to_another_cell_makes_the_picture_again_and_sends_it_again() {
    let moved_to = CellSize::new(20, 40);
    let (measured, block) = carried_face();
    let mut pictures = Pictures::kitty(cell());
    let mut first = Buffer::empty(Rect::new(0, 0, 60, 30));
    pictures.place(&measured, cell(), block, &mut first);

    let mut moved = pictures.celled(moved_to);
    let mut again = Buffer::empty(Rect::new(0, 0, 60, 30));
    moved.place(&measured, moved_to, block, &mut again);

    assert_eq!(
        moved.cell(),
        Some(moved_to),
        "the cache is measured at the cell the window moved to"
    );
    assert_eq!(
        moved.rasterises("face.png"),
        2,
        "the picture is made into pixels at the new cell, over the raster already counted"
    );
    assert_eq!(
        transmission_headers(&again),
        1,
        "and the terminal, holding the old raster, is sent the new one"
    );
}

#[test]
fn a_cache_moved_to_the_cell_it_already_is_makes_nothing_again_and_sends_nothing() {
    let (measured, block) = carried_face();
    let mut pictures = Pictures::kitty(cell());
    let mut first = Buffer::empty(Rect::new(0, 0, 60, 30));
    pictures.place(&measured, cell(), block, &mut first);

    let mut kept = pictures.celled(cell());
    let mut again = Buffer::empty(Rect::new(0, 0, 60, 30));
    kept.place(&measured, cell(), block, &mut again);

    assert_eq!(
        transmission_headers(&again),
        0,
        "a cell that did not move writes the placeholders of what the terminal holds"
    );
    assert_eq!(
        kept.rasterises("face.png"),
        1,
        "and makes no second raster of a picture already made at this cell"
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

    let painted = frame::painted(&mut view, SCREEN.0, SCREEN.1);

    let rows = pictured_rows(painted.cells());
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
        transmission_headers(painted.cells()),
        1,
        "and the header that sends it is on the screen once, in the first cell of its block"
    );
}

#[test]
fn a_picture_painted_a_second_time_carries_no_transmission_header() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(caps::at(cell()), &workspace, "![a purple field](shot.png)");

    let first = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let again = frame::painted(&mut view, SCREEN.0, SCREEN.1);

    assert_eq!(
        transmission_headers(first.cells()),
        1,
        "the first paint is what tells the terminal about the picture"
    );
    assert_eq!(
        transmission_headers(again.cells()),
        0,
        "and the second paint writes the placeholders of a picture the terminal already holds"
    );
    assert_eq!(
        pictured_rows(again.cells()).len(),
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

    frame::painted(&mut view, SCREEN.0, SCREEN.1);
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

    frame::painted(&mut view, 60, SCREEN.1);
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

    let wide = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert_eq!(
        transmission_headers(wide.cells()),
        1,
        "the picture is sent once at the size the wide band fits it into"
    );
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        1,
        "and made once for that size"
    );

    let narrow = frame::painted(&mut view, 60, SCREEN.1);
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        2,
        "the narrower band fits the picture into fewer columns, so it is another picture"
    );
    assert_eq!(
        transmission_headers(narrow.cells()),
        1,
        "and the terminal is told about that size once"
    );

    let again = frame::painted(&mut view, 60, SCREEN.1);
    assert_eq!(
        transmission_headers(again.cells()),
        0,
        "the size the band shows is still held: a repaint of it transmits nothing"
    );
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        2,
        "and makes no second raster of it"
    );

    let back = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        3,
        "the size the band has left is held no longer, so going back to it makes the picture \
         again"
    );
    assert_eq!(
        transmission_headers(back.cells()),
        1,
        "and sends it again, which is the price of a cache that is bounded"
    );
}

#[test]
fn a_picture_block_copied_out_of_the_band_is_its_words_and_never_its_bytes() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(caps::at(cell()), &workspace, "![a purple field](shot.png)");
    let painted = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let drawn = *pictured_rows(painted.cells())
        .first()
        .expect("the picture is on the screen");

    view.drag(Cell::new(drawn, 0), Cell::new(drawn, SCREEN.0 - 1));
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
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
    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let words: String = frame.rows().join("\n");

    assert!(
        words.contains("a purple field"),
        "the picture's words are on the screen, and are the only thing it is said as: {words:?}"
    );
    assert_eq!(
        transmission_headers(frame.cells()),
        0,
        "and no byte of a picture protocol is in a frame that places none"
    );
    assert!(
        pictured_rows(frame.cells()).is_empty(),
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
fn a_diagram_writes_its_sequence_into_every_row_of_the_block_it_is_placed_over() {
    let (_root, workspace) = workspace(40, 40);
    let mut view = frame(
        caps::at(cell()),
        &workspace,
        "```mermaid\nflowchart TD\n  A-->B\n  B-->C\n  C-->D\n```\n\nand the prose under it",
    );

    let painted = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let rows = pictured_rows(painted.cells());
    assert!(
        rows.len() > 3,
        "a flowchart is placed over more rows than the one it opened on: {rows:?}"
    );
    for y in rows {
        assert!(
            (0..SCREEN.0).any(|x| painted.cell(x, y).symbol().contains(PLACEHOLDER)),
            "row {y} of the block carries the picture's own sequence: a row whose cells are \
             skipped and whose first cell holds no sequence is placed over nothing"
        );
    }
}

#[test]
fn a_diagram_and_an_image_in_one_answer_are_placed_each_over_its_own_block() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(
        caps::at(cell()),
        &workspace,
        "![a purple field](shot.png)\n\n```mermaid\nflowchart TD\n  A-->B\n```",
    );

    let painted = frame::painted(&mut view, SCREEN.0, SCREEN.1);

    assert!(
        pictured_rows(painted.cells()).len() >= 10,
        "both pictures are on the screen: {:?}",
        pictured_rows(painted.cells())
    );
    assert_eq!(
        transmission_headers(painted.cells()),
        2,
        "and each was transmitted once, from its own first cell"
    );
}

#[test]
fn the_face_is_placed_over_the_top_rows_of_the_rail_above_the_first_block() {
    for cols in [100, 120] {
        let mut view = railed(true, true);
        let painted = frame::painted(&mut view, cols, 40);
        let rail = rail_band(&painted);
        let face = face::over_rail(rail.width - 1, &carried(true)).expect("the face a rail holds");

        assert_eq!(
            pictured_rows(painted.cells()),
            (rail.y..rail.y + face.rows()).collect::<Vec<u16>>(),
            "the face is drawn in the {cols}-column frame's top rows, {} of them, from the rail's first \
             row",
            face.rows()
        );
        for y in rail.y..rail.y + face.rows() {
            let columns = pictured_cols(painted.cells(), y);
            assert_eq!(
                (columns.first().copied(), columns.len()),
                (
                    Some(rail.x + 1 + (rail.width - 1 - face.cols()) / 2),
                    usize::from(face.cols())
                ),
                "row {y} of the face is {} columns centred in the rail's columns past the one the \
                 frame's seam stands in: {columns:?}",
                face.cols()
            );
            assert!(
                !rail_row(painted.cells(), rail, y).contains('─'),
                "row {y} of the face has a hairline drawn through it: {:?}",
                rail_row(painted.cells(), rail, y)
            );
        }

        for y in rail.y + face.rows()..rail.bottom() {
            assert!(
                pictured_cols(painted.cells(), y).is_empty(),
                "row {y} of the rail stands under the face and holds a cell of it: {:?}",
                pictured_cols(painted.cells(), y)
            );
        }

        let rule = rail.y + face.rows();
        assert_eq!(
            rail_row(painted.cells(), rail, rule).trim_end(),
            format!(" {}", "─".repeat(usize::from(rail.width) - 1)),
            "the face is closed by the one rule the rail draws in its body, opening past the \
             column the frame's seam runs down"
        );
        let first = rule + 1;
        assert_eq!(
            rail_row(painted.cells(), rail, first).trim_end(),
            "  wire      anthropic",
            "and the first block stands on the row under that rule, whole"
        );
        assert_eq!(
            rail_row(painted.cells(), rail, first + 1).trim_end(),
            format!(
                "  model     {}",
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
        let mut bare = railed(false, true);
        let without = frame::painted(&mut bare, cols, 40);
        let mut faced = railed(true, true);
        let with = frame::painted(&mut faced, cols, 40);
        let rail = rail_band(&with);

        let stated = |buffer: &Buffer, from: u16| -> Vec<String> {
            (from..rail.bottom())
                .map(|y| rail_row(buffer, rail, y).trim_end().to_owned())
                .filter(|row| !row.is_empty())
                .collect()
        };
        let under = u16::try_from(pictured_rows(with.cells()).len()).expect("the band's rows") + 1;

        assert_eq!(
            stated(with.cells(), rail.y + under),
            stated(without.cells(), rail.y),
            "the {cols}-column frame draws the same facts under the face that it drew from the \
             rail's first row: none is given up to the face"
        );
        let counted = |rows: Vec<String>| -> Vec<String> {
            rows.into_iter()
                .filter(|row| row.contains("more"))
                .collect()
        };
        assert_eq!(
            counted(stated(with.cells(), rail.y + under)),
            counted(stated(without.cells(), rail.y)),
            "and the rail counts as gone under the face exactly what it counts from its first row, \
             the width taking the facts it takes and the face none: {:?}",
            stated(with.cells(), rail.y + under)
        );
    }
}

#[test]
fn below_a_hundred_columns_no_face_is_drawn_and_the_strip_is_the_row_it_was() {
    for cols in [40, 80, 99] {
        let mut faced = railed(true, true);
        let with = frame::painted(&mut faced, cols, 24);
        let mut bare = railed(false, true);
        let without = frame::painted(&mut bare, cols, 24);

        assert!(
            pictured_rows(with.cells()).is_empty(),
            "a {cols}-column frame draws one row of facts and no face: it has no band to reserve"
        );
        same_cells_outside_ground(
            with.cells(),
            without.cells(),
            [strip_band(cols, 24), transcript_band(&with)],
            8,
        );
    }
}

#[test]
fn a_frame_the_face_is_turned_off_for_is_the_frame_a_terminal_that_places_no_picture_draws() {
    for cols in [100, 120] {
        let mut turned_off = railed(true, false);
        let off = frame::painted(&mut turned_off, cols, 40);
        let rail = rail_band(&off);
        let mut placeless = railed(false, true);
        let placeless = frame::painted(&mut placeless, cols, 40);

        assert!(
            pictured_rows(off.cells()).is_empty(),
            "a frame the config turned the face off for is given no cell of it"
        );
        assert_eq!(
            rail_row(off.cells(), rail, rail.y).trim_end(),
            "  wire      anthropic",
            "the facts stand from the rail's first row, and the row under them is the second \
             fact: no band is reserved for a face that is not drawn, and no rule closes one"
        );
        same_cells_outside_ground(
            off.cells(),
            placeless.cells(),
            [rail, transcript_band(&off)],
            8,
        );

        let mut back_on = railed(true, true);
        let back_on = frame::painted(&mut back_on, cols, 40);
        let face = face::over_rail(rail.width - 1, &carried(true)).expect("a face the rail holds");
        assert_eq!(
            pictured_rows(back_on.cells()).len(),
            usize::from(face.rows()),
            "a frame the setting is turned back on for draws the face again, over the rows it \
             reserved before"
        );
    }
}

#[test]
fn the_rail_beside_the_splash_screen_loses_no_fact_to_the_face_it_carries() {
    let mut pictured = splash(true);
    let with = frame::painted(&mut pictured, 120, 40);
    let mut placeless = splash(false);
    let without = frame::painted(&mut placeless, 120, 40);
    let rail = rail_band(&with);
    let face = face::over_rail(rail.width - 1, &carried(true)).expect("the face a rail holds");

    let pictured_in_rail = |y: u16| -> Vec<u16> {
        pictured_cols(with.cells(), y)
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
                Some(rail.x + 1 + (rail.width - 1 - face.cols()) / 2),
                usize::from(face.cols())
            ),
            "row {y} of the face is {} columns centred in the rail's columns past the one the \
             frame's seam stands in: {columns:?}",
            face.cols()
        );
    }

    let hairline = rail.y + face.rows();
    assert_eq!(
        rail_row(with.cells(), rail, hairline).trim_end(),
        format!(" {}", "─".repeat(usize::from(rail.width) - 1)),
        "the face the rail carries is closed by the one rule the rail draws in its body, \
         opening past the column the frame's seam runs down"
    );
    let first = hairline + 1;
    assert_eq!(
        rail_row(with.cells(), rail, first).trim_end(),
        "  wire      anthropic",
        "and the first fact stands on the row under that rule, whole"
    );

    let stated = |buffer: &Buffer, from: u16| -> Vec<String> {
        (from..rail.bottom())
            .map(|y| rail_row(buffer, rail, y).trim_end().to_owned())
            .filter(|row| !row.is_empty())
            .collect()
    };
    assert_eq!(
        stated(with.cells(), first),
        stated(without.cells(), rail.y),
        "the splash screen's rail draws the facts a terminal that places no picture draws from \
         its first row, so the face it carries takes no fact of one"
    );
    assert!(
        !stated(with.cells(), first)
            .iter()
            .any(|row| row.contains("more")),
        "and none is counted as gone, so the rail grows no row saying one went: {:?}",
        stated(with.cells(), first)
    );
}

#[test]
fn a_splash_screen_the_face_is_turned_off_for_is_the_one_a_placeless_terminal_draws() {
    let mut turned_off = View::of(caps::faced(false), boot_facts(), Transcript::none());
    let off = frame::painted(&mut turned_off, 120, 40);
    let rail = rail_band(&off);
    let mut placeless = splash(false);
    let without = frame::painted(&mut placeless, 120, 40);

    assert!(
        pictured_rows(off.cells()).is_empty(),
        "a splash frame the config turned the face off for is given no cell of the one the rail \
         carries"
    );
    assert_eq!(
        rail_row(off.cells(), rail, rail.y).trim_end(),
        "  wire      anthropic",
        "the facts stand from the rail's first row: no band is reserved for a face that is not \
         drawn, and no rule closes one"
    );
    same_cells_outside_ground(
        off.cells(),
        without.cells(),
        [rail, transcript_band(&off)],
        5,
    );
}

#[test]
fn an_eighty_column_splash_screen_holds_no_face_and_neither_does_a_runs_frame() {
    let mut launching = splash(true);
    let splash = frame::painted(&mut launching, 80, 24);
    let mut live = railed(true, true);
    let run = frame::painted(&mut live, 80, 24);

    assert!(
        pictured_rows(splash.cells()).is_empty(),
        "80 columns leave no rail beside the splash screen, and the frame carries no other face: \
         {:?}",
        pictured_rows(splash.cells())
    );
    assert!(
        pictured_rows(run.cells()).is_empty(),
        "and a conversation's 80-column frame is the strip it was drawn as, with no face on it \
         either: {:?}",
        pictured_rows(run.cells())
    );
}

#[test]
fn a_terminal_that_places_no_picture_draws_the_facts_from_the_rails_first_row() {
    for cols in [100, 120] {
        let mut view = railed(false, true);
        let painted = frame::painted(&mut view, cols, 40);
        let rail = rail_band(&painted);

        assert!(
            pictured_rows(painted.cells()).is_empty(),
            "a terminal that places no picture is given no cell of the face"
        );
        assert_eq!(
            rail_row(painted.cells(), rail, rail.y).trim_end(),
            "  wire      anthropic",
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
        let painted = frame::painted(&mut view, cols, 40);
        let rail = rail_band(&painted);
        let face = face::over_rail(rail.width - 1, &carried(true)).expect("the face a rail holds");
        let reserved = painted
            .told()
            .layout()
            .expect("a laid out frame")
            .rail
            .expect("a frame this wide holds a ledger")
            .1
            .with_picture(face.cols(), face.rows())
            .picture
            .expect("the cells the rail reserved for the face");

        for y in rail.y..rail.bottom() {
            for x in rail.x..rail.right() {
                let covered = pictured_cols(painted.cells(), y).contains(&x);
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
    let frame = frame::painted(&mut view, 80, 24);
    let band = frame.told().layout().expect("a laid out frame").composer;
    let ground = composer_plane(&view);
    assert_eq!(ground.area(), band);
    assert_eq!(
        ground.pixels().dimensions(),
        (800, u32::from(band.height) * 20)
    );
    assert!(
        (band.y..band.bottom())
            .any(|y| { (band.x..band.right()).any(|x| frame.cells()[(x, y)].symbol() == "f") })
    );
    assert!((band.y..band.bottom()).all(|y| {
        (band.x..band.right()).all(|x| !frame.cells()[(x, y)].symbol().contains(PLACEHOLDER))
    }));
    let palette = *frame.palette();
    let shade = shade_of(&frame, PlaneId::Composer);
    let (prompt, rail) = (
        channels(shade.row(0)).expect("the band's first row in pixels"),
        channels(palette.band(Band::Rail)).expect("the tone it closes on"),
    );
    let top = ground.pixels().get_pixel(400, 0);
    assert_eq!(
        top.0[..3],
        prompt,
        "the ground starts on the band the prompt is read on, and no pixel row of the well \
         stands above it"
    );
    let down = cell().height();
    for row in 1..band.height - 1 {
        let y = u32::from(row) * down + down / 2;
        assert_eq!(
            ground.pixels().get_pixel(400, y).0[..3],
            prompt,
            "row {row} of the band is a whole row of the prompt's band, and no step between the \
             two tones"
        );
    }
    assert_eq!(
        ground
            .pixels()
            .get_pixel(400, ground.pixels().height() - 1)
            .0[..3],
        rail,
        "and it comes down to the rail's tone the band closes on"
    );
    assert_ne!(
        ground.pixels().get_pixel(25, 30),
        ground.pixels().get_pixel(400, 30),
        "the caret glows"
    );
}

#[test]
fn no_pixel_of_the_composers_band_is_a_blend_of_the_two_tones_it_closes_through() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    view.paste("forge");
    let painted = frame::painted(&mut view, 80, 24);
    let palette = *painted.palette();
    let shade = shade_of(&painted, PlaneId::Composer);
    let ground = composer_plane(&view);
    let down = cell().height();
    let (prompt, rail) = (
        channels(palette.band(Band::Prompt)).expect("the prompt's band in pixels"),
        channels(palette.band(Band::Rail)).expect("the rail's tone in pixels"),
    );

    for y in 0..ground.pixels().height() {
        let tone = channels(shade.row((y / down) as u16)).expect("a tone of the band in pixels");
        assert!(
            tone == prompt || tone == rail,
            "pixel row {y} of the band sits on {tone:?}, which is neither of the two tones the \
             band is drawn in"
        );
        for x in 0..ground.pixels().width() {
            let pixel = ground.pixels().get_pixel(x, y);
            let drawn = [pixel[0], pixel[1], pixel[2]];
            assert!(
                (0..3).all(|channel| drawn[channel] >= tone[channel]),
                "column {x} of pixel row {y} is {drawn:?}, under the {tone:?} its row sits on: a \
                 ground mixed between two surfaces is a state, and the caret's glow is the one \
                 blend the band makes"
            );
        }
    }
}

#[test]
fn the_frame_a_launch_opens_on_stands_on_the_bands_own_pixel_ground() {
    let mut view = splash(true);
    let frame = frame::painted(&mut view, 120, 40);
    let band = frame.told().layout().expect("a laid out frame").composer;
    let ground = composer_plane(&view);
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
    let palette = *frame.palette();
    for row in 0..band.height {
        let (x, y) = (band.x + band.width / 2, band.y + row);
        assert_eq!(
            frame.cells()[(x, y)].bg,
            match row {
                0 => palette.edge_ink(),
                _ => palette.ground(None).bg.unwrap(),
            },
            "row {row} of the band is claimed as the ground the pixels are, and not ramped \
             cell by cell over it \u{2014} the row it is headed by is the frame's seam"
        );
    }
    let (width, _) = ground.pixels().dimensions();
    assert_eq!(
        ground.pixels().get_pixel(width - 1, 0).0[..3],
        channels(palette.band(Band::Prompt)).expect("the prompt's band in pixels"),
        "and the band the prompt is read on is the plane's first pixel row, which is how it \
         reads over the band's own ground"
    );
}

#[test]
fn a_256_colour_palette_draws_the_bands_ground_in_its_own_greys() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::greys(), &workspace, "hello");
    let frame = frame::painted(&mut view, 80, 24);
    let palette = *frame.palette();
    assert_eq!(
        palette,
        Palette::ANSI,
        "a terminal that names no twenty-four-bit colour draws the design in its greys"
    );
    let ground = composer_plane(&view);
    let (width, height) = ground.pixels().dimensions();
    let well = channels(palette.band(Band::Well)).expect("the band's well in pixels");
    let shade = shade_of(&frame, PlaneId::Composer);
    let (prompt, rail) = (
        channels(palette.band(Band::Prompt)).expect("the prompt's band in pixels"),
        channels(palette.band(Band::Rail)).expect("the rail's tone in pixels"),
    );
    assert_eq!(
        (prompt, rail),
        ([48, 48, 48], [38, 38, 38]),
        "the band this palette spells runs from the prompt's grey down to the rail's own"
    );
    assert_eq!(
        ground.pixels().get_pixel(width - 1, 0).0[..3],
        channels(shade.row(0)).expect("the band's first row in pixels"),
        "the plane's top row is the band's own head row, the band the prompt is read on"
    );
    assert_eq!(
        ground.pixels().get_pixel(width - 1, height - 1).0[..3],
        rail,
        "and its last is the rail's tone the band closes on"
    );
    assert_ne!(
        ground.pixels().get_pixel(width - 1, 0).0[..3],
        well,
        "and not the well of the transcript band, which the composer's band stands on"
    );
    assert_ne!(
        ground.pixels().get_pixel(width - 1, 0).0[..3],
        [22, 31, 42],
        "and not the band the design's truecolour prompt is read on"
    );
    let bottom = ground.pixels().get_pixel(width - 1, height - 1);
    for at in 0..3 {
        assert!(
            bottom.0[at].abs_diff(rail[at]) < bottom.0[at].abs_diff(prompt[at]),
            "the plane's bottom row {:?} is nearer the rail's tone {rail:?} than the prompt's \
             band {prompt:?}",
            bottom.0
        );
    }
    for y in 0..height {
        let pixel = ground.pixels().get_pixel(width - 1, y);
        assert_eq!(
            (pixel[0], pixel[1], pixel[2]),
            (pixel[1], pixel[1], pixel[1]),
            "the plane's last column is grey at pixel row {y}, far from the caret's glow"
        );
    }
}

#[test]
fn a_truecolour_frames_band_is_drawn_in_the_two_tones_the_palette_gives_it() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    let frame = frame::painted(&mut view, 80, 24);
    let band = frame.told().layout().expect("a laid out frame").composer;
    let ground = composer_plane(&view);
    let (width, height) = ground.pixels().dimensions();
    assert_eq!(
        (width, height),
        (
            u32::from(band.width) * cell().width(),
            u32::from(band.height) * cell().height()
        ),
        "the plane covers the band's own rect, at the pixels of the cell it is drawn on"
    );
    let palette = *frame.palette();
    let (prompt, rail) = (
        channels(palette.band(Band::Prompt)).expect("the prompt's band in pixels"),
        channels(palette.band(Band::Rail)).expect("the rail's tone in pixels"),
    );
    let column = width - 1;
    let top = ground.pixels().get_pixel(column, 0).0;
    assert_eq!(
        top[..3],
        prompt,
        "the plane's top row is the band the prompt is read on, and no pixel row of the well \
         above it"
    );
    let bottom = ground.pixels().get_pixel(column, height - 1).0;
    assert_eq!(
        bottom[..3],
        rail,
        "and its bottom row is the rail's own tone, which is the row the band closes on"
    );
    let mid = ground.pixels().get_pixel(column, height / 2).0;
    assert_eq!(
        mid[..3],
        prompt,
        "and half way down the plane stands on the prompt's band still: the band climbs no ramp \
         to its last row"
    );
}

#[test]
fn the_bands_ground_is_the_shade_sampled_row_by_row_from_the_prompt_band_to_the_rail() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    let frame = frame::painted(&mut view, 80, 24);
    let palette = *frame.palette();
    let band = frame.told().layout().expect("a laid out frame").composer;
    let ground = composer_plane(&view);
    let (width, height) = ground.pixels().dimensions();
    let column = width - 1;
    let shade = shade_of(&frame, PlaneId::Composer);
    let first = channels(shade.row(0)).expect("the band's first row in pixels");
    let last = channels(palette.band(Band::Rail)).expect("the tone it closes on");

    for row in 0..band.height {
        let at = u32::from(row) * cell().height() + cell().height() / 2;
        let pixel = ground.pixels().get_pixel(column, at).0;
        let said = channels(shade.row(row)).expect("a row in pixels");
        for channel in 0..3 {
            assert!(
                pixel[channel].abs_diff(said[channel]) <= 1,
                "channel {channel} of row {row} of the plane is {} where the shade says {}: the \
                 plane is the shade sampled at the cell",
                pixel[channel],
                said[channel]
            );
        }
    }
    assert_eq!(
        ground.pixels().get_pixel(column, 0).0[..3],
        first,
        "the plane opens on the band the prompt is read on"
    );
    assert_eq!(
        ground.pixels().get_pixel(column, height - 1).0[..3],
        last,
        "and closes on the rail's tone"
    );
}

#[test]
fn the_carets_glow_is_the_same_light_on_every_pixel_row_of_its_column() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    view.paste("forge");
    let frame = frame::painted(&mut view, 80, 24);
    let band = frame.told().layout().expect("a laid out frame").composer;
    let caret = frame
        .told()
        .caret()
        .expect("a draft that takes the keys places the terminal's cursor");
    let ground = composer_plane(&view);
    let (width, height) = ground.pixels().dimensions();
    let at = u32::from(caret.x - band.x) * cell().width() + cell().width() / 2;
    let step = |y: u32| {
        let lit = ground.pixels().get_pixel(at, y);
        let far = ground.pixels().get_pixel(width - 1, y);
        [0, 1, 2].map(|channel| lit[channel].saturating_sub(far[channel]))
    };

    let waited = step(0);
    assert!(
        waited.iter().any(|channel| *channel > 0),
        "the column the caret waits in is no lighter than the band's far side on its first pixel \
         row: {waited:?}"
    );
    for y in 1..height {
        let row = step(y);
        for channel in 0..3 {
            assert!(
                row[channel].abs_diff(waited[channel]) <= 1,
                "channel {channel} of the light over the caret is {} on pixel row {y} and {} on \
                 the row above it, so the glow follows the caret's column alone",
                row[channel],
                waited[channel]
            );
        }
    }
}

#[test]
fn the_bands_glow_still_falls_off_under_the_caret() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    view.paste("forge");
    let frame = frame::painted(&mut view, 80, 24);
    let band = frame.told().layout().expect("a laid out frame").composer;
    let caret = frame
        .told()
        .caret()
        .expect("a draft that takes the keys places the terminal's cursor");
    let caret = (caret.x, caret.y);
    let ground = composer_plane(&view);
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
fn the_bands_head_row_is_the_frames_seam_drawn_over_the_planes_light() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    let frame = frame::painted(&mut view, 80, 24);
    let palette = *frame.palette();
    let band = frame.told().layout().expect("a laid out frame").composer;
    let ground = composer_plane(&view);
    assert_eq!(
        ground.area(),
        band,
        "the plane stands under the band's own head row"
    );
    let edge = channels(shade_of(&frame, PlaneId::Composer).row(0)).expect("the light in pixels");
    let head = middle_of(ground, 0);
    for channel in 0..3 {
        assert!(
            head[channel].abs_diff(edge[channel]) <= 1,
            "channel {channel} of the middle of the band's head cell is {} where the light the \
             band meets the frame on is {}",
            head[channel],
            edge[channel]
        );
    }
    let mut claimed = 0;
    for x in band.x..band.right() {
        assert_eq!(
            frame.cells()[(x, band.y)].bg,
            palette.edge_ink(),
            "column {x} of the band's head row is not the one edge ink, so the seam that heads \
             the band is read as a cell's light rather than the plane's"
        );
        claimed += 1;
    }
    assert_eq!(
        claimed, band.width,
        "every column of the head row is claimed by the seam of the frame"
    );
}

#[test]
fn a_rails_band_is_a_plane_of_its_own_beside_the_composers() {
    let mut view = railed(true, false);
    let frame = frame::painted(&mut view, 120, 40);
    let rail = frame
        .told()
        .layout()
        .expect("a laid out frame")
        .rail
        .expect("a rail")
        .1
        .area;
    let palette = *frame.palette();

    assert_eq!(
        frame
            .told()
            .shades()
            .iter()
            .filter_map(Shade::id)
            .collect::<Vec<PlaneId>>(),
        vec![PlaneId::Rail, PlaneId::Transcript, PlaneId::Composer],
        "the frame hands back the rail's shading beside the transcript band's and the \
         composer's, each under its own identity"
    );
    let band = plane_of(&view, PlaneId::Rail);
    assert_eq!(
        band.area(),
        rail,
        "the plane covers the rail's own rows and columns, and the columns the seam reaches back \
         over are the transcript band's"
    );
    assert_eq!(
        band.pixels().dimensions(),
        (
            u32::from(rail.width) * cell().width(),
            u32::from(rail.height) * cell().height()
        ),
        "at the pixels of the cell it is drawn on"
    );
    let (width, height) = band.pixels().dimensions();
    let tone = channels(palette.band(Band::Rail)).expect("a band in pixels");
    assert_eq!(
        band.pixels().get_pixel(0, 0).0[..3],
        tone,
        "the plane's head is the rail's own tone, and no column of its own is lit"
    );
    assert_eq!(
        band.pixels().get_pixel(width - 1, height - 1).0[..3],
        tone,
        "and its foot is that same tone, so no pixel of the band is drawn on the shadow"
    );
    let middle = band.pixels().get_pixel(width - 1, height / 2).0;
    for channel in 0..3 {
        assert!(
            middle[channel].abs_diff(tone[channel]) <= 1,
            "channel {channel} of the plane half way down the rail is {} where the rail's own \
             tone is {}: the shade is flat above its last row",
            middle[channel],
            tone[channel]
        );
    }
    for y in rail.y..rail.bottom() {
        assert!(
            !rail_row(frame.cells(), rail, y).contains(PLACEHOLDER),
            "row {y} of the rail's band is drawn by the plane the cells stand on, and no cell of \
             it is replaced by the placeholder glyph"
        );
    }
}

fn railed_words(face: bool) -> View {
    View::of(
        caps::faced(face),
        sindri::facts::Facts::default()
            .asked()
            .railed(railed_facts()),
        Transcript::of(vec![
            Entry::Assistant {
                turn: 1,
                text: SecretSet::default().redact(
                    "the words the transcript band holds beside the rail that stands on its \
                     right, row after row, so the band's head is lit over prose a reader reads",
                ),
            },
            Entry::Assistant {
                turn: 2,
                text: SecretSet::default().redact(
                    "and a second run's answer under it, so the band holds more than one row of \
                     words and the light above them falls on no blank band",
                ),
            },
        ]),
    )
}

#[test]
fn a_frames_three_bands_are_three_planes_of_their_own() {
    let mut view = railed_words(true);
    let frame = frame::painted(&mut view, 120, 40);
    let palette = *frame.palette();
    let laid = frame.told().layout().expect("a laid out frame");
    let rail = laid.rail.expect("a rail on a frame that knows a fact").1;
    let Shade::Rail {
        face: Some(face), ..
    } = shade_of(&frame, PlaneId::Rail)
    else {
        panic!(
            "the rail's shade carries no face, and the rows Sindri's face stands on are the rows \
             the frame's light falls on"
        )
    };
    let bands = [
        (PlaneId::Rail, rail.area),
        (PlaneId::Transcript, laid.transcript),
        (PlaneId::Composer, laid.composer),
    ];

    assert_eq!(
        frame
            .told()
            .shades()
            .iter()
            .filter_map(Shade::id)
            .collect::<Vec<PlaneId>>(),
        vec![PlaneId::Rail, PlaneId::Transcript, PlaneId::Composer],
        "a frame hands back a plane for each of the three bands it draws, each under its own \
         identity, in the order it paints them"
    );
    for (id, area) in bands {
        let plane = plane_of(&view, id);
        assert_eq!(
            plane.area(),
            area,
            "{id:?}'s plane covers a rect that is not its band's own"
        );
        assert_eq!(
            plane.pixels().dimensions(),
            (
                u32::from(area.width) * cell().width(),
                u32::from(area.height) * cell().height()
            ),
            "{id:?}'s plane is not drawn at the pixels of the cell it is drawn on"
        );
    }

    let well = channels(palette.band(Band::Well)).expect("a band in pixels");
    let lift = palette.lift(Band::Well);
    let lit = well.map(|channel| channel.saturating_add(lift));
    let band = plane_of(&view, PlaneId::Transcript);
    let (width, height) = band.pixels().dimensions();
    for x in 0..width {
        let head = band.pixels().get_pixel(x, 0).0;
        assert_eq!(
            head[..3],
            lit,
            "column {x} of the transcript band's plane is not the well lit to the most it can \
             take on its first pixel row: the head of the band is where the frame's light falls"
        );
    }
    for y in cell().height()..2 * cell().height() {
        let second = band.pixels().get_pixel(0, y).0;
        assert_eq!(
            second[..3],
            well.map(|channel| channel.saturating_add(lift / 2)),
            "pixel row {y} of the transcript band's plane is not the head's light halved: the \
             light falls away from the band's head in whole channels"
        );
    }
    for y in 2 * cell().height()..height {
        for x in 0..width {
            let pixel = band.pixels().get_pixel(x, y).0;
            assert_eq!(
                pixel[..3],
                well,
                "pixel row {y}, column {x} of the transcript band's plane is not the well: the \
                 prose is read on the well, and the light has fallen back to it by the band's \
                 third row"
            );
        }
    }

    let tone = channels(palette.band(Band::Rail)).expect("a band in pixels");
    let lit = tone.map(|channel| channel.saturating_add(palette.lift(Band::Rail)));
    let band = plane_of(&view, PlaneId::Rail);
    let height = band.pixels().height();
    for row in 0..rail.area.height {
        let y = rail.area.y + row;
        let stands = y < face.bottom();
        for down in 0..cell().height() {
            for x in 0..band.pixels().width() {
                let pixel = band
                    .pixels()
                    .get_pixel(x, u32::from(row) * cell().height() + down)
                    .0;
                let wanted = match stands {
                    true => lit,
                    false => tone,
                };
                assert_eq!(
                    pixel[..3],
                    wanted,
                    "row {row}, column {x} of the rail's plane is not {}: the rows Sindri's face \
                     stands on are the rows the light falls on, and every row below them is the \
                     rail's own tone",
                    match stands {
                        true => "the rail's tone lit",
                        false => "the rail's own tone",
                    }
                );
            }
        }
    }
    assert!(
        height >= u32::from(face.bottom() - rail.area.y) * cell().height(),
        "the rail's plane is shorter than the rows the face stands on"
    );
    for y in face.y..face.bottom() {
        let words: String = (rail.area.x + 1..rail.area.right())
            .map(|x| frame.cells()[(x, y)].symbol().to_owned())
            .filter(|symbol| !symbol.contains('\u{1b}'))
            .collect();
        assert!(
            words.trim().is_empty(),
            "row {y} of the rail holds a word where Sindri's face stands, and the light falls \
             there because no word is read there: {words:?}"
        );
    }
}

fn ratio(one: [u8; 3], other: [u8; 3]) -> f64 {
    let luminance = |colour: [u8; 3]| {
        let [red, green, blue] = colour.map(|channel| {
            let channel = f64::from(channel) / 255.0;
            match channel <= 0.03928 {
                true => channel / 12.92,
                false => ((channel + 0.055) / 1.055).powf(2.4),
            }
        });
        0.2126 * red + 0.7152 * green + 0.0722 * blue
    };
    let (one, other) = (luminance(one), luminance(other));
    let (high, low) = match one > other {
        true => (one, other),
        false => (other, one),
    };
    (high + 0.05) / (low + 0.05)
}

#[test]
fn every_ink_a_frame_draws_keeps_its_floor_on_the_pixel_its_cell_stands_on() {
    let mut view = railed_words(true);
    let frame = frame::painted(&mut view, 120, 40);
    let palette = *frame.palette();
    let area = frame.area();
    let planes: Vec<&Plane> = PlaneId::ALL
        .into_iter()
        .map(|id| plane_of(&view, id))
        .collect();
    let mut read = 0;
    let mut lit = 0;

    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let Some(ink) = frame.ink(x, y) else {
                continue;
            };
            let colour = channels(palette.colour(ink));
            let Some(colour) = colour else {
                continue;
            };
            let Some(plane) = planes
                .iter()
                .find(|plane| plane.area().contains(Position::new(x, y)))
            else {
                continue;
            };
            let pixel = plane.pixels().get_pixel(
                (u32::from(x) - u32::from(plane.area().x)) * cell().width() + cell().width() / 2,
                (u32::from(y) - u32::from(plane.area().y)) * cell().height() + cell().height() / 2,
            );
            let under = [pixel[0], pixel[1], pixel[2]];
            let kept = ratio(colour, under);
            let row = (u32::from(y) - u32::from(plane.area().y)) / cell().height();
            let rung = channels(shade_of(&frame, plane.id()).row(row as u16))
                .expect("the band's own row in pixels");
            read += 1;
            lit += usize::from(under != rung);
            assert!(
                kept >= sindri::palette::LEGIBLE,
                "{} reads at {kept}:1 on the pixel {:?} its cell stands on at row {y}, column \
                 {x}: the light a plane carries is the most every ink can take and still clear \
                 its floor, so no word of the frame is washed out by the band it is drawn on",
                ink.name(),
                under
            );
        }
    }
    assert!(
        read > 0 && lit > 0,
        "the frame draws no word on a plane, so nothing above is proved: {read} words read and \
         {lit} of them on a lit pixel"
    );
}

#[test]
fn a_palette_that_draws_no_colour_is_handed_no_plane_over_the_transcript() {
    let mut view = View::of(
        caps::uncoloured(),
        sindri::facts::Facts::default()
            .asked()
            .railed(railed_facts()),
        Transcript::none(),
    );
    let frame = frame::painted(&mut view, 120, 40);
    let palette = *frame.palette();
    assert_eq!(
        palette,
        Palette::SLOTS,
        "a terminal told NO_COLOR draws the reader's own slots"
    );
    assert!(
        frame
            .told()
            .shades()
            .iter()
            .all(|shade| shade.id() != Some(PlaneId::Transcript)),
        "a palette that draws no colour is handed no shading over the transcript band: its own \
         rows are the flat band it already had"
    );
    let band = transcript_band(&frame);
    let shade = Shade::transcript(band, None, &palette).expect("a band to shade");
    for row in 0..band.height {
        let (x, y) = (band.x + band.width / 2, band.y + row);
        assert_eq!(
            frame.cells()[(x, y)].bg,
            shade.row(row),
            "row {row} of the transcript band stands on a ground of its own, and a palette that \
             spells no colour is handed no plane to stand on"
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
    let frame = frame::painted(&mut view, 120, 40);
    assert_eq!(
        view.capabilities().palette(),
        Palette::SLOTS,
        "a terminal told NO_COLOR draws the reader's own slots"
    );
    assert!(
        frame
            .told()
            .shades()
            .iter()
            .all(|shade| shade.id() != Some(PlaneId::Rail)),
        "a palette that draws no colour is handed no shading over the rail: its band is the flat \
         band it already had"
    );
}

#[test]
fn a_palette_that_draws_no_colour_is_handed_no_plane_over_the_composer() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::uncoloured(), &workspace, "hello");
    let frame = frame::painted(&mut view, 80, 24);
    let palette = *frame.palette();
    assert_eq!(
        palette,
        Palette::SLOTS,
        "a terminal told NO_COLOR draws the reader's own slots"
    );
    assert!(
        frame
            .told()
            .shades()
            .iter()
            .all(|shade| shade.id() != Some(PlaneId::Composer)),
        "a palette that draws no colour is handed no shading over the composer: the band drawn in \
         its place is the flat one the frame already had"
    );
    let band = frame.told().layout().expect("a laid out frame").composer;
    let shade = Shade::composer(band, None, None, &palette).expect("a band to shade");
    for row in 0..band.height {
        let (x, y) = (band.x + band.width / 2, band.y + row);
        assert_eq!(
            frame.cells()[(x, y)].bg,
            shade.row(row),
            "row {row} of the band stands on the flat ground the reader's own slots are, and on \
             no ground a plane was claimed over"
        );
    }
}

#[test]
fn a_terminal_that_places_no_picture_keeps_the_bands_own_rows_and_hands_back_no_plane() {
    let (_root, workspace) = workspace(10, 20);
    let mut plain = frame(Capabilities::default(), &workspace, "hello");
    let mut pictured = frame(caps::at(cell()), &workspace, "hello");
    let plain_frame = frame::painted(&mut plain, 80, 24);
    let picture_frame = frame::painted(&mut pictured, 80, 24);
    assert!(
        plain_frame.told().shades().is_empty(),
        "a terminal that places no picture is handed no shading at all: {:?}",
        plain_frame.told().shades()
    );
    let band = plain_frame.told().layout().unwrap().composer;
    assert_eq!(band, picture_frame.told().layout().unwrap().composer);
    let palette = plain_frame.palette();
    let shade = Shade::composer(band, None, None, palette).expect("a band to shade");
    for row in 0..band.height {
        let x = band.x + band.width / 2;
        let y = band.y + row;
        let ground = match row {
            0 => palette.edge_ink(),
            _ => shade.row(row),
        };
        assert_eq!(
            plain_frame.cells()[(x, y)].bg,
            ground,
            "row {row} of the band stands on its own ground where no plane is placed"
        );
        assert_eq!(
            picture_frame.cells()[(x, y)].bg,
            match row {
                0 => palette.edge_ink(),
                _ => palette.ground(None).bg.unwrap(),
            },
            "row {row} of the band a plane is placed under stands on the terminal's own ground"
        );
    }
    for x in band.x..band.right() {
        assert_eq!(
            plain_frame.cells()[(x, band.y)].bg,
            palette.edge_ink(),
            "column {x} of the head row stands on the one edge ink where no plane is placed"
        );
        assert_eq!(
            picture_frame.cells()[(x, band.y)].bg,
            palette.edge_ink(),
            "and on the same one edge ink over the plane a terminal that places one is handed"
        );
    }
}

#[test]
fn a_pass_that_shades_the_same_bands_makes_no_band_image() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    let first = frame::painted(&mut view, 80, 24);
    let made = view.planes().made();
    assert!(
        made > 0,
        "the first pass makes the images its bands stand on"
    );
    let again = frame::painted(&mut view, 80, 24);
    assert_eq!(
        view.planes().made(),
        made,
        "a pass that shades the same bands makes no band image"
    );
    assert_eq!(
        first.told().shades(),
        again.told().shades(),
        "and the two passes hand the terminal the same bands: {:?} against {:?}",
        first.told().shades(),
        again.told().shades()
    );
}

#[test]
fn a_pass_whose_caret_column_moved_makes_the_composers_band_again() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    view.paste("forge");
    let first = frame::painted(&mut view, 80, 24);
    let made = view.planes().made();
    let band = first.told().layout().expect("a laid out frame").composer;
    view.paste(" more");
    let again = frame::painted(&mut view, 80, 24);
    assert_eq!(
        again.told().layout().expect("a laid out frame").composer,
        band,
        "the band stands where it did, and only the caret's column moved"
    );
    assert_eq!(
        view.planes().made(),
        made + 1,
        "a pass in which the caret's column moved makes the composer's band again, and no band \
         whose shading stood"
    );
}

#[test]
fn a_window_whose_cell_changed_is_drawn_at_the_cell_it_now_reports() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(caps::at(cell()), &workspace, "![a purple field](shot.png)");
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let made = view.planes().made();

    view.celled(Some(CellSize::new(20, 40)));
    let crossed = frame::painted(&mut view, SCREEN.0, SCREEN.1);

    assert_eq!(
        view.capabilities().cell(),
        CellSize::new(20, 40),
        "a frame drawn on another screen is drawn at the cell that screen has"
    );
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        2,
        "the picture is made into pixels again at the new cell"
    );
    assert_eq!(
        transmission_headers(crossed.cells()),
        1,
        "and the terminal, holding the old raster, is sent the new one"
    );
    assert!(
        view.planes().made() > made,
        "and the bands' images are made again at the new cell: {made} images before the pass"
    );
    let band = crossed.told().layout().expect("a laid out frame").composer;
    assert_eq!(
        composer_plane(&view).pixels().dimensions(),
        (u32::from(band.width) * 20, u32::from(band.height) * 40),
        "the composer's plane is drawn at the pixels of the cell the window now has"
    );
}

#[test]
fn a_window_whose_cell_stood_is_drawn_as_it_was_with_no_header_and_no_band_image() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(caps::at(cell()), &workspace, "![a purple field](shot.png)");
    let before = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let made = view.planes().made();

    view.celled(Some(cell()));
    let kept = frame::painted(&mut view, SCREEN.0, SCREEN.1);

    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        1,
        "a cell that did not move makes no second raster"
    );
    assert_eq!(
        transmission_headers(kept.cells()),
        0,
        "and sends the terminal no header it already holds"
    );
    assert_eq!(
        view.planes().made(),
        made,
        "and makes no band image whose shading stood"
    );
    assert_eq!(
        kept.told().shades(),
        before.told().shades(),
        "and hands the terminal the same shades as the pass before it"
    );
}

#[test]
fn a_terminal_that_reports_no_pixels_keeps_the_cell_it_was_given() {
    let (_root, workspace) = workspace(400, 200);
    let mut view = frame(caps::at(cell()), &workspace, "![a purple field](shot.png)");
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let made = view.planes().made();

    view.celled(None);
    let again = frame::painted(&mut view, SCREEN.0, SCREEN.1);

    assert_eq!(
        view.capabilities().cell(),
        cell(),
        "a terminal that reports no pixels is drawn at the cell it was given"
    );
    assert_eq!(
        view.capabilities().pictures().rasterises("shot.png"),
        1,
        "and repaints as it did before"
    );
    assert_eq!(
        transmission_headers(again.cells()),
        0,
        "and sends no header"
    );
    assert_eq!(view.planes().made(), made, "and makes no band image");
}

#[test]
fn a_pass_drawn_at_a_new_size_makes_the_bands_images_again() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    let small = frame::painted(&mut view, 80, 24);
    let made = view.planes().made();
    let large = frame::painted(&mut view, 120, 40);
    assert!(
        view.planes().made() > made,
        "a pass drawn at a new size makes the bands' images again: {made} images before it"
    );
    assert_ne!(
        small.told().shades(),
        large.told().shades(),
        "and the bands it hands the terminal are not the bands of the frame before it: {:?} \
         against {:?}",
        small.told().shades(),
        large.told().shades()
    );
}

#[test]
fn a_frame_whose_marker_has_stepped_makes_no_band_image() {
    let (_root, workspace) = workspace(10, 20);
    let mut view = frame(caps::at(cell()), &workspace, "hello");
    view.handed(view.facts().clone().pulsed(Some(Pulse::live(0))));
    let turned = frame::painted(&mut view, 80, 24);
    let made = view.planes().made();
    view.handed(view.facts().clone().pulsed(Some(Pulse::live(80))));
    let stepped = frame::painted(&mut view, 80, 24);
    assert_ne!(
        turned.words(),
        stepped.words(),
        "the marker turned between the two passes"
    );
    assert_eq!(
        view.planes().made(),
        made,
        "a frame whose marker has stepped makes no band image"
    );
}

fn plane_of(view: &View, id: PlaneId) -> &Plane {
    view.planes()
        .held()
        .iter()
        .find(|plane| plane.id() == id)
        .unwrap_or_else(|| {
            panic!(
                "the frame holds no image under {id:?}, and a terminal that places pictures is \
             handed its bands' shading from every paint: {:?}",
                view.planes()
                    .held()
                    .iter()
                    .map(Plane::id)
                    .collect::<Vec<PlaneId>>()
            )
        })
}

fn composer_plane(view: &View) -> &Plane {
    plane_of(view, PlaneId::Composer)
}

fn shade_of(drawn: &frame::Frame, id: PlaneId) -> Shade {
    *drawn
        .told()
        .shades()
        .iter()
        .find(|shade| shade.id() == Some(id))
        .unwrap_or_else(|| panic!("the frame handed no shade under {id:?}"))
}

fn middle_of(plane: &Plane, row: u16) -> [u8; 3] {
    let pixel = plane.pixels().get_pixel(
        plane.pixels().width() / 2,
        u32::from(row) * cell().height() + cell().height() / 2,
    );
    [pixel[0], pixel[1], pixel[2]]
}

fn same_cells_outside_ground(
    pictured: &Buffer,
    placeless: &Buffer,
    planes: [Rect; 2],
    composer_rows: u16,
) {
    let [rail, transcript] = planes;
    assert_eq!(pictured.area, placeless.area);
    let own = Palette::TRUECOLOR
        .ground(None)
        .bg
        .expect("the terminal's own ground");
    for y in pictured.area.y..pictured.area.bottom() {
        for x in pictured.area.x..pictured.area.right() {
            let with = &pictured[(x, y)];
            let without = &placeless[(x, y)];
            let on_rail =
                (rail.y..rail.bottom()).contains(&y) && (rail.x..rail.right()).contains(&x);
            if y < pictured.area.bottom() - composer_rows - 1 && !on_rail {
                match transcript.contains(Position::new(x, y)) {
                    true => {
                        assert_eq!(
                            (with.symbol(), with.fg, with.modifier),
                            (without.symbol(), without.fg, without.modifier),
                            "the word at ({x}, {y}) of the transcript band is not the one a \
                             terminal that places no picture draws, and the plane under it takes \
                             no word and no ink of it"
                        );
                        assert_eq!(
                            with.bg, own,
                            "the cell at ({x}, {y}) of the transcript band is not the terminal's \
                             own ground, which the band's plane is read through"
                        );
                    }
                    false => assert_eq!(with, without, "outside the two bands at ({x}, {y})"),
                }
            } else if on_rail {
                assert_eq!(
                    (with.symbol(), with.fg, with.modifier),
                    (without.symbol(), without.fg, without.modifier),
                    "the rail's words at ({x}, {y})"
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

#[test]
fn a_diagram_gives_way_on_its_rows_to_the_band_it_is_read_in() {
    let (_root, workspace) = workspace(40, 40);
    let source = "```mermaid\nflowchart TD\n  A-->B\n  B-->C\n  C-->D\n```";
    let of = |surface: &Surface| {
        let rows = markdown::rows_on(source, 60, 0, surface);
        let at = rows
            .iter()
            .position(|row| row.picture().is_some())
            .expect("the diagram is the picture of its block");
        rows[at].picture().expect("the picture of that row").clone()
    };

    let unbounded = of(&surface(&workspace));
    let banded = of(&surface(&workspace).band(8));

    assert!(
        unbounded.rows() > 8,
        "a diagram is taller than a band of eight rows: {} rows",
        unbounded.rows()
    );
    assert!(
        banded.rows() <= 8 && banded.cols() < unbounded.cols(),
        "so it is scaled to the band it is read in, keeping its shape: {} by {} cells against {} \
         by {}",
        banded.cols(),
        banded.rows(),
        unbounded.cols(),
        unbounded.rows()
    );
}

#[test]
fn a_diagram_is_placed_over_the_rows_of_a_band_that_holds_its_block_alone() {
    let (_root, workspace) = workspace(40, 40);
    let source = "## a heading\n\n```mermaid\nflowchart TD\n  A-->B\n  B-->C\n  C-->D\n```";

    let mut view = frame(caps::at(cell()), &workspace, source);
    let painted = frame::painted(&mut view, 60, 16);
    let placed = pictured_rows(painted.cells());

    assert!(
        !placed.is_empty(),
        "a band that holds a diagram's block draws the diagram, and not a hole of blank rows: {:?}",
        (0..16).map(|y| painted.row(y)).collect::<Vec<_>>()
    );
    assert_eq!(
        placed,
        (*placed.first().expect("a placed row")..=*placed.last().expect("a placed row"))
            .collect::<Vec<u16>>(),
        "and the cells it is placed over are one unbroken block: {:?}",
        (0..16).map(|y| painted.row(y)).collect::<Vec<_>>()
    );
}

#[test]
fn a_picture_the_band_cuts_short_is_placed_over_the_rows_it_is_shown_in() {
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
    let shown = Rect { height: 2, ..block };

    let mut screen = Buffer::empty(Rect::new(0, 0, 80, 30));
    pictures.place(&measured, cell(), shown, &mut screen);

    assert_eq!(
        pictured_rows(&screen),
        [2, 3],
        "the rows the band holds are the picture's, and the ones under it are left as they were"
    );
    assert_eq!(
        transmission_headers(&screen),
        1,
        "and the picture is sent once, at the size it was measured into and not the size it is \
         shown at"
    );
}
