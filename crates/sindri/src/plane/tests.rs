//! What a band's shade says of the rows it covers, and the images a frame holds of it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::*;
use crate::composer::{MAX_ROWS, MIN_ROWS};
use crate::ink::Ink;
use crate::pen::Clip;
use crate::readback::under;

/// The two spellings that name colours of their own; `SLOTS` wears the reader's theme instead.
const SPELLED: [(&str, Palette); 2] = [("ansi", Palette::ANSI), ("truecolor", Palette::TRUECOLOR)];

/// The columns a band stands in, wide enough for every column these tests claim.
const COLUMNS: u16 = 80;

/// The cell the test terminals these tests speak of draw at.
const CELL: CellSize = CellSize::NOMINAL;

/// The composer's band of `rows` rows in `palette`, with no plane placed under it.
fn raised(rows: u16, palette: &Palette) -> Shade {
    Shade::composer(Rect::new(0, 0, COLUMNS, rows), None, None, false, palette)
        .expect("a band of rows")
}

/// The relative luminance of a colour, from the channels it is spelled in.
fn luminance(colour: Color) -> f64 {
    let [red, green, blue] = channels(colour)
        .expect("a colour in pixels")
        .map(|channel| {
            let channel = f64::from(channel) / 255.0;
            match channel <= 0.03928 {
                true => channel / 12.92,
                false => ((channel + 0.055) / 1.055).powf(2.4),
            }
        });
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

/// The images a frame holds of the shades one pass hands it.
fn held(shades: &[Shade]) -> Planes {
    let mut planes = Planes::default();
    planes.hold(shades);
    planes
}

/// The pixel at the middle of `row`'s cells in a plane drawn at `cell`, read as its channels.
fn middle(plane: &Plane, row: u16, cell: CellSize) -> [u8; 3] {
    let x = plane.pixels().width() / 2;
    let y = u32::from(row) * cell.height() + cell.height() / 2;
    let pixel = plane.pixels().get_pixel(x, y);
    [pixel[0], pixel[1], pixel[2]]
}

/// Whether two colours are within one step of each other, channel by channel.
fn near(left: [u8; 3], right: [u8; 3]) -> bool {
    (0..3).all(|channel| left[channel].abs_diff(right[channel]) <= 1)
}

/// The channels of a colour the palette spells, and never a slot of the reader's own.
fn spelled(colour: Color) -> [u8; 3] {
    channels(colour).expect("a colour in pixels")
}

/// The plane a shade is drawn as, and the band it covers.
fn drawn(shade: Shade) -> Plane {
    Plane::draw(shade).expect("a band a plane is drawn under")
}

#[test]
fn a_raised_band_runs_from_the_composers_tone_down_to_the_rails_own() {
    for (spelling, palette) in SPELLED {
        for rows in MIN_ROWS..=MAX_ROWS {
            assert_eq!(
                raised(rows, &palette).row(0),
                palette.band(Band::Composer),
                "{spelling} draws the top row of a raised band of {rows} rows on a tone other \
                     than the composer's, the depth above the rail's"
            );
            assert_eq!(
                raised(rows, &palette).row(rows - 1),
                palette.band(Band::Rail),
                "{spelling} draws the last row of a raised band of {rows} rows on a tone other \
                     than the rail's own"
            );
        }
    }
}

#[test]
fn a_one_row_band_has_no_room_for_both_ends_and_is_drawn_as_its_head() {
    for (spelling, palette) in SPELLED {
        assert_eq!(
            raised(1, &palette).row(0),
            palette.band(Band::Composer),
            "{spelling} draws a one-row band on anything but its head, and the one row is all \
                 the band has to say it is raised"
        );
    }
}

#[test]
fn a_raised_bands_rows_never_climb_and_never_leave_its_two_ends() {
    for (spelling, palette) in SPELLED {
        let head = luminance(palette.band(Band::Composer));
        let foot = luminance(palette.band(Band::Rail));
        for rows in MIN_ROWS..=MAX_ROWS {
            let band = raised(rows, &palette);
            for row in 0..rows {
                let ground = luminance(band.row(row));
                assert!(
                    (foot..=head).contains(&ground),
                    "{spelling} draws row {row} of a raised band of {rows} rows at {ground}, \
                         outside the two ends {foot} and {head} the band is shaded between"
                );
                if row > 0 {
                    assert!(
                        ground <= luminance(band.row(row - 1)),
                        "{spelling} draws row {row} of a raised band of {rows} rows brighter \
                             than the row above it, so the surface climbs away from its foot"
                    );
                }
            }
        }
    }
}

#[test]
fn a_truecolour_band_up_to_its_ceiling_draws_no_two_rows_in_one_ground() {
    for rows in MIN_ROWS..=MAX_ROWS {
        let band = raised(rows, &Palette::TRUECOLOR);
        for row in 1..rows {
            assert_ne!(
                band.row(row),
                band.row(row - 1),
                "truecolour draws rows {} and {row} of a {rows}-row band in one ground, and \
                     the design has a shade to spare for every row up to the ceiling",
                row - 1
            );
        }
    }
}

#[test]
fn an_ansi_band_has_only_the_two_greys_its_two_ends_leave_it() {
    for rows in MIN_ROWS..=MAX_ROWS {
        let band = raised(rows, &Palette::ANSI);
        for row in 0..rows {
            assert!(
                matches!(band.row(row), Color::Indexed(235 | 236)),
                "ansi draws row {row} of a {rows}-row band in {:?}, outside the two greys the \
                     rail's tone and the composer's leave between them",
                band.row(row)
            );
        }
    }
}

#[test]
fn no_row_of_a_raised_band_is_darker_than_the_well() {
    for (spelling, palette) in SPELLED {
        let well = luminance(palette.band(Band::Well));
        for rows in MIN_ROWS..=MAX_ROWS {
            let band = raised(rows, &palette);
            for row in 0..rows {
                assert!(
                    luminance(band.row(row)) >= well,
                    "{spelling} draws row {row} of a raised band of {rows} rows under the well, \
                         so the band sits sunken and not raised"
                );
            }
        }
    }
}

#[test]
fn a_surface_drawn_in_no_colour_ramps_nowhere() {
    for rows in MIN_ROWS..=MAX_ROWS {
        let band = raised(rows, &Palette::SLOTS);
        for row in 0..rows {
            assert_eq!(
                band.row(row),
                Color::Reset,
                "row {row} of a raised band of {rows} rows draws no ground, so the frame stays \
                     flat"
            );
        }
    }
}

#[test]
fn a_bands_plane_is_its_shade_sampled_at_the_cell() {
    for (spelling, palette) in SPELLED {
        let bands = [
            (
                "the ledger's rail",
                Shade::rail(Rail::Ledger, Rect::new(6, 2, 32, 8), Some(CELL), &palette),
            ),
            (
                "a strip",
                Shade::rail(Rail::Strip, Rect::new(6, 2, 32, 1), Some(CELL), &palette),
            ),
            (
                "the composer's band",
                Shade::composer(Rect::new(0, 20, 80, 5), Some(CELL), None, false, &palette),
            ),
        ];
        for (band, shade) in bands {
            let shade = shade.expect("a band to shade");
            let plane = drawn(shade);
            assert_eq!(
                plane.area(),
                shade.area(),
                "{spelling} draws {band}'s plane over a rect of its own, and a plane covers the \
                 band whose shade it is drawn from"
            );
            assert_eq!(
                plane.pixels().dimensions(),
                (
                    u32::from(shade.area().width) * CELL.width(),
                    u32::from(shade.area().height) * CELL.height()
                ),
                "{spelling} draws {band}'s plane at other than the pixels of the cell it is drawn on"
            );
            for row in 0..shade.area().height {
                let pixel = middle(&plane, row, CELL);
                let said = spelled(shade.row(row));
                assert!(
                    near(pixel, said),
                    "{spelling} draws the middle of row {row} of {band} at {pixel:?} where the \
                     shade says {said:?}: a plane is the shade sampled at the cell, and no colour \
                     of its own"
                );
            }
        }
    }
}

#[test]
fn a_composers_plane_opens_on_its_head_and_covers_no_pixel_of_the_well() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let plane = drawn(
            Shade::composer(area, Some(CELL), None, false, &palette).expect("a band to shade"),
        );
        let head = spelled(palette.band(Band::Composer));
        let foot = spelled(palette.band(Band::Rail));
        let well = spelled(palette.band(Band::Well));

        let top = middle(&plane, 0, CELL);
        assert!(
            near(top, head),
            "{spelling} opens the composer's plane on {top:?} where the band's head is {head:?}, \
             so no pixel row of the well is drawn above the band"
        );
        assert_ne!(
            top, well,
            "{spelling} draws the well over the top cell of the composer's band, which stands on \
             its own head and not on the band under it"
        );
        assert!(
            near(middle(&plane, area.height - 1, CELL), foot),
            "{spelling} closes the composer's plane on {:?} where the rail's own tone {foot:?} is \
             the foot it falls to",
            middle(&plane, area.height - 1, CELL)
        );
    }
}

