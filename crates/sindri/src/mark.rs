//! The mark: the cells a reader has marked, and the words they hold, as one span the
//! highlight is painted from and the copy is read out of, so the two cannot disagree
//! (§12, #50).
//!
//! A mark is two corners and nothing more — the cell a drag began on and the cell it was
//! last moved to — so a redraw keeps it, `Shift+↑/↓` grow it a whole row at a time, and
//! `Ctrl-Y` copies it again with no fresh selection. What a mark covers is a [`Span`]:
//! the marked cells, row by row, each carrying the glyphs those cells hold. That is the
//! whole point of this module. A highlight painted from one reading of a drag and a
//! clipboard filled from another is a picture that can lie about what a copy took, and
//! §12's "a drag marks exactly the cells the copy will take" is a rule about one value,
//! not two that agree by care.

/// One cell of the transcript band: a row counted from the band's top row and a column
/// counted from the frame's left edge — the coordinates a drag is spoken in, and the ones
/// a span's cells are painted at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The band row, from the top of the transcript's band.
    pub row: u16,
    /// The column, from the frame's left edge.
    pub column: u16,
}

impl Cell {
    /// The cell `row` rows down the band and `column` columns in from the left edge.
    pub fn new(row: u16, column: u16) -> Cell {
        Cell { row, column }
    }
}

/// A reader's mark: the two corners of a drag, the anchor the drag began on and the head
/// it was last moved to. A corner rather than a remembered picture, because the rows under
/// a mark are re-laid on a resize and re-scrolled on a scroll, and a mark that remembered
/// cells of a screen would point at whatever came to sit there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    /// The cell the drag began on, which stays put as the mark grows.
    anchor: Cell,
    /// The cell the drag was last moved to, which is the end that moves.
    head: Cell,
}

impl Mark {
    /// The mark a drag makes: the cell it began on and the cell it has been dragged to.
    pub fn drag(anchor: Cell, head: Cell) -> Mark {
        Mark { anchor, head }
    }

    /// The cell the drag began on.
    pub fn anchor(self) -> Cell {
        self.anchor
    }

    /// The cell the drag was last moved to.
    pub fn head(self) -> Cell {
        self.head
    }

    /// The same mark with its head one whole row down the band, or one row back up it:
    /// `Shift+↓` and `Shift+↑` grow a mark by rows and never by cells (§12, #50). The
    /// anchor stays where the drag began, so the mark grows away from it.
    pub fn grown(self, down: bool) -> Mark {
        let row = match down {
            true => self.head.row.saturating_add(1),
            false => self.head.row.saturating_sub(1),
        };
        Mark {
            head: Cell { row, ..self.head },
            ..self
        }
    }
}

/// The cells a mark covers, row by row, each with the glyphs those cells hold: what the
/// highlight paints and what `Ctrl-Y` hands out, which is why there is one of these and
/// not two readings of a drag (§12).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// The rows of it, top to bottom, and no row the mark covers nothing of.
    rows: Vec<Marked>,
}

/// One row of a span: the band row it lies on, the column its cells begin at, and the
/// glyphs those cells hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marked {
    /// The band row, from the top of the transcript's band.
    row: u16,
    /// The column of the first cell this row's part of the mark covers.
    left: u16,
    /// The glyphs those cells hold, in the order they are on the screen.
    glyphs: String,
}

impl Marked {
    /// The band row this is on.
    pub fn row(&self) -> u16 {
        self.row
    }

    /// The column the marked cells of this row begin at.
    pub fn left(&self) -> u16 {
        self.left
    }

    /// How many columns those cells take.
    pub fn width(&self) -> u16 {
        u16::try_from(self.glyphs.chars().count()).unwrap_or(u16::MAX)
    }

    /// The glyphs they hold.
    pub fn glyphs(&self) -> &str {
        &self.glyphs
    }
}

impl Span {
    /// The span `mark` makes over the rows a band shows, top to bottom, as the lines they
    /// are drawn as. A row the band never reached is no part of the span, and neither is a
    /// cell that holds nothing: a mark covers the glyphs a reader can see it cover (§12).
    pub fn of(mark: Mark, lines: &[String]) -> Span {
        let first = mark.anchor.row.min(mark.head.row);
        let last = mark.anchor.row.max(mark.head.row);
        let from = mark.anchor.column.min(mark.head.column);
        let to = mark.anchor.column.max(mark.head.column);
        let mut rows = Vec::new();
        for row in first..=last {
            let Some(line) = lines.get(usize::from(row)) else {
                continue;
            };
            if let Some(marked) = marked_row(row, line, from, to) {
                rows.push(marked);
            }
        }
        Span { rows }
    }

    /// Every row of it, top to bottom.
    pub fn rows(&self) -> &[Marked] {
        &self.rows
    }

