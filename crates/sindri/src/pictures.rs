//! The pictures a frame has placed, kept by the cells they took and written back as the Kitty
//! protocol's unicode placeholders: the bytes go to the terminal once per size, and every
//! render after it draws glyphs that transmit nothing.

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

/// Shared, not owned: a frame is copied, and the copy must not transmit a picture the
/// protocol's own flag says the terminal already holds.
type Placed = Rc<RefCell<StatefulProtocol>>;

/// The pictures a frame has drawn, each kept by its source and the cells it took.
#[derive(Clone, Default)]
pub struct Pictures {
    picker: Option<Picker>,
    held: HashMap<(String, u16, u16), Placed>,
    /// What the paint in hand has placed; empty between paints, because a prune takes it.
    shown: HashSet<(String, u16, u16)>,
    /// How many times each source has been made into pixels: the instrument a test reads.
    rasterises: HashMap<String, usize>,
}

impl Pictures {
    /// A frame whose terminal places no picture: every source stays the words it was laid out as.
    pub fn none() -> Pictures {
        Pictures::default()
    }

    /// A frame whose terminal places pictures, in the Kitty protocol: the protocol is named
    /// here rather than guessed at, because one this frame cannot speak draws half a picture.
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

    /// Whether this terminal places pictures at all, which is what a block was laid out for.
    pub fn draws(&self) -> bool {
        self.picker.is_some()
    }

    /// How many times `source` has been made into pixels on this screen.
    pub fn rasterises(&self, source: &str) -> usize {
        self.rasterises.get(source).copied().unwrap_or(0)
    }

    /// Drops every picture the paint in hand did not place, so the width the band shows is the
    /// whole of what this cache holds: a size the band has left is made and sent again when it
    /// comes back, which is the cost of the bound.
    pub fn retain_shown(&mut self) {
        let shown = std::mem::take(&mut self.shown);
        self.held.retain(|key, _| shown.contains(key));
    }

    /// Draws `picture` over `area`, the cells the transcript reserved for it. A picture that
    /// cannot be made into pixels is drawn as nothing: its row already carries its words, and
    /// half a picture is worse than a caption.
    pub fn place(&mut self, picture: &Picture, cell: CellSize, area: Rect, buffer: &mut Buffer) {
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
        // The protocol remembers the rectangle it encoded, so asking about rows that moved
        // would send the bytes again on every paint: it is asked about the size at the origin.
        let size = Rect { x: 0, y: 0, ..area };
        if placed.needs_resize(&Resize::Scale(None), size).is_some() {
            placed.resize_encode(&Resize::Scale(None), size);
        }
        placed.render(area, buffer);
    }
}

impl std::fmt::Debug for Pictures {
    /// The sources and sizes this frame holds, and nothing of the protocol's own state.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Pictures")
            .field("draws", &self.draws())
            .field("held", &self.held.keys().collect::<Vec<_>>())
            .finish()
    }
}
