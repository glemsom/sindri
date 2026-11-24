use std::path::PathBuf;

use ratatui::style::Color;

use sindri_core::policy::Enforcement;
use sindri_core::records::Usage;

use super::*;
use crate::ink::Ink;
use crate::ledger::{Ctx, FIELD, FLOOR, FULL_CELL, GAP, METER, Rail, tokens};

const COLUMNS: u16 = FLOOR;
use crate::palette::{Band, channels};
use crate::pen;
use crate::plane::{Axis, Bar, Placement, Plane, PlaneId};
use crate::readback::{column_of, inks, under};
use crate::rows::{MARGIN, margins};
use crate::size::{Laid, Sizes};

const METERED: u16 = COLUMNS + MARGIN * 2;

const LEFT: u16 = 20;

fn columns(text: &str) -> usize {
    usize::from(pen::cells(text))
}

const PALETTE: Palette = Palette::TRUECOLOR;

fn paint(
    facts: &Facts,
    rail: Rail,
    area: Rect,
    marker: Option<&Pulse>,
    buffer: &mut Buffer,
    palette: &Palette,
) {
    super::paint(
        facts,
        rail,
        RailBand::of(area),
        marker,
        (None, None),
        buffer,
        palette,
    );
}

fn ids(placements: &[Placement]) -> Vec<PlaneId> {
    placements
        .iter()
        .map(|placement| placement.shade().id())
        .collect()
}

fn band(width: u16, height: u16) -> Rect {
    Rect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

fn stood(height: u16) -> Rect {
    Rect {
        x: LEFT,
        ..band(COLUMNS, height)
    }
}

fn framed(rail: Rect) -> Rect {
    Rect {
        width: rail.right(),
        ..rail
    }
}

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

const SERVED: &str = "claude-sonnet-4-5-20250929";

const CUT: &str = "claude-sonnet-4-5…";

const BRANCH: &str = "spec/1-capability";

fn ctx_line(width: u16, value: &str) -> String {
    let room = width
        .saturating_sub(margins())
        .saturating_sub(FIELD)
        .saturating_sub(pen::cells(value));
    format!(
        "{}{}{value}{}{}",
        pen::blank(MARGIN),
        pen::pad("ctx", FIELD),
        pen::blank(room),
        pen::blank(MARGIN)
    )
}

fn frame() -> Facts {
    Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from(SERVED)),
        think: Some(String::from("high")),
        turns: Some((3, 250)),
        ctx: Some(Ctx {
            size: 48_200,
            window: None,
        }),
        cache: Some(88),
        workspace: Some(PathBuf::from(
            "/home/glemsom/Documents/git/sindri/.worktrees/46-rail",
        )),
        branch: Some(String::from("spec/1-capability")),
        loaded: ["bash", "records"].into_iter().collect(),
        sandbox: Some(Enforcement::Bubblewrap),
        session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
        temp: Some(PathBuf::from(
            "/home/glemsom/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
        )),
        ..Facts::default()
    }
}

fn windowed() -> Facts {
    Facts {
        ctx: Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        ..spent()
    }
}

fn spent() -> Facts {
    Facts {
        spend: Usage {
            input: Some(48_200),
            output: Some(3_100),
            total: Some(51_300),
            ..Usage::default()
        },
        ..frame()
    }
}

fn unspent() -> Facts {
    Facts {
        ctx: Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        ..frame()
    }
}

const CONTRACT: [(&str, &str); 12] = [
    ("wire", "anthropic"),
    ("model", CUT),
    ("think", "high"),
    ("turns", "3 / 250"),
    ("ctx", "48.2k"),
    ("cache", "88%"),
    ("workspace", "…/46-rail"),
    ("branch", "spec/1-capability"),
    ("skills", "2"),
    ("sandbox", "bwrap"),
    ("session", "9f2c7a41"),
    ("temp", "…/9f2c7a41/tmp"),
];

#[test]
fn a_ledger_stands_on_its_own_tone_on_every_row_it_claims() {
    let area = band(COLUMNS, 8);
    let mut buffer = Buffer::empty(area);

    let placement = super::paint(
        &frame(),
        Rail::Ledger,
        RailBand::of(area),
        None,
        (None, None),
        &mut buffer,
        &PALETTE,
    )
    .into_iter()
    .next()
    .expect("a band to shade");

    assert!(
        !placement.places(),
        "a terminal that places no picture is handed no plane under the rail"
    );
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, y),
                match x == area.x {
                    true => PALETTE.edge_ink(),
                    false => PALETTE.band(Band::Rail),
                },
                "row {y}, column {x} of the ledger is not the rail's own tone: the band is one \
                 surface from its first row to its last, and the one column of it the frame \
                 claims is its seam"
            );
        }
    }
}

#[test]
fn the_foot_is_drawn_on_the_bands_last_row_where_no_shadow_falls() {
    let area = band(COLUMNS, 24);
    let mut buffer = Buffer::empty(area);

    paint(&spent(), Rail::Ledger, area, None, &mut buffer, &PALETTE);

    let floor = area.bottom() - 1;
    assert_eq!(
        under(&buffer, area.x + 1, floor),
        PALETTE.band(Band::Rail),
        "the band's last row is the rail's own tone past its seam, and the foot stands on it \
         rather than above it"
    );
    assert_eq!(
        glyphs(&buffer, floor).trim_end(),
        format!("{}{}51.3k", pen::blank(MARGIN), pen::pad("total", FIELD)),
        "the total is the last row of the foot, so it stands on the band's own last row, with \
         its figure opening on the field its label is set in"
    );
}

#[test]
fn a_fact_standing_on_the_rails_first_column_reads_back_as_the_ink_it_is_written_in() {
    let area = band(COLUMNS, 8);
    let mut buffer = Buffer::empty(area);

    paint(&frame(), Rail::Ledger, area, None, &mut buffer, &PALETTE);

    let first = buffer
        .cell((area.x + MARGIN, area.y))
        .expect("the label's first column");
    assert_eq!(
        under(&buffer, area.x + MARGIN, area.y),
        PALETTE.band(Band::Rail),
        "the label's first glyph stands on the rail's own band"
    );
    assert_eq!(
        Ink::of(first, &PALETTE),
        Some(Ink::Dim),
        "and is read as the dim ink, because a band is a surface a cell is drawn on and never \
         an ink it is drawn in"
    );
}

