//! The ink the stroke is drawn in: the panel's own on the quiet side, the
//! seam's on the side the light arrives, the theme's walk between them — and
//! nothing else, at any alpha.

use super::*;

#[test]
fn the_left_side_is_the_panels_ink_and_the_right_side_the_seams_own() {
    let frame = frame();
    let middle = frame.height() / 2;
    assert_eq!(
        rgb_of(&frame, 0, middle),
        rgb(theme::PROMPT),
        "the left vertical is the ink the panel is drawn in, to the last pixel"
    );
    assert_eq!(
        rgb_of(&frame, 0, border::CHAMFER),
        rgb(theme::PROMPT),
        "and so is the corner it turns"
    );
    for y in [
        border::CHAMFER,
        middle,
        frame.height() - 1 - border::CHAMFER,
    ] {
        assert_eq!(
            rgb_of(&frame, frame.width() - 1, y),
            rgb(theme::SEAM),
            "the right vertical and both its corners are the seam's own, at row {y}"
        );
    }
    // On the cuts themselves, halfway along each diagonal: the corners rather
    // than the rules that meet at them.
    let (w, h) = (frame.width(), frame.height());
    let half = border::CHAMFER / 2;
    let corners = [
        (half, half),
        (w - 1 - half, half),
        (half, h - 1 - half),
        (w - 1 - half, h - 1 - half),
    ];
    for (x, y) in corners {
        assert!(
            alpha(&frame, x, y) > 0,
            "the corner at {x},{y} is on the stroke"
        );
        let want = if x < w / 2 {
            theme::PROMPT
        } else {
            theme::SEAM
        };
        assert_eq!(
            rgb_of(&frame, x, y),
            rgb(want),
            "the corner at {x},{y} takes the ink of the side it is on"
        );
    }
}

#[test]
fn the_rule_walks_from_the_one_ink_to_the_other_and_ends_where_they_do() {
    let frame = frame();
    let cut = border::CHAMFER;
    let bottom = frame.height() - 1;
    assert_eq!(
        rgb_of(&frame, cut, bottom),
        rgb(theme::PROMPT),
        "the rule begins in exactly the ink its corner is drawn in"
    );
    assert_eq!(
        rgb_of(&frame, frame.width() - 2 - cut, bottom),
        rgb(theme::SEAM),
        "and ends in the seam's, so the corner needs no second hue"
    );
    let middle = rgb_of(&frame, frame.width() / 4, bottom);
    assert_ne!(middle, rgb(theme::PROMPT), "and it walks between them");
    assert_ne!(middle, rgb(theme::SEAM), "without arriving at either early");
}

#[test]
fn the_quiet_ink_is_the_one_the_composer_hands_over() {
    let faint = border::stroke(PANEL, CELL, border::Ink::Faint, LABEL);
    assert_eq!(
        rgb_of(&faint, 0, faint.height() / 2),
        rgb(theme::FAINT),
        "a run holding the keyboard draws its panel in faint, and the picture says so"
    );
    assert_eq!(
        rgb_of(&faint, faint.width() - 1, faint.height() / 2),
        rgb(theme::SEAM),
        "while the side the light lands on is the seam's whatever the ink is"
    );
}

#[test]
fn a_cut_that_leaves_rule_on_both_sides_walks_the_two_pieces_together() {
    // A label further in leaves the top rule a wall on each side of the cut, and
    // both pieces carry the walk the bottom rule carries at the same column.
    let label = 5..13;
    let frame = border::stroke(PANEL, CELL, border::Ink::Prompt, label.clone());
    let gap = (
        u32::from(label.start) * u32::from(CELL.0),
        u32::from(label.end) * u32::from(CELL.0),
    );
    let walls = [
        border::CHAMFER,
        gap.0 - border::PAD - 1,
        gap.1 + border::PAD + 1,
        frame.width() - 2 - border::CHAMFER,
    ];
    for x in walls {
        assert!(alpha(&frame, x, 0) > 0, "the top rule is painted at {x}");
        assert_eq!(
            rgb_of(&frame, x, 0),
            rgb_of(&frame, x, frame.height() - 1),
            "and both rules are the same colour at column {x}: one walk, cut once"
        );
    }
    assert_eq!(
        rgb_of(&frame, border::CHAMFER, 0),
        rgb(theme::PROMPT),
        "the walk still leaves the panel's ink"
    );
    assert_eq!(
        rgb_of(&frame, frame.width() - 2 - border::CHAMFER, 0),
        rgb(theme::SEAM),
        "and arrives at the seam's"
    );
}

/// The one law a picture obeys: every hue it paints is the theme's, and the hue
/// is checked whatever the alpha — the transparent air included, so nothing
/// hides behind a faint edge and a resampled picture drags no black into the
/// stroke. A picture's cells are placeholders and carry no colour, which is why
/// the law is asked of the pixel and not of the cell.
#[test]
fn every_painted_pixel_is_a_colour_the_theme_owns() {
    for panel in [
        frame(),
        border::stroke(PANEL, CELL, border::Ink::Faint, LABEL),
    ] {
        for y in 0..panel.height() {
            for x in 0..panel.width() {
                let [r, g, b, _] = at(&panel, x, y);
                let colour = Color::Rgb(r, g, b);
                assert!(
                    theme::owns(colour),
                    "the pixel at {x},{y} is {colour:?}, a hue the theme does not name or walk"
                );
            }
        }
    }
}
