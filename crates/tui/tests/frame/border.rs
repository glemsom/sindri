//! The composer's border as a terminal that speaks Kitty graphics is sent it:
//! the picture its cells carry instead of `╭` and `─`, the words inside the
//! panel and the one word in the picture's gap, and the cursor. The assertions
//! are on cells and on the sequences those cells hold — the `a=T` that sends
//! the pixels and the `a=p` that re-places them — never on a screenshot.

use std::ops::Range;

use super::*;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use sindri_tui::graphics::Graphics;

/// The word the border carries, and the columns of the panel it takes: one
/// column in, where `Block::title` puts a left-aligned title.
const TITLE: &str = " prompt ";
const TITLE_COLUMNS: Range<u16> = 1..TITLE.len() as u16 + 1;

/// The mark a cell carrying a picture's escape reads as.
const DRAWN: &str = "\u{25A9}";

/// A frame whose terminal speaks Kitty graphics, and so draws its border as a
/// picture.
fn lit() -> App {
    App::new(launch(), None).with_graphics(Some(Graphics::kitty()))
}

/// A frame's own cells and the cursor it leaves on the terminal: what the
/// widgets drew, before `Buffer::diff` decides what to send. A picture's escape
/// is longer than the cell it fills, so a diff cannot answer for the cells
/// beside one.
fn frame_of(app: &mut App, width: u16, height: u16) -> (Buffer, Position) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    let mut drawn = Buffer::empty(Rect::new(0, 0, width, height));
    terminal
        .draw(|frame| {
            render::draw(app, frame);
            drawn = frame.buffer_mut().clone();
        })
        .expect("the frame draws");
    let cursor = terminal
        .get_cursor_position()
        .expect("the backend holds the cursor the frame was given");
    (drawn, cursor)
}

/// Types `text` into the composer, one key at a time: a draft is what the user
/// made, not something a test set behind their back.
fn typed(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(ch),
            crossterm::event::KeyModifiers::NONE,
        ));
    }
}

/// The composer's rect: the words' columns, from the row its top rule opens on
/// to its last row, which is the one above the footer the frame ends in. The
/// panel is the transcript's width, so at a hundred columns and the rail's 32
/// it ends where the margin begins, and a terminal too narrow for the rail's
/// column gives it the whole width.
fn panel(cells: &Buffer) -> Rect {
    let width = cells.area.width;
    let columns = if width >= render::RAIL_MIN_WIDTH {
        width - RAIL as u16 - render::GAP_COLUMNS
    } else {
        width
    };
    let height = cells.area.height;
    let top = (0..height)
        .find(|y| {
            let cell = &cells[(0, *y)];
            cell.symbol().contains('\u{1b}') || cell.symbol().starts_with('╭')
        })
        .unwrap_or_else(|| panic!("the composer is drawn: {:?}", rows(cells)));
    Rect {
        x: 0,
        y: top,
        width: columns,
        height: height - 1 - top,
    }
}

/// What one row of a frame reads as, a picture as the one mark the cell it
/// rides in stands for.
fn row(cells: &Buffer, y: u16) -> String {
    (0..cells.area.width)
        .map(|x| {
            let cell = &cells[(x, y)];
            if cell.symbol().contains('\u{1b}') {
                DRAWN.to_string()
            } else {
                cell.symbol().to_string()
            }
        })
        .collect()
}

fn rows(cells: &Buffer) -> Vec<String> {
    (0..cells.area.height).map(|y| row(cells, y)).collect()
}

/// The cells of the panel's ring: the two rules and the two verticals, which is
/// where the glyph frame draws its border.
fn ring(area: Rect) -> Vec<(u16, u16)> {
    let mut cells = Vec::new();
    for x in area.x..area.right() {
        cells.push((x, area.y));
        cells.push((x, area.bottom() - 1));
    }
    for y in area.y + 1..area.bottom() - 1 {
        cells.push((area.x, y));
        cells.push((area.right() - 1, y));
    }
    cells
}

fn inside(area: Rect, x: u16, y: u16) -> bool {
    x > area.x && x < area.right() - 1 && y > area.y && y < area.bottom() - 1
}

