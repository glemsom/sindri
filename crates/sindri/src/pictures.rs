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

pub const PLACEHOLDER: char = '\u{10EEEE}';

#[derive(Clone, Default)]
pub struct Pictures {
    picker: Option<Picker>,
    held: HashMap<(String, u16, u16), Placed>,
    shown: HashSet<(String, u16, u16)>,
    rasterises: HashMap<String, usize>,
}

impl Pictures {
    pub fn none() -> Pictures {
        Pictures::default()
    }

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

    pub fn cell(&self) -> Option<CellSize> {
        self.picker.as_ref().map(|picker| {
            let (width, height) = picker.font_size();
            CellSize::new(u32::from(width), u32::from(height))
        })
    }

    pub fn celled(self, cell: CellSize) -> Pictures {
        match self.draws() && self.cell() != Some(cell) {
            true => Pictures {
                rasterises: self.rasterises,
                ..Pictures::kitty(cell)
            },
            false => self,
        }
    }

    pub fn draws(&self) -> bool {
        self.picker.is_some()
    }

    pub fn rasterises(&self, source: &str) -> usize {
        self.rasterises.get(source).copied().unwrap_or(0)
    }

    pub fn retain_shown(&mut self) {
        let shown = std::mem::take(&mut self.shown);
        self.held.retain(|key, _| shown.contains(key));
    }

    pub fn place(&mut self, picture: &Picture, cell: CellSize, shown: Rect, buffer: &mut Buffer) {
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
        let size = Rect {
            x: 0,
            y: 0,
            width: picture.cols(),
            height: picture.rows(),
        };
        if placed.needs_resize(&Resize::Scale(None), size).is_some() {
            placed.resize_encode(&Resize::Scale(None), size);
        }
        placed.render(shown, buffer);
    }
}

impl std::fmt::Debug for Pictures {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Pictures")
            .field("draws", &self.draws())
            .field("held", &self.held.keys().collect::<Vec<_>>())
            .finish()
    }
}
