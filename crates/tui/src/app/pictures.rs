//! The picture store the frame holds: what a markdown `src` is worth, built
//! from the two facts it is made of — the terminal's one answer and the
//! workspace a relative `src` resolves against — so the layout asks a store
//! and never the disk. It is also where a diagram's drawing is asked for and
//! where the answer is taken in: the loop carries the ask to a thread of its own
//! and hands the drawing back here, which is what keeps a layout off the frame.

use crate::picture::{Drawing, Refusal, Store};

use super::App;

impl App {
    /// The store rebuilt from the facts as they now stand. Either of the two
    /// arrives by a builder, so whichever came last builds it; a terminal that
    /// draws no picture has no store at all.
    pub(super) fn store(mut self) -> Self {
        self.pictures = Store::new(self.graphics.as_ref(), self.workspace_dir());
        self
    }

    /// What a picture's source is worth: the store that measures it, or nothing
    /// where the terminal draws no picture.
    pub(crate) fn pictures(&self) -> Option<&Store> {
        self.pictures.as_ref()
    }

    /// The store a frame draws from: mutable because a protocol is decoded once
    /// and then held, which is what makes a repaint pixels and not a reload.
    pub(crate) fn pictures_mut(&mut self) -> Option<&mut Store> {
        self.pictures.as_mut()
    }

    /// The bodies the frames so far have asked to have drawn and no thread has
    /// been given: what the loop takes and makes, each once, off the frame's own
    /// thread. A drawing is a mermaid layout and not a line of output, so the
    /// frame that laid its panel out waited for nothing.
    pub fn take_drawings(&mut self) -> Vec<String> {
        self.pictures().map(Store::take_wanted).unwrap_or_default()
    }

    /// A drawing that has come back, kept where the frames that asked for it
    /// will find it — and the entries that say its fence are measured again,
    /// because the rows a panel stood in for are the diagram's now. A body no
    /// entry says belongs to a thread that went while it was being drawn: it is
    /// kept, and it dirties nothing, because the answer is dropped rather than
    /// joined.
    pub fn drawn(&mut self, body: String, drawing: Result<Drawing, Refusal>) {
        let Some(store) = self.pictures() else {
            return;
        };
        let let_go = store.keep(&body, drawing);
        // A drawing let go to make room is a block that is its panel again until
        // it is drawn afresh, so the entry that says it is measured again too.
        let changed = std::iter::once(&body).chain(let_go.as_ref());
        let found = self
            .entries()
            .iter()
            .position(|entry| changed.clone().any(|body| entry.holds(body)));
        match found {
            Some(index) => self.dirty_from(index),
            None => self.dirty(),
        }
        // The rows below a reader's window were counted from heights the drawing
        // changed, unless the block is above the place they are reading.
        if found.is_none_or(|index| index >= self.window.at.0) {
            self.window.forget();
        }
    }
}

#[cfg(test)]
mod tests;
