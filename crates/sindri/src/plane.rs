use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::graphics::CellSize;
use crate::ledger::Rail;
use crate::palette::{Band, Palette, channels};
use crate::pen;

const EDGE_RULE: char = '│';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneId {
    Composer,
    Rail,
}

impl PlaneId {
    pub const ALL: [PlaneId; 2] = [PlaneId::Composer, PlaneId::Rail];

    pub fn image(self) -> u32 {
        match self {
            PlaneId::Composer => 333,
            PlaneId::Rail => 334,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    Transcript {
        area: Rect,
        well: Color,
    },
    Rail {
        area: Rect,
        cell: Option<CellSize>,
        tone: Color,
        shape: Rail,
    },
    Composer {
        area: Rect,
        cell: Option<CellSize>,
        prompt: Color,
        rail: Color,
        caret: Option<u16>,
    },
}

impl Shade {
    pub fn transcript(area: Rect, palette: &Palette) -> Option<Shade> {
        covers(area).then(|| Shade::Transcript {
            area,
            well: palette.band(Band::Well),
        })
    }

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

    pub fn seam(&self) -> Option<(Axis, Rect)> {
        let area = self.area();
        match self {
            Shade::Transcript { .. } => None,
            Shade::Rail {
                shape: Rail::Strip, ..
            } => None,
            Shade::Rail { .. } => Some((Axis::Down, Rect { width: 1, ..area })),
            Shade::Composer { .. } => Some((Axis::Across, Rect { height: 1, ..area })),
        }
    }

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

    pub fn places(&self) -> bool {
        self.cell().is_some()
    }

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
            Shade::Transcript { .. } => None,
            Shade::Rail { cell, .. } | Shade::Composer { cell, .. } => *cell,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Down,
    Across,
}

#[derive(Clone, Debug)]
pub struct Plane {
    shade: Shade,
    id: PlaneId,
    pixels: image::RgbaImage,
}

impl Plane {
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
        let (down, across) = (cell.height(), f64::from(cell.width()));
        let pixels = image::RgbaImage::from_fn(width, height, |x, y| {
            let tone = rows[(y / down) as usize];
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

#[derive(Clone, Debug, Default)]
pub struct Planes {
    held: Vec<Plane>,
    made: usize,
}

impl Planes {
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

pub fn delete(id: PlaneId) -> Vec<u8> {
    format!("\x1b_Ga=d,d=i,i={}\x1b\\", id.image()).into_bytes()
}

fn covers(area: Rect) -> bool {
    area.width > 0 && area.height > 0
}

fn extent(area: Rect, cell: CellSize) -> (u32, u32) {
    (
        u32::from(area.width) * cell.width(),
        u32::from(area.height) * cell.height(),
    )
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
