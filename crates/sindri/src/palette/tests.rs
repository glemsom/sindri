use super::*;

/// The two spellings that name colours of their own; `SLOTS` wears the reader's theme instead.
const SPELLED: [(&str, Palette); 2] = [("ansi", Palette::ANSI), ("truecolor", Palette::TRUECOLOR)];

#[test]
fn a_terminal_told_no_colour_is_given_the_six_slots_and_no_background() {
    let uncoloured = Palette::probed(Some("1"), Some("xterm-256color"), Some("truecolor"));

    assert_eq!(uncoloured, Palette::SLOTS);
    assert_eq!(
        Palette::SLOTS.ink(Ink::Accent).0,
        Color::Cyan,
        "an uncoloured terminal keeps the slot it always had, and not the design's accent"
    );
    for band in Band::ALL {
        assert_eq!(
            Palette::SLOTS.band(band),
            Color::Reset,
            "{band:?} draws no background, because a reader who asked for no colour asked \
                 for their own"
        );
    }
}

#[test]
fn a_dumb_terminal_is_given_the_six_slots_and_no_background() {
    assert_eq!(
        Palette::probed(None, Some("dumb"), None),
        Palette::SLOTS,
        "a terminal that says it is dumb can hold no colour at all"
    );
}

#[test]
fn a_terminal_naming_truecolor_is_given_the_designed_palette() {
    assert_eq!(
        Palette::probed(None, Some("xterm-256color"), Some("truecolor")),
        Palette::TRUECOLOR
    );
    assert_eq!(
        Palette::probed(None, Some("xterm-direct"), None),
        Palette::TRUECOLOR,
        "a terminal that names it in `TERM` says it as plainly as one that names it in \
             `COLORTERM`"
    );
}

#[test]
fn a_terminal_naming_no_truecolor_is_given_the_same_design_in_ansi() {
    let ansi = Palette::probed(None, Some("screen-256color"), None);
    assert_eq!(ansi, Palette::ANSI);

    for ink in Ink::ALL {
        let (ansi_foreground, ansi_modifier) = ansi.ink(ink);
        let (rgb_foreground, rgb_modifier) = Palette::TRUECOLOR.ink(ink);
        assert_eq!(
            ansi_modifier,
            rgb_modifier,
            "{} spends one modifier",
            ink.name()
        );
        match ink {
            Ink::Mark => {
                assert_eq!(ansi_foreground, Color::Reset);
                assert_eq!(rgb_foreground, Color::Reset);
            }
            ink => {
                assert!(
                    matches!(ansi_foreground, Color::Indexed(_)),
                    "{} is spelled in the colours a 256-colour terminal has",
                    ink.name()
                );
                assert!(
                    matches!(rgb_foreground, Color::Rgb(..)),
                    "{} is the colour the design was drawn in",
                    ink.name()
                );
            }
        }
    }
    for band in Band::ALL {
        assert!(
            matches!(ansi.band(band), Color::Indexed(_)),
            "{band:?} keeps the design's three steps and spells them in ANSI"
        );
    }
    assert!(
        matches!(ansi.edge_ink(), Color::Indexed(_)),
        "the edge is spelled in the colours a 256-colour terminal has"
    );
    assert!(
        matches!(Palette::TRUECOLOR.edge_ink(), Color::Rgb(..)),
        "and it is the colour the design was drawn in"
    );
}

#[test]
fn every_palette_the_probe_hands_out_is_a_spelling_the_floor_and_the_ladder_are_proved_on() {
    let mut picked: Vec<Palette> = Vec::new();

    // The vocabulary the probe reads, so every spelling it accepts and every one it turns down
    // is walked: a palette gated on one of these cannot land unproved.
    let no_colour = [None, Some("1")];
    let terms = [
        None,
        Some(""),
        Some("dumb"),
        Some("screen-256color"),
        Some("xterm-direct"),
        Some("xterm-truecolor"),
    ];
    let colorterms = [None, Some("truecolor"), Some("24bit"), Some("24-bit")];

    for no_color in no_colour {
        for term in terms {
            for colorterm in colorterms {
                let palette = Palette::probed(no_color, term, colorterm);
                if palette == Palette::SLOTS {
                    continue;
                }
                assert!(
                    SPELLED.iter().any(|(_, spelled)| *spelled == palette),
                    "the probe hands NO_COLOR={no_color:?} TERM={term:?} \
                         COLORTERM={colorterm:?} a palette no spelling in SPELLED names, so a \
                         third palette lands without the legibility floor and the ladder being \
                         re-proved on it"
                );
                if !picked.contains(&palette) {
                    picked.push(palette);
                }
            }
        }
    }

    for (spelling, palette) in SPELLED {
        assert!(
            picked.contains(&palette),
            "{spelling} is a spelling these tests prove and no fact the probe reads picks it, \
                 so it names a palette no terminal is ever drawn in"
        );
    }
}

