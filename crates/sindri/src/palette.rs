use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier, Style};

use crate::ink::{Face, Ink, Look};

/// The colours one spelling of the design is drawn in: three palettes, one design, and never a
/// second look for a terminal that names fewer colours than another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    inks: Inks,
    bands: Bands,
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

impl Palette {
    /// The terminal's own six slots and no background: what a reader who asked for no colour gets.
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
    };

    /// The design in the colours a terminal with no twenty-four-bit colour has.
    pub const ANSI: Palette = Palette {
        inks: Inks {
            plain: (Color::Indexed(252), Modifier::empty()),
            dim: (Color::Indexed(240), Modifier::empty()),
            muted: (Color::Indexed(246), Modifier::empty()),
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
    };

    /// The design as it was drawn.
    pub const TRUECOLOR: Palette = Palette {
        inks: Inks {
            plain: (Color::Rgb(206, 212, 221), Modifier::empty()),
            dim: (Color::Rgb(88, 96, 106), Modifier::empty()),
            muted: (Color::Rgb(140, 149, 160), Modifier::empty()),
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

    /// The style the cells of a band start as: the plain ink over the band's own colour, and over
    /// the terminal's own for a cell on no band. A cell a band's words leave blank is still the
    /// frame's ink, and a band that shrank leaves no stripe of the frame before it behind.
    pub fn ground(&self, band: Option<Band>) -> Style {
        let (foreground, modifier) = self.ink(Ink::Plain);
        Style::default()
            .fg(foreground)
            .bg(band.map_or(Color::Reset, |band| self.band(band)))
            .add_modifier(modifier)
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

    #[test]
    fn a_terminal_told_no_colour_is_given_the_six_slots_and_no_background() {
        let uncoloured = Palette::probed(Some("1"), Some("xterm-256color"), Some("truecolor"));

        assert_eq!(uncoloured, Palette::SLOTS);
        assert_eq!(
            Palette::SLOTS.ink(Ink::Accent).0,
            Color::Cyan,
            "an uncoloured terminal keeps the slot it always had, and not the design's accent"
        );
        for band in [Band::Rail, Band::Composer, Band::Well] {
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
        for band in [Band::Rail, Band::Composer, Band::Well] {
            assert!(
                matches!(ansi.band(band), Color::Indexed(_)),
                "{band:?} keeps the design's three steps and spells them in ANSI"
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
