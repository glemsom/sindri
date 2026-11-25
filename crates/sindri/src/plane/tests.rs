use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::*;
use crate::composer::{MAX_ROWS, MIN_ROWS};
use crate::face::Motion;
use crate::ink::Ink;
use crate::instrument;
use crate::ledger::RailBand;
use crate::pen::Clip;
use crate::pulse::Turning;
use crate::readback::under;
use crate::rows::MARGIN;

const SPELLED: [(&str, Palette); 2] = [("ansi", Palette::ANSI), ("truecolor", Palette::TRUECOLOR)];

const COLUMNS: u16 = 80;

/// How far two pixels either side of a fall may be apart: the channel the ordered dither adds,
/// which says an amount of light in one pixel where the floor would have quantised it away, and
/// the one the steepest lamp falls over a pixel.
const DITHER_STEP: i32 = 2;

const CELL: CellSize = CellSize::NOMINAL;

fn shaded(rows: u16, palette: &Palette) -> Placement {
    Placement::composer(Rect::new(0, 0, COLUMNS, rows), None, palette).expect("a band of rows")
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

fn held(placements: &[Placement]) -> Planes {
    let mut planes = Planes::default();
    planes.hold(placements);
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

fn railed(shape: Rail, area: Rect, cell: Option<CellSize>, palette: &Palette) -> Option<Placement> {
    Placement::rail(shape, RailBand::of(area), cell, palette)
}

fn faced(
    shape: Rail,
    area: Rect,
    rows: u16,
    cell: Option<CellSize>,
    palette: &Palette,
) -> Placement {
    Placement::rail(
        shape,
        RailBand::of(area).with_picture(area.width, rows),
        cell,
        palette,
    )
    .expect("a band to shade")
}

/// The light a pixel of a band's plane carries over the rung of the row it falls in.
fn light_over(placement: &Placement, plane: &Plane, x: u32, y: u32) -> i32 {
    lit_over(placement, plane, CELL, x, y)
}

/// The same light, on a plane drawn at any size of cell: read on the channel the shadow has not
/// drained to the floor, which is the one the light is whole in.
fn lit_over(placement: &Placement, plane: &Plane, cell: CellSize, x: u32, y: u32) -> i32 {
    let rung = spelled(placement.shade().row((y / cell.height()) as u16));
    let pixel = plane.pixels().get_pixel(x, y);
    (0..3)
        .map(|channel| i32::from(pixel[channel]) - i32::from(rung[channel]))
        .min()
        .expect("three channels")
}

/// The pixels a seam's own cell spans along the axis that parts the two bands.
fn seam_extent(axis: Axis, cell: CellSize) -> u32 {
    match axis {
        Axis::Down => cell.width(),
        Axis::Across => (cell.height() / 2).max(1),
    }
}

/// Whether a pixel of a plane falls in the seam's own cell.
fn in_seam(axis: Axis, cell: CellSize, x: u32, y: u32) -> bool {
    match axis {
        Axis::Down => x < cell.width(),
        Axis::Across => y < seam_extent(axis, cell),
    }
}

/// The pixel of a plane `along` pixels from the joint, on the line the seam's cell opens on.
fn along_seam(axis: Axis, along: u32) -> (u32, u32) {
    match axis {
        Axis::Down => (along, 0),
        Axis::Across => (0, along),
    }
}

/// The rail's band and the composer's band, each parted from the band beside it on its own axis and
/// drawn at `cell`.
fn parted_bands(palette: &Palette, cell: CellSize) -> [(Axis, Placement); 2] {
    [
        (
            Axis::Down,
            faced(
                Rail::Ledger,
                Rect::new(20, 0, 32, 6),
                3,
                Some(cell),
                palette,
            ),
        ),
        (
            Axis::Across,
            Placement::composer(Rect::new(0, 20, 40, 4), Some(cell), palette)
                .expect("a band to shade"),
        ),
    ]
}

/// The channels a seam takes from its band's rung at the joint.
fn weight_of(palette: &Palette) -> i32 {
    i32::from(palette.seam())
}

fn drawn(placement: Placement) -> Plane {
    Plane::draw(*placement.shade()).expect("a band a plane is drawn under")
}

/// The fall a seam draws, which stands in the seam's own cell whatever the band beside it is.
fn keeps_its_fall(placement: Placement, palette: &Palette, cell: CellSize, what: &str) {
    let plane = drawn(placement);
    let (width, height) = plane.pixels().dimensions();
    let (axis, _) = placement.seam().expect("a band the frame parts");
    let weight = weight_of(palette);
    let extent = seam_extent(axis, cell);
    let lit = |along: u32| {
        let (x, y) = along_seam(axis, along);
        lit_over(&placement, &plane, cell, x, y)
    };

    let mut darkest = (i32::MAX, (0, 0));
    for y in 0..height {
        for x in 0..width {
            let carried = lit_over(&placement, &plane, cell, x, y);
            if carried < darkest.0 {
                darkest = (carried, (x, y));
            }
        }
    }
    assert_eq!(
        darkest,
        (-weight, along_seam(axis, 0)),
        "{what}, at {} by {} pixels: the plane is darkest at {:?}, carrying {} channels over its \
         rung, where the joint stands at {:?} and the weight it takes is {weight}",
        cell.width(),
        cell.height(),
        darkest.1,
        darkest.0,
        along_seam(axis, 0)
    );
    assert_eq!(
        lit(extent - 1),
        weight,
        "{what}, at {} by {} pixels: the last pixel of the seam's cell carries {} channels over \
         its rung where the hairline lifts the same {weight} the joint takes",
        cell.width(),
        cell.height(),
        lit(extent - 1)
    );
    assert_eq!(
        lit(extent - 2),
        0,
        "{what}, at {} by {} pixels: the pixel before the hairline carries {} channels over its \
         rung, so the hairline is one pixel wide and no fall of its own",
        cell.width(),
        cell.height(),
        lit(extent - 2)
    );
    let line = match axis {
        Axis::Down => height,
        Axis::Across => width,
    };
    for at in 0..line {
        let (x, y) = match axis {
            Axis::Down => (extent, at),
            Axis::Across => (at, extent),
        };
        let carried = lit_over(&placement, &plane, cell, x, y);
        assert_eq!(
            carried,
            0,
            "{what}, at {} by {} pixels: pixel row {y}, column {x} of the line after the seam's \
             cell carries {carried} channels over its rung, so no band of another width leaves a \
             joint in the wrong pixel",
            cell.width(),
            cell.height()
        );
    }
}

fn bands(palette: &Palette) -> [(&'static str, Placement); 4] {
    [
        (
            "the transcript's band",
            Placement::transcript(Rect::new(0, 0, 80, 6), Some(CELL), palette)
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
            Placement::composer(Rect::new(0, 20, 80, 5), Some(CELL), palette)
                .expect("a band to shade"),
        ),
    ]
}

fn lit_bands(palette: &Palette) -> Vec<(&'static str, Band, Placement)> {
    let mut all: Vec<(&'static str, Band, Placement)> = bands(palette)
        .into_iter()
        .map(|(band, placement)| (band, band_of(&placement), placement))
        .collect();
    all.push((
        "the caret's light",
        Band::Prompt,
        caret_light(Rect::new(0, 20, 80, 5), 12, palette),
    ));
    all
}

fn band_of(placement: &Placement) -> Band {
    match placement.shade().id() {
        PlaneId::Transcript => Band::Well,
        PlaneId::Rail | PlaneId::Gauge | PlaneId::Bar(_) | PlaneId::Bezel => Band::Rail,
        PlaneId::Composer | PlaneId::Caret | PlaneId::Face => Band::Prompt,
    }
}

fn gauge_ink(area: Rect, share: u8, turned: bool, palette: &Palette) -> Placement {
    Placement::gauge(
        area,
        Some(CELL),
        instrument::Gauge {
            share: instrument::Share::of(share),
            turned,
        },
        palette,
    )
    .expect("an instrument to shade")
}

fn bar_ink(area: Rect, bar: Bar, share: u8, palette: &Palette) -> Placement {
    Placement::bar(area, Some(CELL), bar, instrument::Share::of(share), palette)
        .expect("an instrument to shade")
}

fn instruments(
    area: Rect,
    cell: Option<CellSize>,
    palette: &Palette,
) -> Vec<(&'static str, Option<Placement>)> {
    let share = instrument::Share::of(50);
    vec![
        (
            "a gauge",
            Placement::gauge(
                area,
                cell,
                instrument::Gauge {
                    share,
                    turned: false,
                },
                palette,
            ),
        ),
        (
            "a bar",
            Placement::bar(area, cell, Bar::Input, share, palette),
        ),
        ("a bezel", Placement::bezel(area, cell, palette)),
    ]
}

fn bezelled(area: Rect, palette: &Palette) -> Placement {
    Placement::bezel(area, Some(CELL), palette).expect("an instrument to shade")
}

fn caret_light(band: Rect, caret: u16, palette: &Palette) -> Placement {
    Placement::caret(band, Some(CELL), caret, palette).expect("the light the caret casts")
}

fn shown(at: Placement) -> image::math::Rect {
    at.shows()
        .expect("a placement that stands on a picture of its own")
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
                    band.shade().row(row),
                    palette.band(Band::Prompt),
                    "{spelling} draws row {row} of a {rows}-row band on a tone other than the \
                     prompt's own, and the band the draft is typed in is the band a prompt is \
                     read on"
                );
            }
            assert_eq!(
                band.shade().row(rows - 1),
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
            shaded(1, &palette).shade().row(0),
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
                let ground = band.shade().row(row);
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
                    luminance(band.shade().row(row)) >= well,
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
                band.shade().row(row),
                Color::Reset,
                "row {row} of a {rows}-row band draws a ground, and a palette that asked for no \
                 colour has none to draw"
            );
        }
    }
}

