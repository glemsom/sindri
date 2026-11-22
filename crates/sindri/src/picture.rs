use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::graphics::{Ceiling, CellSize, Footprint, Svg};
use crate::link;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    workspace: Option<PathBuf>,
    cell: CellSize,
    draws: bool,
    sizing: bool,
    band: Option<u16>,
}

impl Surface {
    pub fn none() -> Surface {
        Surface::default()
    }

    pub fn of(workspace: Option<&Path>, cell: CellSize, draws: bool, sizing: bool) -> Surface {
        Surface {
            workspace: workspace.map(Path::to_path_buf),
            cell,
            draws,
            sizing,
            band: None,
        }
    }

    pub fn band(self, rows: u16) -> Surface {
        Surface {
            band: Some(rows),
            ..self
        }
    }

    pub fn rows(&self) -> u16 {
        self.band.unwrap_or(u16::MAX)
    }

    pub fn sizing(&self) -> bool {
        self.sizing
    }

    pub fn workspace(&self) -> Option<&Path> {
        self.workspace.as_deref()
    }

    pub fn cell(&self) -> CellSize {
        self.cell
    }

    pub fn draws(&self) -> bool {
        self.draws
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    source: String,
    drawn: Drawn,
    cols: u16,
    rows: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drawn {
    File(PathBuf),
    Drawing(String),
    Carried(&'static [u8]),
}

impl Picture {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn pixels(&self, cell: CellSize) -> (u32, u32) {
        (
            u32::from(self.cols) * cell.width(),
            u32::from(self.rows) * cell.height(),
        )
    }

    pub fn rasterise(&self, cell: CellSize) -> Option<image::RgbaImage> {
        let (width, height) = self.pixels(cell);
        let drawing = match &self.drawn {
            Drawn::Drawing(svg) => svg.clone(),
            Drawn::File(path) if is_svg(path) => std::fs::read_to_string(path).ok()?,
            Drawn::File(path) => {
                return Some(raster(image::open(path).ok()?, (width, height), cell));
            }
            Drawn::Carried(bytes) => {
                return Some(raster(
                    image::load_from_memory(bytes).ok()?,
                    (width, height),
                    cell,
                ));
            }
        };
        Svg::parse(&drawing)
            .ok()?
            .rasterise_at((width, height))
            .ok()
    }
}

pub fn words(source: &str, alt: &str) -> String {
    match alt.trim() {
        "" => source.to_owned(),
        alt => alt.to_owned(),
    }
}

pub fn measure(source: &str, ceiling: Ceiling, surface: &Surface) -> Result<Picture, Refusal> {
    let path = resolve(source, surface.workspace())?;
    of(source, Drawn::File(path), ceiling, surface)
}

pub fn drawing(
    source: &str,
    svg: &str,
    ceiling: Ceiling,
    surface: &Surface,
) -> Result<Picture, Refusal> {
    of(source, Drawn::Drawing(svg.to_owned()), ceiling, surface)
}

pub fn carried(
    source: &str,
    bytes: &'static [u8],
    ceiling: Ceiling,
    surface: &Surface,
) -> Result<Picture, Refusal> {
    of(source, Drawn::Carried(bytes), ceiling, surface)
}

fn of(source: &str, drawn: Drawn, ceiling: Ceiling, surface: &Surface) -> Result<Picture, Refusal> {
    let (width, height) = extent(&drawn)?;
    let footprint = Footprint::fit(width, height, surface.cell(), ceiling).map_err(Refusal::of)?;
    Ok(Picture {
        source: source.to_owned(),
        drawn,
        cols: footprint.cols(),
        rows: footprint.rows(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Nowhere,
    Outside { source: String },
    Unreadable { reason: String },
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::Nowhere => String::from("there is no workspace to look for the picture in"),
            Refusal::Outside { source } => format!(
                "{source} is outside the workspace, and only a file the run could read is drawn"
            ),
            Refusal::Unreadable { reason } => format!("the picture could not be read: {reason}"),
        }
    }

    fn of(refusal: crate::graphics::Refusal) -> Refusal {
        Refusal::Unreadable {
            reason: refusal.message(),
        }
    }

    fn unreadable(error: impl std::fmt::Display) -> Refusal {
        Refusal::Unreadable {
            reason: error.to_string(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message())
    }
}

/// Whether this terminal speaks the Kitty graphics protocol, read off `TERM` and
/// `TERM_PROGRAM`: asking it costs a round trip this launch has spent on the cell size, and a
/// terminal naming neither is taken to place no picture, so the words stay whole.
pub fn speaks_kitty(term: Option<&str>, program: Option<&str>) -> bool {
    link::named(term, program, &["kitty", "ghostty", "wezterm"])
}

fn resolve(source: &str, workspace: Option<&Path>) -> Result<PathBuf, Refusal> {
    let Some(workspace) = workspace else {
        return Err(Refusal::Nowhere);
    };
    let root = workspace
        .canonicalize()
        .map_err(|error| Refusal::Unreadable {
            reason: error.to_string(),
        })?;
    let path = root
        .join(source)
        .canonicalize()
        .map_err(|error| Refusal::Unreadable {
            reason: error.to_string(),
        })?;
    match path.starts_with(&root) {
        true => Ok(path),
        false => Err(Refusal::Outside {
            source: source.to_owned(),
        }),
    }
}

fn extent(drawn: &Drawn) -> Result<(u32, u32), Refusal> {
    match drawn {
        Drawn::Drawing(svg) => measured(svg),
        Drawn::File(path) if is_svg(path) => {
            let svg = std::fs::read_to_string(path).map_err(Refusal::unreadable)?;
            measured(&svg)
        }
        Drawn::File(path) => image::image_dimensions(path).map_err(Refusal::unreadable),
        Drawn::Carried(bytes) => header(bytes),
    }
}

fn header(bytes: &[u8]) -> Result<(u32, u32), Refusal> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(Refusal::unreadable)?;
    reader.into_dimensions().map_err(Refusal::unreadable)
}

const FLOOR: u8 = 16;

const SHARE: u32 = 4;

fn raster(
    picture: image::DynamicImage,
    (width, height): (u32, u32),
    cell: CellSize,
) -> image::RgbaImage {
    let mut pixels = picture
        .resize_exact(width, height, image::imageops::FilterType::Triangle)
        .to_rgba8();
    feathered(&mut pixels, cell);
    pixels
}

fn feathered(pixels: &mut image::RgbaImage, cell: CellSize) {
    let (width, height) = pixels.dimensions();
    let across = feather(cell.width(), width);
    let down = feather(cell.height(), height);
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        let light = pixel[3];
        pixel[3] = match light < FLOOR {
            true => 0,
            false => {
                (f32::from(light) * fade(x, width, across) * fade(y, height, down)).round() as u8
            }
        };
    }
}

fn feather(cell: u32, extent: u32) -> u32 {
    cell.min(extent / SHARE).max(1)
}

fn fade(at: u32, extent: u32, span: u32) -> f32 {
    let distance = at.min(extent.saturating_sub(1).saturating_sub(at));
    distance.min(span) as f32 / span as f32
}

fn measured(svg: &str) -> Result<(u32, u32), Refusal> {
    Svg::parse(svg)
        .map(|drawing| drawing.size())
        .map_err(|refusal| refusal.message())
        .map_err(|reason| Refusal::Unreadable { reason })
}

fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}
