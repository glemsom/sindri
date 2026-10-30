//! The store that knows what a markdown `src` is worth, and holds what it
//! costs: it locates a picture in the workspace, measures it from
//! its header alone, refuses what cannot be drawn with a reason the caption can
//! print, and keeps a handful of decoded protocols alive so the same answer
//! redraws for nothing.
//!
//! It decides nothing about rows or drawing. The layout asks it how many rows a
//! `src` takes at a column width; the renderer asks it for the protocol that
//! draws them.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

use crate::graphics::Graphics;

/// The most rows a picture may take: the face's own allowance.
pub const MAX_ROWS: u16 = 20;

/// The most bytes a source may be. A picture past this is not the picture the
/// model meant, and reading it costs a frame's budget.
const BYTES: u64 = 16 * 1024 * 1024;

/// The most pixels a decode may have, refused before the decode happens: a
/// decompression bomb is one small file, and forty megapixels is more than any
/// screenshot or photograph the frame would draw.
const PIXELS: u64 = 40_000_000;

/// A mebibyte, which is only ever used to say the cap in the caption.
const MIB: f64 = (1024 * 1024) as f64;

/// How many decoded protocols stay live: each holds its pixels, so the store
/// keeps what a screenful of pictures needs and lets the oldest go.
const LIVE: usize = 8;

/// The formats the store draws: the four the decoders behind `ratatui-image`
/// link, and nothing else.
const DRAWS: [ImageFormat; 4] = [
    ImageFormat::Png,
    ImageFormat::Jpeg,
    ImageFormat::Gif,
    ImageFormat::WebP,
];

pub struct Store {
    /// Where a relative `src` resolves: the workspace the frame was launched in.
    workspace: PathBuf,
    /// The answer a protocol is built from. It speaks Kitty, or there is no
    /// store: no sixel, no half-blocks, no fallback.
    picker: Picker,
    /// What has been decoded, oldest first: the last is the most recent, and
    /// the first is what makes room for a ninth.
    live: Vec<Live>,
    /// How many protocols have been built. A decode is the cost this store
    /// exists to avoid, so it is counted where a test can see it was avoided.
    decodes: u64,
}

/// One protocol the store holds: the answer it was decoded for, and the pixels
/// that answer bought.
struct Live {
    key: Key,
    protocol: StatefulProtocol,
}

/// The whole of what a protocol is made from: which file, which version of it,
/// and the block it is drawn into. A redraw asks again with the same six facts
/// and is handed the same pixels; a file rewritten under a new answer is not.
#[derive(Debug, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    length: u64,
    mtime: SystemTime,
    columns: u16,
    rows: u16,
}

impl Key {
    /// The key of a file as it is now: the lookup's one stat, and the only one.
    fn of(path: &Path, columns: u16, rows: u16) -> Option<Key> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Key {
            path: path.to_path_buf(),
            length: meta.len(),
            mtime: meta.modified().ok()?,
            columns,
            rows,
        })
    }
}

impl Store {
    /// The store, or nothing where the terminal cannot draw: a frame without a
    /// Kitty-capable answer has no pictures to hold.
    pub fn new(graphics: Option<&Graphics>, workspace: impl Into<PathBuf>) -> Option<Store> {
        let picker = graphics?.picker().clone();
        if picker.protocol_type() != ProtocolType::Kitty {
            return None;
        }
        Some(Store {
            workspace: workspace.into(),
            picker,
            live: Vec::new(),
            decodes: 0,
        })
    }

    /// What `src` is worth at `columns`: the rows it takes, read from its
    /// header and never from its pixels.
    pub fn measure(&self, src: &str, columns: u16) -> Worth {
        match self.weigh(src, columns) {
            Ok(worth) => worth,
            Err(refusal) => Worth::Refused(refusal),
        }
    }

    fn weigh(&self, src: &str, columns: u16) -> Result<Worth, Refusal> {
        let path = self.locate(src)?;
        self.guard(&path)?;
        let pixels = dimensions(&path)?;
        if u64::from(pixels.0) * u64::from(pixels.1) > PIXELS {
            let (width, height) = pixels;
            return Err(Refusal::TooManyPixels { width, height });
        }
        let (span, rows) = self.block(pixels, columns);
        Ok(Worth::Drawn {
            path,
            columns: span,
            rows,
        })
    }

    /// The protocol that draws a measured picture. A lookup stats the file and
    /// nothing else, so a redraw of the same answer costs nothing and an agent
    /// that rewrote `plot.png` under a new answer pays once.
    /// `None` where nothing was measured, or where the file has gone since.
    pub fn protocol(&mut self, worth: &Worth) -> Option<&mut StatefulProtocol> {
        let Worth::Drawn {
            path,
            columns,
            rows,
        } = worth
        else {
            return None;
        };
        let key = Key::of(path, *columns, *rows)?;
        let at = match self.live.iter().position(|live| live.key == key) {
            Some(at) => at,
            None => {
                let protocol = self.picker.new_resize_protocol(decode(path)?);
                self.decodes += 1;
                if self.live.len() >= LIVE {
                    self.live.remove(0);
                }
                self.live.push(Live { key, protocol });
                self.live.len() - 1
            }
        };
        Some(&mut self.live[at].protocol)
    }

