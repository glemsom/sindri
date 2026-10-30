//! The glow in the margin: the seam's light strip (#62) drawn in the one column
//! between the words and the rail, on a terminal that speaks Kitty graphics
//! (#63). With no answer from the terminal there is no glow and the frame is
//! the frame it is without this ticket.

use super::*;

use std::path::{Path, PathBuf};

use ratatui::buffer::Buffer;
use ratatui::style::Color;
use sindri_tui::graphics::Graphics;

/// A frame whose terminal speaks Kitty graphics, and so draws the glow.
fn lit() -> App {
    App::new(launch(), None).with_graphics(Some(Graphics::kitty()))
}

/// The frame's own cells: what the widgets drew, which is what a golden frame
/// is made of. A backend's buffer is the diff from the frame before it, and a
/// picture's escape sequence is longer than the cell it fills, so the diff
/// cannot answer for the cells beside one (#63).
fn cells(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    let mut drawn = Buffer::empty(Rect::new(0, 0, width, height));
    terminal
        .draw(|frame| {
            render::draw(app, frame);
            drawn = frame.buffer_mut().clone();
        })
        .expect("the frame draws");
    drawn
}

/// What one row of the frame's cells reads as, a picture as the mark the test
/// backend reads it as.
fn row(cells: &Buffer, y: u16) -> String {
    (0..cells.area.width)
        .map(|x| {
            let cell = &cells[(x, y)];
            if cell.symbol().contains('\u{1b}') {
                "\u{25A9}".to_string()
            } else {
                cell.symbol().to_string()
            }
        })
        .collect()
}

/// The margins of a frame's rows, top first.
fn margins(cells: &Buffer) -> Vec<char> {
    (0..cells.area.height)
        .map(|y| row(cells, y).chars().nth(MARGIN).unwrap_or(' '))
        .collect()
}

/// The first row the composer's border opens on: the body ends above it.
fn composer_at(rows: &[String]) -> usize {
    rows.iter()
        .position(|row| row.starts_with('╭'))
        .unwrap_or_else(|| panic!("the composer is drawn: {rows:#?}"))
}

#[test]
fn the_glow_is_the_margins_cells_at_the_height_of_the_body() {
    // The strip has a rect and it is the whole margin: from the top of the body
    // down to the row above the composer, and not one row further. It is two
    // cells wide, and the second is covered by the picture rather than painted
    // — the cell `Buffer::diff` would otherwise leave unpainted, taken from the
    // margin so the rail's first column keeps its own ink (`render::GAP_COLUMNS`).
    let drawn = cells(&mut lit(), 100, 30);
    let shown: Vec<String> = (0..drawn.area.height).map(|y| row(&drawn, y)).collect();
    let body = composer_at(&shown);
    let column = margins(&drawn);
    assert_eq!(
        column[..body].iter().collect::<String>(),
        "\u{25A9}".repeat(body),
        "every row of the body's margin carries the strip: {shown:#?}"
    );
    assert!(
        column[body..].iter().all(|cell| *cell != '\u{25A9}'),
        "and nothing below it: the composer's own rows are untouched: {shown:#?}"
    );
    let beside = MARGIN + 1;
    for y in 0..body {
        let cell = &drawn[(beside as u16, y as u16)];
        assert!(cell.skip, "({beside}, {y}) is the strip's own second cell");
        assert_eq!(cell.symbol(), " ", "covered, so it paints no glyph");
    }
}

#[test]
fn the_glow_touches_the_margin_and_nothing_else() {
    // What the strip may not change: the transcript, the rail's own area, the
    // composer and the footer. A frame with a glow is the frame without one
    // everywhere but the cells the margin was given.
    let plain = cells(&mut app(), 100, 30);
    let shown: Vec<String> = (0..plain.area.height).map(|y| row(&plain, y)).collect();
    let body = composer_at(&shown);
    let glowing = cells(&mut lit(), 100, 30);
    let gap = MARGIN..MARGIN + render::GAP_COLUMNS as usize;
    for y in 0..plain.area.height {
        for x in 0..plain.area.width {
            if gap.contains(&(x as usize)) && (y as usize) < body {
                continue;
            }
            let (left, right) = (&plain[(x, y)], &glowing[(x, y)]);
            assert_eq!(
                (left.symbol(), left.fg, left.bg),
                (right.symbol(), right.fg, right.bg),
                "({x}, {y}) is outside the body's margin, and it differs"
            );
        }
    }
}

#[test]
fn with_no_answer_from_the_terminal_the_margin_is_the_canvas_it_was() {
    // No graphics value, no glow: the margin is the terminal's own black from
    // the top of the body down to the composer, and a frame without this
    // ticket is the frame this one draws (#63).
    let plain = cells(&mut app(), 100, 30);
    let shown: Vec<String> = (0..plain.area.height).map(|y| row(&plain, y)).collect();
    for y in 0..composer_at(&shown) {
        let cell = &plain[(MARGIN as u16, y as u16)];
        assert_eq!(cell.symbol(), " ", "({MARGIN}, {y}) is canvas");
        assert_eq!(cell.fg, Color::Reset, "with no ink of its own");
        assert_eq!(cell.bg, Color::Reset, "and no background");
    }
}

