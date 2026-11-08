//! A mermaid fence becomes a picture, or is refused with a reason the caption can print:
//! `mermaid-rs-renderer` draws the fence's source into an SVG in this process, the graphics
//! seam turns that SVG into pixels, and a fence that will not draw is the source it was
//! written as with one sentence under it (§12, ADR-0003).
//!
//! The renderer is pure Rust on purpose. Mermaid's own layout asks a DOM for `getBBox()` and
//! `getComputedTextLength()`, so every faithful embed of the reference implementation is a
//! browser in an overcoat, and a binary that cannot assume `node` exists cannot go that way.
//! What that buys is a rewrite, so its failure mode is a wrong picture rather than an error
//! — which is why a refusal is a first-class answer here, and why the corpus of real fences
//! beside this module freezes what the rewrite draws rather than what it promises.
//!
//! Two promises are this module's, and both are about time nobody spends. A fence whose
//! closing line has not arrived is not a diagram: [`block`] answers [`Block::Streaming`]
//! without asking the renderer at all, so a half-written diagram costs nothing and never
//! flickers under the reader (ADR-0008). And every render is given a wall clock. The
//! renderer is one synchronous call and Rust cannot interrupt one, so the work runs on its
//! own thread and the frame waits on a channel with a timeout: a pathological fence is then
//! a refusal the caption prints instead of a transcript that never paints again. The thread
//! that overran is left to finish by itself, because the one thing worse than a wasted
//! thread is a frame that waited for it.

use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::Duration;

use crate::graphics::{self, CellSize, Footprint, Svg};

/// How long one fence is given to become an SVG, wall clock.
///
/// Half a second is generous for the diagrams a fence holds — a flowchart of five hundred
/// nodes is under a hundred milliseconds — and short enough that a reader who wrote
/// something the renderer's layout cannot finish waits for a caption instead of a frame.
/// It is one number for every fence because it is a bound on a stall the reader feels and
/// not a measure of any diagram.
pub const BOUND: Duration = Duration::from_millis(500);

/// Whether the closing line of a mermaid fence has arrived: the one fact that decides
/// whether the fence is a diagram yet (ADR-0008).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fence {
    /// The closing line has arrived, so the source is whole and can be drawn.
    Closed,
    /// The fence is still being streamed, so its source is half a diagram.
    Streaming,
}

/// What a mermaid fence becomes in a transcript entry.
#[derive(Debug)]
pub enum Block {
    /// The fence drew: the picture, measured in cells before a pixel of it was spent.
    ///
    /// Boxed because a drawing is most of this enum: a transcript that refuses a fence pays
    /// for a pointer and not for the room a picture it does not have would take.
    Picture(Box<Diagram>),
    /// The fence did not draw, and the one sentence a caption prints in its place.
    Refused {
        /// Why the diagram is not there, in words a caption can print.
        caption: String,
    },
    /// The fence is still arriving, so nothing was rendered at all.
    Streaming,
}

/// The block a fence of `source` becomes, given whether it has closed.
///
/// A fence still arriving never reaches the renderer, and that is the whole of what makes a
/// half-written diagram free: the alternative — rendering what has arrived so far — spends a
/// render per fragment on a picture the reader would see flicker (ADR-0008).
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

/// A mermaid fence the renderer drew, as the drawing it answered with and the picture the
/// graphics seam made of it: what is measured in cells, what is rasterised, and the source
/// the block copies as when the picture cannot be placed.
#[derive(Debug)]
pub struct Diagram {
    /// The fence's own words, kept because a picture's block copies as its words (§12).
    source: String,
    /// The SVG the renderer answered with, which is what the corpus compares.
    svg: String,
    /// The same SVG parsed and laid out, which is what is measured and drawn.
    picture: Svg,
}

