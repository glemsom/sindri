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
    /// thread — so the frame that laid a panel out waited for no layout.
    pub fn take_drawings(&mut self) -> Vec<String> {
        self.pictures().map(Store::take_wanted).unwrap_or_default()
    }

    /// A drawing that has come back, kept where the frames that asked for it
    /// will find it, and every height forgotten with it. What a fence is worth in
    /// rows is the diagram's now and the panel stood in for it — and the answer
    /// above a streamed cut says how many rows it is worth, so a diagram above
    /// that cut moves every row below it. A body nothing holds any more belongs
    /// to a thread that went while it was being drawn: the answer is dropped
    /// rather than joined, and forgetting heights it did not cause costs a
    /// frame's measure and nothing else.
    pub fn drawn(&mut self, body: String, drawing: Result<Drawing, Refusal>) {
        let Some(store) = self.pictures() else {
            return;
        };
        store.keep(&body, drawing);
        self.dirty();
        // The rows below a reader's window were counted from heights that changed.
        self.window.forget();
    }
}

#[cfg(test)]
impl App {
    /// The drawings the frames so far have asked for, made here as the loop makes
    /// them — on a thread of its own there, on this one here — and handed back to
    /// the store. A test has no loop, so it is the loop, and answers how many
    /// bodies it drew.
    pub(crate) fn drained(&mut self) -> usize {
        let asked = self.take_drawings();
        for body in &asked {
            self.drawn(body.clone(), crate::picture::draw(body));
        }
        asked.len()
    }
}

#[cfg(test)]
mod tests;