#[test]
fn a_seams_joint_is_darker_than_both_bands_and_ends_in_a_hairline_on_its_own() {
    for (spelling, palette) in SPELLED {
        let well = spelled(palette.band(Band::Well));
        for (axis, placement) in parted_bands(&palette, CELL) {
            let plane = drawn(placement);
            let weight = weight_of(&palette);
            let extent = seam_extent(axis, CELL);
            let rung = spelled(placement.shade().row(0));
            let lit = |along: u32| {
                let (x, y) = along_seam(axis, along);
                let pixel = plane.pixels().get_pixel(x, y);
                [pixel[0], pixel[1], pixel[2]]
            };

            let joint = lit(0);
            for channel in 0..3 {
                assert!(
                    joint[channel] < rung[channel] && joint[channel] < well[channel],
                    "{spelling} draws channel {channel} of the joint at {} where the band it \
                     parts is drawn on {} and the well beside it on {}: the joint is a shadow \
                     under both bands",
                    joint[channel],
                    rung[channel],
                    well[channel]
                );
            }

            let hairline = lit(extent - 1);
            for channel in 0..3 {
                assert!(
                    hairline[channel] > rung[channel] && hairline[channel] > well[channel],
                    "{spelling} draws channel {channel} of the last pixel of the seam's cell at \
                     {} where the band it stands on is drawn on {} and the well beside it on {}: \
                     the seam ends in a hairline lifted off the band",
                    hairline[channel],
                    rung[channel],
                    well[channel]
                );
            }

            for along in 1..extent - 1 {
                let (here, nearer) = (lit(along)[0], lit(along - 1)[0]);
                assert!(
                    here >= nearer,
                    "{spelling} draws the pixel {along} away from the joint at {here} where the \
                     pixel nearer the joint carries {nearer}: the shadow rises out of the joint \
                     to the band and never steps back down into it"
                );
                let step = i32::from(here) - i32::from(nearer);
                assert!(
                    step <= weight / 2 + DITHER_STEP,
                    "{spelling} draws the pixel {along} away from the joint {step} channels over \
                     the pixel beside it: the joint is a fall of {weight} channels and not a step \
                     of them"
                );
            }
        }
    }
}

#[test]
fn a_seam_falls_at_one_rate_across_its_cell_on_both_axes() {
    let palette = Palette::TRUECOLOR;
    for cell in [CELL, CellSize::new(12, 12), CellSize::new(8, 30)] {
        let mut halves: Vec<u32> = Vec::new();
        for (axis, placement) in parted_bands(&palette, cell) {
            let plane = drawn(placement);
            let weight = weight_of(&palette);
            let extent = seam_extent(axis, cell);
            let along = (0..extent)
                .find(|along| {
                    let (x, y) = along_seam(axis, *along);
                    lit_over(&placement, &plane, cell, x, y) <= weight / 2
                })
                .unwrap_or(extent - 1);
            halves.push(along * 100 / extent);
        }
        assert!(
            halves[0].abs_diff(halves[1]) <= 100 / 8,
            "the rail's column is half its weight {} per cent of the way across its cell and the \
             composer's row {} per cent of the way across its own, at {} by {} pixels: the two \
             axes draw one fall, spent over the seam's own cell rather than over a run of pixels",
            halves[0],
            halves[1],
            cell.width(),
            cell.height()
        );
    }
}

#[test]
fn a_seams_fall_stands_in_the_seams_own_cell_whatever_the_band_is_wide() {
    let palette = Palette::TRUECOLOR;
    for cell in [CELL, CellSize::new(12, 12)] {
        for columns in [3u16, 4, 6, 10] {
            let placement = railed(
                Rail::Ledger,
                Rect::new(20, 0, columns, 6),
                Some(cell),
                &palette,
            )
            .expect("a band to shade");
            keeps_its_fall(
                placement,
                &palette,
                cell,
                &format!("a rail of {columns} columns"),
            );
        }
        for rows in [2u16, 5] {
            let placement = Placement::composer(Rect::new(0, 20, 40, rows), Some(cell), &palette)
                .expect("a band to shade");
            keeps_its_fall(placement, &palette, cell, &format!("a band of {rows} rows"));
        }
    }
}

