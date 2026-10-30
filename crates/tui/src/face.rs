//! The face of Sindri, drawn where the terminal speaks the Kitty graphics
//! protocol. Where it does not, there is no placeholder and no gap: the rail
//! simply begins at its first fact (ADR-0011 §5).

use std::io::IsTerminal;

use image::DynamicImage;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{FilterType, Resize, StatefulImage};

/// The most rows the face may take. A square crop in a 32-column rail takes
/// fewer — the rail asks [`Face::rows`] rather than assuming this.
pub const ROWS: u16 = 20;

/// How the asset is scaled down to the rail's cells. Ratatui's own default is
/// nearest neighbour, which turns a glow drawn in single pixels into grit;
/// the whole picture is resampled from 795×989 to a few hundred pixels a side.
const RESIZE: Resize = Resize::Fit(Some(FilterType::Lanczos3));

/// The alpha below which the asset is ground rather than picture.
///
/// The picture carries a wash of saturated blue over its whole canvas at ten
/// per cent opacity — a ground the artist drew on, not a halo. A terminal that
/// blends in linear light, as Ghostty does by default, turns ten per cent into
/// most of a third, so the rail gets a blue rectangle around the face instead
/// of a face. Everything under this floor is turned off, and what is left is
/// stretched back over the range so the glow keeps its curve.
const GROUND: u8 = 48;

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
        // A cell twice as tall as it is wide, as a terminal's is: the layout
        // tests then see the picture in the shape it really takes.
        let mut picker = Picker::from_fontsize((8, 18));
        picker.set_protocol_type(ProtocolType::Kitty);
        Face::from(picker)
    }

    fn from(picker: Picker) -> Option<Face> {
        let image = image::load_from_memory(PNG).ok()?;
        Some(Face {
            protocol: picker.new_resize_protocol(grounded(image)),
        })
    }

    /// The rows the face draws in when given `area`: the picture keeps its
    /// shape, so a rail taller in pixels than it is wide leaves rows to spare,
    /// and the rail must reserve only the rows the picture takes.
    pub fn rows(&self, area: Rect) -> u16 {
        self.protocol.size_for(RESIZE, area).height.min(area.height)
    }

    /// Draws the face into `area`, leaving the cells it covers alone.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        StatefulImage::<StatefulProtocol>::default()
            .resize(RESIZE)
            .render(area, buf, &mut self.protocol);
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

/// The picture with its ground taken out: the whole frame, uncropped so that
/// no glow is cut off at an edge, and with the alpha below [`GROUND`] turned
/// off so that nothing is drawn where the artist only meant to draw a ground.
///
/// Colour goes with it: a pixel that is transparent still carries blue, and
/// resampling mixes the colour of pixels the eye never sees, so the two are
/// cleared together.
fn grounded(image: DynamicImage) -> DynamicImage {
    let mut image = image.to_rgba8();
    for pixel in image.pixels_mut() {
        let alpha = pixel[3].saturating_sub(GROUND);
        if alpha == 0 {
            *pixel = image::Rgba([0, 0, 0, 0]);
        } else {
            pixel[3] = (u16::from(alpha) * 255 / u16::from(255 - GROUND)) as u8;
        }
    }
    DynamicImage::ImageRgba8(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture with a ground: transparent-blue everywhere, a figure in the
    /// middle, and one faint pixel just above the floor.
    fn picture() -> DynamicImage {
        let mut image = image::RgbaImage::new(4, 4);
        for pixel in image.pixels_mut() {
            *pixel = image::Rgba([0, 0, 254, 20]);
        }
        image.put_pixel(1, 1, image::Rgba([85, 255, 225, 255]));
        image.put_pixel(2, 1, image::Rgba([85, 255, 225, GROUND + 1]));
        DynamicImage::ImageRgba8(image)
    }

    #[test]
    fn the_ground_is_taken_out_and_the_picture_is_kept() {
        let grounded = grounded(picture()).to_rgba8();
        assert_eq!(
            *grounded.get_pixel(0, 0),
            image::Rgba([0, 0, 0, 0]),
            "the wash becomes nothing at all, colour included"
        );
        assert_eq!(
            grounded.get_pixel(1, 1)[3],
            255,
            "an opaque pixel stays opaque"
        );
        assert_eq!(
            grounded.get_pixel(1, 1).0[..3],
            [85, 255, 225],
            "and keeps its colour"
        );
        assert!(
            grounded.get_pixel(2, 1)[3] < 4,
            "a pixel on the floor is all but gone: {}",
            grounded.get_pixel(2, 1)[3]
        );
    }

    #[test]
    fn the_face_is_drawn_whole_and_keeps_its_shape() {
        // Nothing is cropped any more: the glow reached the frame's own edges
        // and the crop was what cut it.
        let face = Face::kitty().expect("the embedded face decodes");
        let drawn = face.rows(Rect::new(0, 0, 32, 20));
        assert!(
            drawn <= ROWS,
            "the picture never overruns the rail: {drawn}"
        );
        assert!(drawn > 0, "the picture takes rows: {drawn}");
    }
}
