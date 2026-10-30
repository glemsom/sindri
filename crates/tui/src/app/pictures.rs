//! The picture store the frame holds: what a markdown `src` is worth, built
//! from the two facts it is made of — the terminal's one answer and the
//! workspace a relative `src` resolves against — so the layout asks a store
//! and never the disk (ADR-0016 §1, §3, §5).

use crate::picture::Store;

use super::App;

impl App {
    /// The store rebuilt from the facts as they now stand. Either of the two
    /// arrives by a builder, so whichever came last builds it; a terminal that
    /// draws no picture has no store at all (ADR-0016 §1).
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
    /// and then held, which is what makes a repaint pixels and not a reload
    /// (ADR-0016 §5, §6).
    pub(crate) fn pictures_mut(&mut self) -> Option<&mut Store> {
        self.pictures.as_mut()
    }
}
