//! The mark a drag leaves on the transcript: from press to release the cells
//! the drag covers are washed in one theme ink, the ink beneath them kept. One
//! function computes those spans, and the copy reads the same one, so the mark
//! can never light a cell the copy does not take nor leave dark a cell it does —
//! but for a picture's block, which is one region taken: its graphic's rows and
//! its caption are lit together, and its words are taken once.

use std::ops::Range;

use ratatui::layout::Position;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::selection::{Placed, Selection, Viewport, words};

/// One row a drag covers: which drawn row, the characters a copy takes of it,
/// and the cells the mark lights — both half-open, and both resting on the same
/// glyph boundary, so a wide glyph is copied and lit whole rather than in half.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The drawn row, counted from the first row of the transcript's view.
    pub row: usize,
    /// The characters of the row a copy takes, or `None` where the row is a
    /// picture's and not the first of its block: the block's words are taken
    /// once, on the row that holds them, and every other row of it is lit
    /// without being taken.
    pub chars: Option<Range<usize>>,
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
    let mut row = first;
    while row <= last {
        // A picture's block is one region: a drag that reaches any row of it
        // takes the block whole. Its words are taken once, on the row the block
        // starts at, however deep in its rows the drag began or ended; the rows
        // the window holds are all lit, because the reader who dragged into a
        // graphic meant the graphic and not the blank cells it is drawn on.
        if let Some(block) = viewport.block(row) {
            // The window's own end, and never behind the row the drag reached: a
            // block claims at least that row, so the walk always moves on.
            let stop = (block.at + block.rows)
                .min(viewport.rows.len())
                .max(row + 1);
            let cells = block_cells(viewport, block);
            for at in block.at..stop {
                let chars = (at == block.at).then(|| 0..words(viewport, at).chars().count());
                covered.push(Span {
                    row: at,
                    chars,
                    cells: cells.clone(),
                });
            }
            row = stop;
            continue;
        }
        let text = words(viewport, row);
        let from = if row == first {
            char_at(text, (start.x.saturating_sub(area.x)) as usize)
        } else {
            0
        };
        let to = if row != last || below {
            text.chars().count()
        } else {
            // The copy trims the blank cells a drag stopped short on, so the
            // mark gives them back too: nothing is lit that nothing is taken of.
            let mut to = char_after(text, (end.x.saturating_sub(area.x)) as usize);
            while to > from && text.chars().nth(to - 1).is_some_and(char::is_whitespace) {
                to -= 1;
            }
            to
        };
        // A row that yields no character still covers its span: the copy keeps
        // it as an empty line between the rows around it, and the mark, lighting
        // no cell, is honest about having nothing there to light.
        let (at, past) = (cell_of(text, from), cell_of(text, to));
        covered.push(Span {
            row,
            chars: Some(from..to),
            cells: at..past,
        });
        row += 1;
    }
    covered
}

