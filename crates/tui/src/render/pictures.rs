//! The pictures a frame draws: where a block's rows are, whether the block is
//! whole in the window, and the painting that costs pixels rather than a
//! reload. A picture is not a cell and not a row of its own: it is drawn over
//! the rows the markdown reserved for it, after the paragraph that carries
//! them, from the protocol the store already holds. Nothing here decides a row
//! count or a rect — the store measured those, and the frame laid them out.
//!
//! A block no window can hold whole — a diagram tall enough that its rows are
//! more than the column has — is the one thing a blank block must never be: it
//! says so on its first visible row, in the ink a caption keeps, and names the
//! rows it asked for against the window's. A block merely scrolled past is
//! drawn nowhere and says nothing, because the reader who scrolled it is the
//! one who moved it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{FilterType, Resize, StatefulImage};

use crate::app::App;
use crate::picture::Worth;
use crate::selection::Placed;
use crate::theme;

/// How a picture is fitted to the cells its rows are: the whole of it, resampled
/// rather than gritted, exactly as the face is.
const RESIZE: Resize = Resize::Fit(Some(FilterType::Lanczos3));

/// A picture among the rows a frame is laying out: the row its block starts at
/// among them, the column of the entry's body its graphic hangs off, the rows
/// the whole block covers — the graphic's and its caption's — and what the
/// store measured. The frame re-bases `row` as it lays entries out, and the
/// drawn frame turns it into the cells the graphic is given.
pub(crate) struct Laid {
    pub(crate) row: usize,
    pub(crate) column: u16,
    /// The rows of the block the frame's top skipped over: a block with any of
    /// them is not whole in the window, whatever is left of it here.
    pub(crate) cut: usize,
    pub(crate) block: usize,
    /// What a copy of the block takes: a caption, or a diagram's own source.
    pub(crate) copy: String,
    /// The source as the model wrote it: what the reason a block gives for
    /// standing empty names, where the caption may have scrolled away.
    pub(crate) src: String,
    pub(crate) worth: Worth,
}

/// A picture the window shows, and whether its graphic is whole in it: the whole
/// one is drawn, and the block's rows and words are the window's either way.
pub(crate) struct Shown {
    pub(crate) laid: Laid,
    pub(crate) whole: bool,
    /// What the block says in place of a graphic no scroll can make whole: the
    /// rows it asks for against the window's.
    pub(crate) reason: Option<String>,
}

impl Laid {
    /// The row the block ends past: the graphic's rows and its caption's, which
    /// is where the words after it resume.
    pub(crate) fn end(&self) -> usize {
        self.row + self.block
    }
}

/// The pictures the window shows, re-based to its rows, and whether each one's
/// graphic is whole in it: a block partly scrolled out is drawn nowhere — its
/// caption and blank rows say where it is — and it appears whole once it is
/// whole in view. Cropping is out of scope, so a row of it is
/// never drawn at an edge.
///
/// It is the graphic that is whole or absent, not the block: a caption is words
/// like any other and scrolls with them, so a picture does not vanish because
/// the row under it is off the window. The block stays in the list either way,
/// because its rows are blank ones the window holds and a copy must not take.
pub(crate) fn on_screen(pictures: Vec<Laid>, start: usize, visible: usize) -> Vec<Shown> {
    pictures
        .into_iter()
        .filter_map(|laid| {
            let rows = usize::from(laid.worth.rows());
            let whole = laid.cut == 0 && laid.row >= start && laid.row + rows <= start + visible;
            // A graphic with more rows than the window has is drawn nowhere at
            // any scroll — the frame never crops — so the block says why it is
            // blank rather than leaving the reader a caption over nothing. It is
            // the graphic's rows against the window's, whatever the source: a
            // photograph of twenty rows on a body of fifteen has the same
            // problem and deserves the same sentence.
            let reason = (rows > visible)
                .then(|| format!("‹{}: {rows} rows, over the window's {visible}›", laid.src));
            let at = laid.row.saturating_sub(start);
            let end = laid.end().saturating_sub(start);
            (at < visible && end > 0).then_some(Shown {
                laid: Laid { row: at, ..laid },
                whole,
                reason,
            })
        })
        .collect()
}

/// Where each picture on screen goes: the cells its rows were reserved as, in
/// the frame's own coordinates, which is what a copy and a repaint are both
/// read from.
pub(crate) fn placed(pictures: &[Shown], at: Rect) -> Vec<Placed> {
    pictures
        .iter()
        .map(|shown| {
            let laid = &shown.laid;
            Placed {
                at: laid.row,
                rows: laid.block,
                copy: laid.copy.clone(),
                area: Rect {
                    x: at.x + laid.column,
                    y: at.y + laid.row as u16,
                    width: laid.worth.columns(),
                    height: laid.worth.rows(),
                },
                worth: laid.worth.clone(),
                drawn: shown.whole,
                reason: shown.reason.clone(),
            }
        })
        .collect()
}

/// Draws one picture over the cells its rows are, from the protocol the store
/// holds: a repaint of a live run costs pixels and never a decode. A source the
/// store has nothing to draw for — refused, or gone since it was measured —
/// draws nothing, and its caption says what it is.
pub(crate) fn paint(app: &mut App, picture: &Placed, buf: &mut Buffer) {
    let Some(store) = app.pictures_mut() else {
        return;
    };
    let Some(protocol) = store.protocol(&picture.worth) else {
        return;
    };
    StatefulImage::<StatefulProtocol>::default()
        .resize(RESIZE)
        .render(picture.area, buf, protocol);
}

/// Says why a block is drawn nowhere, on the first row of it the window holds:
/// the reason `on_screen` kept, in the ink a caption keeps, so a block taller
/// than the window is a sentence rather than blank cells under a caption.
pub(crate) fn explain(picture: &Placed, buf: &mut Buffer) {
    let Some(reason) = &picture.reason else {
        return;
    };
    // The block's first row the window holds: the graphic's own where it starts
    // inside the window, and the window's top where the block began above it.
    let row = picture.area.y.max(buf.area.y);
    let left = picture.area.x.max(buf.area.x);
    let room = picture
        .area
        .right()
        .min(buf.area.right())
        .saturating_sub(left);
    if row >= buf.area.bottom() || room == 0 {
        return;
    }
    buf.set_stringn(left, row, reason, usize::from(room), theme::faint());
}

#[cfg(test)]
mod tests;
