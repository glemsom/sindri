//! The mark the reader's drag leaves, as the frame holds it: the rows a copy
//! takes, the cells they are lit on, and the place in the thread those rows
//! were last read against — a mark stands on the text it was drawn on, and not
//! on the cells that happened to hold it.

use super::App;
use crate::selection::{Selection, Viewport};

impl App {
    /// The frame's viewport, and the mark read against it: the two are one
    /// question, since a mark is drawn on the rows the last frame laid out —
    /// and its rows are a place in the thread, not on the glass, so a note that
    /// lands under them moves the mark with the text.
    pub(crate) fn set_viewport(&mut self, viewport: Viewport) {
        self.rebased(&viewport);
        self.viewport = viewport;
    }

    fn rebased(&mut self, viewport: &Viewport) {
        let at = self.window.at;
        let Some(from) = self.mark_at.replace(at) else {
            return;
        };
        // A drag in flight is the pointer's and follows it, not the text.
        if self.selection.is_empty() || self.selection.dragging {
            return;
        }
        // A thread shorter than the column is written on its floor, so rows
        // arriving grow it upward and move the words in the same place in the
        // frame: the place alone does not say, and the top of the viewport does.
        let moved = if from == at {
            0
        } else {
            crate::render::layout::rows_between(self, viewport.area.width, from, at)
        };
        let top = i32::from(viewport.area.y) - i32::from(self.viewport.area.y);
        let shift = i32::try_from(moved).map_or(i32::MIN, |moved| top - moved);
        let (anchor, head) = (self.selection.anchor.y, self.selection.head.y);
        let (above, below) = (
            i32::from(anchor.min(head)) + shift,
            i32::from(anchor.max(head)) + shift,
        );
        let first = i32::from(viewport.area.y);
        let past = first + viewport.rows.len() as i32;
        // The text the mark stood on has left the window, so the rows under it
        // are another entry's: a mark standing there would copy what the reader
        // never took, and say nothing of the swap. It goes.
        if above < first || below >= past {
            self.clear_mark();
            return;
        }
        self.selection.translate(shift as isize);
    }

    /// Takes the mark away. Every gesture but the copy itself does this: a mark
    /// is the reader's until they do the next thing, and no longer.
    pub(crate) fn clear_mark(&mut self) {
        self.selection = Selection::default();
        self.selecting = 0;
    }
}
