//! The seam's light strip: the picture the seam is made of where the terminal
//! can draw one, and nothing else. A function from a size and the theme's two
//! stops to an `image::RgbaImage`, so what the strip looks like is a test's
//! business rather than a screenshot's. Nothing here owns a layout, an `App` or
//! a terminal, and nothing here draws: making the strip once per body height,
//! and replacing it on a resize, is the drawing side's business.

use image::RgbaImage;
use ratatui::style::Color;

use crate::theme::{HEADING, PROMPT};

/// The alpha the strip peaks at: about a half, not nine tenths.
///
/// `face.rs`'s `GROUND` is the reason, written down there first: a terminal
/// that blends in linear light — Ghostty does by default — turns ten per cent
/// of alpha into most of a third, so a wash meant to be felt arrives as a
/// block of colour. A hairline at 0.55, tripled, is bright; the curve either
/// side of it is still a curve.
pub const PEAK: f32 = 0.55;

/// How sharply the light falls off from the rail toward the transcript: the
/// exponent of the horizontal ramp, and the whole design of the strip. One
/// cell is a hairline at the peak and a glow across the last few pixels, so
/// the curve is a fourth power rather than a square: a third of the way in is
/// a hundredth of the peak, and the second-to-last column stays under it even
/// where a terminal triples the light.
pub const FALLOFF: i32 = 4;

/// The two stops the light is made of, read off the theme's names once: a
/// renderer names a colour or nothing, so the strip blends `HEADING` and
/// `PROMPT` and names no third hue of its own.
const STOPS: [[u8; 3]; 2] = [channels(HEADING), channels(PROMPT)];

/// The strip as a picture: `px_w` pixels across and `px_h` pixels down, the
/// transcript's edge as its first column and the rail's as its last. A size too small to have both edges is no light
/// where it cannot be a gradient: one column is the rail's, and so the peak,
/// while one row is the last row, and so nothing.
pub fn strip(px_w: u32, px_h: u32) -> RgbaImage {
    let size = Size { px_w, px_h };
    let mut picture = RgbaImage::new(px_w, px_h);
    for y in 0..px_h {
        let [r, g, b] = hue(size, y);
        for x in 0..px_w {
            picture.put_pixel(x, y, image::Rgba([r, g, b, size.alpha(x, y)]));
        }
    }
    picture
}

/// The pixels a strip was asked for, and the two ramps its edges need: they
/// travel together, and every division by a span is answered in one place.
#[derive(Clone, Copy)]
struct Size {
    px_w: u32,
    px_h: u32,
}

impl Size {
    /// How far across the strip a pixel sits: nothing at the transcript's
    /// edge, all the way at the rail's.
    fn across(self, x: u32) -> f32 {
        share(x, self.px_w)
    }

    /// How much of the light a row keeps: all of it at the top, under the
    /// face, and none of it on the last row, so the seam dissolves instead of
    /// stopping.
    fn fade(self, y: u32) -> f32 {
        1.0 - share(y, self.px_h)
    }

    /// The alpha of one pixel: the peak, shaped from the transcript's edge
    /// toward the rail, and again from the top of the seam down to its last
    /// row.
    fn alpha(self, x: u32, y: u32) -> u8 {
        let lit = PEAK * 255.0 * self.across(x).powi(FALLOFF) * self.fade(y);
        (lit.round() as u8).min(floor())
    }
}

/// The brightest alpha the peak constant allows, as a pixel.
fn floor() -> u8 {
    (PEAK * 255.0).round() as u8
}

/// How far along a span of `px` pixels a step sits, from nothing to one. A
/// span of one pixel is one pixel wide, so every step in it is its last.
fn share(at: u32, px: u32) -> f32 {
    if px > 1 {
        at as f32 / (px - 1) as f32
    } else {
        1.0
    }
}

/// One row's colour: the theme's `HEADING` at the top, its `PROMPT` on the
/// last row, and the walk between them. Every pixel carries the hue,
/// transparent ones too, because a terminal that resamples the strip mixes the
/// colour of pixels the eye never sees, and a black pixel would drag the lamp
/// grey.
fn hue(size: Size, y: u32) -> [u8; 3] {
    let t = 1.0 - size.fade(y);
    let [from, to] = STOPS;
    [0, 1, 2].map(|channel| {
        let (start, end) = (f32::from(from[channel]), f32::from(to[channel]));
        (start + (end - start) * t).round() as u8
    })
}

/// A theme stop's truecolour bytes, read at compile time: a stop that is not a
/// truecolour is a module that does not build rather than a colour the strip
/// cannot blend.
const fn channels(colour: Color) -> [u8; 3] {
    match colour {
        Color::Rgb(r, g, b) => [r, g, b],
        _ => panic!("the strip blends two truecolour stops"),
    }
}