#[test]
fn a_composers_plane_opens_on_the_light_of_a_head_that_takes_the_keys() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let plane = drawn(
            Shade::composer(area, Some(CELL), None, true, &palette).expect("a band to shade"),
        );

        assert!(
            near(middle(&plane, 0, CELL), spelled(palette.edge(Edge::Lit))),
            "{spelling} opens the plane of a band that takes the keys on {:?} where its head row \
             is the lit edge",
            middle(&plane, 0, CELL)
        );
    }
}

#[test]
fn a_strips_plane_is_the_one_row_it_has() {
    for (spelling, palette) in SPELLED {
        let plane = drawn(
            Shade::rail(Rail::Strip, Rect::new(6, 2, 32, 1), Some(CELL), &palette)
                .expect("a band to shade"),
        );
        let (width, height) = plane.pixels().dimensions();
        let tone = spelled(palette.band(Band::Rail));

        assert_eq!(
            height,
            CELL.height(),
            "{spelling} draws a strip's plane over more pixel rows than the one row it was handed"
        );
        for y in 0..height {
            for x in 0..width {
                let pixel = plane.pixels().get_pixel(x, y);
                assert_eq!(
                    [pixel[0], pixel[1], pixel[2]],
                    tone,
                    "{spelling} draws pixel row {y}, column {x} of a strip at something other \
                     than the rail's own tone: one row has no fall to draw and no shadow in it"
                );
            }
        }
    }
}