/// Whether a symbol is one of the glyphs a rounded frame is drawn with.
fn border_glyph(symbol: &str) -> bool {
    symbol
        .chars()
        .any(|ch| matches!(ch, '─' | '│' | '╭' | '╮' | '╰' | '╯'))
}

/// The panel's cells whose escape says `needle`: `a=T` sends the pixels, `a=p`
/// places an image the terminal already holds, and the label is never one.
fn says(cells: &Buffer, area: Rect, needle: &str) -> Vec<(u16, u16)> {
    (area.y..area.bottom())
        .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
        .filter(|(x, y)| cells[(*x, *y)].symbol().contains(needle))
        .collect()
}

/// The panels whose cells carry a transmission: the pixels themselves, in the
/// one cell the picture is anchored at.
fn picture(cells: &Buffer, area: Rect) -> Vec<(u16, u16)> {
    says(cells, area, "a=T,f=100")
}

/// The id the anchor cell's transmission carries: the image the terminal knows
/// this picture by, so two transmissions are told apart rather than counted.
fn transmitted_id(cells: &Buffer, area: Rect) -> String {
    let (x, y) = picture(cells, area)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("the picture is transmitted: {:?}", rows(cells)));
    let sequence = cells[(x, y)].symbol();
    let at = sequence
        .find(",i=")
        .unwrap_or_else(|| panic!("the transmission names its image: {sequence:?}"));
    sequence[at + 3..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect()
}

#[test]
fn the_borders_cells_carry_the_picture_and_no_box_drawing_glyph() {
    // On this terminal the ring is not drawn: the stroke is a picture placed
    // over the panel's whole rect, and its pixels say the border. What is left
    // in those cells is blank and marked skipped, so no glyph is painted over
    // the pixels — and the one cell the escape rides in, which is the panel's
    // own top-left and is sent, or the picture never arrives.
    let (cells, _) = frame_of(&mut lit(), 100, 30);
    let area = panel(&cells);
    assert_eq!(
        picture(&cells, area),
        vec![(area.x, area.y)],
        "one transmission, in the one cell the picture is anchored at: {:?}",
        rows(&cells)
    );
    for (x, y) in ring(area) {
        let cell = &cells[(x, y)];
        assert!(
            !border_glyph(cell.symbol()),
            "({x}, {y}) is a border cell and carries the picture, not a glyph: {:?}",
            cell.symbol()
        );
        if (x, y) == (area.x, area.y) {
            assert!(
                !cell.skip,
                "the cell the escape rides in is sent, or the picture never arrives"
            );
            continue;
        }
        if y == area.y && TITLE_COLUMNS.contains(&(x - area.x)) {
            continue;
        }
        assert!(
            cell.skip,
            "({x}, {y}) is covered by the picture, so no glyph is ever sent there"
        );
    }
}

#[test]
fn the_draft_inside_reads_as_it_does_beside_the_glyph_frame() {
    // A picture is a layer, not a cell: every cell inside the panel holds the
    // draft exactly as the frame that draws its border in glyphs holds it, and
    // every one of them is handed back to the terminal so it is sent.
    let mut plain = app();
    let mut kitty = lit();
    for app in [&mut plain, &mut kitty] {
        typed(app, "why does this test fail?");
    }
    let (plain_cells, _) = frame_of(&mut plain, 100, 30);
    let (kitty_cells, _) = frame_of(&mut kitty, 100, 30);
    let area = panel(&plain_cells);
    assert_eq!(panel(&kitty_cells), area, "the same panel, drawn two ways");
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if !inside(area, x, y) {
                continue;
            }
            let (left, right) = (&plain_cells[(x, y)], &kitty_cells[(x, y)]);
            assert_eq!(
                (left.symbol(), left.fg, left.bg),
                (right.symbol(), right.fg, right.bg),
                "({x}, {y}) is inside the composer, and the picture changes nothing there"
            );
            assert!(
                !right.skip,
                "({x}, {y}) is handed back as the draft's own cell"
            );
        }
    }
    assert!(
        row(&kitty_cells, area.y + 1).contains("why does this test fail?"),
        "and the draft is read through the picture: {:?}",
        rows(&kitty_cells)
    );
}