#[test]
fn a_picture_over_the_rails_top_rows_is_drawn_on_the_rails_ground_and_no_fact_stands_there() {
    let area = band(COLUMNS, 30);
    let mut buffer = Buffer::empty(area);

    super::paint(
        &frame(),
        Rail::Ledger,
        RailBand::of(area).with_picture(19, 11),
        None,
        (None, None),
        &mut buffer,
        &PALETTE,
    );

    for y in area.y..area.y + 11 {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, y),
                match x == area.x {
                    true => PALETTE.edge_ink(),
                    false => PALETTE.band(Band::Rail),
                },
                "row {y}, column {x} of the band the picture is drawn over is not the rail's own \
                 ground: the face would stand on the frame and not on the rail, and the one \
                 column beside the transcript stays the frame's seam"
            );
        }
        assert_eq!(
            glyphs(&buffer, y).trim_end(),
            "",
            "row {y} of the band the picture is drawn over holds a fact, which the face \
                 would be drawn across"
        );
    }
    assert_eq!(
        under(&buffer, area.x + 1, 11),
        PALETTE.band(Band::Rail),
        "and the rule that closes the face is written over the rail's own ground and never in \
         place of it, so the band is one surface under the picture and under the rule"
    );
    assert_eq!(
        under(&buffer, area.x, 11),
        PALETTE.edge_ink(),
        "and it opens one column in, so the seam's first column runs unbroken down the rail"
    );
    assert_eq!(
        glyphs(&buffer, 11).trim_end(),
        format!(" {}", pen::filled(COLUMNS - 1, '\u{2500}')).trim_end(),
        "the face is closed by the one rule the rail draws in its body, past the seam's column, \
             and the block under it is parted from it by no second line"
    );
    assert_eq!(
        glyphs(&buffer, 12).trim_end(),
        "  wire      anthropic",
        "so the first block stands on the row under that rule"
    );
}

#[test]
fn a_ledger_handed_a_cell_is_handed_a_shade_of_its_own_band_and_a_seam_in_its_first_column() {
    let rail = stood(8);
    let mut buffer = Buffer::empty(framed(rail));

    let placement = super::paint(
        &frame(),
        Rail::Ledger,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    )
    .into_iter()
    .next()
    .expect("a palette that spells its colours is handed the band's shading");
    let plane = Plane::draw(*placement.shade()).expect("a band a plane is drawn under");

    assert_eq!(
        placement.cells(),
        rail,
        "the placement covers the rail's own rows and columns, and no column beside it"
    );
    assert_eq!(
        plane.pixels().dimensions(),
        (
            u32::from(rail.width) * CellSize::NOMINAL.width(),
            u32::from(rail.height) * CellSize::NOMINAL.height()
        ),
        "at the pixels of the cell it is drawn on"
    );
    assert_eq!(
        placement.seam().expect("a ledger beside a transcript band"),
        (Axis::Down, Rect { width: 1, ..rail }),
        "and the seam the shade draws is the one column of the rail's own band, down every row \
         it claims"
    );
}

#[test]
fn a_ledger_stood_at_the_frames_edge_keeps_its_band_and_draws_its_seam_in_its_own_first_column() {
    let rail = Rect {
        x: 1,
        ..band(COLUMNS, 8)
    };
    let mut buffer = Buffer::empty(framed(rail));

    let placement = super::paint(
        &frame(),
        Rail::Ledger,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    )
    .into_iter()
    .next()
    .expect("a palette that spells its colours is handed the band's shading");

    assert_eq!(
        placement.cells(),
        rail,
        "a rail's shade is its own band, whatever the frame has to the left of it"
    );
    assert_eq!(
        placement.seam().expect("a ledger beside a transcript band"),
        (Axis::Down, Rect { width: 1, ..rail }),
        "and its seam is the rail's own first column, so no column of a frame the rail does not \
         reach is claimed by it"
    );
}

#[test]
fn the_rails_plane_is_one_tone_in_every_pixel_it_covers() {
    let rail = stood(8);
    let mut buffer = Buffer::empty(framed(rail));

    let placement = super::paint(
        &frame(),
        Rail::Ledger,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    )
    .into_iter()
    .next()
    .expect("a palette that spells its colours is handed the band's shading");
    let plane = Plane::draw(*placement.shade()).expect("a band a plane is drawn under");

    let pixels = plane.pixels();
    let (width, height) = pixels.dimensions();
    let tone = channels(PALETTE.band(Band::Rail)).expect("a band in pixels");
    let seam = CellSize::NOMINAL.width();

    for y in 0..height {
        for x in 0..width {
            if x < seam {
                continue;
            }
            assert_eq!(
                pixels.get_pixel(x, y).0[..3],
                tone,
                "pixel row {y}, column {x} of the rail's plane is not the rail's own tone: no row \
                 of the band falls into the shadow, its last cell included, and the only column \
                 that is drawn is the seam's own"
            );
        }
    }
}

#[test]
fn a_ledger_handed_a_cell_lights_no_column_and_shadows_no_row_of_its_band() {
    let rail = stood(8);
    let mut buffer = Buffer::empty(framed(rail));

    super::paint(
        &frame(),
        Rail::Ledger,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    )
    .into_iter()
    .next()
    .expect("a palette that spells its colours is handed the band's shading");

    for y in rail.y..rail.bottom() {
        for x in rail.x..rail.right() {
            assert_eq!(
                under(&buffer, x, y),
                PALETTE.ground(None).bg.expect("the terminal's own ground"),
                "row {y}, column {x} of the band is not claimed as the terminal's own ground: a \
                 lit column down the rail or a shadow across its last is a line drawn where the \
                 plane falls, and the shade is no line \u{2014} the seam's own cell included, \
                 which the plane draws its parting in"
            );
        }
    }
}

#[test]
fn a_strip_is_handed_a_plane_of_its_one_row_and_no_seam() {
    let rail = Rect {
        x: LEFT,
        ..band(40, 1)
    };
    let mut buffer = Buffer::empty(framed(rail));

    let placement = super::paint(
        &frame(),
        Rail::Strip,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    )
    .into_iter()
    .next()
    .expect("a palette that spells its colours is handed the band's shading");

    assert_eq!(
        placement.cells(),
        rail,
        "a strip is one row of the rail's own columns, and its plane takes no column of the \
         transcript's"
    );
    assert!(
        placement.seam().is_none(),
        "a strip lies under the transcript band and parts nothing, so it makes no seam"
    );
    let plane = Plane::draw(*placement.shade()).expect("a band a plane is drawn under");
    let (width, height) = plane.pixels().dimensions();
    let tone = channels(PALETTE.band(Band::Rail)).expect("a band in pixels");
    assert_eq!(
        height,
        CellSize::NOMINAL.height(),
        "a strip's plane is the one row it has"
    );
    for y in 0..height {
        for x in 0..width {
            assert_eq!(
                plane.pixels().get_pixel(x, y).0[..3],
                tone,
                "pixel row {y}, column {x} of the strip's plane is not the rail's own band \
                 colour: one row has no shadow to fall into and no column to fall across"
            );
        }
    }
}

