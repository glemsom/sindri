//! A picture an answer points at, resolved to a file inside the workspace and measured into
//! the cells it will take: a source is a file the run could read, and one that leaves the
//! workspace is refused with the reason the caption prints (§12, #55).
//!
//! Measuring is all this module does to a source, and it spends no pixel doing it: the path
//! is resolved, the picture's own declared extent is read — an SVG's, or a raster's header —
//! and that is divided by the cell it is drawn at. That is the number a transcript needs
//! before it can reserve a row for a picture, and it is the same promise ADR-0003 makes of a
//! mermaid fence, held for a file instead of a diagram.
//!
//! A picture is *of* something, and the words saying so are the block's: the alternative text
//! an answer wrote over the source, or the source itself when it wrote none. Those words are
//! laid out as rows either way, because a picture's block copies as its words and never as
//! the bytes it is drawn from — so a reader who marks a screenshot and pastes it into a
//! prompt pastes what it is called, which is the whole of what a terminal can honestly hand
//! out.
//!
//! The pixels are not here either. A picture is drawn by [`crate::pictures`], when the band
//! first shows it, and [`Surface`] is the one value that says how big the cells it is measured
//! against are and whether this terminal places a picture at all.

use std::path::{Path, PathBuf};

use crate::graphics::{CellSize, Footprint, Svg};

/// The facts a picture is measured against: the workspace a source has to resolve inside, the
/// pixels in one cell, and whether this terminal draws pictures at all.
///
/// Those three are one value because they are one question — how big is this picture here —
/// and because the answer is part of the rows an entry was laid out as (ADR-0008): a frame
/// that changed any of them lays its rows out again, and a frame that changed none re-lays
/// nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    /// The directory a source is resolved inside, and nothing for a frame that was told no
    /// directory: a frame with no workspace draws no picture rather than guessing at one.
    workspace: Option<PathBuf>,
    /// The pixels in one cell of the terminal, which is the size a picture is measured and
    /// drawn at.
    cell: CellSize,
    /// Whether this terminal places pictures: one that speaks no protocol for them is given
    /// the words instead, because a picture that cannot be placed must degrade whole — a
    /// block of rows nothing fills is a hole in the transcript (§12, ADR-0003).
    draws: bool,
}

impl Surface {
    /// The surface of a frame nobody told anything: no workspace, the nominal cell, and no
    /// protocol for pictures. What a fixture draws on, so a frame fixture holds the words an
    /// answer wrote and never the bytes of a placement (ADR-0012).
    pub fn none() -> Surface {
        Surface::default()
    }

    /// The surface of a frame that was told where it stands, how big its cells are, and
    /// whether its terminal places a picture.
    pub fn of(workspace: Option<&Path>, cell: CellSize, draws: bool) -> Surface {
        Surface {
            workspace: workspace.map(Path::to_path_buf),
            cell,
            draws,
        }
    }

    /// The directory a source is resolved inside.
    pub fn workspace(&self) -> Option<&Path> {
        self.workspace.as_deref()
    }

    /// The pixels in one cell.
    pub fn cell(&self) -> CellSize {
        self.cell
    }

    /// Whether this terminal places pictures.
    pub fn draws(&self) -> bool {
        self.draws
    }
}

/// A picture an answer pointed at, measured: the source as it was written, the file it
/// resolved to, and the cells it takes at the width the band has.
///
/// The source is kept beside the path because the two answer different questions. The source
/// is the words the answer wrote, which is what the reader marked and what a second answer
/// writing the same words shares; the path is the file the pixels are read from (ADR-0008).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    /// The `src` as the answer wrote it, which is the first half of what a placement is kept
    /// by.
    source: String,
    /// The file it resolved to, which is what the pixels are made from.
    path: PathBuf,
    /// The columns it takes in the band it was measured for.
    cols: u16,
    /// The rows it takes, which is how many rows of the transcript stand for it.
    rows: u16,
}

