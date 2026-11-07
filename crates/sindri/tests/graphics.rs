//! The picture's own contract: an SVG measured in cells before a pixel is spent, and then
//! drawn at the pixels the cell it was measured against really has (§12, ADR-0003).
//!
//! Nothing here knows what made the SVG. A mermaid fence is the ticket above this one
//! (#54), and what it hands over is an SVG string; what this holds the module to is the
//! picture, so a test's SVG is the diagram a fence would produce and never the fence.

use sindri::graphics::{CellSize, Footprint, Refusal, Svg};
use sindri::view::View;

/// The namespace an SVG declares, named once so a picture below reads as its drawing rather
/// than as its boilerplate.
const NAMESPACE: &str = "http://www.w3.org/2000/svg";

/// A diagram the shape a fence draws: a background, a labelled node, and an arrow between
/// two of them, at the size the SVG itself declares.
fn diagram(width: u32, height: u32) -> String {
    format!(
        r##"<svg xmlns="{NAMESPACE}" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
             <rect x="0" y="0" width="{width}" height="{height}" fill="#ffffff"/>
             <rect x="20" y="40" width="120" height="60" fill="#e8e8ff" stroke="#333333"/>
             <text x="40" y="80" font-family="sans-serif" font-size="16">start</text>
             <path d="M140 70 L 200 70" stroke="#333333" stroke-width="2"/>
           </svg>"##
    )
}

/// One cell of black at the left edge of a picture eight pixels wide: a shape whose edge
/// falls on a pixel boundary, so a picture drawn at the wrong size cannot hide it.
fn half() -> String {
    format!(
        r##"<svg xmlns="{NAMESPACE}" width="8" height="16" viewBox="0 0 8 16">
             <rect x="0" y="0" width="4" height="16" fill="#000000"/>
           </svg>"##
    )
}

/// The pixel at `(x, y)` of a raster, as the four channels a Kitty placement takes.
fn pixel(image: &image::RgbaImage, x: u32, y: u32) -> [u8; 4] {
    image.get_pixel(x, y).0
}

/// Every pixel of a raster that carries any ink at all.
fn inked(image: &image::RgbaImage) -> usize {
    image.pixels().filter(|pixel| pixel.0[3] > 0).count()
}