#[test]
fn a_palette_that_names_no_colour_draws_no_ground_of_its_own() {
    for (spelling, palette) in SPELLED {
        for band in Band::ALL {
            assert_ne!(
                palette.band(band),
                Color::Reset,
                "{spelling} leaves {band:?} on the terminal's own ground, and a palette that \
                 spells a ground draws every band of a frame in it"
            );
        }
    }
    for band in Band::ALL {
        assert_eq!(
            Palette::SLOTS.band(band),
            Color::Reset,
            "a terminal asked for no colour leaves {band:?} on the ground it already has"
        );
    }
}

#[test]
fn the_frame_has_one_edge_ink_and_no_ink_and_no_band_is_drawn_in_it() {
    for (spelling, palette) in SPELLED {
        let edge = palette.edge_ink();

        for ink in Ink::ALL
            .into_iter()
            .filter(|ink| channels(palette.colour(*ink)).is_some())
        {
            assert_ne!(
                palette.colour(ink),
                edge,
                "{spelling} draws {} in the edge's own colour, and an edge is a surface a row \
                 is drawn on rather than an ink it is drawn in",
                ink.name()
            );
        }
        for band in Band::ALL {
            assert_ne!(
                palette.band(band),
                edge,
                "{spelling} leaves {band:?} and the edge no way to be told apart"
            );
        }
    }
}

#[test]
fn no_boundary_of_the_frame_is_its_brightest_surface() {
    for (spelling, palette) in SPELLED {
        let edge = lightness(palette.edge_ink());

        for ink in Ink::ALL
            .into_iter()
            .filter(|ink| channels(palette.colour(*ink)).is_some())
        {
            let drawn = lightness(palette.colour(ink));
            assert!(
                edge < drawn,
                "{spelling} draws the edge at L* {edge} over {} at L* {drawn}: a boundary \
                 brighter than the words beside it is the one thing a reader's eye lands on",
                ink.name()
            );
        }
    }
}

#[test]
fn the_prompt_band_is_spelled_in_every_palette_and_flat_where_a_palette_spells_no_ground() {
    assert_eq!(
        Palette::SLOTS.band(Band::Prompt),
        Color::Reset,
        "a palette that spells no colour is the reader's own ground under the prompt too, because \
         a palette that spells no ground draws none"
    );
    for (spelling, palette) in SPELLED {
        assert_ne!(
            palette.band(Band::Prompt),
            Color::Reset,
            "{spelling} spells the prompt band in a colour of its own, so the reader's own words \
             are lifted off the conversation they were asked in"
        );
    }
}

