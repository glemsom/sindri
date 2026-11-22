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

/// How far two pixels either side of a fall may be apart: the channel the ordered dither adds,
/// which says an amount of light in one pixel where the floor would have quantised it away, and
/// the one the steepest lamp falls over a pixel.
const DITHER_STEP: i32 = 2;

const CELL: CellSize = CellSize::NOMINAL;

fn shaded(rows: u16, palette: &Palette) -> Shade {
    Shade::composer(Rect::new(0, 0, COLUMNS, rows), None, palette).expect("a band of rows")
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

/// The light a pixel of a band's plane carries over the rung of the row it falls in.
fn light_over(shade: &Shade, plane: &Plane, x: u32, y: u32) -> i32 {
    let down = CELL.height();
    let rung = spelled(shade.row((y / down) as u16));
    let pixel = plane.pixels().get_pixel(x, y);
    i32::from(pixel[0]) - i32::from(rung[0])
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
            Shade::composer(Rect::new(0, 20, 80, 5), Some(CELL), palette).expect("a band to shade"),
        ),
    ]
}

fn lit_bands(palette: &Palette) -> Vec<(&'static str, Band, Shade)> {
    let mut all: Vec<(&'static str, Band, Shade)> = bands(palette)
        .into_iter()
        .map(|(band, shade)| (band, band_of(&shade), shade))
        .collect();
    all.push((
        "the caret's light",
        Band::Prompt,
        caret_light(Rect::new(0, 20, 80, 5), 12, palette),
    ));
    all
}

fn band_of(shade: &Shade) -> Band {
    match shade {
        Shade::Transcript { .. } => Band::Well,
        Shade::Rail { .. } => Band::Rail,
        Shade::Composer { .. } | Shade::Caret { .. } => Band::Prompt,
    }
}

fn caret_light(band: Rect, caret: u16, palette: &Palette) -> Shade {
    Shade::caret(band, Some(CELL), caret, palette).expect("the light the caret casts")
}

/// Where the light is centred, in the pixels of the image it is drawn as.
fn light_centre() -> u32 {
    u32::from(LIGHT_COLS / 2) * CELL.width() + CELL.width() / 2
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
fn a_bands_plane_is_its_shade_drawn_at_the_pixels_of_the_cell_it_covers() {
    for (spelling, palette) in SPELLED {
        for (band, shade) in bands(&palette) {
            let plane = drawn(shade);
            let peak = palette.peak(band_of(&shade));
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
                let rung = spelled(shade.row(row));
                let pixel = middle(&plane, row, CELL);
                for channel in 0..3 {
                    assert!(
                        pixel[channel] >= rung[channel]
                            && pixel[channel] <= rung[channel].saturating_add(peak),
                        "{spelling} draws the middle of row {row} of {band} at {pixel:?} where the \
                         row's own rung is {rung:?} and the light it spends is at most {peak} \
                         channels: a plane is the shade's own rung with its light added, and no \
                         colour of its own"
                    );
                }
            }
        }
    }
}

#[test]
fn a_composers_plane_opens_on_the_prompts_band_and_covers_no_pixel_of_the_well() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let plane = drawn(Shade::composer(area, Some(CELL), &palette).expect("a band to shade"));
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
fn a_rails_light_is_a_lamp_centred_on_the_face_and_gone_by_its_margins() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(6, 2, 32, 12);
    let shade = faced(Rail::Ledger, area, 3, Some(CELL), &palette);
    let Shade::Rail {
        face: Some(face), ..
    } = shade
    else {
        panic!("a rail a face stands on carries the rect its light is centred on");
    };
    let plane = drawn(shade);
    let peak = palette.peak(Band::Rail);
    let (width, height) = plane.pixels().dimensions();
    let (across, down) = (CELL.width(), CELL.height());
    let lit = |x: u32, y: u32| light_over(&shade, &plane, x, y);
    let (most, (left, top)) = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|at| (lit(at.0, at.1), at))
        .max()
        .expect("a plane with pixels in it");

    assert_eq!(
        most,
        i32::from(peak),
        "the rail's light peaks at {most} channels over its tone where it spends the {peak} the \
         palette hands it as a share of its rung's ceiling"
    );
    assert!(
        (u32::from(face.x - area.x) * across..u32::from(face.x - area.x + face.width) * across)
            .contains(&left)
            && (u32::from(face.y - area.y) * down..u32::from(face.y - area.y + face.height) * down)
                .contains(&top),
        "the brightest pixel of the rail stands at column {left}, pixel row {top}, outside the \
         columns and rows Sindri's face stands on at {face:?}: the light is a lamp centred on the \
         face and not a tint across the band"
    );

    let words = u32::from(MARGIN);
    let foot = u32::from(face.y - area.y + face.height) * down + words * down;
    for y in 0..height {
        for x in 0..width {
            let inside = x / across >= words && x / across < u32::from(area.width) - words;
            if inside && y < foot {
                continue;
            }
            assert_eq!(
                lit(x, y),
                0,
                "pixel row {y}, column {x} of the rail is lit where the light has left it: the \
                 lamp is spent at the margin the rail's words open in across, and a margin past \
                 the rows the face stands on, so it draws no edge of its own"
            );
        }
    }

    let walks = [
        (
            (left..width).map(|x| (x, top)).collect::<Vec<(u32, u32)>>(),
            "across the rail, away from the face towards its further margin",
        ),
        (
            (0..=left).rev().map(|x| (x, top)).collect(),
            "across the rail, away from the face towards its nearer margin",
        ),
        (
            (top..height).map(|y| (left, y)).collect(),
            "down the column the face's middle falls in",
        ),
        (
            (0..=top).rev().map(|y| (left, y)).collect(),
            "up the column the face's middle falls in",
        ),
    ];
    for (walk, where_to) in walks {
        for pair in walk.windows(2) {
            let (here, next) = (lit(pair[0].0, pair[0].1), lit(pair[1].0, pair[1].1));
            assert!(
                next <= here + DITHER_STEP,
                "{where_to}, the light is {here} channels at {:?} and {next} at {:?}: it falls \
                 away from the face there, and no row of it is a constant",
                pair[0],
                pair[1]
            );
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
fn the_composers_plane_carries_no_light_wherever_the_caret_waits() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let shade = Shade::composer(area, Some(CELL), &palette).expect("a band to shade");
        let plane = drawn(shade);
        let (width, height) = plane.pixels().dimensions();

        for y in 0..height {
            let rung = spelled(shade.row((y / CELL.height()) as u16));
            for x in 0..width {
                let pixel = plane.pixels().get_pixel(x, y);
                assert_eq!(
                    [pixel[0], pixel[1], pixel[2]],
                    rung,
                    "{spelling} draws pixel row {y}, column {x} of the composer's band at {:?} \
                     where the band's own tone is {rung:?}: the light the caret casts is an image of \
                     its own, and a keystroke places it again and repaints no band",
                    [pixel[0], pixel[1], pixel[2]]
                );
            }
        }
    }
}