/// The renderer's own start-up, paid once before any fence is put on a clock.
///
/// [`BOUND`] is a bound on a stall the reader feels and not a measure of any diagram, but the
/// first render in a process pays for the renderer's initialisation as well as for the drawing
/// — and on a loaded machine that alone can outlast half a second, refusing a fence a warm
/// renderer draws in a few milliseconds. So the first caller draws a fence nobody asked for
/// and throws it away, untimed, and every fence after it is timed against the drawing alone.
fn warmed() {
    static WARM: OnceLock<()> = OnceLock::new();
    WARM.get_or_init(|| {
        let _ = mermaid_rs_renderer::render(WARM_UP);
    });
}

/// The fence the renderer is woken with: two nodes and an edge, which is nothing to draw and
/// everything the renderer needs to have drawn once.
const WARM_UP: &str = "flowchart TD\n  A-->B";

impl Diagram {
    /// Draws `source` within [`BOUND`].
    pub fn of(source: &str) -> Result<Diagram, Refusal> {
        warmed();
        let fence = source.to_owned();
        let drawn = within(BOUND, move || {
            // The renderer's error is kept as words: nothing downstream of here should have
            // to link its error type.
            mermaid_rs_renderer::render(&fence).map_err(|error| error.to_string())
        })?;
        let svg = drawn.map_err(|reason| Refusal::Unreadable { reason })?;
        // An SVG this front end cannot read is a refusal too, and its reason is the
        // picture's: `usvg` is the one way it reads a drawing.
        let picture = Svg::parse(&svg).map_err(|refusal| Refusal::Unreadable {
            reason: refusal.message(),
        })?;
        Ok(Diagram {
            source: source.to_owned(),
            svg,
            picture,
        })
    }

    /// The fence's own words.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The SVG the renderer answered with.
    pub fn svg(&self) -> &str {
        &self.svg
    }

    /// The drawing itself, for the cells it takes and the pixels it is made of.
    pub fn picture(&self) -> &Svg {
        &self.picture
    }

    /// How many cells of a `cell` the diagram takes. No pixel is spent to answer this, which
    /// is what lets a transcript reserve the rows before it decides to draw (§12, ADR-0003).
    pub fn footprint(&self, cell: CellSize) -> Result<Footprint, graphics::Refusal> {
        self.picture.footprint(cell)
    }

    /// The diagram as the pixels a terminal is asked to place, at the size the footprint
    /// measured it into.
    pub fn rasterise(&self, cell: CellSize) -> Result<image::RgbaImage, graphics::Refusal> {
        self.picture.rasterise(cell)
    }
}

/// Why a fence is not a picture: the reason a caption prints under its source (§12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The renderer would not read the fence, in its own words — which is what tells a
    /// writer which line of the diagram was wrong.
    Unreadable {
        /// What the renderer said, kept whole because it is what the caption prints.
        reason: String,
    },
    /// The renderer had not answered when its bound passed.
    TooSlow {
        /// The wall clock it was given.
        bound: Duration,
    },
    /// The renderer stopped without an answer: it panicked, or left in some way that sent
    /// nothing back. A refusal and not a crash, because the frame outlives one fence.
    Broken,
}

impl Refusal {
    /// The reason as the one sentence a caption prints, because a fence refused with no
    /// reason is a diagram that silently disappeared.
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

/// `work` on its own thread, waited on for `bound` of wall clock and no longer: the guard
/// every render passes through, so a fence that cannot be drawn cannot hang the frame.
///
/// A thread is the only way to bound a call this crate cannot interrupt, and the thread is
/// deliberately left detached when the bound passes: it finishes, its answer goes to a
/// channel nobody is listening to, and the transcript has already moved on.
pub fn within<T: Send + 'static>(
    bound: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Refusal> {
    let (sender, answer) = mpsc::channel();
    std::thread::spawn(move || {
        // A receiver that has given up is not the work's problem: the frame stopped
        // listening for the same reason the work is late.
        let _ = sender.send(work());
    });
    match answer.recv_timeout(bound) {
        Ok(value) => Ok(value),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Refusal::TooSlow { bound }),
        // The renderer panicked: the thread that held the sender unwound without sending,
        // and the only thing the channel can say about that is that it is gone.
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Refusal::Broken),
    }
}
