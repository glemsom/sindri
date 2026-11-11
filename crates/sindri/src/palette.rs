use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier, Style};

use crate::composer;
use crate::ink::{Face, Ink, Look};

/// The colours one spelling of the design is drawn in: three palettes, one design, and never a
/// second look for a terminal that names fewer colours than another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    inks: Inks,
    bands: Bands,
    ramp: Ramp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Inks {
    plain: (Color, Modifier),
    dim: (Color, Modifier),
    muted: (Color, Modifier),
    accent: (Color, Modifier),
    ok: (Color, Modifier),
    err: (Color, Modifier),
    mark: (Color, Modifier),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bands {
    rail: Color,
    composer: Color,
    well: Color,
}

/// The grounds a raised surface's face is drawn in, one per row from the lit edge along its top to
/// the shadow under its bottom: every height a band takes is drawn from this one ladder, so the band
/// reads as one raised surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ramp {
    steps: [Color; STEPS],
}

const STEPS: usize = composer::MAX_ROWS as usize;

impl Palette {
    /// The terminal's own six slots, no background and no edge: what a reader who asked for no
    /// colour gets, and every frame drawn in it is flat.
    pub const SLOTS: Palette = Palette {
        inks: Inks {
            plain: (Color::Reset, Modifier::empty()),
            dim: (Color::DarkGray, Modifier::empty()),
            muted: (Color::Gray, Modifier::empty()),
            accent: (Color::Cyan, Modifier::empty()),
            ok: (Color::Green, Modifier::empty()),
            err: (Color::Red, Modifier::empty()),
            mark: (Color::Reset, Modifier::REVERSED),
        },
        bands: Bands {
            rail: Color::Reset,
            composer: Color::Reset,
            well: Color::Reset,
        },
        ramp: Ramp {
            steps: [Color::Reset; STEPS],
        },
    };

    /// The design in the colours a terminal with no twenty-four-bit colour has.
    pub const ANSI: Palette = Palette {
        inks: Inks {
            plain: (Color::Indexed(255), Modifier::empty()),
            dim: (Color::Indexed(246), Modifier::empty()),
            muted: (Color::Indexed(250), Modifier::empty()),
            accent: (Color::Indexed(179), Modifier::empty()),
            ok: (Color::Indexed(108), Modifier::empty()),
            err: (Color::Indexed(174), Modifier::empty()),
            mark: (Color::Reset, Modifier::REVERSED),
        },
        bands: Bands {
            rail: Color::Indexed(235),
            composer: Color::Indexed(236),
            well: Color::Indexed(234),
        },
        ramp: Ramp {
            steps: [
                Color::Indexed(238),
                Color::Indexed(237),
                Color::Indexed(236),
                Color::Indexed(235),
                Color::Indexed(234),
                Color::Indexed(233),
            ],
        },
    };

    /// The design as it was drawn.
    pub const TRUECOLOR: Palette = Palette {
        inks: Inks {
            plain: (Color::Rgb(233, 241, 251), Modifier::empty()),
            dim: (Color::Rgb(141, 149, 159), Modifier::empty()),
            muted: (Color::Rgb(181, 189, 199), Modifier::empty()),
            accent: (Color::Rgb(226, 168, 96), Modifier::empty()),
            ok: (Color::Rgb(140, 184, 132), Modifier::empty()),
            err: (Color::Rgb(208, 104, 106), Modifier::empty()),
            mark: (Color::Reset, Modifier::REVERSED),
        },
        bands: Bands {
            rail: Color::Rgb(26, 30, 36),
            composer: Color::Rgb(32, 37, 44),
            well: Color::Rgb(20, 23, 28),
        },
        ramp: Ramp {
            steps: [
                Color::Rgb(48, 54, 64),
                Color::Rgb(40, 45, 54),
                Color::Rgb(32, 37, 44),
                Color::Rgb(26, 30, 36),
                Color::Rgb(20, 23, 28),
                Color::Rgb(12, 14, 18),
            ],
        },
    };

    /// The palette a terminal is drawn in, from the three facts that say what it can do.
    pub fn probed(no_color: Option<&str>, term: Option<&str>, colorterm: Option<&str>) -> Palette {
        if no_color.is_some() || term == Some("dumb") {
            return Palette::SLOTS;
        }
        match tells_truecolor(term, colorterm) {
            true => Palette::TRUECOLOR,
            false => Palette::ANSI,
        }
    }

