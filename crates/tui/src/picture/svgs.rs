//! The drawings a fence's body has been made into, and the ones a frame has
//! asked for: a mermaid layout is tens of milliseconds, so a measure never
//! makes one. It looks the body up, is handed the svg the store holds, the
//! parser's reason there is none, or nothing at all — a body no thread has been
//! given yet — and a frame that was handed nothing lays the panel out and asks
//! for the drawing, which is made off its own thread and handed back here.
//!
//! The ask is the store's own mutation, behind a `RefCell` and not a `&mut`: a
//! measure is asked of an immutable borrow of the app on the frame's path, and a
//! `&mut` here would ripple through `markdown::lines` and the layout for
//! nothing. The store is still the frame's alone — the drawing is made on a
//! thread and the answer comes back through the loop's channel, so nothing here
//! is ever touched by two threads at once.
//!
//! The cache is bounded, and it lets go the *least recently used* entry rather
//! than the oldest: the diagram a reader scrolls gently off the top of the
//! window is the one they are most likely to scroll back to, and charging them
//! a re-render for the round trip is the trade this cache exists to refuse. A
//! body let go is asked for again, and is its panel until the answer lands.
//!
//! A body that will not render is held too, as the reason: a refusal costs the
//! renderer the same walk as a drawing, and a fence that will not parse is one
//! the frame would otherwise ask about every time it lays it out.

use std::cell::{Cell, Ref, RefCell};
use std::collections::VecDeque;

use image::DynamicImage;

use super::Refusal;
use super::diagram::{self, Drawing};
use super::source::Sized;
use super::store::LIVE;

/// The most diagrams held: a screenful, the same screenful the raster beside it
/// keeps, because that is the working set — what a reader is looking at, and what
/// a gentle scroll reaches next. The bound is not what makes the round trip cheap
/// on its own: the policy is, and it is least recently used and never oldest
/// first, so the diagram that leaves the top of the window is the last let go and
/// not the first.
pub(super) const SVGS: usize = LIVE;

/// The drawings a fence body has been made into, least recently used first, and
/// the bodies a frame has asked for that no thread has been given yet.
#[derive(Default)]
pub(super) struct Svgs {
    /// What has been drawn or refused, least recently used first: the back is
    /// the most recent, and the front is what makes room for one past the bound.
    held: RefCell<VecDeque<Held>>,
    /// The bodies the frame has asked about and no thread has been given: the
    /// loop takes them, and what it hands back is `keep`'s.
    asked: RefCell<Vec<String>>,
    /// The bodies a thread is drawing: asked for once and never twice while the
    /// answer is out, so a frame a tick long asks one question and not eight.
    flight: RefCell<Vec<String>>,
    /// How many bodies have been handed to the renderer. The layout this cache
    /// exists to keep off the frame's path, counted where a repaint has to prove
    /// it made none.
    renders: Cell<u64>,
}

/// One body and what the renderer made of it: the svg and the size read off it,
/// or the parser's reason there is none.
struct Held {
    body: String,
    drawn: Result<Drawing, Refusal>,
}

impl Svgs {
    /// What a body is worth to a frame that cannot wait for a layout: the pixels
    /// its drawing is, the parser's reason it draws none, or the ask — a body no
    /// thread has been given, which is a frame's to lay the panel out for and
    /// never a refusal to print.
    pub(super) fn sized(&self, body: &str) -> Sized {
        match self.find(body) {
            Some(at) => match &self.held(at).drawn {
                Ok(drawn) => Sized::Of(drawn.size),
                Err(why) => Sized::Refused(why.clone()),
            },
            None => {
                self.ask(body);
                Sized::Pending
            }
        }
    }

    /// The pixels a body's diagram draws at `width` by `height`, from the svg the
    /// store already holds: `None` where nothing was drawn for it, which a
    /// measured block is the one caller that cannot be.
    pub(super) fn rasterised(&self, body: &str, width: u32, height: u32) -> Option<DynamicImage> {
        let at = self.find(body)?;
        let held = self.held(at);
        let drawn = held.drawn.as_ref().ok()?;
        diagram::raster(drawn, width, height)
    }

    /// The bodies a frame has asked for and no thread has been given: each is
    /// drawn once, off the frame's own thread, and its answer is `keep`'s.
    /// Handing them over is where a layout is counted, and not the ask: the frame
    /// that asked made none, and a repaint before the answer lands makes none
    /// either.
    pub(super) fn take_wanted(&self) -> Vec<String> {
        let taken = std::mem::take(&mut *self.asked.borrow_mut());
        self.flight.borrow_mut().extend(taken.iter().cloned());
        self.renders.set(self.renders.get() + taken.len() as u64);
        taken
    }

    /// A drawing of `body`, as the thread that made it hands it back: held as the
    /// most recent of the lot, and no longer in flight. Answers the body let go to
    /// make room, where one was — a block that is its panel again until it is
    /// drawn afresh.
    pub(super) fn keep(&self, body: &str, drawn: Result<Drawing, Refusal>) -> Option<String> {
        self.flight.borrow_mut().retain(|one| one != body);
        let mut held = self.held.borrow_mut();
        let let_go = if held.len() >= SVGS {
            held.pop_front().map(|one| one.body)
        } else {
            None
        };
        held.push_back(Held {
            body: body.to_owned(),
            drawn,
        });
        let_go
    }

    /// How many bodies have been put through the renderer, for the store to
    /// report and a test to read.
    #[cfg(test)]
    pub(super) fn renders(&self) -> u64 {
        self.renders.get()
    }

    /// Where a body is held, making it the most recently used of the lot — which
    /// is what holds on to the diagram a reader scrolled back to and lets go of
    /// one scrolled past. `None` where the body was never handed back.
    fn find(&self, body: &str) -> Option<usize> {
        let mut held = self.held.borrow_mut();
        let at = held.iter().position(|one| one.body == body)?;
        let one = held.remove(at)?;
        held.push_back(one);
        Some(held.len() - 1)
    }

    /// The body at `at`, held for as long as the caller needs it — a raster reads
    /// the svg through it, and the cache is not asked again underneath.
    fn held(&self, at: usize) -> Ref<'_, Held> {
        Ref::map(self.held.borrow(), |held| &held[at])
    }

    /// Asks for a body's drawing: it is asked for once and never twice while the
    /// answer is out, so a frame a tick long asks one question and not eight.
    fn ask(&self, body: &str) {
        if self.flight.borrow().iter().any(|one| one == body) {
            return;
        }
        let mut asked = self.asked.borrow_mut();
        if asked.iter().any(|one| one == body) {
            return;
        }
        asked.push(body.to_owned());
    }
}
