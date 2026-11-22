//! What a band's shade says of the rows it covers, and the images a frame holds of it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::*;
use crate::composer::{MAX_ROWS, MIN_ROWS};
use crate::ink::Ink;
use crate::pen::Clip;
use crate::readback::under;
use crate::rows::MARGIN;

/// The two spellings that name colours of their own; `SLOTS` wears the reader's theme instead.
const SPELLED: [(&str, Palette); 2] = [("ansi", Palette::ANSI), ("truecolor", Palette::TRUECOLOR)];

/// The columns a band stands in, wide enough for every column these tests claim.
const COLUMNS: u16 = 80;

/// The cell the test terminals these tests speak of draw at.
const CELL: CellSize = CellSize::NOMINAL;

/// The composer's band of `rows` rows in `palette`, with no plane placed under it.
fn shaded(rows: u16, palette: &Palette) -> Shade {
    Shade::composer(Rect::new(0, 0, COLUMNS, rows), None, None, palette).expect("a band of rows")
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

/// The three bands a frame draws a plane under, each against the name a failure is reported in.
fn bands(palette: &Palette) -> [(&'static str, Shade); 3] {
    [
        (
            "the ledger's rail",
            Shade::rail(Rail::Ledger, Rect::new(6, 2, 32, 8), Some(CELL), palette)
                .expect("a band to shade"),
        ),
        (
            "a strip",
            Shade::rail(Rail::Strip, Rect::new(6, 2, 32, 1), Some(CELL), palette)
                .expect("a band to shade"),
        ),
        (
            "the composer's band",
            Shade::composer(Rect::new(0, 20, 80, 5), Some(CELL), None, palette)
                .expect("a band to shade"),
        ),
    ]
}

#[test]
fn the_bands_band_is_the_prompts_tone_but_its_last_row_which_closes_on_the_rails() {
    for (spelling, palette) in SPELLED {
        for rows in MIN_ROWS..=MAX_ROWS {
            let band = shaded(rows, &palette);
            for row in 0..rows - 1 {
                assert_eq!(
                    band.row(row),
                    palette.band(Band::Prompt),
                    "{spelling} draws row {row} of a {rows}-row band on a tone other than the \
                     prompt's own, and the band the draft is typed in is the band a prompt is \
                     read on"
                );
            }
            assert_eq!(
                band.row(rows - 1),
                palette.band(Band::Rail),
                "{spelling} closes a {rows}-row band on a tone other than the rail's own, so the \
                 band is parted from the surface it stands on"
            );
        }
    }
}

#[test]
fn a_one_row_band_is_the_bands_last_row_and_closes_on_the_rails_tone() {
    for (spelling, palette) in SPELLED {
        assert_eq!(
            shaded(1, &palette).row(0),
            palette.band(Band::Rail),
            "{spelling} draws the one row of a one-row band on anything but the tone it closes \
             on, and the closing row is the last of the band"
        );
    }
}

#[test]
fn no_row_of_the_bands_band_is_a_blend_of_the_two_tones_it_closes_through() {
    for (spelling, palette) in SPELLED {
        let (prompt, rail) = (palette.band(Band::Prompt), palette.band(Band::Rail));
        for rows in MIN_ROWS..=MAX_ROWS {
            let band = shaded(rows, &palette);
            for row in 0..rows {
                let ground = band.row(row);
                assert!(
                    ground == prompt || ground == rail,
                    "{spelling} draws row {row} of a {rows}-row band in {ground:?}, a tone \
                     between the two it is drawn in: the band rises out of the well in whole rows \
                     and closes into the rail's tone in one"
                );
            }
        }
    }
}

#[test]
fn no_row_of_the_bands_band_is_darker_than_the_well() {
    for (spelling, palette) in SPELLED {
        let well = luminance(palette.band(Band::Well));
        for rows in MIN_ROWS..=MAX_ROWS {
            let band = shaded(rows, &palette);
            for row in 0..rows {
                assert!(
                    luminance(band.row(row)) >= well,
                    "{spelling} draws row {row} of a {rows}-row band under the well, so the band \
                         sits sunken and not over the well it is typed above"
                );
            }
        }
    }
}

#[test]
fn a_band_drawn_in_no_colour_draws_no_ground_on_any_row() {
    for rows in MIN_ROWS..=MAX_ROWS {
        let band = shaded(rows, &Palette::SLOTS);
        for row in 0..rows {
            assert_eq!(
                band.row(row),
                Color::Reset,
                "row {row} of a {rows}-row band draws a ground, and a palette that asked for no \
                 colour has none to draw"
            );
        }
    }
}

#[test]
fn a_bands_plane_is_its_shade_sampled_at_the_cell() {
    for (spelling, palette) in SPELLED {
        for (band, shade) in bands(&palette) {
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
fn a_composers_plane_opens_on_the_prompts_band_and_covers_no_pixel_of_the_well() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let plane =
            drawn(Shade::composer(area, Some(CELL), None, &palette).expect("a band to shade"));
        let prompt = spelled(palette.band(Band::Prompt));
        let rail = spelled(palette.band(Band::Rail));
        let well = spelled(palette.band(Band::Well));

        let top = middle(&plane, 0, CELL);
        assert!(
            near(top, prompt),
            "{spelling} opens the composer's plane on {top:?} where the prompt's own band is \
             {prompt:?}, so no pixel row of the well is drawn above the band"
        );
        assert_ne!(
            top, well,
            "{spelling} draws the well over the top cell of the composer's band, which stands on \
             the prompt's band and not on the band under it"
        );
        assert!(
            near(middle(&plane, area.height - 1, CELL), rail),
            "{spelling} closes the composer's plane on {:?} where the rail's own tone {rail:?} \
             is the tone it closes on",
            middle(&plane, area.height - 1, CELL)
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
    let plane =
        drawn(Shade::composer(area, Some(CELL), Some(caret), &palette).expect("a band to shade"));
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
fn the_carets_glow_is_the_only_blend_a_shade_makes() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let caret = 12;
        let shade =
            Shade::composer(area, Some(CELL), Some(caret), &palette).expect("a band to shade");
        let plane = drawn(shade);
        let (width, height) = plane.pixels().dimensions();
        let down = CELL.height();

        for x in 0..width {
            let lift = |y: u32| {
                let ground = spelled(shade.row((y / down) as u16));
                let pixel = plane.pixels().get_pixel(x, y);
                [0, 1, 2].map(|channel| i32::from(pixel[channel]) - i32::from(ground[channel]))
            };
            let waited = lift(0);

            assert!(
                waited.iter().all(|channel| *channel >= 0),
                "{spelling} draws column {x} of the band below the row it falls in on its first \
                 pixel row: {waited:?}, and the glow only ever adds light"
            );
            for y in 1..height {
                let row = lift(y);
                assert_eq!(
                    row, waited,
                    "{spelling} draws column {x} of pixel row {y} {row:?} over the row it falls \
                     in where the first pixel row reads {waited:?}: the caret's glow is the only \
                     blend a shade makes, and it is the same on every pixel row of its column"
                );
            }
        }
    }
}

#[test]
fn no_row_of_a_shades_plane_is_a_blend_of_the_one_beside_it() {
    for (spelling, palette) in SPELLED {
        for (band, shade) in bands(&palette) {
            let plane = drawn(shade);
            let (width, height) = plane.pixels().dimensions();
            let down = CELL.height();

            for y in 0..height {
                let ground = spelled(shade.row((y / down) as u16));
                for x in 0..width {
                    let pixel = plane.pixels().get_pixel(x, y);
                    assert_eq!(
                        [pixel[0], pixel[1], pixel[2]],
                        ground,
                        "{spelling} draws pixel row {y}, column {x} of {band} at {:?} where the \
                         row it falls in is {ground:?}: a shade's plane is no blend of the row \
                         beside it",
                        [pixel[0], pixel[1], pixel[2]]
                    );
                }
            }
        }
    }
}

#[test]
fn a_bands_glow_dies_away_across_the_cells_away_from_the_caret() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 20, 80, 5);
    let caret = 4;
    let plane =
        drawn(Shade::composer(area, Some(CELL), Some(caret), &palette).expect("a band to shade"));
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
fn every_row_of_the_rail_is_one_tone_so_an_ink_reads_the_same_wherever_it_falls() {
    for (spelling, palette) in SPELLED {
        let tone = palette.band(Band::Rail);
        for (shape, heights) in [
            (Rail::Ledger, &[1u16, 2, 8, 24][..]),
            (Rail::Strip, &[1u16][..]),
        ] {
            for height in heights {
                let area = Rect::new(20, 0, 32, *height);
                let shade = Shade::rail(shape, area, None, &palette).expect("a band to shade");
                let mut buffer = Buffer::empty(Rect {
                    x: 0,
                    width: area.right(),
                    ..area
                });
                shade.claim(&mut buffer, &palette);

                for row in 0..area.height {
                    assert_eq!(
                        shade.row(row),
                        tone,
                        "{spelling} draws row {row} of a {shape:?} of {height} rows on another \
                         colour than the rail's own tone: a row of the rail is no shadow, so an \
                         ink drawn there reads at one strength"
                    );
                    for x in area.x..area.right() {
                        assert_eq!(
                            under(&buffer, x, area.y + row),
                            match x == area.x && shade.seam().is_some() {
                                true => palette.edge_ink(),
                                false => tone,
                            },
                            "{spelling} claims row {row}, column {x} of a {shape:?} of {height} \
                             rows on another colour than the rail's own tone, and the one column \
                             of it the frame claims is its seam"
                        );
                    }
                }
            }
        }
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
        "the rail's plane covers the rail's own columns, and the seam is drawn in the first of \
         them rather than in a column beside"
    );
    pen::claim(&mut buffer, frame, palette.ground(Some(Band::Well)));
    shade.claim(&mut buffer, &palette);

    for x in 0..band.x {
        assert_eq!(
            under(&buffer, x, 0),
            palette.band(Band::Well),
            "column {x} of the transcript band is not the band's own: no claim of the rail's is \
             made past the columns it is drawn in, and the crease the well fell into is gone"
        );
    }
    for y in band.y..band.bottom() {
        for x in band.x..band.right() {
            assert_eq!(
                under(&buffer, x, y),
                match x == band.x {
                    true => palette.edge_ink(),
                    false => palette.ground(None).bg.expect("the terminal's own ground"),
                },
                "row {y}, column {x} of the band is not the terminal's own ground, which its \
                 plane is read through, and the one column the frame claims over it is its seam"
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
    let shade = Shade::composer(area, None, None, &palette).expect("a band to shade");
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
                match row {
                    0 => palette.edge_ink(),
                    row => shade.row(row),
                },
                "column {x} of row {row} is not the shade the band's own rows sit on, and the \
                 row the seam heads is the one the band does not"
            );
        }
    }
}

#[test]
fn a_band_a_plane_stands_under_is_claimed_on_the_terminals_own_ground() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, 40, 6);
    let shade = Shade::composer(area, Some(CELL), None, &palette).expect("a band to shade");
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
                match row {
                    0 => palette.edge_ink(),
                    _ => palette.ground(None).bg.expect("the terminal's own ground"),
                },
                "column {x} of row {row} is not the terminal's own ground, so the image drawn \
                 under it is read through a cell that is not its own, and the row the seam heads \
                 is the one the frame claims over the image"
            );
        }
    }
}