    pub fn ink(&self, ink: Ink) -> (Color, Modifier) {
        match ink {
            Ink::Plain => self.inks.plain,
            Ink::Dim => self.inks.dim,
            Ink::Muted => self.inks.muted,
            Ink::Accent => self.inks.accent,
            Ink::Ok => self.inks.ok,
            Ink::Err => self.inks.err,
            Ink::Mark => self.inks.mark,
        }
    }

    /// The style the cells of a flat band start as: the plain ink over the band's own colour, and
    /// over the terminal's own for a cell on no band. A cell a band's words leave blank is still the
    /// frame's ink, and a band that shrank leaves no stripe of the frame before it behind.
    pub fn ground(&self, band: Option<Band>) -> Style {
        self.paint(band.map_or(Color::Reset, |band| self.band(band)))
    }

    /// The style a row of a raised surface is claimed in: the plain ink over the ground the ramp
    /// gives that row, so the ink written on it is the ink it was written in.
    pub fn raised(&self, row: u16, rows: u16) -> Style {
        self.paint(self.step(row, rows))
    }

    fn paint(&self, background: Color) -> Style {
        let (foreground, modifier) = self.ink(Ink::Plain);
        Style::default()
            .fg(foreground)
            .bg(background)
            .add_modifier(modifier)
    }

    pub fn edge(&self, edge: Edge) -> Color {
        match edge {
            Edge::Lit => self.ramp.steps[0],
            Edge::Shadow => self.ramp.steps[STEPS - 1],
        }
    }

    /// The ground the `row`th row of a raised surface `rows` rows tall sits on.
    pub fn step(&self, row: u16, rows: u16) -> Color {
        self.ramp.steps[step_at(row, rows)]
    }

    pub fn band(&self, band: Band) -> Color {
        match band {
            Band::Rail => self.bands.rail,
            Band::Composer => self.bands.composer,
            Band::Well => self.bands.well,
        }
    }

    /// The look a cell is read back as, against the palette it was drawn with. The most specific ink
    /// wins, because only one spends a modifier: reverse video over no colour is both `mark` and
    /// `plain` reversed. A cell's background is never read, so a rail cell reads back on its surface.
    pub fn look_of(&self, cell: &Cell) -> Option<Look> {
        if cell.underline_color != Color::Reset {
            return None;
        }
        Ink::ALL.into_iter().rev().find_map(|ink| {
            let (foreground, spent) = self.ink(ink);
            if foreground != cell.fg || !cell.modifier.contains(spent) {
                return None;
            }
            let rest = cell.modifier - spent;
            let face = Face::ALL.into_iter().find(|face| face.modifier() == rest)?;
            Some(Look::new(ink, face))
        })
    }
}

impl Default for Palette {
    /// The design as it was drawn: the probe is what decides a terminal cannot hold it, so a frame
    /// nobody told a palette about draws the one this product is designed in.
    fn default() -> Palette {
        Palette::TRUECOLOR
    }
}

/// The three places a frame paints a background under its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    Rail,
    Composer,
    Well,
}

impl Band {
    pub const ALL: [Band; 3] = [Band::Rail, Band::Composer, Band::Well];
}

/// A boundary's share of the light, painted under a row: never an ink, so a cell on an edge still
/// reads back as the ink it is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Lit,
    Shadow,
}

impl Edge {
    pub const ALL: [Edge; 2] = [Edge::Lit, Edge::Shadow];
}

/// The step of the ramp the `row`th of `rows` rows lands on: both ends are drawn at every height a
/// band takes, so none is lit twice and none is shadowed twice. A surface taller than the ramp has
/// fewer steps than rows and repeats one, and a surface of one row is drawn lit.
fn step_at(row: u16, rows: u16) -> usize {
    let span = usize::from(rows.saturating_sub(1));
    match span {
        0 => 0,
        span => usize::from(row.min(rows - 1)) * (STEPS - 1) / span,
    }
}

/// Whether the terminal says it draws twenty-four-bit colour: `COLORTERM` is the one variable
/// that says so outright, and a few terminals name it in `TERM` instead.
fn tells_truecolor(term: Option<&str>, colorterm: Option<&str>) -> bool {
    let named = |value: Option<&str>| {
        value.is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "truecolor" | "24bit" | "24-bit"
            )
        })
    };
    named(colorterm)
        || term.is_some_and(|term| {
            let term = term.to_ascii_lowercase();
            term.contains("truecolor") || term.ends_with("-direct")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::{MAX_ROWS, MIN_ROWS};

    /// The two spellings that name colours of their own; `SLOTS` wears the reader's theme instead.
    const SPELLED: [(&str, Palette); 2] =
        [("ansi", Palette::ANSI), ("truecolor", Palette::TRUECOLOR)];

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
}
