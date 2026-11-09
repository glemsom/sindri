//! A mermaid fence becomes a picture, or is refused with a reason the caption prints:
//! `mermaid-rs-renderer` draws the fence's source into an SVG in this process, and a fence
//! that will not draw is the source it was written as with one sentence under it.

use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::Duration;

use crate::graphics::{self, CellSize, Footprint, Svg};

/// How long one fence is given to become an SVG, wall clock: generous for the diagrams a fence
/// holds, and short enough that a pathological one costs a caption instead of a frame. It
/// bounds a stall the reader feels, not the size of any diagram.
pub const BOUND: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fence {
    Closed,
    Streaming,
}

#[derive(Debug)]
pub enum Block {
    /// Boxed because a drawing is most of this enum, so a fence that refuses pays for a pointer.
    Picture(Box<Diagram>),
    Refused {
        caption: String,
    },
    Streaming,
}

/// The block a fence of `source` becomes. A fence still arriving never reaches the renderer,
/// which is what makes a half-written diagram cost nothing and never flicker.
pub fn block(source: &str, fence: Fence) -> Block {
    match fence {
        Fence::Streaming => Block::Streaming,
        Fence::Closed => match Diagram::of(source) {
            Ok(diagram) => Block::Picture(Box::new(diagram)),
            Err(refusal) => Block::Refused {
                caption: refusal.message(),
            },
        },
    }
}

#[derive(Debug)]
pub struct Diagram {
    source: String,
    svg: String,
    picture: Svg,
}

/// The renderer's own start-up, drawn once before any fence is put on a clock: on a loaded
/// machine initialisation alone outlasts [`BOUND`], which would refuse a fence a warm renderer
/// draws in milliseconds.
fn warmed() {
    static WARM: OnceLock<()> = OnceLock::new();
    WARM.get_or_init(|| {
        let _ = mermaid_rs_renderer::render(WARM_UP);
    });
}

/// Two nodes and an edge: nothing to draw, everything the renderer needs to have drawn once.
const WARM_UP: &str = "flowchart TD\n  A-->B";

impl Diagram {
    /// Draws `source` within [`BOUND`] wall clock.
    pub fn of(source: &str) -> Result<Diagram, Refusal> {
        Diagram::of_within(BOUND, source)
    }

    /// Draws `source` within `bound`: the wall clock the caller can afford to wait.
    pub fn of_within(bound: Duration, source: &str) -> Result<Diagram, Refusal> {
        warmed();
        let fence = source.to_owned();
        let drawn = within(bound, move || {
            // The renderer's error is kept as words: nothing downstream links its error type.
            mermaid_rs_renderer::render(&fence).map_err(|error| error.to_string())
        })?;
        let svg = drawn.map_err(|reason| Refusal::Unreadable { reason })?;
        let picture = Svg::parse(&svg).map_err(|refusal| Refusal::Unreadable {
            reason: refusal.message(),
        })?;
        Ok(Diagram {
            source: source.to_owned(),
            svg,
            picture,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn svg(&self) -> &str {
        &self.svg
    }

    pub fn picture(&self) -> &Svg {
        &self.picture
    }

    /// How many cells of a `cell` the diagram takes, without spending a pixel.
    pub fn footprint(&self, cell: CellSize) -> Result<Footprint, graphics::Refusal> {
        self.picture.footprint(cell)
    }

    /// The diagram as the pixels a terminal is asked to place, at the footprint's own size.
    pub fn rasterise(&self, cell: CellSize) -> Result<image::RgbaImage, graphics::Refusal> {
        self.picture.rasterise(cell)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Unreadable {
        reason: String,
    },
    TooSlow {
        bound: Duration,
    },
    /// The renderer stopped without an answer: it panicked, or left in a way that sent nothing.
    Broken,
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::Unreadable { reason } => format!("the diagram could not be drawn: {reason}"),
            Refusal::TooSlow { bound } => format!(
                "the diagram took longer than {} ms to draw, so its source stands in its place",
                bound.as_millis()
            ),
            Refusal::Broken => {
                String::from("the diagram's renderer stopped without drawing anything")
            }
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message())
    }
}

/// `work` on its own thread, waited on for `bound` and no longer. A thread is the only way to
/// bound a call this crate cannot interrupt, and it is left detached when the bound passes: it
/// finishes, its answer goes nowhere, and the transcript has already moved on.
pub fn within<T: Send + 'static>(
    bound: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Refusal> {
    let (sender, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    match answer.recv_timeout(bound) {
        Ok(value) => Ok(value),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Refusal::TooSlow { bound }),
        // The sender unwound without sending, which is all the channel can report of a panic.
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Refusal::Broken),
    }
}
