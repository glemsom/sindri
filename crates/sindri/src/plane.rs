//! The planes a frame places under its cells: an image the terminal draws at the pixels of a band,
//! the identity it is placed under, and the bytes that take it away again.

use ratatui::layout::Rect;

use crate::graphics::CellSize;
use crate::palette::{Band, Edge, Palette, channels};

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
    /// The composer's band: the ramp the design draws in its own greys, with the glow under the
    /// column the caret waits in. None for an area with no rows or columns.
    pub fn composer(area: Rect, cell: CellSize, caret: Option<(u16, u16)>) -> Option<Plane> {
        let (width, height) = extent(area, cell)?;
        let glow = caret
            .map(|(x, _)| (f64::from(x.saturating_sub(area.x)) + 0.5) * f64::from(cell.width()));
        let pixels = image::RgbaImage::from_fn(width, height, |x, y| {
            let t = f64::from(y) / f64::from(height.max(1));
            let light = glow.map_or(0.0, |center| {
                let distance = (f64::from(x) - center) / (f64::from(cell.width()) * 8.0);
                let depth = t / 0.65;
                (-(distance * distance + depth * depth) * 2.0).exp() * 22.0
            });
            let channel = |top: f64, bottom: f64| {
                (top * (1.0 - t) + bottom * t + light).clamp(0.0, 255.0) as u8
            };
            image::Rgba([
                channel(36.0, 8.0),
                channel(49.0, 13.0),
                channel(63.0, 19.0),
                255,
            ])
        });
        Some(Plane {
            id: PlaneId::Composer,
            area,
            pixels,
        })
    }

    /// The rail's band: the band's own colour at the head, falling continuously into the shadow at
    /// its foot. None for an area with no rows or columns, and for a palette that draws no colour
    /// this design can spell in a pixel, where the band's own rows are the whole of its shading.
    pub fn rail(area: Rect, cell: CellSize, palette: &Palette) -> Option<Plane> {
        let head = channels(palette.band(Band::Rail))?;
        let foot = channels(palette.edge(Edge::Shadow))?;
        let (width, height) = extent(area, cell)?;
        let span = f64::from(height.saturating_sub(1).max(1));
        let pixels =
            image::RgbaImage::from_fn(width, height, |_, y| mix(head, foot, f64::from(y) / span));
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

/// The colour a plane is at `t` of the way down a ramp from `head` to `foot`.
fn mix(head: [u8; 3], foot: [u8; 3], t: f64) -> image::Rgba<u8> {
    let channel =
        |head: u8, foot: u8| (f64::from(head) * (1.0 - t) + f64::from(foot) * t).round() as u8;
    image::Rgba([
        channel(head[0], foot[0]),
        channel(head[1], foot[1]),
        channel(head[2], foot[2]),
        255,
    ])
}
