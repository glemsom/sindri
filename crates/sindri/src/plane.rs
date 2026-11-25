use base64::Engine;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::face::Motion;
use crate::graphics::CellSize;
use crate::ink::Ink;
use crate::instrument;
use crate::ledger::{Rail, RailBand};
use crate::palette::{Band, Palette, channels};
use crate::pen;
use crate::rows::MARGIN;

const EDGE_RULE: char = '│';

/// The cells the caret's light spans, centred on the caret's own: the fall is spent a dozen
/// cells out, so the box the light is drawn in leaves no step of its own.
pub const LIGHT_COLS: u16 = 25;

/// The cells either side of the caret the light it casts is spent over: the fall is gone by the
/// box it is drawn in, so the box leaves no step of its own.
const LIGHT_REACH: u16 = 12;

/// The placement identity every image is put under. An image whose light moved is put under the
/// identity it stood by before, so the terminal replaces it and no pixel of it travels.
const PLACEMENT: u32 = 1;

/// The depth every plane is placed at. Kitty draws an image under a cell that carries a
/// background only below `INT32_MIN / 2`, and a seam's edge ink is one such cell.
const DEPTH: i64 = i32::MIN as i64 / 2 - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneId {
    Composer,
    Rail,
    Transcript,
    Caret,
    Face,
    Gauge,
    Bar(Bar),
    Bezel,
    Prompt(u32),
    Tool(u32),
}

impl PlaneId {
    const ALL: [PlaneId; 9] = [
        PlaneId::Composer,
        PlaneId::Rail,
        PlaneId::Transcript,
        PlaneId::Caret,
        PlaneId::Face,
        PlaneId::Gauge,
        PlaneId::Bar(Bar::Input),
        PlaneId::Bar(Bar::Output),
        PlaneId::Bezel,
    ];

    /// The identity the image is held under and layered by: two images at the frame's one depth
    /// stand by identity, so the caret's light is read over the band it lights.
    fn image(self) -> u32 {
        match self {
            PlaneId::Composer => 333,
            PlaneId::Rail => 334,
            PlaneId::Transcript => 335,
            PlaneId::Caret => 336,
            PlaneId::Face => 337,
            PlaneId::Gauge => 338,
            PlaneId::Bar(Bar::Input) => 339,
            PlaneId::Bar(Bar::Output) => 340,
            PlaneId::Bezel => 341,
            PlaneId::Prompt(id) => 0x8000_0000 | id,
            PlaneId::Tool(id) => 0x4000_0000 | id,
        }
    }
}

/// What a band sits on: the facts its plane is drawn from and no word about where it stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shade {
    kind: Kind,
}

/// The cells a plane's image spans: a band's size is a fact its pixels are drawn from, where the
/// cells it stands on are the placement's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Size {
    cols: u16,
    rows: u16,
}

impl Size {
    fn of(area: Rect) -> Size {
        Size {
            cols: area.width,
            rows: area.height,
        }
    }