#[test]
fn a_rail_handed_a_cell_in_no_colour_draws_the_flat_band_it_drew_without_one() {
    for (shape, area) in [(Rail::Ledger, band(COLUMNS, 8)), (Rail::Strip, band(40, 1))] {
        let mut pictured = Buffer::empty(area);
        let mut placeless = Buffer::empty(area);

        let placement = super::paint(
            &frame(),
            shape,
            RailBand::of(area),
            None,
            (Some(CellSize::NOMINAL), None),
            &mut pictured,
            &Palette::SLOTS,
        )
        .into_iter()
        .next()
        .expect("a band to shade");
        paint(&frame(), shape, area, None, &mut placeless, &Palette::SLOTS);

        assert!(
            !placement.places(),
            "a palette that draws no colour is handed no plane under the {shape:?}, so the band's \
             own rows are the whole of its shading"
        );
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                assert_eq!(
                    pictured[(x, y)],
                    placeless[(x, y)],
                    "row {y}, column {x} of the {shape:?} is not the cell the band drew with no \
                     plane under it: a palette that draws no colour draws the one flat band a \
                     reader who asked for it already had"
                );
            }
        }
    }
}

#[test]
fn the_strip_is_one_row_of_the_rails_ground_and_draws_no_elevation() {
    for width in [40, 80, 99] {
        let area = band(width, 1);
        let mut buffer = Buffer::empty(area);

        paint(&frame(), Rail::Strip, area, None, &mut buffer, &PALETTE);

        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, area.y),
                PALETTE.band(Band::Rail),
                "column {x} of a {width}-column strip is not the rail's own ground: one row \
                     has no room for both ends of the ramp, and no boundary to draw a rule on"
            );
        }
    }
}

#[test]
fn a_rail_drawn_in_no_colour_draws_no_edge() {
    for (shape, area) in [(Rail::Ledger, band(COLUMNS, 8)), (Rail::Strip, band(40, 1))] {
        let mut buffer = Buffer::empty(area);

        paint(&frame(), shape, area, None, &mut buffer, &Palette::SLOTS);

        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                assert_eq!(
                    under(&buffer, x, y),
                    Color::Reset,
                    "row {y}, column {x} of a {shape:?} is given a ground, and a reader who \
                         asked for no colour asked for their own"
                );
            }
        }
    }
}

#[test]
fn a_gauge_stands_on_the_row_the_window_is_stated_on() {
    let facts = windowed();
    let rail = Rect {
        x: LEFT,
        ..band(40, 20)
    };
    let mut buffer = Buffer::empty(framed(rail));

    let placements = super::paint(
        &facts,
        Rail::Ledger,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    );
    let rows = ledger::rows(&facts, ledger::Shape::Ledger, rail, None);
    let ctx = rows
        .iter()
        .find(|row| row.kind() == Some(Kind::Ctx))
        .expect("the row the ctx fact is stated on");

    assert_eq!(
        ids(&placements)[1],
        PlaneId::Gauge,
        "a rail whose terminal places pictures is handed its band's shade and then the gauge the \
         window's own fill draws"
    );
    assert_eq!(
        placements[1].cells(),
        Rect {
            x: rail.x + MARGIN,
            y: rail.y + ctx.y(),
            width: rail.width - margins(),
            height: 1,
        },
        "the gauge stands on the row the ctx fact is stated on, in the rail's columns inside the \
         Margin on both edges"
    );
}

#[test]
fn each_bar_stands_on_the_row_its_figure_is_stated_on() {
    let facts = spent();
    let rail = Rect {
        x: LEFT,
        ..band(40, 24)
    };
    let mut buffer = Buffer::empty(framed(rail));

    let placements = super::paint(
        &facts,
        Rail::Ledger,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    );
    let rows = ledger::rows(&facts, ledger::Shape::Ledger, rail, None);
    let stated = |kind| {
        rows.iter()
            .find(|row| row.kind() == Some(kind))
            .unwrap_or_else(|| panic!("the row {kind:?} is stated on"))
            .y()
    };

    assert_eq!(
        ids(&placements)[1..3].to_vec(),
        vec![PlaneId::Bar(Bar::Input), PlaneId::Bar(Bar::Output)],
        "the foot's two figures are drawn as a bar each, the input's before the output's"
    );
    for (placement, kind) in placements[1..3].iter().zip([Kind::Input, Kind::Output]) {
        assert_eq!(
            placement.cells(),
            Rect {
                x: rail.x + MARGIN,
                y: rail.y + stated(kind),
                width: rail.width - margins(),
                height: 1,
            },
            "the {kind:?} bar stands on the row its figure is stated on, in the rail's columns \
             inside the Margin on both edges"
        );
    }
}

#[test]
fn a_rail_that_states_no_spend_is_handed_its_gauge_and_no_bezel() {
    let facts = unspent();
    let rail = Rect {
        x: LEFT,
        ..band(40, 24)
    };
    let mut buffer = Buffer::empty(framed(rail));

    let placements = super::paint(
        &facts,
        Rail::Ledger,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        ids(&placements),
        vec![PlaneId::Rail, PlaneId::Gauge],
        "a rail whose foot states no figure is handed its band's shade and the gauge and no \
         bezel at all, the room under its last fact being plain air"
    );
}

#[test]
fn the_bezel_stands_in_the_room_the_rail_leaves_between_its_last_fact_and_its_foot() {
    let facts = spent();
    let rail = Rect {
        x: LEFT,
        ..band(40, 24)
    };
    let mut buffer = Buffer::empty(framed(rail));

    let placements = super::paint(
        &facts,
        Rail::Ledger,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    );
    let rows = ledger::rows(&facts, ledger::Shape::Ledger, rail, None);
    let room = instrument::bezel(&rows)
        .expect("the air the rail leaves between its last fact and its foot");

    assert_eq!(
        ids(&placements).last().copied(),
        Some(PlaneId::Bezel),
        "the rail's last instrument is the bezel that frames the air it leaves"
    );
    assert_eq!(
        placements.last().expect("an instrument").cells(),
        Rect {
            x: rail.x + MARGIN,
            y: rail.y + room.start,
            width: rail.width - margins(),
            height: room.len() as u16,
        },
        "the bezel stands on the rows the rail numbers them by, in the rail's own columns inside \
         the Margin on both edges"
    );
}

fn installed() -> (Facts, RailBand) {
    let rail = Rect {
        x: LEFT,
        ..band(40, 36)
    };
    (windowed(), RailBand::of(rail).with_picture(19, 11))
}

#[test]
fn the_rails_instruments_are_the_gauge_the_two_bars_and_the_bezel() {
    let (facts, band) = installed();
    let mut buffer = Buffer::empty(framed(band.area));

    let placements = super::paint(
        &facts,
        Rail::Ledger,
        band,
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        ids(&placements),
        vec![
            PlaneId::Rail,
            PlaneId::Gauge,
            PlaneId::Bar(Bar::Input),
            PlaneId::Bar(Bar::Output),
            PlaneId::Bezel,
        ],
        "the band's shade is handed first and then its instruments, the gauge before the two \
         bars and the bars before the bezel, each under an identity of its own"
    );
}