impl Picture {
    /// The `src` as the answer wrote it.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The file the picture is drawn from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The columns this picture takes.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// The rows this picture takes.
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// The pixels this picture is drawn at on a `cell`: the size the placement is asked for,
    /// so no terminal has to stretch a picture it was handed.
    pub fn pixels(&self, cell: CellSize) -> (u32, u32) {
        (
            u32::from(self.cols) * cell.width(),
            u32::from(self.rows) * cell.height(),
        )
    }
}

/// The words a picture is spoken of as: the alternative text an answer wrote over it, and the
/// source when it wrote none — a picture with no words would copy as nothing at all.
pub fn words(source: &str, alt: &str) -> String {
    match alt.trim() {
        "" => source.to_owned(),
        alt => alt.to_owned(),
    }
}

/// The picture `source` names, measured into the cells it takes in a band `room` columns wide
/// on `surface`.
///
/// This is the whole of what a transcript has to know before it can lay a picture out, and it
/// costs no pixel: the path is resolved, the picture's own extent is read, and that is
/// divided by the cell. A source that resolves outside the workspace, and one the read cannot
/// make a picture of, are refused here rather than at the placement, so the row standing for
/// it can say why (§12).
pub fn measure(source: &str, room: u16, surface: &Surface) -> Result<Picture, Refusal> {
    let path = resolve(source, surface.workspace())?;
    let (width, height) = size(&path).map_err(|error| Refusal::Unreadable { reason: error })?;
    let footprint = Footprint::fit(width, height, surface.cell(), room).map_err(Refusal::of)?;
    Ok(Picture {
        source: source.to_owned(),
        path,
        cols: footprint.cols(),
        rows: footprint.rows(),
    })
}

/// Why a picture is not drawn: the reason a caption prints in the place of it (§12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A frame that was told no workspace has nowhere to look a source up, so it draws no
    /// picture rather than one from a directory nobody named.
    Nowhere,
    /// The source resolves outside the workspace, which is a file the run was never trusted
    /// with and so not one an answer may point the reader at.
    Outside {
        /// The source as the answer wrote it, because a caption has to say what it refused.
        source: String,
    },
    /// The picture could not be read as one, and what the reader said was wrong with it.
    Unreadable {
        /// Why it is not a picture, kept whole because it is what the caption prints.
        reason: String,
    },
}

impl Refusal {
    /// The reason as the one sentence a caption prints, in the same voice a fence's refusals
    /// are said in (§12).
    pub fn message(&self) -> String {
        match self {
            Refusal::Nowhere => String::from("there is no workspace to look for the picture in"),
            Refusal::Outside { source } => format!(
                "{source} is outside the workspace, and only a file the run could read is drawn"
            ),
            Refusal::Unreadable { reason } => format!("the picture could not be read: {reason}"),
        }
    }

    /// A refusal of the picture code itself, said in this module's words: a picture the
    /// renderer would not measure is one this module could not read.
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

/// The path `source` resolves to, which has to be a file inside `workspace`: the canonical
/// path of both, so neither `..` nor a symbolic link can walk an answer out of the directory
/// the run was trusted with.
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

/// The extent `path` declares, in pixels: an SVG's own, which is measured with no pixel
/// spent, or the extent a raster picture's header carries — read without decoding the picture,
/// which is the half of "measured without pixels" that belongs to a file rather than to an
/// SVG.
fn size(path: &Path) -> Result<(u32, u32), String> {
    if is_svg(path) {
        let drawing = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        return Svg::parse(&drawing)
            .map(|svg| svg.size())
            .map_err(|refusal| refusal.message());
    }
    image::image_dimensions(path).map_err(|error| error.to_string())
}

/// Whether `path` is an SVG, which this crate draws itself rather than decodes as a raster:
/// the one question that tells the two ways of making a picture apart.
fn is_svg(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}