#[test]
fn the_carets_glow_is_the_one_light_on_every_pixel_row_of_the_column_it_waits_in() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 20, 80, 5);
    let caret = 12;
    let plane = drawn(
        Shade::composer(area, Some(CELL), Some(caret), false, &palette).expect("a band to shade"),
    );
    let (width, _) = plane.pixels().dimensions();
    let lit = u32::from(caret) * CELL.width() + CELL.width() / 2;
    let (far, height) = (width - 1, plane.pixels().height());
    let step = |y: u32| {
        let light = plane.pixels().get_pixel(lit, y);
        let dark = plane.pixels().get_pixel(far, y);
        [0, 1, 2].map(|channel| light[channel].saturating_sub(dark[channel]))
    };

    let waited = step(0);
    assert!(
        waited.iter().any(|channel| *channel > 0),
        "the column the caret waits in is no lighter than the band's far side on its first pixel \
         row: {waited:?}"
    );
    for y in 1..height {
        let row = step(y);
        for channel in 0..3 {
            assert!(
                row[channel].abs_diff(waited[channel]) <= 1,
                "channel {channel} of the light over the caret is {} on pixel row {y} and {} on \
                 the row above it, so the glow follows the caret's column alone",
                row[channel],
                waited[channel]
            );
        }
    }
}

#[test]
fn a_bands_glow_dies_away_across_the_cells_away_from_the_caret() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 20, 80, 5);
    let caret = 4;
    let plane = drawn(
        Shade::composer(area, Some(CELL), Some(caret), true, &palette).expect("a band to shade"),
    );
    let row = plane.pixels().height() / 2;
    let waited = u32::from(caret) * CELL.width() + CELL.width() / 2;
    let mut last = u8::MAX;
    for x in 0..plane.pixels().width() {
        let pixel = plane.pixels().get_pixel(x, row);
        assert!(
            pixel[0] <= last || x <= waited,
            "the band at pixel {x} of row {row} is lighter than the pixel before it, so the light \
             does not fall away from the caret"
        );
        last = pixel[0];
    }
    let lit = plane.pixels().get_pixel(waited, row);
    let far = plane.pixels().get_pixel(plane.pixels().width() - 1, row);
    assert!(
        lit[0] > far[0],
        "the cell the caret waits in is no lighter than the band's far side: {lit:?} against \
         {far:?}"
    );
}

