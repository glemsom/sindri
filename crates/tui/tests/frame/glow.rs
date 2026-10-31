//! The glow in the margin: the seam's light strip drawn in the one column
//! between the words and the rail, on a terminal that speaks Kitty graphics.
//! With no answer from the terminal there is no glow, and the margin is the
//! plain canvas beside the words.

use super::*;

use std::path::{Path, PathBuf};

use ratatui::buffer::Buffer;
use sindri_tui::graphics::Graphics;
use sindri_tui::theme;

/// A frame whose terminal speaks Kitty graphics, and so draws the glow.
fn lit() -> App {
    App::new(launch(), None).with_graphics(Some(Graphics::kitty()))
}

/// The frame's own cells: what the widgets drew, which is what a golden frame
/// is made of. A backend's buffer is the diff from the frame before it, and a
/// picture's escape sequence is longer than the cell it fills, so the diff
/// cannot answer for the cells beside one.
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

/// The first row the composer's border opens on: the body ends above it. The
/// composer draws its frame as a picture here, so the row opens on the escape
/// that carries it rather than on a corner glyph.
fn composer_at(rows: &[String]) -> usize {
    rows.iter()
        .position(|row| row.starts_with('╭') || row.starts_with('\u{25A9}'))
        .unwrap_or_else(|| panic!("the composer is drawn: {rows:#?}"))
}

#[test]
fn the_glow_runs_down_beside_the_composer_and_dissolves_at_the_footer() {
    // The strip has a rect from the top of the body down to the composer's last
    // row: now that the panel ends at the seam, the margin beside it is canvas
    // for its whole height, so the light goes on beside the composer instead of
    // dying on the last row above its top rule. At thirty rows the body is
    // twenty-five and the composer four, so the strip is twenty-nine rows and
    // the footer is the one row it leaves.
    let drawn = cells(&mut lit(), 100, 30);
    let shown: Vec<String> = (0..drawn.area.height).map(|y| row(&drawn, y)).collect();
    let composer = composer_at(&shown);
    let footer = shown.len() - 1;
    assert_eq!(
        footer - composer,
        4,
        "the empty composer is two rows between its two rules: {shown:#?}"
    );
    let column = margins(&drawn);
    assert_eq!(
        column[..footer].iter().collect::<String>(),
        "\u{25A9}".repeat(footer),
        "every row from the top of the body to the composer's last carries the \
         strip: {shown:#?}"
    );
    assert_ne!(
        column[footer], '\u{25A9}',
        "the footer is the row the light dissolves on, not the panel's top rule: {shown:#?}"
    );
    let beside = MARGIN + 1;
    for y in 0..footer {
        let cell = &drawn[(beside as u16, y as u16)];
        assert!(cell.skip, "({beside}, {y}) is the strip's own second cell");
        assert_eq!(cell.symbol(), " ", "covered, so it paints no glyph");
    }
}

#[test]
fn the_strip_draws_no_cell_inside_the_composers_rect() {
    // The composer's own dwelling: its columns are 0..MARGIN, and the strip's
    // begin at MARGIN. Not one cell of the panel carries the strip's mark, on
    // any of its rows — the two rects are beside each other, never over.
    let drawn = cells(&mut lit(), 100, 30);
    let shown: Vec<String> = (0..drawn.area.height).map(|y| row(&drawn, y)).collect();
    let open = composer_at(&shown);
    let footer = shown.len() - 1;
    for y in open..footer {
        for x in 0..MARGIN {
            let cell = &drawn[(x as u16, y as u16)];
            assert!(
                !cell.symbol().contains('\u{10EEEE}'),
                "({x}, {y}) is the composer's and not the strip's"
            );
        }
    }
}

#[test]
fn the_rail_keeps_its_own_ground_on_every_row_the_strip_lights() {
    // The strip's brightest column is the one nearest the rail, so the rail's
    // first column must stay the rail's: its ink sits on the background the
    // theme's 4.5:1 is measured against, on the composer's rows as on the
    // body's. Nothing of the strip reaches a column from the rail's first
    // rightwards.
    let plain = cells(&mut app(), 100, 30);
    let glowing = cells(&mut lit(), 100, 30);
    let rail = MARGIN + render::GAP_COLUMNS as usize;
    for y in 0..plain.area.height {
        for x in rail..plain.area.width as usize {
            let (left, right) = (&plain[(x as u16, y)], &glowing[(x as u16, y)]);
            assert_eq!(
                (left.symbol(), left.fg, left.bg),
                (right.symbol(), right.fg, right.bg),
                "({x}, {y}) is the rail's, and a glow does not reach it"
            );
        }
    }
}