#[test]
fn a_ledgers_seam_is_its_own_first_column_and_takes_no_column_beside_it() {
    let palette = Palette::TRUECOLOR;
    let rail = Rect::new(20, 2, 32, 8);
    let shade = Shade::rail(Rail::Ledger, rail, None, &palette).expect("a band to shade");

    assert_eq!(
        shade.seam(),
        Some((Axis::Down, Rect { width: 1, ..rail })),
        "the seam is one column of the rail's own down every row it claims, and no column of the \
         transcript band's"
    );
}

#[test]
fn a_seam_is_claimed_in_the_one_edge_ink_and_the_rail_keeps_its_tone_beside_it() {
    for palette in [Palette::TRUECOLOR, Palette::ANSI] {
        let rail = Rect::new(20, 2, 32, 8);
        let frame = Rect {
            x: 0,
            width: rail.right(),
            ..rail
        };
        for cell in [None, Some(CELL)] {
            let shade = Shade::rail(Rail::Ledger, rail, cell, &palette).expect("a band to shade");
            let mut buffer = Buffer::empty(frame);
            shade.claim(&mut buffer, &palette);

            for y in rail.y..rail.bottom() {
                assert_eq!(
                    under(&buffer, rail.x, y),
                    palette.edge_ink(),
                    "row {y} of the rail's first column is not the one edge ink, and that column \
                     is the seam the rail is parted from the transcript band by"
                );
                for x in rail.x + 1..rail.right() {
                    let wanted = match shade.places() {
                        true => palette.ground(None).bg.expect("the terminal's own ground"),
                        false => palette.band(Band::Rail),
                    };
                    assert_eq!(
                        under(&buffer, x, y),
                        wanted,
                        "row {y}, column {x} of the rail is not the ground its shade says it \
                         sits on: the seam takes its first column and leaves the band the rest"
                    );
                }
            }
            for x in frame.x..rail.x {
                assert_eq!(
                    under(&buffer, x, rail.y),
                    Color::Reset,
                    "column {x} of the rows the rail is drawn over is claimed by the rail, and \
                     the transcript band's columns are its own"
                );
            }
        }
    }
}

