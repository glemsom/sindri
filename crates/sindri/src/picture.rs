//! A picture an answer points at: its source resolved inside the workspace, its extent read
//! from the file rather than drawn from it, and its words kept beside it, because a picture's
//! block copies as its words and never as its bytes.

use std::path::{Path, PathBuf};

use crate::graphics::{CellSize, Footprint, Svg};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    workspace: Option<PathBuf>,
    cell: CellSize,
    draws: bool,
}

impl Surface {
    pub fn none() -> Surface {
        Surface::default()
    }

    pub fn of(workspace: Option<&Path>, cell: CellSize, draws: bool) -> Surface {
        Surface {
            workspace: workspace.map(Path::to_path_buf),
            cell,
            draws,
        }
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
    /// The SVG a mermaid fence's source became, kept as text rather than as a parsed tree: a
    /// row is copied and compared like any other value, so the drawing is parsed when drawn.
    Drawing(String),
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

    /// The pixels of this picture at the cells it was measured into, or nothing when the file
    /// has gone since it was measured. A raster is scaled by a filter that averages rather than
    /// drops, because nearest neighbour loses whole letters of a screenshot fitted into a band.
    pub fn rasterise(&self, cell: CellSize) -> Option<image::RgbaImage> {
        let (width, height) = self.pixels(cell);
        let drawing = match &self.drawn {
            Drawn::Drawing(svg) => svg.clone(),
            Drawn::File(path) if is_svg(path) => std::fs::read_to_string(path).ok()?,
            Drawn::File(path) => {
                let picture = image::open(path).ok()?;
                return Some(
                    picture
                        .resize_exact(width, height, image::imageops::FilterType::Triangle)
                        .to_rgba8(),
                );
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

pub fn measure(source: &str, room: u16, surface: &Surface) -> Result<Picture, Refusal> {
    let path = resolve(source, surface.workspace())?;
    of(source, Drawn::File(path), room, surface)
}

pub fn drawing(source: &str, svg: &str, room: u16, surface: &Surface) -> Result<Picture, Refusal> {
    of(source, Drawn::Drawing(svg.to_owned()), room, surface)
}

fn of(source: &str, drawn: Drawn, room: u16, surface: &Surface) -> Result<Picture, Refusal> {
    let (width, height) = extent(&drawn)?;
    let footprint = Footprint::fit(width, height, surface.cell(), room).map_err(Refusal::of)?;
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
    Outside {
        source: String,
    },
    Unreadable {
        /// The read's own words, kept rather than its error type because the caption prints them.
        reason: String,
    },
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
    let names_a_terminal_that_places_pictures = |word: &str| {
        let word = word.to_ascii_lowercase();
        ["kitty", "ghostty", "wezterm"]
            .iter()
            .any(|name| word.contains(name))
    };
    term.is_some_and(names_a_terminal_that_places_pictures)
        || program.is_some_and(names_a_terminal_that_places_pictures)
}

/// The canonical path of `source` and of `workspace`, so neither `..` nor a symbolic link can
/// walk an answer out of the directory the run was trusted with.
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

/// The extent `drawn` declares: an SVG's own, or a raster's header read without decoding it.
fn extent(drawn: &Drawn) -> Result<(u32, u32), Refusal> {
    let unreadable = |reason: String| Refusal::Unreadable { reason };
    match drawn {
        Drawn::Drawing(svg) => measured(svg),
        Drawn::File(path) if is_svg(path) => {
            let svg =
                std::fs::read_to_string(path).map_err(|error| unreadable(error.to_string()))?;
            measured(&svg)
        }
        Drawn::File(path) => {
            image::image_dimensions(path).map_err(|error| unreadable(error.to_string()))
        }
    }
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