#[test]
fn the_ramp_lays_the_grounds_a_frame_is_drawn_on_by_lightness_and_every_band_reads_its_own() {
    for (spelling, palette) in SPELLED {
        let ramp = palette.ramp();

        assert_eq!(
            ramp.len(),
            Rung::ALL.len(),
            "{spelling} lays {} rungs where the design names {}",
            ramp.len(),
            Rung::ALL.len()
        );
        for (at, (rung, colour)) in ramp.iter().enumerate() {
            assert_eq!(
                *rung,
                Rung::ALL[at],
                "{spelling} lays {rung:?} at rung {at}, where the ramp's own order names {:?}",
                Rung::ALL[at]
            );
            if let Some((below, under)) = at.checked_sub(1).map(|below| ramp[below]) {
                let (here, there) = (lightness(*colour), lightness(under));
                assert!(
                    here > there,
                    "{spelling} lays {rung:?} at L* {here} over {below:?} at L* {there}: the \
                     ramp is the one order its grounds are read in, and a rung is lighter than \
                     the one it stands on"
                );
            }
        }

        let mut named: Vec<Rung> = Vec::new();
        for band in Band::ALL {
            let rung = band.rung();
            assert_eq!(
                palette.band(band),
                palette.rung(rung),
                "{spelling} draws {band:?} in {:?} where {rung:?} reads {:?}: every band's \
                 ground is read off the ramp",
                palette.band(band),
                palette.rung(rung)
            );
            assert!(
                !named.contains(&rung),
                "{spelling} draws {band:?} on {rung:?}, and a band beside it is drawn there too"
            );
            named.push(rung);
        }
        assert_eq!(
            named.len(),
            Rung::ALL.len(),
            "{spelling} names {:?}, so a rung of the ramp is drawn on by no band",
            named
        );
    }
}

#[test]
fn every_palette_spells_the_prompt_band_and_the_rails_tone_apart() {
    for (spelling, palette) in SPELLED {
        assert_ne!(
            palette.band(Band::Prompt),
            palette.band(Band::Rail),
            "{spelling} spells the prompt's band and the rail's tone one colour, so the band a \
             draft closes on is the band it rises on"
        );
    }
}

/// The least step the design lays between two rungs of its ramp, in L*: less and they read as one
/// surface.
const STEP: f64 = 1.5;

/// The most it lays, in L*: more and the band above reads as a second material, not a neighbour.
const CEILING: f64 = 6.0;

#[test]
fn no_two_rungs_of_the_ramp_sit_closer_than_the_designs_step_or_further_apart_than_its_ceiling() {
    for (spelling, palette) in SPELLED {
        let ramp = palette.ramp();

        for at in 1..ramp.len() {
            let (lower, under) = ramp[at - 1];
            let (upper, over) = ramp[at];
            let (under, over) = (lightness(under), lightness(over));
            let step = over - under;

            assert!(
                (STEP..=CEILING).contains(&step),
                "{spelling} lays {upper:?} at L* {over} over {lower:?} at L* {under}, a step of \
                 {step}: the ramp keeps its rungs between {STEP} and {CEILING}"
            );
        }
    }
}

/// The step the ramp keeps between the edge and the grounds it parts, in the L* the ramp itself is
/// laid out in: three is the least a reader can follow without a word beside it.
const APART: f64 = 3.0;

#[test]
fn the_hairline_is_no_rung_of_the_ramp_and_keeps_its_own_floor_below_every_rung() {
    let mut stands_further = false;

    for (spelling, palette) in SPELLED {
        let hairline = palette.edge_ink();
        let under = lightness(hairline);

        for (rung, colour) in palette.ramp() {
            assert_ne!(
                colour, hairline,
                "{spelling} draws {rung:?} in the hairline's own colour, so the ramp's step rule \
                 touches the edge it parts"
            );
            let over = lightness(colour);
            assert!(
                under < over,
                "{spelling} draws the hairline at L* {under} over {rung:?} at L* {over}: the \
                 edge is the darkest surface of the frame, under every ground it parts"
            );
            assert!(
                over - under >= APART,
                "{spelling} draws the hairline at L* {under} under {rung:?} at L* {over}: a \
                 boundary {} L* below the ground it parts is a boundary the eye cannot follow",
                over - under
            );
        }

        let lowest = lightness(palette.rung(Rung::ALL[0]));
        if lowest - under > CEILING {
            stands_further = true;
        }
    }

    assert!(
        stands_further,
        "no spelling stands its hairline further below the lowest rung than the ramp's ceiling \
         allows, so nothing proves an edge may sit a longer step below the surfaces it parts than \
         one surface may sit above another"
    );
}

#[test]
fn a_terminal_told_no_colour_draws_no_edge() {
    assert_eq!(
        Palette::SLOTS.edge_ink(),
        Color::Reset,
        "a reader who asked for no colour is given no edge of this design's: the one it keeps \
         is its own bright or dark rule, and every frame drawn in it is flat"
    );
}

