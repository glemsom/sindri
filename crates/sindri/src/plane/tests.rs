//! What a band's shade says of the rows it covers, and the images a frame holds of it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::*;
use crate::composer::{MAX_ROWS, MIN_ROWS};
use crate::ink::Ink;
use crate::readback::under;

/// The two spellings that name colours of their own; `SLOTS` wears the reader's theme instead.
const SPELLED: [(&str, Palette); 2] = [("ansi", Palette::ANSI), ("truecolor", Palette::TRUECOLOR)];

/// The columns a band stands in, wide enough for every row these tests claim.
const COLUMNS: u16 = 80;

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
fn a_rail_drawn_in_cells_lights_its_first_column_and_shadows_its_last_row() {
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
        assert_eq!(
            under(&buffer, area.x, y),
            palette.edge(Edge::Lit),
            "row {y} of the ledger's first column is not the light it meets the transcript on"
        );
        for x in area.x + 1..area.right() {
            assert_eq!(
                under(&buffer, x, y),
                palette.band(Band::Rail),
                "row {y}, column {x} of the band is not the rail's own tone"
            );
        }
    }
    for x in area.x..area.right() {
        assert_eq!(
            under(&buffer, x, area.bottom() - 1),
            palette.edge(Edge::Shadow),
            "column {x} of the band's last row is not the shadow the band falls to, which the \
             lit column of the corner gives way to"
        );
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
fn the_composers_head_row_is_claimed_in_the_light_it_takes_the_keys_in() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, 40, 6);
    let band = Shade::composer(area, None, None, true, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(area);

    band.claim(&mut buffer, &palette);

    for x in area.x..area.right() {
        assert_eq!(
            under(&buffer, x, area.y),
            palette.edge(Edge::Lit),
            "column {x} of the head row is not the light a band that takes the keys meets the \
             frame on, which is a boundary and never the band's own ground"
        );
        assert_ne!(
            under(&buffer, x, area.y),
            band.row(0),
            "and it is claimed over the band's first row rather than in it"
        );
    }
    for x in area.x..area.right() {
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
    let shade = Shade::composer(area, Some(CellSize::NOMINAL), None, true, &palette)
        .expect("a band to shade");
    let mut buffer = Buffer::empty(area);

    assert!(
        shade.places(),
        "a terminal that places a picture and a palette that spells a pixel is handed a plane"
    );
    shade.claim(&mut buffer, &palette);

    for row in 1..area.height {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, area.y + row),
                palette.ground(None).bg.expect("the terminal's own ground"),
                "column {x} of row {row} is not the terminal's own ground, so the image drawn \
                 under it is read through a cell that is not its own"
            );
        }
    }
    for x in area.x..area.right() {
        assert_eq!(
            under(&buffer, x, area.y),
            palette.edge(Edge::Lit),
            "column {x} of the head row is still the light the band takes the keys in, claimed \
             over the plane as well"
        );
    }
}

#[test]
fn a_band_whose_shade_did_not_move_makes_no_image() {
    let area = Rect::new(2, 3, 40, 4);
    let shade = Shade::composer(
        area,
        Some(CellSize::NOMINAL),
        None,
        false,
        &Palette::TRUECOLOR,
    )
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
    let mut planes = held(&[transcript, composer]);

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
        Shade::composer(
            area,
            Some(CellSize::NOMINAL),
            Some(caret),
            false,
            &Palette::TRUECOLOR,
        )
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
