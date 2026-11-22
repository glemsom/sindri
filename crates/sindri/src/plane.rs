//! The planes a frame places under its cells: the shade a band's ground is drawn from, the image
//! it is drawn as, the identity a placement is named by, and the bytes that take one away again.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::graphics::CellSize;
use crate::ledger::Rail;
use crate::palette::{Band, Palette, channels};
use crate::pen;

/// The glyph the rail's first column is ruled in where a palette spells no edge: the seam a terminal
/// with no colour of its own is given, so its bands are parted by a line rather than by nothing.
const COLUMN: char = '│';

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
    /// The rail on its own tone down every row it claims, parted from the transcript band beside it
    /// by its first column where it is a ledger.
    Rail {
        area: Rect,
        cell: Option<CellSize>,
        tone: Color,
        shape: Rail,
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

    /// The rail's band over the rows and columns it covers, with the one column that parts it from
    /// the transcript band beside it.
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
            shape,
        })
    }

    /// The cells of this band the frame's seam is drawn in, and none for a band that parts nothing:
    /// the rail's first column down every row it claims, and the row that heads the composer's band.
    pub fn seam(&self) -> Option<Rect> {
        let area = self.area();
        match self {
            Shade::Transcript { .. } => None,
            Shade::Rail {
                shape: Rail::Strip, ..
            } => None,
            Shade::Rail { .. } => Some(Rect { width: 1, ..area }),
            Shade::Composer { .. } => Some(Rect { height: 1, ..area }),
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
    /// under them, and otherwise every row on the colour the shade says it sits on. The seam's own
    /// cells are claimed once, in the one edge ink, whatever the terminal can place.
    pub fn claim(&self, buffer: &mut Buffer, palette: &Palette) {
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
            let Some(taken) = seam
                .filter(|seam| seam.y <= y && y < seam.bottom())
                .map(|seam| Rect {
                    x: seam.x,
                    width: seam.width,
                    ..line
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
            seam_cell(buffer, taken, self.column_seam(), palette);
        }
    }

    /// Whether this band's seam runs down its columns, which the rail's does and the composer's band
    /// does not.
    fn column_seam(&self) -> bool {
        matches!(self, Shade::Rail { .. })
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

/// The cells of one row the frame's seam is drawn in: filled in the one edge ink, and ruled in the
/// dim ink where the palette spells no edge at all.
fn seam_cell(buffer: &mut Buffer, line: Rect, column: bool, palette: &Palette) {
    match (palette.draws_edge(), column) {
        (true, _) => pen::claim(buffer, line, palette.over(palette.edge())),
        (false, true) => {
            for x in line.x..line.right() {
                pen::glyph(buffer, x, line.y, COLUMN, palette.dim_edge(), palette);
            }
        }
        (false, false) => {}
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