#[test]
fn the_transcript_band_is_claimed_on_the_well_and_makes_no_seam() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, 20, 4);
    let shade = Shade::transcript(area, &palette).expect("a band of rows");
    let mut buffer = Buffer::empty(area);

    assert!(
        !shade.places(),
        "no plane stands under the transcript band, so its own rows are the whole of its ground"
    );
    shade.claim(&mut buffer, &palette);

    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, y),
                palette.band(Band::Well),
                "row {y}, column {x} of the transcript band is not the well it is read on: a \
                 band on the well meets no band in its own columns and lights none of them"
            );
            assert_eq!(
                Ink::of(buffer.cell((x, y)).expect("a cell of the band"), &palette),
                Some(Ink::Plain),
                "row {y}, column {x} of the well is claimed in an ink other than the frame's own"
            );
        }
    }
}

#[test]
fn a_rail_drawn_in_cells_is_its_tone_down_to_the_shadow_across_its_last_row() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(20, 0, 32, 8);
    let shade = Shade::rail(Rail::Ledger, area, None, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(Rect {
        x: 0,
        width: area.right(),
        ..area
    });

    assert!(
        !shade.places(),
        "a terminal that places no picture is handed no plane under the rail"
    );
    shade.claim(&mut buffer, &palette);

    for y in area.y..area.bottom() - 1 {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, y),
                palette.band(Band::Rail),
                "row {y}, column {x} of the band is not the rail's own tone: the rail's first \
                 column is no lighter than the rest of it, because the seam is what parts it from \
                 the transcript"
            );
        }
    }
    for x in area.x..area.right() {
        assert_eq!(
            under(&buffer, x, area.bottom() - 1),
            palette.edge(Edge::Shadow),
            "column {x} of the band's last row is not the shadow the band falls to"
        );
    }
}

#[test]
fn a_rails_plane_covers_the_rails_own_columns_and_no_column_of_the_transcript() {
    let palette = Palette::TRUECOLOR;
    let band = Rect::new(COLUMNS / 4, 0, 32, 8);
    let shade = Shade::rail(Rail::Ledger, band, Some(CELL), &palette).expect("a band to shade");
    let frame = Rect::new(0, 0, COLUMNS, band.height);
    let mut buffer = Buffer::empty(frame);

    assert_eq!(
        shade.area(),
        band,
        "the rail's plane covers the rail's own columns, and the columns the seam reaches back \
         over are the transcript band's"
    );
    pen::claim(&mut buffer, frame, palette.ground(Some(Band::Well)));
    shade.claim(&mut buffer, &palette);

    for x in band.x - SEAM..band.x {
        assert_eq!(
            under(&buffer, x, 0),
            palette.band(Band::Well),
            "column {x} of the columns the seam reaches back over is not the transcript band's \
             own: no claim of the rail's is made past the columns it is drawn in"
        );
    }
    for y in band.y..band.bottom() {
        for x in band.x..band.right() {
            assert_eq!(
                under(&buffer, x, y),
                palette.ground(None).bg.expect("the terminal's own ground"),
                "row {y}, column {x} of the band is not the terminal's own ground, which its \
                 plane is read through"
            );
        }
    }
}

#[test]
fn a_strip_drawn_in_cells_makes_no_seam_at_all() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(20, 0, 40, 1);
    let shade = Shade::rail(Rail::Strip, area, None, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(Rect {
        x: 0,
        width: area.right(),
        ..area
    });

    assert!(
        !shade.places(),
        "a strip is one row of the rail's own cells"
    );
    assert!(
        shade.seam().is_none(),
        "a strip lies under the transcript and parts nothing, so it makes no seam"
    );
    shade.claim(&mut buffer, &palette);

    for x in area.x..area.right() {
        assert_eq!(
            under(&buffer, x, area.y),
            palette.band(Band::Rail),
            "column {x} of the strip is not the rail's own tone: one row has no room for the \
             shadow the band falls to, and no boundary to light"
        );
    }
}

