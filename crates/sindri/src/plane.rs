//! The planes a frame places under its cells: the shade a band's ground is drawn from, the image
//! it is drawn as, the identity a placement is named by, and the bytes that take one away again.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::graphics::CellSize;
use crate::ledger::Rail;
use crate::palette::{self, Band, Edge, Palette, channels};
use crate::pen;

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

/// The one description of what a band sits on: the colour each of its rows is claimed in, the seams
/// it makes with the band beside it, and every fact an image of it is drawn from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    /// The transcript band, on the well, with no seam of its own.
    Transcript { area: Rect, well: Color },
    /// The rail on its own tone, the frame's shadow across its last row, the light its first
    /// column meets the transcript on, and the seam a ledger's plane widens across.
    Rail {
        area: Rect,
        cell: Option<CellSize>,
        shape: Rail,
        tone: Color,
        shadow: Color,
        lit: Color,
        seam: Option<Seam>,
    },
    /// The composer's band rising out of the well into its head row and falling to the rail's own
    /// tone under it, with the light its head row is claimed in where it takes the keys.
    Composer {
        area: Rect,
        cell: Option<CellSize>,
        well: Color,
        head: Color,
        foot: Color,
        lit: Option<Color>,
        caret: Option<u16>,
    },
}

impl Shade {
    /// The transcript band on the well, and nothing for an area with no rows or no columns.
    pub fn transcript(area: Rect, palette: &Palette) -> Option<Shade> {
        covers(area).then(|| Shade::Transcript {
            area,
            well: palette.band(Band::Well),
        })
    }

    /// The composer's band, with no plane under it unless the palette spells its three colours as
    /// pixels: the glow under the caret's column is drawn in the image, so it is a fact of the
    /// shade.
    pub fn composer(
        area: Rect,
        cell: Option<CellSize>,
        caret: Option<u16>,
        lit: bool,
        palette: &Palette,
    ) -> Option<Shade> {
        if !covers(area) {
            return None;
        }
        let well = palette.band(Band::Well);
        let head = palette.band(Band::Composer);
        let foot = palette.band(Band::Rail);
        let cell = cell.filter(|_| {
            channels(well).is_some() && channels(head).is_some() && channels(foot).is_some()
        });
        Some(Shade::Composer {
            area,
            cell,
            well,
            head,
            foot,
            lit: lit.then(|| palette.edge(Edge::Lit)),
            caret,
        })
    }

    /// The rail's band over the rows and columns it covers, widened across the seam a ledger's
    /// plane falls out of the transcript on, and not widened at all where no plane is drawn.
    pub fn rail(
        shape: Rail,
        area: Rect,
        cell: Option<CellSize>,
        palette: &Palette,
    ) -> Option<Shade> {
        if !covers(area) {
            return None;
        }
        let tone = palette.band(Band::Rail);
        let shadow = palette.edge(Edge::Shadow);
        let cell = cell.filter(|_| channels(tone).is_some() && channels(shadow).is_some());
        let seam = match (shape, cell) {
            (Rail::Ledger, Some(cell)) => Seam::of(SEAM.min(area.x), cell, shadow, palette),
            _ => None,
        };
        let widened = seam.map_or(0, Seam::widened);
        Some(Shade::Rail {
            area: Rect {
                x: area.x - widened,
                width: area.width + widened,
                ..area
            },
            cell,
            shape,
            tone,
            shadow,
            lit: palette.edge(Edge::Lit),
            seam,
        })
    }

    /// The identity the terminal holds this band's plane under, and nothing for the transcript
    /// band, which no plane stands under.
    pub fn id(&self) -> Option<PlaneId> {
        match self {
            Shade::Transcript { .. } => None,
            Shade::Rail { .. } => Some(PlaneId::Rail),
            Shade::Composer { .. } => Some(PlaneId::Composer),
        }
    }

    pub fn area(&self) -> Rect {
        match self {
            Shade::Transcript { area, .. }
            | Shade::Rail { area, .. }
            | Shade::Composer { area, .. } => *area,
        }
    }

    /// Whether a plane is drawn under the band, which is the one thing a cell of it is claimed by.
    pub fn places(&self) -> bool {
        self.cell().is_some()
    }

    /// The colour that row of the band sits on under no seam: the well, the rail's own tone, the
    /// shadow across a ledger's last row, or the shade the composer's band is raised in.
    pub fn row(&self, row: u16) -> Color {
        match self {
            Shade::Transcript { well, .. } => *well,
            Shade::Rail {
                area,
                shape,
                tone,
                shadow,
                ..
            } => match (*shape, row) {
                (Rail::Ledger, row) if row == area.height.saturating_sub(1) => *shadow,
                _ => *tone,
            },
            Shade::Composer {
                area, head, foot, ..
            } => palette::blend(*head, *foot, row, area.height.saturating_sub(1)),
        }
    }

