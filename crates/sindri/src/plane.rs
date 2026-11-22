use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::graphics::CellSize;
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
pub const PLACEMENT: u32 = 1;

/// The depth every plane is placed at. Kitty draws an image under a cell that carries a
/// background only below `INT32_MIN / 2`, and a seam's edge ink is one such cell.
pub const DEPTH: i64 = i32::MIN as i64 / 2 - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneId {
    Composer,
    Rail,
    Transcript,
    Caret,
}

impl PlaneId {
    pub const ALL: [PlaneId; 4] = [
        PlaneId::Composer,
        PlaneId::Rail,
        PlaneId::Transcript,
        PlaneId::Caret,
    ];

    /// The identity the image is held under and layered by: two images at the frame's one depth
    /// stand by identity, so the caret's light is read over the band it lights.
    pub fn image(self) -> u32 {
        match self {
            PlaneId::Composer => 333,
            PlaneId::Rail => 334,
            PlaneId::Transcript => 335,
            PlaneId::Caret => 336,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    Transcript {
        area: Rect,
        cell: Option<CellSize>,
        well: Color,
        peak: u8,
    },
    Rail {
        area: Rect,
        cell: Option<CellSize>,
        tone: Color,
        shape: Rail,
        face: Option<Rect>,
        peak: u8,
    },
    Composer {
        area: Rect,
        cell: Option<CellSize>,
        prompt: Color,
        rail: Color,
    },
    /// The light the caret casts, drawn as an image of its own: the one plane whose placement
    /// moves while the pixels it stands from do not.
    Caret {
        band: Rect,
        cell: Option<CellSize>,
        prompt: Color,
        rail: Color,
        peak: u8,
        caret: u16,
    },
}

impl Shade {
    pub fn transcript(area: Rect, cell: Option<CellSize>, palette: &Palette) -> Option<Shade> {
        if !covers(area) {
            return None;
        }
        let well = palette.band(Band::Well);
        Some(Shade::Transcript {
            area,
            cell: cell.filter(|_| channels(well).is_some()),
            well,
            peak: palette.peak(Band::Well),
        })
    }

    pub fn composer(area: Rect, cell: Option<CellSize>, palette: &Palette) -> Option<Shade> {
        if !covers(area) {
            return None;
        }
        let prompt = palette.band(Band::Prompt);
        let rail = palette.band(Band::Rail);
        let spelled = channels(prompt).is_some() && channels(rail).is_some();
        Some(Shade::Composer {
            area,
            cell: cell.filter(|_| spelled),
            prompt,
            rail,
        })
    }

    pub fn rail(
        shape: Rail,
        band: RailBand,
        cell: Option<CellSize>,
        palette: &Palette,
    ) -> Option<Shade> {
        if !covers(band.area) {
            return None;
        }
        let tone = palette.band(Band::Rail);
        let cell = cell.filter(|_| channels(tone).is_some());
        Some(Shade::Rail {
            area: band.area,
            cell,
            tone,
            shape,
            face: band.picture,
            peak: palette.peak(Band::Rail),
        })
    }

    pub fn caret(
        band: Rect,
        cell: Option<CellSize>,
        caret: u16,
        palette: &Palette,
    ) -> Option<Shade> {
        if !covers(band) || caret < band.x || caret >= band.right() {
            return None;
        }
        let prompt = palette.band(Band::Prompt);
        let rail = palette.band(Band::Rail);
        let spelled = channels(prompt).is_some() && channels(rail).is_some();
        Some(Shade::Caret {
            band,
            cell: cell.filter(|_| spelled),
            prompt,
            rail,
            peak: palette.peak(Band::Prompt),
            caret,
        })
    }

    pub fn seam(&self) -> Option<(Axis, Rect)> {
        let area = self.area();
        match self {
            Shade::Transcript { .. } | Shade::Caret { .. } => None,
            Shade::Rail {
                shape: Rail::Strip, ..
            } => None,
            Shade::Rail { .. } => Some((Axis::Down, Rect { width: 1, ..area })),
            Shade::Composer { .. } => Some((Axis::Across, Rect { height: 1, ..area })),
        }
    }

    pub fn id(&self) -> Option<PlaneId> {
        match self {
            Shade::Transcript { .. } => Some(PlaneId::Transcript),
            Shade::Rail { .. } => Some(PlaneId::Rail),
            Shade::Composer { .. } => Some(PlaneId::Composer),
            Shade::Caret { .. } => Some(PlaneId::Caret),
        }
    }

    pub fn area(&self) -> Rect {
        match self {
            Shade::Transcript { area, .. }
            | Shade::Rail { area, .. }
            | Shade::Composer { area, .. } => *area,
            Shade::Caret { band, .. } => *band,
        }
    }

    pub fn places(&self) -> bool {
        self.cell().is_some()
    }

    pub fn row(&self, row: u16) -> Color {
        match self {
            Shade::Transcript { well, .. } => *well,
            Shade::Rail { tone, .. } => *tone,
            Shade::Composer {
                area, prompt, rail, ..
            } => match closes(row, area.height) {
                true => *rail,
                false => *prompt,
            },
            Shade::Caret {
                band, prompt, rail, ..
            } => match closes(row, band.height) {
                true => *rail,
                false => *prompt,
            },
        }
    }

    /// The image this shade is transmitted as: the pixels, with everything that only places them
    /// left out, so a light that moved is a placement and no transmission.
    pub fn image(&self) -> Option<Image> {
        let cell = self.cell()?;
        Some(match self {
            Shade::Transcript {
                area, well, peak, ..
            } => Image::Transcript {
                area: *area,
                cell,
                well: channels(*well)?,
                peak: *peak,
            },
            Shade::Rail {
                area,
                tone,
                face,
                peak,
                ..
            } => Image::Rail {
                area: *area,
                cell,
                tone: channels(*tone)?,
                face: *face,
                peak: *peak,
            },
            Shade::Composer {
                area, prompt, rail, ..
            } => Image::Composer {
                area: *area,
                cell,
                prompt: channels(*prompt)?,
                rail: channels(*rail)?,
            },
            Shade::Caret {
                band,
                prompt,
                rail,
                peak,
                ..
            } => Image::Caret {
                rows: band.height,
                cell,
                prompt: channels(*prompt)?,
                rail: channels(*rail)?,
                peak: *peak,
            },
        })
    }

    pub fn place(&self) -> Option<Placement> {
        let cell = self.cell()?;
        let whole = |area: Rect| Placement {
            left: area.x,
            top: area.y,
            cols: area.width,
            rows: area.height,
            skip: 0,
            from: 0,
            width: u32::from(area.width) * cell.width(),
        };
        Some(match self {
            Shade::Transcript { area, .. }
            | Shade::Rail { area, .. }
            | Shade::Composer { area, .. } => whole(*area),
            Shade::Caret { band, caret, .. } => {
                let wanted = i64::from(*caret) - i64::from(LIGHT_COLS / 2);
                let first = i64::from(band.x).max(wanted);
                let last = i64::from(band.right()).min(wanted + i64::from(LIGHT_COLS));
                let shown = u16::try_from(last - first).ok()?;
                let skip = u16::try_from(first - wanted).ok()?;
                Placement {
                    left: u16::try_from(first).ok()?,
                    top: band.y,
                    cols: shown,
                    rows: band.height,
                    skip,
                    from: u32::from(skip) * cell.width(),
                    width: u32::from(shown) * cell.width(),
                }
            }
        })
    }

    pub fn claim(&self, buffer: &mut Buffer, palette: &Palette) {
        if matches!(self, Shade::Caret { .. }) {
            return;
        }
        let area = self.area();
        let seam = self.seam();
        for row in 0..area.height {
            let y = area.y.saturating_add(row);
            let ground = match self.places() {
                true => palette.ground(None),
                false => palette.over(self.row(row)),
            };
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

    fn cell(&self) -> Option<CellSize> {
        match self {
            Shade::Transcript { cell, .. }
            | Shade::Rail { cell, .. }
            | Shade::Composer { cell, .. }
            | Shade::Caret { cell, .. } => *cell,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Down,
    Across,
}

/// What a shade is transmitted as: the pixels, and no word about where they stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Image {
    Transcript {
        area: Rect,
        cell: CellSize,
        well: [u8; 3],
        peak: u8,
    },
    Rail {
        area: Rect,
        cell: CellSize,
        tone: [u8; 3],
        face: Option<Rect>,
        peak: u8,
    },
    Composer {
        area: Rect,
        cell: CellSize,
        prompt: [u8; 3],
        rail: [u8; 3],
    },
    Caret {
        rows: u16,
        cell: CellSize,
        prompt: [u8; 3],
        rail: [u8; 3],
        peak: u8,
    },
}

impl Image {
    pub fn id(self) -> PlaneId {
        match self {
            Image::Transcript { .. } => PlaneId::Transcript,
            Image::Rail { .. } => PlaneId::Rail,
            Image::Composer { .. } => PlaneId::Composer,
            Image::Caret { .. } => PlaneId::Caret,
        }
    }

    fn cell(self) -> CellSize {
        match self {
            Image::Transcript { cell, .. }
            | Image::Rail { cell, .. }
            | Image::Composer { cell, .. }
            | Image::Caret { cell, .. } => cell,
        }
    }

    /// The cells the image covers at its own corner, which is where a band's plane stands.
    pub fn area(self) -> Rect {
        match self {
            Image::Transcript { area, .. }
            | Image::Rail { area, .. }
            | Image::Composer { area, .. } => area,
            Image::Caret { rows, .. } => Rect::new(0, 0, LIGHT_COLS, rows),
        }
    }

    fn rows(self) -> u16 {
        match self {
            Image::Transcript { area, .. }
            | Image::Rail { area, .. }
            | Image::Composer { area, .. } => area.height,
            Image::Caret { rows, .. } => rows,
        }
    }

    fn rung(self, row: u16) -> [u8; 3] {
        match self {
            Image::Transcript { well, .. } => well,
            Image::Rail { tone, .. } => tone,
            Image::Composer {
                area, prompt, rail, ..
            } => match closes(row, area.height) {
                true => rail,
                false => prompt,
            },
            Image::Caret {
                rows, prompt, rail, ..
            } => match closes(row, rows) {
                true => rail,
                false => prompt,
            },
        }
    }

    /// The lamp this image's pixels stand in, at the image's own pixel origin.
    fn light(self) -> Light {
        let cell = self.cell();
        let (across, down) = (f64::from(cell.width()), f64::from(cell.height()));
        match self {
            Image::Transcript { peak, .. } => Light::lamp(
                peak,
                At { x: 0.0, y: 0.0 },
                At {
                    x: f64::INFINITY,
                    y: CROWN_ROWS * down,
                },
            ),
            Image::Rail {
                area,
                face: Some(face),
                peak,
                ..
            } => {
                let centre = At {
                    x: (f64::from(face.x.saturating_sub(area.x)) + f64::from(face.width) / 2.0)
                        * across,
                    y: (f64::from(face.y.saturating_sub(area.y)) + f64::from(face.height) / 2.0)
                        * down,
                };
                // The light is spent a margin out on each axis: at the rail's own margin
                // across, and a margin past the rows the face stands on.
                let width = f64::from(area.width) * across;
                let reach = At {
                    x: (centre.x.min(width - centre.x) - f64::from(MARGIN) * across).max(1.0),
                    y: (f64::from(face.height) / 2.0 + f64::from(MARGIN)) * down,
                };
                Light::lamp(peak, centre, reach)
            }
            Image::Rail { .. } | Image::Composer { .. } => Light::NONE,
            Image::Caret { rows, peak, .. } => Light::lamp(
                peak,
                At {
                    x: (f64::from(LIGHT_COLS / 2) + 0.5) * across,
                    y: 0.0,
                },
                At {
                    x: f64::from(LIGHT_REACH) * across,
                    y: f64::from(rows.saturating_sub(1).max(1)) * down,
                },
            ),
        }
    }

    fn pixels(self) -> image::RgbaImage {
        let cell = self.cell();
        let area = self.area();
        let (width, height) = (
            u32::from(area.width) * cell.width(),
            u32::from(area.height) * cell.height(),
        );
        let down = cell.height();
        let light = self.light();
        image::RgbaImage::from_fn(width, height, |x, y| {
            let row = u16::try_from(y / down).unwrap_or(u16::MAX).min(self.rows());
            let lit = dithered(light.at(f64::from(x), f64::from(y)), (x, y));
            let [red, green, blue] = self.rung(row).map(|at| at.saturating_add(lit));
            image::Rgba([red, green, blue, 255])
        })
    }
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

/// The light a band carries: one lamp, its peak, the point it stands on and the reach it is spent
/// over on each axis.
#[derive(Clone, Copy, Debug)]
struct Light {
    peak: f64,
    centre: At,
    reach: At,
}

impl Light {
    /// A band with nothing standing on it, which its pixels read as a peak of no light.
    const NONE: Light = Light {
        peak: 0.0,
        centre: At { x: 0.0, y: 0.0 },
        reach: At { x: 1.0, y: 1.0 },
    };

    /// A lamp at `centre`, spent over `reach` on each axis: a reach of no bound is an axis the
    /// light does not fall on, and one past the reach is where it is exactly nothing.
    fn lamp(peak: u8, centre: At, reach: At) -> Light {
        Light {
            peak: f64::from(peak),
            centre,
            reach,
        }
    }

    /// The light at a pixel: the peak at the lamp's centre, and nothing at all past the reach.
    fn at(self, x: f64, y: f64) -> f64 {
        let across = (x - self.centre.x) / self.reach.x;
        let down = (y - self.centre.y) / self.reach.y;
        let away = across * across + down * down;
        match away < 1.0 {
            true => self.peak * (-(SPENT * away)).exp(),
            false => 0.0,
        }
    }
}

/// The light a pixel carries in the channel it is drawn in: the ordered matrix's threshold is
/// added before the floor, so an amount of light that is no whole channel is dithered rather
/// than blended into the rung beside it.
fn dithered(light: f64, (x, y): (u32, u32)) -> u8 {
    let down = usize::try_from(y % 8).unwrap_or(0);
    let across = usize::try_from(x % 8).unwrap_or(0);
    let threshold = (f64::from(THRESHOLDS[down][across]) + 0.5) / 64.0;
    (light + threshold).floor().clamp(0.0, 255.0) as u8
}

/// Where an image stands and which columns of it are shown there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub left: u16,
    pub top: u16,
    pub cols: u16,
    pub rows: u16,
    /// The columns of the image skipped at its left edge, and the pixel those columns begin at.
    pub skip: u16,
    pub from: u32,
    /// The pixels of the image shown, which are the ones the columns it stands over hold.
    pub width: u32,
}

#[derive(Clone, Debug)]
pub struct Plane {
    image: Image,
    id: PlaneId,
    pixels: image::RgbaImage,
}

impl Plane {
    pub(crate) fn draw(shade: Shade) -> Option<Plane> {
        let image = shade.image()?;
        Some(Plane {
            image,
            id: image.id(),
            pixels: image.pixels(),
        })
    }

    pub fn id(&self) -> PlaneId {
        self.id
    }

    pub fn area(&self) -> Rect {
        self.image.area()
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
    pub fn hold(&mut self, shades: &[Shade]) {
        let mut held = Vec::with_capacity(shades.len());
        for shade in shades {
            let Some(image) = shade.image() else {
                continue;
            };
            match self.held.iter().position(|plane| plane.image == image) {
                Some(at) => held.push(self.held.remove(at)),
                None => {
                    if let Some(plane) = Plane::draw(*shade) {
                        held.push(plane);
                        self.made += 1;
                    }
                }
            }
        }
        self.held = held;
    }

    pub fn image(&self, shade: &Shade) -> Option<&Plane> {
        let image = shade.image()?;
        self.held.iter().find(|plane| plane.image == image)
    }

    pub fn held(&self) -> &[Plane] {
        &self.held
    }

    pub fn made(&self) -> usize {
        self.made
    }
}

pub fn delete(id: PlaneId) -> Vec<u8> {
    format!("\x1b_Ga=d,d=i,i={},q=2\x1b\\", id.image()).into_bytes()
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
