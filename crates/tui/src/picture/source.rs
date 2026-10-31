//! One picture's source, and what can be learned about it before a pixel is
//! spent: a markdown `src` resolved against the workspace, or a mermaid fence's
//! own body. Both answer the same two questions — how many pixels it is, and
//! what pixels it draws at a size — so the store above them never asks which
//! kind it holds.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat, ImageReader, Limits};

use crate::picture::svgs::Svgs;
use crate::picture::{BYTES, DIAGRAM_ROWS, MAX_ROWS, PIXELS, Refusal};

/// The formats the store draws: the four the decoders behind `ratatui-image`
/// link, and nothing else.
const DRAWS: [ImageFormat; 4] = [
    ImageFormat::Png,
    ImageFormat::Jpeg,
    ImageFormat::Gif,
    ImageFormat::WebP,
];

/// What a picture is drawn from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A `src` resolved to a file in the workspace: what an ordinary markdown
    /// image is.
    File(PathBuf),
    /// A fence's body: the diagram's own text, which needs no workspace, no
    /// file and no second read of anything but itself.
    Fence(String),
}

impl Source {
    /// The source a markdown `src` names: an absolute path as it is, anything
    /// else against the workspace, and a scheme refused by name rather than
    /// fetched. `~` is a name like any other — the frame expands nothing, and a
    /// shell is not in the loop. A path the frame cannot afford to read is
    /// refused here, before a decoder sees it.
    pub(super) fn of(workspace: &Path, src: &str) -> Result<Source, Refusal> {
        if let Some(scheme) = scheme(src) {
            return Err(Refusal::Scheme(scheme.to_owned()));
        }
        let path = Path::new(src);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            workspace.join(path)
        };
        guard(&path)?;
        Ok(Source::File(path))
    }

    /// The most rows this source may take: a picture's allowance for a file,
    /// a diagram's taller one for a fence — see [`MAX_ROWS`](crate::picture::MAX_ROWS).
    pub(super) fn allowance(&self) -> u16 {
        match self {
            Source::File(_) => MAX_ROWS,
            Source::Fence(_) => DIAGRAM_ROWS,
        }
    }

    /// The natural size of what this source draws, in pixels: a file's header
    /// and never its pixels, a fence's diagram from the svg the renderer wrote
    /// for it — which `svgs` holds, so a body measured twice is parsed once.
    pub(super) fn measured(&self, svgs: &Svgs) -> Result<(u32, u32), Refusal> {
        match self {
            Source::File(path) => dimensions(path),
            Source::Fence(body) => svgs.measured(body),
        }
    }

    /// The pixels this source draws, fitted into `width` by `height`, or `None`
    /// where nothing can be drawn: a file decoded under the cap its header was
    /// already held to, a fence rasterised on the panel's ground from the svg
    /// the store already has.
    pub(super) fn rasterised(&self, svgs: &Svgs, width: u32, height: u32) -> Option<DynamicImage> {
        match self {
            Source::File(path) => decode(path),
            Source::Fence(body) => svgs.rasterised(body, width, height),
        }
    }
}

/// What a file is, before any decoder sees it: a file, of a size the frame can
/// afford to read. A directory, a missing file and a file past the byte cap are
/// all answered here, without opening one.
fn guard(path: &Path) -> Result<(), Refusal> {
    let meta = std::fs::metadata(path).map_err(|error| unreadable(path, &error))?;
    if meta.is_dir() {
        return Err(Refusal::Directory);
    }
    if meta.len() > BYTES {
        return Err(Refusal::TooBig(meta.len()));
    }
    Ok(())
}

/// The scheme a `src` names, if it names one: `https:`, `http:`, `data:` and
/// any other, refused by name rather than fetched. A leading word of letters
/// followed by `:` is a scheme and not a path, which is also why a file with a
/// colon in its name is asked for by its absolute path.
fn scheme(src: &str) -> Option<&str> {
    let (head, _) = src.split_once(':')?;
    let mut chars = head.chars();
    let first = chars.next()?;
    (first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    .then_some(head)
}

/// The size in the file's header, and no pixel of it: a header read is what a
/// frame that measures every entry it lays out can afford.
fn dimensions(path: &Path) -> Result<(u32, u32), Refusal> {
    let file = File::open(path).map_err(|error| unreadable(path, &error))?;
    let reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(Refusal::from_io)?;
    let format = reader.format().ok_or(Refusal::NotAnImage)?;
    if !DRAWS.contains(&format) {
        return Err(Refusal::Unsupported(named(format)));
    }
    reader.into_dimensions().map_err(|_| Refusal::Damaged)
}

/// The pixels a measured file holds, decoded under the cap its header was
/// already held to: a file rewritten between the measure and the decode cannot
/// outgrow it either.
fn decode(path: &Path) -> Option<DynamicImage> {
    let file = File::open(path).ok()?;
    let mut reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .ok()?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(PIXELS * 4);
    reader.limits(limits);
    reader.decode().ok()
}

/// A format as the reason says it: its first extension, which is what a model
/// named it by.
fn named(format: ImageFormat) -> String {
    format
        .extensions_str()
        .first()
        .copied()
        .unwrap_or("unknown")
        .to_owned()
}

/// The error of opening a source, said as a reason. A source that is not there
/// is the one the frame can say something exact about.
fn unreadable(path: &Path, error: &std::io::Error) -> Refusal {
    if error.kind() == std::io::ErrorKind::NotFound {
        return Refusal::Missing;
    }
    Refusal::Unreadable(format!("{}: {error}", path.display()))
}
