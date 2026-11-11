//! The pen: the one place a run of glyphs is written into a frame, and the one place a string's
//! width is measured — a wide glyph two cells wherever it is drawn, a narrow one one.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::ink::Ink;
use crate::palette::Palette;

/// How much of a run a row has no room for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clip {
    /// A run the row has no room for is not drawn at all.
    Whole,
    /// A run is drawn up to the row's right edge and the rest is dropped.
    Edge,
}

/// The columns `text` takes on a row: a wide glyph two cells, a combining mark none.
pub fn cells(text: &str) -> u16 {
    u16::try_from(text.width()).unwrap_or(u16::MAX)
}

/// `text` followed by the blanks that bring it out to `columns`, measured by what it draws: a run
/// hangs from its column because a wide glyph leaves one blank fewer behind it. A run already
/// wider than its column is left whole, because the pen pads and never cuts.
pub fn pad(text: &str, columns: u16) -> String {
    let mut padded = String::from(text);
    padded.push_str(&" ".repeat(usize::from(columns.saturating_sub(cells(text)))));
    padded
}

/// Writes `spans` from `row`'s left edge, each in its own ink, advancing by measured columns and
/// so leaving two behind every wide glyph.
pub fn write(buffer: &mut Buffer, row: Rect, spans: &[(&str, Ink)], clip: Clip, palette: &Palette) {
    if clip == Clip::Whole && width(spans) > row.width {
        return;
    }
    let mut x = row.x;
    for (text, ink) in spans {
        let room = usize::from(row.right().saturating_sub(x));
        let (next, _) = buffer.set_stringn(x, row.y, text, room, ink.style(palette));
        x = next;
    }
}

/// Writes a one-cell `glyph` at a column of a row, in an ink.
pub fn glyph(buffer: &mut Buffer, x: u16, y: u16, glyph: char, ink: Ink, palette: &Palette) {
    if let Some(cell) = buffer.cell_mut((x, y)) {
        cell.set_char(glyph);
        cell.set_style(ink.style(palette));
    }
}

/// Fills `row` edge to edge with a one-cell `fill`, in an ink.
pub fn rule(buffer: &mut Buffer, row: Rect, fill: char, ink: Ink, palette: &Palette) {
    for x in row.x..row.right() {
        glyph(buffer, x, row.y, fill, ink, palette);
    }
}

/// Hangs `spans` on `row`'s far edge, so the last cell of the run is the row's last: a run the row
/// has no room for is refused whole and hangs nowhere.
pub fn hang(buffer: &mut Buffer, row: Rect, spans: &[(&str, Ink)], palette: &Palette) {
    let width = width(spans);
    if width > row.width {
        return;
    }
    let x = row.right() - width;
    write(buffer, Rect { x, ..row }, spans, Clip::Edge, palette);
}

/// The columns a run of spans takes: every span measured by what it draws.
fn width(spans: &[(&str, Ink)]) -> u16 {
    spans
        .iter()
        .map(|(text, _)| cells(text))
        .fold(0, u16::saturating_add)
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::*;
    use crate::ink::Ink;
    use crate::palette::Palette;

    const PALETTE: Palette = Palette::TRUECOLOR;

    fn row_of(width: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, width, 1))
    }

    /// `buffer`'s one row written as `spans`, the way the card writes a row of its own.
    fn run(buffer: &mut Buffer, spans: &[(&str, Ink)], clip: Clip) {
        let row = buffer.area;
        write(buffer, row, spans, clip, &PALETTE);
    }

    fn symbol(buffer: &Buffer, x: u16) -> &str {
        buffer.cell((x, 0)).expect("a cell of the row").symbol()
    }

    fn drawn(buffer: &Buffer) -> String {
        let area = buffer.area;
        (area.x..area.right()).map(|x| symbol(buffer, x)).collect()
    }

    #[test]
    fn a_string_is_measured_by_display_width_and_not_by_the_glyphs_it_is_made_of() {
        assert_eq!(cells("keys"), 4);
        assert_eq!(cells("日本"), 4);
        assert_eq!(cells("e\u{301}"), 1);
    }

    #[test]
    fn a_run_the_row_has_no_room_for_is_refused_whole() {
        let mut narrow = row_of(3);
        run(&mut narrow, &[("keys", Ink::Plain)], Clip::Whole);
        assert_eq!(
            drawn(&narrow),
            "   ",
            "three columns hold no four-column run, and it is refused whole rather than cut"
        );

        let mut roomy = row_of(4);
        run(&mut roomy, &[("keys", Ink::Plain)], Clip::Whole);
        assert_eq!(drawn(&roomy), "keys");
    }

    #[test]
    fn a_run_is_cut_at_the_rows_right_edge() {
        let mut row = row_of(3);
        run(&mut row, &[("keys", Ink::Plain)], Clip::Edge);
        assert_eq!(
            drawn(&row),
            "key",
            "what the row has room for is drawn, up to its last column"
        );
    }

    #[test]
    fn a_wide_glyph_leaves_two_columns_behind_it() {
        let mut row = row_of(3);
        run(&mut row, &[("日本", Ink::Plain)], Clip::Edge);
        assert_eq!(
            drawn(&row),
            "日  ",
            "three columns hold one wide glyph and no part of the second"
        );

        let mut roomy = row_of(4);
        run(&mut roomy, &[("日a", Ink::Plain)], Clip::Edge);
        assert_eq!(symbol(&roomy, 0), "日");
        assert_eq!(
            symbol(&roomy, 2),
            "a",
            "the glyph after a wide one starts in the column the wide one left"
        );
    }

    #[test]
    fn a_run_is_padded_out_to_the_column_it_hangs_from() {
        assert_eq!(
            pad("keys", 6),
            "keys  ",
            "two blanks bring four columns out to six"
        );
        assert_eq!(
            pad("日本", 6),
            "日本  ",
            "two wide glyphs spend four of the six, so two blanks are left and not four"
        );
        assert_eq!(
            cells(&pad("日本", 6)),
            6,
            "and the padded run reaches the column it was given, whatever it is made of"
        );
        assert_eq!(
            pad("keys", 2),
            "keys",
            "a run wider than its column is left whole: the pen pads and never cuts"
        );
    }

    #[test]
    fn a_row_fills_with_a_rule_edge_to_edge() {
        let mut row = row_of(5);
        let area = row.area;
        rule(&mut row, area, '─', Ink::Dim, &PALETTE);
        assert_eq!(drawn(&row), "─────");
        assert_eq!(crate::readback::inks(&row, 0, &PALETTE), [(Ink::Dim, 5)]);
    }

    #[test]
    fn a_value_hangs_on_the_rows_far_edge() {
        let mut row = row_of(10);
        let area = row.area;
        hang(&mut row, area, &[("esc", Ink::Muted)], &PALETTE);
        assert_eq!(
            drawn(&row),
            "       esc",
            "three columns wide in ten starts in the seventh"
        );
    }

    #[test]
    fn a_value_the_row_cannot_hold_hangs_nowhere() {
        let mut row = row_of(2);
        let area = row.area;
        hang(&mut row, area, &[("esc", Ink::Muted)], &PALETTE);
        assert_eq!(
            drawn(&row),
            "  ",
            "a value with no room on the row is refused whole, not drawn in part"
        );
    }
}
