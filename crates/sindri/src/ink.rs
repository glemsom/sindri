use ratatui::buffer::Cell;
use ratatui::style::{Modifier, Style};

use crate::palette::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Plain,
    Dim,
    Muted,
    Accent,
    Ok,
    Err,
    Mark,
}

impl Ink {
    pub const ALL: [Ink; 7] = [
        Ink::Plain,
        Ink::Dim,
        Ink::Muted,
        Ink::Accent,
        Ink::Ok,
        Ink::Err,
        Ink::Mark,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Ink::Plain => "plain",
            Ink::Dim => "dim",
            Ink::Muted => "muted",
            Ink::Accent => "accent",
            Ink::Ok => "ok",
            Ink::Err => "err",
            Ink::Mark => "mark",
        }
    }

    pub fn from_name(name: &str) -> Option<Ink> {
        Ink::ALL.into_iter().find(|ink| ink.name() == name)
    }

    /// The style one cell of this ink is drawn in. It names a foreground and a modifier and
    /// never a background, so an ink painted onto a band keeps the surface under it.
    pub fn style(self, palette: &Palette) -> Style {
        let (foreground, modifier) = palette.ink(self);
        Style::default().fg(foreground).add_modifier(modifier)
    }

    pub fn of(cell: &Cell, palette: &Palette) -> Option<Ink> {
        Look::of(cell, palette).map(Look::ink)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Face {
    #[default]
    Plain,
    Bold,
    Reversed,
}

impl Face {
    pub(crate) const ALL: [Face; 3] = [Face::Plain, Face::Bold, Face::Reversed];

    pub fn name(self) -> &'static str {
        match self {
            Face::Plain => "plain",
            Face::Bold => "bold",
            Face::Reversed => "reversed",
        }
    }

    pub fn from_name(name: &str) -> Option<Face> {
        Face::ALL.into_iter().find(|face| face.name() == name)
    }

    pub(crate) fn modifier(self) -> Modifier {
        match self {
            Face::Plain => Modifier::empty(),
            Face::Bold => Modifier::BOLD,
            Face::Reversed => Modifier::REVERSED,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    ink: Ink,
    face: Face,
}

impl Look {
    pub const fn new(ink: Ink, face: Face) -> Look {
        Look { ink, face }
    }

    pub const fn plain(ink: Ink) -> Look {
        Look::new(ink, Face::Plain)
    }

    pub fn ink(self) -> Ink {
        self.ink
    }

    pub fn face(self) -> Face {
        self.face
    }

    pub fn style(self, palette: &Palette) -> Style {
        self.ink.style(palette).add_modifier(self.face.modifier())
    }

    pub fn name(self) -> String {
        match self.face {
            Face::Plain => String::from(self.ink.name()),
            face => format!("{}+{}", self.ink.name(), face.name()),
        }
    }

    pub fn from_name(name: &str) -> Option<Look> {
        let (ink, face) = match name.split_once('+') {
            Some((ink, face)) => (ink, Face::from_name(face)?),
            None => (name, Face::Plain),
        };
        Some(Look::new(Ink::from_name(ink)?, face))
    }

    pub fn of(cell: &Cell, palette: &Palette) -> Option<Look> {
        palette.look_of(cell)
    }
}

impl From<Ink> for Look {
    fn from(ink: Ink) -> Look {
        Look::plain(ink)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTES: [Palette; 3] = [Palette::SLOTS, Palette::ANSI, Palette::TRUECOLOR];

    #[test]
    fn every_ink_reads_back_as_the_name_it_is_drawn_in() {
        for palette in PALETTES {
            for ink in Ink::ALL {
                let mut cell = Cell::default();
                cell.set_style(ink.style(&palette));

                assert_eq!(
                    Ink::of(&cell, &palette),
                    Some(ink),
                    "a cell drawn in {} is that ink, and a fixture has to be able to say so",
                    ink.name()
                );
                assert_eq!(Ink::from_name(ink.name()), Some(ink));
            }
        }
    }

    #[test]
    fn a_cell_a_mark_covers_reads_back_as_the_mark_and_no_other_ink() {
        for palette in PALETTES {
            let mut cell = Cell::default();
            cell.set_style(Ink::Mark.style(&palette));

            assert_eq!(
                Ink::of(&cell, &palette),
                Some(Ink::Mark),
                "the marked cell is in `mark`, which is a name a reader can look up",
            );
        }
    }

    #[test]
    fn every_look_reads_back_as_the_two_names_it_is_drawn_in() {
        for palette in PALETTES {
            let mark = Look::plain(Ink::Mark);
            for ink in Ink::ALL {
                for face in Face::ALL {
                    let look = Look::new(ink, face);
                    let mut cell = Cell::default();
                    cell.set_style(look.style(&palette));

                    if look.style(&palette) == mark.style(&palette) {
                        assert_eq!(Look::of(&cell, &palette), Some(mark));
                        continue;
                    }
                    assert_eq!(
                        Look::of(&cell, &palette),
                        Some(look),
                        "a cell painted {:?} is that look, and a fixture has to be able to say so",
                        look.name()
                    );
                    assert_eq!(Look::from_name(&look.name()), Some(look));
                }
            }
        }
    }
}
