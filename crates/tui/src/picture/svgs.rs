//! The svgs a fence's body has already been drawn into: a mermaid render is
//! five milliseconds, a frame asks for it once per entry per frame, and the
//! answer never changes for a body that did not. So the svg is kept and handed
//! back for the measure and for the raster both — one render per distinct body
//! is the whole of it, because a diagram's column mapping is the block's job
//! and comes after the drawing.
//!
//! It is the store's own mutation, behind a `RefCell` and not a `&mut`: a
//! measure is asked of an immutable borrow of the app on the frame's path, and
//! a `&mut` here would ripple through `markdown::lines` and the layout for
//! nothing. The frame is single-threaded; if it stops being, that is a `Mutex`
//! and a later ticket.
//!
//! The cache is bounded, and it lets go the *least recently used* entry rather
//! than the oldest: the diagram a reader scrolls gently off the top of the
//! window is the one they are most likely to scroll back to, and charging them
//! a re-render for the round trip is the trade this cache exists to refuse.
//!
//! A body that will not render is not held: a fence still arriving is a new body
//! every frame, and a stream of partial ones would evict the diagrams the reader
//! is looking at to remember refusals that cost nothing to repeat.

use std::cell::{Cell, Ref, RefCell};
use std::collections::VecDeque;

use image::DynamicImage;

use super::Refusal;
use super::diagram::{self, Shape};
use super::store::LIVE;

/// The most diagrams whose svg is held: a screenful, the same screenful the
/// raster beside it keeps, because that is the working set — what a reader is
/// looking at, and what a gentle scroll reaches next. The bound is not what
/// makes the round trip cheap on its own: the policy is, and it is least
/// recently used and never oldest first, so the diagram that leaves the top of
/// the window is the last let go and not the first.
pub(super) const SVGS: usize = LIVE;

/// The svgs a fence body has been drawn into, least recently used first.
#[derive(Default)]
pub(super) struct Svgs {
    /// What has been rendered, least recently used first: the back is the most
    /// recent, and the front is what makes room for one past the bound.
    held: RefCell<VecDeque<Held>>,
    /// How many bodies have been put through the renderer. The cost this cache
    /// exists to avoid, counted where a repaint has to prove it spent nothing.
    renders: Cell<u64>,
}

/// One body and the svg it drew: the size is read off the svg at the render, so
/// a measure never parses a drawing it already has.
struct Held {
    body: String,
    shape: Shape,
}

impl Svgs {
    /// The size in pixels of the diagram a body draws: the render when the body
    /// is not held, and what the held svg says when it is.
    pub(super) fn measured(&self, body: &str) -> Result<(u32, u32), Refusal> {
        Ok(self.shape(body)?.size)
    }

    /// The pixels a body's diagram draws at `width` by `height`, from the svg
    /// the store already rendered for it where it has one.
    pub(super) fn rasterised(&self, body: &str, width: u32, height: u32) -> Option<DynamicImage> {
        let shape = self.shape(body).ok()?;
        diagram::raster(&shape, width, height)
    }

    /// How many bodies have been put through the renderer, for the store to
    /// report and a test to read.
    #[cfg(test)]
    pub(super) fn renders(&self) -> u64 {
        self.renders.get()
    }

    /// The shape a body draws, rendering it where it is not held and making it
    /// the most recently used of the lot either way — which is what holds on to
    /// the diagram a reader scrolled back to and lets go of one scrolled past.
    /// The borrow is the cache's own and nothing under it asks the cache again,
    /// so no svg is copied out of it.
    fn shape(&self, body: &str) -> Result<Ref<'_, Shape>, Refusal> {
        let at = {
            let mut held = self.held.borrow_mut();
            let found = held
                .iter()
                .position(|one| one.body == body)
                .and_then(|at| held.remove(at));
            let one = match found {
                Some(one) => one,
                None => {
                    let shape = diagram::render(body)?;
                    self.renders.set(self.renders.get() + 1);
                    if held.len() >= SVGS {
                        held.pop_front();
                    }
                    Held {
                        body: body.to_owned(),
                        shape,
                    }
                }
            };
            held.push_back(one);
            held.len() - 1
        };
        Ok(Ref::map(self.held.borrow(), |held| &held[at].shape))
    }
}
