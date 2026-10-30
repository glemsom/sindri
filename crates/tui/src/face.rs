//! The face of Sindri, drawn where the terminal speaks the Kitty graphics
//! protocol; where it does not, there is no placeholder and no gap, and the
//! rail simply begins at its first fact. Which protocol that is, and how big
//! its cells are, is not the face's question to ask: it borrows the frame's one
//! answer (`graphics`).

use image::DynamicImage;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;
use ratatui_image::picker::ProtocolType;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{FilterType, Resize, StatefulImage};

use crate::graphics::Graphics;

/// The most rows the face may take. A square crop in a 32-column rail takes
/// fewer — the rail asks [`Face::rows`] rather than assuming this.
pub const ROWS: u16 = 20;

/// How the asset is scaled down to the rail's cells. Ratatui's own
/// default is nearest neighbour, which turns a glow drawn in single
/// pixels into grit; the whole picture is resampled from 795×989 to a few
/// hundred pixels a side.
const RESIZE: Resize = Resize::Fit(Some(FilterType::Lanczos3));

/// The alpha below which the asset is ground rather than picture. The picture
/// carries a wash of saturated blue over its whole canvas at ten per cent
/// opacity — a ground the artist drew on, not a halo. A terminal that blends
/// in linear light, as Ghostty does by default, turns ten per cent into most
/// of a third, so the rail gets a blue rectangle around the face instead of a
/// face. Everything under this floor is turned off, and what is left is
/// stretched back over the range so the glow keeps its curve.
const GROUND: u8 = 48;

const PNG: &[u8] = include_bytes!("../../../face.png");

pub struct Face {
    protocol: StatefulProtocol,
}

impl Face {
    /// The face the terminal's answer allows, or `None` where that answer is not
    /// Kitty graphics: the question was asked once, by [`Graphics`], and this
    /// only reads it.
    pub fn new(graphics: &Graphics) -> Option<Face> {
        let picker = graphics.picker();
        if picker.protocol_type() != ProtocolType::Kitty {
            return None;
        }
        let image = image::load_from_memory(PNG).ok()?;
        Some(Face {
            protocol: picker.new_resize_protocol(grounded(image)),
        })
    }

    /// The face a test draws, built from the made-up answer a test can afford:
    /// nothing is asked of a terminal, and the picture keeps its real shape.
    pub fn kitty() -> Option<Face> {
        Face::new(&Graphics::kitty())
    }

    pub fn rows(&self, area: Rect) -> u16 {
        self.protocol.size_for(RESIZE, area).height.min(area.height)
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        StatefulImage::<StatefulProtocol>::default()
            .resize(RESIZE)
            .render(area, buf, &mut self.protocol);
    }
}

/// The picture with its ground taken out: the whole frame, uncropped so that
/// no glow is cut off at an edge, and with the alpha below [`GROUND`] turned
/// off so that nothing is drawn where the artist only meant to draw a ground.
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
    use ratatui_image::picker::Picker;

    use super::*;

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

    /// An answer with Kitty set and a cell of the given size.
    fn answer(cell: (u16, u16)) -> Graphics {
        let mut picker = Picker::from_fontsize(cell);
        picker.set_protocol_type(ProtocolType::Kitty);
        Graphics::from_picker(picker)
    }

    #[test]
    fn the_face_is_the_shape_the_answer_it_is_given_gives_it() {
        let area = Rect::new(0, 0, 32, 40);
        let tall = Face::new(&answer((8, 18))).expect("a Kitty answer draws the embedded face");
        let taller = Face::new(&answer((8, 36))).expect("and so does another cell size");
        assert!(
            tall.rows(area) > taller.rows(area),
            "the rows are the shared answer's, not a cell size the face makes up: \
             {} against {}",
            tall.rows(area),
            taller.rows(area)
        );
    }

    #[test]
    fn a_terminal_that_does_not_speak_kitty_gets_no_face() {
        let graphics = Graphics::from_picker(Picker::from_fontsize((8, 18)));
        assert!(
            Face::new(&graphics).is_none(),
            "a picture is drawn where the terminal speaks Kitty, and nowhere else: no placeholder"
        );
    }

    #[test]
    fn the_face_is_drawn_whole_and_keeps_its_shape() {
        let face = Face::kitty().expect("the embedded face decodes");
        let drawn = face.rows(Rect::new(0, 0, 32, 20));
        assert!(
            drawn <= ROWS,
            "the picture never overruns the rail: {drawn}"
        );
        assert!(drawn > 0, "the picture takes rows: {drawn}");
    }
}
