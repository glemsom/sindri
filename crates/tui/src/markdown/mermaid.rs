//! A fence that names `mermaid`, where a picture is a block: the store measures
//! the diagram the body draws, and the block reserves those rows and captions
//! them `‹mermaid›` — the fence's own name, since there is no `src` and printing
//! the source would be noise. That name is not something a reader can use, so a
//! diagram is the one block whose copy is its source: the fence body, which is
//! what a reader pastes somewhere else. It does not open in place instead,
//! because the frame's expansion opens an entry and a diagram is a block inside
//! an answer's (`crate::expand`). A fence that will not parse is the panel it
//! was, with the parser's reason under it, because a reader may never lose a
//! diagram's source to a renderer's opinion. A fence the stream has not closed
//! is the panel too: its body is a prefix the next delta lengthens, and a
//! diagram is laid out in tens of milliseconds — ten for a handful of nodes, a
//! hundred for forty — so a render per delta is a core burned on figures thrown
//! away as they are made. The diagram appears the instant its fence closes, and
//! its source is on screen until then, which is what a reader mid-answer wants
//! to read anyway.
//!
//! The layout itself is never the frame's: a closed fence asks the store and is
//! handed either its diagram or nothing at all — a drawing the loop is making on
//! a thread of its own. Nothing is a panel with no reason under it, because "not
//! drawn yet" is silence and a refusal is a sentence; a tick or two later the
//! drawing lands, the entry is measured again, and the diagram takes the panel's
//! rows.

use crate::picture::Worth;

use super::Writer;

/// What a fence naming `mermaid` is at the place in the answer it sits.
pub(super) enum Fence {
    /// The panel it always was: some other language, a terminal that draws no
    /// picture, a place where a block cannot go, a fence still arriving, or a
    /// fence whose diagram no thread has drawn yet.
    Panel,
    /// A mermaid fence nothing will draw, and what the parser said about it.
    Refused(String),
    /// A mermaid fence drawn as a block: its rows are reserved and captioned.
    Drawn,
}

/// What the writer does with a fence: the diagram where a picture is a block,
/// the panel everywhere else — including for a fence whose closing delimiter
/// has not arrived, which `arriving` names and which is drawn as its source
/// because its body is not yet the one the diagram will be made of.
pub(super) fn fence(
    w: &mut Writer<'_>,
    language: Option<&str>,
    body: &str,
    arriving: bool,
) -> Fence {
    if language != Some("mermaid") || w.inline() || arriving {
        return Fence::Panel;
    }
    let Some(store) = w.store else {
        // A frame that draws no picture draws this fence as any other, and says
        // nothing about a diagram it never tried to draw.
        return Fence::Panel;
    };
    let (_, columns) = w.block_margin();
    let body = body.trim_end_matches('\n');
    let worth = store.fence(body, columns as u16);
    match worth {
        Worth::Drawn { .. } => {
            w.place(
                "mermaid".to_string(),
                "‹mermaid›".to_string(),
                worth,
                body.to_string(),
            );
            // A fence is a block of its own, so what follows it starts after a
            // gap — as it does under a panel.
            w.apart = true;
            Fence::Drawn
        }
        Worth::Refused(refusal) => Fence::Refused(refusal.said()),
        // The frame has asked and the answer is on its way: the panel is what a
        // reader reads in the meantime, and it says nothing of a diagram nothing
        // has refused.
        Worth::Pending => Fence::Panel,
    }
}