#[test]
fn the_composers_band_is_claimed_row_by_row_from_its_shade() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, 40, 6);
    let shade = Shade::composer(area, None, None, false, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(area);

    assert!(
        !shade.places(),
        "a terminal that places no picture is handed no plane under the band"
    );
    shade.claim(&mut buffer, &palette);

    for row in 0..area.height {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, area.y + row),
                shade.row(row),
                "column {x} of row {row} is not the shade the band's own rows sit on"
            );
        }
    }
}

#[test]
fn a_band_that_takes_the_keys_is_claimed_on_a_head_row_of_light() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, 40, 6);
    let band = Shade::composer(area, None, None, true, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(area);

    assert_eq!(
        band.row(0),
        palette.edge(Edge::Lit),
        "the head row of a band that takes the keys is the light it meets the frame on"
    );
    band.claim(&mut buffer, &palette);

    for x in area.x..area.right() {
        assert_eq!(
            under(&buffer, x, area.y),
            palette.edge(Edge::Lit),
            "column {x} of the head row is not the light the band takes the keys in"
        );
        assert_eq!(
            under(&buffer, x, area.y + 1),
            band.row(1),
            "column {x} of the row under the head is not the ground the draft is drawn on"
        );
    }
}

#[test]
fn a_band_a_plane_stands_under_is_claimed_on_the_terminals_own_ground() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, 40, 6);
    let shade = Shade::composer(area, Some(CELL), None, true, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(area);

    assert!(
        shade.places(),
        "a terminal that places a picture and a palette that spells a pixel is handed a plane"
    );
    shade.claim(&mut buffer, &palette);

    for row in 0..area.height {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, area.y + row),
                palette.ground(None).bg.expect("the terminal's own ground"),
                "column {x} of row {row} is not the terminal's own ground, so the image drawn \
                 under it is read through a cell that is not its own"
            );
        }
    }
}

#[test]
fn a_ledgers_seam_reaches_the_columns_beside_it_and_falls_from_the_well_into_the_shadow() {
    let palette = Palette::TRUECOLOR;
    let rail = Rect::new(20, 2, 32, 8);
    let shade = Shade::rail(Rail::Ledger, rail, None, &palette).expect("a band to shade");
    let seam = shade
        .seam()
        .expect("a ledger beside a transcript band meets it in a seam");

    assert_eq!(
        seam.area(),
        Rect {
            x: rail.x - SEAM,
            width: SEAM,
            ..rail
        },
        "the seam reaches the {SEAM} columns of the transcript band beside the rail, over the \
         rail's own rows"
    );
    assert_eq!(
        seam.column(seam.area().x),
        palette.band(Band::Well),
        "the seam opens out of the well the transcript band is read on"
    );
    assert_eq!(
        seam.column(rail.x - 1),
        palette.edge(Edge::Shadow),
        "and is exactly the shadow at its last column, which is where the rail comes out of it"
    );
    let (well, shadow) = (
        spelled(palette.band(Band::Well)),
        spelled(palette.edge(Edge::Shadow)),
    );
    for x in seam.area().x + 1..rail.x - 1 {
        let column = spelled(seam.column(x));
        for channel in 0..3 {
            assert!(
                column[channel] < well[channel] && column[channel] > shadow[channel],
                "channel {channel} of column {x} of the seam is {} and not a step between the well \
                 {well:?} and the shadow {shadow:?} it falls through",
                column[channel]
            );
        }
    }
}

#[test]
fn a_seam_reaches_no_column_the_rail_has_none_to_its_left() {
    let palette = Palette::TRUECOLOR;
    let rail = Rect::new(2, 0, 32, 8);
    let shade = Shade::rail(Rail::Ledger, rail, None, &palette).expect("a band to shade");
    let seam = shade.seam().expect("a ledger with a column beside it");

    assert_eq!(
        seam.area(),
        Rect {
            x: 0,
            width: 2,
            ..rail
        },
        "a seam takes the two columns the frame has left of a rail stood at its edge, and never a \
         column the frame does not reach"
    );
    assert!(
        Shade::rail(Rail::Ledger, Rect { x: 0, ..rail }, None, &palette)
            .expect("a band to shade")
            .seam()
            .is_none(),
        "a rail in the frame's first column has no column beside it and makes no seam"
    );
}