#[test]
fn the_margin_keeps_the_canvas_and_spends_no_colour_of_its_own() {
    // The strip is a picture, not paint: its cell carries no foreground and no
    // background, so the terminal's own black still shows through and
    // `tests/theme.rs` has nothing to refuse.
    let glowing = cells(&mut lit(), 100, 30);
    let shown: Vec<String> = (0..glowing.area.height).map(|y| row(&glowing, y)).collect();
    for y in 0..composer_at(&shown) {
        let cell = &glowing[(MARGIN as u16, y as u16)];
        assert_eq!(cell.fg, Color::Reset, "({MARGIN}, {y}) paints no ink");
        assert_eq!(cell.bg, Color::Reset, "nor a background");
    }
}

#[test]
fn the_facts_start_where_they_start_without_a_glow() {
    // The seam is additive: the rail's columns are the rail's whether or not
    // the margin is lit, and the first fact begins at the start of the rail.
    let plain = cells(&mut app(), 100, 30);
    let glowing = cells(&mut lit(), 100, 30);
    let plain: Vec<String> = (0..plain.area.height).map(|y| row(&plain, y)).collect();
    let glowing: Vec<String> = (0..glowing.area.height).map(|y| row(&glowing, y)).collect();
    for (plain, glowing) in plain.iter().zip(&glowing) {
        assert_eq!(
            rail_of(plain),
            rail_of(glowing),
            "the rail is the same rail on a terminal that draws a picture"
        );
        assert_eq!(
            plain.chars().take(MARGIN).collect::<String>(),
            glowing.chars().take(MARGIN).collect::<String>(),
            "and the words are the same words"
        );
    }
}

#[test]
fn the_face_and_the_seam_reach_a_terminal_that_draws_the_glow() {
    // A picture's escape is written into one cell and read as wider than it, so
    // ratatui leaves the cell after it unpainted — a cell, not a column of ink,
    // and the terminal keeps whatever it last had there. The margin is two cells
    // wide so that cell is the strip's own, and the rail's first column, the one
    // the face's escape and its first fact both sit in, reaches the terminal. These rows
    // are a backend's: the diff a terminal is sent (#63).
    let face = Face::kitty().expect("the embedded face decodes");
    let mut app = App::new(launch(), Some(face)).with_graphics(Some(Graphics::kitty()));
    let rows = screen(&mut app, 100, 40);
    assert!(
        rail_of(&rows[0]).starts_with('\u{25A9}'),
        "the face is drawn in the rail's top beside the glow: {:?}",
        rail_of(&rows[0])
    );
    let ledger = rails(&rows, 100);
    for label in ["cache", "model", "workspace", "branch"] {
        assert!(
            fact(&ledger, label).starts_with(label),
            "the rail still states {label:?} at its first column beside a glow: {rows:#?}"
        );
    }
}

#[test]
fn a_click_in_the_margin_copies_nothing() {
    // The strip is drawn beside the rail, never inside it: a click in the
    // margin is not a click on a fact.
    let mut app = lit();
    let rows = screen(&mut app, 100, 30);
    let cache = rows
        .iter()
        .position(|row| rail_of(row).contains("cache"))
        .expect("the rail says what the run has spent");
    app.mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: MARGIN as u16,
        row: cache as u16,
        modifiers: crossterm::event::KeyModifiers::NONE,
    });
    assert!(
        app.selection().is_empty(),
        "the margin is not a rail row and not a transcript row: {:?}",
        app.selection()
    );
}

/// Every Rust source of the crate, so a rule about who asks the terminal can be
/// read off the tree rather than off a comment.
fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the crate's own source tree") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn the_terminal_is_asked_once_and_the_face_and_the_seam_both_borrow_it() {
    // `Picker::from_query_stdio` spends up to a second and reads stdin, so it
    // is asked in exactly one place and both users of a picture read that one
    // answer(#56, #63).
    let mut found = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    assert!(found.len() > 20, "the crate's source was walked: {found:?}");
    let mut asks = Vec::new();
    for path in &found {
        let source = std::fs::read_to_string(path).expect("a source file");
        let asks_here = source.matches("Picker::from_query_stdio()").count();
        if asks_here > 0 {
            asks.push((path.clone(), asks_here));
        }
    }
    assert_eq!(
        asks,
        vec![(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("graphics.rs"),
            1
        )],
        "one terminal, one question, asked once: {asks:?}"
    );

    // And one answer is enough for both: the frame draws the face and the glow
    // from the same `Graphics`, and neither asks a question of its own.
    let face = Face::kitty().expect("the embedded face decodes");
    let mut both = App::new(launch(), Some(face)).with_graphics(Some(Graphics::kitty()));
    let shown = screen(&mut both, 100, 40);
    assert!(
        shown.iter().take(4).any(|row| row.contains('\u{25A9}')),
        "the face is drawn in the rail's top: {shown:#?}"
    );
    assert!(
        margins(&cells(&mut both, 100, 40))[0] == '\u{25A9}',
        "and the margin is lit beside it, from the same answer: {shown:#?}"
    );
}