    /// How many sources have been decoded: the cost this store exists to avoid,
    /// read where a repaint has to prove it spent none.
    #[cfg(test)]
    pub(crate) fn decodes(&self) -> u64 {
        self.decodes
    }

    /// Where a `src` points: an absolute path as it is, anything else against
    /// the workspace. `~` is a name like any other here — the frame expands
    /// nothing, and a shell is not in the loop.
    fn locate(&self, src: &str) -> Result<PathBuf, Refusal> {
        if let Some(scheme) = scheme(src) {
            return Err(Refusal::Scheme(scheme.to_owned()));
        }
        let path = Path::new(src);
        Ok(if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace.join(path)
        })
    }

    /// What the source is, before any decoder sees it: a file, of a size the
    /// frame can afford to read. A directory, a missing file and
    /// a file past the byte cap are all answered here, without opening one.
    fn guard(&self, path: &Path) -> Result<(), Refusal> {
        let meta = std::fs::metadata(path).map_err(|error| unreadable(path, &error))?;
        if meta.is_dir() {
            return Err(Refusal::Directory);
        }
        if meta.len() > BYTES {
            return Err(Refusal::TooBig(meta.len()));
        }
        Ok(())
    }

    /// The block a picture of `pixels` takes in `columns`: it fits the column,
    /// never upscales past it, keeps its aspect from the cell size, and is
    /// capped at [`MAX_ROWS`].
    fn block(&self, pixels: (u32, u32), columns: u16) -> (u16, u16) {
        let (cell_w, cell_h) = self.picker.font_size();
        let (cell_w, cell_h) = (u64::from(cell_w.max(1)), u64::from(cell_h.max(1)));
        let column = u64::from(columns.max(1));
        let (picture_w, picture_h) = (u64::from(pixels.0.max(1)), u64::from(pixels.1.max(1)));
        // The width it is drawn at: never past the column, never past itself.
        let pixels_wide = picture_w.min(column * cell_w);
        let pixels_tall = (picture_h * pixels_wide).div_ceil(picture_w);
        let span = pixels_wide.div_ceil(cell_w).clamp(1, column);
        let rows = pixels_tall.div_ceil(cell_h).clamp(1, u64::from(MAX_ROWS));
        (span as u16, rows as u16)
    }
}

/// What one `src` is worth at a column width: the rows it takes and the columns
/// it spans, or the reason it takes none. The reason is what the caption puts
/// in its `‹›`, after the `src`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Worth {
    Drawn {
        /// The file the `src` resolved to.
        path: PathBuf,
        /// The columns the block spans, inside the column it was asked about.
        columns: u16,
        /// The rows it takes; never more than [`MAX_ROWS`].
        rows: u16,
    },
    Refused(Refusal),
}

impl Worth {
    /// The rows the block takes: none when nothing is drawn, so a caller that
    /// lays out either costs the same.
    pub fn rows(&self) -> u16 {
        match self {
            Worth::Drawn { rows, .. } => *rows,
            Worth::Refused(_) => 0,
        }
    }

    /// The columns the block spans: none when nothing is drawn, so the cells a
    /// graphic is given are the ones it was measured for.
    pub fn columns(&self) -> u16 {
        match self {
            Worth::Drawn { columns, .. } => *columns,
            Worth::Refused(_) => 0,
        }
    }
}

/// Why a `src` takes no rows. Every refusal is a sentence the caption can
/// print, never a panic and never a placeholder box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The source names a scheme: v1 draws no remote picture.
    Scheme(String),
    Missing,
    Directory,
    TooBig(u64),
    TooManyPixels {
        width: u32,
        height: u32,
    },
    NotAnImage,
    Unsupported(String),
    Damaged,
    Unreadable(String),
    /// The terminal was never asked for graphics, or answered that it speaks
    /// none: the one refusal no source is guilty of, because nothing was
    /// measured at all.
    NoGraphics,
}

impl Refusal {
    /// The reason as the caption prints it, after the `src` and a colon:
    /// `a chart ‹plot.png: no such file›`.
    pub fn said(&self) -> String {
        match self {
            Refusal::Scheme(scheme) => format!("{scheme}: is not fetched"),
            Refusal::Missing => "no such file".to_string(),
            Refusal::Directory => "a directory".to_string(),
            Refusal::TooBig(bytes) => {
                let (size, cap) = (*bytes as f64 / MIB, BYTES as f64 / MIB);
                format!("{size:.1} MiB, over the {cap:.0} MiB cap")
            }
            Refusal::TooManyPixels { width, height } => {
                format!(
                    "{width}×{height}, over the {} megapixel cap",
                    PIXELS / 1_000_000
                )
            }
            Refusal::NotAnImage => "not an image".to_string(),
            Refusal::Unsupported(format) => format!("{format}: a format not drawn"),
            Refusal::Damaged => "its header is not whole".to_string(),
            Refusal::Unreadable(why) => why.clone(),
            Refusal::NoGraphics => "this terminal draws no picture".to_string(),
        }
    }

    /// The io error of a file that has already been found, said as a reason.
    fn from_io(error: std::io::Error) -> Refusal {
        Refusal::Unreadable(error.to_string())
    }
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

/// The pixels a measured picture holds, decoded under the cap its header was
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

#[cfg(test)]
mod tests;
