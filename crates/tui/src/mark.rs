//! The mark a drag leaves on the transcript: from press to release the cells
//! the drag covers are washed in one theme ink, the ink beneath them kept. One
//! function computes those spans, and the copy reads the same one, so the mark
//! can never light a cell the copy does not take nor leave dark a cell it does.

use std::ops::Range;

use ratatui::layout::Position;
use unicode_width::UnicodeWidthChar;

use crate::selection::{Selection, Viewport};

/// One row a drag covers: which drawn row, the characters a copy takes of it,
/// and the cells the mark lights — both half-open, and both resting on the same
/// glyph boundary, so a wide glyph is copied and lit whole rather than in half.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The drawn row, counted from the first row of the transcript's view.
    pub row: usize,
    /// The characters of the row a copy takes.
    pub chars: Range<usize>,
    /// The cells of the row the mark lights, in the row's own columns.
    pub cells: Range<usize>,
}

/// The rows a drag covers: the first from the column the drag started on, the
/// last up to the column it ended on, and every row between in full — pulled in
/// to whole characters, because a cell is not a character. The one computation
/// of what a drag reached: [`crate::selection::copied`] slices the rows with it
/// and [`crate::render::mark`] washes the cells it names, so the mark and the
/// copy cannot disagree.
pub fn span(viewport: &Viewport, selection: &Selection) -> Vec<Span> {
    if selection.is_empty() || viewport.rows.is_empty() {
        return Vec::new();
    }
    let area = viewport.area;
    let (start, end) = ordered(selection.anchor, selection.head);
    let (Some(first), Some(last)) = (row_in(viewport, start), row_in(viewport, end)) else {
        return Vec::new();
    };
    let below = end.y >= area.y + viewport.rows.len() as u16;
    let mut covered = Vec::new();
    for row in first..=last {
        // A picture's block copies as its words once, on its first row: the
        // rows past it are a graphic's and hold nothing to take, so a mark over
        // them would light cells the copy leaves behind.
        if row != first && viewport.filler(row).is_some() {
            continue;
        }
        let words = crate::selection::words(viewport, row);
        let from = if row == first {
            char_at(words, (start.x.saturating_sub(area.x)) as usize)
        } else {
            0
        };
        let to = if row != last || below {
            words.chars().count()
        } else {
            // The copy trims the blank cells a drag stopped short on, so the
            // mark gives them back too: nothing is lit that nothing is taken of.
            let mut to = char_after(words, (end.x.saturating_sub(area.x)) as usize);
            while to > from && words.chars().nth(to - 1).is_some_and(char::is_whitespace) {
                to -= 1;
            }
            to
        };
        // A row that yields no character still covers its span: the copy keeps
        // it as an empty line between the rows around it, and the mark, lighting
        // no cell, is honest about having nothing there to light.
        let (at, past) = (cell_of(words, from), cell_of(words, to));
        covered.push(Span {
            row,
            chars: from..to,
            cells: at..past,
        });
    }
    covered
}

/// The character a screen column falls on. A cell is not a character: a wide
/// glyph takes two, and a selection takes it whole rather than half of it — a
/// drag that starts on a wide glyph's second cell starts at that glyph, not
/// past it, so neither the copy nor the mark can drop a character the reader
/// pressed on.
fn char_at(row: &str, column: usize) -> usize {
    let mut used = 0;
    for (index, ch) in row.chars().enumerate() {
        used += UnicodeWidthChar::width(ch).unwrap_or(0);
        if used > column {
            return index;
        }
    }
    row.chars().count()
}

/// The character just past the one covering `column`, so a selection that
/// ends on a wide glyph includes all of it.
fn char_after(row: &str, column: usize) -> usize {
    let mut used = 0;
    for (index, ch) in row.chars().enumerate() {
        used += UnicodeWidthChar::width(ch).unwrap_or(0);
        if used > column {
            return index + 1;
        }
    }
    row.chars().count()
}

