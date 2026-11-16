use super::*;

use crate::pen;

/// Rows of a band as the mark reads them: every glyph with the column it is drawn from and the
/// columns it covers.
fn band(rows: &[&str]) -> Vec<Vec<Glyph>> {
    rows.iter()
        .map(|row| {
            let mut column = 0u16;
            let mut line = Vec::new();
            for glyph in row.chars() {
                let width = pen::cell(glyph);
                line.push((column, glyph, width));
                column = column.saturating_add(width);
            }
            line
        })
        .collect()
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
fn a_mark_over_a_wide_glyph_is_as_wide_as_the_cells_the_glyph_occupies() {
    let lines = band(&["  日本語"]);

    let span = Span::of(Mark::drag(Cell::new(0, 2), Cell::new(0, 7)), &lines);

    assert_eq!(span.text(), "日本語");
    assert_eq!(
        span.rows()[0].left(),
        2,
        "the mark begins in the column the first wide glyph does"
    );
    assert_eq!(
        span.rows()[0].width(),
        6,
        "three wide glyphs cover six cells, and a highlight two cells short of the words \
             it says are marked is a lie about the copy"
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