#[test]
fn the_prompt_stays_a_word_in_the_gap_at_the_column_the_glyph_frame_puts_it() {
    // The picture cuts its gap for exactly the cells `Block::title` writes the
    // word into, so the label is the glyph frame's own word on the glyph
    // frame's own columns — the picture changes the rule around it, not it.
    let (cells, _) = frame_of(&mut lit(), 100, 30);
    let area = panel(&cells);
    assert!(
        row(&cells, area.y).starts_with(&format!("{DRAWN}{TITLE}")),
        "the prompt is text in the picture's gap: {:?}",
        rows(&cells)
    );
    let (plain, _) = frame_of(&mut app(), 100, 30);
    assert_eq!(panel(&plain), area, "the two frames agree on the panel");
    for x in area.x + TITLE_COLUMNS.start..area.x + TITLE_COLUMNS.end {
        let painted = &cells[(x, area.y)];
        assert_eq!(
            painted,
            &plain[(x, area.y)],
            "({x}, {}) is the label, and a picture does not move or repaint it",
            area.y
        );
        assert!(!painted.skip, "({x}, {}) is handed back as text", area.y);
    }
}

#[test]
fn the_cursor_is_on_the_same_cell_with_and_without_the_picture() {
    // The inner rect is computed from the block on both paths, so the draft's
    // first glyph and the cursor after it land on the same cells; this draws
    // both frames and compares what the terminal is told.
    let mut plain = app();
    let mut kitty = lit();
    for app in [&mut plain, &mut kitty] {
        typed(app, "ask");
    }
    let (plain_cells, plain_cursor) = frame_of(&mut plain, 100, 30);
    let (kitty_cells, kitty_cursor) = frame_of(&mut kitty, 100, 30);
    assert_eq!(
        panel(&kitty_cells),
        panel(&plain_cells),
        "the same panel on both paths"
    );
    assert_eq!(
        kitty_cursor, plain_cursor,
        "a border that shifts the cursor by a column is worse than any glyph"
    );
    let area = panel(&plain_cells);
    assert_eq!(
        plain_cursor,
        Position::new(area.x + TITLE_COLUMNS.start + 3, area.y + 1),
        "the cursor is one column past the three letters, on the draft's row"
    );
}

#[test]
fn the_border_is_transmitted_once_and_a_ticking_frame_re_places_it() {
    // The shape of `the_strip_is_transmitted_once_and_a_ticking_frame_re_transmits_nothing`:
    // the pixels go out with the frame that first carries them, and a frame
    // that merely ticked sends the placement alone.
    let mut app = lit();
    let (first, _) = frame_of(&mut app, 100, 30);
    let area = panel(&first);
    assert_eq!(
        picture(&first, area).len(),
        1,
        "the picture goes out once, with the frame that first carries it: {:?}",
        rows(&first)
    );
    let (tick, _) = frame_of(&mut app, 100, 30);
    assert_eq!(panel(&tick), area, "a frame of the same size");
    assert!(
        picture(&tick, area).is_empty(),
        "carries no pixels again: {:?}",
        rows(&tick)
    );
    assert_eq!(
        says(&tick, area, "a=p").len(),
        1,
        "and still places the image the terminal holds, or the border is gone"
    );
}

#[test]
fn a_resize_makes_the_border_again() {
    // The panel is the width of the words, so a wider terminal is a wider
    // picture: the pixels were made for cells this terminal does not hold, and
    // they are sent once under an id of their own before the frame goes quiet.
    let mut app = lit();
    let (first, _) = frame_of(&mut app, 100, 30);
    let narrow = panel(&first);
    let (wider, _) = frame_of(&mut app, 120, 30);
    let area = panel(&wider);
    assert_eq!(
        area.width,
        narrow.width + 20,
        "the panel ends one column short of the seam, and the terminal grew by 20"
    );
    assert_eq!(
        picture(&wider, area).len(),
        1,
        "a composer of another size is a new picture, sent again: {:?}",
        rows(&wider)
    );
    assert_ne!(
        transmitted_id(&wider, area),
        transmitted_id(&first, narrow),
        "and it is a new image, not the one the terminal still holds"
    );
    let (tick, _) = frame_of(&mut app, 120, 30);
    assert!(
        picture(&tick, area).is_empty() && says(&tick, area, "a=p").len() == 1,
        "the picture it just sent is re-placed, never sent again: {:?}",
        rows(&tick)
    );
}
