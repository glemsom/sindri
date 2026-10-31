//! The composer seen as a frame: what a picture of its border may change and
//! what it may not, held where the frame is drawn rather than through a
//! terminal.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use sindri_core::config::Provider;

use crate::graphics::Graphics;
use crate::rail::Launch;
use crate::settings::Missing;

/// The composer's rect in a screen of forty columns: wide enough for the
/// label, the draft and the border, and offset so a cell the composer
/// painted is never one it painted by accident.
const AREA: Rect = Rect {
    x: 1,
    y: 2,
    width: 40,
    height: 4,
};

fn launch() -> Launch {
    Launch {
        session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
        temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
        branch: Some("issue-91".to_string()),
        sandbox: "bwrap · 1 writable".to_string(),
        workspace: "~/git/sindri".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        max_turns: 250,
        context_limit: Some(200_000),
    }
}

/// A frame whose terminal speaks Kitty graphics and so draws its border as a
/// picture, or one that draws no picture at all.
fn app(kitty: bool) -> App {
    let graphics = kitty.then(Graphics::kitty);
    App::new(launch(), None).with_graphics(graphics)
}

/// The composer drawn into a screen of its own, as `render::draw` hands it
/// one.
fn drawn(app: &mut App, area: Rect) -> Buffer {
    let screen = Rect::new(0, 0, area.width + area.x + 2, area.height + area.y + 2);
    let mut buf = Buffer::empty(screen);
    draw(app, area, &mut buf);
    buf
}

/// Types `text` into the composer, one key at a time: a draft is what the
/// user made, not something a test set behind their back.
fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
}

/// A draft two rows long, as `Shift+Enter` makes one.
fn drafted(app: &mut App) {
    typed(app, "why does this test fail?");
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    typed(app, "and the row under it?");
}

/// The cells of the frame's ring: the two rules and the two verticals.
fn ring(area: Rect) -> Vec<(u16, u16)> {
    let mut cells = Vec::new();
    for x in area.x..area.x + area.width {
        cells.push((x, area.y));
        cells.push((x, area.y + area.height - 1));
    }
    for y in area.y + 1..area.y + area.height - 1 {
        cells.push((area.x, y));
        cells.push((area.x + area.width - 1, y));
    }
    cells
}

/// Whether a symbol is one of the glyphs a rounded frame is drawn with: the
/// only cells a picture replaces.
fn glyph(symbol: &str) -> bool {
    matches!(symbol, "─" | "│" | "╭" | "╮" | "╰" | "╯")
}

