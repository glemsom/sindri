//! The ink a cell is drawn in, asked for by name: a colour role the interface spends
//! instead of a colour it names, so a palette change cannot break a frame fixture and
//! a fixture cannot smuggle in a theme (§12, ADR-0012).
//!
//! The glossary's five inks — `dim`, `muted`, `accent`, `ok`, `err` — are joined by
//! `plain`, which is no colour at all: a cell the terminal draws in whatever ink its
//! reader chose. An ink is the whole of a cell's look, foreground and nothing else,
//! and `Ink::of` reads a painted cell back into the name it is in. That reading is
//! how a fixture is written down as roles rather than as a palette, and it is why a
//! cell painted a colour, a weight or a background that is not one of these cannot be
//! written down at all: the fixture would have to name something no ink does.

use ratatui::buffer::Cell;
use ratatui::style::{Color, Style};

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
    /// that an ink names a whole cell's look and no caller can quietly add one.
    pub fn style(self) -> Style {
        Style::default().fg(self.colour())
    }

    /// The ink a painted cell is in, or nothing when its style is not one ink's: a
    /// fixture names inks, so a cell this cannot name is a frame it cannot write down.
    pub fn of(cell: &Cell) -> Option<Ink> {
        let bare = cell.bg == Color::Reset
            && cell.underline_color == Color::Reset
            && cell.modifier.is_empty();
        if !bare {
            return None;
        }
        Ink::ALL.into_iter().find(|ink| ink.colour() == cell.fg)
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