    /// Claims the band's cells from the shade: the terminal's own ground where a plane is drawn
    /// under them, and otherwise every row on the colour the shade says it sits on, then the seams
    /// it makes with the band beside it.
    pub fn claim(&self, buffer: &mut Buffer, palette: &Palette) {
        let area = self.claimed();
        match self.places() {
            true => pen::claim(buffer, area, palette.ground(None)),
            false => {
                for row in 0..area.height {
                    pen::claim(
                        buffer,
                        Rect {
                            y: area.y.saturating_add(row),
                            height: 1,
                            ..area
                        },
                        palette.over(self.row(row)),
                    );
                }
                if let Shade::Rail {
                    shape: Rail::Ledger,
                    lit,
                    ..
                } = *self
                {
                    pen::claim(
                        buffer,
                        Rect {
                            width: 1,
                            height: area.height.saturating_sub(1),
                            ..area
                        },
                        palette.over(lit),
                    );
                }
            }
        }
        if let Shade::Composer { lit: Some(lit), .. } = *self {
            pen::claim(buffer, Rect { height: 1, ..area }, palette.over(lit));
        }
    }

    /// The band's own rows and columns, which is all a claim of its cells reaches: a rail's plane
    /// widens back over the transcript's last ones, and those are the transcript band's cells.
    fn claimed(&self) -> Rect {
        let area = self.area();
        match self {
            Shade::Rail {
                seam: Some(seam), ..
            } => Rect {
                x: area.x.saturating_add(seam.widened()),
                width: area.width.saturating_sub(seam.widened()),
                ..area
            },
            _ => area,
        }
    }

    fn cell(&self) -> Option<CellSize> {
        match self {
            Shade::Transcript { .. } => None,
            Shade::Rail { cell, .. } | Shade::Composer { cell, .. } => *cell,
        }
    }
}

/// A band's ground as the image the terminal draws under its cells, held with the shade it was drawn
/// from: two planes of one frame are told apart by that shade and by nothing else.
#[derive(Clone, Debug)]
pub struct Plane {
    shade: Shade,
    id: PlaneId,
    pixels: image::RgbaImage,
}

impl Plane {
    /// The image a shade is drawn as, and nothing for a band no plane stands under.
    pub(crate) fn draw(shade: Shade) -> Option<Plane> {
        let id = shade.id()?;
        let cell = shade.cell()?;
        let area = shade.area();
        let (width, height) = extent(area, cell);
        let pixels = match shade {
            Shade::Transcript { .. } => return None,
            Shade::Composer { well, caret, .. } => {
                let head = channels(shade.row(0))?;
                let foot = channels(shade.row(area.height.saturating_sub(1)))?;
                let well = channels(well)?;
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
                tone, shadow, seam, ..
            } => {
                let head = channels(tone)?;
                let foot = channels(shadow)?;
                let span = f64::from(height.saturating_sub(1).max(1));
                image::RgbaImage::from_fn(width, height, |x, y| {
                    let down = mix(head, foot, f64::from(y) / span);
                    let colour = seam.and_then(|seam| seam.across(x, down)).unwrap_or(down);
                    image::Rgba([colour[0], colour[1], colour[2], 255])
                })
            }
        };
        Some(Plane { shade, id, pixels })
    }

    pub fn id(&self) -> PlaneId {
        self.id
    }

    pub fn area(&self) -> Rect {
        self.shade.area()
    }

    pub fn pixels(&self) -> &image::RgbaImage {
        &self.pixels
    }
}

/// The band images one frame holds, each against the shade it was drawn from: a frame makes an image
/// when a band's shade moves and keeps it while that shade stands.
#[derive(Clone, Debug, Default)]
pub struct Planes {
    held: Vec<Plane>,
    made: usize,
}

impl Planes {
    /// Holds a pass's `shades`: a shade already held keeps its image, a new one is drawn, and one
    /// the pass no longer names loses its image.
    pub fn hold(&mut self, shades: &[Shade]) {
        let mut held = Vec::with_capacity(shades.len());
        for shade in shades {
            match self.held.iter().position(|plane| plane.shade == *shade) {
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
    /// The seam that widens a plane `widened` columns, and nothing for a palette with no well or no
    /// shadow to spell in a pixel, where there is no fall to start from.
    fn of(widened: u16, cell: CellSize, shadow: Color, palette: &Palette) -> Option<Seam> {
        Some(Seam {
            widened,
            cell,
            well: channels(palette.band(Band::Well))?,
            shadow: channels(shadow)?,
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

#[cfg(test)]
mod tests;
