//! The planes a frame places under its cells: the shading a band's ground is drawn from, the image
//! it is drawn as, the identity a placement is named by, and the bytes that take one away again.

use ratatui::layout::Rect;

use crate::graphics::CellSize;
use crate::ledger::Rail;
use crate::palette::{Band, Edge, Palette, channels};

/// The columns of the transcript the rail's plane widens across: the transcript's last, where the
/// well falls into the shadow the rail comes back out of.
pub const SEAM: u16 = 6;

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

/// A band's shading as a value: which band it is, the rows and columns it covers, the cell it is
/// drawn at and the colours it is drawn in. Every fact an image of it depends on is held here, so
/// drawing one never fails and two passes that shade the same band shade it alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    /// The composer's band, with the caret's column in the frame and none while it waits nowhere:
    /// the glow is drawn under it, and it is the one fact besides the band the shading depends on.
    Composer {
        area: Rect,
        cell: CellSize,
        well: [u8; 3],
        head: [u8; 3],
        foot: [u8; 3],
        caret: Option<u16>,
    },
    /// The rail's band, over the rows and columns it covers already widened across the seam.
    Rail {
        area: Rect,
        cell: CellSize,
        head: [u8; 3],
        foot: [u8; 3],
        seam: Option<Seam>,
    },
}

impl Shade {
    /// The composer's band: its first cell rising out of the transcript's well into its head, the rung
    /// above the rail's tone, which falls into the rail's own tone over the height left, with the glow
    /// under the caret's column. None for an area with no rows or no columns, or no colour in a pixel.
    pub fn composer(
        area: Rect,
        cell: CellSize,
        caret: Option<u16>,
        palette: &Palette,
    ) -> Option<Shade> {
        if !covers(area) {
            return None;
        }
        Some(Shade::Composer {
            area,
            cell,
            well: channels(palette.band(Band::Well))?,
            head: channels(palette.step(0, area.height))?,
            foot: channels(palette.step(area.height - 1, area.height))?,
            caret,
        })
    }

    /// The rail's band: the band's own colour at the head, falling into the shadow at its foot, and
    /// a ledger's widened across the seam, where the well falls into the crease and the band comes
    /// back out of it. None for an area with no rows or columns or a palette with no colour in it.
    pub fn rail(shape: Rail, area: Rect, cell: CellSize, palette: &Palette) -> Option<Shade> {
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
        if !covers(area) {
            return None;
        }
        Some(Shade::Rail {
            area,
            cell,
            head,
            foot,
            seam,
        })
    }

    pub fn id(&self) -> PlaneId {
        match self {
            Shade::Composer { .. } => PlaneId::Composer,
            Shade::Rail { .. } => PlaneId::Rail,
        }
    }

    pub fn area(&self) -> Rect {
        match self {
            Shade::Composer { area, .. } | Shade::Rail { area, .. } => *area,
        }
    }

    fn cell(&self) -> CellSize {
        match self {
            Shade::Composer { cell, .. } | Shade::Rail { cell, .. } => *cell,
        }
    }
}

/// A band's ground as the image the terminal draws under its cells, held with the shading it was
/// drawn from: two planes of one frame are told apart by that shading and by nothing else.
#[derive(Clone, Debug)]
pub struct Plane {
    shade: Shade,
    pixels: image::RgbaImage,
}

