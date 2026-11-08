//! An SVG becomes the pixels a picture is drawn in, and is measured in cells before one is
//! spent: `usvg` parses and lays the drawing out, `resvg` and `tiny-skia` rasterise it, and
//! `image` hands the raw RGBA on to the placement that draws it (§12, ADR-0003).
//!
//! Nothing here knows what produced the SVG. A mermaid fence is the ticket above this one
//! (#54) and the Kitty placement is the ticket after it (#55); what is here is the seam
//! between them, so an SVG string goes in and a picture comes out and neither side learns
//! the other's business.
//!
//! Two promises are this module's, and both are about pixels nobody spent.
//!
//! A diagram's words are `<text>`, and `usvg` converts text to paths while it parses: a
//! font database with nothing in it does not fail, it converts the label to *no* paths and
//! the words vanish silently (ADR-0003). So one font travels in the binary and is loaded
//! into the parser's own database, and the fonts of the machine are never read — the
//! `usvg` this crate builds cannot read them, which is what makes that a fact rather than
//! an intention. A laptop with no fonts draws the picture a workstation does.
//!
//! And the size of a picture is settled without a pixel: [`usvg::Tree::size()`] is the
//! SVG's own extent, and dividing it by the cell's pixel size says how many cells the
//! picture takes. That is the number a transcript needs before it can say whether the
//! picture fits, and [`Svg::footprint`] answers it without touching the renderer.
//! [`Svg::rasterises`] counts the times the renderer was touched, so a test can hold that
//! claim to a number rather than to a reading of the code.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The font this binary carries, so a label is drawn whatever the machine has installed:
/// Liberation Sans, under the SIL Open Font License, which travels beside it in `fonts/`.
const LABEL_FONT: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");

/// The family the bundled font answers to, named the way `fontdb` filed it. A diagram that
/// names no family is set in it, and so is one whose family nothing here carries: `usvg`
/// falls back to the sans-serif face, and this is that face too.
const LABEL_FAMILY: &str = "Liberation Sans";

/// The most cells a picture may take: no terminal is 200 columns by 100 rows of a single
/// picture, and the ceiling is also the guard that stands between a hostile SVG's declared
/// size and an allocation of gigabytes — which is why it is checked on the footprint,
/// before a pixel is asked for (§12).
const MAX_COLS: u16 = 200;
const MAX_ROWS: u16 = 100;

/// The size of one terminal cell, in pixels: the yardstick a picture is measured in and
/// drawn at.
///
/// It is a value a caller hands in and never something this module asks a terminal for.
/// The terminal UI probes its own tty once before its event loop and passes the answer on
/// (ADR-0003); batch mode has no terminal and so passes nothing, and a test passes whatever
/// size makes its picture easy to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    /// How many pixels wide one cell is.
    width: u32,
    /// How many pixels tall one cell is.
    height: u32,
}

impl CellSize {
    /// The cell a terminal that will not say what it has is assumed to have: ten pixels
    /// wide and twenty tall, which is what a terminal on a 96 dpi screen set in a
    /// ten-point monospace face comes to.
    pub const NOMINAL: CellSize = CellSize {
        width: 10,
        height: 20,
    };

    /// A cell of `width` by `height` pixels. A terminal that says zero is a terminal whose
    /// size nobody can divide by, so the smallest cell this value will hold is one pixel.
    pub fn new(width: u32, height: u32) -> CellSize {
        CellSize {
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// How many pixels wide one cell is.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// How many pixels tall one cell is.
    pub fn height(&self) -> u32 {
        self.height
    }
}

impl Default for CellSize {
    /// The nominal cell, which is the answer of every terminal that was never asked.
    fn default() -> CellSize {
        CellSize::NOMINAL
    }
}

/// How much of the screen a picture takes, in cells: what a transcript lays out before it
/// knows whether the picture will be drawn at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// The columns the picture takes, rounded up.
    cols: u16,
    /// The rows the picture takes, rounded up.
    rows: u16,
}

impl Footprint {
    /// The columns the picture takes.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// The rows the picture takes.
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// The cells a picture `width` by `height` pixels takes when it is fitted into a band
    /// `room` columns across: the size a block of a transcript is laid out at, and the size
    /// the picture is then drawn at.
    ///
    /// A picture is fitted and not refused for being wide, because a picture wider than the
    /// band is the ordinary case — every screenshot on a terminal narrower than the window
    /// it was taken on — and a row that ran over its width would be a row the paint clips and
    /// whatever copies it loses. It is fitted by one scale on both axes, so it keeps the
    /// shape it was drawn in, and that scale only ever shrinks it. The columns are the
    /// scale's, rounded down to whole cells and never fewer than the one cell a picture has to
    /// have, and the rows are the ratio the picture was drawn at read at the columns it got:
    /// three cells by two hundred and fifty fitted into eighty columns is one cell by
    /// eighty-three rows, and not the hundred rows a squashed picture would have spent. The
    /// one place the shape is not kept is the ceiling — a picture needing more rows than a
    /// transcript has is as tall as a transcript allows (§12).
    ///
    /// What is still refused is a picture with no extent at all: there is nothing to divide,
    /// nothing to draw, and a block of no cells would be a picture that disappeared without
    /// a caption saying why (§12).
    pub fn fit(width: u32, height: u32, cell: CellSize, room: u16) -> Result<Footprint, Refusal> {
        let cols = cells(width as f32, cell.width());
        let rows = cells(height as f32, cell.height());
        if cols == 0 || rows == 0 {
            return Err(Refusal::Sizeless);
        }
        let room = room.clamp(1, MAX_COLS);
        let scale = (f64::from(room) / f64::from(cols))
            .min(f64::from(MAX_ROWS) / f64::from(rows))
            .min(1.0);
        let (cols, rows) = shaped(cols, rows, scale, room);
        Ok(Footprint { cols, rows })
    }