#[test]
fn a_seam_is_claimed_in_cells_whatever_the_terminal_can_place() {
    let palette = Palette::TRUECOLOR;
    let rail = Rect::new(20, 2, 32, 8);
    let pictured = Shade::rail(Rail::Ledger, rail, Some(CELL), &palette).expect("a band to shade");
    let placeless = Shade::rail(Rail::Ledger, rail, None, &palette).expect("a band to shade");

    assert!(pictured.places(), "and a plane stands under this one");
    let mut drawn = Buffer::empty(Rect {
        x: 0,
        width: rail.right(),
        ..rail
    });
    let mut flat = drawn.clone();
    pictured.claim(&mut drawn, &palette);
    placeless.claim(&mut flat, &palette);

    for y in rail.y..rail.bottom() {
        assert_eq!(
            under(&drawn, rail.x, y),
            under(&flat, rail.x, y),
            "row {y} of the seam is drawn in a colour the other frame does not draw: a seam is \
             one pass of cells, and a plane stands under neither of its columns"
        );
    }
}

#[test]
fn the_composers_seam_is_the_row_that_heads_its_band() {
    let palette = Palette::TRUECOLOR;
    let band = Rect::new(0, 20, COLUMNS, 4);
    let shade = Shade::composer(band, None, None, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(band);

    assert_eq!(
        shade.seam(),
        Some((Axis::Across, Rect { height: 1, ..band })),
        "the composer's band is headed by the one row that parts it from the rail above it"
    );
    shade.claim(&mut buffer, &palette);
    for x in band.x..band.right() {
        assert_eq!(
            under(&buffer, x, band.y),
            palette.edge_ink(),
            "column {x} of the composer's head row is not the one edge ink, and the row carries \
             the draft's count"
        );
    }
    for row in 1..band.height {
        for x in band.x..band.right() {
            assert_eq!(
                under(&buffer, x, band.y + row),
                shade.row(row),
                "row {row} of the band is claimed on a ground other than the one its shade says: \
                 the seam takes the head row and no row of the draft"
            );
        }
    }
}

#[test]
fn a_palette_that_draws_no_edge_rules_the_seam_in_the_dim_ink_instead() {
    let rail = Rect::new(20, 0, 32, 4);
    let frame = Rect {
        x: 0,
        width: rail.right(),
        ..rail
    };
    let shade = Shade::rail(Rail::Ledger, rail, None, &Palette::SLOTS).expect("a band to shade");
    let mut buffer = Buffer::empty(frame);

    assert!(
        !Palette::SLOTS.draws_edge(),
        "and this palette spells no edge"
    );
    shade.claim(&mut buffer, &Palette::SLOTS);
    for y in rail.y..rail.bottom() {
        let cell = buffer.cell((rail.x, y)).expect("a cell of the seam");
        assert_eq!(
            cell.symbol(),
            EDGE_RULE.to_string(),
            "row {y} of the seam is no rule, and a terminal with no colour of its own is given \
             the dim one rather than nothing"
        );
        assert_eq!(Ink::of(cell, &Palette::SLOTS), Some(Ink::Dim));
    }
}

#[test]
fn a_seam_leaves_every_word_beside_it_whole() {
    let palette = Palette::TRUECOLOR;
    let rail = Rect::new(20, 2, 32, 2);
    let shade = Shade::rail(Rail::Ledger, rail, None, &palette).expect("a band to shade");
    let frame = Rect {
        x: 0,
        width: rail.right(),
        ..rail
    };
    let mut buffer = Buffer::empty(frame);
    let words = "a fact the rail draws";

    pen::claim(&mut buffer, frame, palette.ground(Some(Band::Rail)));
    shade.claim(&mut buffer, &palette);
    pen::write(
        &mut buffer,
        Rect {
            x: rail.x + MARGIN,
            height: 1,
            width: rail.width.saturating_sub(MARGIN),
            ..rail
        },
        &[(words, Ink::Muted)],
        Clip::Edge,
        &palette,
    );

    for y in rail.y..rail.bottom() {
        let cell = buffer.cell((rail.x, y)).expect("a cell of the seam");
        assert_eq!(
            under(&buffer, rail.x, y),
            palette.edge_ink(),
            "row {y} of the seam is not the one edge ink"
        );
        assert_eq!(
            cell.symbol(),
            " ",
            "row {y} of the seam carries a glyph, and the seam is a hairline and no word"
        );
    }
    let drawn: String = (rail.x + MARGIN..rail.right())
        .map(|x| {
            buffer
                .cell((x, rail.y))
                .expect("a cell of the row")
                .symbol()
                .to_owned()
        })
        .collect();
    assert_eq!(
        drawn,
        pen::pad(words, rail.width.saturating_sub(MARGIN)),
        "the rail's words are cut by no seam beside them, and a seam is one column of the band"
    );
    assert_eq!(
        Ink::of(
            buffer.cell((rail.x + MARGIN, rail.y)).expect("a word"),
            &palette
        ),
        Some(Ink::Muted),
        "and they keep the ink they were written in, because a ground under a cell takes no ink"
    );
}

#[test]
fn a_band_whose_shade_did_not_move_makes_no_image() {
    let area = Rect::new(2, 3, 40, 4);
    let shade =
        Shade::composer(area, Some(CELL), None, &Palette::TRUECOLOR).expect("a band to shade");
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
    let composer = Shade::composer(area, None, None, &Palette::TRUECOLOR).expect("a band to shade");
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
        Shade::composer(area, Some(CELL), Some(caret), &Palette::TRUECOLOR)
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