impl Plane {
    pub(crate) fn draw(shade: Shade) -> Plane {
        let (width, height) = extent(shade.area(), shade.cell());
        let pixels = match shade {
            Shade::Composer {
                area,
                cell,
                well,
                head,
                foot,
                caret,
            } => {
                let rise = cell.height().min(height.saturating_sub(1)).max(1);
                // The head's own row is the rise's last, so the fall runs over the rows under it.
                let body = height.saturating_sub(rise + 1);
                let glow = caret
                    .map(|x| (f64::from(x.saturating_sub(area.x)) + 0.5) * f64::from(cell.width()));
                image::RgbaImage::from_fn(width, height, |x, y| {
                    let tone = match y < rise {
                        true => mix(well, head, ease(share(y, rise))),
                        false => mix(head, foot, share(y - rise, body)),
                    };
                    let light = glow.map_or(0.0, |center| {
                        let distance = (f64::from(x) - center) / (f64::from(cell.width()) * 8.0);
                        let depth = share(y, height) / 0.65;
                        (-(distance * distance + depth * depth) * 2.0).exp() * 22.0
                    });
                    let channel = |at: u8| (f64::from(at) + light).clamp(0.0, 255.0) as u8;
                    image::Rgba([channel(tone[0]), channel(tone[1]), channel(tone[2]), 255])
                })
            }
            Shade::Rail {
                head, foot, seam, ..
            } => {
                let span = f64::from(height.saturating_sub(1).max(1));
                image::RgbaImage::from_fn(width, height, |x, y| {
                    let down = mix(head, foot, f64::from(y) / span);
                    let colour = seam.and_then(|seam| seam.across(x, down)).unwrap_or(down);
                    image::Rgba([colour[0], colour[1], colour[2], 255])
                })
            }
        };
        Plane { shade, pixels }
    }

    pub fn id(&self) -> PlaneId {
        self.shade.id()
    }

    pub fn area(&self) -> Rect {
        self.shade.area()
    }

    pub fn pixels(&self) -> &image::RgbaImage {
        &self.pixels
    }
}

/// The band images one frame holds, each against the shading it was drawn from: a frame makes an
/// image when a band's shading moves and keeps it while that shading stands.
#[derive(Clone, Debug, Default)]
pub struct Planes {
    held: Vec<Plane>,
    made: usize,
}

impl Planes {
    /// Holds a pass's `shades`: a shading already held keeps its image, a new one is drawn, and one
    /// the pass no longer names loses its image.
    pub fn hold(&mut self, shades: &[Shade]) {
        let mut held = Vec::with_capacity(shades.len());
        for shade in shades {
            match self.held.iter().position(|plane| plane.shade == *shade) {
                Some(at) => held.push(self.held.remove(at)),
                None => {
                    held.push(Plane::draw(*shade));
                    self.made += 1;
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

/// The bytes that delete the plane an id names, which is the only thing a delete says.
pub fn delete(id: PlaneId) -> Vec<u8> {
    format!("\x1b_Ga=d,d=i,i={}\x1b\\", id.image()).into_bytes()
}

/// Whether a band `area` covers a pixel at all: one with no rows or no columns is no image.
fn covers(area: Rect) -> bool {
    area.width > 0 && area.height > 0
}

/// The pixels a band `area` covers at `cell`, which a covered band always has.
fn extent(area: Rect, cell: CellSize) -> (u32, u32) {
    (
        u32::from(area.width) * cell.width(),
        u32::from(area.height) * cell.height(),
    )
}

/// The seam a ledger's plane widens across: the transcript's columns to the left of the rail's
/// first, and the rail's first few, drawn as a fall from the well into the shadow and back out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seam {
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

/// The share `at` of `span`, and the whole of it for a span of none, so nothing is divided by
/// nothing.
fn share(at: u32, span: u32) -> f64 {
    match span {
        0 => 1.0,
        span => f64::from(at.min(span)) / f64::from(span),
    }
}

/// The share of a rise `at` of the way up, eased out: it leaves the well quickly and comes into the
/// head gently, so no row of it takes the whole step.
fn ease(at: f64) -> f64 {
    1.0 - (1.0 - at) * (1.0 - at)
}

/// The colour `at` pixels along a ramp of `span` pixels from `head` to `foot`, its ends the ends of
/// the ramp.
fn fall(head: [u8; 3], foot: [u8; 3], at: u32, span: u32) -> [u8; 3] {
    mix(head, foot, share(at, span))
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
