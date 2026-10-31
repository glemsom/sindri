//! The theme's promises, held by something other than the renderers: every
//! colour the frame paints is one the theme names, no renderer carries a
//! literal of its own, the faint grey is legible on every surface, the inks
//! that order themselves against a surface are ordered by nearness rather than
//! brightness (`orderings`), every surface and every ink of a painted frame
//! (`cells`), and — the law in `pixels` — every pixel of a picture the frame
//! generates is a colour the theme owns.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

/// The law for pixels, in the same spirit and the same place as the cell law
/// below: a picture the frame generates is held to the ramps one pixel at a
/// time.
mod pixels;

/// The laws the frame's cells are held to: the surface under each, and the ink
/// in it.
mod cells;

/// The theme's ordering laws: nearness, stated in one place so a light palette
/// can be held to it too.
mod orderings;

use ratatui::style::Color;
use sindri_tui::theme;

/// A walk names no hue of its own: every step of the composer's border is on
/// the segment between two colours the theme names, and anything off it is a
/// renderer's invention.
#[test]
fn a_walk_between_named_stops_arrives_at_colours_the_theme_owns() {
    for step in 0..=100 {
        let t = step as f32 / 100.0;
        let walked = theme::ramp(theme::PROMPT, theme::SEAM, t);
        assert!(
            theme::owns(walked),
            "step {t} of the walk landed on {walked:?}, which the theme does not own"
        );
        let walked = theme::ramp(theme::FAINT, theme::SEAM, t);
        assert!(
            theme::owns(walked),
            "step {t} of the quiet walk landed on {walked:?}"
        );
    }
    assert!(theme::owns(theme::SEAM), "a walk's far end is a stop");
    assert!(
        !theme::owns(Color::Rgb(0xff, 0x00, 0x80)),
        "a hue a renderer invented is refused"
    );
    // Walked off the end, the ramp is still its own stop: a step of nought and
    // a step of one are the two colours it was given.
    assert_eq!(theme::ramp(theme::PROMPT, theme::SEAM, 0.0), theme::PROMPT);
    assert_eq!(theme::ramp(theme::PROMPT, theme::SEAM, 1.0), theme::SEAM);
}

/// The theme is the only file that may name a colour: a renderer that writes
/// one down is a renderer that has left the theme behind.
#[test]
fn no_renderer_hard_codes_a_colour() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the source tree") {
            let path = entry.expect("a source entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && path.file_name().is_some_and(|name| name != "theme.rs")
            {
                files.push(path);
            }
        }
    }
    assert!(files.len() > 10, "the source tree was found: {files:?}");
    let named = [
        "Color::Rgb(0x",
        "Color::Black",
        "Color::White",
        "Color::Gray",
        "Color::DarkGray",
        "Color::Red",
        "Color::Green",
        "Color::Blue",
        "Color::Yellow",
        "Color::Magenta",
        "Color::Cyan",
    ];
    for file in files {
        let source = std::fs::read_to_string(&file).expect("a source file");
        // A unit test may name a colour to prove a style is copied without
        // one; the renderer above it may not.
        let source = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .to_string();
        for needle in named {
            assert!(
                !source.contains(needle),
                "{} names a colour ({needle}) instead of asking the theme",
                file.display()
            );
        }
    }
}

/// WCAG's relative luminance, so the ratio below is the one a reader's eye
/// is owed and not a guess.
fn luminance(colour: Color) -> f64 {
    let Color::Rgb(r, g, b) = colour else {
        panic!("a contrast ratio needs a truecolour");
    };
    let channel = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn ratio(fg: Color, bg: Color) -> f64 {
    let (brighter, darker) = {
        let (a, b) = (luminance(fg), luminance(bg));
        if a > b { (a, b) } else { (b, a) }
    };
    (brighter + 0.05) / (darker + 0.05)
}

#[test]
fn faint_clears_four_and_a_half_to_one_on_the_panel_and_on_the_canvas() {
    // The canvas is the theme's own, filled before anything is drawn on it.
    for surface in [theme::PANEL, theme::CANVAS] {
        let cleared = ratio(theme::FAINT, surface);
        assert!(
            cleared >= 4.5,
            "faint on {surface:?} is {cleared:.2}:1, under the 4.5:1 a body needs"
        );
    }
}

/// The panel's inks hold the floor with margin rather than sitting just over
/// it: `FAINT` is a fence's edge and a diff's file line, read as often as the
/// code beside them.
#[test]
fn the_panels_faint_and_a_diffs_file_line_hold_the_floor_with_margin() {
    let raised = ratio(theme::FAINT, theme::PANEL);
    assert!(
        raised >= 6.8,
        "faint on the panel is {raised:.2}:1, under the 6.8:1 it was raised to"
    );
    let file = ratio(theme::DIFF_FILE, theme::PANEL);
    assert!(
        file >= 4.5,
        "a diff's file line on the panel is {file:.2}:1"
    );
    // The file line follows faint, which is nearness to the panel both are
    // drawn on — see `orderings` for why an ordering is never brightness.
    orderings::Inks::shipped().assert_file_line_nearer_the_panel();
}

/// A fence's ink is the theme's, and every role in it is body text: a reader
/// reads code the same way they read prose, so each role clears the same floor
/// `FAINT` does, on the panel a fence is drawn on.
#[test]
fn every_syntax_role_clears_four_and_a_half_to_one_on_the_panel() {
    let roles = [
        ("PLAIN", theme::PLAIN),
        ("COMMENT", theme::COMMENT),
        ("KEYWORD", theme::KEYWORD),
        ("FUNCTION", theme::FUNCTION),
        ("TYPE", theme::TYPE),
        ("NUMBER", theme::NUMBER),
        ("STRING", theme::STRING),
    ];
    for (role, colour) in roles {
        assert!(
            theme::PALETTE.contains(&colour),
            "the {role} role is a colour the theme names"
        );
        let cleared = ratio(colour, theme::PANEL);
        assert!(
            cleared >= 4.5,
            "the {role} role on the panel is {cleared:.2}:1, under the 4.5:1 a fence's ink needs"
        );
    }
}

#[test]
fn the_rails_leader_and_seam_are_named_stops_a_step_apart() {
    // The rail's inks are the theme's, not a renderer's.
    assert!(
        theme::PALETTE.contains(&theme::LEADER),
        "a dot leader is painted in a colour the theme names"
    );
    assert!(
        theme::PALETTE.contains(&theme::SEAM),
        "the seam is painted in a colour the theme names"
    );
    // The leader only has to be seen, so it is dimmer than the label it
    // bridges from; the seam is dots at an eighth of a cell, which read darker
    // than they are, so it is a step above faint. Both are statements about
    // nearness — contrast against the canvas — and never about brightness:
    // `orderings` holds the same two laws against a light palette, where a
    // nearer ink is a darker one.
    let inks = orderings::Inks::shipped();
    inks.assert_leader_nearer_the_canvas();
    inks.assert_seam_beyond_faint();
    // A leader is a graphic, not a word, so 3:1 is the bar it clears on the
    // canvas — the gap it bridges is seen, never guessed at. It is measured on
    // the canvas the frame paints, not on a black the frame never sees, and it
    // clears the bar with margin rather than sitting on it.
    let cleared = ratio(theme::LEADER, theme::CANVAS);
    assert!(
        cleared >= 3.3,
        "a leader on the canvas is {cleared:.2}:1, under the 3.3:1 it was raised to"
    );
}
