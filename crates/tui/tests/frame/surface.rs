//! The surface: the frame's tiers of nearness — a composer that floats on its
//! own panel, a code fence that is a panel, a rail on a panel, and an overlay
//! that dims what it covers rather than punching a hole in it.

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use sindri_tui::theme;

/// Where a glyph was drawn, top-left first: the composer's corner and the
/// panel's are both found this way rather than by counting rows.
fn find(buf: &Buffer, glyph: char) -> Option<(u16, u16)> {
    let mut needle = [0u8; 4];
    let symbol = glyph.encode_utf8(&mut needle);
    (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|(x, y)| buf[(*x, *y)].symbol() == symbol)
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

    type_text(&mut app, "why does this test fail?");
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

/// The composer's own rectangle, found by its corner: nothing is open above it,
/// so the topmost rounded corner on screen is the panel's.
fn composer_box(buf: &Buffer) -> Rect {
    let (left, top) = find(buf, '╭').expect("the composer's corner is rounded");
    let right = (left..buf.area.width)
        .find(|column| buf[(*column, top)].symbol() == "╮")
        .expect("the composer closes on its right");
    let bottom = (top..buf.area.height)
        .find(|row| buf[(left, *row)].symbol() == "╰")
        .expect("the composer closes at the bottom");
    Rect {
        x: left,
        y: top,
        width: right - left + 1,
        height: bottom - top + 1,
    }
}

#[test]
fn the_composer_ends_where_the_words_end() {
    // At a hundred columns the body splits into the words' 66, the seam's two
    // and the rail's 32. The composer is laid out in the first of them, so the
    // panel ends where the prose does: its right border sits one column off the
    // strip's own first cell, and not one cell of the rail's column is painted.
    let mut app = app();
    let buf = buffer(&mut app, 100, 24);
    let panel = composer_box(&buf);
    assert_eq!(
        panel.width, 66,
        "the panel takes the transcript's width, not the terminal's: {panel:?}"
    );
    assert_eq!(
        panel.right(),
        MARGIN as u16,
        "and its corner lands on the column the strip's light reaches"
    );

    for y in panel.top()..panel.bottom() {
        for x in panel.right()..buf.area.width {
            let cell = &buf[(x, y)];
            assert_eq!(
                cell.bg,
                theme::CANVAS,
                "({x}, {y}) is beside the panel, and the canvas the theme names is under it"
            );
            assert_eq!(
                cell.symbol(),
                " ",
                "({x}, {y}) is canvas, as the rail's column below its last fact is"
            );
        }
    }
}

#[test]
fn the_cursor_lands_on_the_cell_the_draft_begins_at() {
    // The panel narrowed, the words did not move: an empty draft's cursor is the
    // cell its first character takes, one column inside the left border, and a
    // typed one ends where the typing ends.
    let mut app = app();
    let (left, top) = find(&buffer(&mut app, 100, 24), '╭').expect("the composer's corner");
    assert_eq!(
        cursor(&mut app, 100, 24),
        Position::new(left + 1, top + 1),
        "an empty composer puts the cursor where the first character will go"
    );

    type_text(&mut app, "wh");
    let buf = buffer(&mut app, 100, 24);
    let (left, top) = find(&buf, '╭').expect("the composer's corner");
    assert_eq!(
        buf[(left + 1, top + 1)].symbol(),
        "w",
        "the draft starts one column inside the border, where it always did"
    );
    assert_eq!(
        cursor(&mut app, 100, 24),
        Position::new(left + 3, top + 1),
        "and the cursor sits after what is typed"
    );
}

#[test]
fn with_no_rail_column_the_composer_takes_the_row() {
    // Below a hundred columns the rail is a strip under the transcript rather
    // than a column beside it, so the body has no split here to hand the
    // composer: the panel is the terminal's, corner to corner.
    for width in [40u16, 90] {
        let mut app = app();
        let buf = buffer(&mut app, width, 24);
        let panel = composer_box(&buf);
        assert_eq!(
            panel.x, 0,
            "{width} columns: the panel opens on the terminal's first column"
        );
        assert_eq!(
            panel.right(),
            width,
            "{width} columns: and there is no rail column to leave beside it"
        );
    }
}

#[test]
fn the_footer_keeps_the_whole_width_of_the_terminal() {
    // A hint is a strip, not a panel: the footer runs to the terminal's own
    // right edge, under the rail's column, though the composer no longer does.
    let mut app = app();
    for turn in 1..=40 {
        app.note_info(format!("question {turn}"));
        app.event(Event::TextDelta {
            turn,
            text: format!("answer {turn}"),
        });
    }
    screen(&mut app, 100, 20);
    app.key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
    for row in 1..=3 {
        app.note_info(format!("arrived {row}"));
    }
    let rows = screen(&mut app, 100, 20);
    let footer = rows.last().expect("a footer");
    assert_eq!(
        footer.trim_end().chars().count(),
        100,
        "the badge sits against the terminal's last column: {footer:?}"
    );
    assert!(
        footer.ends_with("↓ 3 new rows"),
        "and it is the footer's own right edge, past where the composer ends: {footer:?}"
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
            // transcript is written on.
            if tier == theme::RAISED {
                chips += 1;
                crossable = true;
                continue;
            }
            assert_eq!(
                tier,
                theme::CANVAS,
                "the rail draws no fill, so the canvas the theme names is under it: ({x},{y})"
            );
        }
        assert_eq!(
            buf[((width - RAIL - 1) as u16, y)].bg,
            theme::CANVAS,
            "and it leaves the canvas the theme names between it and the words: ({y})"
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
