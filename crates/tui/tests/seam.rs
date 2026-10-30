//! The seam's light strip, held to what it looks like rather than to a
//! screenshot: a pure function of a size, walked pixel by pixel (#62).

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use image::RgbaImage;
use sindri_tui::{seam, theme};

/// The strip as the frame asks for it: one cell of eight pixels, twelve rows
/// of eighteen — the body's height at the answer's own cell.
const WIDTH: u32 = 8;
const HEIGHT: u32 = 12 * 18;

fn strip() -> RgbaImage {
    seam::strip(WIDTH, HEIGHT)
}

/// The brightest alpha the peak constant allows, as a pixel.
fn peak() -> u8 {
    (seam::PEAK * 255.0).round() as u8
}

/// The alpha down one column of the strip, top row first.
fn down(image: &RgbaImage, x: u32) -> Vec<u8> {
    (0..image.height())
        .map(|y| image.get_pixel(x, y)[3])
        .collect()
}

/// The alpha across one row, the transcript's edge first.
fn across(image: &RgbaImage, y: u32) -> Vec<u8> {
    (0..image.width())
        .map(|x| image.get_pixel(x, y)[3])
        .collect()
}

#[test]
fn the_strip_is_the_pixels_it_is_given() {
    let strip = strip();
    assert_eq!(
        strip.dimensions(),
        (WIDTH, HEIGHT),
        "the light is the width of the margin and the height of the body, to the pixel"
    );
}

#[test]
fn the_light_leaves_the_transcript_edge_dark_and_peaks_in_the_column_before_the_rail() {
    let strip = strip();
    let peaks = across(&strip, 0);
    assert_eq!(
        peaks[0], 0,
        "the edge that touches the transcript is no light at all: {peaks:?}"
    );
    assert!(
        peaks.windows(2).all(|pair| pair[0] <= pair[1]),
        "the light only ever grows toward the rail: {peaks:?}"
    );
    assert_eq!(
        *peaks.last().expect("a last column"),
        peak(),
        "the last pixel column is the peak, and the peak is the constant: {peaks:?}"
    );
    assert_eq!(
        peaks.iter().filter(|alpha| **alpha == peak()).count(),
        1,
        "one column is a hairline at the peak; the rest of the cell is a glow: {peaks:?}"
    );
}

#[test]
fn the_seam_ends_by_dissolving_rather_than_by_stopping() {
    let strip = strip();
    let edge = down(&strip, WIDTH - 1);
    assert!(
        edge[0] > edge[HEIGHT as usize / 2],
        "the light is brightest at the top, under the face, and dimmer halfway down: {edge:?}"
    );
    assert_eq!(
        *edge.last().expect("a last row"),
        0,
        "the last row is no light at all: the seam has a bottom rather than an edge"
    );
    assert!(
        edge.windows(2).all(|pair| pair[0] >= pair[1]),
        "and it never brightens on the way down: {edge:?}"
    );
}

/// A theme stop's truecolour bytes, as the strip must paint them — read here
/// rather than through the module, so the expectation comes from the theme and
/// not from the code it is checking.
fn stop(colour: ratatui::style::Color) -> [u8; 3] {
    match colour {
        ratatui::style::Color::Rgb(r, g, b) => [r, g, b],
        other => panic!("the theme names a truecolour, and {other:?} is not one"),
    }
}

/// One pixel's colour, without its alpha.
fn colour(image: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let pixel = image.get_pixel(x, y);
    [pixel[0], pixel[1], pixel[2]]
}

#[test]
fn the_seams_hue_travels_from_the_heading_to_the_prompt() {
    let strip = strip();
    let rail = WIDTH - 1;
    assert_eq!(
        colour(&strip, rail, 0),
        stop(theme::HEADING),
        "the top of the seam is the hue the face's headings are: one lamp, not two"
    );
    assert_eq!(
        colour(&strip, rail, HEIGHT - 1),
        stop(theme::PROMPT),
        "and the bottom of it is the hue the prompt is"
    );
}

#[test]
fn no_pixel_is_a_colour_the_theme_does_not_name() {
    let strip = strip();
    let (from, to) = (stop(theme::HEADING), stop(theme::PROMPT));
    for (x, y, pixel) in strip.enumerate_pixels() {
        let painted = [pixel[0], pixel[1], pixel[2]];
        assert!(
            on_the_way(painted, from, to),
            "({x},{y}) {painted:?} is a hue neither stop names, and the theme names stops, \
             not colours"
        );
    }
}

