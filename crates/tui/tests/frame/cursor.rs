//! The composer's cursor: the terminal is told a column of cells, so a glyph
//! two cells wide moves the cursor two columns while the draft's own index
//! moves one — and a draft taller than the panel's six rows scrolls under it,
//! so the row the cursor is drawn on is the draft's own.

use super::*;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Position;

#[test]
fn two_wide_glyphs_put_the_cursor_four_cells_in() {
    // The editor counts characters and the terminal draws cells: two wide
    // glyphs are two keystrokes and four columns, so the cursor lands in the
    // cell the next character would take rather than two cells inside what is
    // drawn.
    let mut app = app();
    let start = cursor(&mut app, 100, 24);
    type_text(&mut app, "漢字");
    assert_eq!(
        cursor(&mut app, 100, 24),
        Position::new(start.x + 4, start.y),
        "two wide glyphs leave the cursor four cells past the empty draft's"
    );
}

#[test]
fn a_draft_taller_than_the_panel_scrolls_so_the_cursor_is_on_its_own_row() {
    let mut app = app();
    app.paste(&draft(20));

    let rows = screen(&mut app, 100, 24);
    let at = cursor(&mut app, 100, 24);

    assert!(
        !any(&rows, "row 14"),
        "the rows above the window are not drawn: {rows:#?}"
    );
    assert!(
        any(&rows, "row 15"),
        "the six the panel has are the last six"
    );
    let line = &rows[at.y as usize];
    assert!(
        line.contains("row 20"),
        "the row the cursor is drawn on says the draft's last row: {line:?}"
    );
    assert_eq!(
        usize::from(at.x),
        "row 20".len() + 1,
        "and the cursor is in the cell past its last character, one column of border in: {line:?}"
    );
}

#[test]
fn a_cursor_walked_up_a_long_draft_takes_the_panels_six_rows_with_it() {
    let mut app = app();
    app.paste(&draft(20));
    for _ in 0..10 {
        press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    }

    let rows = screen(&mut app, 100, 24);
    let at = cursor(&mut app, 100, 24);

    let line = &rows[at.y as usize];
    assert!(
        line.contains("row 10"),
        "the cursor is drawn on the tenth row it walked to: {line:?}"
    );
    assert!(
        !any(&rows, "row 09"),
        "the panel's window has moved down to the cursor: {rows:#?}"
    );
    assert!(
        any(&rows, "row 15") && !any(&rows, "row 16"),
        "and it still spends all six of its rows: {rows:#?}"
    );
}

#[test]
fn a_draft_that_fits_the_panel_keeps_the_cursor_on_its_last_row() {
    let mut app = app();
    app.paste("row one\nrow two\nrow three");

    let rows = screen(&mut app, 100, 24);
    let at = cursor(&mut app, 100, 24);

    assert!(
        any(&rows, "row one"),
        "every row of a draft the panel holds is drawn: {rows:#?}"
    );
    let line = &rows[at.y as usize];
    assert!(
        line.contains("row three"),
        "and the cursor is on the row it is on in the draft: {line:?}"
    );
}

#[test]
fn a_row_broken_at_the_top_of_a_scrolled_draft_is_drawn_under_the_row_it_broke() {
    let mut app = app();
    app.paste(&draft(20));
    for _ in 0..19 {
        press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);

    let rows = screen(&mut app, 100, 24);
    let at = cursor(&mut app, 100, 24);

    assert!(
        rows[at.y as usize - 1].contains("row 01"),
        "the row the break was made in is drawn above the cursor: {rows:#?}"
    );
    assert_eq!(
        at.x, 1,
        "and the cursor is at the head of the row the break made, one column of border in"
    );
    assert!(
        rows[at.y as usize]
            .chars()
            .nth(usize::from(at.x))
            .is_some_and(|cell| cell == ' '),
        "the row it is on is the empty one the break made: {:?}",
        rows[at.y as usize]
    );
}

#[test]
fn a_terminal_too_short_for_the_panel_draws_the_cursor_inside_it() {
    let mut app = app();
    app.paste(&draft(20));

    let rows = screen(&mut app, 100, 9);
    let at = cursor(&mut app, 100, 9);

    let line = &rows[at.y as usize];
    assert!(
        line.starts_with('│') && line.trim_end().ends_with('│'),
        "nine rows cannot hold the panel's eight, so the cursor is held on a row of the panel's own: {line:?}",
    );
}
