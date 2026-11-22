use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::*;
use crate::ink::{Face, Ink, Look};
use crate::palette::{Band, Palette};

const PALETTE: Palette = Palette::TRUECOLOR;

fn row_of(width: u16) -> Buffer {
    Buffer::empty(Rect::new(0, 0, width, 1))
}

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
fn a_claim_keeps_the_face_a_cell_wore_and_a_wash_strips_it() {
    let bold = Look::new(Ink::Accent, Face::Bold);
    let mut claimed = row_of(4);
    let area = claimed.area;
    write(&mut claimed, area, &[("keys", bold)], Clip::Edge, &PALETTE);
    claim(&mut claimed, area, Ink::Dim.style(&PALETTE));

    assert_eq!(
        Look::of(claimed.cell((0, 0)).expect("a cell of the row"), &PALETTE),
        Some(Look::new(Ink::Dim, Face::Bold)),
        "a claim is the ink it was given over the face the cell already wore"
    );

    let mut washed = row_of(4);
    write(&mut washed, area, &[("keys", bold)], Clip::Edge, &PALETTE);
    wash(&mut washed, area, Ink::Dim.style(&PALETTE));

    assert_eq!(
        Look::of(washed.cell((0, 0)).expect("a cell of the row"), &PALETTE),
        Some(Look::new(Ink::Dim, Face::Plain)),
        "and a wash takes the face with it, so a mark under a scrim reads as its words"
    );
}

#[test]
fn a_wide_glyph_leaves_no_cell_of_the_band_between_it_and_the_next_one() {
    let mut row = row_of(4);
    let area = row.area;
    row.set_style(area, PALETTE.ground(Some(Band::Prompt)));
    write(
        &mut row,
        area,
        &[("日本", Ink::Plain)],
        Clip::Edge,
        &PALETTE,
    );

    for x in area.x..area.right() {
        let cell = row.cell((x, 0)).expect("a cell of the row");
        assert_eq!(
            cell.bg,
            PALETTE.band(Band::Prompt),
            "column {x} is read through to the terminal's own ground: a wide glyph hides the \
                 cell after it and resets it, and the band a row sits on goes with it"
        );
        assert!(
            Look::of(cell, &PALETTE).is_some(),
            "column {x} reads back as no ink a fixture could name"
        );
    }
}

#[test]
fn a_count_of_columns_wider_than_any_row_holds_is_the_widest_a_row_holds() {
    assert_eq!(columns(0), 0);
    assert_eq!(columns(32), 32, "a count a row can hold is that count");
    assert_eq!(
        columns(usize::MAX),
        u16::MAX,
        "and a run wider than any row is the widest a row's own arithmetic carries, never a \
             wrap to nothing"
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
fn a_row_raised_on_the_frames_edge_still_reads_back_as_the_inks_written_on_it() {
    for palette in [Palette::ANSI, Palette::TRUECOLOR] {
        let mut row = row_of(4);
        let area = row.area;
        write(
            &mut row,
            area,
            &[("keys", Ink::Muted)],
            Clip::Edge,
            &palette,
        );
        claim(&mut row, area, Style::default().bg(palette.edge_ink()));

        assert_eq!(
            crate::readback::inks(&row, 0, &palette),
            [(Ink::Muted, 4)],
            "an edge is the surface a row is raised on, so no fixture has to name it"
        );
        assert_eq!(
            row.cell((0, 0)).expect("a cell of the row").bg,
            palette.edge_ink(),
            "and the edge it was given is the surface the row sits on"
        );
    }
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

    ground(&mut row, area, PALETTE.ground(Some(Band::Prompt)));

    assert_eq!(
        drawn(&row),
        "    ",
        "no glyph of the frame a ground covers is read through it"
    );
    assert_eq!(crate::readback::inks(&row, 0, &PALETTE), [(Ink::Plain, 4)]);
    assert_eq!(
        row.cell((0, 0)).expect("a cell of the row").bg,
        PALETTE.band(Band::Prompt),
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
