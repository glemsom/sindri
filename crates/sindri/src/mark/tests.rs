use super::*;
use crate::pen;

fn band(rows: &[&str]) -> Vec<Line> {
    rows.iter()
        .map(|row| {
            let mut column = 0;
            Line::of(
                row.chars()
                    .map(|glyph| {
                        let width = pen::cell(glyph);
                        let result = (column, glyph, width);
                        column += width;
                        result
                    })
                    .collect(),
                false,
            )
        })
        .collect()
}

#[test]
fn selection_is_linear_and_reversible() {
    let lines = band(&["0123456789", "  middle", "", "abcdefghij"]);
    let start = Cell::new(0, 3);
    let end = Cell::new(3, 5);
    for mark in [Mark::drag(start, end), Mark::drag(end, start)] {
        assert_eq!(Span::of(mark, &lines).text(), "3456789\n  middle\n\nabcdef");
    }
}

#[test]
fn same_row_selection_includes_endpoints_and_wide_glyphs() {
    let lines = band(&["abcdefghij", "  日本語"]);
    assert_eq!(
        Span::of(Mark::drag(Cell::new(0, 2), Cell::new(0, 5)), &lines).text(),
        "cdef"
    );
    let span = Span::of(Mark::drag(Cell::new(1, 3), Cell::new(1, 7)), &lines);
    assert_eq!(span.text(), "日本語");
    assert_eq!(span.rows()[0].width(), 6);
}

#[test]
fn indentation_and_blank_lines_are_text_not_discarded_padding() {
    let lines = band(&["  code", "", "    next"]);
    assert_eq!(
        Span::of(Mark::drag(Cell::new(0, 0), Cell::new(2, 20)), &lines).text(),
        "  code\n\n    next"
    );
    assert!(Span::of(Mark::drag(Cell::new(1, 0), Cell::new(1, 4)), &lines).is_empty());
}

#[test]
fn selection_never_extends_past_the_supplied_visible_rows_or_endpoint() {
    let lines = band(&["abcdefghij"]);
    assert_eq!(
        Span::of(Mark::drag(Cell::new(0, 2), Cell::new(0, 4)), &lines).text(),
        "cde"
    );
    assert_eq!(
        Span::of(Mark::drag(Cell::new(0, 2), Cell::new(9, 40)), &lines).text(),
        "cdefghij"
    );
}
