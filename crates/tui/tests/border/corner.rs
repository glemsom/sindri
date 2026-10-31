//! The corners and the label: four cuts at 45°, nothing painted off the stroke,
//! nothing in the interior, and a label that sits in a cut rather than in front
//! of a rule.

use super::*;

#[test]
fn every_corner_is_cut_by_45_degrees() {
    let frame = frame();
    let (w, h) = (frame.width(), frame.height());
    for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1), (1, 1)] {
        assert!(
            alpha(&frame, x, y) == 0,
            "the square corner {x},{y} is cut away, not rounded and not painted"
        );
    }
    // The cut is the stroke, and the corner it cut away is air: a 45° line, with
    // no arc and nothing painted back onto the triangle it took.
    let cut = border::CHAMFER;
    assert!(
        alpha(&frame, cut, 0) > 0 && alpha(&frame, cut - 6, 6) > 0,
        "the diagonal is the corner's stroke"
    );
    assert_eq!(
        alpha(&frame, cut - 9, 3),
        0,
        "and no pixel of the corner is inside it: there is no arc, only a cut"
    );
    let (fw, fh) = (f32::from(w as u16), f32::from(h as u16));
    let cut = f32::from(border::CHAMFER as u16);
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let outside = [
                cut - px - py,
                px - py - (fw - cut),
                py - px - (fh - cut),
                px + py - (fw + fh - cut),
            ]
            .into_iter()
            .fold(f32::NEG_INFINITY, f32::max);
            if outside > 0.5 {
                assert_eq!(
                    alpha(&frame, x, y),
                    0,
                    "pixel {x},{y} is {outside:.2} px outside a corner's cut, and the \
                     corner is cut"
                );
            }
        }
    }
}

#[test]
fn every_painted_pixel_lies_on_the_stroke_and_nothing_lies_inside() {
    let frame = frame();
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            if alpha(&frame, x, y) == 0 {
                continue;
            }
            let reach = distance(&frame, x, y);
            assert!(
                reach <= REACH,
                "pixel {x},{y} is painted {reach:.2} px from the stroke, which is no stroke"
            );
        }
    }
    // Clear of the chamfer and of the taper's width: the panel's wash stays the
    // panel's, because the picture is a stroke over an unknown background.
    let inset = border::CHAMFER + REACH.ceil() as u32;
    for y in inset..frame.height() - inset {
        for x in inset..frame.width() - inset {
            assert_eq!(
                alpha(&frame, x, y),
                0,
                "the interior is transparent at {x},{y}"
            );
        }
    }
}

#[test]
fn the_label_sits_in_a_cut_and_no_pixel_of_its_cells_is_painted() {
    let frame = frame();
    let (from, to) = (
        u32::from(LABEL.start) * u32::from(CELL.0),
        u32::from(LABEL.end) * u32::from(CELL.0),
    );
    // The label sits on the top rule, so its cells are the top row's.
    for y in 0..u32::from(CELL.1) {
        for x in from..to {
            assert_eq!(
                alpha(&frame, x, y),
                0,
                "the label's cell at {x},{y} is air: the rule is cut, not drawn under words"
            );
        }
    }
    for x in [from - 1, from - 2] {
        assert_eq!(
            alpha(&frame, x, 0),
            0,
            "the cut has air on the label's near side, at {x}"
        );
    }
    assert!(
        alpha(&frame, to + border::PAD + 1, 0) > 0,
        "and the rule comes back past it, up to the corner it turns"
    );
}

#[test]
fn a_panel_with_no_pixels_is_no_picture() {
    for area in [
        Rect::new(0, 0, 0, 0),
        Rect::new(0, 0, 4, 0),
        Rect::new(0, 0, 0, 4),
    ] {
        assert_eq!(
            border::stroke(area, CELL, border::Ink::Prompt, LABEL)
                .pixels()
                .len(),
            0,
            "a rect with nothing in {area:?} draws nothing, rather than a drawing of nothing"
        );
    }
}

#[test]
fn a_panel_too_small_for_its_own_chamfer_still_draws() {
    // Four columns of the transcript, one row: the chamfer is clamped to the
    // panel, and the stroke is spent inside what is left rather than running off
    // it or panicking.
    let small = Rect {
        width: 4,
        height: 1,
        ..PANEL
    };
    let frame = border::stroke(small, CELL, border::Ink::Prompt, 1..2);
    assert_eq!(
        frame.dimensions(),
        (32, 18),
        "the picture is still exactly the cells it was given"
    );
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            let [r, g, b, _] = at(&frame, x, y);
            assert!(
                theme::owns(Color::Rgb(r, g, b)),
                "a small panel paints the theme's colours too, at {x},{y}"
            );
        }
    }
    assert!(
        (0..frame.width()).any(|x| alpha(&frame, x, frame.height() / 2) > 0),
        "and its stroke is somewhere on it, not painted out of existence"
    );
}
