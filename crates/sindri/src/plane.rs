//! The planes a frame places under its cells: an image the terminal draws at the pixels of a band,
//! the identity it is placed under, and the bytes that take it away again.

use ratatui::layout::Rect;

use crate::graphics::CellSize;
use crate::palette::{Band, Edge, Palette, channels};

/// An image a terminal holds under the cells of a band: the band it covers, the pixels it is drawn
/// in, and the identity a delete names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plane {
    id: u32,
    area: Rect,
    pixels: image::RgbaImage,
}

impl Plane {
    /// The identity the composer band's plane is placed under.
    pub const COMPOSER: u32 = 333;

    /// The identity the rail's band's plane is placed under.
    pub const RAIL: u32 = 334;

    /// The identities a give-back deletes, whatever frame the launch ended on.
    pub const IDS: [u32; 2] = [Plane::COMPOSER, Plane::RAIL];

    /// The composer band's ground: flat at the head, falling into the shadow at the foot, with the
    /// caret's glow over it. None for an area with no rows or no columns.
    pub fn composer(area: Rect, cell: CellSize, caret: Option<(u16, u16)>) -> Option<Plane> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        Some(Plane {
            id: Plane::COMPOSER,
            area,
            pixels: ground(area, cell, caret),
        })
    }

    /// The rail band's ground: the band's own colour at the head, falling continuously into the
    /// shadow at the foot. None for an area with no rows or no columns, and for a palette that
    /// draws no colour this design can spell in pixels.
    pub fn rail(area: Rect, cell: CellSize, palette: &Palette) -> Option<Plane> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let head = channels(palette.band(Band::Rail))?;
        let foot = channels(palette.edge(Edge::Shadow))?;
        let width = u32::from(area.width) * cell.width();
        let height = u32::from(area.height) * cell.height();
        let span = f64::from(height.saturating_sub(1).max(1));
        Some(Plane {
            id: Plane::RAIL,
            area,
            pixels: image::RgbaImage::from_fn(width, height, |_, y| {
                let t = f64::from(y) / span;
                let mixed = |head: u8, foot: u8| {
                    (f64::from(head) * (1.0 - t) + f64::from(foot) * t).round() as u8
                };
                image::Rgba([
                    mixed(head[0], foot[0]),
                    mixed(head[1], foot[1]),
                    mixed(head[2], foot[2]),
                    255,
                ])
            }),
        })
    }

    pub fn id(&self) -> u32 {
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
pub fn delete(id: u32) -> Vec<u8> {
    format!("\x1b_Ga=d,d=i,i={id}\x1b\\").into_bytes()
}

fn ground(band: Rect, cell: CellSize, caret: Option<(u16, u16)>) -> image::RgbaImage {
    let width = u32::from(band.width) * cell.width();
    let height = u32::from(band.height) * cell.height();
    let glow =
        caret.map(|(x, _)| (f64::from(x.saturating_sub(band.x)) + 0.5) * f64::from(cell.width()));
    image::RgbaImage::from_fn(width, height, |x, y| {
        let t = f64::from(y) / f64::from(height.max(1));
        let light = glow.map_or(0.0, |center| {
            let distance = (f64::from(x) - center) / (f64::from(cell.width()) * 8.0);
            let depth = t / 0.65;
            (-(distance * distance + depth * depth) * 2.0).exp() * 22.0
        });
        let channel =
            |top: f64, bottom: f64| (top * (1.0 - t) + bottom * t + light).clamp(0.0, 255.0) as u8;
        image::Rgba([
            channel(36.0, 8.0),
            channel(49.0, 13.0),
            channel(63.0, 19.0),
            255,
        ])
    })
}
