use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::*;
use crate::composer::{MAX_ROWS, MIN_ROWS};
use crate::ink::Ink;
use crate::ledger::RailBand;
use crate::pen::Clip;
use crate::readback::under;
use crate::rows::MARGIN;

const SPELLED: [(&str, Palette); 2] = [("ansi", Palette::ANSI), ("truecolor", Palette::TRUECOLOR)];

const COLUMNS: u16 = 80;

const CELL: CellSize = CellSize::NOMINAL;

fn shaded(rows: u16, palette: &Palette) -> Shade {
    Shade::composer(Rect::new(0, 0, COLUMNS, rows), None, None, palette).expect("a band of rows")
}

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

fn held(shades: &[Shade]) -> Planes {
    let mut planes = Planes::default();
    planes.hold(shades);
    planes
}

fn middle(plane: &Plane, row: u16, cell: CellSize) -> [u8; 3] {
    let x = plane.pixels().width() / 2;
    let y = u32::from(row) * cell.height() + cell.height() / 2;
    let pixel = plane.pixels().get_pixel(x, y);
    [pixel[0], pixel[1], pixel[2]]
}

fn near(left: [u8; 3], right: [u8; 3]) -> bool {
    (0..3).all(|channel| left[channel].abs_diff(right[channel]) <= 1)
}

fn spelled(colour: Color) -> [u8; 3] {
    channels(colour).expect("a colour in pixels")
}

fn railed(shape: Rail, area: Rect, cell: Option<CellSize>, palette: &Palette) -> Option<Shade> {
    Shade::rail(shape, RailBand::of(area), cell, palette)
}

fn faced(shape: Rail, area: Rect, rows: u16, cell: Option<CellSize>, palette: &Palette) -> Shade {
    Shade::rail(
        shape,
        RailBand::of(area).with_picture(area.width, rows),
        cell,
        palette,
    )
    .expect("a band to shade")
}

fn drawn(shade: Shade) -> Plane {
    Plane::draw(shade).expect("a band a plane is drawn under")
}

fn bands(palette: &Palette) -> [(&'static str, Shade); 4] {
    [
        (
            "the transcript's band",
            Shade::transcript(Rect::new(0, 0, 80, 6), Some(CELL), palette)
                .expect("a band to shade"),
        ),
        (
            "the ledger's rail",
            faced(Rail::Ledger, Rect::new(6, 2, 32, 8), 3, Some(CELL), palette),
        ),
        (
            "a strip",
            railed(Rail::Strip, Rect::new(6, 2, 32, 1), Some(CELL), palette)
                .expect("a band to shade"),
        ),
        (
            "the composer's band",
            Shade::composer(Rect::new(0, 20, 80, 5), Some(CELL), None, palette)
                .expect("a band to shade"),
        ),
    ]
}

fn lit_bands(palette: &Palette) -> Vec<(&'static str, Band, Shade)> {
    let mut all: Vec<(&'static str, Band, Shade)> = bands(palette)
        .into_iter()
        .map(|(band, shade)| (band, band_of(&shade), shade))
        .collect();
    all.push((
        "the composer's band under a caret",
        Band::Prompt,
        Shade::composer(Rect::new(0, 20, 80, 5), Some(CELL), Some(12), palette)
            .expect("a band to shade"),
    ));
    all
}

fn band_of(shade: &Shade) -> Band {
    match shade {
        Shade::Transcript { .. } => Band::Well,
        Shade::Rail { .. } => Band::Rail,
        Shade::Composer { .. } => Band::Prompt,
    }
}

fn carried(shade: &Shade, row: u16) -> [u8; 3] {
    spelled(shade.row(row)).map(|channel| channel.saturating_add(shade.light(row) as u8))
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
                let said = carried(&shade, row);
                assert_eq!(
                    pixel, said,
                    "{spelling} draws the middle of row {row} of {band} at {pixel:?} where the \
                     shade says {said:?}: a plane is the shade sampled at the cell, and no \
                     colour of its own"
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
fn a_rails_plane_is_lit_on_the_rows_the_face_stands_on_and_is_its_own_tone_below() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(6, 2, 32, 8);
        let shade = faced(Rail::Ledger, area, 3, Some(CELL), &palette);
        let Shade::Rail {
            face: Some(face), ..
        } = shade
        else {
            panic!("a rail a face stands on carries the rows it stands on");
        };
        let plane = drawn(shade);
        let tone = spelled(palette.band(Band::Rail));
        let lift = palette.lift(Band::Rail);
        let lit = tone.map(|channel| channel.saturating_add(lift));
        let (width, height) = plane.pixels().dimensions();

        assert_eq!(
            height,
            u32::from(area.height) * CELL.height(),
            "{spelling} draws the rail's plane at other than the pixels of the cell it is drawn on"
        );
        for row in 0..area.height {
            let y = area.y + row;
            let stands = y < face.bottom();
            for down in 0..CELL.height() {
                for x in 0..width {
                    let pixel = plane
                        .pixels()
                        .get_pixel(x, u32::from(row) * CELL.height() + down);
                    let drawn = [pixel[0], pixel[1], pixel[2]];
                    match stands {
                        true => assert_eq!(
                            drawn, lit,
                            "{spelling} draws row {row}, column {x} of the rail at {drawn:?} \
                             where the face stands on it: the rows Sindri's face stands on are \
                             the rows the frame's light falls on"
                        ),
                        false => assert_eq!(
                            drawn, tone,
                            "{spelling} draws row {row}, column {x} of the rail at {drawn:?} \
                             where the rail's own tone is {tone:?}: no light falls below the \
                             face, and a row with no word on it is no lighter than the band"
                        ),
                    }
                }
            }
        }
    }
}

#[test]
fn a_rail_with_no_face_is_its_own_tone_on_every_row() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(6, 2, 32, 8);
        let shade = railed(Rail::Ledger, area, Some(CELL), &palette).expect("a band to shade");
        let plane = drawn(shade);
        let tone = spelled(palette.band(Band::Rail));
        let (width, height) = plane.pixels().dimensions();

        for y in 0..height {
            for x in 0..width {
                let pixel = plane.pixels().get_pixel(x, y);
                assert_eq!(
                    [pixel[0], pixel[1], pixel[2]],
                    tone,
                    "{spelling} draws pixel row {y}, column {x} of a rail that carries no face \
                     at {:?} where the rail's own tone is {tone:?}: no face stands on it, so no \
                     row of it is lit",
                    [pixel[0], pixel[1], pixel[2]]
                );
            }
        }
    }
}

