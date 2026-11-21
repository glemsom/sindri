//! What a row of the composer's band is drawn as: the gutter it opens with, the ink each run of it is
//! read in, and where a run the row has no room for stops.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::*;
use crate::readback::inks;

const PALETTE: Palette = Palette::TRUECOLOR;

fn glyphs(buffer: &Buffer, y: u16) -> String {
    (buffer.area.x..buffer.area.right())
        .map(|x| {
            buffer
                .cell((x, y))
                .unwrap_or_else(|| panic!("row {y}, column {x} is outside the band"))
                .symbol()
                .to_owned()
        })
        .collect()
}

#[test]
fn a_row_of_one_run_draws_the_gutter_then_the_words_in_the_runs_ink() {
    let area = Rect::new(0, 0, 11, 1);
    let mut opened = Buffer::empty(area);
    let mut indented = Buffer::empty(area);

    row(
        &mut opened,
        area,
        Gutter::Open,
        &[("the draft", Ink::Accent)],
        &PALETTE,
    );
    row(
        &mut indented,
        area,
        Gutter::Blank,
        &[("the draft", Ink::Accent)],
        &PALETTE,
    );

    assert_eq!(
        glyphs(&opened, 0),
        " \u{203a}the draft",
        "the sigil stands one blank past the row's first column and the words follow it, so a \
         draft's first glyph sits in the column an answer's prose starts at"
    );
    assert_eq!(
        inks(&opened, 0, &PALETTE),
        [(Ink::Dim, 2), (Ink::Accent, 9)],
        "the gutter is read in the dim ink and the words in the ink the run carries"
    );
    assert_eq!(
        glyphs(&indented, 0),
        "  the draft",
        "a row that opens nothing keeps the gutter blank, so its words stand in the same column"
    );
    assert_eq!(
        inks(&indented, 0, &PALETTE),
        [(Ink::Dim, 2), (Ink::Accent, 9)],
        "and the blank gutter is drawn in the dim ink, exactly as the sigil is"
    );
}

#[test]
fn a_row_of_several_runs_draws_each_in_the_ink_it_carries_in_order() {
    let area = Rect::new(0, 0, 15, 1);
    let mut buffer = Buffer::empty(area);

    row(
        &mut buffer,
        area,
        Gutter::Open,
        &[
            ("one ", Ink::Plain),
            ("two ", Ink::Accent),
            ("three", Ink::Err),
        ],
        &PALETTE,
    );

    assert_eq!(
        glyphs(&buffer, 0),
        " \u{203a}one two three",
        "the runs stand side by side in the order the row is handed them, with no column between them"
    );
    assert_eq!(
        inks(&buffer, 0, &PALETTE),
        [
            (Ink::Dim, 2),
            (Ink::Plain, 4),
            (Ink::Accent, 4),
            (Ink::Err, 5)
        ],
        "each run reads back as one stretch of cells in the ink it carries, so a row of three inks is \
         three runs and not one"
    );
}

#[test]
fn a_run_the_row_has_no_room_for_is_cut_at_the_edge_and_the_run_after_it_is_dropped() {
    let area = Rect::new(0, 0, 10, 1);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 2));

    row(
        &mut buffer,
        area,
        Gutter::Open,
        &[("abcdefghijkl", Ink::Plain), ("more", Ink::Err)],
        &PALETTE,
    );

    assert_eq!(
        glyphs(&buffer, 0),
        " \u{203a}abcdefgh",
        "the run is drawn up to the row's last column and the rest of it is dropped"
    );
    assert_eq!(
        inks(&buffer, 0, &PALETTE),
        [(Ink::Dim, 2), (Ink::Plain, 8)],
        "the columns the cut leaves are the run's own, and the run after them has no column to start in"
    );
    assert_eq!(
        glyphs(&buffer, 1),
        pen::blank(10),
        "the rest of the run wraps to no row below it: a row of the band is one row"
    );
}

#[test]
fn a_lit_gutter_draws_the_sigil_in_the_accent_and_the_other_two_in_the_dim_ink() {
    let area = Rect::new(0, 0, 3, 1);
    for (gutter, glyphs_drawn, inks_drawn) in [
        (
            Gutter::Lit,
            " \u{203a}a",
            &[(Ink::Dim, 1), (Ink::Accent, 1), (Ink::Plain, 1)][..],
        ),
        (
            Gutter::Open,
            " \u{203a}a",
            &[(Ink::Dim, 2), (Ink::Plain, 1)][..],
        ),
        (Gutter::Blank, "  a", &[(Ink::Dim, 2), (Ink::Plain, 1)][..]),
    ] {
        let mut buffer = Buffer::empty(area);
        row(&mut buffer, area, gutter, &[("a", Ink::Plain)], &PALETTE);

        assert_eq!(
            glyphs(&buffer, 0),
            glyphs_drawn,
            "{gutter:?} draws the gutter it names"
        );
        assert_eq!(
            inks(&buffer, 0, &PALETTE),
            inks_drawn,
            "{gutter:?} is drawn in the one ink it carries, and only a lit one is the accent"
        );
    }
}
