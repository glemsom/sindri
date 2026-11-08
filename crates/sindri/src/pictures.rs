//! The pictures a frame has placed: every source the terminal was told about, kept by the
//! cells it took, and written back onto the band as the Kitty protocol's unicode
//! placeholders (§12, ADR-0008).
//!
//! This is the half of a picture that costs something, and both costs are decided here. The
//! pixels are made once per size and never again for that size, because a picture the reader
//! scrolls past is a placement of what the terminal already holds. And the bytes that carry a
//! picture are sent once per size, by the protocol's own first render: every render after it
//! writes the placeholder glyphs into the cells and transmits nothing, which is what makes
//! "a scroll never re-transmits a picture" a fact about the cache and not a hope about the
//! loop (ADR-0008).
//!
//! What is kept is what the band shows and nothing else ([`Pictures::retain_shown`]): a width
//! dragged across a range leaves one placement behind it, not a raster per size it passed
//! through, which is §12's "a resize re-rasterises only what is visible" as a cost and not as
//! a counter.
//!
//! A terminal that places no picture is given none of this at all: [`Pictures::none`] is a
//! frame with no protocol, and its pictures stay the words their block was laid out as. What
//! is never done is half a picture — a row of placeholders with nothing behind them, or a
//! picture drawn over the composer under the band (§12).
//!
//! Nothing here resolves a path or measures a footprint: a [`Picture`] arrives measured, and
//! [`crate::picture`] is where it was.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui_image::Resize;
use ratatui_image::ResizeEncodeRender;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

use crate::graphics::CellSize;
use crate::picture::Picture;

/// One picture the terminal has been told about, kept for the length of the launch.
///
/// Shared rather than owned, because a frame is a value that can be copied and the copy must
/// not transmit the same picture again: the protocol's own flag is what says the bytes are
/// already in the terminal, and two frames holding one picture is what keeps that a fact about
/// the picture rather than about the frame that asked first.
type Placed = Rc<RefCell<StatefulProtocol>>;

/// The pictures a frame has drawn, each kept by its source and the cells it took (ADR-0008).
///
/// The key is the source as the answer wrote it and the two cell counts the picture was
/// measured into, because those three are the whole of what a placement is: the same words
/// pointed at again are the same picture, and the same words in a narrower band are another
/// one.
#[derive(Clone, Default)]
pub struct Pictures {
    /// The protocol this terminal speaks and the font size a placement is built at, and
    /// nothing for a terminal that speaks no protocol for pictures.
    picker: Option<Picker>,
    /// The pictures placed so far, by source and the cells they took: the ones the band
    /// shows, because a paint drops what it did not place.
    held: HashMap<(String, u16, u16), Placed>,
    /// The pictures the paint in hand has placed: what a prune keeps, and nothing between one
    /// paint and the next.
    shown: HashSet<(String, u16, u16)>,
    /// How many times each source has been made into pixels: the instrument a test reads
    /// instead of the shape of this code, and the number §12's "a resize re-rasterises only
    /// what is visible" is a statement about.
    rasterises: HashMap<String, usize>,
}

impl Pictures {
    /// A frame whose terminal places no picture: every source stays the words its block was
    /// laid out as, which is the honest answer on a terminal that speaks no protocol for
    /// them (§12, ADR-0003).
    pub fn none() -> Pictures {
        Pictures::default()
    }

    /// A frame whose terminal places pictures: the Kitty protocol's unicode placeholders,
    /// asked for at the cell size the terminal really has.
    ///
    /// The protocol is named here rather than guessed at, because a terminal that speaks
    /// something else is a terminal this frame draws words on: the one alternative never taken
    /// is a picture half-drawn, or a row of half-block glyphs standing in for a screenshot and
    /// claiming to be it (§12).
    pub fn kitty(cell: CellSize) -> Pictures {
        let font = (
            u16::try_from(cell.width()).unwrap_or(u16::MAX),
            u16::try_from(cell.height()).unwrap_or(u16::MAX),
        );
        let mut picker = Picker::from_fontsize(font);
        picker.set_protocol_type(ProtocolType::Kitty);
        Pictures {
            picker: Some(picker),
            ..Pictures::default()
        }
    }

    /// Whether this frame's terminal places pictures at all: what a picture's block was laid
    /// out for, because a block of rows nothing fills is a hole in the transcript (§12).
    pub fn draws(&self) -> bool {
        self.picker.is_some()
    }

    /// How many times `source` has been made into pixels on this screen.
    pub fn rasterises(&self, source: &str) -> usize {
        self.rasterises.get(source).copied().unwrap_or(0)
    }

    /// Drops every picture the paint in hand has not placed, and keeps the ones it has: what
    /// the band shows is the whole of what this cache holds, so a width dragged across a range
    /// leaves one placement behind it rather than a raster per size it passed through (§12).
    ///
    /// It is called once per paint of the transcript's band, after the band's rows are placed.
    /// A size the band shows again is a picture the terminal already holds and is written as
    /// placeholders with nothing transmitted; a size the band has left is made and sent again
    /// when it comes back, which is the one cost of the bound and is cheaper than keeping every
    /// size a resize passed through for the length of the launch (ADR-0008, #55).
    pub fn retain_shown(&mut self) {
        let shown = std::mem::take(&mut self.shown);
        self.held.retain(|key, _| shown.contains(key));
    }

    /// Draws `picture` over `area`, which is the block of cells the transcript reserved for
    /// it: the bytes go to the terminal the first time this source is seen at this size, and
    /// every placement after that writes placeholders pointing at what is already there
    /// (ADR-0008).
    ///
    /// A picture whose file cannot be made into pixels is drawn as nothing at all, because the
    /// row it stands on already carries its words: a refusal at this point has no row of its
    /// own to be said on, and half a picture is worse than a caption.
    pub fn place(&mut self, picture: &Picture, cell: CellSize, area: Rect, buffer: &mut Buffer) {
        // Borrowed and not cloned: a copy per placement would be a copy per row painted for
        // a value nothing writes to. What is written here are the cache's own fields.
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let source = picture.source().to_owned();
        let key = (source.clone(), picture.cols(), picture.rows());
        self.shown.insert(key.clone());
        let placed = match self.held.get(&key) {
            Some(placed) => placed.clone(),
            None => {
                let Some(pixels) = picture.rasterise(cell) else {
                    return;
                };
                *self.rasterises.entry(source).or_default() += 1;
                let placed = Rc::new(RefCell::new(
                    picker.new_resize_protocol(image::DynamicImage::ImageRgba8(pixels)),
                ));
                self.held.insert(key, placed.clone());
                placed
            }
        };
        let mut placed = placed.borrow_mut();
        // The protocol remembers the rectangle it encoded, so asking it about rows that
        // moved would send the bytes again on every paint: it is asked about the size at the
        // origin, and the placeholders take the block's real cells.
        let size = Rect { x: 0, y: 0, ..area };
        if placed.needs_resize(&Resize::Scale(None), size).is_some() {
            placed.resize_encode(&Resize::Scale(None), size);
        }
        placed.render(area, buffer);
    }
}

impl std::fmt::Debug for Pictures {
    /// The pictures this frame holds, as the sources and sizes they are held by: what a reader
    /// of a failed test needs, and nothing of the protocol's own state.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Pictures")
            .field("draws", &self.draws())
            .field("held", &self.held.keys().collect::<Vec<_>>())
            .finish()
    }
}