#[test]
fn a_palette_that_draws_no_colour_makes_no_seam() {
    let rail = Rect::new(20, 0, 32, 8);
    let shade = Shade::rail(Rail::Ledger, rail, None, &Palette::SLOTS).expect("a band to shade");

    assert!(
        shade.seam().is_none(),
        "a palette that draws no colour leaves every band on the terminal's own ground, so the \
         rail and the transcript are parted by no fall"
    );
}

#[test]
fn a_seam_claims_its_columns_under_the_words_already_drawn_there() {
    let palette = Palette::TRUECOLOR;
    let rail = Rect::new(20, 2, 32, 3);
    let seam = Shade::rail(Rail::Ledger, rail, None, &palette)
        .expect("a band to shade")
        .seam()
        .expect("a ledger beside a transcript band meets it in a seam");
    let area = seam.area();
    let frame = Rect {
        x: 0,
        width: rail.right(),
        ..rail
    };
    let mut buffer = Buffer::empty(frame);
    let words = "the words a row wrote across the seam";

    pen::claim(&mut buffer, frame, palette.ground(Some(Band::Well)));
    pen::write(
        &mut buffer,
        Rect { height: 1, ..area },
        &[(words, Ink::Dim)],
        Clip::Edge,
        &palette,
    );
    seam.claim(&mut buffer, &palette);

    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, y),
                seam.column(x),
                "row {y}, column {x} of the seam is not the shade that column falls in: the seam \
                 is drawn where the rail parts the transcript, and it reaches the rail's rows"
            );
        }
    }
    let written: String = (area.x..area.right())
        .map(|x| {
            buffer
                .cell((x, area.y))
                .expect("a cell of the seam")
                .symbol()
                .to_owned()
        })
        .collect();
    assert_eq!(
        written,
        words[..usize::from(area.width)].to_owned(),
        "the seam is claimed under the words of the row and cuts none of them"
    );
    for x in area.x..area.right() {
        assert_eq!(
            Ink::of(
                buffer.cell((x, area.y)).expect("a cell of the seam"),
                &palette
            ),
            Some(Ink::Dim),
            "column {x} of the seam keeps the ink the row wrote it in, because a ground under a \
             cell takes no ink of its own"
        );
    }
}

#[test]
fn a_band_whose_shade_did_not_move_makes_no_image() {
    let area = Rect::new(2, 3, 40, 4);
    let shade = Shade::composer(area, Some(CELL), None, false, &Palette::TRUECOLOR)
        .expect("a band to shade");
    let mut planes = held(&[shade]);

    assert_eq!(planes.made(), 1, "the first pass makes the band's image");
    planes.hold(&[shade]);
    planes.hold(&[shade]);

    assert_eq!(
        planes.made(),
        1,
        "a pass that shades the band it shaded before makes no image of it"
    );
    assert_eq!(planes.held().len(), 1, "and the frame still holds the one");
}

#[test]
fn a_band_no_plane_stands_under_makes_no_image() {
    let area = Rect::new(0, 0, 40, 4);
    let transcript = Shade::transcript(area, &Palette::TRUECOLOR).expect("a band of rows");
    let composer =
        Shade::composer(area, None, None, false, &Palette::TRUECOLOR).expect("a band to shade");
    let planes = held(&[transcript, composer]);

    assert_eq!(
        planes.made(),
        0,
        "a band drawn in cells alone is drawn as no image at all"
    );
    assert!(
        planes.held().is_empty(),
        "and the frame holds nothing against it: {:?}",
        planes.held()
    );
}

#[test]
fn a_composer_band_whose_caret_moved_makes_its_image_again() {
    let area = Rect::new(2, 3, 40, 4);
    let composer = |caret| {
        Shade::composer(area, Some(CELL), Some(caret), false, &Palette::TRUECOLOR)
            .expect("a band to shade")
    };
    let mut planes = held(&[composer(6)]);
    let made = planes.made();

    planes.hold(&[composer(7)]);

    assert_eq!(
        planes.made(),
        made + 1,
        "the glow is drawn under the caret's column, so a band whose caret moved is drawn again"
    );
}