/// Whether a colour is a blend of the theme's two stops rather than a hue of
/// its own: the mix the strip claims to make, checked from the outside.
fn on_the_way(painted: [u8; 3], from: [u8; 3], to: [u8; 3]) -> bool {
    // The channel the two stops differ in most says how far along the walk a
    // pixel is; every other channel must agree with it to within a rounding.
    let (at, span) = (0..3)
        .map(|channel| {
            (
                channel,
                i32::from(to[channel].max(from[channel]))
                    - i32::from(to[channel].min(from[channel])),
            )
        })
        .max_by_key(|(_, span)| *span)
        .expect("three channels");
    assert!(
        span > 0,
        "two stops that name the same colour are not a walk"
    );
    let walked = (0..=255)
        .map(|step: u16| {
            let t = f32::from(step) / 255.0;
            (0..3)
                .map(|channel| {
                    let start = f32::from(from[channel]);
                    let end = f32::from(to[channel]);
                    (start + (end - start) * t).round() as i32
                })
                .collect::<Vec<i32>>()
        })
        .find(|blend| blend[at] == i32::from(painted[at]));
    walked.is_some_and(|blend| {
        (0..3).all(|channel| (blend[channel] - i32::from(painted[channel])).abs() <= 1)
    })
}

#[test]
fn no_pixel_is_brighter_than_the_peak_and_the_peak_says_why_it_is_not_near_opaque() {
    let strip = strip();
    assert!(
        (0.5..0.6).contains(&seam::PEAK),
        "the peak is about a half, not nine tenths: {}",
        seam::PEAK
    );
    let cap = (seam::PEAK * 255.0).round() as u8;
    let brightest = strip.pixels().map(|pixel| pixel[3]).max();
    assert!(
        brightest.is_some_and(|alpha| alpha <= cap),
        "no pixel outruns the peak constant ({cap}): {brightest:?}"
    );
    // The constant carries its reason beside it, and the reason is the one
    // `face.rs` already paid for: a terminal that blends in linear light.
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/seam.rs"))
        .expect("the seam's own source");
    let at = source.find("pub const PEAK").expect("the peak constant");
    let doc = source[..at]
        .rsplit("\n\n")
        .next()
        .expect("the doc block above the constant");
    assert!(
        doc.contains("GROUND") && doc.contains("face.rs"),
        "the peak names face.rs's GROUND as its reason: {doc}"
    );
}

#[test]
fn the_same_size_gives_the_same_bytes_twice() {
    assert_eq!(
        strip().as_raw(),
        seam::strip(WIDTH, HEIGHT).as_raw(),
        "the strip is a function of its size: no clock, no state, no terminal"
    );
}

#[test]
fn nothing_here_asks_a_terminal_anything() {
    // The strip is a picture, not a drawing: it borrows no `Picker` and knows
    // no protocol. The frame asks once and hands it a size (#63).
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/seam.rs"))
        .expect("the seam's own source");
    let renderer = source.split("#[cfg(test)]").next().unwrap_or_default();
    for needle in ["Picker", "Terminal", "ProtocolType", "from_query_stdio"] {
        assert!(
            !renderer.contains(needle),
            "the seam is a pure function of a size, and it touches {needle}"
        );
    }
}

#[test]
fn a_body_of_no_rows_or_no_margin_is_no_light_rather_than_a_panic() {
    for (w, h) in [(0, 0), (0, HEIGHT), (WIDTH, 0), (WIDTH, 1), (1, HEIGHT)] {
        let strip = seam::strip(w, h);
        assert_eq!(strip.dimensions(), (w, h));
        if h == 1 {
            assert!(
                strip.pixels().all(|pixel| pixel[3] == 0),
                "a seam with no room to fall has nothing to dissolve and so no light"
            );
        }
        if w == 1 && h > 1 {
            assert_eq!(
                strip.get_pixel(0, 0)[3],
                peak(),
                "one column of margin is the column that touches the rail, so it is the peak"
            );
        }
    }
}

#[test]
fn tripled_by_a_terminal_that_blends_in_linear_light_the_strip_is_still_a_gradient() {
    // `face.rs`'s `GROUND` again: what a low wash becomes in linear light. The
    // peak may saturate there, but a third of the way in the curve is still so
    // far under it that the cell reads as a glow rather than as a bar.
    let peaks = across(&strip(), 0);
    let peak = *peaks.last().expect("a last column");
    let third = peaks[peaks.len() / 3];
    assert!(
        third.saturating_mul(3) < peak,
        "a third of the way in is {third}, against a peak of {peak}: {peaks:?}"
    );
    let beside = peaks[peaks.len() - 2];
    assert!(
        beside.saturating_mul(3) < 255,
        "even tripled the hairline is a hairline: the column beside the peak is {beside} \
         and does not run out of room: {peaks:?}"
    );
}