    /// The pixels this footprint is on a `cell`: the size the picture is drawn at, and the
    /// size the terminal is asked to place.
    pub fn pixels(&self, cell: CellSize) -> (u32, u32) {
        (
            u32::from(self.cols) * cell.width(),
            u32::from(self.rows) * cell.height(),
        )
    }
}

/// An SVG parsed and laid out, ready to be measured or drawn.
#[derive(Debug)]
pub struct Svg {
    /// The tree `usvg` laid out. Its own size is the picture's size, which is what a
    /// footprint is measured from and what the pixels are scaled by, so the two can never
    /// disagree about how big the picture is.
    tree: usvg::Tree,
    /// How many times this picture has been handed to the renderer: the instrument a test
    /// reads to hold "a footprint costs no pixels" to something it can count.
    rasterises: AtomicUsize,
}

impl Svg {
    /// Parses `svg`, with the font this binary carries in the parse so a diagram's labels
    /// are drawn (§12, ADR-0003).
    ///
    /// A string that is not an SVG is refused with the parser's own reason, because the
    /// caption a fence is turned down with is only useful if it says what was wrong.
    pub fn parse(svg: &str) -> Result<Svg, Refusal> {
        let tree = usvg::Tree::from_str(svg, options()).map_err(|error| Refusal::Unreadable {
            reason: error.to_string(),
        })?;
        Ok(Svg {
            tree,
            rasterises: AtomicUsize::new(0),
        })
    }

    /// The picture's own extent in pixels, as the SVG declares it: what a caller that has
    /// to fit the picture into a band measures from, before it divides by the cell.
    pub fn size(&self) -> (u32, u32) {
        let size = self.tree.size();
        (size.width() as u32, size.height() as u32)
    }

    /// How many cells of a `cell` this picture takes: the SVG's own extent divided by the
    /// cell's pixel size, rounded up on each axis, so a picture is never cut off by the half
    /// cell that did not divide.
    ///
    /// No pixel is spent to answer this, and the two sizes a picture cannot be drawn at are
    /// refused here rather than at the renderer: one with no extent at all, and one no
    /// transcript has room for. A caller that lays a picture out before drawing it — which is
    /// every caller, because the footprint is what says how many rows it takes — is told
    /// before it reserves the rows (§12).
    pub fn footprint(&self, cell: CellSize) -> Result<Footprint, Refusal> {
        let size = self.tree.size();
        let footprint = Footprint {
            cols: cells(size.width(), cell.width()),
            rows: cells(size.height(), cell.height()),
        };
        if footprint.cols() == 0 || footprint.rows() == 0 {
            return Err(Refusal::Sizeless);
        }
        if footprint.cols() > MAX_COLS || footprint.rows() > MAX_ROWS {
            return Err(Refusal::TooLarge {
                cols: footprint.cols(),
                rows: footprint.rows(),
            });
        }
        Ok(footprint)
    }

    /// The picture as the pixels a terminal is asked to place: straight RGBA, at the pixel
    /// size of the footprint it was measured into, so a shape's edge lands where the SVG
    /// put it instead of being stretched from some other size by the terminal.
    ///
    /// A picture is drawn only at a size the footprint allowed, so a picture no transcript
    /// has room for is refused before the pixels are allocated (§12).
    pub fn rasterise(&self, cell: CellSize) -> Result<image::RgbaImage, Refusal> {
        let (width, height) = self.footprint(cell)?.pixels(cell);
        self.rasterise_at((width, height))
    }

    /// The picture as `width` by `height` pixels, whatever extent it declares of its own:
    /// what a picture fitted into a band's columns is drawn at, because the footprint the band
    /// allowed is smaller than the one the SVG declares and the pixels have to follow the band
    /// rather than the drawing (§12).
    pub fn rasterise_at(&self, (width, height): (u32, u32)) -> Result<image::RgbaImage, Refusal> {
        let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(Refusal::Unrenderable)?;

        let size = self.tree.size();
        // One scale for both axes: the footprint rounds each axis up to a whole cell, and
        // scaling them apart would tilt every diagram. What the round-up leaves over is the
        // margin, and the translate is what splits it.
        let scale = (width as f32 / size.width()).min(height as f32 / size.height());
        let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(
            (width as f32 - size.width() * scale) / 2.0,
            (height as f32 - size.height() * scale) / 2.0,
        );
        resvg::render(&self.tree, transform, &mut pixmap.as_mut());
        self.rasterises.fetch_add(1, Ordering::Relaxed);

        image::RgbaImage::from_raw(width, height, pixmap.take_demultiplied())
            .ok_or(Refusal::Unrenderable)
    }