#[test]
fn an_instrument_moves_no_word_of_the_row_it_stands_on() {
    let (facts, band) = installed();
    let mut pictured = Buffer::empty(framed(band.area));
    let mut plain = Buffer::empty(framed(band.area));

    let placements = super::paint(
        &facts,
        Rail::Ledger,
        band,
        None,
        (Some(CellSize::NOMINAL), None),
        &mut pictured,
        &PALETTE,
    );
    super::paint(
        &facts,
        Rail::Ledger,
        band,
        None,
        (None, None),
        &mut plain,
        &PALETTE,
    );

    assert_eq!(ids(&placements).len(), 5, "a rail of four instruments");
    for placement in &placements[1..] {
        let area = placement.cells();
        for y in area.y..area.bottom() {
            assert_eq!(
                glyphs(&pictured, y),
                glyphs(&plain, y),
                "row {y} of the {:?} states words the row drew before the instrument stood on it: \
                 an instrument is drawn under the cells of a row and never in place of one",
                placement.shade().id()
            );
        }
    }
}

#[test]
fn every_instrument_keeps_the_rails_margin_and_stands_inside_the_block_it_is_drawn_in() {
    let (facts, band) = installed();
    let mut buffer = Buffer::empty(framed(band.area));

    let placements = super::paint(
        &facts,
        Rail::Ledger,
        band,
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(ids(&placements).len(), 5, "a rail of four instruments");
    for placement in &placements[1..] {
        let area = placement.cells();
        assert_eq!(
            (area.x, area.right()),
            (band.facts.x + MARGIN, band.facts.right() - MARGIN),
            "the {:?} at columns {}..{} does not open and close in the Margin the rail keeps on \
             both edges",
            placement.shade().id(),
            area.x,
            area.right()
        );
        assert!(
            area.y >= band.facts.y && area.bottom() <= band.facts.bottom(),
            "the {:?} at rows {}..{} stands outside the block the rail laid for it, {}..{}",
            placement.shade().id(),
            area.y,
            area.bottom(),
            band.facts.y,
            band.facts.bottom()
        );
        assert!(
            area.width <= band.facts.width && area.height <= band.facts.height,
            "the {:?} is bigger than the block it stands in",
            placement.shade().id()
        );
    }
}

#[test]
fn the_bezel_stands_on_no_row_that_states_a_word_and_clears_the_margin_on_both_edges() {
    let (facts, band) = installed();
    let mut buffer = Buffer::empty(framed(band.area));

    let placements = super::paint(
        &facts,
        Rail::Ledger,
        band,
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    );
    let area = placements
        .iter()
        .find(|placement| placement.shade().id() == PlaneId::Bezel)
        .expect("a rail that leaves air between its last fact and its foot")
        .cells();

    assert!(area.height > 0, "a bezel of no row frames nothing");
    for y in area.y..area.bottom() {
        assert_eq!(
            glyphs(&buffer, y).trim_end(),
            "",
            "row {y} of the bezel states a word, and a frame stands in the air the rail leaves"
        );
    }
    assert_eq!(
        (area.x, area.right()),
        (band.facts.x + MARGIN, band.facts.right() - MARGIN),
        "the bezel clears the Margin on both edges of the rail it frames"
    );
}

#[test]
fn a_rail_whose_terminal_places_no_picture_is_handed_the_bands_shade_and_no_instrument() {
    let area = band(COLUMNS, 8);
    let mut buffer = Buffer::empty(area);

    let placements = super::paint(
        &spent(),
        Rail::Ledger,
        RailBand::of(area),
        None,
        (None, None),
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        ids(&placements),
        vec![PlaneId::Rail],
        "a terminal that places no picture is handed the band's shading and no instrument: the \
         blocks keep the meters and the cells they drew without one"
    );
}

#[test]
fn a_strip_is_handed_the_bands_shade_and_no_instrument() {
    let rail = Rect {
        x: LEFT,
        ..band(40, 1)
    };
    let mut buffer = Buffer::empty(framed(rail));

    let placements = super::paint(
        &spent(),
        Rail::Strip,
        RailBand::of(rail),
        None,
        (Some(CellSize::NOMINAL), None),
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        ids(&placements),
        vec![PlaneId::Rail],
        "a strip is one row of the rail's own columns and states its facts as words, so it is \
         handed the band's shading and keeps the meters it drew"
    );
}

#[test]
fn a_runs_size_is_spelled_in_tokens_and_never_counted_out() {
    for (size, value) in [
        (0, "0"),
        (948, "948"),
        (999, "999"),
        (1_000, "1k"),
        (48_200, "48.2k"),
        (200_000, "200k"),
        (999_949, "999.9k"),
        (999_950, "1M"),
        (1_020_000, "1.02M"),
        (1_100_000, "1.1M"),
    ] {
        let facts = Facts {
            ctx: Some(Ctx { size, window: None }),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 1));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 1),
            None,
            &mut buffer,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&buffer, 0),
            ctx_line(COLUMNS, value),
            "{size} tokens is drawn as {value} and never as the count itself"
        );
    }
}

#[test]
fn a_run_with_a_window_draws_the_window_and_the_meter_of_the_share_it_fills() {
    for (size, window, value) in [
        (0, 200_000, "░░░░░░ 0 / 200k"),
        (5_000, 200_000, "░░░░░░ 5k / 200k"),
        (48_200, 0, "48.2k"),
        (48_200, 200_000, "█░░░░░ 48.2k / 200k"),
        (100_000, 1_020_000, "█░░░░░ 100k / 1.02M"),
        (200_000, 200_000, "██████ 200k / 200k"),
        (250_000, 200_000, "██████ 250k / 200k"),
    ] {
        let facts = Facts {
            ctx: Some(Ctx {
                size,
                window: Some(window),
            }),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(METERED, 1));

        paint(
            &facts,
            Rail::Ledger,
            band(METERED, 1),
            None,
            &mut buffer,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&buffer, 0),
            ctx_line(METERED, value),
            "{size} tokens against a window of {window} is drawn as {value}: a share under \
                 half a cell of the meter lights none of it, a run past its window has no larger \
                 share to claim, and a window of no tokens is no window at all"
        );
    }
}

