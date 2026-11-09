//! An SVG becomes the pixels a picture is drawn in: `usvg` parses and lays it out, `resvg` and
//! `tiny-skia` rasterise it, and `image` hands the RGBA on to the placement that draws it.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Liberation Sans, under the SIL Open Font License, which travels beside it in `fonts/`: the
/// one face a diagram's labels are set in, whatever the machine has installed.
const LABEL_FONT: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");

/// The family the bundled font is filed under. A drawing naming a family nothing here
/// carries falls back to the sans-serif face, so this is that face too.
const LABEL_FAMILY: &str = "Liberation Sans";

/// The most cells one picture may take, on either axis: no terminal is two hundred columns of
/// a single picture, and no transcript a hundred rows. The ceiling is also what stands between
/// a hostile SVG's declared size and an allocation of gigabytes, before a pixel is asked for.
const MAX_COLS: u16 = 200;
const MAX_ROWS: u16 = 100;

/// The size of one terminal cell in pixels, handed in by the caller: nothing here asks a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    width: u32,
    height: u32,
}

impl CellSize {
    /// What a terminal that will not say what cell it has is assumed to have: ten by twenty.
    pub const NOMINAL: CellSize = CellSize {
        width: 10,
        height: 20,
    };

    /// A cell of `width` by `height` pixels, and never zero of either: a caller divides by it.
    pub fn new(width: u32, height: u32) -> CellSize {
        CellSize {
            width: width.max(1),
            height: height.max(1),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

impl Default for CellSize {
    fn default() -> CellSize {
        CellSize::NOMINAL
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    cols: u16,
    rows: u16,
}

impl Footprint {
    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// The cells a picture `width` by `height` pixels takes fitted into a band `room` columns
    /// across: one scale on both axes, shrunk and never grown, so the shape is kept. The
    /// ceiling on rows is the one place it gives way, and a picture with no extent is refused.
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

    pub fn pixels(&self, cell: CellSize) -> (u32, u32) {
        (
            u32::from(self.cols) * cell.width(),
            u32::from(self.rows) * cell.height(),
        )
    }
}

#[derive(Debug)]
pub struct Svg {
    /// The tree `usvg` laid out. Its own size is the picture's size, so what a footprint measures
    /// and what the pixels are scaled by can never disagree about how big the picture is.
    tree: usvg::Tree,
    /// How many times this picture has been handed to the renderer: the instrument a test reads.
    rasterises: AtomicUsize,
}

impl Svg {
    pub fn parse(svg: &str) -> Result<Svg, Refusal> {
        let tree = usvg::Tree::from_str(svg, options()).map_err(|error| Refusal::Unreadable {
            reason: error.to_string(),
        })?;
        Ok(Svg {
            tree,
            rasterises: AtomicUsize::new(0),
        })
    }

    pub fn size(&self) -> (u32, u32) {
        let size = self.tree.size();
        (size.width() as u32, size.height() as u32)
    }

    /// The cells of a `cell` this picture takes, rounded up on each axis and spent without a
    /// pixel: a picture with no extent, and one no transcript has room for, are refused here.
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

    /// The picture as RGBA at the pixel size of the footprint it was measured into: a shape's
    /// edge lands where the SVG put it instead of being stretched there by the terminal.
    pub fn rasterise(&self, cell: CellSize) -> Result<image::RgbaImage, Refusal> {
        let (width, height) = self.footprint(cell)?.pixels(cell);
        self.rasterise_at((width, height))
    }

    pub fn rasterise_at(&self, (width, height): (u32, u32)) -> Result<image::RgbaImage, Refusal> {
        let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(Refusal::Unrenderable)?;

        let size = self.tree.size();
        // One scale for both axes: the footprint rounds each axis up to a whole cell, and
        // scaling them apart would tilt every diagram. The translate splits what is left over.
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

    pub fn rasterises(&self) -> usize {
        self.rasterises.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Unreadable {
        reason: String,
    },
    /// Unreachable today — `usvg` refuses a zero size and falls back for a viewBox it cannot
    /// use — and kept as the guard between the fit's scale and a division by zero.
    Sizeless,
    TooLarge {
        cols: u16,
        rows: u16,
    },
    Unrenderable,
}

impl Refusal {
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

/// The options every parse is done with, made once so a hundred pictures load the font once.
/// A font database with nothing in it does not fail: `usvg` converts a label to no paths and it
/// vanishes. So one bundled font is loaded, and this crate's `usvg` cannot read the machine's.
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

/// The whole cells a scaled picture takes: its rows are the drawn ratio read at the columns it
/// actually got, not the ones the scale asked for, so a picture is never taller than its shape.
fn shaped(cols: u16, rows: u16, scale: f64, room: u16) -> (u16, u16) {
    let ratio = f64::from(rows) / f64::from(cols);
    let columns = fitted(f64::from(cols) * scale, room);
    (columns, fitted(f64::from(columns) * ratio, MAX_ROWS))
}

/// How many whole cells a fitted picture takes: never zero, because a picture fitted down to a
/// sliver is still a cell of the transcript rather than a gap in it.
fn fitted(cells: f64, ceiling: u16) -> u16 {
    let whole = cells.floor().max(1.0);
    u16::try_from(whole as u64)
        .unwrap_or(ceiling)
        .min(ceiling)
        .max(1)
}

/// How many cells one axis of `pixels` takes at `cell` pixels a cell, rounded up, and zero only
/// for a picture with no extent at all.
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