#[test]
fn a_bands_plane_is_its_shade_drawn_at_the_pixels_of_the_cell_it_covers() {
    for (spelling, palette) in SPELLED {
        for (band, placement) in bands(&palette) {
            let plane = drawn(placement);
            let peak = palette.peak(band_of(&placement));
            assert_eq!(
                plane.pixels().dimensions(),
                (
                    u32::from(placement.cells().width) * CELL.width(),
                    u32::from(placement.cells().height) * CELL.height()
                ),
                "{spelling} draws {band}'s plane at other than the pixels of the cells the band \
                 stands on, and a plane covers the band whose shade it is drawn from"
            );
            for row in 0..placement.cells().height {
                let rung = spelled(placement.shade().row(row));
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
        let plane =
            drawn(Placement::composer(area, Some(CELL), &palette).expect("a band to shade"));
        let prompt = spelled(palette.band(Band::Prompt));
        let rail = spelled(palette.band(Band::Rail));
        let well = spelled(palette.band(Band::Well));

        let top = middle(&plane, 0, CELL);
        assert!(
            near(top, prompt),
            "{spelling} draws the middle of the head cell at {top:?} where the prompt's own band \
             is {prompt:?}: the seam the row is drawn as is spent by the middle of its cell, and \
             no pixel row of the well is drawn above the band"
        );
        let under = middle(&plane, 1, CELL);
        assert!(
            near(under, prompt),
            "{spelling} opens the composer's plane on {under:?} under the seam's row where the \
             prompt's own band is {prompt:?}, so no pixel row of the well is drawn above the band"
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
    let placement = faced(Rail::Ledger, area, 3, Some(CELL), &palette);
    let Some(face) = RailBand::of(area).with_picture(area.width, 3).picture else {
        panic!("a rail a face stands on reserves the cells its light is centred on");
    };
    let plane = drawn(placement);
    let peak = palette.peak(Band::Rail);
    let (width, height) = plane.pixels().dimensions();
    let (across, down) = (CELL.width(), CELL.height());
    let lit = |x: u32, y: u32| light_over(&placement, &plane, x, y);
    let outside = |x: u32, y: u32| !in_seam(Axis::Down, CELL, x, y);
    let (most, (left, top)) = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|(x, y)| outside(*x, *y))
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
            if (inside && y < foot) || !outside(x, y) {
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
            (left..width)
                .map(|x| (x, top))
                .filter(|(x, y)| outside(*x, *y))
                .collect::<Vec<(u32, u32)>>(),
            "across the rail, away from the face towards its further margin",
        ),
        (
            (0..=left)
                .rev()
                .map(|x| (x, top))
                .filter(|(x, y)| outside(*x, *y))
                .collect(),
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
        let placement = railed(Rail::Ledger, area, Some(CELL), &palette).expect("a band to shade");
        let plane = drawn(placement);
        let tone = spelled(palette.band(Band::Rail));
        let (width, height) = plane.pixels().dimensions();

        for y in 0..height {
            for x in 0..width {
                if in_seam(Axis::Down, CELL, x, y) {
                    continue;
                }
                let pixel = plane.pixels().get_pixel(x, y);
                assert_eq!(
                    [pixel[0], pixel[1], pixel[2]],
                    tone,
                    "{spelling} draws pixel row {y}, column {x} of a rail that carries no face \
                     at {:?} where the rail's own tone is {tone:?}: no face stands on it, so no \
                     pixel of it away from its seam is lit",
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
fn the_composers_plane_carries_no_light_of_the_caret_wherever_it_waits() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let placement = Placement::composer(area, Some(CELL), &palette).expect("a band to shade");
        let plane = drawn(placement);
        let (width, height) = plane.pixels().dimensions();

        for y in 0..height {
            let rung = spelled(placement.shade().row((y / CELL.height()) as u16));
            for x in 0..width {
                if in_seam(Axis::Across, CELL, x, y) {
                    continue;
                }
                let pixel = plane.pixels().get_pixel(x, y);
                let carried = i32::from(pixel[0]) - i32::from(rung[0]);
                assert_eq!(
                    carried, 0,
                    "{spelling} draws pixel row {y}, column {x} of the composer's band {carried} \
                     channels over the {rung:?} it sits on, away from the seam its head row is: \
                     the light the caret casts is an image of its own, and a keystroke places it \
                     again and repaints no band"
                );
            }
        }
    }
}

#[test]
fn the_carets_light_peaks_on_the_row_under_the_seam_and_is_spent_by_its_foot() {
    for (spelling, palette) in SPELLED {
        let area = Rect::new(0, 20, 80, 5);
        let placement = caret_light(area, 12, &palette);
        let plane = drawn(placement);
        let (width, height) = plane.pixels().dimensions();
        let centre = light_centre();
        let down = CELL.height();
        let peak = palette.peak(Band::Prompt);
        let carried = |x: u32, y: u32| light_over(&placement, &plane, x, y);

        assert_eq!(
            (width, height),
            (
                u32::from(LIGHT_COLS) * CELL.width(),
                u32::from(area.height - 1) * down
            ),
            "{spelling} draws the caret's light at other than the pixels of the {LIGHT_COLS} \
             cells it spans over the band's own rows under its seam, and the row a band is \
             headed by is the seam the band is parted from"
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
        for (band, at, placement) in lit_bands(&palette) {
            let plane = drawn(placement);
            let (width, height) = plane.pixels().dimensions();
            let down = CELL.height();
            let most = i32::from(palette.lift(at));
            let seam = placement.seam().map(|(axis, _)| axis);

            for y in 0..height {
                let ground = spelled(placement.shade().row((y / down) as u16));
                for x in 0..width {
                    let pixel = plane.pixels().get_pixel(x, y);
                    let drawn = [pixel[0], pixel[1], pixel[2]];
                    // A channel the shadow has drained to the floor says nothing of the light.
                    let carried: Vec<i32> = (0..3)
                        .filter(|channel| drawn[*channel] > 0)
                        .map(|channel| i32::from(drawn[channel]) - i32::from(ground[channel]))
                        .collect();
                    let one = carried.first().copied().unwrap_or(0);
                    assert!(
                        carried.iter().all(|light| *light == one),
                        "{spelling} draws pixel row {y}, column {x} of {band} at {drawn:?} where \
                         the row it falls in is {ground:?}: a plane's pixel is its band's own rung \
                         with one light on every channel — never a blend of two rungs"
                    );
                    let parted = seam.is_some_and(|axis| in_seam(axis, CELL, x, y));
                    let weight = weight_of(&palette);
                    assert!(
                        match parted {
                            true => (-weight..=weight).contains(&one),
                            false => (0..=most).contains(&one),
                        },
                        "{spelling} draws pixel row {y}, column {x} of {band} {one} channels over \
                         its rung, where a band away from its seam takes no more than the {most} \
                         the palette hands it and the joint itself takes no more than the {weight} \
                         it is proved to take either way"
                    );
                }
            }
        }
    }
}

#[test]
fn no_pixel_of_a_band_is_under_its_rung_but_at_the_joint_it_is_parted_from() {
    for (spelling, palette) in SPELLED {
        for (band, _, placement) in lit_bands(&palette) {
            let plane = drawn(placement);
            let (width, height) = plane.pixels().dimensions();
            let down = CELL.height();
            let seam = placement.seam().map(|(axis, _)| axis);

            for y in 0..height {
                let ground = spelled(placement.shade().row((y / down) as u16));
                for x in 0..width {
                    if seam.is_some_and(|axis| in_seam(axis, CELL, x, y)) {
                        continue;
                    }
                    let pixel = plane.pixels().get_pixel(x, y);
                    for channel in 0..3 {
                        assert!(
                            pixel[channel] >= ground[channel],
                            "{spelling} draws channel {channel} of pixel row {y}, column {x} of \
                             {band} at {} under the {} of the rung it is drawn in: a band's light \
                             only ever adds where no seam parts it, so no pixel of the band sits \
                             under its own rung",
                            pixel[channel],
                            ground[channel]
                        );
                    }
                }
            }

            let Some(axis) = seam else {
                continue;
            };
            let (x, y) = along_seam(axis, 0);
            let ground = spelled(placement.shade().row(0));
            let joint = plane.pixels().get_pixel(x, y);
            for channel in 0..3 {
                assert!(
                    joint[channel] < ground[channel],
                    "{spelling} draws channel {channel} of the joint of {band} at {} where the \
                     band it parts is drawn on {}: the joint is a shadow under the band",
                    joint[channel],
                    ground[channel]
                );
            }
        }
    }
}

#[test]
fn no_two_pixels_of_a_band_are_a_step_apart_the_eye_can_find() {
    for (spelling, palette) in SPELLED {
        for (band, _, placement) in lit_bands(&palette) {
            let plane = drawn(placement);
            let (width, height) = plane.pixels().dimensions();
            let down = CELL.height();
            let rung = |y: u32| spelled(placement.shade().row((y / down) as u16));
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

            let seam = placement.seam().map(|(axis, _)| axis);
            // A seam's cell is a line the frame draws — the shadow at the joint and the hairline
            // that closes it — and no fall of the band's, so the walk of a band skips it.
            let outside = |x: u32, y: u32| !seam.is_some_and(|axis| in_seam(axis, CELL, x, y));

            for y in 0..height {
                for x in 0..width {
                    if x + 1 < width && outside(x, y) && outside(x + 1, y) {
                        assert!(
                            apart(at(x, y), at(x + 1, y)),
                            "{spelling} draws pixel row {y} of {band} at {:?} in column {x} and \
                             {:?} in the column beside it: a fall of light is spent by the pixel, \
                             so no two neighbouring pixels are a step the eye can find",
                            at(x, y),
                            at(x + 1, y)
                        );
                    }
                    if y + 1 < height
                        && rung(y) == rung(y + 1)
                        && outside(x, y)
                        && outside(x, y + 1)
                    {
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
        let placement = Placement::transcript(area, Some(CELL), &palette)
            .expect("a band the plane stands under");
        let plane = drawn(placement);
        let peak = palette.peak(Band::Well);
        let (width, height) = plane.pixels().dimensions();
        let down = CELL.height();
        let lit = |x: u32, y: u32| light_over(&placement, &plane, x, y);

        assert_eq!(
            (width, height),
            (
                u32::from(area.width) * CELL.width(),
                u32::from(area.height) * down
            ),
            "{spelling} draws the transcript's plane at other than the pixels of the cells the \
             band stands on, and a plane covers the band whose shade it is drawn from"
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
            let placement = Placement::transcript(area, cell, &palette).expect("a band of rows");
            let mut buffer = Buffer::empty(area);
            let ground = match placement.places() {
                true => own,
                false => palette.band(Band::Well),
            };

            assert_eq!(
                placement.places(),
                cell.is_some(),
                "{spelling} draws the transcript band under a plane exactly when it is handed the \
             cell to draw one at"
            );
            assert!(
                placement.seam().is_none(),
                "the transcript band meets no band beside it in its own columns, so it parts nothing"
            );
            placement.claim(&mut buffer, &palette);

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
                let placement = railed(shape, area, None, &palette).expect("a band to shade");
                let mut buffer = Buffer::empty(Rect {
                    x: 0,
                    width: area.right(),
                    ..area
                });
                placement.claim(&mut buffer, &palette);

                for row in 0..area.height {
                    assert_eq!(
                        placement.shade().row(row),
                        tone,
                        "{spelling} draws row {row} of a {shape:?} of {height} rows on another \
                         colour than the rail's own tone: a row of the rail is no shadow, so an \
                         ink drawn there reads at one strength"
                    );
                    for x in area.x..area.right() {
                        assert_eq!(
                            under(&buffer, x, area.y + row),
                            match x == area.x && placement.seam().is_some() {
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
    let placement = railed(Rail::Ledger, band, Some(CELL), &palette).expect("a band to shade");
    let frame = Rect::new(0, 0, COLUMNS, band.height);
    let mut buffer = Buffer::empty(frame);

    assert_eq!(
        placement.cells(),
        band,
        "the rail's plane covers the rail's own columns, and the seam is drawn in the first of \
         them rather than in a column beside"
    );
    pen::claim(&mut buffer, frame, palette.ground(Some(Band::Well)));
    placement.claim(&mut buffer, &palette);

    for x in 0..band.x {
        assert_eq!(
            under(&buffer, x, 0),
            palette.band(Band::Well),
            "column {x} of the transcript band is not the band's own: no claim of the rail's is \
             made past the columns it is drawn in, and the crease the well fell into is gone"
        );
    }
    let own = palette.ground(None).bg.expect("the terminal's own ground");
    for y in band.y..band.bottom() {
        for x in band.x..band.right() {
            assert_eq!(
                under(&buffer, x, y),
                own,
                "row {y}, column {x} of the band is not the terminal's own ground, which its \
                 plane is read through — the column of its seam among them, where the fall into \
                 the joint is drawn"
            );
        }
    }
}

#[test]
fn a_strip_drawn_in_cells_makes_no_seam_at_all() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(20, 0, 40, 1);
    let placement = railed(Rail::Strip, area, None, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(Rect {
        x: 0,
        width: area.right(),
        ..area
    });

    assert!(
        !placement.places(),
        "a strip is one row of the rail's own cells"
    );
    assert!(
        placement.seam().is_none(),
        "a strip lies under the transcript and parts nothing, so it makes no seam"
    );
    placement.claim(&mut buffer, &palette);

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
fn the_composers_band_is_claimed_row_by_row_from_its_placement() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, 40, 6);
    let placement = Placement::composer(area, None, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(area);

    assert!(
        !placement.places(),
        "a terminal that places no picture is handed no plane under the band"
    );
    placement.claim(&mut buffer, &palette);

    for row in 0..area.height {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, area.y + row),
                match row {
                    0 => palette.edge_ink(),
                    row => placement.shade().row(row),
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
    let placement = Placement::composer(area, Some(CELL), &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(area);

    assert!(
        placement.places(),
        "a terminal that places a picture and a palette that spells a pixel is handed a plane"
    );
    placement.claim(&mut buffer, &palette);

    let own = palette.ground(None).bg.expect("the terminal's own ground");
    for row in 0..area.height {
        for x in area.x..area.right() {
            assert_eq!(
                under(&buffer, x, area.y + row),
                own,
                "column {x} of row {row} is not the terminal's own ground, so the image drawn \
                 under it is read through a cell that is not its own — the row the seam heads \
                 among them, which the plane draws its parting in"
            );
        }
    }
}

#[test]
fn a_ledgers_seam_is_its_own_first_column_and_takes_no_column_beside_it() {
    let palette = Palette::TRUECOLOR;
    let rail = Rect::new(20, 2, 32, 8);
    let placement = railed(Rail::Ledger, rail, None, &palette).expect("a band to shade");

    assert_eq!(
        placement.seam(),
        Some((Axis::Down, Rect { width: 1, ..rail })),
        "the seam is one column of the rail's own down every row it claims, and no column of the \
         transcript band's"
    );
}

#[test]
fn a_seams_column_is_claimed_in_the_ink_it_is_drawn_in_and_the_rail_keeps_its_tone_beside_it() {
    for palette in [Palette::TRUECOLOR, Palette::ANSI] {
        let rail = Rect::new(20, 2, 32, 8);
        let frame = Rect {
            x: 0,
            width: rail.right(),
            ..rail
        };
        for cell in [None, Some(CELL)] {
            let placement = railed(Rail::Ledger, rail, cell, &palette).expect("a band to shade");
            let mut buffer = Buffer::empty(frame);
            placement.claim(&mut buffer, &palette);

            for y in rail.y..rail.bottom() {
                assert_eq!(
                    under(&buffer, rail.x, y),
                    match placement.places() {
                        true => palette.ground(None).bg.expect("the terminal's own ground"),
                        false => palette.edge_ink(),
                    },
                    "row {y} of the rail's first column is not the colour its seam is claimed in, \
                     and that column is the seam the rail is parted from the transcript band by"
                );
                for x in rail.x + 1..rail.right() {
                    let wanted = match placement.places() {
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
fn a_seam_is_claimed_in_the_one_edge_ink_where_no_plane_stands() {
    let rail = Rect::new(20, 2, 32, 8);
    let frame = Rect {
        x: 0,
        width: rail.right(),
        ..rail
    };

    for (spelling, palette) in SPELLED {
        let placement = railed(Rail::Ledger, rail, None, &palette).expect("a band to shade");
        let mut buffer = Buffer::empty(frame);

        assert!(
            !placement.places(),
            "{spelling} draws a plane under a band this terminal can place no picture on"
        );
        placement.claim(&mut buffer, &palette);
        let (axis, seam) = placement.seam().expect("the seam the rail is parted by");

        assert_eq!(
            axis,
            Axis::Down,
            "{spelling} parts the rail from the transcript band on another axis than the column \
             between them"
        );
        for y in seam.y..seam.bottom() {
            for x in seam.x..seam.right() {
                assert_eq!(
                    under(&buffer, x, y),
                    palette.edge_ink(),
                    "{spelling} draws row {y}, column {x} of the seam in something other than the \
                     one edge ink: where no plane stands the seam is claimed in the cells of the \
                     band it parts"
                );
            }
            assert_eq!(
                under(&buffer, rail.x + 1, y),
                palette.band(Band::Rail),
                "{spelling} draws row {y} of the column beside the seam in anything but the \
                 rail's own tone, and the seam takes the one column"
            );
        }
    }
}

#[test]
fn a_band_a_plane_is_placed_under_gives_its_seam_cell_to_the_pixels() {
    let palette = Palette::TRUECOLOR;
    let own = palette.ground(None).bg.expect("the terminal's own ground");

    for (axis, placement) in parted_bands(&palette, CELL) {
        let area = placement.cells();
        let mut buffer = Buffer::empty(area);

        assert!(
            placement.places(),
            "the terminal places a picture under the band, and a seam's cell is a pixel of it"
        );
        placement.claim(&mut buffer, &palette);
        let (_, seam) = placement.seam().expect("the seam the band is parted by");

        for y in seam.y..seam.bottom() {
            for x in seam.x..seam.right() {
                assert_eq!(
                    under(&buffer, x, y),
                    own,
                    "the {axis:?} seam's cell at row {y}, column {x} is claimed in a colour of \
                     its own, so the pixels the seam is drawn in are read through no cell"
                );
            }
        }
    }
}

#[test]
fn the_composers_seam_is_the_row_that_heads_its_band() {
    let palette = Palette::TRUECOLOR;
    let band = Rect::new(0, 20, COLUMNS, 4);
    let placement = Placement::composer(band, None, &palette).expect("a band to shade");
    let mut buffer = Buffer::empty(band);

    assert_eq!(
        placement.seam(),
        Some((Axis::Across, Rect { height: 1, ..band })),
        "the composer's band is headed by the one row that parts it from the rail above it"
    );
    placement.claim(&mut buffer, &palette);
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
                placement.shade().row(row),
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
    let placement = railed(Rail::Ledger, rail, None, &Palette::SLOTS).expect("a band to shade");
    let mut buffer = Buffer::empty(frame);

    assert!(
        !Palette::SLOTS.draws_edge(),
        "and this palette spells no edge"
    );
    placement.claim(&mut buffer, &Palette::SLOTS);
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
    let placement = railed(Rail::Ledger, rail, None, &palette).expect("a band to shade");
    let frame = Rect {
        x: 0,
        width: rail.right(),
        ..rail
    };
    let mut buffer = Buffer::empty(frame);
    let words = "a fact the rail draws";

    pen::claim(&mut buffer, frame, palette.ground(Some(Band::Rail)));
    placement.claim(&mut buffer, &palette);
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
    let placement =
        Placement::composer(area, Some(CELL), &Palette::TRUECOLOR).expect("a band to shade");
    let mut planes = held(&[placement]);

    assert_eq!(planes.made(), 1, "the first pass makes the band's image");
    planes.hold(&[placement]);
    planes.hold(&[placement]);

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
            Placement::transcript(area, None, &palette).expect("a band of rows"),
        ),
        (
            "the composer's band",
            Placement::composer(area, None, &palette).expect("a band to shade"),
        ),
    ];
    let placeless = held(&bare.map(|(_, placement)| placement));

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

    let pictured =
        held(&[Placement::transcript(area, Some(CELL), &palette).expect("a band of rows")]);
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
fn a_seam_that_did_not_move_makes_no_image_and_a_band_of_another_shape_does() {
    let palette = Palette::TRUECOLOR;
    let rail = |columns| {
        railed(
            Rail::Ledger,
            Rect::new(20, 0, columns, 6),
            Some(CELL),
            &palette,
        )
        .expect("a band to shade")
    };
    let mut planes = held(&[rail(32u16)]);

    assert_eq!(planes.made(), 1, "the first pass makes the band's image");
    planes.hold(&[rail(32)]);
    assert_eq!(
        planes.made(),
        1,
        "a pass whose seam stands in the same pixels makes no image of the band"
    );

    planes.hold(&[rail(30)]);
    assert_eq!(
        (planes.made(), planes.held().len()),
        (2, 1),
        "a rail of another width puts its seam in another column of the pixels, so the band is \
         drawn again and the image it stood on is taken away"
    );

    let folded = Placement::composer(Rect::new(0, 20, 40, 6), Some(CELL), &palette)
        .expect("a band to shade");
    planes.hold(&[folded]);
    planes.hold(&[
        Placement::composer(Rect::new(0, 20, 40, 3), Some(CELL), &palette)
            .expect("a band to shade"),
    ]);
    assert_eq!(
        planes.made(),
        4,
        "a fold that gives the composer's band another foot moves the seam its head row is, so \
         the band is drawn again and no joint of the one before it is left standing"
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
            Placement::composer(band, Some(CELL), &palette).expect("a band to shade"),
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
    let place = |caret| caret_light(band, caret, &palette);
    let shows = |placement: &Placement| {
        placement
            .shows()
            .expect("a light with cells of the band to stand on")
    };

    let middle = place(24);
    assert_eq!(
        (middle.cells().x, middle.cells().width, shows(&middle).x),
        (12, LIGHT_COLS, 0),
        "a caret with room either side of it is the whole of the light, and it starts where its \
         own fall does"
    );
    assert_eq!(
        middle.cells().x + LIGHT_COLS / 2,
        24,
        "and it is placed so that its own centre falls on the caret's cell"
    );

    let left = place(4);
    assert_eq!(
        (left.cells().x, left.cells().width),
        (4, LIGHT_COLS - 12),
        "a caret at the band's first column keeps the light's own centre on it and shows only the \
         columns of it the band has room for"
    );
    assert_eq!(
        shows(&left).x,
        12 * CELL.width(),
        "and the light it shows begins the twelve columns the band has no room for in"
    );
    assert_eq!(
        (left.cells().y, left.cells().height),
        (21, 3),
        "the light stands on the band's own rows under the row its seam heads, and the seam's own \
         row is no band for it to light"
    );

    let right = place(43);
    assert_eq!(
        (right.cells().x, right.cells().width, shows(&right).x),
        (31, LIGHT_COLS - 12, 0),
        "a caret at the band's last column loses the far columns of the light and no cell of it"
    );
    for placement in [middle, left, right] {
        assert_eq!(
            shows(&placement).width,
            u32::from(placement.cells().width) * CELL.width(),
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
    let placement = caret_light(band, 12, &palette);
    let mut buffer = Buffer::empty(band);
    let ground = Color::Rgb(0x0a, 0x0b, 0x0c);
    pen::claim(&mut buffer, band, palette.over(ground));

    assert!(
        placement.places(),
        "the light a caret casts is placed under the words of the band it lights"
    );
    placement.claim(&mut buffer, &palette);

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

fn proved(palette: &Palette) -> u8 {
    palette.lift(Band::Rail)
}

fn over(placement: &Placement, pixel: [u8; 4]) -> [i32; 3] {
    let rung = spelled(placement.shade().row(0));
    std::array::from_fn(|channel| i32::from(pixel[channel]) - i32::from(rung[channel]))
}

#[test]
fn a_gauges_fill_is_the_pixels_of_the_share_it_is_drawn_at_and_its_trace_the_pixel_that_ends_it() {
    let area = Rect::new(4, 7, 8, 1);
    let width = u32::from(area.width) * CELL.width();
    for (spelling, palette) in SPELLED {
        let proved = proved(&palette);
        for percent in [0, 13, 50, 87, 100] {
            let placement = gauge_ink(area, percent, false, &palette);
            let plane = drawn(placement);
            let at = placement.shows().expect("the gauge's ladder to be placed");
            let fill = instrument::Share::of(percent).pixels(width);
            assert_eq!(
                (at.x, at.width),
                (0, width),
                "{spelling} crops a gauge at {percent}% from anywhere but the first band of the \
                 {width} pixels it is drawn over"
            );
            let fill_light = palette.peak(Band::Rail) / 4 * 3;
            let mut traced = None;
            for x in 0..width {
                let pixel = plane.pixels().get_pixel(at.x + x, at.y).0;
                if pixel[3] == 0 {
                    assert!(
                        x >= fill,
                        "{spelling} leaves column {x} of a gauge at {percent}% to the band it \
                         stands on, and no pixel of its {fill} of fill is drawn"
                    );
                    continue;
                }
                let light = over(&placement, pixel);
                if light.iter().any(|channel| *channel < 0) {
                    panic!(
                        "{spelling} draws column {x} of a gauge at {percent}% darker than the rung \
                         it stands on: {:?}",
                        &pixel[..3]
                    );
                }
                if light.iter().any(|channel| *channel > i32::from(proved)) {
                    panic!(
                        "{spelling} draws column {x} of a gauge at {percent}% at {light:?} \
                         channels over its rung, and the most the palette proves the rung can \
                         take is {proved}"
                    );
                }
                if x + 1 == fill {
                    traced = Some(light);
                } else if x < fill {
                    assert!(
                        light
                            .iter()
                            .all(|channel| channel.abs_diff(i32::from(fill_light)) <= 1),
                        "{spelling} draws column {x} of a gauge at {percent}% in {light:?} \
                         channels of light, where every pixel of its fill spends the one \
                         {fill_light} of the {proved} the rail proves, in the hue of the ink the \
                         fact is stated in"
                    );
                }
            }
            match traced {
                None => assert_eq!(fill, 0, "a gauge of no pixels is one no fill"),
                Some(light) => {
                    let peak = palette.peak(Band::Rail);
                    let spent = light.iter().copied().max().expect("three channels");
                    assert_eq!(
                        spent,
                        i32::from(peak),
                        "{spelling} ends a gauge at {percent}% on a trace of {light:?} channels, \
                         where the pixel that ends the fill spends all of the {peak} the rail's \
                         light is proved at of the {proved} the palette proves"
                    );
                    assert!(
                        peak <= proved,
                        "{spelling} spends {peak} channels of light where the most the palette \
                         proves the rail's rung can take is {proved}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_gauges_track_is_ticked_on_the_quarters_its_fill_has_not_reached() {
    let area = Rect::new(0, 0, 8, 1);
    let palette = Palette::TRUECOLOR;
    let width = u32::from(area.width) * CELL.width();
    let placement = gauge_ink(area, 50, false, &palette);
    let plane = drawn(placement);
    let fill = instrument::Share::of(50).pixels(width);
    let quarters: Vec<u32> = (1..4)
        .map(|quarter| quarter * (width / 4))
        .filter(|quarter| *quarter >= fill)
        .collect();
    let ticks: Vec<u32> = (fill..width)
        .filter(|x| plane.pixels().get_pixel(*x, 0).0[3] == 255)
        .collect();
    assert_eq!(
        ticks, quarters,
        "a gauge is ticked on every quarter of the {width}-pixel track its {fill} pixels of fill \
         have not reached, and never through the fill"
    );
    let at = placement.shows().expect("the gauge's ladder to be placed");
    let fill_light = over(&placement, plane.pixels().get_pixel(0, at.y).0);
    for x in quarters {
        let tick = over(&placement, plane.pixels().get_pixel(x, 0).0);
        assert!(
            tick.iter().zip(fill_light).all(|(tick, fill)| tick < &fill),
            "column {x} of the track is drawn at {tick:?} channels of light, and a tick is \
             quieter than the {fill_light:?} of the fill it stands beside"
        );
    }
}

#[test]
fn a_gauges_ladder_holds_every_hundredth_of_the_block_and_its_hundredth_rung_the_whole_width() {
    let area = Rect::new(0, 0, 8, 1);
    let palette = Palette::TRUECOLOR;
    let width = u32::from(area.width) * CELL.width();
    let placement = gauge_ink(area, 50, false, &palette);
    let plane = drawn(placement);
    assert_eq!(
        plane.pixels().dimensions(),
        (width * 2, 101),
        "a gauge's ladder is two bands of the {width} pixels it is drawn over, and one rung per \
         hundredth of the block"
    );
    for rung in 0..101u32 {
        let fill = instrument::Share::of(u8::try_from(rung).expect("a rung")).pixels(width);
        for x in 0..fill {
            assert!(
                plane.pixels().get_pixel(x, rung).0[3] == 255,
                "rung {rung} of the ladder leaves column {x} of its {fill} pixels of fill to the \
                 band, where its hundredth of the block is {fill}"
            );
        }
    }
}

#[test]
fn a_gauges_second_band_is_the_ink_the_frames_own_meter_turns_to() {
    let area = Rect::new(2, 5, 8, 1);
    let palette = Palette::TRUECOLOR;
    let width = u32::from(area.width) * CELL.width();
    let calm = gauge_ink(area, 60, false, &palette);
    let turned = gauge_ink(area, 60, true, &palette);
    let at = turned.shows().expect("the gauge's ladder to be placed");
    assert_eq!(
        (calm.shows().expect("a placement").x, at.x),
        (0, width),
        "a gauge whose run is near its ceiling is placed on the second band of its ladder, where \
         one that is not is placed on the first"
    );
    assert_eq!(
        at.y,
        calm.shows().expect("a placement").y,
        "and the band it turns to is no rung of its own"
    );
    let calm_light = over(&calm, drawn(calm).pixels().get_pixel(20, at.y).0);
    let turned_light = over(&turned, drawn(turned).pixels().get_pixel(at.x + 20, at.y).0);
    let (low, high) = (
        *calm_light.iter().min().expect("three channels"),
        *calm_light.iter().max().expect("three channels"),
    );
    assert!(
        high - low <= 1,
        "the light a calm run's fill spends is the plain ink's own, which is grey: {calm_light:?}"
    );
    assert!(
        turned_light[2] > turned_light[1] && turned_light[1] > turned_light[0],
        "a run near its ceiling spends the accent's hue, which is blue before it is green and \
         green before it is red, and this fill is {turned_light:?}"
    );
}

#[test]
fn a_bar_is_the_pixels_its_figure_takes_and_no_pixel_beside_them() {
    let area = Rect::new(12, 30, 6, 1);
    let width = u32::from(area.width) * CELL.width();
    for (spelling, palette) in SPELLED {
        let proved = proved(&palette);
        for bar in [Bar::Input, Bar::Output] {
            for percent in [0, 25, 50, 100] {
                let placement = bar_ink(area, bar, percent, &palette);
                let plane = drawn(placement);
                let at = placement.shows().expect("the bar's ladder to be placed");
                let fill = instrument::Share::of(percent).pixels(width);
                assert_eq!(
                    (at.x, at.width, at.height),
                    (0, width, 1),
                    "{spelling} crops a {bar:?} bar from a band other than its own, or over \
                     pixels other than the {width} it is drawn on"
                );
                assert_eq!(
                    placement.shade().id(),
                    PlaneId::Bar(bar),
                    "{spelling} holds the {bar:?} bar under an identity of its own"
                );
                for x in 0..width {
                    let pixel = plane.pixels().get_pixel(x, at.y).0;
                    if x >= fill {
                        assert_eq!(
                            pixel,
                            [0, 0, 0, 0],
                            "{spelling} draws column {x} of a {bar:?} bar at {percent}% outside \
                             the {fill} pixels its figure takes"
                        );
                        continue;
                    }
                    let light = over(&placement, pixel);
                    assert!(
                        light.iter().all(|channel| *channel >= 0),
                        "{spelling} draws column {x} of a {bar:?} bar at {percent}% darker than \
                         the rung it stands on: {light:?}"
                    );
                    let (low, high) = (
                        light.iter().copied().min().expect("three channels"),
                        light.iter().copied().max().expect("three channels"),
                    );
                    assert!(
                        high - low <= 2,
                        "{spelling} draws the {bar:?} bar at {percent}% in {light:?} channels of \
                         light, where the ink the foot states its figures in is grey"
                    );
                    assert!(
                        light.iter().all(|channel| *channel <= i32::from(proved)),
                        "{spelling} draws column {x} of a {bar:?} bar at {percent}% at {light:?} \
                         channels of light, where the most the palette proves the rung can take \
                         is {proved}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_bezel_is_a_groove_inside_the_region_it_frames() {
    let area = Rect::new(3, 9, 5, 3);
    let palette = Palette::TRUECOLOR;
    let (width, height) = (
        u32::from(area.width) * CELL.width(),
        u32::from(area.height) * CELL.height(),
    );
    let placement = bezelled(area, &palette);
    let plane = drawn(placement);
    let at = placement.shows().expect("the bezel to be placed");
    assert_eq!(
        (
            placement.cells().x,
            placement.cells().y,
            at.x,
            at.y,
            at.width,
            at.height
        ),
        (area.x, area.y, 0, 0, width, height),
        "a bezel is the region's own pixels, placed over the region it frames"
    );
    assert_eq!(plane.pixels().dimensions(), (width, height));

    let tone = spelled(palette.band(Band::Rail));
    let weight = palette.seam();
    let shadow = tone.map(|channel| channel.saturating_sub(weight));
    let lifted = tone.map(|channel| channel.saturating_add(weight));
    let pixel = |x, y| plane.pixels().get_pixel(x, y).0;

    for x in 0..width {
        assert_eq!(
            &pixel(x, 0)[..3],
            &shadow,
            "column {x} of the bezel's top edge is {:?} where the groove falls {weight} channels \
             into shadow",
            &pixel(x, 0)[..3]
        );
        if x > 0 {
            assert_eq!(
                &pixel(x, height - 1)[..3],
                &lifted,
                "column {x} of its bottom edge is {:?} where the groove lifts {weight} channels \
                 out of the region",
                &pixel(x, height - 1)[..3]
            );
        }
    }
    for y in 0..height {
        assert_eq!(
            &pixel(0, y)[..3],
            &shadow,
            "row {y} of the bezel's left edge is {:?}, and a groove is darkest where it opens",
            &pixel(0, y)[..3]
        );
        if y > 0 {
            assert_eq!(
                &pixel(width - 1, y)[..3],
                &lifted,
                "row {y} of its right edge is {:?}, and a groove is lifted where it closes",
                &pixel(width - 1, y)[..3]
            );
        }
    }
    for x in 1..width - 1 {
        for y in 1..height - 1 {
            assert_eq!(
                pixel(x, y),
                [0, 0, 0, 0],
                "pixel {x}, {y} inside the bezel is drawn over the region's own pixels, and the \
                 bezel is a groove around it"
            );
        }
    }
}

#[test]
fn an_instrument_claims_no_cell_of_the_band_it_is_drawn_under() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(20, 4, 24, 1);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 60, 20));
    let ground = Color::Rgb(0x0a, 0x0b, 0x0c);
    pen::claim(&mut buffer, Rect::new(20, 4, 24, 6), palette.over(ground));
    let claimed = buffer.clone();
    let instruments = [
        ("a gauge", gauge_ink(area, 40, false, &palette)),
        ("a bar", bar_ink(area, Bar::Input, 40, &palette)),
        ("a bezel", bezelled(Rect::new(20, 6, 24, 3), &palette)),
    ];
    for (what, placement) in instruments {
        placement.claim(&mut buffer, &palette);
        assert_eq!(
            buffer, claimed,
            "{what} claims a cell of the band it is drawn under, and the rail's own shade has \
             claimed those cells: an instrument is the pixels it adds and no cell"
        );
    }
}

#[test]
fn an_instrument_in_a_palette_that_spells_no_colour_is_no_shade_at_all() {
    let area = Rect::new(0, 0, 8, 1);
    for (what, placement) in instruments(area, Some(CELL), &Palette::TRUECOLOR) {
        assert!(
            placement.is_some(),
            "{what} on a terminal that places pictures and in a palette that spells colour is an \
             instrument the frame is given"
        );
    }
    for (what, placement) in instruments(area, Some(CELL), &Palette::SLOTS) {
        assert!(
            placement.is_none(),
            "{what} is drawn in inks a palette that spells no colour gives no channel to, so it \
             is no instrument and the frame draws its own meters"
        );
    }
    for (what, placement) in instruments(area, None, &Palette::TRUECOLOR) {
        assert!(
            placement.is_none(),
            "{what} on a terminal that places no picture has no pixel to stand on, so it is no \
             shade at all"
        );
    }
    for (what, placement) in instruments(Rect::new(0, 0, 0, 1), Some(CELL), &Palette::TRUECOLOR) {
        assert!(
            placement.is_none(),
            "{what} over no cell of the frame is no shade at all"
        );
    }
}

#[test]
fn a_gauge_that_moved_one_hundredth_carries_the_same_image_and_a_placement_on_its_rung() {
    let area = Rect::new(0, 0, 8, 1);
    let palette = Palette::TRUECOLOR;
    let first = gauge_ink(area, 40, false, &palette);
    let moved = gauge_ink(area, 41, false, &palette);
    assert_eq!(
        first.shade(),
        moved.shade(),
        "a gauge that moved a hundredth carries the image the terminal already holds, so no \
         pixel of it travels"
    );
    assert_eq!(
        moved.shows().expect("a placement").y,
        41,
        "and the placement it is put under picks the rung it moved to"
    );
    let mut planes = Planes::default();
    for placement in [first, moved] {
        planes.hold(&[placement]);
    }
    assert_eq!(
        planes.made(),
        1,
        "a gauge that moved a hundredth makes no image, and the frame holds the one already made"
    );
    assert!(
        planes.image(moved.shade()).is_some(),
        "and the image it holds answers for the gauge that moved"
    );
}

#[test]
fn every_picture_the_frame_holds_is_held_under_an_identity_of_its_own() {
    for (at, id) in PlaneId::ALL.iter().enumerate() {
        for other in &PlaneId::ALL[at + 1..] {
            assert_ne!(
                id.image(),
                other.image(),
                "{id:?} and {other:?} are one picture to the terminal, and the picture one of \
                 them stands in is replaced by the other"
            );
        }
    }
    assert!(
        PlaneId::ALL.contains(&PlaneId::Face),
        "the face is a picture the frame holds, so the identities given back name it with the \
         others"
    );
}

#[test]
fn giving_every_identity_back_names_each_picture_the_frame_can_hold_exactly_once() {
    let giving = String::from_utf8(give_back()).expect("the sequences are printable");

    for id in PlaneId::ALL {
        let spelled = String::from_utf8(delete(id)).expect("a sequence is printable");
        assert_eq!(
            giving.matches(&spelled).count(),
            1,
            "{id:?} is given back exactly once, so a picture it stands in is neither left in a \
             reader's scrollback nor given back twice: {giving:?}"
        );
    }
    assert_eq!(
        giving.matches("_Ga=d").count(),
        PlaneId::ALL.len(),
        "and the identities given back are the ones the frame can hold, and no other: {giving:?}"
    );
}

/// The bytes as the sequence a terminal reads them as: every byte of the format is ASCII.
fn written(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("a sequence is printable")
}

#[test]
fn a_pass_whose_every_picture_stood_where_it_stood_answers_nothing_at_all() {
    let palette = Palette::TRUECOLOR;
    let band =
        Placement::composer(Rect::new(2, 3, 40, 4), Some(CELL), &palette).expect("a band to shade");
    let planes = held(&[band]);
    let mut record = Held::default();

    assert!(
        !record.pass(&[band], &planes).is_empty(),
        "the first pass answers the bytes that put the one picture the terminal holds nowhere"
    );

    let sent = record.pass(&[band], &planes);

    assert!(
        sent.is_empty(),
        "a pass whose every picture stood where it stood answers no byte at all, and answered {}: \
         {:?}",
        sent.len(),
        written(&sent)
    );
}

#[test]
fn a_pass_whose_caret_moved_answers_a_placement_and_no_upload() {
    let band = Rect::new(2, 3, 40, 4);
    let palette = Palette::TRUECOLOR;
    let frame = |caret| {
        [
            Placement::composer(band, Some(CELL), &palette).expect("a band to shade"),
            caret_light(band, caret, &palette),
        ]
    };
    let mut record = Held::default();
    let stood = frame(6);
    record.pass(&stood, &held(&stood));

    let moved = frame(7);
    let planes = held(&moved);
    let sent = record.pass(&moved, &planes);

    assert_eq!(
        sent,
        place(PlaneId::Caret, moved[1], shown(moved[1])),
        "the light is put on the cells the caret stands on now, under the identity it was held by"
    );
    let spelled = written(&sent);

    assert!(
        spelled.contains(&format!("i={}", PlaneId::Caret.image()))
            && spelled.contains(&format!("p={PLACEMENT}"))
            && spelled.contains(&format!("z={DEPTH}")),
        "and it is put under the identity and the depth the light is held by: {spelled:?}"
    );
    assert!(
        !spelled.contains("\x1b_Ga=T"),
        "no pixel of the light moved, so no pixel travels: {spelled:?}"
    );
}

#[test]
fn a_pass_whose_band_is_gone_answers_one_delete_and_the_identity_stands_again_later() {
    let band = Rect::new(2, 3, 40, 4);
    let palette = Palette::TRUECOLOR;
    let composer = Placement::composer(band, Some(CELL), &palette).expect("a band to shade");
    let light = caret_light(band, 6, &palette);
    let both = [composer, light];
    let mut record = Held::default();
    let planes = held(&both);
    record.pass(&both, &planes);

    let alone = held(&[composer]);
    let sent = record.pass(&[composer], &alone);

    assert_eq!(
        sent,
        delete(PlaneId::Caret),
        "the light the band no longer stands is given back, and nothing else is answered"
    );
    assert_eq!(
        written(&sent)
            .matches(&format!("_Ga=d,d=i,i={}", PlaneId::Caret.image()))
            .count(),
        1,
        "and it is given back once: {:?}",
        written(&sent)
    );

    let sent = record.pass(&both, &planes);

    assert!(
        written(&sent).contains(&format!("_Ga=T,f=100,i={}", PlaneId::Caret.image())),
        "the terminal holds no light, so its pixels travel again: {:?}",
        written(&sent)
    );
    assert!(
        !written(&sent).contains("_Ga=d"),
        "and nothing of it is given back a second time: {:?}",
        written(&sent)
    );
}

#[test]
fn a_pass_that_added_moved_and_removed_a_picture_answers_the_delete_first_then_the_frame_order() {
    let band = Rect::new(2, 3, 40, 4);
    let palette = Palette::TRUECOLOR;
    let composer = Placement::composer(band, Some(CELL), &palette).expect("a band to shade");
    let gauge = gauge_ink(Rect::new(0, 20, 8, 1), 40, false, &palette);
    let before = [composer, caret_light(band, 6, &palette)];
    let mut record = Held::default();
    let planes = held(&before);
    record.pass(&before, &planes);

    let after = [caret_light(band, 7, &palette), gauge];
    let planes = held(&after);
    let sent = record.pass(&after, &planes);

    let mut wanted = delete(PlaneId::Composer);
    wanted.extend(place(PlaneId::Caret, after[0], shown(after[0])));
    wanted.extend(upload(
        planes.image(after[1].shade()).expect("the gauge's plane"),
        after[1],
        shown(after[1]),
    ));

    assert_eq!(
        sent, wanted,
        "the band the frame left is deleted before anything is put on the screen, and the \
         pictures it kept are answered in the frame's own order"
    );
}

#[test]
fn forget_names_each_held_identity_once_and_the_face_is_not_among_them() {
    let band = Rect::new(2, 3, 40, 4);
    let palette = Palette::TRUECOLOR;
    let placements = [
        Placement::composer(band, Some(CELL), &palette).expect("a band to shade"),
        caret_light(band, 6, &palette),
        gauge_ink(Rect::new(0, 20, 8, 1), 40, false, &palette),
    ];
    let planes = held(&placements);
    let mut record = Held::default();
    record.pass(&placements, &planes);

    let sent = record.forget();

    let wanted: Vec<u8> = [PlaneId::Composer, PlaneId::Caret, PlaneId::Gauge]
        .into_iter()
        .flat_map(delete)
        .collect();
    assert_eq!(
        sent, wanted,
        "every picture the terminal held is given back once, in the order it was held"
    );
    assert!(
        !written(&sent).contains(&written(&delete(PlaneId::Face))),
        "the face stands under no shade, so no plane of the frame answers for it: {:?}",
        written(&sent)
    );
    assert!(
        record.forget().is_empty(),
        "and nothing is held once the pictures are given back"
    );
}

fn composer_placement(area: Rect) -> Placement {
    Placement::composer(area, Some(CELL), &Palette::TRUECOLOR).expect("a band to shade")
}

fn rail_placement() -> Placement {
    Placement::rail(
        Rail::Ledger,
        RailBand::of(Rect::new(6, 3, 3, 4)),
        Some(CELL),
        &Palette::TRUECOLOR,
    )
    .expect("a band to shade")
}

fn transcript_placement() -> Placement {
    Placement::transcript(Rect::new(0, 0, 6, 4), Some(CELL), &Palette::TRUECOLOR)
        .expect("a band to shade")
}

#[test]
fn a_plane_is_sent_once_and_the_plane_that_leaves_is_deleted_by_its_own_id() {
    let placements = [
        rail_placement(),
        transcript_placement(),
        composer_placement(Rect::new(2, 3, 2, 1)),
    ];
    let planes = held(&placements);
    let mut record = Held::default();
    let mut wire = record.pass(&placements, &planes);
    let placed = wire.len();
    wire.extend(record.pass(&placements, &planes));
    assert_eq!(
        placed,
        wire.len(),
        "a pass that changes no band answers no byte at all"
    );
    let rest = &placements[1..];
    wire.extend(record.pass(rest, &held(rest)));
    let wire = written(&wire);
    assert_eq!(
        wire.matches("_Ga=T").count(),
        3,
        "three planes, and each placed once: {wire:?}"
    );
    for id in [333, 334, 335] {
        assert!(
            wire.contains(&format!("_Ga=T,f=100,i={id},p=1,q=2,z={DEPTH}")),
            "each band's image is its own, and each is uploaded under its own identity and the \
             frame's one placement: {wire:?}"
        );
    }
    let deleted = wire.matches("_Ga=d").count();
    assert_eq!(
        deleted, 1,
        "the plane that left is deleted and the two that stand are not: {wire:?}"
    );
    assert!(
        wire.contains("_Ga=d,d=i,i=334"),
        "and it is deleted by the identity it was placed under, leaving the transcript band's \
         and the composer's standing: {wire:?}"
    );
}

#[test]
fn a_resized_plane_takes_the_old_one_away_and_places_the_new_one_after_it() {
    let mut record = Held::default();
    let mut wire = Vec::new();
    for placement in [
        composer_placement(Rect::new(2, 3, 2, 1)),
        composer_placement(Rect::new(1, 5, 3, 2)),
    ] {
        wire.extend(record.pass(&[placement], &held(&[placement])));
    }
    let wire = written(&wire);
    assert_eq!(wire.matches("_Ga=T").count(), 2);
    assert_eq!(wire.matches("_Ga=d").count(), 1);
    let taken = wire.find("_Ga=d").unwrap();
    let placed = wire.match_indices("_Ga=T").nth(1).unwrap().0;
    assert!(
        taken < placed,
        "the image a resize made stale is taken away before the one that stands in its place is \
         written: {wire:?}"
    );
    assert!(wire.contains("\x1b[6;2H"));
}

#[test]
fn every_plane_is_placed_under_the_depth_a_cells_own_background_cannot_cover() {
    let band = Rect::new(2, 3, 30, 4);
    let placements = [
        rail_placement(),
        transcript_placement(),
        composer_placement(band),
        caret_light(band, 8, &Palette::TRUECOLOR),
    ];
    let mut record = Held::default();
    let wire = written(&record.pass(&placements, &held(&placements)));

    let depth: i64 = wire
        .split_once("z=")
        .and_then(|(_, rest)| rest.split(',').next())
        .and_then(|value| value.parse().ok())
        .expect("a depth on the ground the frame places");
    assert!(
        depth < -1_073_741_824,
        "a plane is placed below the depth at which a terminal draws an image under a cell that \
         carries a background, and the seam's edge ink is one such cell: {depth}"
    );
    for id in [333, 334, 335, 336] {
        assert!(
            wire.contains(&format!("i={id},p=1,q=2,z={DEPTH},C=1")),
            "the plane under {id} is placed at a depth a cell's background covers: {wire:?}"
        );
    }
    assert!(
        !wire.contains("z=-1,"),
        "no plane stands at the depth the seam's ink was painted over at: {wire:?}"
    );
}

#[test]
fn no_plane_the_frame_places_expects_a_reply_from_the_terminal() {
    let band = Rect::new(2, 3, 30, 4);
    let mut record = Held::default();
    let mut wire = Vec::new();
    for caret in [8, 9] {
        let placements = [
            composer_placement(band),
            caret_light(band, caret, &Palette::TRUECOLOR),
        ];
        wire.extend(record.pass(&placements, &held(&placements)));
    }
    let wire = written(&wire);

    for command in wire.split("\u{1b}_G").skip(1) {
        let head = command.split(';').next().unwrap_or_default();
        if head.starts_with("a=d") {
            assert!(
                head.contains("q=2"),
                "an image given back asks the terminal a question, and an answer to it is read \
                 as the reader's next keystroke: {head:?}"
            );
            continue;
        }
        if !head.starts_with("a=T") && !head.starts_with("a=p") {
            continue;
        }
        assert!(
            head.contains("q=2") && head.contains("C=1"),
            "a placement asks the terminal a question, and an answer to it is read as the \
             reader's next keystroke: {head:?}"
        );
    }
    assert!(
        wire.contains("_Ga=p"),
        "and a placement is what the frame makes of a caret that moved: {wire:?}"
    );
}

#[test]
fn the_carets_light_is_given_back_when_the_band_under_it_loses_its_caret() {
    let band = Rect::new(2, 3, 30, 4);
    let mut record = Held::default();
    let lit = [
        composer_placement(band),
        caret_light(band, 8, &Palette::TRUECOLOR),
    ];
    let mut wire = record.pass(&lit, &held(&lit));
    let bare = [composer_placement(band)];
    wire.extend(record.pass(&bare, &held(&bare)));
    let wire = written(&wire);

    assert!(
        wire.contains(&format!("_Ga=d,d=i,i={}", PlaneId::Caret.image())),
        "the light is given back when the band under it carries no caret, or the terminal is left \
         holding a light that stands nowhere: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=T").count(),
        2,
        "and neither the band nor the light is made again for it: {wire:?}"
    );
}

#[test]
fn a_frame_whose_caret_moved_transmits_no_image_and_places_the_light_again() {
    let band = Rect::new(2, 3, 30, 4);
    let mut record = Held::default();
    let mut wire = Vec::new();
    for caret in [6, 7, 8] {
        let placements = [
            composer_placement(band),
            caret_light(band, caret, &Palette::TRUECOLOR),
        ];
        wire.extend(record.pass(&placements, &held(&placements)));
    }
    let wire = written(&wire);
    assert_eq!(
        wire.matches("_Ga=T").count(),
        2,
        "the band and the light are transmitted once each, and a caret that moved twelve pixels \
         sends no pixel of either: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=p").count(),
        2,
        "and every caret after the first puts the image the terminal holds on the cells it stands \
         on now: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=d").count(),
        0,
        "nothing the frame still holds is taken away for a light that moved: {wire:?}"
    );
    assert!(
        wire.contains(&format!(
            "_Ga=p,i={},p={PLACEMENT},q=2,z={DEPTH},C=1",
            PlaneId::Caret.image()
        )),
        "the light is placed again under the identity and the placement it was held by, and the \
         command is quiet: {wire:?}"
    );
}

#[test]
fn a_band_on_another_cell_or_in_another_palette_is_placed_again_over_the_same_area() {
    let area = Rect::new(2, 3, 2, 1);
    let first = composer_placement(area);
    let finer = Placement::composer(area, Some(CellSize::new(20, 40)), &Palette::TRUECOLOR)
        .expect("a band to shade");
    let greys = Placement::composer(area, Some(CellSize::NOMINAL), &Palette::ANSI)
        .expect("a band to shade");
    assert_eq!(
        (finer.cells(), greys.cells()),
        (first.cells(), first.cells()),
        "the cells the band stands on stand where they did, and only the cell and the palette \
         moved"
    );
    let mut record = Held::default();
    let mut wire = Vec::new();
    for placement in [first, finer, greys] {
        wire.extend(record.pass(&[placement], &held(&[placement])));
    }
    let wire = written(&wire);
    assert_eq!(
        wire.matches("_Ga=T").count(),
        3,
        "a band drawn on another cell or in another palette is placed again: {wire:?}"
    );
    assert!(
        wire.contains(&format!(
            "_Ga=T,f=100,i=333,p=1,q=2,z={DEPTH},C=1,c=2,r=1,x=0,y=0,w=20,h=20"
        )),
        "at the area the band stands on, which never moved: {wire:?}"
    );
}

#[test]
fn a_rail_whose_light_moved_is_placed_again_by_its_own_identity() {
    let area = Rect::new(6, 3, 3, 6);
    let rail = |face: u16| faced(Rail::Ledger, area, face, Some(CELL), &Palette::TRUECOLOR);
    let mut record = Held::default();
    let mut wire = Vec::new();
    for placement in [rail(2), rail(3)] {
        wire.extend(record.pass(&[placement], &held(&[placement])));
    }
    let wire = written(&wire);
    assert_eq!(
        wire.matches("_Ga=T").count(),
        2,
        "the light falls on the rows the face stands on, so a rail whose face moved is placed \
         once more: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=d").count(),
        1,
        "and the image it stood on is taken away and not left under it: {wire:?}"
    );
    assert!(
        wire.contains("_Ga=d,d=i,i=334")
            && wire.contains(&format!("_Ga=T,f=100,i=334,p=1,q=2,z={DEPTH}")),
        "both the delete and the new placement are under the rail's own identity: {wire:?}"
    );
}

#[test]
fn a_percentage_that_moved_places_the_gauge_again_and_sends_no_pixel_of_it() {
    let mut record = Held::default();
    let mut wire = Vec::new();
    for share in [40, 41, 42] {
        let placements = [gauge_ink(
            Rect::new(40, 12, 8, 1),
            share,
            false,
            &Palette::TRUECOLOR,
        )];
        wire.extend(record.pass(&placements, &held(&placements)));
    }
    let wire = written(&wire);

    assert_eq!(
        wire.matches("_Ga=T").count(),
        1,
        "a gauge is transmitted once, and a share that moved a hundredth sends no pixel of it: \
         {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=p").count(),
        2,
        "and every share after the first places the image the terminal holds again: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=d").count(),
        0,
        "nothing the frame still holds is given back for a share that moved: {wire:?}"
    );
    assert!(
        wire.contains(&format!(
            "_Ga=p,i={},p={PLACEMENT},q=2,z={DEPTH},C=1,c=8,r=1,x=0,y=41,w=80,h=1",
            PlaneId::Gauge.image()
        )),
        "the gauge is placed again on the rung it moved to, one pixel of the ladder deep: {wire:?}"
    );
}

#[test]
fn a_gauge_the_rail_no_longer_states_is_given_back_by_its_own_image_id() {
    let mut record = Held::default();
    let stated = [gauge_ink(
        Rect::new(40, 12, 8, 1),
        40,
        false,
        &Palette::TRUECOLOR,
    )];
    let mut wire = record.pass(&stated, &held(&stated));
    wire.extend(record.pass(&[], &Planes::default()));
    let wire = written(&wire);

    assert!(
        wire.contains(&format!("_Ga=d,d=i,i={}", PlaneId::Gauge.image())),
        "a gauge the rail no longer states is given back by its own image id, or the terminal is \
         left holding an instrument that stands nowhere: {wire:?}"
    );
    assert_eq!(
        wire.matches("_Ga=T").count(),
        1,
        "and the gauge is made once and no image of another is made for it: {wire:?}"
    );
}

/// The face the frame asks for: the cells it stands on, its picture in pixels, and the frames past
/// it.
fn a_face() -> (Rect, image::RgbaImage, Vec<image::RgbaImage>) {
    let picture = crate::face::measure(
        crate::graphics::Ceiling::new(40, 12),
        &crate::picture::Surface::of(None, CELL, true, false),
    )
    .expect("the face travels in the binary");
    let at = Rect::new(4, 2, picture.cols(), picture.rows());
    let root = picture
        .rasterise(CELL)
        .expect("the face travels in the binary, so it decodes");
    let frames = crate::face::frames(&root);
    (at, root, frames)
}

/// The face's bytes as the terminal is handed them: the picture, the frames the motion keeps, and
/// the loop over them.
fn handed(motion: Motion) -> String {
    let (at, root, frames) = a_face();
    let wanted = match motion {
        Motion::Still => 1,
        Motion::Turning(_) => frames.len(),
    };
    let (mut sent, kept) = face_sent(at, &root, &frames[..wanted]);
    sent.extend(face_turned(motion, kept));
    written(&sent)
}

fn heads(drawn: &str, opening: &str) -> Vec<String> {
    drawn
        .match_indices("\x1b_G")
        .filter_map(|(at, _)| {
            let rest = &drawn[at + "\x1b_G".len()..];
            let end = rest
                .find(';')
                .into_iter()
                .chain(rest.find("\x1b\\"))
                .min()?;
            Some(rest[..end].to_owned())
        })
        .filter(|head| head.starts_with(opening))
        .map(|head| match head.rsplit_once(",m=") {
            Some((head, _)) => head.to_owned(),
            None => head,
        })
        .collect()
}

/// The rectangle an `a=f` head names, read as left, top, width and height.
fn rectangle(head: &str) -> (u32, u32, u32, u32) {
    let named = |key: &str| -> u32 {
        let (_, tail) = head
            .split_once(&format!("{key}="))
            .unwrap_or_else(|| panic!("{head:?} names no {key}"));
        tail.split(',')
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| panic!("{head:?} names a {key} that is no number"))
    };
    (named("x"), named("y"), named("s"), named("v"))
}

fn differs(root: &image::RgbaImage, frame: &image::RgbaImage, x: u32, y: u32) -> bool {
    root.get_pixel(x, y) != frame.get_pixel(x, y)
}

#[test]
fn every_frame_past_the_root_is_composed_against_it_and_carries_only_the_pixels_that_differ() {
    let (at, root, frames) = a_face();
    let (sent, kept) = face_sent(at, &root, &frames);
    let drawn = written(&sent);
    let (width, height) = root.dimensions();
    let travelling = heads(&drawn, "a=f,");

    assert_eq!(
        kept,
        frames.len(),
        "the terminal is left keeping the root and every frame past it: {drawn:?}"
    );
    assert_eq!(
        drawn
            .matches(&format!("_Ga=T,f=100,i={}", PlaneId::Face.image()))
            .count(),
        1,
        "the root frame is one image under the face's own identity: {drawn:?}"
    );
    assert_eq!(
        travelling.len(),
        frames.len() - 1,
        "one frame's data travels for each frame past the root"
    );
    for (number, head) in travelling.iter().enumerate() {
        let frame = &frames[number + 1];
        let (left, top, across, down) = rectangle(head);
        let number = number + 2;
        let inside =
            |x: u32, y: u32| (left..left + across).contains(&x) && (top..top + down).contains(&y);

        assert!(
            head.contains("c=1") && head.contains("X=1"),
            "frame {number} is composed against the root frame and replaces the rectangle it \
             names: {head:?}"
        );
        assert!(
            left + across <= width && top + down <= height,
            "frame {number} stands inside the root's {width} by {height} pixels: {head:?}"
        );
        for y in 0..height {
            for x in 0..width {
                assert!(
                    !differs(&root, frame, x, y) || inside(x, y),
                    "frame {number} travels every pixel it differs from the root in, and ({x}, \
                     {y}) differs outside the rectangle {head:?}"
                );
            }
        }
        let (right, bottom) = (left + across - 1, top + down - 1);
        let touched = (0..across).any(|step| differs(&root, frame, left + step, top))
            && (0..across).any(|step| differs(&root, frame, left + step, bottom))
            && (0..down).any(|step| differs(&root, frame, left, top + step))
            && (0..down).any(|step| differs(&root, frame, right, top + step));
        assert!(
            touched,
            "and the rectangle is the tightest one there is: every side of it holds a pixel that \
             differs from the root, so {head:?} carries nothing the root does not"
        );
    }
}

#[test]
fn a_run_in_flight_quickens_the_loop_and_a_settled_run_stops_it() {
    let identity = PlaneId::Face.image();
    let drawn = handed(Motion::Turning(Some(Turning::FLIGHT)));
    let gaps = heads(&drawn, &format!("a=a,i={identity},r=")).len();
    assert!(gaps > 0, "a live run's face is left turning: {drawn:?}");
    assert_eq!(
        drawn
            .matches(&format!("z={}", Turning::FLIGHT.gap_ms()))
            .count(),
        gaps,
        "every frame of the loop is told the quickened gap: {drawn:?}"
    );
    assert_eq!(
        heads(&drawn, &format!("a=a,i={identity},s=")),
        [format!("a=a,i={identity},s=3,v=1,q=2")],
        "and the terminal runs it for as long as the run does, and no number of cycles"
    );

    let settled = handed(Motion::Turning(None));
    let stop = format!("a=a,i={identity},s=1,c=1");
    assert_eq!(
        settled.matches(&stop).count(),
        1,
        "a run that settled stops the loop and holds the root frame: {settled:?}"
    );
    let at = settled.find(&stop).expect("the stop");
    assert!(
        !settled[at..].contains("z="),
        "and the frame it holds is told no gap at all: {settled:?}"
    );
    assert_eq!(
        settled.matches("_Ga=T").count(),
        1,
        "with nothing sent again: the frames are the terminal's"
    );
}

#[test]
fn an_error_flashes_twice_and_is_left_standing() {
    let identity = PlaneId::Face.image();
    let drawn = handed(Motion::Turning(Some(Turning::FLASH)));

    assert_eq!(
        heads(&drawn, &format!("a=a,i={identity},s=")),
        [format!("a=a,i={identity},s=3,v=2,q=2")],
        "a failed run's face turns two cycles of the breath and then holds: {drawn:?}"
    );
    assert!(
        drawn.contains(&format!("z={}", Turning::FLASH.gap_ms())),
        "at the gap the flash is drawn in: {drawn:?}"
    );
}

#[test]
fn a_terminal_that_keeps_no_frames_is_handed_the_still_face_and_no_animation_sequence() {
    let (at, root, frames) = a_face();
    let (sent, kept) = face_sent(at, &root, &frames[..1]);
    assert_eq!(kept, 1, "a terminal handed the root alone keeps one frame");
    let drawn = written(&[sent, face_turned(Motion::Still, kept)].concat());

    assert!(
        drawn.contains(&format!("_Ga=T,f=100,i={}", PlaneId::Face.image())),
        "the picture the binary carries is the face: {drawn:?}"
    );
    assert!(
        !drawn.contains("_Ga=f,"),
        "no frame is sent past it: {drawn:?}"
    );
    assert!(
        !drawn.contains("_Ga=a"),
        "and no animation sequence of any kind: {drawn:?}"
    );
}

#[test]
fn a_face_placed_again_stands_in_the_cells_it_moved_to_and_travels_no_pixel() {
    let at = Rect::new(2, 6, 12, 6);
    let drawn = written(&face_placed(at));

    assert!(
        drawn.starts_with(&format!("\x1b7\x1b[{};{}H", at.y + 1, at.x + 1)),
        "the placement stands in the cells the face moved to: {drawn:?}"
    );
    assert_eq!(
        heads(&drawn, "a=p,"),
        [format!(
            "a=p,i={},p={PLACEMENT},q=2,z={DEPTH},C=1,c={},r={}",
            PlaneId::Face.image(),
            at.width,
            at.height
        )],
        "the image the terminal holds is put under the identity it is held by: {drawn:?}"
    );
    assert!(
        !drawn.contains("_Ga=T") && !drawn.contains("_Ga=f,"),
        "no pixel of it travels again: {drawn:?}"
    );
    assert!(
        drawn.ends_with("\x1b8"),
        "and the cursor is put back: {drawn:?}"
    );
}

#[test]
fn the_face_is_given_back_under_the_identity_it_is_held_by() {
    let given = written(&face_back());

    assert_eq!(
        given, "\u{1b}_Ga=d,d=i,i=337,q=2\u{1b}\\",
        "the face is given back under the identity it is held by, and no other picture with it"
    );
}

#[test]
fn the_frames_and_animation_are_asked_for_under_the_face_s_own_identity() {
    let asked = written(&frames_asked());
    let identity = format!("i={}", PlaneId::Face.image());

    assert!(
        asked.contains("e=f") && asked.contains("e=a"),
        "the two features asked for are the frames a picture is kept in and the animation of \
         them: {asked:?}"
    );
    assert!(
        asked.contains("e=f,c=1,X=1"),
        "and the frames are asked for with the canvas and the overwrite mode the face's frames \
         are sent in: {asked:?}"
    );
    assert_eq!(
        asked.matches(&identity).count(),
        2,
        "and both are asked under the face's own identity, {identity}: {asked:?}"
    );
}

#[test]
fn a_band_that_only_moved_makes_no_image_and_is_placed_again_where_it_stands() {
    let palette = Palette::TRUECOLOR;
    let band = |area| Placement::transcript(area, Some(CELL), &palette).expect("a band to shade");
    let (here, there) = (band(Rect::new(0, 0, 40, 4)), band(Rect::new(10, 8, 40, 4)));
    assert_eq!(
        here.shade(),
        there.shade(),
        "the two bands are drawn from the same facts"
    );
    assert_ne!(
        here.cells(),
        there.cells(),
        "and the band stands on other cells"
    );

    let mut planes = Planes::default();
    planes.hold(&[here, there]);
    assert_eq!(
        (planes.held().len(), planes.made()),
        (1, 1),
        "a band that only moved is the one shade, and one image is made of it"
    );

    let mut record = Held::default();
    let first = written(&record.pass(&[here], &planes));
    assert!(
        first.contains(&format!("_Ga=T,f=100,i={}", PlaneId::Transcript.image())),
        "the terminal holds no picture of the band yet, so it is transmitted: {first:?}"
    );

    let again = written(&record.pass(&[there], &planes));
    assert!(
        again.contains(&format!(
            "_Ga=p,i={},p={PLACEMENT},q=2,z={DEPTH},C=1",
            PlaneId::Transcript.image()
        )),
        "a band that only moved is placed again on the cells it stands on now: {again:?}"
    );
    assert!(
        !again.contains("_Ga=T"),
        "and no pixel of a shade that differs in its cells and in no drawing fact is sent \
         again: {again:?}"
    );
    assert!(
        !again.contains("_Ga=d"),
        "and the image the band stood on is not taken back: {again:?}"
    );
}

#[test]
fn a_light_that_moved_over_its_band_makes_no_image_and_sends_no_byte_again() {
    let band = Rect::new(4, 20, 40, 4);
    let light = |caret| {
        Placement::caret(band, Some(CELL), caret, &Palette::TRUECOLOR)
            .expect("the light the caret casts")
    };
    let (stood, moved) = (light(12), light(14));
    assert_eq!(
        stood.shade(),
        moved.shade(),
        "the light is drawn from the same facts wherever the caret stands"
    );
    assert_ne!(
        stood.cells(),
        moved.cells(),
        "and the cells it stands on moved with the caret"
    );

    let mut planes = Planes::default();
    planes.hold(&[stood, moved]);
    assert_eq!(
        planes.held().len(),
        1,
        "the frame holds the one light both placements are drawn from"
    );

    let mut record = Held::default();
    let first = written(&record.pass(&[stood], &planes));
    assert!(
        first.contains(&format!("_Ga=T,f=100,i={}", PlaneId::Caret.image())),
        "the light the terminal holds no picture of is transmitted: {first:?}"
    );

    let sent = record.pass(&[moved], &planes);
    assert_eq!(
        sent,
        place(PlaneId::Caret, moved, shown(moved)),
        "the light is put on the cells the caret stands on now, under the identity it was held by"
    );
    assert!(
        !written(&sent).contains("_Ga=T"),
        "no pixel of it moved, so no pixel travels: {:?}",
        written(&sent)
    );
}

#[test]
fn two_lights_of_one_shade_are_answered_by_the_one_plane_the_frame_holds() {
    let band = Rect::new(4, 20, 40, 4);
    let light = |caret| {
        Placement::caret(band, Some(CELL), caret, &Palette::TRUECOLOR)
            .expect("the light the caret casts")
    };
    let (left, right) = (light(8), light(28));
    assert_ne!(left.cells(), right.cells(), "the two lights stand apart");

    let mut planes = Planes::default();
    planes.hold(&[left]);
    assert!(
        std::ptr::eq(
            planes
                .image(left.shade())
                .expect("the plane under the light"),
            planes
                .image(right.shade())
                .expect("a plane under the light's own facts")
        ),
        "a shade standing in another column is answered by the plane the frame already holds"
    );

    planes.hold(&[left, right]);
    assert_eq!(
        planes.held().len(),
        1,
        "and a frame holding both placements holds the one plane: {:?}",
        planes
            .held()
            .iter()
            .map(Plane::id)
            .collect::<Vec<PlaneId>>()
    );
}

#[test]
fn the_composer_separator_spans_half_a_cell_while_the_rail_keeps_its_width() {
    for (_, palette) in SPELLED {
        for (axis, placement) in parted_bands(&palette, CELL) {
            let plane = drawn(placement);
            let extent = match axis {
                Axis::Down => CELL.width(),
                Axis::Across => CELL.height() / 2,
            };
            let (x, y) = along_seam(axis, extent - 1);
            assert_eq!(lit_over(&placement, &plane, CELL, x, y), weight_of(&palette));
            let (x, y) = along_seam(axis, extent);
            assert_eq!(lit_over(&placement, &plane, CELL, x, y), 0);
        }
    }
}
