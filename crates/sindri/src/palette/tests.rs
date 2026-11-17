use super::*;
use crate::composer::{MAX_ROWS, MIN_ROWS};

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
    for edge in Edge::ALL {
        assert!(
            matches!(ansi.edge(edge), Color::Indexed(_)),
            "{edge:?} is spelled in the colours a 256-colour terminal has"
        );
        assert!(
            matches!(Palette::TRUECOLOR.edge(edge), Color::Rgb(..)),
            "{edge:?} is the colour the design was drawn in"
        );
    }
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
fn the_lit_edge_and_the_shadow_are_surfaces_no_ink_is_drawn_in() {
    for (spelling, palette) in SPELLED {
        let lit = palette.edge(Edge::Lit);
        let shadow = palette.edge(Edge::Shadow);

        assert_ne!(lit, shadow, "{spelling} draws one colour for both edges");
        for ink in Ink::ALL {
            for edge in Edge::ALL {
                assert_ne!(
                    palette.ink(ink).0,
                    palette.edge(edge),
                    "{spelling} draws {} in the colour of {edge:?}, and an edge is a surface \
                         a row is drawn on rather than an ink it is drawn in",
                    ink.name()
                );
            }
        }
        for band in Band::ALL {
            for edge in Edge::ALL {
                assert_ne!(
                    palette.band(band),
                    palette.edge(edge),
                    "{spelling} leaves {band:?} and {edge:?} no way to be told apart"
                );
            }
        }
    }
}

#[test]
fn the_three_bands_are_three_elevations_the_composer_highest_and_the_well_lowest() {
    for (spelling, palette) in SPELLED {
        let composer = luminance(palette.band(Band::Composer));
        let rail = luminance(palette.band(Band::Rail));
        let well = luminance(palette.band(Band::Well));

        assert!(
            composer > rail && rail > well,
            "{spelling} draws the composer, the rail and the well at {composer}, {rail} and \
                 {well}: three bands a frame paints, three elevations, and the well the darkest \
                 ground of them"
        );
    }
}

#[test]
fn the_lit_edge_stands_above_every_band_and_the_shadow_below() {
    for (spelling, palette) in SPELLED {
        let lit = luminance(palette.edge(Edge::Lit));
        let shadow = luminance(palette.edge(Edge::Shadow));

        for band in Band::ALL {
            let ground = luminance(palette.band(band));

            assert!(
                lit > ground,
                "{spelling} lights {band:?} no brighter than its own ground, so nothing above \
                     it can stand raised"
            );
            assert!(
                shadow < ground,
                "{spelling} shadows {band:?} no darker than its own ground, so nothing below \
                     it can sit sunken"
            );
        }
    }
}

#[test]
fn a_raised_surface_ramps_from_the_lit_edge_down_to_the_shadow() {
    for (spelling, palette) in SPELLED {
        for rows in MIN_ROWS..=MAX_ROWS {
            assert_eq!(
                palette.step(0, rows),
                palette.edge(Edge::Lit),
                "{spelling} draws no lit edge along the top of a raised band of {rows} rows"
            );
            assert_eq!(
                palette.step(rows - 1, rows),
                palette.edge(Edge::Shadow),
                "{spelling} leaves no shadow under a raised band of {rows} rows"
            );
            for row in 1..rows {
                assert!(
                    luminance(palette.step(row, rows)) < luminance(palette.step(row - 1, rows)),
                    "{spelling} ramps a raised band of {rows} rows to no darker ground at row\
                         {row}, so two of its rows are drawn in one"
                );
            }
        }
    }
}

#[test]
fn a_band_at_its_ceiling_takes_a_step_of_its_own_on_every_row() {
    for (spelling, palette) in SPELLED {
        let mut steps: Vec<Color> = Vec::new();
        for row in 0..MAX_ROWS {
            let step = palette.step(row, MAX_ROWS);
            assert!(
                !steps.contains(&step),
                "{spelling} draws row {row} of a {MAX_ROWS}-row band on a step the row above it \
                     already sits on, so two rows of the surface are drawn in one"
            );
            steps.push(step);
        }
        assert_eq!(
            steps.len(),
            usize::from(MAX_ROWS),
            "{spelling} gives a band at its ceiling fewer steps than it has rows"
        );
    }
}

#[test]
fn a_three_row_band_is_the_lit_edge_its_own_ground_and_the_shadow() {
    for (spelling, palette) in SPELLED {
        assert_eq!(
            palette.step(1, 3),
            palette.band(Band::Composer),
            "{spelling} gives a raised band of three rows no composer ground to draw its face \
                 on: a lit edge, the band's own colour, and the shadow under it"
        );
    }
}

#[test]
fn a_surface_with_one_row_has_no_room_for_both_ends_and_is_drawn_lit() {
    for (spelling, palette) in SPELLED {
        assert_eq!(
            palette.step(0, 1),
            palette.edge(Edge::Lit),
            "{spelling} lights no one-row band, and a shadow would leave it no top at all"
        );
    }
}

#[test]
fn a_surface_drawn_in_no_colour_ramps_nowhere() {
    for rows in MIN_ROWS..=MAX_ROWS {
        for row in 0..rows {
            assert_eq!(
                Palette::SLOTS.step(row, rows),
                Color::Reset,
                "row {row} of a raised band of {rows} rows draws no ground, so the frame stays \
                     flat"
            );
        }
    }
}

#[test]
fn a_terminal_told_no_colour_draws_no_edge_and_no_shadow() {
    for edge in Edge::ALL {
        assert_eq!(
            Palette::SLOTS.edge(edge),
            Color::Reset,
            "{edge:?} draws no colour, so every frame an uncoloured terminal draws is flat"
        );
    }
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
fn the_lit_edge_is_the_accent_with_no_faint_face_and_a_terminal_with_no_colour_keeps_its_dim_one() {
    assert_eq!(
        Palette::SLOTS.lit_edge(),
        Palette::SLOTS.dim_edge(),
        "a reader who asked for no colour is given no light either, so the edge a live run \
             lights is the dim one it already wears"
    );
    for (spelling, palette) in SPELLED {
        assert_eq!(
            palette.lit_edge(),
            Look::plain(Ink::Accent),
            "{spelling} draws the edge a live run lights in something other than the accent \
                 with no faint face"
        );
        assert_ne!(
            palette.lit_edge(),
            palette.accent_edge(),
            "a live edge is the accent lit where a still one is the accent faint, and a band a \
                 reader cannot tell those apart by says nothing of whether a run is live"
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
fn the_dim_ink_carries_words_on_every_band() {
    for (spelling, palette) in SPELLED {
        for band in Band::ALL {
            let ratio = reads(&palette, Ink::Dim, band);

            assert!(
                ratio >= LEGIBLE,
                "{spelling} reads dim on {band:?} under {LEGIBLE}:1, and a word under that \
                     is a word a reader has to guess"
            );
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
            for band in [
                None,
                Some(Band::Rail),
                Some(Band::Composer),
                Some(Band::Well),
            ] {
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
