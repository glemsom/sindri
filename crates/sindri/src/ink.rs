//! The look a cell is drawn in, asked for by name: the ink it is in and the face it is
//! set in, so a palette change cannot break a frame fixture and a fixture cannot smuggle
//! in a theme (§12, ADR-0012).
//!
//! The glossary's five inks — `dim`, `muted`, `accent`, `ok`, `err` — are joined by
//! `plain`, which is no colour at all: a cell the terminal draws in whatever ink its
//! reader chose. A [`Face`] is the other half of a look and not a sixth colour: §12 has
//! a heading set bold and a run of inline code reversed, and neither is a role a palette
//! could supply. A [`Look`] is the pair, and `Look::of` reads a painted cell back into
//! the two names it is in. That reading is how a fixture is written down as roles rather
//! than as a palette, and it is why a cell painted a colour, a weight or a background
//! that is not one of these cannot be written down at all: the fixture would have to
//! name something no look does.

use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier, Style};

/// A colour role the interface asks for by name — the only way this front end colours
/// a cell, so that no widget, ticket or test can spend a colour nobody named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    /// Nothing is being said in particular: a blank cell, a transcript's prose, the
    /// terminal's own default ink.
    Plain,
    /// The facts a reader consults rather than reads: labels, leader dots, the
    /// hairline a draft is typed under.
    Dim,
    /// Secondary prose — hints, captions, a pulse that has settled — quieter than the
    /// thing it is about.
    Muted,
    /// The one thing the eye should land on: the newest answer, the cursor's row.
    Accent,
    /// A thing that finished well: `✓ exit 0`, a run that reached its final answer.
    Ok,
    /// A thing that failed: `✗ exit 1`, a refusal, a budget that ran out.
    Err,
}

impl Ink {
    /// Every ink there is, in the order this enum names them, so a reader of a fixture
    /// can look one up without a table of its own.
    pub const ALL: [Ink; 6] = [
        Ink::Plain,
        Ink::Dim,
        Ink::Muted,
        Ink::Accent,
        Ink::Ok,
        Ink::Err,
    ];

    /// The name a fixture writes down, and the name a ticket says out loud.
    pub fn name(self) -> &'static str {
        match self {
            Ink::Plain => "plain",
            Ink::Dim => "dim",
            Ink::Muted => "muted",
            Ink::Accent => "accent",
            Ink::Ok => "ok",
            Ink::Err => "err",
        }
    }

    /// The ink a fixture's name stands for, or nothing when no ink is called that.
    pub fn from_name(name: &str) -> Option<Ink> {
        Ink::ALL.into_iter().find(|ink| ink.name() == name)
    }

    /// The style a cell in this ink is drawn in: the foreground, and nothing else, so
    /// that an ink names a colour and no caller can quietly add a second one.
    pub fn style(self) -> Style {
        Style::default().fg(self.colour())
    }

    /// The colour this ink spends. The five roles are the terminal's own colours and
    /// not RGB values, so a reader's theme still shows through them: a role is what the
    /// interface asks for, and which green `ok` is belongs to the terminal (§12).
    fn colour(self) -> Color {
        match self {
            Ink::Plain => Color::Reset,
            Ink::Dim => Color::DarkGray,
            Ink::Muted => Color::Gray,
            Ink::Accent => Color::Cyan,
            Ink::Ok => Color::Green,
            Ink::Err => Color::Red,
        }
    }
}

/// The face a run of glyphs is set in, on top of the ink it is drawn in: §12 asks a
/// heading to be bold and a run of inline code to be reversed, and neither is a colour
/// role, so a cell's look is an ink and a face rather than an ink alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Face {
    /// The terminal's own weight, which is every cell that says nothing else.
    #[default]
    Plain,
    /// What a heading is set in, so it reads as a heading before its words are read.
    Bold,
    /// What inline code is set in: the cell's own two colours swapped, so a run of code
    /// is marked out without a sixth colour role to name (§12).
    Reversed,
}

