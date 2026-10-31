//! The panel every section draws, and the reading of a pixel: the picture's
//! helpers, so no section carries a copy of another's arithmetic.

use image::RgbaImage;
use ratatui::layout::Rect;
use ratatui::style::Color;
use sindri_tui::border;

/// The panel a composer draws: the transcript's width in columns, four rows
/// tall, at the cell the frame is designed for.
pub const PANEL: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 4,
};
pub const CELL: (u16, u16) = (8, 18);
/// ` prompt `: eight columns, inset a cell from the corner so it clears the
/// chamfer — which is what the design allows the label to do.
pub const LABEL: std::ops::Range<u16> = 2..10;
/// Where a column's stroke ends and the panel begins: the most a taper spends.
pub const REACH: f32 = border::LAMP + 0.5;

/// The picture under test: the panel at the prompt's ink, with its label on the
/// top rule's first two cells of air.
pub fn frame() -> RgbaImage {
    border::stroke(PANEL, CELL, border::Ink::Prompt, LABEL)
}

/// What the picture holds at one pixel.
pub fn at(frame: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    frame.get_pixel(x, y).0
}

pub fn alpha(frame: &RgbaImage, x: u32, y: u32) -> u8 {
    at(frame, x, y)[3]
}

/// The colour of one pixel, which is only ever a theme colour.
pub fn rgb_of(frame: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let [r, g, b, _] = at(frame, x, y);
    [r, g, b]
}

/// A theme colour's truecolour bytes: every ink here is a `PALETTE` name.
pub fn rgb(colour: Color) -> [u8; 3] {
    match colour {
        Color::Rgb(r, g, b) => [r, g, b],
        other => panic!("{other:?} is not a truecolour"),
    }
}

/// The top rule's thickness at column `x`, in pixels: the alphas it spends down
/// the column, which is the stroke's weight where nothing else is drawn.
pub fn down(frame: &RgbaImage, x: u32) -> f32 {
    (0..frame.height())
        .map(|y| f32::from(alpha(frame, x, y)) / 255.0)
        .take_while(|weight| *weight > 0.0)
        .sum()
}

/// The right vertical's thickness at row `y`, in pixels: the alphas it spends
/// along the last few columns of the row.
pub fn across(frame: &RgbaImage, y: u32) -> f32 {
    (frame.width() - 8..frame.width())
        .map(|x| f32::from(alpha(frame, x, y)) / 255.0)
        .sum()
}

/// How far a painted pixel sits from the stroke, in the spec's own terms: from
/// the corner's 45° line where it is inside a corner's square, which is the only
/// stroke a cut corner has, and from the nearest edge everywhere else.
pub fn distance(frame: &RgbaImage, x: u32, y: u32) -> f32 {
    let (w, h) = (frame.width() as f32, frame.height() as f32);
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    let cut = f32::from(border::CHAMFER as u16);
    let near_x = px < cut + REACH || px > w - cut - REACH;
    let near_y = py < cut + REACH || py > h - cut - REACH;
    let diagonals = [
        (cut - px - py).abs(),
        (px - py - (w - cut)).abs(),
        (py - px - (h - cut)).abs(),
        (px + py - (w + h - cut)).abs(),
    ];
    let corner = (near_x && near_y)
        .then(|| diagonals.into_iter().fold(f32::INFINITY, f32::min) / 2.0_f32.sqrt());
    [px, py, w - px, h - py]
        .into_iter()
        .chain(corner)
        .fold(f32::INFINITY, f32::min)
}