#[test]
fn the_carets_light_peaks_on_the_head_row_of_its_band_and_is_spent_by_its_foot() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let shade = caret_light(area, 12, &palette);
        let plane = drawn(shade);
        let (width, height) = plane.pixels().dimensions();
        let centre = light_centre();
        let down = CELL.height();
        let peak = palette.peak(Band::Prompt);
        let carried = |x: u32, y: u32| light_over(&shade, &plane, x, y);

        assert_eq!(
            (width, height),
            (
                u32::from(LIGHT_COLS) * CELL.width(),
                u32::from(area.height) * down
            ),
            "{spelling} draws the caret's light at other than the cells it spans over the band's \
             own rows"
        );
        assert_eq!(
            carried(centre, 0),
            i32::from(peak),
            "{spelling} draws the caret's own column of the band's head row at {} channels over \
             the row it lights where the light's peak is {peak}: the band is lit from above, and \
             the light the caret casts lands on its head",
            carried(centre, 0)
        );
        for y in 0..height.saturating_sub(1) {
            assert!(
                carried(centre, y + 1) <= carried(centre, y) + DITHER_STEP,
                "{spelling} draws pixel row {y} of the caret's own column at {} channels over the \
                 row it lights where the row under it carries {}: the light falls down the band \
                 rather than being painted across it",
                carried(centre, y),
                carried(centre, y + 1)
            );
        }
        for x in 0..width {
            assert_eq!(
                carried(x, height - 1),
                0,
                "{spelling} draws column {x} of the band's foot at {} channels over the row it \
                 lights: the light is spent by the band it falls down, and the band is read on \
                 its own tone at the foot",
                carried(x, height - 1)
            );
        }
        for y in 0..height {
            for x in [0, width - 1] {
                assert_eq!(
                    carried(x, y),
                    0,
                    "{spelling} draws column {x}, pixel row {y} of the light's own edge at {} \
                     channels over the row it lights: the light is spent by the last cell it \
                     spans, so the box it is drawn in leaves no step of its own",
                    carried(x, y)
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
fn no_two_pixels_of_a_band_are_a_step_apart_the_eye_can_find() {
    for (spelling, palette) in SPELLED {
        for (band, _, shade) in lit_bands(&palette) {
            let plane = drawn(shade);
            let (width, height) = plane.pixels().dimensions();
            let down = CELL.height();
            let rung = |y: u32| spelled(shade.row((y / down) as u16));
            let at = |x: u32, y: u32| {
                let pixel = plane.pixels().get_pixel(x, y);
                [pixel[0], pixel[1], pixel[2]]
            };
            let apart = |left: [u8; 3], right: [u8; 3]| {
                (0..3).all(|channel| {
                    i32::from(left[channel]).abs_diff(i32::from(right[channel]))
                        <= u32::try_from(DITHER_STEP).unwrap_or(1)
                })
            };

            for y in 0..height {
                for x in 0..width {
                    if x + 1 < width {
                        assert!(
                            apart(at(x, y), at(x + 1, y)),
                            "{spelling} draws pixel row {y} of {band} at {:?} in column {x} and \
                             {:?} in the column beside it: a fall of light is spent by the pixel, \
                             so no two neighbouring pixels are a step the eye can find",
                            at(x, y),
                            at(x + 1, y)
                        );
                    }
                    if y + 1 < height && rung(y) == rung(y + 1) {
                        assert!(
                            apart(at(x, y), at(x, y + 1)),
                            "{spelling} draws column {x} of {band} at {:?} in pixel row {y} and \
                             {:?} in the pixel row under it: the light falls by the pixel and \
                             never steps between two of them",
                            at(x, y),
                            at(x, y + 1)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_carets_light_dies_away_either_side_of_the_caret() {
    let palette = Palette::TRUECOLOR;
    let plane = drawn(caret_light(Rect::new(0, 20, 80, 5), 4, &palette));
    let row = plane.pixels().height() / 2;
    let centre = light_centre();

    let mut last = 0;
    for x in 0..centre {
        let pixel = plane.pixels().get_pixel(x, row);
        assert!(
            i32::from(pixel[0]) + DITHER_STEP >= last,
            "the light at pixel {x} of row {row} is darker than the pixel further from its centre \
             before it, so it does not rise towards the caret"
        );
        last = i32::from(pixel[0]);
    }
    let mut last = i32::from(u8::MAX);
    for x in centre..plane.pixels().width() {
        let pixel = plane.pixels().get_pixel(x, row);
        assert!(
            i32::from(pixel[0]) <= last + DITHER_STEP,
            "the light at pixel {x} of row {row} is lighter than the pixel nearer its centre \
             before it, so it does not fall away from the caret"
        );
        last = i32::from(pixel[0]);
    }
    let lit = plane.pixels().get_pixel(centre, row);
    let far = plane.pixels().get_pixel(plane.pixels().width() - 1, row);
    assert!(
        lit[0] > far[0],
        "the caret's own column is no lighter than the light's far side: {lit:?} against {far:?}"
    );
}

#[test]
fn a_transcripts_light_crowns_its_head_and_falls_to_the_well_by_its_third_row() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 0, 20, 6);
        let shade =
            Shade::transcript(area, Some(CELL), &palette).expect("a band the plane stands under");
        let plane = drawn(shade);
        let peak = palette.peak(Band::Well);
        let (width, height) = plane.pixels().dimensions();
        let down = CELL.height();
        let lit = |x: u32, y: u32| light_over(&shade, &plane, x, y);

        assert_eq!(
            plane.area(),
            area,
            "{spelling} draws the transcript's plane over a rect of its own, and a plane covers \
             the band whose shade it is drawn from"
        );
        assert_eq!(
            height,
            u32::from(area.height) * down,
            "{spelling} draws the transcript's plane at other than the pixels of the cell it is \
             drawn on"
        );
        for x in 0..width {
            assert_eq!(
                lit(x, 0),
                i32::from(peak),
                "{spelling} draws column {x} of the plane's first pixel row at {} channels over \
                 the well where the band's head is lit to the {peak} it spends: the head of the \
                 band is where a page's light falls, and it falls on every column of it",
                lit(x, 0)
            );
        }
        for x in [0, width / 2, width - 1] {
            for y in 1..height {
                assert!(
                    lit(x, y) <= lit(x, y - 1) + DITHER_STEP,
                    "{spelling} draws pixel row {y} of column {x} at {} channels over the well \
                     where the row above it carries {}: the crown falls away from the band's head \
                     by the pixel and never steps back up it",
                    lit(x, y),
                    lit(x, y - 1)
                );
            }
        }
        let second = lit(0, down);
        assert!(
            second > 0 && second < i32::from(peak),
            "{spelling} draws the second cell row at {second} channels over the well where the head \
             carries {peak}: the light is neither the head's own nor spent at the cell's boundary, \
             so it falls through the pixels of a row instead of stepping by the row",
        );
        assert!(
            lit(0, 2 * down - 1) < i32::from(peak) / 4,
            "{spelling} draws the last pixel row of the band's second cell at {} channels over the \
             well: the crown is all but spent by the foot of that row",
            lit(0, 2 * down - 1)
        );
        for row in 2..area.height {
            for y in 0..down {
                for x in 0..width {
                    let carried = lit(x, u32::from(row) * down + y);
                    assert_eq!(
                        carried, 0,
                        "{spelling} draws pixel row {y} of cell row {row} of the transcript's \
                         band at {carried} channels over the well: the prose is read on the well, \
                         and the light has fallen back to it by the band's third row"
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
    let shade = Shade::composer(area, None, &palette).expect("a band to shade");
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
    let shade = Shade::composer(area, Some(CELL), &palette).expect("a band to shade");
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
    let shade = Shade::composer(band, None, &palette).expect("a band to shade");
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
    let shade = Shade::composer(area, Some(CELL), &Palette::TRUECOLOR).expect("a band to shade");
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
            Shade::composer(area, None, &palette).expect("a band to shade"),
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
fn a_caret_that_moved_makes_no_image_and_the_light_it_stands_from_is_held() {
    let band = Rect::new(2, 3, 40, 4);
    let palette = Palette::TRUECOLOR;
    let frame = |caret| {
        [
            Shade::composer(band, Some(CELL), &palette).expect("a band to shade"),
            caret_light(band, caret, &palette),
        ]
    };
    let mut planes = held(&frame(6));
    let made = planes.made();

    planes.hold(&frame(7));
    planes.hold(&frame(19));

    assert_eq!(
        planes.made(),
        made,
        "a caret that moved makes no image at all: its light is placed again where it stands, and \
         the band under it never changed"
    );
    assert_eq!(
        planes
            .held()
            .iter()
            .map(Plane::id)
            .collect::<Vec<PlaneId>>(),
        vec![PlaneId::Composer, PlaneId::Caret],
        "and the frame still holds the band and the one light it was drawn from"
    );
}

#[test]
fn the_carets_light_is_placed_on_the_carets_own_cell_and_cut_to_the_bands_columns() {
    let band = Rect::new(4, 20, 40, 4);
    let palette = Palette::TRUECOLOR;
    let place = |caret| {
        caret_light(band, caret, &palette)
            .place()
            .expect("a light with cells of the band to stand on")
    };

    let middle = place(24);
    assert_eq!(
        (middle.left, middle.cols, middle.skip),
        (12, LIGHT_COLS, 0),
        "a caret with room either side of it is the whole of the light, and it starts where its \
         own fall does"
    );
    assert_eq!(
        middle.left + LIGHT_COLS / 2,
        24,
        "and it is placed so that its own centre falls on the caret's cell"
    );

    let left = place(4);
    assert_eq!(
        (left.left, left.cols, left.skip),
        (4, LIGHT_COLS - 12, 12),
        "a caret at the band's first column keeps the light's own centre on it and shows only the \
         columns of it the band has room for"
    );
    assert_eq!(
        (left.top, left.rows),
        (20, 4),
        "the light stands on the band's own rows and no row beside them"
    );

    let right = place(43);
    assert_eq!(
        (right.left, right.cols, right.skip),
        (31, LIGHT_COLS - 12, 0),
        "a caret at the band's last column loses the far columns of the light and no cell of it"
    );
    for placement in [middle, left, right] {
        assert_eq!(
            u32::from(placement.skip) * CELL.width(),
            placement.from,
            "the columns shown of the light are the ones its placement names, in pixels of it"
        );
        assert_eq!(
            u32::from(placement.cols) * CELL.width(),
            placement.width,
            "and the columns it stands over are as many pixels of it"
        );
    }
}

#[test]
fn the_carets_light_is_held_over_the_band_it_lights_and_no_band_over_it() {
    let order = |id: PlaneId| id.image();
    assert!(
        order(PlaneId::Caret) > order(PlaneId::Composer),
        "two images at the frame's one depth stand in the order of their identities, and the \
         light the caret casts stands over the band it lights"
    );
    for band in [PlaneId::Rail, PlaneId::Transcript] {
        assert!(
            order(PlaneId::Caret) > order(band),
            "and no band's plane stands over it: {band:?} is held under {} where the light is \
             held under {}",
            order(band),
            order(PlaneId::Caret)
        );
    }
}

#[test]
fn the_carets_light_claims_no_cell_of_the_band_it_lights() {
    let band = Rect::new(0, 20, 40, 4);
    let palette = Palette::TRUECOLOR;
    let shade = caret_light(band, 12, &palette);
    let mut buffer = Buffer::empty(band);
    let ground = Color::Rgb(0x0a, 0x0b, 0x0c);
    pen::claim(&mut buffer, band, palette.over(ground));

    assert!(
        shade.places(),
        "the light a caret casts is placed under the words of the band it lights"
    );
    shade.claim(&mut buffer, &palette);

    for y in band.y..band.bottom() {
        for x in band.x..band.right() {
            assert_eq!(
                under(&buffer, x, y),
                ground,
                "row {y}, column {x} of the band is claimed again by the light the caret casts: the band \
                 claims the cells its words stand on, and the light under them is no cell of its own"
            );
        }
    }
}