    /// The words it holds: each row's glyphs, the rows joined by a line break — byte for
    /// byte what the highlight covers, which is what §12 asks of a copy.
    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(|marked| marked.glyphs.as_str())
            .collect::<Vec<&str>>()
            .join("\n")
    }

    /// Whether the span covers nothing at all, which is a drag that landed on blanks.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// The part of one row's line a mark between columns `from` and `to` covers, as the span's
/// row, or nothing when it covers no glyph of it. The blanks at either end of what it keeps
/// are given up and the row begins at its first glyph: a cell that holds nothing is no part
/// of a mark, so a mark that covered one would be a highlight promising words it has not
/// got (§12).
fn marked_row(row: u16, line: &str, from: u16, to: u16) -> Option<Marked> {
    let mut kept: Vec<(u16, char)> = Vec::new();
    let mut column = 0u16;
    for glyph in line.chars() {
        let width = columns(glyph);
        let end = column.saturating_add(width);
        if end > from && column <= to {
            kept.push((column, glyph));
        }
        column = end;
    }
    let first = kept.iter().position(|(_, glyph)| !glyph.is_whitespace())?;
    let last = kept.iter().rposition(|(_, glyph)| !glyph.is_whitespace())?;
    Some(Marked {
        row,
        left: kept[first].0,
        glyphs: kept[first..=last].iter().map(|(_, glyph)| *glyph).collect(),
    })
}

/// The columns one glyph takes: a glyph can be two cells wide, and a band counts cells and
/// never glyphs, so a mark's cells are measured the way the row was laid out.
fn columns(glyph: char) -> u16 {
    let mut encoded = [0; 4];
    u16::try_from(crate::rows::glyphs(glyph.encode_utf8(&mut encoded))).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    //! What a mark covers, as a value: the cells of each row it reaches and the words
    //! those cells hold. The picture and the clipboard are held to each other on a
    //! painted frame in `tests/mark.rs`; this is the rule they both read.

    use super::*;

    /// The lines a band of rows is drawn as, from `rows`.
    fn band(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|row| String::from(*row)).collect()
    }

    #[test]
    fn a_drag_along_one_row_covers_the_glyphs_between_its_two_cells() {
        let lines = band(&["abcdefghij"]);

        let span = Span::of(Mark::drag(Cell::new(0, 2), Cell::new(0, 5)), &lines);

        assert_eq!(
            span.text(),
            "cdef",
            "the cells the drag began and ended on are in it"
        );
        assert_eq!(span.rows().len(), 1);
        assert_eq!(
            span.rows()[0].left(),
            2,
            "and the row begins where the drag did"
        );
    }

    #[test]
    fn a_drag_down_the_band_covers_every_row_between_its_two_cells() {
        let lines = band(&["abcd", "efgh", "ijkl", "mnop"]);

        let span = Span::of(Mark::drag(Cell::new(1, 0), Cell::new(2, 3)), &lines);

        assert_eq!(
            span.text(),
            "efgh\nijkl",
            "a drag from row one to row two takes both rows whole, and no other"
        );
    }

    #[test]
    fn a_column_dragged_over_cuts_every_row_of_the_span_at_the_same_two_columns() {
        let lines = band(&["0123456789", "abcdefghij"]);

        let span = Span::of(Mark::drag(Cell::new(0, 3), Cell::new(1, 5)), &lines);

        assert_eq!(
            span.text(),
            "345\ndef",
            "a drag is a block and not a snaking line"
        );
    }

    #[test]
    fn blanks_at_either_end_of_a_marked_row_are_no_part_of_it() {
        // An answer is set two columns in from the left edge, so a drag taken from the
        // edge itself covers the gutter: a cell that holds nothing is no part of a mark.
        let lines = band(&["  a gutter is not words"]);

        let span = Span::of(Mark::drag(Cell::new(0, 0), Cell::new(0, 40)), &lines);

        assert_eq!(span.text(), "a gutter is not words");
        assert_eq!(
            span.rows()[0].left(),
            2,
            "and the span begins at the first glyph"
        );
    }

    #[test]
    fn a_row_the_band_never_reached_is_no_part_of_the_span() {
        let lines = band(&["one row"]);

        let span = Span::of(Mark::drag(Cell::new(0, 0), Cell::new(9, 40)), &lines);

        assert_eq!(
            span.rows().len(),
            1,
            "a band of one row marks one row, however far the drag ran past it"
        );
    }

    #[test]
    fn a_drag_that_lands_on_nothing_covers_nothing() {
        let lines = band(&["   ", "hex"]);

        let span = Span::of(Mark::drag(Cell::new(0, 0), Cell::new(0, 3)), &lines);

        assert!(
            span.is_empty(),
            "a row of blanks is no part of a mark, so a drag over one covers nothing"
        );
        assert_eq!(span.text(), "");
    }
}
