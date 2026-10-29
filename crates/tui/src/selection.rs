//! Selecting rendered rows with the mouse, and the text that leaves with them:
//! exactly the rows the frame drew, with every style dropped (ADR-0011 §7).
//!
//! The selection is a pair of screen cells, resolved against the rows the last
//! frame drew, so a copy is always of what the user saw — not of the model's
//! markdown source, and not of a style run a terminal would have to interpret.

use std::path::PathBuf;

use ratatui::layout::{Position, Rect};

/// What the transcript column last drew: its rectangle, the plain text of each
/// visible row, and the row that offers a spill file.
#[derive(Debug, Clone, Default)]
pub struct Viewport {
    /// Where the column was on screen.
    pub area: Rect,
    /// One entry per visible row, ANSI stripped.
    pub rows: Vec<String>,
    /// The row holding the spill affordance, with the file it offers.
    pub spill: Option<(usize, PathBuf)>,
}

impl Viewport {
    /// Whether a screen cell is inside the column.
    pub fn holds(&self, at: Position) -> bool {
        at.x >= self.area.x
            && at.x < self.area.x + self.area.width
            && at.y >= self.area.y
            && at.y < self.area.y + self.area.height
    }

    /// Which row of the column a screen row is, when it is one.
    pub fn row_of(&self, at: Position) -> Option<usize> {
        self.holds(at).then(|| (at.y - self.area.y) as usize)
    }
}

/// A selection being dragged: where the drag began and where it is now, in
/// screen cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    /// The cell the drag started on.
    pub anchor: Position,
    /// The cell the pointer is on now.
    pub head: Position,
    /// Whether the button is still down.
    pub dragging: bool,
}

impl Selection {
    /// A selection that begins where the button went down.
    pub fn start(at: Position) -> Self {
        Selection {
            anchor: at,
            head: at,
            dragging: true,
        }
    }

    /// The button came up: the selection is what it is.
    pub fn finish(&mut self) {
        self.dragging = false;
    }

    /// Whether anything is selected: a click is not a drag.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// The text a selection takes: the rows it covers, the first from the column
/// the drag started on, the last up to the column it ended on, and every row
/// between in full. Rows are joined with one newline each, and nothing else is
/// added — a copy is not a paraphrase.
pub fn copied(viewport: &Viewport, selection: &Selection) -> Option<String> {
    if selection.is_empty() || viewport.rows.is_empty() {
        return None;
    }
    let (start, end) = ordered(selection.anchor, selection.head);
    let area = viewport.area;
    let first = row_in(viewport, start)?;
    let last = row_in(viewport, end)?;
    // A drag that runs off the bottom of the column takes its last row whole.
    let below = end.y >= area.y + viewport.rows.len() as u16;
    let mut lines = Vec::new();
    for index in first..=last {
        let row = &viewport.rows[index];
        let from = if index == first {
            (start.x.saturating_sub(area.x)) as usize
        } else {
            0
        };
        let to = if index != last || below {
            row.chars().count()
        } else {
            (end.x.saturating_sub(area.x)) as usize + 1
        };
        let taken: String = row
            .chars()
            .skip(from)
            .take(to.saturating_sub(from))
            .collect::<String>()
            .trim_end()
            .to_string();
        if !taken.is_empty() || index != first {
            lines.push(taken);
        }
    }
    let text = lines.join("\n").trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Two cells, in reading order.
fn ordered(one: Position, other: Position) -> (Position, Position) {
    if (one.y, one.x) <= (other.y, other.x) {
        (one, other)
    } else {
        (other, one)
    }
}

/// The row a cell is on, clamped to the rows that were drawn: a drag that runs
/// off the bottom of the column stops at its last row rather than selecting
/// nothing.
fn row_in(viewport: &Viewport, at: Position) -> Option<usize> {
    let row = at.y.checked_sub(viewport.area.y)? as usize;
    let last = viewport.rows.len().checked_sub(1)?;
    Some(row.min(last))
}

#[cfg(test)]
mod tests {
    use super::{Selection, Viewport, copied};
    use ratatui::layout::{Position, Rect};

    fn viewport(rows: &[&str]) -> Viewport {
        Viewport {
            area: Rect::new(0, 2, 60, rows.len() as u16),
            rows: rows.iter().map(|row| row.to_string()).collect(),
            spill: None,
        }
    }

    fn drag(from: (u16, u16), to: (u16, u16)) -> Selection {
        let mut selection = Selection::start(Position::new(from.0, from.1));
        selection.head = Position::new(to.0, to.1);
        selection.finish();
        selection
    }

    #[test]
    fn a_drag_takes_exactly_the_rows_it_covers() {
        let rows = viewport(&["# Title", "hello world", "  indented"]);
        let taken = copied(&rows, &drag((0, 2), (60, 3)));
        assert_eq!(taken.as_deref(), Some("# Title\nhello world"));
    }

    #[test]
    fn the_first_and_last_rows_are_cut_at_the_columns_the_drag_reached() {
        let rows = viewport(&["alpha", "bravo", "charlie"]);
        let taken = copied(&rows, &drag((2, 2), (3, 3)));
        assert_eq!(taken.as_deref(), Some("pha\nbrav"));
    }

    #[test]
    fn a_drag_from_the_bottom_upwards_reads_the_same() {
        let rows = viewport(&["alpha", "bravo"]);
        let up = copied(&rows, &drag((0, 3), (1, 2)));
        let down = copied(&rows, &drag((1, 2), (0, 3)));
        assert_eq!(up, down, "a selection has no direction");
        assert_eq!(up.as_deref(), Some("lpha\nb"));
    }

    #[test]
    fn a_click_is_not_a_selection_and_a_drag_off_the_rows_is_clamped() {
        assert_eq!(copied(&viewport(&["alpha"]), &drag((3, 2), (3, 2))), None);
        let rows = viewport(&["alpha", "bravo"]);
        assert_eq!(
            copied(&rows, &drag((0, 2), (0, 40))).as_deref(),
            Some("alpha\nbravo"),
            "a drag past the last row stops at it"
        );
    }

    #[test]
    fn a_selection_is_the_characters_of_the_rows_and_no_byte_of_anything_else() {
        let rows = viewport(&["bold plain", "│ quoted"]);
        let taken = copied(&rows, &drag((0, 2), (20, 3))).unwrap_or_default();
        assert_eq!(taken, "bold plain\n│ quoted");
    }
}
