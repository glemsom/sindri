//! The mark a drag leaves: the cells it lights, read back out of the drawn
//! frame rather than trusted from the arithmetic — lit exactly where the drag
//! reached, dark where it did not, and the ink underneath still its own.

use super::*;

use std::sync::{Arc, Mutex};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_tui::copy::{Clipboard, Copied};
use sindri_tui::theme;

/// A clipboard that only records: what a drag takes is asserted on, and the
/// test never touches the user's own.
#[derive(Clone, Default)]
struct Taking(Arc<Mutex<Vec<String>>>);

impl Taking {
    fn saw(&self) -> Vec<String> {
        self.0.lock().map(|seen| seen.clone()).unwrap_or_default()
    }
}

impl Clipboard for Taking {
    fn copy(&self, text: &str) -> Copied {
        if let Ok(mut seen) = self.0.lock() {
            seen.push(text.to_string());
        }
        Copied::Sent
    }
}

fn answered(clipboard: Taking, text: &str) -> App {
    let mut app = App::with_clipboard(launch(), None, Box::new(clipboard));
    app.note_info("a question");
    app.event(Event::TextDelta {
        turn: 1,
        text: text.to_string(),
    });
    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        ..Finished::default()
    });
    app
}

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

/// The text the mark has washed, one string per drawn row — read from the
/// cells themselves, so a mark that paints the wrong rectangle cannot pass.
fn lit(cells: &Buffer, area: Rect) -> Vec<String> {
    (area.y..area.y + area.height)
        .map(|y| {
            (area.x..area.x + area.width)
                .filter(|x| cells[(*x, y)].bg == theme::MARK)
                .map(|x| cells[(x, y)].symbol())
                .collect::<String>()
        })
        .filter(|row| !row.is_empty())
        .collect()
}

/// Whether any cell of the frame is washed at all: a click leaves none.
fn washes(cells: &Buffer) -> usize {
    (0..cells.area.height)
        .flat_map(|y| (0..cells.area.width).map(move |x| (x, y)))
        .filter(|at| cells[*at].bg == theme::MARK)
        .count()
}

#[test]
fn a_drag_lights_exactly_the_cells_its_copy_takes() {
    let clipboard = Taking::default();
    let mut app = answered(
        clipboard.clone(),
        "alpha bravo charlie\n\ndelta echo foxtrot",
    );
    let rows = screen(&mut app, 100, 20);
    let first = row_of(&rows, "alpha bravo") as u16;
    let last = row_of(&rows, "delta echo") as u16;
    let quiet = buffer(&mut app, 100, 20);

    // Press on the `p` of alpha, drag to the `e` of echo: the first row from
    // the column the drag started on, the last up to where it ended.
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 4, first);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 7, last);
    let cells = buffer(&mut app, 100, 20);
    let washed = lit(&cells, app.viewport().area);
    assert_eq!(
        washed,
        vec!["pha bravo charlie".to_string(), "  delta".to_string()],
        "the mark lights the cells of the rows between the press and the release"
    );

    // The ink beneath survives the wash: a lit cell keeps the character and the
    // ink it was drawn in, and only its surface changes.
    let (x, y) = (4, first);
    assert_eq!(cells[(x, y)].symbol(), quiet[(x, y)].symbol());
    assert_eq!(
        cells[(x, y)].fg,
        quiet[(x, y)].fg,
        "the ink is the row's own"
    );
    assert_ne!(
        cells[(x, y)].bg,
        quiet[(x, y)].bg,
        "the surface is the wash"
    );
    assert_eq!(
        cells[(2, first)].bg,
        quiet[(2, first)].bg,
        "a cell before the drag's column is untouched"
    );
    assert_eq!(
        cells[(9, last)].bg,
        quiet[(9, last)].bg,
        "and a cell past where it ended is untouched"
    );

    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 7, last);
    let copied = clipboard.saw();
    assert_eq!(copied.len(), 1, "one drag is one copy: {copied:?}");
    // A blank row between the two has no cell to light and still copies as the
    // empty line it is: what is lit is what is taken, line for line.
    assert_eq!(
        copied[0]
            .lines()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>(),
        washed,
        "the mark lit exactly what the copy took, and nothing else: {copied:?}"
    );
    let after = buffer(&mut app, 100, 20);
    assert_eq!(
        washes(&after),
        0,
        "the selection is spent with the copy, so the wash goes with it"
    );
}

