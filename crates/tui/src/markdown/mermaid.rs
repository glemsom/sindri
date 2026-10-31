//! A fence that names `mermaid`, where a picture is a block: the store measures
//! the diagram the body draws, and the block reserves those rows and captions
//! them `‹mermaid›` — the fence's own name, since there is no `src` and printing
//! the source would be noise. A fence that will not parse is the panel it was,
//! with the parser's reason under it, because a reader may never lose a
//! diagram's source to a renderer's opinion; a fence still arriving is an
//! incomplete one, which fails to parse cheaply, so a streaming answer shows its
//! source and then the diagram the instant the fence closes.

use crate::picture::Worth;

use super::Writer;

/// What a fence naming `mermaid` is at the place in the answer it sits.
pub(super) enum Fence {
    /// The panel it always was: some other language, a terminal that draws no
    /// picture, or a place where a block cannot go.
    Panel,
    /// A mermaid fence nothing will draw, and what the parser said about it.
    Refused(String),
    /// A mermaid fence drawn as a block: its rows are reserved and captioned.
    Drawn,
}

/// What the writer does with a fence: the diagram where a picture is a block,
/// the panel everywhere else.
pub(super) fn fence(w: &mut Writer<'_>, language: Option<&str>, body: &str) -> Fence {
    if language != Some("mermaid") || w.inline() {
        return Fence::Panel;
    }
    let Some(store) = w.store else {
        // A frame that draws no picture draws this fence as any other, and says
        // nothing about a diagram it never tried to draw.
        return Fence::Panel;
    };
    let (_, columns) = w.block_margin();
    let worth = store.fence(body.trim_end_matches('\n'), columns as u16);
    match worth {
        Worth::Drawn { .. } => {
            w.place("mermaid".to_string(), "‹mermaid›".to_string(), worth);
            // A fence is a block of its own, so what follows it starts after a
            // gap — as it does under a panel.
            w.apart = true;
            Fence::Drawn
        }
        Worth::Refused(refusal) => Fence::Refused(refusal.said()),
    }
}