    /// How many times this picture has been rasterised: the instrument a test reads
    /// instead of the shape of this code, and the number that makes "measured without
    /// pixels" an assertion.
    pub fn rasterises(&self) -> usize {
        self.rasterises.load(Ordering::Relaxed)
    }
}

/// Why a picture is not drawn: the reason a caption prints in place of it (§12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The SVG could not be read, and the parser's own words for what was wrong with it.
    Unreadable {
        /// What `usvg` said, kept whole because it is what the caption prints.
        reason: String,
    },
    /// The picture declares no extent, so there is nothing to measure and nothing to draw.
    /// `usvg` refuses a zero size with an [`Refusal::Unreadable`] of its own and falls back
    /// to a default extent for a viewBox it cannot use, so no test can reach this today: it
    /// stands between the scale below and a division by a zero it would not survive, and it
    /// is kept for that rather than for a diagram a reader might see.
    Sizeless,
    /// The picture takes more cells than any transcript has room for.
    TooLarge {
        /// The columns it would have taken.
        cols: u16,
        /// The rows it would have taken.
        rows: u16,
    },
    /// The renderer would not make pixels of the size it was asked for.
    Unrenderable,
}

impl Refusal {
    /// The reason as the one sentence a caption prints, because a picture refused with no
    /// reason is a fence that silently disappeared.
    pub fn message(&self) -> String {
        match self {
            Refusal::Unreadable { reason } => format!("the picture could not be read: {reason}"),
            Refusal::Sizeless => {
                String::from("the picture has no size, so there is nothing to draw")
            }
            Refusal::TooLarge { cols, rows } => format!(
                "the picture needs {cols} by {rows} cells and no transcript has room for it"
            ),
            Refusal::Unrenderable => {
                String::from("the picture could not be drawn at the size it takes")
            }
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message())
    }
}

/// The options every parse is done with, made once: a run that draws a hundred pictures
/// reads the bundled font into a database once rather than a hundred times.
///
/// `usvg`'s own default holds an empty font database and this adds one font to it — there
/// is no call here that would load the machine's, and this crate's `usvg` is built without
/// the feature that could. That is the whole of the promise that a machine with no fonts
/// draws the same diagram as one with a thousand (ADR-0003).
fn options() -> &'static usvg::Options<'static> {
    static OPTIONS: OnceLock<usvg::Options<'static>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut options = usvg::Options {
            font_family: LABEL_FAMILY.to_owned(),
            ..usvg::Options::default()
        };
        let database = options.fontdb_mut();
        database.load_font_data(LABEL_FONT.to_vec());
        database.set_sans_serif_family(LABEL_FAMILY);
        options
    })
}

/// The whole cells a picture drawn `cols` by `rows` takes when it is scaled by `scale` into
/// a band `room` columns across, with the shape it was drawn in kept.
///
/// The columns are the scale's, rounded down to the whole cells a band can be filled with.
/// The rows are the ratio the picture was drawn at, read at the columns there are and not at
/// the columns the scale asked for: three cells by two hundred and fifty fitted into eighty
/// is one column, and one column of that picture is eighty-three rows — a hundred would be the
/// same picture squashed by a fifth, which is the thing one scale is meant to prevent. So a
/// picture is never taller than the shape it was drawn in, and the only thing that can still
/// press on it is the ceiling: a picture needing more rows than a transcript has is as tall as
/// the transcript allows (§12).
fn shaped(cols: u16, rows: u16, scale: f64, room: u16) -> (u16, u16) {
    let ratio = f64::from(rows) / f64::from(cols);
    let columns = fitted(f64::from(cols) * scale, room);
    (columns, fitted(f64::from(columns) * ratio, MAX_ROWS))
}

/// How many whole cells a fitted picture takes, rounded down so a fit never asks for a
/// column the band does not have, and never fewer than one so a picture fitted very small is
/// still a block of the transcript rather than a gap in it.
fn fitted(cells: f64, ceiling: u16) -> u16 {
    let whole = cells.floor().max(1.0);
    u16::try_from(whole as u64)
        .unwrap_or(ceiling)
        .min(ceiling)
        .max(1)
}

/// How many cells one axis of `pixels` takes at `cell` pixels a cell, rounded up, and zero
/// only for a picture with no extent at all.
fn cells(pixels: f32, cell: u32) -> u16 {
    let whole = (pixels / cell as f32).ceil();
    if whole < 1.0 {
        0
    } else if whole >= f32::from(u16::MAX) {
        u16::MAX
    } else {
        whole as u16
    }
}
