use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier, Style};

use crate::ink::{Face, Faces, Ink, Look};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    inks: Inks,
    ramp: [Color; Rung::ALL.len()],
    edge: Color,
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

/// The floor every ink is proved against on the rung it is read on.
pub const LEGIBLE: f64 = 4.2;

/// The channels a seam's shadow takes past the step it has to clear, so a joint reads as a fold
/// and not as the rung beside it.
const JOINT: u8 = 2;

impl Palette {
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
        ramp: [Color::Reset; Rung::ALL.len()],
        edge: Color::Reset,
    };

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
        ramp: [
            Color::Indexed(234),
            Color::Indexed(235),
            Color::Indexed(236),
        ],
        edge: Color::Indexed(232),
    };

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
        ramp: [
            Color::Rgb(14, 21, 29),
            Color::Rgb(19, 27, 37),
            Color::Rgb(22, 31, 42),
        ],
        edge: Color::Rgb(5, 8, 13),
    };

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

    pub fn ground(&self, band: Option<Band>) -> Style {
        self.over(band.map_or(Color::Reset, |band| self.band(band)))
    }

    pub(crate) fn over(&self, colour: Color) -> Style {
        let (foreground, modifier) = self.ink(Ink::Plain);
        Style::default()
            .fg(foreground)
            .bg(colour)
            .add_modifier(modifier)
    }

    pub fn accent_edge(&self) -> Look {
        match self.draws_edge() {
            true => Look::new(Ink::Accent, Face::Faint),
            false => self.dim_edge(),
        }
    }

    pub fn dim_edge(&self) -> Look {
        Look::plain(Ink::Dim)
    }

    pub fn draws_edge(&self) -> bool {
        self.edge != Color::Reset
    }

    pub fn edge_ink(&self) -> Color {
        self.edge
    }

    pub fn band(&self, band: Band) -> Color {
        self.rung(band.rung())
    }

    pub fn rung(&self, rung: Rung) -> Color {
        self.ramp[rung as usize]
    }

    pub fn ramp(&self) -> [(Rung, Color); Rung::ALL.len()] {
        Rung::ALL.map(|rung| (rung, self.rung(rung)))
    }

    pub fn colour(&self, ink: Ink) -> Color {
        self.ink(ink).0
    }

    /// The most light a band's rung can take: the last channel of light this palette's every ink
    /// still clears its floor on, on the lit pixel.
    pub fn lift(&self, band: Band) -> u8 {
        let Some(rung) = channels(self.band(band)) else {
            return 0;
        };
        let inks: Vec<[u8; 3]> = Ink::ALL
            .into_iter()
            .filter_map(|ink| channels(self.colour(ink)))
            .collect();
        let mut held = 0;
        for light in 0..=u8::MAX {
            if inks
                .iter()
                .any(|ink| ratio(*ink, lit(rung, light)) < LEGIBLE)
            {
                break;
            }
            held = light;
        }
        held
    }

    /// The light a band spends: the share it names of the most its rung can take, and never a
    /// channel over that most.
    pub fn peak(&self, band: Band) -> u8 {
        (f64::from(self.lift(band)) * band.share()).round() as u8
    }

    /// The channels a seam drawn in this band takes from its rung at the joint: the step from that
    /// rung down to the well and the margin over it, so the joint sits under every band it parts.
    pub fn joint(&self, band: Band) -> u8 {
        let (Some(rung), Some(well)) = (channels(self.band(band)), channels(self.band(Band::Well)))
        else {
            return 0;
        };
        let step = (0..3)
            .map(|channel| rung[channel].saturating_sub(well[channel]))
            .max()
            .unwrap_or(0);
        step.saturating_add(JOINT)
    }

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

    fn ink_of(&self, colour: Color) -> Option<Ink> {
        Ink::ALL.into_iter().find(|ink| self.colour(*ink) == colour)
    }
}

impl Default for Palette {
    fn default() -> Palette {
        Palette::TRUECOLOR
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    Rail,
    Well,
    Prompt,
}

impl Band {
    pub const ALL: [Band; 3] = [Band::Well, Band::Rail, Band::Prompt];

    /// The share of its proved ceiling a band spends: the rail's is 85 %, and a band that
    /// spends all of it keeps its brightness.
    pub fn share(self) -> f64 {
        match self {
            Band::Rail => 0.85,
            Band::Well | Band::Prompt => 1.0,
        }
    }

    pub fn rung(self) -> Rung {
        match self {
            Band::Rail => Rung::Rail,
            Band::Well => Rung::Well,
            Band::Prompt => Rung::Prompt,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rung {
    Well = 0,
    Rail = 1,
    Prompt = 2,
}

impl Rung {
    pub const ALL: [Rung; 3] = [Rung::Well, Rung::Rail, Rung::Prompt];

    pub fn name(self) -> &'static str {
        match self {
            Rung::Well => "well",
            Rung::Rail => "rail",
            Rung::Prompt => "prompt",
        }
    }
}

fn lit(rung: [u8; 3], light: u8) -> [u8; 3] {
    rung.map(|channel| channel.saturating_add(light))
}

fn ratio(one: [u8; 3], other: [u8; 3]) -> f64 {
    let (one, other) = (luminance(one), luminance(other));
    let (high, low) = match one > other {
        true => (one, other),
        false => (other, one),
    };
    (high + 0.05) / (low + 0.05)
}

fn luminance(colour: [u8; 3]) -> f64 {
    let [red, green, blue] = colour.map(|channel| {
        let channel = f64::from(channel) / 255.0;
        match channel <= 0.03928 {
            true => channel / 12.92,
            false => ((channel + 0.055) / 1.055).powf(2.4),
        }
    });
    0.2126 * red + 0.7152 * green + 0.0722 * blue
}

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