/// Whether `(x, y)` is one of a rect's cells.
fn inside(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

/// What one row of `area` reads as, an escape taken as the one mark the cell
/// it rides in stands for.
fn line(buf: &Buffer, area: Rect, y: u16) -> String {
    (area.x..area.x + area.width)
        .map(|x| match buf[(x, y)].symbol() {
            symbol if symbol.contains('\u{1b}') => "\u{25A9}".to_string(),
            symbol => symbol.to_string(),
        })
        .collect()
}

/// The escapes a frame carries, one per cell they ride in.
fn escapes(buf: &Buffer) -> Vec<String> {
    buf.content
        .iter()
        .map(|cell| cell.symbol().to_string())
        .filter(|symbol| symbol.contains('\u{1b}'))
        .collect()
}

#[test]
fn a_terminal_that_draws_no_picture_gets_the_rounded_frame_it_always_did() {
    let buf = drawn(&mut app(false), AREA);
    let top = line(&buf, AREA, AREA.y);
    assert!(
        top.starts_with("╭ prompt "),
        "the floor is the rounded frame with its title: {top:?}"
    );
    assert!(
        top.ends_with("╮") && buf[(AREA.x, AREA.y + AREA.height - 1)].symbol() == "╰",
        "and its four corners: {top:?}"
    );
    assert!(
        buf[(AREA.x + AREA.width - 1, AREA.y)].fg != Color::Reset,
        "with the rule walked toward the seam"
    );
    assert!(
        escapes(&buf).is_empty(),
        "a terminal that speaks no graphics draws no picture"
    );
}

#[test]
fn the_border_cells_are_blank_surface_where_the_picture_is_drawn() {
    let buf = drawn(&mut app(true), AREA);
    for (x, y) in ring(AREA) {
        let cell = &buf[(x, y)];
        assert!(
            !glyph(cell.symbol()),
            "({x}, {y}) is a border cell and draws no glyph: {:?}",
            cell.symbol()
        );
        assert_eq!(
            cell.bg,
            theme::PANEL,
            "({x}, {y}) is the panel's surface the picture is drawn on"
        );
    }
}

#[test]
fn the_picture_is_placed_once_over_the_composer_rect() {
    let buf = drawn(&mut app(true), AREA);
    assert_eq!(
        escapes(&buf).len(),
        1,
        "one escape, in the one cell it is anchored at"
    );
    assert!(
        buf[(AREA.x, AREA.y)].symbol().contains("\u{1b}_G"),
        "and it is the composer's own top-left cell"
    );
}

#[test]
fn the_two_frames_differ_only_in_the_border_cells() {
    let mut plain = app(false);
    let mut kitty = app(true);
    drafted(&mut plain);
    drafted(&mut kitty);
    let plain = drawn(&mut plain, AREA);
    let kitty = drawn(&mut kitty, AREA);
    let ring = ring(AREA);
    let label = stroke::label_cells(AREA);
    for y in 0..plain.area.height {
        for x in 0..plain.area.width {
            let (left, right) = (&plain[(x, y)], &kitty[(x, y)]);
            if ring.contains(&(x, y)) && !inside(label, x, y) {
                assert!(
                    !glyph(right.symbol()),
                    "({x}, {y}) is a border cell, and the stroke draws it"
                );
                continue;
            }
            assert_eq!(
                left, right,
                "({x}, {y}) is inside the composer, and a picture is a layer, not a cell"
            );
        }
    }
}

#[test]
fn the_placeholder_is_the_same_words_on_both_paths() {
    let plain = drawn(&mut app(false), AREA);
    let kitty = drawn(&mut app(true), AREA);
    let inside = Rect {
        x: AREA.x + 1,
        width: AREA.width - 2,
        ..AREA
    };
    let words = line(&plain, inside, AREA.y + 1);
    assert!(
        words.starts_with("Ask, or drop a log here · @ path · / c"),
        "an empty composer invites the first keystroke: {words:?}"
    );
    assert_eq!(
        line(&kitty, inside, AREA.y + 1),
        words,
        "and a picture changes nothing about the words inside"
    );
}

#[test]
fn the_label_is_real_text_in_the_gap_the_picture_leaves_for_it() {
    let buf = drawn(&mut app(true), AREA);
    let top = line(&buf, AREA, AREA.y);
    assert!(
        top.starts_with("\u{25A9} prompt "),
        "the prompt is a word on the border, at the column it sits at today: {top:?}"
    );
    assert_eq!(
        buf[(AREA.x + 2, AREA.y)].fg,
        theme::PROMPT,
        "and it is drawn in the ink the panel is lit with"
    );
}

#[test]
fn the_label_lands_on_the_cells_the_glyph_frame_draws_it_in() {
    let plain = drawn(&mut app(false), AREA);
    let kitty = drawn(&mut app(true), AREA);
    let label = stroke::label_cells(AREA);
    for x in label.x..label.x + label.width {
        assert_eq!(
            plain[(x, AREA.y)],
            kitty[(x, AREA.y)],
            "({x}, {}) is the label, and the picture cuts a gap for exactly it",
            AREA.y
        );
    }
}

#[test]
fn the_missing_line_is_the_same_words_on_both_paths() {
    // A run with no key says so where the prompt would be, and a picture of
    // the border does not change a word of it.
    let missing = || {
        let missing = Missing::of(Provider::Anthropic, true, false)
            .expect("a run with no key is missing one");
        let mut app = app(true);
        app.missing = Some(missing);
        app
    };
    let mut kitty = missing();
    let mut plain = app(false);
    plain.missing = kitty.missing;
    let inside = Rect {
        x: AREA.x + 1,
        width: AREA.width - 2,
        ..AREA
    };
    let words = line(&drawn(&mut plain, AREA), inside, AREA.y + 1);
    assert!(
        words.contains("cannot be sent"),
        "the composer states what it cannot do: {words:?}"
    );
    assert_eq!(
        line(&drawn(&mut kitty, AREA), inside, AREA.y + 1),
        words,
        "and the picture leaves the line where the glyphs drew it"
    );
}

#[test]
fn the_cursor_is_given_the_same_point_on_both_paths() {
    let mut plain = app(false);
    let mut kitty = app(true);
    typed(&mut plain, "ask");
    typed(&mut kitty, "ask");
    let mut plain_buf = Buffer::empty(Rect::new(0, 0, 60, 12));
    let mut kitty_buf = Buffer::empty(Rect::new(0, 0, 60, 12));
    assert_eq!(
        draw(&mut plain, AREA, &mut plain_buf),
        draw(&mut kitty, AREA, &mut kitty_buf),
        "a drawn border that shifts the cursor by one column is worse than any glyph"
    );
}