#[test]
fn the_glow_changes_the_margins_cells_and_nothing_but_the_composers_own() {
    // What the strip may not change: the transcript, the rail's own area and the
    // footer. A frame with a glow is the frame without one everywhere but the
    // cells the margin was given. The composer's own rows are left out: on this
    // terminal its border is a picture too — `render/composer.rs` holds that —
    // and it differs for a reason that is not the glow.
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
            if (body..plain.area.height as usize - 1).contains(&(y as usize)) {
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
    // No graphics value, no glow: the margin is the canvas the theme names from
    // the top of the body down to the composer, untouched by any picture, and
    // this frame draws what a frame that never asked for graphics draws.
    let plain = cells(&mut app(), 100, 30);
    let shown: Vec<String> = (0..plain.area.height).map(|y| row(&plain, y)).collect();
    for y in 0..composer_at(&shown) {
        let cell = &plain[(MARGIN as u16, y as u16)];
        assert_eq!(cell.symbol(), " ", "({MARGIN}, {y}) is canvas");
        assert_eq!(
            cell.fg,
            theme::PLAIN,
            "with the canvas's ink, which is the theme's and not the terminal's"
        );
        assert_eq!(
            cell.bg,
            theme::CANVAS,
            "and a background the theme names, not the terminal's"
        );
    }
}

#[test]
fn the_margin_keeps_the_canvas_and_spends_no_colour_of_its_own() {
    // The strip is a picture, not paint: its cell carries the canvas's ink and
    // none of its own, and what it is drawn over is the canvas the theme names —
    // the colour its edges antialias against is the theme's and not the
    // terminal's, and `tests/theme` has nothing to refuse. Its last row is the composer's last
    // row, so the margin is canvas beside the panel's every row as well as the
    // body's: the rect is sized from the panel and not from the terminal.
    let glowing = cells(&mut lit(), 100, 30);
    let shown: Vec<String> = (0..glowing.area.height).map(|y| row(&glowing, y)).collect();
    let footer = shown.len() - 1;
    for y in 0..footer {
        let cell = &glowing[(MARGIN as u16, y as u16)];
        assert_eq!(
            cell.fg,
            theme::PLAIN,
            "({MARGIN}, {y}) carries the canvas's ink, not the picture's"
        );
        assert_eq!(
            cell.bg,
            theme::CANVAS,
            "and sits on the canvas the theme names"
        );
    }
    assert_eq!(
        column_of(&shown, footer - 1),
        '\u{25A9}',
        "the strip is still a picture on the composer's last row: {shown:#?}"
    );
}

/// One row's margin, as the terminal reads it.
fn column_of(rows: &[String], y: usize) -> char {
    rows[y].chars().nth(MARGIN).unwrap_or(' ')
}

#[test]
fn the_rails_facts_and_the_words_sit_where_they_sit_beside_a_glow() {
    // The seam is additive: the rail's columns are the rail's whether or not
    // the margin is lit, and the first fact begins at the start of the rail. The
    // composer's rows are the composer's: its border is a picture on this
    // terminal, and the footer under it is not.
    let plain = cells(&mut app(), 100, 30);
    let glowing = cells(&mut lit(), 100, 30);
    let plain: Vec<String> = (0..plain.area.height).map(|y| row(&plain, y)).collect();
    let glowing: Vec<String> = (0..glowing.area.height).map(|y| row(&glowing, y)).collect();
    let body = composer_at(&plain);
    let footer = plain.len() - 1;
    for (y, (plain, glowing)) in plain.iter().zip(&glowing).enumerate() {
        if (body..footer).contains(&y) {
            continue;
        }
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
    // the face's escape and its first fact both sit in, reaches the terminal.
    // These rows are a backend's: the diff a terminal is sent.
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
        let row = fact(&ledger, label);
        let head: String = row.chars().take(render::RAIL_INSET).collect();
        let field: String = row.chars().skip(render::RAIL_INSET).take(LABEL).collect();
        assert!(
            head.trim().is_empty() && field.trim_end() == label,
            "the rail still states {label:?} inside its inset beside a glow: {rows:#?}"
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
    // answer.
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