#[test]
fn a_strips_plane_is_the_one_row_it_has() {
    for (spelling, palette) in SPELLED {
        let plane = drawn(
            railed(Rail::Strip, Rect::new(6, 2, 32, 1), Some(CELL), &palette)
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
fn the_composer_planes_light_is_the_bands_lift_under_the_caret() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let caret = 12;
        let shade =
            Shade::composer(area, Some(CELL), Some(caret), &palette).expect("a band to shade");
        let plane = drawn(shade);
        let lift = palette.lift(Band::Prompt);
        let column = u32::from(caret) * CELL.width() + CELL.width() / 2;
        let far = plane.pixels().width() - 1;

        assert_eq!(
            spelled(shade.row(0)),
            spelled(palette.band(Band::Prompt)),
            "{spelling} draws the row under the caret on the prompt's own band"
        );
        for row in 0..area.height {
            let rung = spelled(shade.row(row));
            let lit = rung.map(|channel| channel.saturating_add(lift));
            for down in 0..CELL.height() {
                let y = u32::from(row) * CELL.height() + down;
                let pixel = plane.pixels().get_pixel(column, y);
                assert_eq!(
                    [pixel[0], pixel[1], pixel[2]],
                    lit,
                    "{spelling} draws the cell the caret waits in on row {row}, pixel row \
                     {down}, at {:?} where the row lit to the most it can take is {lit:?}: the \
                     light's peak is the band's own lift, and it is the same on every pixel row \
                     of the column",
                    [pixel[0], pixel[1], pixel[2]]
                );
                let edge = plane.pixels().get_pixel(far, y);
                assert_eq!(
                    [edge[0], edge[1], edge[2]],
                    rung,
                    "{spelling} draws the band's far column on row {row}, pixel row {down}, at \
                     {:?} where the row's own rung is {rung:?}: the glow has died away there, \
                     and no pixel of the band is a rung of another's",
                    [edge[0], edge[1], edge[2]]
                );
            }
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
fn no_pixel_of_any_bands_plane_is_a_blend_of_two_rungs() {
    for (spelling, palette) in SPELLED {
        for (band, at, shade) in lit_bands(&palette) {
            let plane = drawn(shade);
            let (width, height) = plane.pixels().dimensions();
            let down = CELL.height();
            let most = palette.lift(at);

            for y in 0..height {
                let ground = spelled(shade.row((y / down) as u16));
                for x in 0..width {
                    let pixel = plane.pixels().get_pixel(x, y);
                    let drawn = [pixel[0], pixel[1], pixel[2]];
                    let carried: Vec<u16> = (0..3)
                        .map(|channel| u16::from(drawn[channel]) - u16::from(ground[channel]))
                        .collect();
                    assert!(
                        carried.iter().all(|light| *light == carried[0])
                            && carried[0] <= u16::from(most),
                        "{spelling} draws pixel row {y}, column {x} of {band} at {drawn:?} where \
                         the row it falls in is {ground:?}: a plane's pixel is its band's own rung \
                         with one light added to every channel — never a blend of two rungs, and \
                         never more light than the {most} the band can take"
                    );
                }
            }
        }
    }
}

#[test]
fn no_pixel_of_any_bands_plane_is_darker_than_the_rung_it_sits_on() {
    for (spelling, palette) in SPELLED {
        for (band, _, shade) in lit_bands(&palette) {
            let plane = drawn(shade);
            let (width, height) = plane.pixels().dimensions();
            let down = CELL.height();

            for y in 0..height {
                let ground = spelled(shade.row((y / down) as u16));
                for x in 0..width {
                    let pixel = plane.pixels().get_pixel(x, y);
                    for channel in 0..3 {
                        assert!(
                            pixel[channel] >= ground[channel],
                            "{spelling} draws channel {channel} of pixel row {y}, column {x} of \
                             {band} at {} under the {} of the rung it is drawn in: a plane's light \
                             only ever adds, so no pixel of a band sits under its own rung",
                            pixel[channel],
                            ground[channel]
                        );
                    }
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
fn a_transcript_bands_plane_is_its_head_lit_and_the_well_by_its_third_row() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 0, 20, 6);
        let shade =
            Shade::transcript(area, Some(CELL), &palette).expect("a band the plane stands under");
        let plane = drawn(shade);
        let well = spelled(palette.band(Band::Well));
        let lift = palette.lift(Band::Well);
        let lit = |light: u8| well.map(|channel| channel.saturating_add(light));
        let (width, height) = plane.pixels().dimensions();

        assert_eq!(
            plane.area(),
            area,
            "{spelling} draws the transcript's plane over a rect of its own, and a plane covers \
             the band whose shade it is drawn from"
        );
        assert_eq!(
            height,
            u32::from(area.height) * CELL.height(),
            "{spelling} draws the transcript's plane at other than the pixels of the cell it is \
             drawn on"
        );
        for x in 0..width {
            let head = plane.pixels().get_pixel(x, 0);
            assert_eq!(
                [head[0], head[1], head[2]],
                lit(lift),
                "{spelling} draws column {x} of the plane's first pixel row at {:?} where the \
                 band's own head row lit to the most it can take is {:?}: the head of the band \
                 is where the frame's light falls",
                [head[0], head[1], head[2]],
                lit(lift)
            );
        }
        for y in 0..CELL.height() {
            let second = plane.pixels().get_pixel(0, y + CELL.height());
            assert_eq!(
                [second[0], second[1], second[2]],
                lit(lift / 2),
                "{spelling} draws pixel row {y} of the band's second cell row at {:?} where half \
                 the light it can take is {:?}: the light falls away from the head in whole \
                 channels",
                [second[0], second[1], second[2]],
                lit(lift / 2)
            );
        }
        for row in 2..area.height {
            for y in 0..CELL.height() {
                for x in 0..width {
                    let pixel = plane
                        .pixels()
                        .get_pixel(x, u32::from(row) * CELL.height() + y);
                    assert_eq!(
                        [pixel[0], pixel[1], pixel[2]],
                        well,
                        "{spelling} draws pixel row {y} of cell row {row} of the transcript's \
                         band at {:?} where the well is {well:?}: the prose is read on the well, \
                         and the light has fallen back to it by the band's third row",
                        [pixel[0], pixel[1], pixel[2]]
                    );
                }
            }
        }
    }
}

#[test]
fn the_transcript_band_is_claimed_on_the_well_or_on_the_terminals_own_ground_under_its_plane() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 0, 20, 4);
        let own = palette.ground(None).bg.expect("the terminal's own ground");

        for cell in [None, Some(CELL)] {
            let shade = Shade::transcript(area, cell, &palette).expect("a band of rows");
            let mut buffer = Buffer::empty(area);
            let ground = match shade.places() {
                true => own,
                false => palette.band(Band::Well),
            };

            assert_eq!(
                shade.places(),
                cell.is_some(),
                "{spelling} draws the transcript band under a plane exactly when it is handed the \
             cell to draw one at"
            );
            assert!(
                shade.seam().is_none(),
                "the transcript band meets no band beside it in its own columns, so it parts nothing"
            );
            shade.claim(&mut buffer, &palette);

            for y in area.y..area.bottom() {
                for x in area.x..area.right() {
                    assert_eq!(
                        under(&buffer, x, y),
                        ground,
                        "{spelling} claims row {y}, column {x} of the transcript band on something \
                     other than the well: where a plane stands the plane is the well and the cell \
                     is the terminal's own ground read through it, and where none does the cell \
                     is the well itself"
                    );
                    assert_eq!(
                        Ink::of(buffer.cell((x, y)).expect("a cell of the band"), &palette),
                        Some(Ink::Plain),
                        "{spelling} claims row {y}, column {x} of the transcript band in an ink \
                     other than the frame's own, so a word of it reads as a word of another kind"
                    );
                }
            }
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
                let shade = railed(shape, area, None, &palette).expect("a band to shade");
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
    let shade = railed(Rail::Ledger, band, Some(CELL), &palette).expect("a band to shade");
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
    let shade = railed(Rail::Strip, area, None, &palette).expect("a band to shade");
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
    let shade = railed(Rail::Ledger, rail, None, &palette).expect("a band to shade");

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
            let shade = railed(Rail::Ledger, rail, cell, &palette).expect("a band to shade");
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
    let pictured = railed(Rail::Ledger, rail, Some(CELL), &palette).expect("a band to shade");
    let placeless = railed(Rail::Ledger, rail, None, &palette).expect("a band to shade");

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
    let shade = railed(Rail::Ledger, rail, None, &Palette::SLOTS).expect("a band to shade");
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
    let shade = railed(Rail::Ledger, rail, None, &palette).expect("a band to shade");
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
fn a_transcript_band_a_plane_stands_under_makes_its_image_and_one_it_does_not_makes_none() {
    let area = Rect::new(0, 0, 40, 4);
    let palette = Palette::TRUECOLOR;

    let bare = [
        (
            "the transcript's band",
            Shade::transcript(area, None, &palette).expect("a band of rows"),
        ),
        (
            "the composer's band",
            Shade::composer(area, None, None, &palette).expect("a band to shade"),
        ),
    ];
    let placeless = held(&bare.map(|(_, shade)| shade));

    assert_eq!(
        placeless.made(),
        0,
        "a band drawn in cells alone is drawn as no image at all"
    );
    assert!(
        placeless.held().is_empty(),
        "and the frame holds nothing against it: {:?}",
        placeless.held()
    );

    let pictured = held(&[Shade::transcript(area, Some(CELL), &palette).expect("a band of rows")]);
    assert_eq!(
        pictured.made(),
        1,
        "the transcript band a terminal places a picture under makes the one image it is drawn \
         as"
    );
    assert_eq!(
        pictured.held().first().map(Plane::id),
        Some(PlaneId::Transcript),
        "and it is held under the transcript band's own identity"
    );
}

#[test]
fn a_rails_light_moving_makes_its_image_again() {
    let area = Rect::new(2, 3, 40, 6);
    let palette = Palette::TRUECOLOR;
    let mut planes =
        held(&[railed(Rail::Ledger, area, Some(CELL), &palette).expect("a band to shade")]);
    let made = planes.made();

    planes.hold(&[faced(Rail::Ledger, area, 3, Some(CELL), &palette)]);

    assert_eq!(
        (planes.made(), planes.held().len()),
        (made + 1, 1),
        "the light falls on the rows Sindri's face stands on, so a rail whose face moved is \
         drawn again and the image it stood on is taken away"
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