#[test]
fn a_click_lights_no_cell_at_all() {
    let clipboard = Taking::default();
    let mut app = answered(clipboard.clone(), "alpha bravo charlie");
    let rows = screen(&mut app, 100, 20);
    let row = row_of(&rows, "alpha bravo") as u16;
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 4, row);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 4, row);
    assert_eq!(
        washes(&buffer(&mut app, 100, 20)),
        0,
        "a click takes nothing, so it lights nothing"
    );
    assert!(
        clipboard.saw().is_empty(),
        "and there was nothing to copy: {:?}",
        clipboard.saw()
    );
}

#[test]
fn a_drag_over_a_fence_washes_it_and_its_ink_and_its_border_still_read() {
    let clipboard = Taking::default();
    let mut app = answered(clipboard.clone(), "```rust\nlet answer = 42;\n```");
    let rows = screen(&mut app, 100, 20);
    let fence = row_of(&rows, "let answer") as u16;
    let line = rows[fence as usize].clone();
    let edge = line
        .chars()
        .position(|ch| ch == '│')
        .unwrap_or_else(|| panic!("the panel has a border: {line:?}")) as u16;
    let quiet = buffer(&mut app, 100, 20);

    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        edge,
        fence,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        edge + 6,
        fence,
    );
    let cells = buffer(&mut app, 100, 20);
    for column in edge..=edge + 6 {
        assert_eq!(
            cells[(column, fence)].bg,
            theme::MARK,
            "column {column} of the fence is washed"
        );
    }
    assert_eq!(
        cells[(edge, fence)].symbol(),
        "│",
        "the panel's border is still a border under the wash"
    );
    for (column, wanted) in [(edge, "border"), (edge + 2, "keyword")] {
        assert_eq!(
            cells[(column, fence)].fg,
            quiet[(column, fence)].fg,
            "the {wanted} keeps the ink it was drawn in"
        );
    }

    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        edge + 6,
        fence,
    );
    let copied = clipboard.saw();
    assert!(
        copied.first().is_some_and(|copy| copy == "│ let a"),
        "and the copy is the row's text, border and all: {copied:?}"
    );
}

#[test]
fn a_wide_glyph_at_the_marks_edge_is_lit_whole() {
    let clipboard = Taking::default();
    let mut app = answered(clipboard.clone(), "ab日本");
    let rows = screen(&mut app, 100, 20);
    let row = row_of(&rows, "ab日") as u16;
    let column = rows[row as usize]
        .chars()
        .position(|ch| ch == '日')
        .unwrap_or_else(|| panic!("`日` is on screen: {rows:#?}")) as u16;
    // `日` occupies two cells: the drag runs from its first to its second.
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        column,
        row,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        column + 1,
        row,
    );
    let cells = frame_cells(&mut app, 100, 20);
    for column in column..column + 2 {
        assert_eq!(
            cells[(column, row)].bg,
            theme::MARK,
            "cell {column} of the wide glyph is washed with the rest of it"
        );
    }
    assert_eq!(
        cells[(column - 1, row)].bg,
        theme::CANVAS,
        "and the cell before it is not"
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        column + 1,
        row,
    );
    assert_eq!(
        clipboard.saw(),
        vec!["日".to_string()],
        "the glyph is copied whole, as it is lit whole"
    );
}

#[test]
fn a_drag_that_starts_on_a_wide_glyphs_second_cell_lights_it_whole() {
    let clipboard = Taking::default();
    let mut app = answered(clipboard.clone(), "ab日本");
    let rows = screen(&mut app, 100, 20);
    let row = row_of(&rows, "ab日") as u16;
    let column = rows[row as usize]
        .chars()
        .position(|ch| ch == '日')
        .unwrap_or_else(|| panic!("`日` is on screen: {rows:#?}")) as u16;
    // The press lands on `日`'s second cell: the glyph is still the glyph, so
    // the mark begins at the cell it begins on rather than past it.
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        column + 1,
        row,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        column + 3,
        row,
    );
    let cells = frame_cells(&mut app, 100, 20);
    for washed in column..column + 4 {
        assert_eq!(
            cells[(washed, row)].bg,
            theme::MARK,
            "cell {washed} — `日` from its own first cell, and `本` — is washed"
        );
    }
    assert_eq!(
        cells[(column - 1, row)].bg,
        theme::CANVAS,
        "and the cell before the glyph the press landed on is not"
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        column + 3,
        row,
    );
    assert_eq!(
        clipboard.saw(),
        vec!["日本".to_string()],
        "and the copy starts at the glyph the press landed on, whole"
    );
}