#[test]
fn a_nearly_full_window_is_the_one_share_the_rail_raises_its_voice_about() {
    for (size, meter, ink) in [
        (178_000, "█████░", Ink::Plain),
        (180_000, "█████░", Ink::Accent),
        (200_000, "██████", Ink::Accent),
        (240_000, "██████", Ink::Accent),
    ] {
        let facts = Facts {
            ctx: Some(Ctx {
                size,
                window: Some(200_000),
            }),
            ..Facts::default()
        };
        let mut ledger = Buffer::empty(band(COLUMNS, 1));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 1),
            None,
            &mut ledger,
            &PALETTE,
        );

        let filled = meter.chars().filter(|cell| *cell == FULL_CELL).count();
        assert_eq!(
            glyphs(&ledger, 0),
            ctx_line(COLUMNS, &format!("{meter} {} / 200k", tokens(size))),
            "{size} tokens of two hundred thousand fills {filled} of the meter's six cells"
        );
        let at = usize::from(COLUMNS)
            - usize::from(MARGIN)
            - METER
            - 1
            - usize::from(pen::cells("178k / 200k"));
        for cell in 0..METER {
            let column = u16::try_from(at + cell).unwrap_or(u16::MAX);
            let drawn = Ink::of(
                ledger.cell((column, 0)).expect("a cell of the meter"),
                &PALETTE,
            );
            assert_eq!(
                drawn,
                Some(match cell < filled {
                    true => ink,
                    false => Ink::Dim,
                }),
                "cell {cell} of the meter {size} tokens fills is drawn in {drawn:?}: a share \
                     under ninety percent is read in `{}` and one at it in the accent, so a run \
                     one turn from a full window is the one the eye lands on",
                ink.name()
            );
        }

        let mut strip = Buffer::empty(band(40, 1));

        paint(&facts, Rail::Strip, band(40, 1), None, &mut strip, &PALETTE);

        assert_eq!(
            Ink::of(
                strip.cell((4 + MARGIN, 0)).expect("a cell of the meter"),
                &PALETTE,
            ),
            Some(ink),
            "the strip draws the same meter and reads it in the same ink the ledger does: \
                 one threshold, in one place"
        );
    }
}

#[test]
fn a_frame_handed_no_size_draws_no_ctx_row_and_counts_nothing_for_it() {
    let mut unknown = frame();
    unknown.ctx = None;
    let mut roomy = Buffer::empty(band(COLUMNS, 17));

    paint(
        &unknown,
        Rail::Ledger,
        band(COLUMNS, 17),
        None,
        &mut roomy,
        &PALETTE,
    );

    let rows: Vec<String> = (0..17).map(|y| glyphs(&roomy, y)).collect();
    assert_eq!(
        rows[..7],
        [
            "  wire      anthropic           ",
            "  model     claude-sonnet-4-5…  ",
            "  think     high                ",
            "                                ",
            "  turns     3 / 250             ",
            "  cache     88%                 ",
            "                                ",
        ],
        "a frame before the first reply has no size, so it draws no ctx row and the \
             block closes up under the turns"
    );
    assert!(
        !rows.iter().any(|row| row.contains("more")),
        "and the rail gave nothing up, so it counts nothing: no row may say so"
    );

    let band = band(COLUMNS, 8);
    let mut tight = Buffer::empty(band);

    paint(&frame(), Rail::Ledger, band, None, &mut tight, &PALETTE);
    let with = glyphs(&tight, 7);

    paint(&unknown, Rail::Ledger, band, None, &mut tight, &PALETTE);
    let without = glyphs(&tight, 7);

    assert_eq!(
        (with.trim_end(), without.trim_end()),
        ("  +7 more", "  +6 more"),
        "the count is of the facts the frame knew: the size joining the frame is one \
             more fact given up at this height and no fact the rail draws, so its absence \
             leaves the count one shorter and never one longer"
    );
}

#[test]
fn the_ledger_draws_whether_the_model_thinks_below_the_model_it_belongs_to() {
    let facts = Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet")),
        think: Some(String::from("high")),
        ..Facts::default()
    };
    let mut buffer = Buffer::empty(band(COLUMNS, 3));

    paint(
        &facts,
        Rail::Ledger,
        band(COLUMNS, 3),
        None,
        &mut buffer,
        &PALETTE,
    );

    let rows: Vec<String> = (0..3).map(|y| glyphs(&buffer, y)).collect();
    assert_eq!(
        rows,
        [
            "  wire      anthropic           ",
            "  model     claude-sonnet       ",
            "  think     high                ",
        ],
        "the model block holds the fact, one row below the model it is about"
    );
}

#[test]
fn the_ledger_reads_its_blocks_in_the_contracts_order_with_air_between_two() {
    let mut buffer = Buffer::empty(band(COLUMNS, 17));

    paint(
        &frame(),
        Rail::Ledger,
        band(COLUMNS, 17),
        None,
        &mut buffer,
        &PALETTE,
    );

    let rows: Vec<String> = (0..17).map(|y| glyphs(&buffer, y)).collect();
    assert_eq!(
        rows,
        [
            "  wire      anthropic           ",
            "  model     claude-sonnet-4-5…  ",
            "  think     high                ",
            "                                ",
            "  turns     3 / 250             ",
            "  ctx       48.2k               ",
            "  cache     88%                 ",
            "                                ",
            "  workspace …/46-rail           ",
            "  branch    spec/1-capability   ",
            "                                ",
            "  skills    2                   ",
            "                                ",
            "  sandbox   bwrap               ",
            "                                ",
            "  session   9f2c7a41            ",
            "  temp      …/9f2c7a41/tmp      ",
        ],
        "six blocks, an empty row between two of them, nothing given up at this width"
    );
    assert_eq!(
        inks(&buffer, 3, &PALETTE),
        [(Ink::Plain, 32)],
        "the row between two blocks is the rail's own ground and no rule is drawn on it"
    );
    assert_eq!(
        inks(&buffer, 4, &PALETTE),
        [(Ink::Plain, 2), (Ink::Dim, 10), (Ink::Plain, 20)],
        "the margin is the ground's own ink, the label field is `dim`, the ground stands \
             between it and the value, and the value is `plain`"
    );
}

#[test]
fn a_frame_handed_no_wire_draws_no_wire_row() {
    let facts = Facts {
        wire: None,
        model: Some(String::from("claude-sonnet")),
        ..Facts::default()
    };
    let mut buffer = Buffer::empty(band(COLUMNS, 2));

    paint(
        &facts,
        Rail::Ledger,
        band(COLUMNS, 2),
        None,
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        (glyphs(&buffer, 0).trim_end(), glyphs(&buffer, 1).trim_end()),
        ("  model     claude-sonnet", ""),
        "a rail handed no wire has no wire row, and nothing went, so no row counts it"
    );
}

#[test]
fn a_ledger_row_draws_the_wire_by_its_word_and_nothing_beside_it() {
    for (word, row) in [
        ("anthropic", "  wire      anthropic           "),
        ("openai", "  wire      openai              "),
        ("copilot", "  wire      copilot             "),
    ] {
        let facts = Facts {
            wire: Some(String::from(word)),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 1));

        paint(
            &facts,
            Rail::Ledger,
            band(COLUMNS, 1),
            None,
            &mut buffer,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&buffer, 0),
            row,
            "the label in its field and the word after it, with the band's margin either side"
        );
    }
}

