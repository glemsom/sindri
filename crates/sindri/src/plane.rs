use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

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
pub const PLACEMENT: u32 = 1;

/// The depth every plane is placed at. Kitty draws an image under a cell that carries a
/// background only below `INT32_MIN / 2`, and a seam's edge ink is one such cell.
pub const DEPTH: i64 = i32::MIN as i64 / 2 - 1;

/// The two figures of the foot a bar stands under.
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
    Gauge,
    Bar(Bar),
    Bezel,
}

impl PlaneId {
    pub const ALL: [PlaneId; 8] = [
        PlaneId::Composer,
        PlaneId::Rail,
        PlaneId::Transcript,
        PlaneId::Caret,
        PlaneId::Gauge,
        PlaneId::Bar(Bar::Input),
        PlaneId::Bar(Bar::Output),
        PlaneId::Bezel,
    ];

    /// The identity the image is held under and layered by: two images at the frame's one depth
    /// stand by identity, so the caret's light is read over the band it lights.
    pub fn image(self) -> u32 {
        match self {
            PlaneId::Composer => 333,
            PlaneId::Rail => 334,
            PlaneId::Transcript => 335,
            PlaneId::Caret => 336,
            PlaneId::Gauge => 338,
            PlaneId::Bar(Bar::Input) => 339,
            PlaneId::Bar(Bar::Output) => 340,
            PlaneId::Bezel => 341,
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
        seam: Option<Seam>,
    },
    Composer {
        area: Rect,
        cell: Option<CellSize>,
        prompt: Color,
        rail: Color,
        seam: Option<Seam>,
    },
    Gauge {
        area: Rect,
        cell: CellSize,
        share: u8,
        turned: bool,
        tone: Color,
        calm: Color,
        accent: Color,
        tick: Color,
        peak: u8,
    },
    Bar {
        area: Rect,
        cell: CellSize,
        bar: Bar,
        share: u8,
        tone: Color,
        ink: Color,
        peak: u8,
    },
    Bezel {
        area: Rect,
        cell: CellSize,
        tone: Color,
        weight: u8,
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
            seam: Some(Seam::across(palette)),
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
            seam: seam_of(shape, palette),
        })
    }

    /// An instrument stands over the cells of a fact's row and claims none of them: its pixels
    /// are the only surface it adds, and the row's own shade has claimed the rest.
    pub fn gauge(
        area: Rect,
        cell: Option<CellSize>,
        gauge: instrument::Gauge,
        palette: &Palette,
    ) -> Option<Shade> {
        if !covers(area) {
            return None;
        }
        let cell = cell?;
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
        Some(Shade::Gauge {
            area,
            cell,
            share: gauge.share.rungs(),
            turned: gauge.turned,
            tone,
            calm,
            accent,
            tick,
            peak: palette.peak(Band::Rail),
        })
    }

    pub fn bezel(area: Rect, cell: Option<CellSize>, palette: &Palette) -> Option<Shade> {
        if !covers(area) {
            return None;
        }
        let cell = cell?;
        let tone = palette.band(Band::Rail);
        channels(tone)?;
        // The groove is light taken from the rung and given back to it, and the most a plane under
        // a word may take is the most the palette proves: the seam's own weight where it fits.
        let weight = palette.seam().min(palette.peak(Band::Rail));
        Some(Shade::Bezel {
            area,
            cell,
            tone,
            weight,
        })
    }

    pub fn bar(
        area: Rect,
        cell: Option<CellSize>,
        bar: Bar,
        share: instrument::Share,
        palette: &Palette,
    ) -> Option<Shade> {
        if !covers(area) {
            return None;
        }
        let cell = cell?;
        let tone = palette.band(Band::Rail);
        let ink = palette.colour(match bar {
            Bar::Input => Ink::Accent,
            Bar::Output => Ink::Muted,
        });
        if [tone, ink].iter().any(|colour| channels(*colour).is_none()) {
            return None;
        }
        Some(Shade::Bar {
            area,
            cell,
            bar,
            share: share.rungs(),
            tone,
            ink,
            peak: palette.peak(Band::Rail),
        })
    }

    pub fn caret(
        band: Rect,
        cell: Option<CellSize>,
        caret: u16,
        palette: &Palette,
    ) -> Option<Shade> {
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

    /// The one cell of the band the seam is drawn in, on the axis that parts it from the band
    /// beside, and nothing where this band parts nothing.
    pub fn seam(&self) -> Option<(Axis, Rect)> {
        let seam = match self {
            Shade::Transcript { .. }
            | Shade::Caret { .. }
            | Shade::Gauge { .. }
            | Shade::Bar { .. }
            | Shade::Bezel { .. } => return None,
            Shade::Rail { seam, .. } | Shade::Composer { seam, .. } => (*seam)?,
        };
        let area = self.area();
        Some((
            seam.axis,
            match seam.axis {
                Axis::Down => Rect { width: 1, ..area },
                Axis::Across => Rect { height: 1, ..area },
            },
        ))
    }

    pub fn id(&self) -> Option<PlaneId> {
        match self {
            Shade::Transcript { .. } => Some(PlaneId::Transcript),
            Shade::Rail { .. } => Some(PlaneId::Rail),
            Shade::Composer { .. } => Some(PlaneId::Composer),
            Shade::Caret { .. } => Some(PlaneId::Caret),
            Shade::Gauge { .. } => Some(PlaneId::Gauge),
            Shade::Bar { bar, .. } => Some(PlaneId::Bar(*bar)),
            Shade::Bezel { .. } => Some(PlaneId::Bezel),
        }
    }

    pub fn area(&self) -> Rect {
        match self {
            Shade::Transcript { area, .. }
            | Shade::Rail { area, .. }
            | Shade::Composer { area, .. }
            | Shade::Gauge { area, .. }
            | Shade::Bar { area, .. }
            | Shade::Bezel { area, .. } => *area,
            Shade::Caret { band, .. } => *band,
        }
    }

    pub fn places(&self) -> bool {
        self.cell().is_some()
    }

    pub fn row(&self, row: u16) -> Color {
        match self {
            Shade::Transcript { well, .. } => *well,
            Shade::Rail { tone, .. }
            | Shade::Gauge { tone, .. }
            | Shade::Bar { tone, .. }
            | Shade::Bezel { tone, .. } => *tone,
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
                seam,
                ..
            } => Image::Rail {
                area: *area,
                cell,
                tone: channels(*tone)?,
                face: *face,
                peak: *peak,
                seam: *seam,
            },
            Shade::Composer {
                area,
                prompt,
                rail,
                seam,
                ..
            } => Image::Composer {
                area: *area,
                cell,
                prompt: channels(*prompt)?,
                rail: channels(*rail)?,
                seam: *seam,
            },
            Shade::Gauge {
                area,
                tone,
                calm,
                accent,
                tick,
                peak,
                ..
            } => Image::Gauge {
                cols: area.width,
                cell,
                tone: channels(*tone)?,
                calm: channels(*calm)?,
                turned: channels(*accent)?,
                tick: channels(*tick)?,
                peak: *peak,
            },
            Shade::Bar {
                area,
                bar,
                tone,
                ink,
                peak,
                ..
            } => Image::Bar {
                cols: area.width,
                cell,
                bar: *bar,
                tone: channels(*tone)?,
                ink: channels(*ink)?,
                peak: *peak,
            },
            Shade::Bezel {
                area, tone, weight, ..
            } => Image::Bezel {
                area: *area,
                cell,
                tone: channels(*tone)?,
                weight: *weight,
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
            down: 0,
            height: u32::from(area.height) * cell.height(),
        };
        Some(match self {
            Shade::Transcript { area, .. }
            | Shade::Rail { area, .. }
            | Shade::Composer { area, .. }
            | Shade::Bezel { area, .. } => whole(*area),
            Shade::Bar { area, share, .. } => {
                let width = u32::from(area.width) * cell.width();
                Placement {
                    down: u32::from(*share),
                    width,
                    height: 1,
                    ..whole(*area)
                }
            }
            Shade::Gauge {
                area,
                share,
                turned,
                ..
            } => {
                let width = u32::from(area.width) * cell.width();
                Placement {
                    from: match turned {
                        true => width,
                        false => 0,
                    },
                    down: u32::from(*share),
                    width,
                    height: 1,
                    ..whole(*area)
                }
            }
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
                    down: 0,
                    height: u32::from(band.height) * cell.height(),
                }
            }
        })
    }

    pub fn claim(&self, buffer: &mut Buffer, palette: &Palette) {
        if matches!(
            self,
            Shade::Caret { .. } | Shade::Gauge { .. } | Shade::Bar { .. } | Shade::Bezel { .. }
        ) {
            return;
        }
        let area = self.area();
        if self.places() {
            pen::claim(buffer, area, palette.ground(None));
            return;
        }
        let seam = self.seam();
        for row in 0..area.height {
            let y = area.y.saturating_add(row);
            let ground = palette.over(self.row(row));
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
            Shade::Gauge { cell, .. } | Shade::Bar { cell, .. } | Shade::Bezel { cell, .. } => {
                Some(*cell)
            }
        }
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
        seam: Option<Seam>,
    },
    Composer {
        area: Rect,
        cell: CellSize,
        prompt: [u8; 3],
        rail: [u8; 3],
        seam: Option<Seam>,
    },
    /// The ladder a gauge's share is carried by: one rung per hundredth of a block `cols` cells
    /// wide, the ink a meter is filled with beside the ink it turns to — which band a rung is
    /// cropped from is the placement's, so a gauge that turned sends no pixel.
    Gauge {
        cols: u16,
        cell: CellSize,
        tone: [u8; 3],
        calm: [u8; 3],
        turned: [u8; 3],
        tick: [u8; 3],
        peak: u8,
    },
    /// The ladder a figure's share is carried by: one rung per hundredth of a block `cols` cells
    /// wide.
    Bar {
        cols: u16,
        cell: CellSize,
        bar: Bar,
        tone: [u8; 3],
        ink: [u8; 3],
        peak: u8,
    },
    /// The region's own pixels, its top and left edges taken `weight` channels into shadow and
    /// its bottom and right edges lifted by them.
    Bezel {
        area: Rect,
        cell: CellSize,
        tone: [u8; 3],
        weight: u8,
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
            Image::Gauge { .. } => PlaneId::Gauge,
            Image::Bar { bar, .. } => PlaneId::Bar(bar),
            Image::Bezel { .. } => PlaneId::Bezel,
        }
    }

    fn cell(self) -> CellSize {
        match self {
            Image::Transcript { cell, .. }
            | Image::Rail { cell, .. }
            | Image::Composer { cell, .. }
            | Image::Caret { cell, .. }
            | Image::Gauge { cell, .. }
            | Image::Bar { cell, .. }
            | Image::Bezel { cell, .. } => cell,
        }
    }

    /// The cells the image covers at its own corner, which is where a band's plane stands.
    pub fn area(self) -> Rect {
        match self {
            Image::Transcript { area, .. }
            | Image::Rail { area, .. }
            | Image::Composer { area, .. } => area,
            Image::Caret { rows, .. } => Rect::new(0, 0, LIGHT_COLS, rows),
            Image::Gauge { cols, .. } | Image::Bar { cols, .. } => Rect::new(0, 0, cols, 1),
            Image::Bezel { area, .. } => area,
        }
    }

    fn seam(self) -> Option<Seam> {
        match self {
            Image::Rail { seam, .. } | Image::Composer { seam, .. } => seam,
            Image::Transcript { .. }
            | Image::Caret { .. }
            | Image::Gauge { .. }
            | Image::Bar { .. }
            | Image::Bezel { .. } => None,
        }
    }

    fn rows(self) -> u16 {
        match self {
            Image::Transcript { area, .. }
            | Image::Rail { area, .. }
            | Image::Composer { area, .. } => area.height,
            Image::Caret { rows, .. } => rows,
            Image::Gauge { .. } | Image::Bar { .. } => 1,
            Image::Bezel { area, .. } => area.height,
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
            Image::Gauge { .. } | Image::Bar { .. } | Image::Bezel { .. } => [0; 3],
        }
    }

    /// The light this image's pixels stand in, at the image's own pixel origin.
    fn light(self) -> Light {
        let cell = self.cell();
        let (across, down) = (f64::from(cell.width()), f64::from(cell.height()));
        let light = match self {
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
            Image::Gauge { .. } | Image::Bar { .. } | Image::Bezel { .. } => Light::NONE,
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
        };
        match self.seam() {
            Some(seam) => light.parted(seam, cell),
            None => light,
        }
    }

    fn pixels(self) -> image::RgbaImage {
        match self {
            Image::Gauge {
                cols,
                cell,
                tone,
                calm,
                turned,
                tick,
                peak,
            } => return gauge(cols, cell, tone, [calm, turned], tick, peak),
            Image::Bar {
                cols,
                cell,
                tone,
                ink,
                peak,
                ..
            } => return track(cols, cell, tone, ink, peak),
            Image::Bezel {
                area,
                cell,
                tone,
                weight,
            } => return groove(area, cell, tone, weight),
            _ => {}
        }
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
            let [red, green, blue] = self
                .rung(row)
                .map(|at| (i32::from(at) + lit).clamp(0, 255) as u8);
            image::Rgba([red, green, blue, 255])
        })
    }
}