#[test]
fn the_accent_edge_is_the_accent_drawn_faint_and_a_terminal_with_no_colour_is_given_none() {
    let faint = Look::new(Ink::Accent, Face::Faint);

    assert_eq!(
        Palette::SLOTS.accent_edge(),
        Palette::SLOTS.dim_edge(),
        "a reader who asked for no colour is given no accent edge: their focus is carried by \
             the card's border and the scrim, and never by a colour they asked away"
    );
    assert_eq!(
        Palette::SLOTS.dim_edge(),
        Look::plain(Ink::Dim),
        "a surface that takes no keys keeps the dim ink its edge row has always been drawn in"
    );
    for (spelling, palette) in SPELLED {
        assert_eq!(
            palette.accent_edge(),
            faint,
            "{spelling} draws the accent edge as something other than the accent in the faint \
                 face, and a second ink is what this design never adds"
        );
        assert_eq!(palette.dim_edge(), Look::plain(Ink::Dim));

        let mut cell = Cell::default();
        cell.set_style(palette.accent_edge().style(&palette));
        assert_eq!(
            Look::of(&cell, &palette),
            Some(faint),
            "the accent edge {spelling} draws reads back as no look a fixture can name"
        );
    }
}

#[test]
fn no_two_inks_resolve_to_one_foreground_and_modifier() {
    for (spelling, palette) in [
        ("slots", Palette::SLOTS),
        ("ansi", Palette::ANSI),
        ("truecolor", Palette::TRUECOLOR),
    ] {
        for (at, ink) in Ink::ALL.into_iter().enumerate() {
            for other in Ink::ALL.into_iter().skip(at + 1) {
                assert_ne!(
                    palette.ink(ink),
                    palette.ink(other),
                    "{spelling} draws {} and {} the same, so a cell drawn in one reads back \
                         as the other",
                    ink.name(),
                    other.name()
                );
            }
        }
    }
}

/// The contrast a word is legible at: the dim ink carries the strip's separators and the
/// rail's secondary lines, so no band may read it below this.
const LEGIBLE: f64 = 4.2;

#[test]
fn every_ink_keeps_its_legibility_floor_on_every_rung_a_frame_draws_on() {
    for (spelling, palette) in SPELLED {
        let mut grounds: Vec<(&str, Color)> = palette
            .ramp()
            .iter()
            .map(|(rung, colour)| (rung.name(), *colour))
            .collect();
        grounds.push(("the hairline", palette.edge_ink()));

        for ink in Ink::ALL
            .into_iter()
            .filter(|ink| channels(palette.colour(*ink)).is_some())
        {
            for (ground, colour) in &grounds {
                let ratio = contrast(palette.colour(ink), *colour);

                assert!(
                    ratio >= LEGIBLE,
                    "{spelling} reads {} on {ground} under {LEGIBLE}:1, and a word under that \
                     is a word a reader has to guess",
                    ink.name()
                );
            }
        }
    }
}

#[test]
fn the_ladder_keeps_the_steps_it_had() {
    // The rungs this ladder is held to: the design's three, in both spellings, so a rung that
    // came closer to the one above it than these do is caught.
    let drawn = [
        (
            "ansi",
            Palette::ANSI,
            [
                Color::Indexed(252),
                Color::Indexed(246),
                Color::Indexed(240),
            ],
        ),
        (
            "truecolor",
            Palette::TRUECOLOR,
            [
                Color::Rgb(206, 212, 221),
                Color::Rgb(140, 149, 160),
                Color::Rgb(88, 96, 106),
            ],
        ),
    ];
    let rungs = [Ink::Plain, Ink::Muted, Ink::Dim];

    for (spelling, palette, ladder) in drawn {
        for band in Band::ALL {
            let had = ladder.map(|rung| contrast(rung, palette.band(band)));
            let has = rungs.map(|ink| reads(&palette, ink, band));

            assert!(
                has[0] > has[1] && has[1] > has[2],
                "{spelling} reads plain, muted and dim on {band:?} as {has:?}, which is no \
                     ladder a reader can climb"
            );
            assert!(
                has[0] - has[1] >= had[0] - had[1],
                "{spelling} brings plain and muted {:?} contrast points closer on {band:?} \
                     than the {} it was drawn at",
                has[0] - has[1],
                had[0] - had[1]
            );
            assert!(
                has[1] - has[2] >= had[1] - had[2],
                "{spelling} brings muted and dim {:?} contrast points closer on {band:?} \
                     than the {} it was drawn at",
                has[1] - has[2],
                had[1] - had[2]
            );
        }
    }
}