#[test]
fn a_fact_with_no_cut_rule_of_its_own_is_dropped_when_no_column_is_left_for_it() {
    let facts = Facts {
        branch: Some(String::from("spec/a-branch-name-that-cannot-fit")),
        ..Facts::default()
    };
    let mut buffer = Buffer::empty(band(COLUMNS, 2));

    paint(
        &facts,
        Rail::Ledger,
        band(COLUMNS, 2),
        None,
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&buffer, 0),
        "  +1 more                       ",
        "a branch wider than the rail's whole column leaves no room for its value, so the \
             fact goes and the row says so"
    );
    assert!(
        !glyphs(&buffer, 0).contains("spec/a"),
        "and never the start of it in the fact's place"
    );
}

#[test]
fn a_served_name_longer_than_the_column_is_drawn_by_its_head_and_its_mark() {
    let facts = Facts {
        model: Some(String::from("claude-sonnet-4-5-20250929")),
        ..Facts::default()
    };
    let mut ledger = Buffer::empty(band(COLUMNS, 1));

    paint(
        &facts,
        Rail::Ledger,
        band(COLUMNS, 1),
        None,
        &mut ledger,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&ledger, 0),
        "  model     claude-sonnet-4-5…  ",
        "a dated name takes the value column whole, and the mark says the date stamp went"
    );

    let mut strip = Buffer::empty(band(40, 1));

    paint(&facts, Rail::Strip, band(40, 1), None, &mut strip, &PALETTE);

    assert_eq!(
        glyphs(&strip, 0).trim_end(),
        "  model claude-sonnet-4-5…",
        "and the strip's copy of the same name is cut the same way"
    );
}

#[test]
fn a_served_name_with_no_segment_to_keep_is_dropped_and_counted() {
    let facts = Facts {
        model: Some(String::from("aservednamethatnobreaksanywhere")),
        ..Facts::default()
    };
    let mut ledger = Buffer::empty(band(COLUMNS, 2));

    paint(
        &facts,
        Rail::Ledger,
        band(COLUMNS, 2),
        None,
        &mut ledger,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&ledger, 0),
        "  +1 more                       ",
        "a name cut to nothing says nothing, so the fact goes and the row counts it"
    );
    assert!(
        !glyphs(&ledger, 0).contains("aserved"),
        "and never the start of it in the fact's place"
    );

    let mut strip = Buffer::empty(band(40, 1));

    paint(&facts, Rail::Strip, band(40, 1), None, &mut strip, &PALETTE);

    assert_eq!(
        glyphs(&strip, 0).trim_end(),
        "  +1 more",
        "and the strip gives up the same fact, and says so in the same words"
    );
}

#[test]
fn a_band_too_short_for_every_block_gives_facts_up_in_the_sacrifice_order() {
    let mut buffer = Buffer::empty(band(COLUMNS, 8));

    paint(
        &frame(),
        Rail::Ledger,
        band(COLUMNS, 8),
        None,
        &mut buffer,
        &PALETTE,
    );

    let rows: Vec<String> = (0..8).map(|y| glyphs(&buffer, y)).collect();
    assert_eq!(
        rows,
        [
            "  wire      anthropic           ",
            "  model     claude-sonnet-4-5…  ",
            "  think     high                ",
            "                                ",
            "  turns     3 / 250             ",
            "                                ",
            "  sandbox   bwrap               ",
            "  +7 more                       ",
        ],
        "temp, cache, session, branch, skills, ctx and workspace went, in that \
             order, and the foot of the rail says seven"
    );
}

#[test]
fn a_band_that_can_only_hold_the_model_block_draws_the_model_without_its_think_row() {
    let mut buffer = Buffer::empty(band(COLUMNS, 4));

    paint(
        &frame(),
        Rail::Ledger,
        band(COLUMNS, 4),
        None,
        &mut buffer,
        &PALETTE,
    );

    let rows: Vec<String> = (0..4).map(|y| glyphs(&buffer, y)).collect();
    assert_eq!(
        rows,
        [
            "  model     claude-sonnet-4-5…  ",
            "                                ",
            "  sandbox   bwrap               ",
            "  +10 more                      ",
        ],
        "thinking is given up before the model it sits under, so the model keeps its row"
    );
}

#[test]
fn a_strip_carries_the_share_of_a_named_window_and_gives_up_the_size_beside_it() {
    let facts = Facts {
        ctx: Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        ..Facts::default()
    };
    let mut ledger = Buffer::empty(band(METERED, 1));

    paint(
        &facts,
        Rail::Ledger,
        band(METERED, 1),
        None,
        &mut ledger,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&ledger, 0),
        "  ctx       █░░░░░ 48.2k / 200k     ",
        "the ledger's ctx row says the size against the window it is a share of, and draws \
             that share as a meter beside them"
    );

    let mut strip = Buffer::empty(band(40, 1));

    paint(&facts, Rail::Strip, band(40, 1), None, &mut strip, &PALETTE);

    assert_eq!(
        glyphs(&strip, 0).trim_end(),
        "  ctx █░░░░░",
        "the strip's one row has no room for the size or the window it is a share of, so \
             it keeps the meter that says how much of the window is left"
    );
}

#[test]
fn forty_columns_hold_the_fact_a_reader_watches_and_the_model_they_are() {
    let mut buffer = Buffer::empty(band(40, 1));

    paint(
        &frame(),
        Rail::Strip,
        band(40, 1),
        None,
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&buffer, 0),
        "  ctx 48.2k · model claude-sonnet-4-5…  ",
        "forty columns hold the run's size and the model, because the four facts this \
             frame carries leave no room for the count beside the two"
    );
}

#[test]
fn ninety_nine_columns_hold_the_four_facts_this_frame_carries_and_count_the_rest() {
    let mut facts = frame();
    facts.ctx = Some(Ctx {
        size: 48_200,
        window: Some(200_000),
    });
    let mut buffer = Buffer::empty(band(99, 1));

    paint(
        &facts,
        Rail::Strip,
        band(99, 1),
        None,
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&buffer, 0).trim_end(),
        format!("  ctx █░░░░░ · model {CUT} · sandbox bwrap · branch {BRANCH} · +8 more"),
        "ninety-nine columns hold every fact this frame's list carries and the count of \
             the eight the strip does not carry at all"
    );
}

#[test]
fn the_strip_gives_its_facts_up_from_the_tail_of_its_own_list() {
    let mut counted = Buffer::empty(band(47, 1));

    paint(
        &frame(),
        Rail::Strip,
        band(47, 1),
        None,
        &mut counted,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&counted, 0).trim_end(),
        "  ctx 48.2k · model claude-sonnet-4-5…",
        "the branch and the sandbox posture went, and the eleven columns the count takes \
             are past the six the row has left beside them"
    );

    let mut facts_win = Buffer::empty(band(60, 1));

    paint(
        &frame(),
        Rail::Strip,
        band(60, 1),
        None,
        &mut facts_win,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&facts_win, 0).trim_end(),
        "  ctx 48.2k · model claude-sonnet-4-5… · sandbox bwrap",
        "three facts beat the count: the room the count would take is the room the \
             sandbox posture is drawn in"
    );
}

