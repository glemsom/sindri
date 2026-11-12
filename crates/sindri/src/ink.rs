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
    Italic,
    Reversed,
    /// The accent at half its weight, so a face can recede without a second ink being named.
    Faint,
}

impl Face {
    /// Every face in the one order a name reads them, with the plain one first because it is the
    /// face of a look that wears none.
    pub(crate) const ALL: [Face; 5] = [
        Face::Plain,
        Face::Bold,
        Face::Italic,
        Face::Reversed,
        Face::Faint,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Face::Plain => "plain",
            Face::Bold => "bold",
            Face::Italic => "italic",
            Face::Reversed => "reversed",
            Face::Faint => "faint",
        }
    }

    pub fn from_name(name: &str) -> Option<Face> {
        Face::ALL.into_iter().find(|face| face.name() == name)
    }

    pub(crate) fn modifier(self) -> Modifier {
        match self {
            Face::Plain => Modifier::empty(),
            Face::Bold => Modifier::BOLD,
            Face::Italic => Modifier::ITALIC,
            Face::Reversed => Modifier::REVERSED,
            Face::Faint => Modifier::DIM,
        }
    }

    /// The one bit this face holds in a set, and no bit at all for the face a look wearing none is
    /// named by: `plain` is the empty set rather than a weight beside the others.
    const fn bit(self) -> u8 {
        match self {
            Face::Plain => 0,
            Face::Bold => 1,
            Face::Italic => 1 << 1,
            Face::Reversed => 1 << 2,
            Face::Faint => 1 << 3,
        }
    }
}

/// The faces one ink wears: a set, because a strong word inside a heading is bold and italic at
/// once rather than a third weight nobody named.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Faces(u8);

impl Faces {
    pub const PLAIN: Faces = Faces(0);
    pub const BOLD: Faces = Faces::wearing(Face::Bold);
    pub const ITALIC: Faces = Faces::wearing(Face::Italic);
    pub const REVERSED: Faces = Faces::wearing(Face::Reversed);
    pub const FAINT: Faces = Faces::wearing(Face::Faint);

    pub const fn wearing(face: Face) -> Faces {
        Faces(face.bit())
    }

    /// The faces both sets name, so two faces compose into one look.
    pub const fn and(self, other: Faces) -> Faces {
        Faces(self.0 | other.0)
    }

    pub fn has(self, face: Face) -> bool {
        self.0 & face.bit() != 0
    }

    pub fn is_plain(self) -> bool {
        self.0 == 0
    }

    /// The set read in the one order a name writes it.
    pub fn iter(self) -> impl Iterator<Item = Face> {
        Face::ALL.into_iter().filter(move |face| self.has(*face))
    }

    pub fn modifier(self) -> Modifier {
        self.iter()
            .fold(Modifier::empty(), |spent, face| spent | face.modifier())
    }

    pub fn name(self) -> String {
        let names: Vec<&str> = self.iter().map(Face::name).collect();
        match names.is_empty() {
            true => String::from(Face::Plain.name()),
            false => names.join("+"),
        }
    }

    pub fn from_name(name: &str) -> Option<Faces> {
        name.split('+').try_fold(Faces::PLAIN, |worn, face| {
            Some(worn.and(Faces::wearing(Face::from_name(face)?)))
        })
    }

    /// The faces `modifier` wears, and nothing at all when it carries a weight no face names: a
    /// cell is read back only as a look the product could have drawn. The underline is spent
    /// first, because it is the line a look carries and never a weight the ink wears.
    pub fn of_modifier(modifier: Modifier) -> Option<Faces> {
        let mut rest = modifier - Modifier::UNDERLINED;
        let mut worn = Faces::PLAIN;
        for face in Face::ALL {
            let spent = face.modifier();
            if spent.is_empty() || !rest.contains(spent) {
                continue;
            }
            rest -= spent;
            worn = worn.and(Faces::wearing(face));
        }
        rest.is_empty().then_some(worn)
    }
}

impl From<Face> for Faces {
    fn from(face: Face) -> Faces {
        Faces::wearing(face)
    }
}

/// How a look is written down: its ink, the faces it wears after a `+`, and the ink of the line
/// under it after a `~`. The fixture names every cell of a frame this way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    ink: Ink,
    faces: Faces,
    underline: Option<Ink>,
}

impl Look {
    pub fn new(ink: Ink, faces: impl Into<Faces>) -> Look {
        Look {
            ink,
            faces: faces.into(),
            underline: None,
        }
    }

    pub const fn plain(ink: Ink) -> Look {
        Look {
            ink,
            faces: Faces::PLAIN,
            underline: None,
        }
    }

    /// The same look with a line drawn under it, in an ink of its own: what a link will wear.
    pub const fn underlined(self, underline: Ink) -> Look {
        Look {
            underline: Some(underline),
            ..self
        }
    }

    pub fn ink(self) -> Ink {
        self.ink
    }

    pub fn faces(self) -> Faces {
        self.faces
    }

    pub fn underline(self) -> Option<Ink> {
        self.underline
    }

