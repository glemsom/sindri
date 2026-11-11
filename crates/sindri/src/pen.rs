//! The pen: the one place a run of glyphs is written into a frame, the one place a string's width is
//! measured — a wide glyph two cells wherever it is drawn, a narrow one one — and the one place a
//! row is filled to its edge or a run is hung on it.

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::ink::{Ink, Look};
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

/// The columns one `glyph` takes, counted the way a row of it is drawn.
pub fn cell(glyph: char) -> u16 {
    cells(glyph.encode_utf8(&mut [0; 4]))
}

/// `columns` of one `glyph`: the run a row fills to its edge with, and the one a name is padded
/// out to. The pen spells a repeated run and no caller does.
pub fn filled(columns: u16, glyph: char) -> String {
    std::iter::repeat_n(glyph, usize::from(columns)).collect()
}

/// `columns` blanks: the run a value hangs from, and the one a name is padded out to.
pub fn blank(columns: u16) -> String {
    filled(columns, ' ')
}

/// `text` followed by the blanks that bring it out to `columns`, measured by what it draws. A run
/// already wider than its column is left whole, because the pen pads and never cuts.
pub fn pad(text: &str, columns: u16) -> String {
    let mut padded = String::from(text);
    padded.push_str(&blank(columns.saturating_sub(cells(text))));
    padded
}

/// Writes `spans` from `row`'s left edge, each in its own look, advancing by measured columns and
/// so leaving two behind every wide glyph.
pub fn write<S: AsRef<str>, D: Copy + Into<Look>>(
    buffer: &mut Buffer,
    row: Rect,
    spans: &[(S, D)],
    clip: Clip,
    palette: &Palette,
) {
    if clip == Clip::Whole && width(spans) > row.width {
        return;
    }
    let mut x = row.x;
    for (text, look) in spans {
        let look: Look = (*look).into();
        let room = usize::from(row.right().saturating_sub(x));
        let (next, _) = buffer.set_stringn(x, row.y, text.as_ref(), room, look.style(palette));
        x = next;
    }
}

/// Writes `text` from column `x` of row `y` and answers the column after it: the run is measured
/// by what it draws, so a wide glyph leaves the column behind it blank and is never cut in half.
pub fn run(buffer: &mut Buffer, x: u16, y: u16, text: &str, ink: Ink, palette: &Palette) -> u16 {
    let row = Rect {
        x,
        y,
        width: buffer.area.right().saturating_sub(x),
        height: 1,
    };
    write(buffer, row, &[(text, ink)], Clip::Edge, palette);
    x.saturating_add(cells(text))
}

/// Writes a one-cell `glyph` at a column of a row, in an ink.
pub fn glyph(buffer: &mut Buffer, x: u16, y: u16, glyph: char, ink: Ink, palette: &Palette) {
    each(Rect::new(x, y, 1, 1), buffer, |cell| {
        cell.set_char(glyph);
        cell.set_style(ink.style(palette));
    });
}

/// Claims every cell of `area` in `style` and leaves each one its glyph and its own background: the
/// scrim a card dims the frame behind it with.
pub fn wash(buffer: &mut Buffer, area: Rect, style: Style) {
    let claimed = style.remove_modifier(Modifier::all());
    each(area, buffer, |cell| {
        cell.set_style(claimed);
    });
}

/// Blanks every cell of `area` to `style`: the ground a card covers a frame with, so no glyph the
/// frame behind it drew is read through it.
pub fn ground(buffer: &mut Buffer, area: Rect, style: Style) {
    each(area, buffer, |cell| {
        cell.reset();
        cell.set_style(style);
    });
}

/// Every cell of `area`, top to bottom and left to right: the one loop that walks cells, so a
/// module that draws or paints a frame asks the pen for it.
fn each(area: Rect, buffer: &mut Buffer, mut edit: impl FnMut(&mut Cell)) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                edit(cell);
            }
        }
    }
}

/// Fills `row` edge to edge with a one-cell `fill`, in an ink.
pub fn rule(buffer: &mut Buffer, row: Rect, fill: char, ink: Ink, palette: &Palette) {
    write(
        buffer,
        row,
        &[(filled(row.width, fill), ink)],
        Clip::Edge,
        palette,
    );
}

/// The column a run `width` columns wide starts at when it hangs on `row`'s far edge, and none when
/// the row has no room for it: the one hanging column a fact's value and a card's key both ask.
pub fn hanging(row: Rect, width: u16) -> Option<u16> {
    (width <= row.width).then(|| row.right() - width)
}