#[test]
fn a_strip_with_one_fact_draws_it_and_counts_the_fact_it_does_not_carry() {
    let one = Facts {
        sandbox: Some(Enforcement::Bubblewrap),
        ..Facts::default()
    };
    let mut buffer = Buffer::empty(band(40, 1));

    paint(&one, Rail::Strip, band(40, 1), None, &mut buffer, &PALETTE);

    assert_eq!(
        glyphs(&buffer, 0).trim_end(),
        "  sandbox bwrap",
        "the one fact the frame knows is drawn, and no count stands where nothing went"
    );

    let elsewhere = Facts {
        wire: Some(String::from("anthropic")),
        ..Facts::default()
    };
    let mut buffer = Buffer::empty(band(40, 1));

    paint(
        &elsewhere,
        Rail::Strip,
        band(40, 1),
        None,
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&buffer, 0).trim_end(),
        "  +1 more",
        "a fact the strip does not carry is counted rather than drawn, because the \
             strip's four are the whole of what its row reads"
    );
}

#[test]
fn no_fact_is_ever_drawn_partially_at_any_width_between_twenty_and_two_hundred() {
    for width in 20..=200u16 {
        let rail = Rail::of(width);
        let area = match rail {
            Rail::Ledger => band(COLUMNS, 40),
            Rail::Strip => band(width, 1),
        };
        let mut buffer = Buffer::empty(area);
        paint(&frame(), rail, area, None, &mut buffer, &PALETTE);
        let rows: Vec<String> = (0..area.height).map(|y| glyphs(&buffer, y)).collect();

        let mut went = 0;
        for (label, value) in CONTRACT {
            match rows.iter().find(|row| row.contains(label)) {
                Some(row) => assert!(
                    row.contains(value),
                    "at {width} columns the {label} row reads {row:?}, which is not \
                         the whole {value:?}"
                ),
                None => went += 1,
            }
        }

        let more = format!("+{went} more");
        let screen = rows.join("\n");
        if went == 0 {
            assert!(
                !screen.contains("more"),
                "at {width} columns nothing went, so no row may count what did"
            );
            continue;
        }
        if screen.contains(&more) {
            continue;
        }
        assert_eq!(
            rail,
            Rail::Strip,
            "at {width} columns the ledger dropped the {more} row it had room to draw"
        );
        let drawn = columns(screen.trim_end()).saturating_sub(usize::from(MARGIN));
        assert!(
            drawn + columns(GAP) + columns(&more) > usize::from(width.saturating_sub(margins())),
            "at {width} columns the strip dropped the {more} count it had room to draw \
                 beside the {drawn} columns it kept"
        );
    }
}

#[test]
fn a_strip_row_is_the_four_facts_of_its_list_beside_each_other_with_one_gap() {
    let mut buffer = Buffer::empty(band(220, 1));

    paint(
        &frame(),
        Rail::Strip,
        band(220, 1),
        None,
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&buffer, 0).trim_end(),
        format!("  ctx 48.2k · model {CUT} · sandbox bwrap · branch {BRANCH} · +8 more"),
        "one row, the strip's own four facts in its own order, a gap between two of \
             them, and the count of the eight it does not carry even with the room to"
    );

    let mut narrower = Buffer::empty(band(88, 1));

    paint(
        &frame(),
        Rail::Strip,
        band(88, 1),
        None,
        &mut narrower,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&narrower, 0).trim_end(),
        format!("  ctx 48.2k · model {CUT} · sandbox bwrap · branch {BRANCH}"),
        "eighty-eight columns hold the four facts and no count: the count's own \
             eleven columns are the ones this row does not have"
    );
}

#[test]
fn a_ledger_row_sets_its_label_dim_and_its_value_plain_on_the_field() {
    let facts = Facts {
        model: Some(String::from("claude-sonnet")),
        think: Some(String::from("high")),
        ..Facts::default()
    };
    let mut buffer = Buffer::empty(band(COLUMNS, 2));

    paint(
        &facts,
        Rail::Ledger,
        band(COLUMNS, 2),
        None,
        &mut buffer,
        &PALETTE,
    );

    assert_eq!(
        glyphs(&buffer, 0),
        "  model     claude-sonnet       ",
        "the label in its ten-column field and the value inside the band's margin"
    );
    assert_eq!(
        glyphs(&buffer, 1),
        "  think     high                ",
        "and a value of another width opens in that same column: one field for every label, \
             and no run of blanks or leaders out to the frame's edge"
    );
    assert_eq!(
        inks(&buffer, 0, &PALETTE),
        [(Ink::Plain, 2), (Ink::Dim, 10), (Ink::Plain, 20)],
        "the margin is the ground's own ink, the field is `dim` and the value is the fact, \
             so it is `plain`"
    );
}

#[test]
fn no_width_of_rail_draws_a_run_of_leaders() {
    for width in 20..=200u16 {
        let rail = Rail::of(width);
        let area = match rail {
            Rail::Ledger => band(COLUMNS, 40),
            Rail::Strip => band(width, 1),
        };
        let mut buffer = Buffer::empty(area);

        paint(&frame(), rail, area, None, &mut buffer, &PALETTE);

        let screen: Vec<String> = (0..area.height).map(|y| glyphs(&buffer, y)).collect();
        assert!(
            !screen
                .iter()
                .any(|row| row.contains("..") || row.contains(". ")),
            "at {width} columns a row reads {screen:?}: the space between a label and its \
                 value is blank, and no leader run stands in it"
        );
    }
}

#[test]
fn a_window_is_drawn_as_a_meter_beside_the_size_it_is_a_share_of() {
    let facts = Facts {
        turns: Some((3, 250)),
        ctx: Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        }),
        cache: Some(88),
        ..Facts::default()
    };
    let mut buffer = Buffer::empty(band(METERED, 3));

    paint(
        &facts,
        Rail::Ledger,
        band(METERED, 3),
        None,
        &mut buffer,
        &PALETTE,
    );

    let rows: Vec<String> = (0..3).map(|y| glyphs(&buffer, y)).collect();
    assert_eq!(
        rows,
        [
            "  turns         3 /  250            ",
            "  ctx       █░░░░░ 48.2k / 200k     ",
            "  cache     88%                     ",
        ],
        "a quarter of the window is one cell of the meter and the size is the number beside \
             it, and the hit rate is no meter at all"
    );
    let ahead = u16::try_from(METER + 1).expect("the meter and the space after it fit a row");
    assert_eq!(
        column_of(&rows[0], '/').map(|at| at + ahead),
        column_of(&rows[1], '/'),
        "the two halves of a pair line up: every slash in the block lands one column past its \
         value's halves, and the meter and the space it is followed by are no part of a pair"
    );
}