/// The cells a picture's block spans: from the column its rows begin at — the
/// graphic's and the caption's are the same one, so the gutter left of them is
/// not the block's — to the wider of the words it copies as and the graphic
/// drawn over them, so the block is lit as one region rather than as a run of
/// its own text: a graphic's rows hold none, and a block half a column wide
/// would say nothing. The words are as wide as their widest line, a diagram's
/// copy being a fence body and so several.
fn block_cells(viewport: &Viewport, block: &Placed) -> Range<usize> {
    let at = usize::from(block.area.x.saturating_sub(viewport.area.x));
    let spoken = block
        .copy
        .lines()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0);
    let wide = spoken.max(usize::from(block.area.width));
    at..(at + wide).min(usize::from(viewport.area.width))
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
        assert_eq!((covered[0].row, &covered[0].chars), (0, &Some(2..5)));
        assert_eq!((covered[0].cells.start, covered[0].cells.end), (2, 5));
        assert_eq!((covered[1].row, &covered[1].chars), (1, &Some(0..4)));
        assert_eq!((covered[1].cells.start, covered[1].cells.end), (0, 4));
    }

    #[test]
    fn a_wide_glyph_at_an_edge_is_covered_by_both_of_its_cells() {
        let rows = viewport(&["ab日本"]);
        // The drag ends on the first cell of `日`: the glyph is taken whole, and
        // lit whole, so a mark never shows half a character.
        let covered = span(&rows, &drag((2, 2), (3, 2)));
        assert_eq!(covered.len(), 1);
        assert_eq!(covered[0].chars, Some(2..3));
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
        assert_eq!(covered[0].chars, Some(2..4));
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

    /// A frame with a picture in it: its graphic's two rows, its caption row
    /// under them, and the block's words on the row the block starts at — which
    /// is where the frame puts them, so a copy is the caption once.
    fn block() -> Viewport {
        let mut viewport = viewport(&["above", "a chart ‹plot.png›", "", "", "the next block"]);
        viewport.pictures.push(Placed {
            at: 1,
            rows: 3,
            area: Rect::new(2, 7, 10, 2),
            copy: "a chart ‹plot.png›".to_string(),
            worth: crate::picture::Worth::Refused(crate::picture::Refusal::Missing),
            drawn: true,
            reason: None,
        });
        viewport
    }

    #[test]
    fn a_drag_that_starts_deep_in_a_blocks_rows_covers_the_block_whole() {
        let covered = span(&block(), &drag((2, 5), (5, 5)));
        assert_eq!(
            covered.iter().map(|one| one.row).collect::<Vec<_>>(),
            vec![1, 2, 3],
            "the whole block, from its first row: a drag deep in a graphic is the \
             block and not the blank cells it is drawn on"
        );
        assert_eq!(
            covered[0].chars,
            Some(0..18),
            "and its words are taken once, on the row the block starts at"
        );
        assert!(
            covered[1..].iter().all(|one| one.chars.is_none()),
            "the rest of the block is lit and not taken: {covered:?}"
        );
        assert!(
            covered.iter().all(|one| one.cells == (2..20)),
            "one region taken: every row of the block is lit from the column it \
             begins at, across the wider of its words and its graphic: {covered:?}"
        );
    }

    #[test]
    fn a_block_the_window_holds_only_part_of_covers_the_rows_that_are_there() {
        // A block of six rows in a window that holds two of them: the frame lays
        // out the rows it holds and the rest are not there to light.
        let mut viewport = viewport(&["above", "", "", "", "", ""]);
        viewport.pictures.push(Placed {
            at: 4,
            rows: 6,
            area: Rect::new(2, 6, 10, 4),
            copy: "a chart ‹plot.png›".to_string(),
            worth: crate::picture::Worth::Refused(crate::picture::Refusal::Missing),
            drawn: false,
            reason: None,
        });
        let covered = span(&viewport, &drag((2, 6), (2, 9)));
        assert_eq!(
            covered.iter().map(|one| one.row).collect::<Vec<_>>(),
            vec![4, 5],
            "the rows of the block the window holds, and no row beyond it"
        );
    }

    #[test]
    fn a_diagrams_block_is_as_wide_as_its_widest_line_and_not_as_tall_as_their_sum() {
        // A diagram's copy is a fence body: several lines, so the block is as
        // wide as the longest of them rather than as wide as all of them.
        let mut viewport = block();
        viewport.pictures[0].copy = "stateDiagram-v2\n    [*] --> Resting".to_string();
        let covered = span(&viewport, &drag((2, 3), (2, 4)));
        assert!(
            covered.iter().all(|one| one.cells == (2..21)),
            "the widest line and the graphic, not the lines added together: {covered:?}"
        );
        assert_eq!(
            covered[0].chars,
            Some(0..35),
            "and the whole source is what the block is taken as"
        );
    }

    #[test]
    fn a_drag_that_starts_above_a_block_and_ends_inside_it_covers_it_whole() {
        let covered = span(&block(), &drag((3, 2), (5, 4)));
        assert_eq!(
            covered.iter().map(|one| one.row).collect::<Vec<_>>(),
            vec![0, 1, 2, 3],
            "the rows above the block, and then the block whole: the drag ended in \
             its second row and the block is still one region"
        );
        assert_eq!(covered[0].chars, Some(3..5), "the row the drag started on");
        assert_eq!(
            covered[1].chars,
            Some(0..18),
            "then the block's own words, untrimmed by where the drag stopped"
        );
        assert_eq!(
            covered[1..]
                .iter()
                .filter(|one| one.chars.is_some())
                .count(),
            1,
            "and no other row of the block is taken"
        );
    }
}