    pub fn style(self, palette: &Palette) -> Style {
        let style = self.ink.style(palette).add_modifier(self.faces.modifier());
        match self.underline {
            Some(underline) => style
                .add_modifier(Modifier::UNDERLINED)
                .underline_color(palette.colour(underline)),
            None => style,
        }
    }

    pub fn name(self) -> String {
        let mut name = match self.faces.is_plain() {
            true => String::from(self.ink.name()),
            false => format!("{}+{}", self.ink.name(), self.faces.name()),
        };
        if let Some(underline) = self.underline {
            name.push('~');
            name.push_str(underline.name());
        }
        name
    }

    pub fn from_name(name: &str) -> Option<Look> {
        let (name, underline) = match name.split_once('~') {
            Some((name, underline)) => (name, Some(Ink::from_name(underline)?)),
            None => (name, None),
        };
        let (ink, faces) = match name.split_once('+') {
            Some((ink, faces)) => (ink, Faces::from_name(faces)?),
            None => (name, Faces::PLAIN),
        };
        let look = Look::new(Ink::from_name(ink)?, faces);
        Some(match underline {
            Some(underline) => look.underlined(underline),
            None => look,
        })
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
    use crate::palette::Band;

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
    fn the_accent_drawn_faint_reads_back_as_the_accent_drawn_faint() {
        let faint = Look::new(Ink::Accent, Face::Faint);

        assert_eq!(
            faint.name(),
            "accent+faint",
            "the face a look is drawn in is its name"
        );
        for palette in PALETTES {
            for band in [
                None,
                Some(Band::Rail),
                Some(Band::Composer),
                Some(Band::Well),
            ] {
                let mut cell = Cell::default();
                cell.set_style(faint.style(&palette));
                if let Some(band) = band {
                    cell.set_style(Style::default().bg(palette.band(band)));
                }

                assert_eq!(
                    Look::of(&cell, &palette),
                    Some(faint),
                    "a faint accent written on {band:?} is a look a fixture can name, so it reads \
                     back as itself"
                );
            }
        }
        assert_eq!(Look::from_name("accent+faint"), Some(faint));
    }

    #[test]
    fn a_cell_wearing_two_faces_reads_back_as_both_of_them_in_one_order() {
        let emphasis = Look::new(Ink::Plain, Faces::BOLD.and(Faces::ITALIC));

        assert_eq!(
            emphasis.name(),
            "plain+bold+italic",
            "a look wears a set of faces, and the fixture writes them in one order"
        );
        assert_eq!(
            Look::from_name("plain+italic+bold"),
            Some(emphasis),
            "the faces are a set, so the order they are named in is not the look"
        );
        for palette in PALETTES {
            let mut cell = Cell::default();
            cell.set_style(emphasis.style(&palette));

            assert_eq!(
                Look::of(&cell, &palette),
                Some(emphasis),
                "a cell wearing bold and italic at once reads back as neither one of them alone"
            );
        }
    }

    #[test]
    fn the_italic_face_is_one_an_ink_may_wear_and_every_palette_draws_the_design_in_it() {
        for ink in Ink::ALL {
            let italic = Look::new(ink, Face::Italic);
            for palette in PALETTES {
                let mut cell = Cell::default();
                cell.set_style(italic.style(&palette));

                assert_eq!(
                    Look::of(&cell, &palette),
                    Some(italic),
                    "the {} ink drawn italic reads back as some other look, and every palette is \
                     asked because one of them draws the ink in the terminal's own colour",
                    ink.name()
                );
            }
        }

        let slotted = Look::new(Ink::Accent, Face::Italic).style(&Palette::SLOTS);
        assert!(
            slotted.add_modifier.contains(Modifier::ITALIC),
            "the palette a terminal that draws no colour of its own is handed still draws the \
             face the design asks for"
        );
    }

    #[test]
    fn a_look_may_carry_a_line_under_it_and_reads_back_as_its_ink_its_faces_and_that_line() {
        let linked = Look::new(Ink::Accent, Faces::ITALIC).underlined(Ink::Dim);

        assert_eq!(linked.ink(), Ink::Accent);
        assert_eq!(linked.faces(), Faces::ITALIC);
        assert_eq!(linked.underline(), Some(Ink::Dim));
        assert_eq!(
            linked.name(),
            "accent+italic~dim",
            "an underlined look is named by its ink, its faces and the ink under it"
        );
        assert_eq!(Look::from_name("accent+italic~dim"), Some(linked));

        for palette in PALETTES {
            for band in [
                None,
                Some(Band::Rail),
                Some(Band::Composer),
                Some(Band::Well),
            ] {
                let mut cell = Cell::default();
                cell.set_style(linked.style(&palette));
                if let Some(band) = band {
                    cell.set_style(Style::default().bg(palette.band(band)));
                }

                assert_eq!(
                    Look::of(&cell, &palette),
                    Some(linked),
                    "an underlined look written on {band:?} reads back as some other look, so a \
                     fixture cannot write down a link"
                );
            }
        }
    }

    #[test]
    fn every_look_reads_back_as_the_names_it_is_drawn_in() {
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