/// The rungs an instrument's ladder is drawn at: one per hundredth of the block's width, one
/// pixel deep, which the terminal stretches down the row the rung is placed on.
const RUNGS: u32 = 101;

/// A pixel an instrument leaves to the band it is drawn over.
const NOTHING: [u8; 4] = [0, 0, 0, 0];

/// Two bands of `cols` pixels' width, each rung filled as deep as its hundredth, traced by its last
/// pixel and ticked on the track where the fill has not reached.
fn gauge(
    cols: u16,
    cell: CellSize,
    tone: [u8; 3],
    bands: [[u8; 3]; 2],
    tick: [u8; 3],
    peak: u8,
) -> image::RgbaImage {
    let width = u32::from(cols) * cell.width();
    let quarter = (width / 4).max(1);
    image::RgbaImage::from_fn(width * 2, RUNGS, |x, rung| {
        let (ink, x) = match x < width {
            true => (bands[0], x),
            false => (bands[1], x - width),
        };
        let fill = rung_share(rung).pixels(width);
        image::Rgba(match x < fill {
            true => lit(tone, ink, spend_of_edge(peak, x, fill)),
            false if x > 0 && x % quarter == 0 => lit(tone, tick, peak / 3),
            false => NOTHING,
        })
    })
}

/// A track of `cols` pixels in the ink a figure takes: each rung filled as deep as its hundredth,
/// and nothing beside it.
fn track(cols: u16, cell: CellSize, tone: [u8; 3], ink: [u8; 3], peak: u8) -> image::RgbaImage {
    let width = u32::from(cols) * cell.width();
    image::RgbaImage::from_fn(width, RUNGS, |x, rung| {
        let fill = rung_share(rung).pixels(width);
        image::Rgba(match x < fill {
            true => lit(tone, ink, peak),
            false => NOTHING,
        })
    })
}