fn sizing(facts: &Facts, area: Rect, marker: Option<&Pulse>) -> Sizes {
    let rail_band = RailBand::of(area);
    let rows = ledger::rows(facts, Rail::Ledger.into(), rail_band.facts, marker);
    let mut buffer = Buffer::empty(area);
    ledger::draw(&rows, rail_band.facts, &mut buffer, &PALETTE);
    sizes(&rows, rail_band, &mut buffer, &PALETTE)
}

fn run<'a>(laid: &'a Sizes, words: &str) -> &'a Laid {
    laid.laid()
        .iter()
        .find(|laid| laid.run().words() == words)
        .unwrap_or_else(|| panic!("no {words:?} run is laid"))
}

#[test]
fn a_facts_label_is_set_three_quarters_of_a_cell_wide() {
    let laid = sizing(&spent(), band(COLUMNS, 21), None);
    assert_eq!(run(&laid, "input").run().fraction(), Some((3, 4, 2, 0)));
}

#[test]
fn a_dim_run_a_row_does_not_state_is_drawn_plain() {
    let rail = RailBand::of(band(COLUMNS, 1));
    let rows = ledger::rows(&frame(), Rail::Strip.into(), rail.facts, None);
    let mut buffer = Buffer::empty(rail.facts);
    ledger::draw(&rows, rail.facts, &mut buffer, &PALETTE);

    assert!(
        rows.iter()
            .flat_map(|row| row.parts())
            .any(|(_, part)| matches!(part, Part::Run(_, Ink::Dim))),
        "the strip sets its labels and its gaps in the dim ink"
    );
    assert!(
        rows.iter().all(|row| row.big().is_empty())
            && sizes(&rows, rail, &mut buffer, &PALETTE).laid().is_empty(),
        "a run is offered to the terminal only where a row states it as a span: a run written dim \
         that is no span is drawn plain"
    );
}

#[test]
fn a_facts_label_is_set_big_on_the_column_its_field_opens() {
    let laid = sizing(&spent(), band(COLUMNS, 21), None);

    let label = run(&laid, "input");
    assert_eq!(
        label.left(),
        MARGIN,
        "a label is set big on the first column of the field its row opens with"
    );
    assert_eq!(label.run().scale(), 1);
    assert_eq!(label.run().fraction(), Some((3, 4, 2, 0)));
}

#[test]
fn a_span_whose_run_crosses_the_bands_right_margin_is_laid_nowhere() {
    let facts = Facts {
        state: Some(sindri_core::run::State::Running),
        ..Facts::default()
    };

    let wide = sizing(&facts, band(28, 2), None);
    let headline = run(&wide, "running");
    assert_eq!(
        headline.left(),
        MARGIN + FIELD,
        "the state's word opens on the field its label is set in"
    );
    assert!(
        headline.left() + headline.run().width() <= 28 - MARGIN,
        "the run ends on the band's last column but one"
    );

    let narrow = sizing(&facts, band(27, 2), None);
    assert!(
        narrow
            .laid()
            .iter()
            .all(|laid| laid.run().words() != "running"),
        "a run that reaches past the band's right margin is laid nowhere, though the band's \
         own edge would have kept it: {:?}",
        narrow.laid().iter().map(Laid::run).collect::<Vec<_>>()
    );
    assert_eq!(
        run(&narrow, "state").row(),
        0,
        "the row is laid all the same, so it is the run the margin gave up and never the row"
    );
}

#[test]
fn a_two_row_span_on_the_bands_last_row_is_laid_nowhere() {
    let facts = Facts {
        state: Some(sindri_core::run::State::Running),
        ..Facts::default()
    };

    let laid = sizing(&facts, band(28, 1), None);
    assert!(
        laid.laid()
            .iter()
            .all(|laid| laid.run().words() != "running"),
        "a two-row run stands only where the row under it stands inside the band: {:?}",
        laid.laid().iter().map(Laid::run).collect::<Vec<_>>()
    );
    assert_eq!(
        run(&laid, "state").row(),
        0,
        "and the row it would have stood on is still laid"
    );
}

#[test]
fn the_foot_figure_is_set_big_with_its_unit_beside_it() {
    let laid = sizing(&spent(), band(COLUMNS, 21), None);
    let figure = run(&laid, "48.2");
    assert_eq!(figure.run().scale(), 2);
    assert_eq!(run(&laid, "k").left(), figure.left() + figure.run().width());
}

#[test]
fn the_states_word_is_set_big_past_the_mark_in_front_of_it() {
    let facts = Facts {
        state: Some(sindri_core::run::State::Running),
        ..Facts::default()
    };
    let live = Pulse::live(0);
    let laid = sizing(&facts, band(COLUMNS, 8), Some(&live));
    let headline = run(&laid, "running");
    assert_eq!(headline.run().scale(), 2);
    assert_eq!(headline.row(), 0);
    assert_eq!(headline.left(), MARGIN + FIELD + 2);
}

#[test]
fn a_foot_figure_that_states_no_unit_is_set_big_with_nothing_beside_it() {
    let facts = Facts {
        spend: Usage {
            input: Some(900),
            ..spent().spend
        },
        ..spent()
    };
    let laid = sizing(&facts, band(COLUMNS, 21), None);

    let figure = run(&laid, "900");
    assert_eq!(
        figure.run().scale(),
        2,
        "a spend under a thousand is still the foot's figure, so it is still set big"
    );
    assert!(
        laid.laid().iter().all(|one| one.run().words() != "k"),
        "a figure with no unit states none, so nothing stands past the cells it is drawn at"
    );
}

#[test]
fn a_states_word_is_laid_nowhere_over_a_row_already_written() {
    let facts = Facts {
        state: Some(sindri_core::run::State::Running),
        ..Facts::default()
    };
    let rail = RailBand::of(band(COLUMNS, 8));
    let rows = ledger::rows(&facts, Rail::Ledger.into(), rail.facts, None);
    let mut buffer = Buffer::empty(rail.facts);
    ledger::draw(&rows, rail.facts, &mut buffer, &PALETTE);

    let first = sizes(&rows, rail, &mut buffer, &PALETTE);
    let headline = run(&first, "running");
    let (left, under_row) = (headline.left(), headline.row() + 1);
    assert!(
        headline.row() + 1 < rail.facts.height,
        "the word stands over a row inside the band"
    );

    pen::glyph(&mut buffer, left, under_row, 'x', Ink::Plain, &PALETTE);

    let second = sizes(&rows, rail, &mut buffer, &PALETTE);
    assert!(
        second
            .laid()
            .iter()
            .all(|laid| laid.run().words() != "running"),
        "a State's word is laid only over a row left untouched: {:?}",
        second.laid().iter().map(Laid::run).collect::<Vec<_>>()
    );
}