/// ADR-0003's first consequence: the font travels in the binary, because `usvg` converts a
/// `<text>` to paths at parse time and a database holding no font converts it to nothing —
/// every diagram's labels would vanish silently instead of failing.
///
/// The control is `usvg`'s own default database, which holds no font: the same label drawn
/// through it leaves no ink at all, and drawn through this module it is there. The
/// machine's fonts are in neither — this crate's `usvg` cannot even load them — so a
/// laptop with no fonts draws the picture a workstation with a thousand would.
#[test]
fn a_label_is_drawn_in_the_bundled_font_so_a_machine_with_no_fonts_still_shows_it() {
    let label = format!(
        r#"<svg xmlns="{NAMESPACE}" width="60" height="20" viewBox="0 0 60 20">
             <text x="2" y="15" font-family="sans-serif" font-size="14">Sindri</text>
           </svg>"#
    );

    let ours = Svg::parse(&label)
        .expect("the label parses")
        .rasterise(CellSize::new(10, 20))
        .expect("the label draws");

    assert!(
        inked(&ours) > 20,
        "the label is in the picture: {} pixels of it",
        inked(&ours)
    );

    // The control: the same SVG through a parser with nothing in its font database.
    let bare = usvg::Tree::from_str(&label, &usvg::Options::default()).expect("usvg parses it");
    let mut pixmap = tiny_skia::Pixmap::new(60, 20).expect("a 60 by 20 pixmap");
    resvg::render(&bare, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let nothing = pixmap
        .pixels()
        .iter()
        .filter(|pixel| pixel.alpha() > 0)
        .count();

    assert_eq!(
        nothing, 0,
        "with no font to convert the text with, there is nothing to draw, which is what the \
         bundled font is for"
    );
}

/// §12's measurement: the footprint is the SVG's own size over the cell's pixel size, in
/// cells, rounded up — a picture half a cell over the edge has lost that half.
#[test]
fn a_diagram_is_measured_in_cells_from_its_size_and_the_cell_it_is_drawn_at() {
    let svg = Svg::parse(&diagram(300, 200)).expect("the diagram parses");
    let footprint = svg
        .footprint(CellSize::new(10, 20))
        .expect("a diagram a transcript has room for");

    assert_eq!(
        (footprint.cols(), footprint.rows()),
        (30, 10),
        "300 by 200 pixels at a 10 by 20 cell is 30 cells by 10"
    );
    assert_eq!(
        footprint.pixels(CellSize::new(10, 20)),
        (300, 200),
        "and the pixels it is drawn at are the cells it takes times the cell"
    );

    // A size that does not divide by the cell rounds up on both axes: 301 by 201 is
    // 31 cells by 11, and nothing of the picture is dropped off the end.
    let odd = Svg::parse(&diagram(301, 201)).expect("the diagram parses");
    let footprint = odd
        .footprint(CellSize::new(10, 20))
        .expect("a diagram a transcript has room for");

    assert_eq!((footprint.cols(), footprint.rows()), (31, 11));
}

/// ADR-0003's promise about cost: the footprint is the SVG's own numbers, so it is known
/// before the renderer is touched, which is what lets a transcript decide a picture does
/// not fit without paying for the picture.
#[test]
fn a_footprint_costs_no_pixels() {
    let svg = Svg::parse(&diagram(300, 200)).expect("the diagram parses");

    let _ = svg
        .footprint(CellSize::new(10, 20))
        .expect("the diagram is measured");

    assert_eq!(
        svg.rasterises(),
        0,
        "the footprint was measured without the renderer"
    );

    // And the count can see a raster: a counter that never moved would pass the line
    // above whatever the code did.
    let pixels = svg
        .rasterise(CellSize::new(10, 20))
        .expect("the diagram draws");

    assert_eq!(svg.rasterises(), 1, "one raster, and the count knows it");
    assert_eq!(
        (pixels.width(), pixels.height()),
        (300, 200),
        "the picture is drawn at the pixels of the cells it was measured against"
    );
}

/// The picture that is drawn is the picture that was measured: the raster is made at the
/// cell's own pixels, so a shape's edge lands on a pixel boundary instead of being
/// stretched from some other size and smeared across two.
#[test]
fn the_pixels_are_made_at_the_cell_size_the_picture_was_measured_at() {
    let svg = Svg::parse(&half()).expect("the half-cell picture parses");

    let one = svg.rasterise(CellSize::new(8, 16)).expect("it draws");
    assert_eq!(
        (one.width(), one.height()),
        (8, 16),
        "an 8 by 16 picture at an 8 by 16 cell is one cell of pixels"
    );
    assert_eq!(
        pixel(&one, 3, 8),
        [0, 0, 0, 255],
        "the shape's last column is black"
    );
    assert_eq!(
        pixel(&one, 4, 8),
        [0, 0, 0, 0],
        "and the column past its edge is not touched, so the edge is where the SVG put it"
    );

    // The same picture on a bigger terminal: the pixels follow the cell the TUI probed,
    // so the edge falls on the boundary the doubled size gives it.
    let doubled = svg.rasterise(CellSize::new(16, 32)).expect("it draws");
    assert_eq!((doubled.width(), doubled.height()), (16, 32));
    assert_eq!(pixel(&doubled, 7, 16), [0, 0, 0, 255]);
    assert_eq!(pixel(&doubled, 8, 16), [0, 0, 0, 0]);
}

/// §12's refusal: a picture a transcript has no room for is turned down by the size it would
/// have taken, at the measurement — before a row is reserved for it and before a pixel of it
/// is made — and the reason is what the caption prints.
#[test]
fn a_picture_no_transcript_has_room_for_is_refused_by_its_size_and_costs_no_pixels() {
    let huge = Svg::parse(&diagram(4_000, 3_000)).expect("a huge diagram still parses");

    let refusal = huge
        .footprint(CellSize::new(10, 20))
        .expect_err("no transcript lays it out");

    assert!(
        matches!(refusal, Refusal::TooLarge { .. }),
        "a size refusal is the one reason a big picture has: {refusal}"
    );
    assert!(
        refusal.message().contains("400") && refusal.message().contains("150"),
        "the refusal says the size it wanted, so the caption can print it: {}",
        refusal.message()
    );

    let drawn = huge
        .rasterise(CellSize::new(10, 20))
        .expect_err("nothing draws it");

    assert_eq!(
        drawn, refusal,
        "and the renderer turns it down the way the measurement did"
    );
    assert_eq!(
        huge.rasterises(),
        0,
        "a picture that is not drawn costs no pixels"
    );
}

/// A picture an answer points at is wider than the band far more often than not — every
/// screenshot on a terminal narrower than the window it was taken on — so the fit is the
/// ordinary case and not a refusal: the cells it declares become the cells the band has,
/// by one scale for both axes, and what is left over goes back to the band.
#[test]
fn a_picture_wider_than_the_band_is_fitted_into_it_and_keeps_its_proportions() {
    let fitted = Footprint::fit(400, 200, CellSize::new(10, 20), 20).expect("a picture fits");

    assert_eq!(
        (fitted.cols(), fitted.rows()),
        (20, 5),
        "40 by 10 cells of picture fitted into 20 columns is 20 by 5: one scale for both axes"
    );

    // A picture the band already has room for is not stretched to fill it: a four-cell
    // picture is four cells on a screen eighty columns wide.
    let small = Footprint::fit(40, 40, CellSize::new(10, 20), 80).expect("a picture fits");

    assert_eq!(
        (small.cols(), small.rows()),
        (4, 2),
        "a picture is fitted down to a band and never up to one"
    );

    // And one taller than a transcript is ever given is cut by the rows it may have, not by
    // the columns: the ceiling is the guard between a declared size and an allocation.
    let tall = Footprint::fit(100, 100_000, CellSize::new(10, 20), 80).expect("a picture fits");

    assert!(
        tall.rows() <= 100 && tall.cols() <= 80,
        "a picture of a thousand rows takes the hundred a transcript has: {tall:?}"
    );
}

/// A picture with no extent declares nothing to draw, and it is refused at the fit for the
/// same reason a footprint refuses it: a block of no cells would be a picture that
/// disappeared without a caption saying so.
#[test]
fn a_picture_with_no_extent_at_all_is_refused_by_the_fit() {
    let refusal = Footprint::fit(0, 0, CellSize::new(10, 20), 80).expect_err("nothing to draw");

    assert_eq!(
        refusal,
        Refusal::Sizeless,
        "a picture of no size is the one reason a size can be refused before drawing"
    );
}

/// The pixels of a fitted picture are the pixels of the cells it was fitted into, and not
/// the pixels it declares: a transcript that reserved twenty columns has to be handed a
/// twenty-column picture, or the terminal stretches one into the other.
#[test]
fn a_picture_is_drawn_at_the_size_the_band_fitted_it_into() {
    let svg = Svg::parse(&diagram(400, 200)).expect("the diagram parses");
    let fitted = Footprint::fit(400, 200, CellSize::new(10, 20), 20).expect("a picture fits");

    let pixels = svg
        .rasterise_at(fitted.pixels(CellSize::new(10, 20)))
        .expect("the picture draws");

    assert_eq!(
        (pixels.width(), pixels.height()),
        (200, 100),
        "twenty cells by five at a ten by twenty cell is 200 by 100 pixels"
    );
    assert_eq!(
        (svg.size().0, svg.size().1),
        (400, 200),
        "and the size it declares is the one the fit was measured from"
    );
}

/// A thing that is not an SVG is refused with the parser's own reason, because a caption
/// that said only "no" would leave a reader guessing what a fence held.
#[test]
fn something_that_is_not_an_svg_is_refused_with_the_reason_the_parser_gave() {
    let refusal = Svg::parse("<html><body>not a picture</body></html>").expect_err("it is not one");

    assert!(
        matches!(refusal, Refusal::Unreadable { .. }),
        "an unparsable picture is that one refusal: {refusal}"
    );
    let reason = refusal
        .message()
        .strip_prefix("the picture could not be read: ")
        .expect("the message is the caption's, and the caption leads with what it is")
        .to_owned();

    assert!(
        !reason.is_empty(),
        "the message carries the parser's own reason and not only a no"
    );
}

/// The cell pixel size is a value the terminal hands the picture code, so a test draws at
/// a size of its own choosing and batch mode — which never has one — never asks (§12).
#[test]
fn a_frame_draws_at_the_cell_size_it_was_handed() {
    let mut view = View::boot();

    assert_eq!(
        view.cell_size(),
        CellSize::NOMINAL,
        "a frame nobody measured draws at the nominal cell"
    );

    view.set_cell_size(CellSize::new(9, 18));

    assert_eq!(
        view.cell_size(),
        CellSize::new(9, 18),
        "and a frame handed a terminal's own cell keeps it"
    );
}