/// The cell a character starts on: a wide glyph's second cell is its first, so
/// the mark begins where the glyph does and not halfway through it.
fn cell_of(row: &str, index: usize) -> usize {
    row.chars()
        .take(index)
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

fn ordered(one: Position, other: Position) -> (Position, Position) {
    if (one.y, one.x) <= (other.y, other.x) {
        (one, other)
    } else {
        (other, one)
    }
}

fn row_in(viewport: &Viewport, at: Position) -> Option<usize> {
    let row = at.y.checked_sub(viewport.area.y)? as usize;
    let last = viewport.rows.len().checked_sub(1)?;
    Some(row.min(last))
}

#[cfg(test)]
mod tests {
    use super::span;
    use crate::selection::{Placed, Selection, Viewport};
    use ratatui::layout::{Position, Rect};

    fn viewport(rows: &[&str]) -> Viewport {
        Viewport {
            area: Rect::new(0, 2, 60, rows.len() as u16),
            rows: rows.iter().map(|row| row.to_string()).collect(),
            spill: Vec::new(),
            expand: Vec::new(),
            pictures: Vec::new(),
        }
    }

    fn drag(from: (u16, u16), to: (u16, u16)) -> Selection {
        let mut selection = Selection::start(Position::new(from.0, from.1));
        selection.head = Position::new(to.0, to.1);
        selection.finish();
        selection
    }

    #[test]
    fn a_drag_covers_the_rows_the_copy_takes_and_the_cells_each_row_holds() {
        let rows = viewport(&["alpha", "bravo", "charlie"]);
        let covered = span(&rows, &drag((2, 2), (3, 3)));
        assert_eq!(
            covered.len(),
            2,
            "the rows between the press and the release: {covered:?}"
        );
        assert_eq!((covered[0].row, &covered[0].chars), (0, &(2..5)));
        assert_eq!((covered[0].cells.start, covered[0].cells.end), (2, 5));
        assert_eq!((covered[1].row, &covered[1].chars), (1, &(0..4)));
        assert_eq!((covered[1].cells.start, covered[1].cells.end), (0, 4));
    }

    #[test]
    fn a_wide_glyph_at_an_edge_is_covered_by_both_of_its_cells() {
        let rows = viewport(&["ab日本"]);
        // The drag ends on the first cell of `日`: the glyph is taken whole, and
        // lit whole, so a mark never shows half a character.
        let covered = span(&rows, &drag((2, 2), (3, 2)));
        assert_eq!(covered.len(), 1);
        assert_eq!(covered[0].chars, 2..3);
        assert_eq!(
            (covered[0].cells.start, covered[0].cells.end),
            (2, 4),
            "the wide glyph's two cells, not one"
        );
    }

    #[test]
    fn a_drag_that_starts_on_a_wide_glyphs_second_cell_still_covers_it_whole() {
        let rows = viewport(&["ab日本"]);
        // The press is on the second cell of `日`: the glyph is covered whole,
        // and lit from its own first cell rather than from halfway through it.
        let covered = span(&rows, &drag((3, 2), (4, 2)));
        assert_eq!(covered.len(), 1);
        assert_eq!(covered[0].chars, 2..4);
        assert_eq!(
            (covered[0].cells.start, covered[0].cells.end),
            (2, 6),
            "both glyphs from the cell `日` begins on"
        );
    }

    #[test]
    fn a_click_covers_no_cell_at_all() {
        assert!(span(&viewport(&["alpha"]), &drag((3, 2), (3, 2))).is_empty());
    }

    #[test]
    fn a_pictures_rows_after_its_first_are_covered_by_nothing() {
        let mut rows = viewport(&["a chart ‹plot.png›", "", "", "after"]);
        rows.pictures.push(Placed {
            at: 0,
            rows: 3,
            area: Rect::new(0, 2, 13, 2),
            copy: "a chart ‹plot.png›".to_string(),
            worth: crate::picture::Worth::Refused(crate::picture::Refusal::Missing),
            drawn: true,
            reason: None,
        });
        let covered = span(&rows, &drag((0, 2), (60, 5)));
        assert_eq!(
            covered.iter().map(|one| one.row).collect::<Vec<_>>(),
            vec![0, 3],
            "a block's blank rows are nothing to copy, so nothing is lit there"
        );
    }
}