    fn pixels(self, cell: CellSize) -> (u32, u32) {
        (
            u32::from(self.cols) * cell.width(),
            u32::from(self.rows) * cell.height(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Bounded {
        owner: PlaneId,
        size: Size,
        full_rows: usize,
        first_row: usize,
        cell: CellSize,
        tone: Color,
        well: Color,
        accent: Color,
        peak: u8,
        weight: u8,
    },
    Transcript {
        size: Size,
        cell: Option<CellSize>,
        well: Color,
        peak: u8,
    },
    Rail {
        size: Size,
        cell: Option<CellSize>,
        tone: Color,
        face: Option<Rect>,
        peak: u8,
        seam: Option<Seam>,
    },
    Composer {
        size: Size,
        cell: Option<CellSize>,
        prompt: Color,
        rail: Color,
        seam: Option<Seam>,
    },
    /// The row a gauge's share is carried by: the pixels at every hundredth of a block `size.cols`
    /// cells wide, the ink the fact is stated in beside the ink it turns to as the share moved —
    /// which band is shown is the placement's, so a gauge that turned sends no pixel.
    Gauge {
        size: Size,
        cell: CellSize,
        tone: Color,
        calm: Color,
        accent: Color,
        tick: Color,
        peak: u8,
    },
    /// The row a figure's share is carried by: the pixels at every hundredth of a block `size.cols`
    /// cells wide.
    Bar {
        size: Size,
        cell: CellSize,
        bar: Bar,
        tone: Color,
        ink: Color,
        peak: u8,
    },
    /// The bezel's own pixels, its top and left edges taken `weight` channels into shadow and its
    /// bottom and right edges lifted by them.
    Bezel {
        size: Size,
        cell: CellSize,
        tone: Color,
        weight: u8,
    },
    /// The light the caret casts, drawn as an image of its own: the one plane whose placement moves
    /// while the pixels it stands from do not.
    Caret {
        size: Size,
        cell: CellSize,
        prompt: Color,
        rail: Color,
        peak: u8,
    },
}

impl Shade {
    pub fn id(&self) -> PlaneId {
        match self.kind {
            Kind::Bounded { owner, .. } => owner,
            Kind::Transcript { .. } => PlaneId::Transcript,
            Kind::Rail { .. } => PlaneId::Rail,
            Kind::Composer { .. } => PlaneId::Composer,
            Kind::Caret { .. } => PlaneId::Caret,
            Kind::Gauge { .. } => PlaneId::Gauge,
            Kind::Bar { bar, .. } => PlaneId::Bar(bar),
            Kind::Bezel { .. } => PlaneId::Bezel,
        }
    }

    pub fn row(&self, row: u16) -> Color {
        match self.kind {
            Kind::Bounded { tone, .. } => tone,
            Kind::Transcript { well, .. } => well,
            Kind::Rail { tone, .. }
            | Kind::Gauge { tone, .. }
            | Kind::Bar { tone, .. }
            | Kind::Bezel { tone, .. } => tone,
            Kind::Composer {
                size, prompt, rail, ..
            } => match closes(row, size.rows) {
                true => rail,
                false => prompt,
            },
            Kind::Caret {
                size, prompt, rail, ..
            } => match closes(row, size.rows) {
                true => rail,
                false => prompt,
            },
        }
    }

    /// A band a palette spells no colour for is drawn in cells alone, so its shade has no cell and
    /// no plane stands under it.
    fn places(&self) -> bool {
        self.cell().is_some()
    }

    fn size(&self) -> Size {
        match self.kind {
            Kind::Bounded { size, .. }
            | Kind::Transcript { size, .. }
            | Kind::Rail { size, .. }
            | Kind::Composer { size, .. }
            | Kind::Gauge { size, .. }
            | Kind::Bar { size, .. }
            | Kind::Bezel { size, .. }
            | Kind::Caret { size, .. } => size,
        }
    }

    /// The joint the plane is drawn parted on, and nothing where this shade parts nothing.
    fn seam(&self) -> Option<Seam> {
        match self.kind {
            Kind::Rail { seam, .. } | Kind::Composer { seam, .. } => seam,
            Kind::Bounded { .. }
            | Kind::Transcript { .. }
            | Kind::Caret { .. }
            | Kind::Gauge { .. }
            | Kind::Bar { .. }
            | Kind::Bezel { .. } => None,
        }
    }

    fn cell(&self) -> Option<CellSize> {
        match self.kind {
            Kind::Transcript { cell, .. }
            | Kind::Rail { cell, .. }
            | Kind::Composer { cell, .. } => cell,
            Kind::Bounded { cell, .. }
            | Kind::Caret { cell, .. }
            | Kind::Gauge { cell, .. }
            | Kind::Bar { cell, .. }
            | Kind::Bezel { cell, .. } => Some(cell),
        }
    }

    fn rung(&self, row: u16) -> [u8; 3] {
        channels(self.row(row)).unwrap_or([0; 3])
    }

    /// The light this shade's pixels stand in, at the plane's own pixel origin.
    fn light(&self) -> Light {
        let Some(cell) = self.cell() else {
            return Light::NONE;
        };
        let (across, down) = (f64::from(cell.width()), f64::from(cell.height()));
        let light = match self.kind {
            Kind::Transcript { peak, .. } => Light::lamp(
                peak,
                At { x: 0.0, y: 0.0 },
                At {
                    x: f64::INFINITY,
                    y: CROWN_ROWS * down,
                },
            ),
            Kind::Rail {
                size,
                face: Some(face),
                peak,
                ..
            } => {
                let centre = At {
                    x: (f64::from(face.x) + f64::from(face.width) / 2.0) * across,
                    y: (f64::from(face.y) + f64::from(face.height) / 2.0) * down,
                };
                // The light is spent a margin out on each axis: at the rail's own margin
                // across, and a margin past the rows the face stands on.
                let width = f64::from(size.cols) * across;
                let reach = At {
                    x: (centre.x.min(width - centre.x) - f64::from(MARGIN) * across).max(1.0),
                    y: (f64::from(face.height) / 2.0 + f64::from(MARGIN)) * down,
                };
                Light::lamp(peak, centre, reach)
            }
            Kind::Bounded { .. } | Kind::Rail { .. } | Kind::Composer { .. } => Light::NONE,
            Kind::Gauge { .. } | Kind::Bar { .. } | Kind::Bezel { .. } => Light::NONE,
            Kind::Caret { size, peak, .. } => Light::lamp(
                peak,
                At {
                    x: (f64::from(LIGHT_COLS / 2) + 0.5) * across,
                    y: 0.0,
                },
                At {
                    x: f64::from(LIGHT_REACH) * across,
                    y: f64::from(size.rows.saturating_sub(1).max(1)) * down,
                },
            ),
        };
        match self.seam() {
            Some(seam) => light.parted(seam, cell),
            None => light,
        }
    }

    /// The pixels this shade is drawn as, and none where the band is drawn in cells alone.
    fn pixels(&self) -> Option<image::RgbaImage> {
        let cell = self.cell()?;
        let size = self.size();
        match self.kind {
            Kind::Bounded {
                owner,
                full_rows,
                first_row,
                tone,
                well,
                accent,
                peak,
                weight,
                ..
            } => {
                let (width, height) = size.pixels(cell);
                let full_height = full_rows as u64 * u64::from(cell.height());
                let first_pixel = first_row as u64 * u64::from(cell.height());
                let radius = (cell.height() / 4).max(1) as f64;
                let tone = channels(tone)?;
                let well = channels(well)?;
                let accent = channels(accent)?;
                return Some(image::RgbaImage::from_fn(width, height, |x, y| {
                    let y = u64::from(y) + first_pixel;
                    let dx = f64::from(x.min(width - 1 - x));
                    let dy = y.min(full_height - 1 - y) as f64;
                    let corner =
                        ((radius - dx).max(0.0).powi(2) + (radius - dy).max(0.0).powi(2)).sqrt();
                    if corner > radius {
                        return image::Rgba(opaque(well));
                    }
                    let edge = dx.min(dy);
                    if edge < 1.0 || corner > radius - 1.0 {
                        let accent = if matches!(owner, PlaneId::Tool(_))
                            && x >= cell.width().saturating_mul(2)
                        {
                            tone
                        } else {
                            accent
                        };
                        return image::Rgba(lit(tone, accent, peak));
                    }
                    let light =
                        f64::from(peak) * (-(y as f64) / f64::from(cell.height().max(1))).exp();
                    let shadow = f64::from(weight) * (-((full_height - 1 - y) as f64) / 2.0).exp();
                    let spend = dithered(light - shadow, (x, (y % 8) as u32));
                    image::Rgba(opaque(
                        tone.map(|at| (i32::from(at) + spend).clamp(0, 255) as u8),
                    ))
                }));
            }
            Kind::Gauge {
                tone,
                calm,
                accent,
                tick,
                peak,
                ..
            } => {
                return Some(gauge_pixels(
                    size.cols,
                    cell,
                    channels(tone)?,
                    [channels(calm)?, channels(accent)?],
                    channels(tick)?,
                    peak,
                ));
            }
            Kind::Bar {
                tone, ink, peak, ..
            } => {
                return Some(bar_pixels(
                    size.cols,
                    cell,
                    channels(tone)?,
                    channels(ink)?,
                    peak,
                ));
            }
            Kind::Bezel { tone, weight, .. } => {
                return Some(bezel_pixels(size, cell, channels(tone)?, weight));
            }
            _ => {}
        }
        let (width, height) = size.pixels(cell);
        let down = cell.height();
        let light = self.light();
        Some(image::RgbaImage::from_fn(width, height, |x, y| {
            let row = u16::try_from(y / down).unwrap_or(u16::MAX).min(size.rows);
            let lit = dithered(light.at(f64::from(x), f64::from(y)), (x, y));
            let [red, green, blue] = self
                .rung(row)
                .map(|at| (i32::from(at) + lit).clamp(0, 255) as u8);
            image::Rgba([red, green, blue, 255])
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Down,
    Across,
}

/// The joint a band is parted from: the axis the seam runs on, and the channels its shadow takes
/// from the band's rung at the joint and its hairline lifts back over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seam {
    axis: Axis,
    weight: u8,
}

impl Seam {
    fn across(palette: &Palette) -> Seam {
        Seam {
            axis: Axis::Across,
            weight: palette.seam(),
        }
    }

    /// The point `along` pixels from the joint, in the pixels of the image a shade is drawn as.
    fn at(self, along: f64) -> At {
        match self.axis {
            Axis::Down => At { x: along, y: 0.0 },
            Axis::Across => At { x: 0.0, y: along },
        }
    }

    /// A reach of `reach` pixels on the seam's own axis, and no bound on the other: the fall
    /// crosses the seam and never runs along it.
    fn reach(self, reach: f64) -> At {
        match self.axis {
            Axis::Down => At {
                x: reach,
                y: f64::INFINITY,
            },
            Axis::Across => At {
                x: f64::INFINITY,
                y: reach,
            },
        }
    }
}

/// The depths an instrument's image is drawn at: one for every hundredth of the block's width, one
/// pixel deep, which the terminal stretches down the row the image is placed on.
const HUNDREDTHS: u32 = 101;

/// A pixel an instrument leaves to the band it is drawn over.
const NOTHING: [u8; 4] = [0, 0, 0, 0];

/// The hundredth of the block an image drawn at `depth` stands for.
fn hundredth(depth: u32) -> instrument::Share {
    instrument::Share::of(u8::try_from(depth).unwrap_or(100))
}

fn gauge_pixels(
    cols: u16,
    cell: CellSize,
    tone: [u8; 3],
    bands: [[u8; 3]; 2],
    tick: [u8; 3],
    peak: u8,
) -> image::RgbaImage {
    let width = u32::from(cols) * cell.width();
    let quarter = (width / 4).max(1);
    image::RgbaImage::from_fn(width * 2, HUNDREDTHS, |x, depth| {
        let (ink, x) = match x < width {
            true => (bands[0], x),
            false => (bands[1], x - width),
        };
        let fill = hundredth(depth).pixels(width);
        image::Rgba(match x < fill {
            true => lit(tone, ink, spend_of_edge(peak, x, fill)),
            false if x > 0 && x % quarter == 0 => lit(tone, tick, peak / 3),
            false => NOTHING,
        })
    })
}

fn bar_pixels(
    cols: u16,
    cell: CellSize,
    tone: [u8; 3],
    ink: [u8; 3],
    peak: u8,
) -> image::RgbaImage {
    let width = u32::from(cols) * cell.width();
    image::RgbaImage::from_fn(width, HUNDREDTHS, |x, depth| {
        let fill = hundredth(depth).pixels(width);
        image::Rgba(match x < fill {
            true => lit(tone, ink, peak),
            false => NOTHING,
        })
    })
}

fn bezel_pixels(size: Size, cell: CellSize, tone: [u8; 3], weight: u8) -> image::RgbaImage {
    let (width, height) = size.pixels(cell);
    image::RgbaImage::from_fn(width, height, |x, y| {
        image::Rgba(
            match (y == 0 || x == 0, y + 1 == height || x + 1 == width) {
                (true, _) => darkened(tone, weight),
                (false, true) => lifted(tone, weight),
                (false, false) => NOTHING,
            },
        )
    })
}

/// The light a gauge's fill spends at the pixel `x` of a fill `fill` pixels deep: three quarters of
/// the rail's peak, and all of it on the trace that ends the fill.
fn spend_of_edge(peak: u8, x: u32, fill: u32) -> u8 {
    match x + 1 == fill {
        true => peak,
        false => peak / 4 * 3,
    }
}

/// The rail's `tone` with `spend` channels of light added in the hue of the `ink` the fact is stated
/// in, and never the ink itself: the palette proves the most a rung takes under every ink it spells,
/// so the words over an instrument keep their floor.
fn lit(tone: [u8; 3], ink: [u8; 3], spend: u8) -> [u8; 4] {
    let hue = u16::from(ink.into_iter().max().unwrap_or(1).max(1));
    let lit: [u8; 3] = std::array::from_fn(|channel| {
        let share = u16::from(spend) * u16::from(ink[channel]) / hue;
        tone[channel].saturating_add(u8::try_from(share).unwrap_or(u8::MAX))
    });
    [lit[0], lit[1], lit[2], 255]
}

fn opaque(ink: [u8; 3]) -> [u8; 4] {
    [ink[0], ink[1], ink[2], 255]
}

fn lifted(ink: [u8; 3], weight: u8) -> [u8; 4] {
    opaque(ink.map(|channel| channel.saturating_add(weight)))
}

fn darkened(ink: [u8; 3], weight: u8) -> [u8; 4] {
    opaque(ink.map(|channel| channel.saturating_sub(weight)))
}

/// The rows the transcript band's light falls over: a crown spent by the band's third row, where
/// the prose under it is read on the well.
const CROWN_ROWS: f64 = 2.0;

/// How far into its reach a lamp has fallen when it is spent: under half a channel of any peak
/// the palette hands a band, so what the dither rounds up at the rim is a pixel and no step.
const SPENT: f64 = 4.0;

/// The thresholds a pixel's light is quantised through, eight by eight and a sixty-fourth apart:
/// a fall of a few levels over a band is dithered, and no band of its own shows.
const THRESHOLDS: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// A point in the pixels of the image a light is drawn as.
#[derive(Clone, Copy, Debug)]
struct At {
    x: f64,
    y: f64,
}

/// The light a band carries: the lamp its own light stands on, and the joint it is parted from.
#[derive(Clone, Copy, Debug)]
struct Light {
    lamp: Lamp,
    shadow: Lamp,
    hairline: Lamp,
}

/// One lamp: the light it stands at, negative where it takes light away, the point it is centred on
/// and the reach it is spent over on each axis.
#[derive(Clone, Copy, Debug)]
struct Lamp {
    light: f64,
    centre: At,
    reach: At,
}

impl Lamp {
    /// A lamp that stands nowhere, which a pixel reads as no light at all.
    const NONE: Lamp = Lamp {
        light: 0.0,
        centre: At { x: 0.0, y: 0.0 },
        reach: At { x: 1.0, y: 1.0 },
    };

    fn new(light: f64, centre: At, reach: At) -> Lamp {
        Lamp {
            light,
            centre,
            reach,
        }
    }

    /// The light at a pixel: all of it at the lamp's centre, and nothing at all past the reach.
    fn at(self, x: f64, y: f64) -> f64 {
        let across = (x - self.centre.x) / self.reach.x;
        let down = (y - self.centre.y) / self.reach.y;
        let away = across * across + down * down;
        match away < 1.0 {
            true => self.light * (-(SPENT * away)).exp(),
            false => 0.0,
        }
    }
}

impl Light {
    /// A band with nothing standing on it: no lamp, and no joint to be parted from.
    const NONE: Light = Light {
        lamp: Lamp::NONE,
        shadow: Lamp::NONE,
        hairline: Lamp::NONE,
    };

    /// A lamp at `centre`, spent over `reach` on each axis: a reach of no bound is an axis the
    /// light does not fall on, and one past the reach is where it is exactly nothing.
    fn lamp(peak: u8, centre: At, reach: At) -> Light {
        Light {
            lamp: Lamp::new(f64::from(peak), centre, reach),
            ..Light::NONE
        }
    }

    /// This light with its band parted from the one beside it: the shadow the seam's cell falls
    /// into at the joint, spent by its middle, and the hairline its last pixel lifts over the band.
    fn parted(self, seam: Seam, cell: CellSize) -> Light {
        let extent = f64::from(match seam.axis {
            Axis::Down => cell.width(),
            Axis::Across => (cell.height() / 2).max(1),
        });
        let weight = f64::from(seam.weight);
        Light {
            shadow: Lamp::new(-weight, seam.at(0.0), seam.reach(extent / 2.0)),
            hairline: Lamp::new(weight, seam.at(extent - 1.0), seam.reach(0.5)),
            ..self
        }
    }

    /// The light at a pixel: what each of the lamps this band carries stands there.
    fn at(self, x: f64, y: f64) -> f64 {
        self.lamp.at(x, y) + self.shadow.at(x, y) + self.hairline.at(x, y)
    }
}

/// The light a pixel carries in the channels it is drawn in: the ordered matrix's threshold is
/// added to its magnitude before the floor, so a light that is no whole channel is dithered into a
/// pixel of its own and no rung beside it is blended, and a shadow is dithered as a light is.
fn dithered(light: f64, (x, y): (u32, u32)) -> i32 {
    let down = usize::try_from(y % 8).unwrap_or(0);
    let across = usize::try_from(x % 8).unwrap_or(0);
    let threshold = (f64::from(THRESHOLDS[down][across]) + 0.5) / 64.0;
    let whole = (light.abs() + threshold).floor().min(255.0) as i32;
    match light < 0.0 {
        true => -whole,
        false => whole,
    }
}

/// A shade and the cells it stands on: the unit a terminal is told about in one push.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    shade: Shade,
    cells: Rect,
    shows: Option<image::math::Rect>,
}

pub(crate) struct SurfaceWindow {
    pub rows: usize,
    pub skip: usize,
    pub band_rows: u16,
}

pub(crate) struct ToolStyle {
    pub ink: Ink,
    pub compact: bool,
}

impl Placement {
    pub(crate) fn prompt(
        id: u32,
        cells: Rect,
        rows: usize,
        skip: usize,
        band_rows: u16,
        cell: Option<CellSize>,
        palette: &Palette,
    ) -> Option<Placement> {
        let cell = cell?;
        let tone = palette.band(Band::Prompt);
        channels(tone)?;
        if !covers(cells) || rows == 0 || id >= 0x8000_0000 {
            return None;
        }
        let reach = usize::from(band_rows.max(cells.height).max(1));
        let first_row = (skip / reach).saturating_sub(1) * reach;
        let raster_rows = rows.saturating_sub(first_row).min(reach.saturating_mul(3));
        Some(Placement {
            shade: Shade {
                kind: Kind::Bounded {
                    owner: PlaneId::Prompt(id),
                    size: Size {
                        cols: cells.width,
                        rows: u16::try_from(raster_rows).ok()?,
                    },
                    full_rows: rows,
                    first_row,
                    cell,
                    tone,
                    well: palette.band(Band::Well),
                    accent: palette.ink(Ink::Accent).0,
                    peak: palette.peak(Band::Prompt),
                    weight: palette.seam() / 3,
                },
            },
            cells,
            shows: Some(image::math::Rect {
                x: 0,
                y: u32::try_from(skip - first_row).ok()? * cell.height(),
                width: u32::from(cells.width) * cell.width(),
                height: u32::from(cells.height) * cell.height(),
            }),
        })
    }

    /// The transcript band: the well its prose is read on, lit at its head.
    pub fn transcript(area: Rect, cell: Option<CellSize>, palette: &Palette) -> Option<Placement> {
        if !covers(area) {
            return None;
        }
        let well = palette.band(Band::Well);
        Some(Placement::whole(
            area,
            Shade {
                kind: Kind::Transcript {
                    size: Size::of(area),
                    cell: cell.filter(|_| channels(well).is_some()),
                    well,
                    peak: palette.peak(Band::Well),
                },
            },
        ))
    }

    pub(crate) fn tool(
        id: u32,
        cells: Rect,
        window: SurfaceWindow,
        cell: Option<CellSize>,
        palette: &Palette,
        style: ToolStyle,
    ) -> Option<Placement> {
        if id >= 0x4000_0000 {
            return None;
        }
        let SurfaceWindow {
            rows,
            skip,
            band_rows,
        } = window;
        let ToolStyle { ink, compact } = style;
        let mut at = Self::prompt(id, cells, rows, skip, band_rows, cell, palette)?;
        if let Kind::Bounded {
            owner,
            tone,
            accent,
            peak,
            weight,
            ..
        } = &mut at.shade.kind
        {
            *owner = PlaneId::Tool(id);
            *tone = palette.band(Band::Rail);
            *accent = palette.ink(ink).0;
            *peak = palette.peak(Band::Rail) / 3;
            *weight = if compact { 0 } else { palette.seam() / 6 };
        }
        Some(at)
    }

    /// The composer's band: the draft's rows on the prompt's tone, closed on the rail's own.
    pub fn composer(area: Rect, cell: Option<CellSize>, palette: &Palette) -> Option<Placement> {
        if !covers(area) {
            return None;
        }
        let prompt = palette.band(Band::Prompt);
        let rail = palette.band(Band::Rail);
        let spelled = channels(prompt).is_some() && channels(rail).is_some();
        Some(Placement::whole(
            area,
            Shade {
                kind: Kind::Composer {
                    size: Size::of(area),
                    cell: cell.filter(|_| spelled),
                    prompt,
                    rail,
                    seam: Some(Seam::across(palette)),
                },
            },
        ))
    }

    pub fn rail(
        shape: Rail,
        band: RailBand,
        cell: Option<CellSize>,
        palette: &Palette,
    ) -> Option<Placement> {
        if !covers(band.area) {
            return None;
        }
        let tone = palette.band(Band::Rail);
        let face = band.picture.map(|picture| Rect {
            x: picture.x.saturating_sub(band.area.x),
            y: picture.y.saturating_sub(band.area.y),
            ..picture
        });
        Some(Placement::whole(
            band.area,
            Shade {
                kind: Kind::Rail {
                    size: Size::of(band.area),
                    cell: cell.filter(|_| channels(tone).is_some()),
                    tone,
                    face,
                    peak: palette.peak(Band::Rail),
                    seam: seam_of(shape, palette),
                },
            },
        ))
    }

    /// An instrument stands over the cells of a fact's row and claims none of them: its pixels
    /// are the only surface it adds, and the row's own shade has claimed the rest.
    pub fn gauge(
        area: Rect,
        cell: Option<CellSize>,
        gauge: instrument::Gauge,
        palette: &Palette,
    ) -> Option<Placement> {
        let cell = cell.filter(|_| covers(area))?;
        let tone = palette.band(Band::Rail);
        let calm = palette.colour(Ink::Plain);
        let accent = palette.colour(Ink::Accent);
        let tick = palette.colour(Ink::Dim);
        if [tone, calm, accent, tick]
            .iter()
            .any(|colour| channels(*colour).is_none())
        {
            return None;
        }
        let width = u32::from(area.width) * cell.width();
        Some(Placement {
            shade: Shade {
                kind: Kind::Gauge {
                    size: Size {
                        cols: area.width,
                        rows: 1,
                    },
                    cell,
                    tone,
                    calm,
                    accent,
                    tick,
                    peak: palette.peak(Band::Rail),
                },
            },
            cells: area,
            shows: Some(image::math::Rect {
                x: match gauge.turned {
                    true => width,
                    false => 0,
                },
                y: u32::from(gauge.share.hundredth()),
                width,
                height: 1,
            }),
        })
    }

    pub fn bar(
        area: Rect,
        cell: Option<CellSize>,
        bar: Bar,
        share: instrument::Share,
        palette: &Palette,
    ) -> Option<Placement> {
        let cell = cell.filter(|_| covers(area))?;
        let tone = palette.band(Band::Rail);
        let ink = palette.colour(Ink::Plain);
        if [tone, ink].iter().any(|colour| channels(*colour).is_none()) {
            return None;
        }
        Some(Placement {
            shade: Shade {
                kind: Kind::Bar {
                    size: Size {
                        cols: area.width,
                        rows: 1,
                    },
                    cell,
                    bar,
                    tone,
                    ink,
                    peak: palette.peak(Band::Rail),
                },
            },
            cells: area,
            shows: Some(image::math::Rect {
                x: 0,
                y: u32::from(share.hundredth()),
                width: u32::from(area.width) * cell.width(),
                height: 1,
            }),
        })
    }

    pub fn bezel(area: Rect, cell: Option<CellSize>, palette: &Palette) -> Option<Placement> {
        let cell = cell.filter(|_| covers(area))?;
        let tone = palette.band(Band::Rail);
        channels(tone)?;
        // The groove is light taken from the rung and given back to it, and the most a plane under
        // a word may take is the most the palette proves: the seam's own weight where it fits.
        let weight = palette.seam().min(palette.peak(Band::Rail));
        Some(Placement::whole(
            area,
            Shade {
                kind: Kind::Bezel {
                    size: Size::of(area),
                    cell,
                    tone,
                    weight,
                },
            },
        ))
    }

    /// The light the caret casts, on the cells of the band it lights that it falls over.
    pub fn caret(
        band: Rect,
        cell: Option<CellSize>,
        caret: u16,
        palette: &Palette,
    ) -> Option<Placement> {
        let cell = cell?;
        // The row a band is headed by is a seam, and the light the caret casts falls on the band
        // under it.
        let band = Rect {
            y: band.y.saturating_add(1),
            height: band.height.saturating_sub(1),
            ..band
        };
        if !covers(band) || caret < band.x || caret >= band.right() {
            return None;
        }
        let prompt = palette.band(Band::Prompt);
        let rail = palette.band(Band::Rail);
        if [prompt, rail]
            .iter()
            .any(|colour| channels(*colour).is_none())
        {
            return None;
        }
        let wanted = i64::from(caret) - i64::from(LIGHT_COLS / 2);
        let first = i64::from(band.x).max(wanted);
        let last = i64::from(band.right()).min(wanted + i64::from(LIGHT_COLS));
        let shown = u16::try_from(last - first).ok()?;
        let skip = u32::try_from(first - wanted).ok()?;
        Some(Placement {
            shade: Shade {
                kind: Kind::Caret {
                    size: Size {
                        cols: LIGHT_COLS,
                        rows: band.height,
                    },
                    cell,
                    prompt,
                    rail,
                    peak: palette.peak(Band::Prompt),
                },
            },
            cells: Rect::new(u16::try_from(first).ok()?, band.y, shown, band.height),
            shows: Some(image::math::Rect {
                x: skip * cell.width(),
                y: 0,
                width: u32::from(shown) * cell.width(),
                height: u32::from(band.height) * cell.height(),
            }),
        })
    }

    /// The whole of a shade's picture, on the cells of the band it is drawn for.
    fn whole(cells: Rect, shade: Shade) -> Placement {
        Placement {
            shade,
            cells,
            shows: shade.cell().map(|cell| {
                let (width, height) = shade.size().pixels(cell);
                image::math::Rect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                }
            }),
        }
    }

    pub fn shade(&self) -> &Shade {
        &self.shade
    }

    pub fn cells(&self) -> Rect {
        self.cells
    }

    /// The seam's own cell, within the cells the placement claims.
    pub fn seam(&self) -> Option<(Axis, Rect)> {
        let axis = self.shade.seam()?.axis;
        Some((
            axis,
            match axis {
                Axis::Down => Rect {
                    width: 1,
                    ..self.cells
                },
                Axis::Across => Rect {
                    height: 1,
                    ..self.cells
                },
            },
        ))
    }

    pub fn claim(&self, buffer: &mut Buffer, palette: &Palette) {
        if matches!(
            self.shade.kind,
            Kind::Caret { .. } | Kind::Gauge { .. } | Kind::Bar { .. } | Kind::Bezel { .. }
        ) {
            return;
        }
        let area = self.cells;
        if self.places() {
            pen::claim(buffer, area, palette.ground(None));
            return;
        }
        let seam = self.seam();
        for row in 0..area.height {
            let y = area.y.saturating_add(row);
            let ground = palette.over(self.shade.row(row));
            let line = Rect {
                y,
                height: 1,
                ..area
            };
            let Some((axis, taken)) = seam
                .filter(|(_, seam)| seam.y <= y && y < seam.bottom())
                .map(|(axis, seam)| {
                    (
                        axis,
                        Rect {
                            x: seam.x,
                            width: seam.width,
                            ..line
                        },
                    )
                })
            else {
                pen::claim(buffer, line, ground);
                continue;
            };
            pen::claim(
                buffer,
                Rect {
                    width: taken.x.saturating_sub(line.x),
                    ..line
                },
                ground,
            );
            pen::claim(
                buffer,
                Rect {
                    x: taken.right(),
                    width: line.right().saturating_sub(taken.right()),
                    ..line
                },
                ground,
            );
            seam_cell(buffer, taken, axis, palette);
        }
    }

    /// The rect of the shade's picture these cells show, and none where no picture is placed.
    pub fn shows(&self) -> Option<image::math::Rect> {
        self.shows
    }

    /// The picture's own pixel at the middle of the image cell the frame's cell lands on: the two
    /// spaces are counted apart, so nothing outside asks which of them a number is in.
    pub fn pixel(&self, x: u16, y: u16) -> Option<(u32, u32)> {
        let shows = self.shows?;
        if !self.cells.contains((x, y).into()) {
            return None;
        }
        let column = u32::from(x - self.cells.x);
        let row = u32::from(y - self.cells.y);
        let across = u32::from(self.cells.width.max(1));
        let down = u32::from(self.cells.height.max(1));
        Some((
            shows.x + column * shows.width / across + shows.width / across / 2,
            shows.y + row * shows.height / down + shows.height / down / 2,
        ))
    }

    pub fn places(&self) -> bool {
        self.shade.places()
    }
}

#[derive(Clone, Debug)]
pub struct Plane {
    shade: Shade,
    pixels: image::RgbaImage,
}

impl Plane {
    pub(crate) fn draw(shade: Shade) -> Option<Plane> {
        Some(Plane {
            pixels: shade.pixels()?,
            shade,
        })
    }

    pub fn id(&self) -> PlaneId {
        self.shade.id()
    }

    pub fn pixels(&self) -> &image::RgbaImage {
        &self.pixels
    }
}

#[derive(Clone, Debug, Default)]
pub struct Planes {
    held: Vec<Plane>,
    made: usize,
}

impl Planes {
    pub fn hold(&mut self, placements: &[Placement]) {
        let mut held = Vec::with_capacity(placements.len());
        for placement in placements.iter().filter(|placement| placement.places()) {
            if held
                .iter()
                .any(|plane: &Plane| plane.shade == placement.shade)
            {
                continue;
            }
            match self
                .held
                .iter()
                .position(|plane| plane.shade == placement.shade)
            {
                Some(at) => held.push(self.held.remove(at)),
                None => {
                    if let Some(plane) = Plane::draw(placement.shade) {
                        held.push(plane);
                        self.made += 1;
                    }
                }
            }
        }
        self.held = held;
    }

    pub fn image(&self, shade: &Shade) -> Option<&Plane> {
        self.held.iter().find(|plane| plane.shade == *shade)
    }

    pub fn held(&self) -> &[Plane] {
        &self.held
    }

    pub fn made(&self) -> usize {
        self.made
    }
}

#[derive(Clone, Debug, Default)]
pub struct Held {
    standing: Vec<Placement>,
}

impl Held {
    /// The bytes that bring the terminal up to date for one pass, in the order it must apply
    /// them: a delete for every picture it holds and the frame no longer stands, then a
    /// transmission or a bare placement for each placement in the frame's order.
    pub fn pass(&mut self, placements: &[Placement], planes: &Planes) -> Vec<u8> {
        let placing: Vec<Placement> = placements
            .iter()
            .copied()
            .filter(|at| at.places())
            .collect();
        let before = std::mem::take(&mut self.standing);
        let mut sent = Vec::new();
        for held in before
            .iter()
            .filter(|held| placing.iter().all(|at| at.shade != held.shade))
        {
            sent.extend(delete(held.shade.id()));
        }
        let mut stands = Vec::with_capacity(placing.len());
        for at in placing {
            let (Some(shows), Some(plane)) = (at.shows(), planes.image(&at.shade)) else {
                continue;
            };
            match before.iter().find(|held| held.shade == at.shade) {
                None => sent.extend(upload(plane, at, shows)),
                Some(held) if held.cells != at.cells || held.shows != Some(shows) => {
                    sent.extend(place(at.shade.id(), at, shows));
                }
                Some(_) => {}
            }
            stands.push(at);
        }
        self.standing = stands;
        sent
    }

    /// The bytes that give back every picture the terminal holds, and none held after.
    pub fn forget(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.standing)
            .into_iter()
            .flat_map(|held| delete(held.shade.id()))
            .collect()
    }
}

/// The joint a rail of this shape is parted by: a ledger takes the frame's own column and is parted
/// down it, and a strip lies under the band and parts nothing.
fn seam_of(shape: Rail, palette: &Palette) -> Option<Seam> {
    match shape {
        Rail::Ledger => Some(Seam {
            axis: Axis::Down,
            weight: palette.seam(),
        }),
        Rail::Strip => None,
    }
}

/// One plane transmitted as the picture a terminal keeps under a band: its pixels as a PNG, in
/// the chunks the protocol allows, under its identity, its placement and the frame's depth. `C=1`
/// keeps a tall placement from scrolling, and `q=2` leaves no reply to read as a keystroke.
fn upload(plane: &Plane, at: Placement, shows: image::math::Rect) -> Vec<u8> {
    let cells = at.cells();
    let head = format!(
        "a=T,f=100,i={},p={},q=2,z={},C=1,c={},r={},x={},y={},w={},h={}",
        plane.id().image(),
        PLACEMENT,
        DEPTH,
        cells.width,
        cells.height,
        shows.x,
        shows.y,
        shows.width,
        shows.height
    );
    let mut sent = format!("\x1b7\x1b[{};{}H", cells.y + 1, cells.x + 1).into_bytes();
    transmit(&mut sent, &head, &encoded(plane.pixels()));
    sent.extend_from_slice(b"\x1b8");
    sent
}

/// The image the terminal already holds, put on the cells it stands on now: no byte of it is
/// sent again, and the placement it is put under is the one it was held by.
fn place(id: PlaneId, at: Placement, shows: image::math::Rect) -> Vec<u8> {
    let cells = at.cells();
    let head = format!(
        "a=p,i={},p={},q=2,z={},C=1,c={},r={},x={},y={},w={},h={}",
        id.image(),
        PLACEMENT,
        DEPTH,
        cells.width,
        cells.height,
        shows.x,
        shows.y,
        shows.width,
        shows.height
    );
    let mut sent = format!("\x1b7\x1b[{};{}H", cells.y + 1, cells.x + 1).into_bytes();
    sent.extend_from_slice(format!("\x1b_G{head}\x1b\\").as_bytes());
    sent.extend_from_slice(b"\x1b8");
    sent
}

/// The pixels as a PNG in base64: the encoder writes into a buffer in memory, and a picture a
/// shade draws is inside the format's bounds, so no step here can fail.
fn encoded(pixels: &image::RgbaImage) -> String {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(pixels.clone())
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("a picture in memory encodes as a PNG");
    base64::engine::general_purpose::STANDARD.encode(png)
}

/// One image, in the chunks the protocol allows: the first chunk carries the head, and each chunk
/// after it says only whether another follows.
fn transmit(sent: &mut Vec<u8>, head: &str, encoded: &str) {
    for (index, chunk) in encoded.as_bytes().chunks(4096).enumerate() {
        let more = usize::from((index + 1) * 4096 < encoded.len());
        match index {
            0 => sent.extend_from_slice(format!("\x1b_G{head},m={more};").as_bytes()),
            _ => sent.extend_from_slice(format!("\x1b_Gm={more};").as_bytes()),
        }
        sent.extend_from_slice(chunk);
        sent.extend_from_slice(b"\x1b\\");
    }
}

fn delete(id: PlaneId) -> Vec<u8> {
    let mode = if matches!(id, PlaneId::Prompt(_) | PlaneId::Tool(_)) {
        'I'
    } else {
        'i'
    };
    format!("\x1b_Ga=d,d={mode},i={},q=2\x1b\\", id.image()).into_bytes()
}

/// The bytes that give every picture the frame can hold back, each identity named once.
pub fn give_back() -> Vec<u8> {
    let mut bytes: Vec<u8> = PlaneId::ALL.into_iter().flat_map(delete).collect();
    bytes.extend_from_slice(b"\x1b_Ga=d,d=A,q=2\x1b\\");
    bytes
}

/// The face's picture sent at `at`, each frame past the root composed against it in the overwrite
/// mode and cropped to the pixels that differ, and how many frames the terminal is left keeping.
pub fn face_sent(
    at: Rect,
    root: &image::RgbaImage,
    frames: &[image::RgbaImage],
) -> (Vec<u8>, usize) {
    let head = format!(
        "a=T,f=100,i={},p={},q=2,z={},C=1,c={},r={}",
        PlaneId::Face.image(),
        PLACEMENT,
        DEPTH,
        at.width,
        at.height
    );
    let mut sent = format!("\x1b7\x1b[{};{}H", at.y + 1, at.x + 1).into_bytes();
    transmit(&mut sent, &head, &encoded(root));
    sent.extend_from_slice(b"\x1b8");
    let mut kept = 1;
    for frame in frames.iter().skip(1) {
        let Some((left, top, width, height)) = differing(root, frame) else {
            continue;
        };
        let head = format!(
            "a=f,f=100,i={},q=2,c=1,X=1,x={left},y={top},s={width},v={height}",
            PlaneId::Face.image()
        );
        let shown = image::imageops::crop_imm(frame, left, top, width, height).to_image();
        transmit(&mut sent, &head, &encoded(&shown));
        kept += 1;
    }
    (sent, kept)
}

/// The bare placement of a face the terminal already holds: no pixel of it travels, and the cells
/// it stands on now are the ones it is put on.
pub fn face_placed(at: Rect) -> Vec<u8> {
    let head = format!(
        "a=p,i={},p={},q=2,z={},C=1,c={},r={}",
        PlaneId::Face.image(),
        PLACEMENT,
        DEPTH,
        at.width,
        at.height
    );
    let mut sent = format!("\x1b7\x1b[{};{}H", at.y + 1, at.x + 1).into_bytes();
    sent.extend_from_slice(format!("\x1b_G{head}\x1b\\").as_bytes());
    sent.extend_from_slice(b"\x1b8");
    sent
}

/// The loop the motion asks for over the frames the terminal keeps, or the frame it holds where
/// the motion has stopped.
pub fn face_turned(motion: Motion, frames: usize) -> Vec<u8> {
    let mut sent = Vec::new();
    match motion {
        Motion::Still => {}
        Motion::Turning(None) => {
            let stopped = format!("a=a,i={},s=1,c=1,q=2", PlaneId::Face.image());
            sent.extend_from_slice(format!("\x1b_G{stopped}\x1b\\").as_bytes());
        }
        Motion::Turning(Some(turning)) => {
            for frame in 1..=frames {
                let looped = format!(
                    "a=a,i={},r={frame},z={},q=2",
                    PlaneId::Face.image(),
                    turning.gap_ms()
                );
                sent.extend_from_slice(format!("\x1b_G{looped}\x1b\\").as_bytes());
            }
            let cycles = turning.cycles().unwrap_or(1);
            let spent = format!("a=a,i={},s=3,v={cycles},q=2", PlaneId::Face.image());
            sent.extend_from_slice(format!("\x1b_G{spent}\x1b\\").as_bytes());
        }
    }
    sent
}

/// The bytes that give back the face's picture and the cells it stands on.
pub fn face_back() -> Vec<u8> {
    delete(PlaneId::Face)
}

/// The two features a terminal is asked about a picture's frames and their turning: frames composed
/// onto a previous one in the overwrite mode, and the loop that turns them.
pub fn frames_asked() -> Vec<u8> {
    let mut asked = Vec::new();
    for feature in ["e=f,c=1,X=1", "e=a"] {
        let query = format!("a=q,i={},{feature}", PlaneId::Face.image());
        asked.extend_from_slice(format!("\x1b_G{query}\x1b\\").as_bytes());
    }
    asked
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

/// Whether a row of a band is the one the band closes on, drawn in the tone of the band it meets.
fn closes(row: u16, rows: u16) -> bool {
    row.saturating_add(1) >= rows
}

fn covers(area: Rect) -> bool {
    area.width > 0 && area.height > 0
}

fn seam_cell(buffer: &mut Buffer, line: Rect, axis: Axis, palette: &Palette) {
    match (palette.draws_edge(), axis) {
        (true, _) => pen::claim(buffer, line, palette.over(palette.edge_ink())),
        (false, Axis::Down) => {
            for x in line.x..line.right() {
                pen::glyph(buffer, x, line.y, EDGE_RULE, palette.dim_edge(), palette);
            }
        }
        (false, Axis::Across) => {}
    }
}

#[cfg(test)]
mod tests;
