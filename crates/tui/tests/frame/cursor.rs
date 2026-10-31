//! The composer's cursor: the terminal is told a column of cells, so a glyph
//! two cells wide moves the cursor two columns while the draft's own index
//! moves one.

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Position;

fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
}

/// The cell the frame gives the cursor, as the terminal is told it.
fn cursor(app: &mut App) -> Position {
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(app, frame))
        .expect("the frame draws");
    terminal
        .get_cursor_position()
        .expect("the frame places the cursor")
}

#[test]
fn two_wide_glyphs_put_the_cursor_four_cells_in() {
    // The editor counts characters and the terminal draws cells: two wide
    // glyphs are two keystrokes and four columns, so the cursor lands in the
    // cell the next character would take rather than two cells inside what is
    // drawn.
    let mut app = app();
    let start = cursor(&mut app);
    typed(&mut app, "漢字");
    assert_eq!(
        cursor(&mut app),
        Position::new(start.x + 4, start.y),
        "two wide glyphs leave the cursor four cells past the empty draft's"
    );
}