fn groove(area: Rect, cell: CellSize, tone: [u8; 3], weight: u8) -> image::RgbaImage {
    let (width, height) = (
        u32::from(area.width) * cell.width(),
        u32::from(area.height) * cell.height(),
    );
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

/// The hundredth of the block a rung of a ladder stands for.
fn rung_share(rung: u32) -> instrument::Share {
    instrument::Share::of(u8::try_from(rung).unwrap_or(100))
}

/// The light a gauge's fill spends at the pixel `x` of a fill `fill` pixels deep: three quarters of
/// the most the rail proves, and all of it on the trace that ends the fill.
fn spend_of_edge(peak: u8, x: u32, fill: u32) -> u8 {
    match x + 1 == fill {
        true => peak,
        false => peak / 4 * 3,
    }
}

/// The rail's `tone` with `spend` channels of light added in the hue of the `ink` the fact is
/// stated in: a pixel of an instrument is the rung it stands on and never an ink over it, so every
/// word the rail states keeps its floor where the instrument is drawn under it.
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
            Axis::Across => cell.height(),
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

/// Where an image stands and which rect of it is shown there: the cells it is placed on, and the
/// source rect — `skip`/`from`/`width` along that axis and `down`/`height` down the other.
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
    /// The pixel row of the image shown first, and the pixels shown from it down.
    pub down: u32,
    pub height: u32,
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
