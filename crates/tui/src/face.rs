//! The face of Sindri, drawn where the terminal speaks the Kitty graphics
//! protocol. Where it does not, there is no placeholder and no gap: the rail
//! simply begins at its first fact (ADR-0011 §5).

use std::io::IsTerminal;

use image::DynamicImage;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;
use ratatui_image::StatefulImage;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

/// The rows the face occupies: 20×20 cells.
pub const ROWS: u16 = 20;

/// The asset itself, carried in the binary: an installed copy needs no
/// neighbouring file.
const PNG: &[u8] = include_bytes!("../../../face.png");

/// The face, and the protocol state drawing it keeps between frames.
pub struct Face {
    protocol: StatefulProtocol,
}

impl Face {
    /// The face for a terminal that answers for Kitty graphics, or nothing at
    /// all — silence is the fallback.
    pub fn detect() -> Option<Face> {
        if !std::io::stdout().is_terminal() || !announces_kitty() {
            return None;
        }
        let picker = Picker::from_query_stdio().ok()?;
        if picker.protocol_type() != ProtocolType::Kitty {
            return None;
        }
        Face::from(picker)
    }

    /// The face drawn as Kitty graphics whether or not this terminal answers:
    /// the embedded asset, as both `detect` and the layout tests use it.
    pub fn kitty() -> Option<Face> {
        let mut picker = Picker::from_fontsize((8, 12));
        picker.set_protocol_type(ProtocolType::Kitty);
        Face::from(picker)
    }

    fn from(picker: Picker) -> Option<Face> {
        let image = image::load_from_memory(PNG).ok()?;
        Some(Face {
            protocol: picker.new_resize_protocol(square(image)),
        })
    }

    /// Draws the face into `area`, leaving the cells it covers alone.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        StatefulImage::<StatefulProtocol>::default().render(area, buf, &mut self.protocol);
    }
}

/// Whether this terminal can plausibly speak Kitty graphics. Asking costs a
/// second of startup on every terminal that cannot answer, so a terminal that
/// says nothing about itself is never asked: silence is the fallback anyway.
fn announces_kitty() -> bool {
    const TELLS: [&str; 3] = ["TERM", "TERM_PROGRAM", "LC_TERMINAL"];
    const SPEAKERS: [&str; 4] = ["kitty", "ghostty", "wezterm", "foot"];
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || TELLS.iter().any(|name| {
            std::env::var(name).is_ok_and(|value| {
                let value = value.to_ascii_lowercase();
                SPEAKERS.iter().any(|speaker| value.contains(speaker))
            })
        })
}

/// The square crop the face is drawn from: the middle of the picture, so the
/// figure is centred rather than stretched.
fn square(image: DynamicImage) -> DynamicImage {
    let (width, height) = (image.width(), image.height());
    let side = width.min(height);
    image.crop_imm((width - side) / 2, (height - side) / 2, side, side)
}
