//! The composer's cursor: the terminal is told a column of cells, so a glyph
//! two cells wide moves the cursor two columns while the draft's own index
//! moves one.

use super::*;

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
