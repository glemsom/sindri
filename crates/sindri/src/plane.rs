//! The planes a frame places under its cells: an image the terminal draws at the pixels of a band,
//! the identity it is placed under, and the bytes that take it away again.

use ratatui::layout::Rect;

use crate::graphics::CellSize;
use crate::ledger::Rail;
use crate::palette::{Band, Edge, Palette, channels};

/// The columns of the transcript the rail's plane widens across: the transcript's last, where the
/// well falls into the shadow the rail comes back out of.
pub const SEAM: u16 = 3;

/// Which band's ground a plane is, and the identity a placement is named by: the two planes of one
/// frame are told apart by it and by nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneId {
    Composer,
    Rail,
}

impl PlaneId {
    pub const ALL: [PlaneId; 2] = [PlaneId::Composer, PlaneId::Rail];

    /// The image id the terminal holds this plane under, which is what a delete names.
    pub fn image(self) -> u32 {
        match self {
            PlaneId::Composer => 333,
            PlaneId::Rail => 334,
        }
    }
}

/// A band's ground as an image the terminal draws under its cells: the pixels, the rect they cover,
/// and the identity the placement is held by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plane {
    id: PlaneId,
    area: Rect,
    pixels: image::RgbaImage,
}

impl Plane {
    /// The composer's band: the ladder's head falling into its foot over its height, with the glow
    /// under the column the caret waits in. None for an area with no rows or columns or a palette
    /// with no colour in it.
    pub fn composer(
        area: Rect,
        cell: CellSize,
        caret: Option<(u16, u16)>,
        palette: &Palette,
    ) -> Option<Plane> {
        let (width, height) = extent(area, cell)?;
        let head = channels(palette.step(0, area.height))?;
        let foot = channels(palette.step(area.height - 1, area.height))?;
        let glow = caret
            .map(|(x, _)| (f64::from(x.saturating_sub(area.x)) + 0.5) * f64::from(cell.width()));
        let pixels = image::RgbaImage::from_fn(width, height, |x, y| {
            let t = f64::from(y) / f64::from(height.max(1));
            let light = glow.map_or(0.0, |center| {
                let distance = (f64::from(x) - center) / (f64::from(cell.width()) * 8.0);
                let depth = t / 0.65;
                (-(distance * distance + depth * depth) * 2.0).exp() * 22.0
            });
            let channel = |head: u8, foot: u8| {
                (f64::from(head) * (1.0 - t) + f64::from(foot) * t + light).clamp(0.0, 255.0) as u8
            };
            image::Rgba([
                channel(head[0], foot[0]),
                channel(head[1], foot[1]),
                channel(head[2], foot[2]),
                255,
            ])
        });
        Some(Plane {
            id: PlaneId::Composer,
            area,
            pixels,
        })
    }

    /// The rail's band: the band's own colour at the head, falling into the shadow at its foot, and
    /// a ledger's widened across the seam, where the well falls into the crease and the band comes
    /// back out of it. None for an area with no rows or columns or a palette with no colour in it.
    pub fn rail(shape: Rail, area: Rect, cell: CellSize, palette: &Palette) -> Option<Plane> {
        let head = channels(palette.band(Band::Rail))?;
        let foot = channels(palette.edge(Edge::Shadow))?;
        let seam = match shape {
            Rail::Ledger => Seam::of(SEAM.min(area.x), cell, foot, palette),
            Rail::Strip => None,
        };
        let widened = seam.map_or(0, Seam::widened);
        let area = Rect {
            x: area.x - widened,
            width: area.width + widened,
            ..area
        };
        let (width, height) = extent(area, cell)?;
        let span = f64::from(height.saturating_sub(1).max(1));
        let pixels = image::RgbaImage::from_fn(width, height, |x, y| {
            let down = mix(head, foot, f64::from(y) / span);
            let colour = seam.and_then(|seam| seam.across(x, down)).unwrap_or(down);
            image::Rgba([colour[0], colour[1], colour[2], 255])
        });
        Some(Plane {
            id: PlaneId::Rail,
            area,
            pixels,
        })
    }

    pub fn id(&self) -> PlaneId {
        self.id
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn pixels(&self) -> &image::RgbaImage {
        &self.pixels
    }
}

/// The bytes that delete the plane an id names, which is the only thing a delete says.
pub fn delete(id: PlaneId) -> Vec<u8> {
    format!("\x1b_Ga=d,d=i,i={}\x1b\\", id.image()).into_bytes()
}

/// The pixels a band `area` covers at `cell`, and nothing for a band with no rows or columns.
fn extent(area: Rect, cell: CellSize) -> Option<(u32, u32)> {
    match area.width == 0 || area.height == 0 {
        true => None,
        false => Some((
            u32::from(area.width) * cell.width(),
            u32::from(area.height) * cell.height(),
        )),
    }
}

/// The seam a ledger's plane widens across: the transcript's columns to the left of the rail's
/// first, and the rail's first few, drawn as a fall from the well into the shadow and back out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seam {
    widened: u16,
    cell: CellSize,
    well: [u8; 3],
    shadow: [u8; 3],
}

impl Seam {
    /// The seam that widens a plane `widened` columns, and nothing for a palette with no well to
    /// spell in a pixel, where there is no fall to start from.
    fn of(widened: u16, cell: CellSize, shadow: [u8; 3], palette: &Palette) -> Option<Seam> {
        Some(Seam {
            widened,
            cell,
            well: channels(palette.band(Band::Well))?,
            shadow,
        })
    }

    fn widened(self) -> u16 {
        self.widened
    }

    /// The colour the seam is at pixel column `x`, over the `down` the rail's own fall gives that
    /// column, and nothing for a column the rail's fall is the whole of.
    fn across(&self, x: u32, down: [u8; 3]) -> Option<[u8; 3]> {
        let crease = u32::from(self.widened) * self.cell.width();
        let out = u32::from(SEAM) * self.cell.width();
        match x {
            x if x < crease => Some(fall(self.well, self.shadow, x, crease - 1)),
            x if x - crease < out => Some(fall(self.shadow, down, x - crease, out - 1)),
            _ => None,
        }
    }
}

/// The colour `at` pixels along a ramp of `span` pixels from `head` to `foot`, its ends the ends of
/// the ramp.
fn fall(head: [u8; 3], foot: [u8; 3], at: u32, span: u32) -> [u8; 3] {
    let t = match span {
        0 => 1.0,
        span => f64::from(at.min(span)) / f64::from(span),
    };
    mix(head, foot, t)
}

/// The colour a plane is at `t` of the way down a ramp from `head` to `foot`.
fn mix(head: [u8; 3], foot: [u8; 3], t: f64) -> [u8; 3] {
    let channel =
        |head: u8, foot: u8| (f64::from(head) * (1.0 - t) + f64::from(foot) * t).round() as u8;
    [
        channel(head[0], foot[0]),
        channel(head[1], foot[1]),
        channel(head[2], foot[2]),
    ]
}
