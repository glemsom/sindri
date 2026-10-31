//! The law for pixels, held as the cell law is: every pixel a picture paints is
//! `theme::owns` of its colour — a `PALETTE` name or a step on a `RAMPS`
//! segment, within the one step a float walk rounds to — whatever its alpha.
//! Alpha is not a hiding place, so a hue drawn at four out of two hundred and
//! fifty-five is as much a renderer's invention as one drawn opaque, and a
//! picture's edge antialiases in alpha alone because blending a hue toward the
//! canvas would leave the ramp. What that edge fades against is the canvas the
//! frame paints, which is why a renderer draws a picture over a ground it has
//! painted rather than over whatever the terminal had there.
//!
//! A picture whose pixels the frame *chooses* is held to it: the seam's strip,
//! which is a constant walked into ink. A picture the frame only *places* — the
//! face, an answer's image — is not: its hues are the artist's and the
//! reader's, never a renderer's.

use image::{Rgba, RgbaImage};
use sindri_tui::{seam, theme};

/// The generated pictures this frame paints, at the sizes the frame asks them
/// for, including a strip too small to be a gradient. A painter that generates
/// pixels is added here as well as to the frame: the law only sees the pictures
/// it is shown.
fn pictures() -> Vec<(&'static str, RgbaImage)> {
    vec![
        ("the seam's strip", seam::strip(8, 12 * 18)),
        ("a strip of one pixel", seam::strip(1, 1)),
        ("a strip with no row to fade down", seam::strip(8, 1)),
    ]
}

/// The first pixel of a picture the theme does not own: where it is and
/// what it was painted in, which is what a failure has to name.
fn off_ramp(image: &RgbaImage) -> Option<((u32, u32), Rgba<u8>)> {
    image
        .enumerate_pixels()
        .find(|(_, _, pixel)| !theme::owns_pixel(pixel))
        .map(|(x, y, pixel)| ((x, y), *pixel))
}

/// A truecolour's bytes, so a test can name a colour the theme names.
fn rgb(colour: ratatui::style::Color) -> [u8; 3] {
    match colour {
        ratatui::style::Color::Rgb(r, g, b) => [r, g, b],
        other => panic!("{other:?} is not a truecolour"),
    }
}

#[test]
fn every_pixel_a_picture_paints_comes_from_a_colour_the_theme_names() {
    for (what, picture) in pictures() {
        assert!(picture.pixels().count() > 0, "{what} paints something");
        if let Some(((x, y), pixel)) = off_ramp(&picture) {
            panic!(
                "{what} paints ({x},{y}) in {:?}, a hue the theme does not name",
                pixel.0
            );
        }
    }
}

/// The acceptance of the law: a picture walked from the ramps passes whole,
/// and one invented pixel in it fails by its coordinates and its colour.
#[test]
fn a_walked_picture_passes_and_one_invented_pixel_fails_by_its_place() {
    let (from, to) = (rgb(theme::PROMPT), rgb(theme::SEAM));
    let mut walked = RgbaImage::new(16, 16);
    for y in 0..walked.height() {
        for x in 0..walked.width() {
            let t = x as f32 / (walked.width() - 1) as f32;
            let mut step = [0; 3];
            for (channel, byte) in step.iter_mut().enumerate() {
                let (start, end) = (f32::from(from[channel]), f32::from(to[channel]));
                *byte = (start + (end - start) * t).round() as u8;
            }
            walked.put_pixel(x, y, Rgba([step[0], step[1], step[2], (y * 16) as u8]));
        }
    }
    assert_eq!(
        off_ramp(&walked),
        None,
        "a walk of the ramps is the whole law"
    );

    walked.put_pixel(9, 3, Rgba([0xff, 0x3b, 0x00, 0xff]));
    assert_eq!(
        off_ramp(&walked),
        Some(((9, 3), Rgba([0xff, 0x3b, 0x00, 0xff]))),
        "one invented hue in one pixel fails, named by its place and its colour"
    );
}

/// Alpha is not a colour, and it is not a hiding place either: the hue of a
/// pixel is the hue of a pixel at every opacity.
#[test]
fn a_hue_is_judged_whatever_its_alpha() {
    for alpha in [0, 1, 4, 128, 254, 255] {
        let [r, g, b] = rgb(theme::PROMPT);
        assert!(
            theme::owns_pixel(&Rgba([r, g, b, alpha])),
            "a named colour is the theme's at alpha {alpha}"
        );
        assert!(
            !theme::owns_pixel(&Rgba([0xff, 0x00, 0x80, alpha])),
            "a hue a renderer invented is refused at alpha {alpha}: a picture \
             cannot smuggle one in by drawing it faintly"
        );
    }
}
