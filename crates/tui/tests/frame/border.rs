//! The composer's border as a terminal is sent it: rounded glyphs, four sides,
//! walked from the panel's ink at the left to the seam's at the right. The
//! assertions are on the cells a reader sees — the corners that close the frame,
//! the step each column takes, and the ink at either end — and never on a
//! screenshot.

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use sindri_tui::theme;

/// The composer's frame, found by its rounded corners rather than by counting
/// rows: a frame that lost a corner is a failure this says out loud.
fn frame_of(buf: &Buffer) -> Rect {
    let (x, y) = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|(x, y)| buf[(*x, *y)].symbol() == "╭")
        .expect("the composer's corner is rounded");
    let right = (x..buf.area.width)
        .find(|column| buf[(*column, y)].symbol() == "╮")
        .expect("the top rule closes on a rounded corner");
    let bottom = (y..buf.area.height)
        .find(|row| buf[(x, *row)].symbol() == "╰")
        .expect("the left vertical closes on a rounded corner");
    assert_eq!(
        buf[(right, bottom)].symbol(),
        "╯",
        "and the bottom rule closes on one too"
    );
    Rect {
        x,
        y,
        width: right - x + 1,
        height: bottom - y + 1,
    }
}

/// One truecolour channel of a cell, so a walk can be read as numbers.
fn channel(colour: Color, at: usize) -> i16 {
    match colour {
        Color::Rgb(r, g, b) => i16::from([r, g, b][at]),
        other => panic!("{other:?} is not a truecolour, and the walk is"),
    }
}

/// Every side of the frame walked from `from` at its left-hand end to the
/// seam's at its right-hand one, one small step at a time: what the effect is,
/// asserted where a reader would see it.
fn walked(buf: &Buffer, from: Color) {
    let area = frame_of(buf);
    assert_eq!(
        buf[(area.x, area.y)].fg,
        from,
        "the walk leaves the ink the panel has at its left-hand end"
    );
    assert_eq!(
        buf[(area.right() - 1, area.y)].fg,
        theme::SEAM,
        "and arrives at the seam's at the right-hand one, beside the light"
    );
    // The two verticals are the walk's two ends, not a flat ink of their own:
    // a side that kept one colour would be a step the corners have to hide.
    for y in [area.y + 1, area.bottom() - 2] {
        assert_eq!(
            buf[(area.x, y)].fg,
            from,
            "the left vertical is the near end of the walk on row {y}"
        );
        assert_eq!(
            buf[(area.right() - 1, y)].fg,
            theme::SEAM,
            "and the right vertical is the far end on row {y}"
        );
    }
    // The top rule walked one step a column, and slowly: the red channel is the
    // one the two stops are furthest apart in, and a column that jumped a tenth
    // of the way would read as two borders rather than as one light.
    let mut previous = channel(from, 0);
    for x in area.x..area.right() {
        let cell = &buf[(x, area.y)];
        if !matches!(cell.symbol(), "─" | "╭" | "╮") {
            continue;
        }
        assert!(
            theme::owns(cell.fg),
            "({x}, {}) is walked off the theme's segments: {:?}",
            area.y,
            cell.fg
        );
        let step = channel(cell.fg, 0) - previous;
        assert!(
            (0..=8).contains(&step),
            "the walk took {step} of the red channel at column {x}, which is no slow change"
        );
        previous = channel(cell.fg, 0);
    }
    // The bottom rule is the same walk: a frame lit on one side only is a frame
    // with a shadow under it.
    assert_eq!(
        buf[(area.x + 1, area.bottom() - 1)].fg,
        buf[(area.x + 1, area.y)].fg,
        "the bottom rule steps with the top one"
    );
    assert_eq!(
        buf[(area.right() - 2, area.bottom() - 1)].fg,
        buf[(area.right() - 2, area.y)].fg,
        "at both ends, and every column between"
    );
}

#[test]
fn the_composer_is_a_rounded_frame_walked_from_the_prompt_to_the_seam() {
    walked(&buffer(&mut app(), 100, 24), theme::PROMPT);
}

#[test]
fn a_run_quiets_the_walk_without_taking_its_ends_away() {
    let mut app = app();
    type_text(&mut app, "why does this test fail?");
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.start().expect("the draft becomes a run");
    walked(&buffer(&mut app, 100, 24), theme::FAINT);
}

/// The frame at the narrowest terminal the composer is drawn on whole: a walk
/// with no columns to spend is still the frame's two ends and never a panic.
#[test]
fn a_narrow_panel_walks_the_columns_it_has() {
    let buf = buffer(&mut app(), 40, 12);
    let area = frame_of(&buf);
    assert!(
        area.width >= 2,
        "the composer is a frame at forty columns: {area:?}"
    );
    assert_eq!(
        buf[(area.x, area.y)].fg,
        theme::PROMPT,
        "a frame too narrow for a gradient leaves from the ink it was drawn in"
    );
    assert_eq!(
        buf[(area.right() - 1, area.y)].fg,
        theme::SEAM,
        "and still lands on the seam at its right-hand corner"
    );
}
