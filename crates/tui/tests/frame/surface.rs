//! The surface: the frame's tiers of nearness — a composer that floats on its
//! own panel, a code fence that is a panel, a rail on a panel, and an overlay
//! that dims what it covers rather than punching a hole in it (ADR-0011 §23).

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier};
use sindri_tui::theme;

fn buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(app, frame))
        .expect("the frame draws");
    terminal.backend().buffer().clone()
}

/// Where a glyph was drawn, top-left first: the composer's corner and the
/// panel's are both found this way rather than by counting rows.
fn find(buf: &Buffer, glyph: char) -> Option<(u16, u16)> {
    let mut needle = [0u8; 4];
    let symbol = glyph.encode_utf8(&mut needle);
    (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|(x, y)| buf[(*x, *y)].symbol() == symbol)
}

fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
}

#[test]
fn the_composer_is_a_panel_whose_border_says_who_has_the_focus() {
    let mut app = app();
    let idle = buffer(&mut app, 100, 24);
    let (x, y) = find(&idle, '╭').expect("the composer's corner is rounded");
    assert_eq!(
        idle[(x, y)].fg,
        theme::PROMPT,
        "the composer holds the focus, so its border is the prompt's colour"
    );
    assert_eq!(
        idle[(x, y)].bg,
        theme::PANEL,
        "the composer floats on a panel, not on the terminal's black"
    );
    let right = (x..100)
        .find(|column| idle[(*column, y)].symbol() == "╮")
        .expect("a rounded border closes on its right corner, not a square one");
    assert_eq!(
        idle[(right, y)].fg,
        theme::SEAM,
        "the border is a walk lit from the rail's side: its far corner is the seam's ink"
    );
    assert_ne!(
        idle[(right, y)].fg,
        idle[(x, y)].fg,
        "a walk that arrives where it started says nothing"
    );
    let half = (x + right) / 2;
    assert!(
        idle[(half, y)].symbol() == "─" && theme::owns(idle[(half, y)].fg),
        "the rule between the corners is walked, and never off the theme's segments: {:?}",
        idle[(half, y)].fg
    );
    assert!(
        (0..24).any(|row| idle[(x, row)].symbol() == "╰"),
        "and on its bottom corner: {idle:?}"
    );
    assert_eq!(idle[(x + 2, y + 1)].bg, theme::PANEL, "the panel is filled");

    typed(&mut app, "why does this test fail?");
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.start().expect("the draft becomes a run");
    let running = buffer(&mut app, 100, 24);
    let (x, y) = find(&running, '╭').expect("the composer's corner is still there");
    assert_eq!(
        running[(x, y)].fg,
        theme::FAINT,
        "a live run takes the focus, so the border steps back without vanishing"
    );
    assert_eq!(
        running[(x, y)].bg,
        theme::PANEL,
        "the panel does not move when the border changes"
    );
}

/// The overlay's own rectangle, found by its rounded corner: the topmost one
/// on screen, because the composer's is drawn below it.
fn overlay_box(buf: &Buffer) -> Rect {
    let (x, y) = find(buf, '╭').expect("the overlay's corner is rounded");
    let right = (x..buf.area.width)
        .find(|column| buf[(*column, y)].symbol() == "╮")
        .expect("the overlay closes on the right");
    let bottom = (y..buf.area.height)
        .find(|row| buf[(x, *row)].symbol() == "╰")
        .expect("the overlay closes at the bottom");
    Rect {
        x,
        y,
        width: right - x + 1,
        height: bottom - y + 1,
    }
}

fn dims(buf: &Buffer) -> Vec<(u16, u16)> {
    (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .filter(|(x, y)| buf[(*x, *y)].modifier.contains(Modifier::DIM))
        .collect()
}

#[test]
fn a_help_overlay_dims_what_it_covers_instead_of_punching_a_hole() {
    let mut app = app();
    app.event(Event::TextDelta {
        turn: 1,
        text: "an answer the overlay is about to cover".to_string(),
    });
    app.key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    let buf = buffer(&mut app, 100, 40);
    let boxed = overlay_box(&buf);

    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let dim = buf[(x, y)].modifier.contains(Modifier::DIM);
            if boxed.contains(Position::new(x, y)) {
                assert!(
                    !dim,
                    "the overlay reads at full strength: ({x},{y}) is {}",
                    buf[(x, y)].symbol()
                );
            } else {
                assert!(dim, "what the overlay covers recedes: ({x},{y})");
            }
        }
    }
    assert!(
        dims(&buf).len() > 1000,
        "the pass covers the frame, not a border of it"
    );
}

#[test]
fn a_spill_overlay_dims_the_transcript_it_covers() {
    let dir = tempfile::TempDir::new().expect("a temp directory");
    let path = dir.path().join("spill.txt");
    std::fs::write(&path, "line one\nline two\nline three\n").expect("the spill file");
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("seq 500".to_string()),
    });
    app.over(
        CallOver::Exited(0),
        Spilled {
            truncated: true,
            files: vec![path],
        },
    );
    app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(
        matches!(app.overlay(), Some(sindri_tui::overlay::Overlay::Spill(_))),
        "Ctrl-O opens the spill file"
    );
    let buf = buffer(&mut app, 100, 40);
    let boxed = overlay_box(&buf);
    assert!(
        !boxed.contains(Position::new(1, 1)),
        "the spill covers the frame below the top row: {boxed:?}"
    );
    assert!(
        buf[(0, 0)].modifier.contains(Modifier::DIM),
        "the rows it covers are dimmed"
    );
}

#[test]
fn the_rail_is_chrome_on_the_canvas_one_column_away_from_the_transcript() {
    let mut app = app();
    let buf = buffer(&mut app, 100, 24);
    let width = buf.area.width as usize;
    let composer = find(&buf, '╭').map(|(_, y)| y).expect("the composer");
    let mut chips = 0;
    let mut banded = 0;
    for y in 0..composer {
        let mut crossable = false;
        for x in (width - RAIL)..width {
            let tier = buf[(x as u16, y)].bg;
            // The posture's chip is the one row of the rail with a surface
            // behind it; every other cell of the rail is the canvas the
            // transcript is written on (ADR-0011 §23).
            if tier == theme::RAISED {
                chips += 1;
                crossable = true;
                continue;
            }
            assert_eq!(
                tier,
                Color::Reset,
                "the rail draws no fill, so the canvas shows through: ({x},{y})"
            );
        }
        assert_eq!(
            buf[((width - RAIL - 1) as u16, y)].bg,
            Color::Reset,
            "and it leaves the terminal's own canvas between it and the words: ({y})"
        );
        banded += usize::from(crossable);
    }
    assert!(chips > 0, "the posture is the one raised thing on the rail");
    assert_eq!(
        banded, 1,
        "the rail spends its ink on one row, the chip's, and not on a fill"
    );
}

#[test]
fn a_running_calls_stdout_and_stderr_differ_without_colour() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("make".to_string()),
    });
    app.output(Side::Stdout, "compiling sindri-tui");
    app.output(Side::Stderr, "warning: unused import");
    let rows = screen(&mut app, 100, 24);
    let mark = |needle: &str| {
        let row = rows
            .iter()
            .find(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} is on screen: {rows:#?}"));
        row.split(needle).next().unwrap_or_default().to_string()
    };
    let out = mark("compiling sindri-tui");
    let err = mark("warning: unused import");

    assert!(
        !out.trim().is_empty() && !err.trim().is_empty(),
        "both sides carry a marker: {out:?} and {err:?}"
    );
    assert_ne!(
        out, err,
        "the two sides are told apart by a glyph, not by hue alone: {rows:#?}"
    );
}
