use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier, Style};

use crate::ink::{Face, Faces, Ink, Look};

/// The colours one spelling of the design is drawn in: three palettes, one design, and never a
/// second look for a terminal that names fewer colours than another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    inks: Inks,
    bands: Bands,
    ends: Ends,
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

/// The two ends a boundary is drawn at: the lit column a band meets the frame on and the shadow
/// across its last row, and no ink of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ends {
    lit: Color,
    shadow: Color,
}

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
        ends: Ends {
            lit: Color::Reset,
            shadow: Color::Reset,
        },
    };

    /// The design in the colours a terminal with no twenty-four-bit colour has.
    pub const ANSI: Palette = Palette {
        inks: Inks {
            plain: (Color::Indexed(255), Modifier::empty()),
            dim: (Color::Indexed(246), Modifier::empty()),
            muted: (Color::Indexed(250), Modifier::empty()),
            accent: (Color::Indexed(80), Modifier::empty()),
            ok: (Color::Indexed(115), Modifier::empty()),
            err: (Color::Indexed(210), Modifier::empty()),
            mark: (Color::Reset, Modifier::REVERSED),
        },
        bands: Bands {
            rail: Color::Indexed(235),
            composer: Color::Indexed(236),
            well: Color::Indexed(234),
        },
        ends: Ends {
            lit: Color::Indexed(239),
            shadow: Color::Indexed(232),
        },
    };

    /// The design as it was drawn.
    pub const TRUECOLOR: Palette = Palette {
        inks: Inks {
            plain: (Color::Rgb(231, 240, 250), Modifier::empty()),
            dim: (Color::Rgb(136, 146, 156), Modifier::empty()),
            muted: (Color::Rgb(175, 185, 195), Modifier::empty()),
            accent: (Color::Rgb(92, 200, 232), Modifier::empty()),
            ok: (Color::Rgb(127, 211, 166), Modifier::empty()),
            err: (Color::Rgb(242, 128, 122), Modifier::empty()),
            mark: (Color::Reset, Modifier::REVERSED),
        },
        bands: Bands {
            rail: Color::Rgb(19, 27, 37),
            composer: Color::Rgb(25, 35, 47),
            well: Color::Rgb(14, 21, 29),
        },
        ends: Ends {
            lit: Color::Rgb(36, 49, 63),
            shadow: Color::Rgb(8, 13, 19),
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

    /// The style a row of a raised surface is claimed in: the plain ink over the ground `step` gives
    /// that row, so the ink written on it is the ink it was written in.
    pub fn raised(&self, row: u16, rows: u16) -> Style {
        self.paint(self.step(row, rows))
    }

    /// The style a cell of one end of a boundary's light is claimed in: the plain ink over that
    /// edge, so a cell on an edge still reads back as the ink it is written in.
    pub fn edged(&self, edge: Edge) -> Style {
        self.paint(self.edge(edge))
    }

    fn paint(&self, background: Color) -> Style {
        let (foreground, modifier) = self.ink(Ink::Plain);
        Style::default()
            .fg(foreground)
            .bg(background)
            .add_modifier(modifier)
    }

    /// The look an edge row is drawn in while its surface takes the keys: the accent in the faint
    /// face, and the dim edge for a palette that draws no edge at all.
    pub fn accent_edge(&self) -> Look {
        match self.draws_edge() {
            true => Look::new(Ink::Accent, Face::Faint),
            false => self.dim_edge(),
        }
    }

    /// The look an edge row is drawn in while a run is live: the accent lit, and the dim edge for a
    /// palette that draws no light at all.
    pub fn lit_edge(&self) -> Look {
        match self.draws_edge() {
            true => Look::plain(Ink::Accent),
            false => self.dim_edge(),
        }
    }

    /// The look an edge row is drawn in while its surface takes no keys.
    pub fn dim_edge(&self) -> Look {
        Look::plain(Ink::Dim)
    }

    /// Whether this palette draws an edge at all.
    fn draws_edge(&self) -> bool {
        self.edge(Edge::Lit) != Color::Reset
    }

    /// The colour an edge is drawn in: the lit end at its lightest, the shadow at its darkest.
    pub fn edge(&self, edge: Edge) -> Color {
        match edge {
            Edge::Lit => self.ends.lit,
            Edge::Shadow => self.ends.shadow,
        }
    }

    /// The ground the `row`th row of a raised surface `rows` rows tall sits on: its head is the
    /// composer's tone, the depth above the rail's, and its foot is the rail's own, with the rows
    /// between shaded from one end to the other.
    pub fn step(&self, row: u16, rows: u16) -> Color {
        let span = rows.saturating_sub(1);
        shade(self.bands.composer, self.bands.rail, row, span)
    }

    pub fn band(&self, band: Band) -> Color {
        match band {
            Band::Rail => self.bands.rail,
            Band::Composer => self.bands.composer,
            Band::Well => self.bands.well,
        }
    }

    /// The colour an ink is drawn in, and never the face it wears: the colour the line under a
    /// cell is drawn in, read back to the ink it is.
    pub fn colour(&self, ink: Ink) -> Color {
        self.ink(ink).0
    }

    /// The look a cell is read back as, against the palette it was drawn with. The most specific ink
    /// wins, because only one spends a modifier: reverse video over no colour is both `mark` and
    /// `plain` reversed. A cell's background is never read, so a rail cell reads back on its surface.
    pub fn look_of(&self, cell: &Cell) -> Option<Look> {
        let underlined = cell.modifier.contains(Modifier::UNDERLINED);
        if !underlined && cell.underline_color != Color::Reset {
            return None;
        }
        let underline = match underlined {
            true => Some(self.ink_of(cell.underline_color)?),
            false => None,
        };
        Ink::ALL.into_iter().rev().find_map(|ink| {
            let (foreground, spent) = self.ink(ink);
            if foreground != cell.fg || !cell.modifier.contains(spent) {
                return None;
            }
            let look = Look::new(ink, Faces::of_modifier(cell.modifier - spent)?);
            Some(underline.map_or(look, |underline| look.underlined(underline)))
        })
    }

    /// The ink a colour is drawn in: the first ink that draws it, because a palette whose inks
    /// share a colour still has to read the line under a cell back as one of them.
    fn ink_of(&self, colour: Color) -> Option<Ink> {
        Ink::ALL.into_iter().find(|ink| self.colour(*ink) == colour)
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

/// The channels a colour is spelled in, and nothing for a colour the terminal draws in a slot of
/// the reader's own: the sixteen slots and `Reset` have none this design can put in a pixel.
pub fn channels(colour: Color) -> Option<[u8; 3]> {
    match colour {
        Color::Rgb(red, green, blue) => Some([red, green, blue]),
        Color::Indexed(at @ 16..=231) => {
            let at = at - 16;
            let step = [0, 95, 135, 175, 215, 255];
            Some([
                step[usize::from(at / 36)],
                step[usize::from(at % 36 / 6)],
                step[usize::from(at % 6)],
            ])
        }
        Color::Indexed(at @ 232..=255) => Some([8 + 10 * (at - 232); 3]),
        _ => None,
    }
}

/// The shade `at` of `span` from `head` to `foot`, in the spelling both ends share: two `Rgb` ends
/// mix per channel and two `Indexed` ends mix their slots, which ascend with the grey they are, and
/// ends with no shade between them are drawn as the head.
fn shade(head: Color, foot: Color, at: u16, span: u16) -> Color {
    let (at, span) = (u32::from(at.min(span)), u32::from(span));
    let between = |head: u8, foot: u8| match span {
        0 => head,
        span => ((u32::from(head) * (span - at) + u32::from(foot) * at + span / 2) / span) as u8,
    };
    match (head, foot) {
        (Color::Rgb(red, green, blue), Color::Rgb(foot_red, foot_green, foot_blue)) => Color::Rgb(
            between(red, foot_red),
            between(green, foot_green),
            between(blue, foot_blue),
        ),
        (Color::Indexed(head), Color::Indexed(foot)) => Color::Indexed(between(head, foot)),
        _ => head,
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
mod tests;
