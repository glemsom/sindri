#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub row: u16,
    pub column: u16,
}

impl Cell {
    pub fn new(row: u16, column: u16) -> Cell {
        Cell { row, column }
    }
}

/// Two corners and not the cells they covered: a resize re-lays the band and a scroll
/// moves it, so a mark that remembered cells would point at whatever came to sit there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    anchor: Cell,
    head: Cell,
}

impl Mark {
    pub fn drag(anchor: Cell, head: Cell) -> Mark {
        Mark { anchor, head }
    }

    pub fn anchor(self) -> Cell {
        self.anchor
    }

    pub fn head(self) -> Cell {
        self.head
    }

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    rows: Vec<Marked>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marked {
    row: u16,
    left: u16,
    glyphs: String,
}

impl Marked {
    pub fn row(&self) -> u16 {
        self.row
    }

    pub fn left(&self) -> u16 {
        self.left
    }

    pub fn width(&self) -> u16 {
        u16::try_from(self.glyphs.chars().count()).unwrap_or(u16::MAX)
    }

    pub fn glyphs(&self) -> &str {
        &self.glyphs
    }
}

impl Span {
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

    pub fn rows(&self) -> &[Marked] {
        &self.rows
    }

    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(|marked| marked.glyphs.as_str())
            .collect::<Vec<&str>>()
            .join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

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

/// A glyph can be two cells wide and a band counts cells and never glyphs, so a mark's
/// cells are measured the way the row was laid out.
fn columns(glyph: char) -> u16 {
    let mut encoded = [0; 4];
    u16::try_from(crate::rows::glyphs(glyph.encode_utf8(&mut encoded))).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

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
