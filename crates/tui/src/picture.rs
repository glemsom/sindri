//! The store that knows what a picture's source is worth, and holds what it
//! costs. A source is either a markdown `src` resolved to a file in the
//! workspace or a mermaid fence's body; either way the store measures it
//! without spending its pixels, refuses what cannot be drawn with a reason the
//! caption can print, and keeps a handful of rendered protocols alive so the
//! same answer redraws for nothing. It decides nothing about rows or drawing:
//! the layout asks it how many rows a source takes at a column width, and the
//! renderer asks it for the protocol that draws them.

mod diagram;
mod source;
mod store;
mod svgs;
#[cfg(test)]
mod tests;

pub use source::Source;
pub use store::Store;

/// The most rows a picture may take: the face's own allowance.
///
/// A diagram takes [`DIAGRAM_ROWS`] instead, and the difference is the shape of
/// the thing. A photograph is landscape: fitted to a column it loses height it
/// never used and reads as well as it did. A diagram is drawn top to bottom, so
/// its height *is* the dimension that carries its words and a cap on rows is a
/// cap on legibility. Measured on the ten-node `flowchart TD` of #117 — 585×984
/// pixels of its own, `crates/tui/fixtures/diagrams/ten-nodes.mmd`:
///
/// | body | twenty rows | forty rows |
/// |:--|:--|:--|
/// | 40 columns | 320×360, 5.1 px of text | 320×540, 7.7 px |
/// | 60 columns | 480×360, 5.1 px | 480×720, 10.2 px |
/// | 80 columns | 592×360, 5.1 px | 592×720, 10.2 px |
///
/// At twenty rows every column draws it at 5.1 pixels of text, 28% of a cell,
/// and its letters come out one and two pixels tall: the smear of #117. At
/// forty the ceiling binds only from sixty columns up, where the text lands at
/// 57% of a cell and its letters at nine pixels; at forty columns the width
/// binds first and the diagram takes thirty rows at 43%, its letters five
/// pixels tall. A narrow body is not something a row allowance can fix — what
/// the allowance buys is every body wide enough to hold the block.
///
/// So the allowance is a ceiling and never a target: a diagram takes the rows
/// its own aspect asks of the column, up to forty, and one that asks for more
/// was written too big and is fitted like any picture. Asking for more rows
/// than a picture only became safe once a block the window cannot hold whole
/// said so on its first row instead of sitting blank under its caption
/// (`render::pictures::explain`).
pub const MAX_ROWS: u16 = 20;

/// The most rows a diagram may take: twice a picture's twenty, because a
/// diagram is drawn top to bottom. [`MAX_ROWS`] carries the arithmetic.
pub const DIAGRAM_ROWS: u16 = 40;

/// The most bytes a source may be. A picture past this is not the picture the
/// model meant, and reading it costs a frame's budget.
const BYTES: u64 = 16 * 1024 * 1024;

/// The most pixels a decode may have, refused before the decode happens: a
/// decompression bomb is one small file, and forty megapixels is more than any
/// screenshot or photograph the frame would draw.
const PIXELS: u64 = 40_000_000;

/// A mebibyte, which is only ever used to say the cap in the caption.
const MIB: f64 = (1024 * 1024) as f64;

/// What one source is worth at a column width: the rows it takes and the
/// columns it spans, or the reason it takes none. The reason is what the caption
/// puts in its `‹›` after the source, and what a fence prints under its panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Worth {
    Drawn {
        /// What was measured: a file in the workspace, or a fence's own body.
        source: Source,
        /// The columns the block spans, inside the column it was asked about.
        columns: u16,
        /// The rows it takes; never more than the allowance it earns —
        /// [`MAX_ROWS`] for an image, [`DIAGRAM_ROWS`] for a diagram.
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

/// Why a source takes no rows. Every refusal is a sentence the caption can
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
    /// A fence's diagram is not one the renderer will draw, in the parser's own
    /// words: the source is still on screen, with this under it, because a
    /// reader must never lose a diagram to a renderer's opinion.
    Diagram(String),
    /// The terminal was never asked for graphics, or answered that it speaks
    /// none: the one refusal no source is guilty of, because nothing was
    /// measured at all.
    NoGraphics,
}

impl Refusal {
    /// The reason as the caption prints it, after the source and a colon:
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
            Refusal::Diagram(why) => why.clone(),
            Refusal::NoGraphics => "this terminal draws no picture".to_string(),
        }
    }

    /// The io error of a file that has already been found, said as a reason.
    fn from_io(error: std::io::Error) -> Refusal {
        Refusal::Unreadable(error.to_string())
    }
}
