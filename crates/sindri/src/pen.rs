//! The pen: the one place a run of glyphs is written into a frame, and the one place a string's
//! width is measured — so a wide glyph takes two cells wherever it is drawn and a narrow one.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::ink::Ink;
use crate::palette::Palette;

/// The columns `text` takes on a row, measured rather than counted: a wide glyph is two cells and
/// a combining mark is none.
pub fn cells(text: &str) -> u16 {
    u16::try_from(text.width()).unwrap_or(u16::MAX)
}

/// How much of a run a row has no room for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clip {
    /// A run the row has no room for is not drawn at all.
    Whole,
    /// A run is drawn up to the row's right edge and the rest is dropped.
    Edge,
}

/// Writes `spans` from `row`'s left edge, each in its own ink, and answers whether the whole run
/// landed: the columns it advances are measured, so a wide glyph leaves two behind it.
pub fn write(
    buffer: &mut Buffer,
    row: Rect,
    spans: &[(&str, Ink)],
    clip: Clip,
    palette: &Palette,
) -> bool {
    let width: u32 = spans.iter().map(|(text, _)| u32::from(cells(text))).sum();
    let fits = width <= u32::from(row.width);
    if clip == Clip::Whole && !fits {
        return false;
    }
    let mut x = row.x;
    for (text, ink) in spans {
        let room = usize::from(row.right().saturating_sub(x));
        let (next, _) = buffer.set_stringn(x, row.y, text, room, ink.style(palette));
        x = next;
    }
    fits
}

/// Fills `row` with `glyph` in `ink`, from its left edge to its last column.
pub fn rule(buffer: &mut Buffer, row: Rect, glyph: char, ink: Ink, palette: &Palette) {
    let fill: String = std::iter::repeat_n(glyph, usize::from(row.width)).collect();
    write(buffer, row, &[(&fill, ink)], Clip::Edge, palette);
}

/// Hangs `spans` on `row`'s far edge, so the last cell of the run is the row's last, and answers
/// the column it starts in: a run the row has no room for is refused whole and hangs nowhere.
pub fn hang(
    buffer: &mut Buffer,
    row: Rect,
    spans: &[(&str, Ink)],
    palette: &Palette,
) -> Option<u16> {
    let width: u32 = spans.iter().map(|(text, _)| u32::from(cells(text))).sum();
    let start = u32::from(row.right()).checked_sub(width)?;
    if start < u32::from(row.x) {
        return None;
    }
    let x = u16::try_from(start).ok()?;
    write(buffer, Rect { x, ..row }, spans, Clip::Whole, palette).then_some(x)
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
    fn run(buffer: &mut Buffer, spans: &[(&str, Ink)], clip: Clip) -> bool {
        let row = buffer.area;
        write(buffer, row, spans, clip, &PALETTE)
    }

    fn symbol(buffer: &Buffer, x: u16) -> &str {
        buffer.cell((x, 0)).expect("a cell of the row").symbol()
    }

    fn drawn(buffer: &Buffer) -> String {
        let area = buffer.area;
        (area.x..area.right())
            .map(|x| {
                buffer
                    .cell((x, area.y))
                    .expect("a cell of the row")
                    .symbol()
            })
            .collect()
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
        assert!(
            !run(&mut narrow, &[("keys", Ink::Plain)], Clip::Whole),
            "three columns hold no four-column run"
        );
        assert_eq!(
            drawn(&narrow),
            "   ",
            "and the run is refused whole rather than drawn in part"
        );

        let mut roomy = row_of(4);
        assert!(run(&mut roomy, &[("keys", Ink::Plain)], Clip::Whole));
        assert_eq!(drawn(&roomy), "keys");
    }

    #[test]
    fn a_run_is_cut_at_the_rows_right_edge() {
        let mut row = row_of(3);
        assert!(
            !run(&mut row, &[("keys", Ink::Plain)], Clip::Edge),
            "a cut run did not land whole"
        );
        assert_eq!(
            drawn(&row),
            "key",
            "and what the row has room for is drawn, up to its last column"
        );
    }

    #[test]
    fn a_wide_glyph_leaves_two_columns_behind_it() {
        let mut row = row_of(3);
        run(&mut row, &[("日本", Ink::Plain)], Clip::Edge);
        assert_eq!(symbol(&row, 0), "日");
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
        assert_eq!(
            hang(&mut row, area, &[("esc", Ink::Muted)], &PALETTE),
            Some(7),
            "three columns wide in ten starts in the seventh"
        );
        assert_eq!(drawn(&row), "       esc");
    }

    #[test]
    fn a_value_the_row_cannot_hold_hangs_nowhere() {
        let mut row = row_of(2);
        let area = row.area;
        assert_eq!(hang(&mut row, area, &[("esc", Ink::Muted)], &PALETTE), None);
        assert_eq!(
            drawn(&row),
            "  ",
            "a value with no room on the row is refused whole, not drawn in part"
        );
    }
}