impl Face {
    /// Every face there is, in the order this enum names them, so a fixture's reader can
    /// look one up without a table of its own.
    const ALL: [Face; 3] = [Face::Plain, Face::Bold, Face::Reversed];

    /// The name a fixture writes down, and the name a ticket says out loud.
    pub fn name(self) -> &'static str {
        match self {
            Face::Plain => "plain",
            Face::Bold => "bold",
            Face::Reversed => "reversed",
        }
    }

    /// The face a fixture's name stands for, or nothing when no face is called that.
    pub fn from_name(name: &str) -> Option<Face> {
        Face::ALL.into_iter().find(|face| face.name() == name)
    }

    /// The modifier this face adds to a cell's style: the whole of what a face is, and
    /// empty for the terminal's own weight.
    fn modifier(self) -> Modifier {
        match self {
            Face::Plain => Modifier::empty(),
            Face::Bold => Modifier::BOLD,
            Face::Reversed => Modifier::REVERSED,
        }
    }
}

/// A cell's whole look: the [`Ink`] it is drawn in and the [`Face`] it is set in.
///
/// The pair is what a fixture writes down and what `Row::paint` spends, so a look is
/// asked for by name all the way down: `dim` is `dim`, and a bold heading's cell is
/// `plain+bold` — two names and never a colour or a modifier a caller reached for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    /// The colour role the cell is drawn in.
    ink: Ink,
    /// The weight or reversal it is set in.
    face: Face,
}

impl Look {
    /// A cell drawn in `ink` and set in `face`.
    pub const fn new(ink: Ink, face: Face) -> Look {
        Look { ink, face }
    }

    /// A cell drawn in `ink` and set in the terminal's own weight, which is every cell
    /// no construct of §12 says otherwise about.
    pub const fn plain(ink: Ink) -> Look {
        Look::new(ink, Face::Plain)
    }

    /// The ink this look is drawn in.
    pub fn ink(self) -> Ink {
        self.ink
    }

    /// The face this look is set in.
    pub fn face(self) -> Face {
        self.face
    }

    /// The style a cell in this look is drawn in: the ink's foreground and the face's
    /// modifier, and nothing else a caller could reach for.
    pub fn style(self) -> Style {
        self.ink.style().add_modifier(self.face.modifier())
    }

    /// The name a fixture writes down: the ink alone, `plain`, `dim`, `accent`, or the
    /// ink and the face `dim+bold` when the cell is set in something but plain weight.
    pub fn name(self) -> String {
        match self.face {
            Face::Plain => String::from(self.ink.name()),
            face => format!("{}+{}", self.ink.name(), face.name()),
        }
    }

    /// The look a fixture's name stands for, or nothing when no look is called that.
    pub fn from_name(name: &str) -> Option<Look> {
        let (ink, face) = match name.split_once('+') {
            Some((ink, face)) => (ink, Face::from_name(face)?),
            None => (name, Face::Plain),
        };
        Some(Look::new(Ink::from_name(ink)?, face))
    }

    /// The look a painted cell is in, or nothing when its style is not one look's: a
    /// fixture names looks, so a cell this cannot name is a frame it cannot write down.
    /// A cell is a foreground, a face and nothing else — no background, no underline
    /// colour, and no modifier that is not a face's.
    pub fn of(cell: &Cell) -> Option<Look> {
        if cell.bg != Color::Reset || cell.underline_color != Color::Reset {
            return None;
        }
        let ink = Ink::ALL.into_iter().find(|ink| ink.colour() == cell.fg)?;
        let face = Face::ALL
            .into_iter()
            .find(|face| face.modifier() == cell.modifier)?;
        Some(Look::new(ink, face))
    }
}

/// A look made of an ink alone, set in the terminal's own weight: what a row says when
/// its ink is the whole of what it has to say.
impl From<Ink> for Look {
    fn from(ink: Ink) -> Look {
        Look::plain(ink)
    }
}