/// The contrast an ink reads at on a band's ground.
fn reads(palette: &Palette, ink: Ink, band: Band) -> f64 {
    contrast(palette.ink(ink).0, palette.band(band))
}

/// The contrast one colour reads against another, the way WCAG measures it.
fn contrast(one: Color, other: Color) -> f64 {
    let (one, other) = (luminance(one), luminance(other));
    let (high, low) = if one > other {
        (one, other)
    } else {
        (other, one)
    };
    (high + 0.05) / (low + 0.05)
}

/// The relative luminance of a colour, from the channels it is spelled in.
fn luminance(color: Color) -> f64 {
    let [red, green, blue] = spelled(color).map(|channel| {
        let channel = f64::from(channel) / 255.0;
        match channel <= 0.03928 {
            true => channel / 12.92,
            false => ((channel + 0.055) / 1.055).powf(2.4),
        }
    });
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

/// The CIELAB lightness of a colour, read off the luminance of the channels it is spelled in.
fn lightness(color: Color) -> f64 {
    let luminance = luminance(color);
    match luminance > 0.008_856 {
        true => 116.0 * luminance.cbrt() - 16.0,
        false => 903.3 * luminance,
    }
}

/// The channels a colour is spelled in, so a ratio can be measured: the terminal's own sixteen
/// slots are the reader's colours and not this design's, and no ratio here is about them.
fn spelled(color: Color) -> [u8; 3] {
    match color {
        Color::Rgb(red, green, blue) => [red, green, blue],
        Color::Indexed(at @ 16..=231) => {
            let at = at - 16;
            let step = [0, 95, 135, 175, 215, 255];
            [
                step[usize::from(at / 36)],
                step[usize::from(at % 36 / 6)],
                step[usize::from(at % 6)],
            ]
        }
        Color::Indexed(at @ 232..=255) => [8 + 10 * (at - 232); 3],
        other => panic!("{other:?} is a slot this design does not draw and cannot measure"),
    }
}

#[test]
fn a_cell_is_read_back_by_its_foreground_and_never_its_background() {
    for palette in [Palette::SLOTS, Palette::ANSI, Palette::TRUECOLOR] {
        for ink in Ink::ALL {
            for band in [None, Some(Band::Rail), Some(Band::Prompt), Some(Band::Well)] {
                let mut cell = Cell::default();
                cell.set_style(ink.style(&palette));
                if let Some(band) = band {
                    cell.set_style(Style::default().bg(palette.band(band)));
                }

                assert_eq!(
                    Ink::of(&cell, &palette),
                    Some(ink),
                    "{} over {band:?} is still {}",
                    ink.name(),
                    ink.name()
                );
            }
        }
    }
}

#[test]
fn a_colour_is_spelled_in_the_channels_a_pixel_is_drawn_in_and_a_slots_colour_in_none() {
    assert_eq!(
        channels(Color::Rgb(19, 27, 37)),
        Some([19, 27, 37]),
        "a colour the design names is already the channels a pixel is drawn in"
    );
    assert_eq!(
        channels(Color::Indexed(196)),
        Some([255, 0, 0]),
        "and the cube's index is the three steps of its axis"
    );
    assert_eq!(
        channels(Color::Indexed(235)),
        Some([38, 38, 38]),
        "a grey is the ladder the design leans on where it has no twenty-four-bit colour"
    );
    for slots in [
        Color::Reset,
        Color::Indexed(0),
        Color::Indexed(15),
        Color::Black,
        Color::Cyan,
        Color::DarkGray,
    ] {
        assert_eq!(
            channels(slots),
            None,
            "{slots:?} is a colour the reader's own terminal holds, and no pixel of this design \
             is drawn in it"
        );
    }
}
