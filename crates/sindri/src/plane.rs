//! The planes a frame places under its cells: the shade a band's ground is drawn from, the image
//! it is drawn as, the identity a placement is named by, and the bytes that take one away again.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::graphics::CellSize;
use crate::ledger::Rail;
use crate::palette::{self, Band, Edge, Palette, channels};
use crate::pen;

/// The columns of the transcript band a seam takes beside a rail: the ones the well falls into the
/// shadow across, so the rail is parted from the band beside it by a fall and not by a line.
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
    /// The rail on its own tone down every row it claims, and the seam it makes with the transcript
    /// band where it is a ledger.
    Rail {
        area: Rect,
        cell: Option<CellSize>,
        tone: Color,
        seam: Option<Seam>,
    },
    /// The composer's band: the prompt's band on every row but its last, which closes into the
    /// rail's tone under it.
    Composer {
        area: Rect,
        cell: Option<CellSize>,
        prompt: Color,
        rail: Color,
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

    /// The composer's band, with no plane under it unless the palette spells both its tones as
    /// pixels: the glow under the caret's column is drawn in the image, so it is a fact of the
    /// shade.
    pub fn composer(
        area: Rect,
        cell: Option<CellSize>,
        caret: Option<u16>,
        palette: &Palette,
    ) -> Option<Shade> {
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
            caret,
        })
    }

    /// The rail's band over the rows and columns it covers, and the seam it makes with the
    /// transcript band beside it.
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
        let cell = cell.filter(|_| channels(tone).is_some());
        Some(Shade::Rail {
            area,
            cell,
            tone,
            seam: Seam::of(area, shape, palette),
        })
    }

    /// The seam this shade makes with the band beside it, and none for a band that parts nothing.
    pub fn seam(&self) -> Option<Seam> {
        match self {
            Shade::Rail { seam, .. } => *seam,
            Shade::Transcript { .. } | Shade::Composer { .. } => None,
        }
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

    /// The colour that row of the band sits on: the well, the rail's own tone on every row of it,
    /// or a composer's own two tones.
    pub fn row(&self, row: u16) -> Color {
        match self {
            Shade::Transcript { well, .. } => *well,
            Shade::Rail { tone, .. } => *tone,
            Shade::Composer {
                area, prompt, rail, ..
            } => match row.saturating_add(1) >= area.height {
                true => *rail,
                false => *prompt,
            },
        }
    }

    /// Claims the band's cells from the shade: the terminal's own ground where a plane is drawn
    /// under them, and otherwise every row on the colour the shade says it sits on.
    pub fn claim(&self, buffer: &mut Buffer, palette: &Palette) {
        let area = self.area();
        if self.places() {
            pen::claim(buffer, area, palette.ground(None));
            return;
        }
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
    /// The image a shade is drawn as, and nothing for a band no plane stands under: the shade's
    /// row colours read between rows at the pixels of the cell, so every row keeps its own.
    pub(crate) fn draw(shade: Shade) -> Option<Plane> {
        let id = shade.id()?;
        let cell = shade.cell()?;
        let area = shade.area();
        let (width, height) = extent(area, cell);
        let rows = (0..area.height)
            .map(|row| channels(shade.row(row)))
            .collect::<Option<Vec<[u8; 3]>>>()?;
        let glow = match shade {
            Shade::Composer {
                caret: Some(caret), ..
            } => Some((f64::from(caret.saturating_sub(area.x)) + 0.5) * f64::from(cell.width())),
            _ => None,
        };
        let (down, across) = (f64::from(cell.height()), f64::from(cell.width()));
        let last = f64::from(area.height.saturating_sub(1));
        let pixels = image::RgbaImage::from_fn(width, height, |x, y| {
            let at = ((f64::from(y) + 0.5) / down - 0.5).clamp(0.0, last);
            let (above, below) = (at.floor(), at.ceil());
            let tone = mix(rows[above as usize], rows[below as usize], at - above);
            let light = glow.map_or(0.0, |center| {
                let distance = (f64::from(x) - center) / (across * 8.0);
                (-(distance * distance) * 2.0).exp() * 22.0
            });
            let channel = |at: u8| (f64::from(at) + light).clamp(0.0, 255.0) as u8;
            image::Rgba([channel(tone[0]), channel(tone[1]), channel(tone[2]), 255])
        });
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

/// The fall that parts the transcript band from the rail beside it: the columns it takes from the
/// transcript's own, each one's cells claimed in a shade of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seam {
    area: Rect,
    well: Color,
    shadow: Color,
}

impl Seam {
    /// The seam the band `area` of a `shape` rail makes, and none for a shape that lies under the
    /// transcript band, a palette with no colour to fall in, or a rail with no column beside it.
    pub fn of(area: Rect, shape: Rail, palette: &Palette) -> Option<Seam> {
        if shape != Rail::Ledger || !palette.draws_colour() {
            return None;
        }
        let reach = SEAM.min(area.x);
        (reach > 0).then(|| Seam {
            area: Rect {
                x: area.x - reach,
                width: reach,
                ..area
            },
            well: palette.band(Band::Well),
            shadow: palette.edge(Edge::Shadow),
        })
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    /// The colour column `x` of the seam falls in: the well at its first column and exactly the
    /// shadow at its last.
    pub fn column(&self, x: u16) -> Color {
        palette::blend(
            self.well,
            self.shadow,
            x.saturating_sub(self.area.x),
            self.area.width.saturating_sub(1),
        )
    }

    /// Claims the seam under the cells already drawn in its columns, so no word of the transcript
    /// band is cut by it.
    pub fn claim(&self, buffer: &mut Buffer, palette: &Palette) {
        for x in self.area.x..self.area.right() {
            pen::beneath(
                buffer,
                Rect {
                    x,
                    width: 1,
                    ..self.area
                },
                palette.over(self.column(x)),
            );
        }
    }
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
