//! The ink a cell is drawn in, asked for by name: a colour role the interface spends
//! instead of a colour it names, so a palette change cannot break a frame fixture and
//! a fixture cannot smuggle in a theme (§12, ADR-0012).
//!
//! The glossary's five inks — `dim`, `muted`, `accent`, `ok`, `err` — are joined by
//! `plain`, which is no colour at all: a cell the terminal draws in whatever ink its
//! reader chose, and by `mark`, which is the reader's own selection and so the one ink
//! that spends a modifier instead of a colour. An ink is the whole of a cell's look,
//! and `Ink::of` reads a painted cell back into the name it is in. That reading is
//! how a fixture is written down as roles rather than as a palette, and it is why a
//! cell painted a colour, a weight or a background that is not one of these cannot be
//! written down at all: the fixture would have to name something no ink does.

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
    /// A cell the reader's mark covers. The one ink that spends no colour: a selection is
    /// the terminal's own reverse video, so a mark reads the same in every theme and no
    /// palette change can move it (§12, #50).
    Mark,
}

impl Ink {
    /// Every ink there is, in the order this enum names them, so a reader of a fixture
    /// can look one up without a table of its own.
    pub const ALL: [Ink; 7] = [
        Ink::Plain,
        Ink::Dim,
        Ink::Muted,
        Ink::Accent,
        Ink::Ok,
        Ink::Err,
        Ink::Mark,
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
            Ink::Mark => "mark",
        }
    }

    /// The ink a fixture's name stands for, or nothing when no ink is called that.
    pub fn from_name(name: &str) -> Option<Ink> {
        Ink::ALL.into_iter().find(|ink| ink.name() == name)
    }

    /// The style a cell in this ink is drawn in: the whole of a cell's look, so that an
    /// ink names one and no caller can quietly add to it.
    pub fn style(self) -> Style {
        let (foreground, background, modifier) = self.look();
        Style::default()
            .fg(foreground)
            .bg(background)
            .add_modifier(modifier)
    }

    /// The ink a painted cell is in, or nothing when its look is not one ink's: a fixture
    /// names inks, so a cell this cannot name is a frame it cannot write down.
    pub fn of(cell: &Cell) -> Option<Ink> {
        if cell.underline_color != Color::Reset {
            return None;
        }
        let look = (cell.fg, cell.bg, cell.modifier);
        Ink::ALL.into_iter().find(|ink| ink.look() == look)
    }

    /// The whole look this ink stands for, as a foreground, a background and a modifier:
    /// what a cell drawn in it holds, and what `Ink::of` reads a cell back by.
    ///
    /// The six that spend a colour are the terminal's own and not RGB values, so a reader's
    /// theme still shows through them: a role is what the interface asks for, and which
    /// green `ok` is belongs to the terminal (§12). `mark` asks for no colour at all — the
    /// terminal's reverse video is the look every terminal already gives a selection.
    fn look(self) -> (Color, Color, Modifier) {
        let look = |colour| (colour, Color::Reset, Modifier::empty());
        match self {
            Ink::Plain => look(Color::Reset),
            Ink::Dim => look(Color::DarkGray),
            Ink::Muted => look(Color::Gray),
            Ink::Accent => look(Color::Cyan),
            Ink::Ok => look(Color::Green),
            Ink::Err => look(Color::Red),
            Ink::Mark => (Color::Reset, Color::Reset, Modifier::REVERSED),
        }
    }
}

#[cfg(test)]
mod tests {
    //! What a painted cell reads back as, which is how a fixture writes a frame down: an
    //! ink is the whole look of a cell, foreground and the one modifier a mark spends.

    use super::*;

    #[test]
    fn every_ink_reads_back_as_the_name_it_is_drawn_in() {
        for ink in Ink::ALL {
            let mut cell = Cell::default();
            cell.set_style(ink.style());

            assert_eq!(
                Ink::of(&cell),
                Some(ink),
                "a cell drawn in {} is that ink, and a fixture has to be able to say so",
                ink.name()
            );
            assert_eq!(Ink::from_name(ink.name()), Some(ink));
        }
    }

    #[test]
    fn a_cell_a_mark_covers_reads_back_as_the_mark_and_no_other_ink() {
        let mut cell = Cell::default();
        cell.set_style(Ink::Mark.style());

        assert_eq!(
            Ink::of(&cell),
            Some(Ink::Mark),
            "the marked cell is in `mark`, which is a name a reader can look up"
        );
    }
}