/// Hangs `spans` on `row`'s far edge, so the last cell of the run is the row's last: a run the row
/// has no room for is refused whole and hangs nowhere.
pub fn hang<S: AsRef<str>, D: Copy + Into<Look>>(
    buffer: &mut Buffer,
    row: Rect,
    spans: &[(S, D)],
    palette: &Palette,
) {
    let Some(x) = hanging(row, width(spans)) else {
        return;
    };
    write(buffer, Rect { x, ..row }, spans, Clip::Edge, palette);
}

/// The columns a run of spans takes: every span measured by what it draws.
fn width<S: AsRef<str>, D>(spans: &[(S, D)]) -> u16 {
    spans
        .iter()
        .map(|(text, _)| cells(text.as_ref()))
        .fold(0, u16::saturating_add)
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use super::*;
    use crate::ink::Ink;
    use crate::palette::{Band, Palette};

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
        assert_eq!(cell('日'), 2, "one glyph is measured as the run it is in");
        assert_eq!(cell('e'), 1);
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
    fn a_wash_claims_a_rows_ink_and_keeps_the_surface_and_the_glyphs_under_it() {
        let mut row = row_of(4);
        let area = row.area;
        row.set_style(area, PALETTE.ground(Some(Band::Rail)));
        write(&mut row, area, &[("keys", Ink::Mark)], Clip::Edge, &PALETTE);

        wash(&mut row, area, Ink::Dim.style(&PALETTE));

        assert_eq!(
            drawn(&row),
            "keys",
            "the glyphs a wash passes over are still the row's"
        );
        assert_eq!(
            crate::readback::inks(&row, 0, &PALETTE),
            [(Ink::Dim, 4)],
            "and each of them reads back as the ink the wash claimed, the mark's face gone with it"
        );
        assert_eq!(
            row.cell((0, 0)).expect("a cell of the row").bg,
            PALETTE.band(Band::Rail),
            "a band is the surface a cell sits on, so a wash leaves it alone"
        );
    }

    #[test]
    fn a_ground_blanks_a_row_and_lays_the_surface_it_is_given() {
        let mut row = row_of(4);
        let area = row.area;
        write(
            &mut row,
            area,
            &[("keys", Ink::Accent)],
            Clip::Edge,
            &PALETTE,
        );

        ground(&mut row, area, PALETTE.ground(Some(Band::Composer)));

        assert_eq!(
            drawn(&row),
            "    ",
            "no glyph of the frame a ground covers is read through it"
        );
        assert_eq!(crate::readback::inks(&row, 0, &PALETTE), [(Ink::Plain, 4)]);
        assert_eq!(
            row.cell((0, 0)).expect("a cell of the row").bg,
            PALETTE.band(Band::Composer),
            "and the surface it was given is the one the row sits on"
        );
    }

    #[test]
    fn a_row_fills_to_its_edge_with_one_glyph_and_one_run() {
        assert_eq!(filled(4, '─'), "────", "four columns of the one glyph");
        assert_eq!(
            cells(&filled(4, '─')),
            4,
            "and the run draws in the columns it was given"
        );
        assert_eq!(
            blank(3),
            filled(3, ' '),
            "a run of blanks is the same fill in a space"
        );
        assert_eq!(
            filled(0, '─'),
            "",
            "a row of no columns holds no run at all"
        );
    }

    #[test]
    fn a_run_is_padded_out_to_the_column_it_hangs_from() {
        assert_eq!(
            blank(3),
            "   ",
            "a hanging column is blanks and nothing else"
        );
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
    fn a_run_hangs_on_its_rows_far_edge_and_nowhere_the_row_has_no_room() {
        let row = Rect::new(0, 0, 10, 1);
        assert_eq!(
            hanging(row, 3),
            Some(7),
            "three columns wide in ten starts in the seventh"
        );
        assert_eq!(
            hanging(row, 10),
            Some(0),
            "a run as wide as its row starts at the row's own first column"
        );
        assert_eq!(
            hanging(row, 11),
            None,
            "a run the row has no room for hangs on no column at all"
        );
        let offset = Rect::new(4, 2, 10, 1);
        assert_eq!(
            hanging(offset, 3),
            Some(11),
            "a row that starts partway along a frame hangs from its own far edge"
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
