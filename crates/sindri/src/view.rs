//! What one frame holds and where each part of it goes: the transcript above, the
//! composer's band — the hairline a draft is typed under and the draft itself — and
//! the footer's hints along the bottom edge (§12).
//!
//! `View::paint` is the one seam a frame is drawn through. The terminal calls it on
//! the frame the real screen is painted from, and a test calls it on a frame going to
//! ratatui's headless backend, so the picture a fixture holds and the picture a reader
//! sees are produced by the same code — the point of ADR-0012, and the reason a
//! visual decision can be graded by an agent at all.
//!
//! What is here is the frame and, since #41, the composer's band painted from the
//! draft it is handed: the rows the box takes are the `Composer`'s to decide, and this
//! file asks rather than counts. The transcript's rows are the core's to lay out
//! (§12, ADR-0008), the rail's ledger and its 32 columns are ticket #46's, and the
//! pulse is #39's. Each of them paints into the band below — or, in the rail's case,
//! into the split this file does not yet make — and changes its fixture deliberately
//! in the commit that changes it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::composer::{Composer, HAIRLINE_ROWS, Key, Outcome, State};
use crate::ink::Ink;

/// The hints §12 never takes away, whichever way the frame narrows: the key that sends
/// a prompt and the key that says which keys there are.
const FLOOR_HINTS: [(&str, Ink); 3] = [
    ("Enter send", Ink::Muted),
    (" · ", Ink::Dim),
    ("? keys", Ink::Muted),
];

/// The hairline a draft is typed under: one glyph, so the composer's top edge is a row
/// of them, drawn in `dim` because it is a fact about the frame rather than a thing said.
const HAIRLINE: char = '─';

/// One frame's worth of facts, in the shape the terminal draws them. Boot knows none
/// of them but the draft's, which is empty — no run, no transcript row, nothing loaded
/// — so the frame is its chrome and nothing else, and this type is where the tickets
/// after this one put the facts they do have.
#[derive(Debug, Default, Clone)]
pub struct View {
    /// The draft a prompt is typed into: the composer's band is painted from this
    /// value and from nothing else (#41).
    composer: Composer,
}

impl View {
    /// The frame a launch opens on: the screen has just been taken, no run has started
    /// and nothing has been typed, so the transcript is empty, the composer shows its
    /// hairline over one empty row and the footer shows the hints never given up.
    pub fn boot() -> View {
        View::default()
    }

    /// The frame a draft somebody typed is painted on. A fixture builds one this way,
    /// so the accepted picture of a six-row composer is made through the same value a
    /// reader's six rows are.
    pub fn of(composer: Composer) -> View {
        View { composer }
    }

    /// The draft this frame is drawn around.
    pub fn composer(&self) -> &Composer {
        &self.composer
    }

    /// Hands the composer one key and says what the terminal is to do about it: paint
    /// the band again, send the draft, quit, or leave a run cancelled and the draft
    /// where it was. The composer decides; the terminal acts (ADR-0005).
    pub fn key(&mut self, key: Key) -> Outcome {
        self.composer.key(key)
    }

    /// Hands the composer a pasted prompt: one draft, its line breaks kept, and no
    /// path back to something a run could take.
    pub fn paste(&mut self, text: &str) {
        self.composer.paste(text);
    }

    /// Says whether a run is live, which is the one fact `Ctrl-C`'s rule reads: the run
    /// loop says so when it starts and when it stops (#43), and nothing else may.
    pub fn set_state(&mut self, state: State) {
        self.composer.set_state(state);
    }

    /// Paints this frame into `area`: every band of it, in the ink each band is drawn
    /// in, from the top of the screen to the bottom edge.
    pub fn paint(&self, area: Rect, buffer: &mut Buffer) {
        let bands = bands(area, self.composer.rows());
        self.transcript(bands.transcript, buffer);
        self.composer_band(bands.composer, buffer);
        self.footer(bands.footer, buffer);
    }

    /// The transcript: what the run has said so far, newest last, and where a reader
    /// scrolled back is told what arrived below. A row of it is the core's to lay out
    /// on the width it is drawn at (ADR-0008), so this file paints the band's ink and
    /// leaves its rows to the ticket that has them: at boot there is nothing to say.
    fn transcript(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
    }

    /// The composer: the hairline across the band's top row and the draft's lines
    /// below it — the rows the box takes, from the line it has scrolled to. Where the
    /// cursor sits inside a line is not painted here but handed to the terminal (see
    /// `View::cursor`), because a cell of a frame is a glyph in an ink and a caret is
    /// neither.
    fn composer_band(&self, area: Rect, buffer: &mut Buffer) {
        let hairline = Rect { height: 1, ..area };
        for x in hairline.x..hairline.right() {
            if let Some(cell) = buffer.cell_mut((x, hairline.y)) {
                cell.set_char(HAIRLINE);
                cell.set_style(Ink::Dim.style());
            }
        }

        let draft = Rect {
            y: area.y.saturating_add(HAIRLINE_ROWS),
            height: area.height.saturating_sub(HAIRLINE_ROWS),
            ..area
        };
        for (row, line) in self.composer.lines().iter().enumerate() {
            let Some(offset) = u16::try_from(row).ok() else {
                break;
            };
            let Some(y) = draft.y.checked_add(offset) else {
                break;
            };
            if y >= draft.bottom() {
                break;
            }
            draft_line(buffer, Rect { y, ..draft }, line);
        }
    }

    /// Where the terminal's own cursor belongs, so a reader sees where typing lands:
    /// the caret's cell on its row of the draft, and nothing when the caret has run
    /// past the band's edge.
    ///
    /// The caret is the terminal's cursor and never a cell of the frame. An ink names a
    /// foreground and nothing else (ADR-0012), so a caret drawn as a cell would be a
    /// letter tinted `accent` — invisible on the space it usually sits on — and a
    /// reverse-video block cannot be written into a fixture at all.
    pub fn cursor(&self, area: Rect) -> Option<(u16, u16)> {
        let bands = bands(area, self.composer.rows());
        let (row, column) = self.composer.caret();
        let y = bands
            .composer
            .y
            .checked_add(HAIRLINE_ROWS)?
            .checked_add(row)?;
        let x = area.x.checked_add(column)?;
        (column < area.width && y < bands.composer.bottom()).then_some((x, y))
    }

    /// The footer: the hints §12 keeps at every width, on the bottom edge. The five
    /// more a 120-column frame shows are the chrome's ticket; these three spans are the
    /// floor it degrades to, so the golden frame holds what no width can take away.
    fn footer(&self, area: Rect, buffer: &mut Buffer) {
        write(buffer, area, &FLOOR_HINTS);
    }
}

/// The bands a frame is cut into, top to bottom.
struct Bands {
    /// Everything above the composer: the transcript's rows, and the columns the rail
    /// will take from them (ticket #46 owns that split).
    transcript: Rect,
    /// The composer's rows of draft, with the hairline on the first of them.
    composer: Rect,
    /// The footer's single row of hints, on the bottom edge.
    footer: Rect,
}

/// Cuts `area` into its three bands. The chrome is measured up from the bottom edge and
/// the transcript takes what is left, because the footer holds one row and the composer
/// as many as the draft asks for (two to six), while the transcript is the band a run
/// grows into (§12).
fn bands(area: Rect, composer_rows: u16) -> Bands {
    let footer = area.height.min(1);
    let composer = composer_rows.min(area.height.saturating_sub(footer));
    let transcript = area.height.saturating_sub(footer).saturating_sub(composer);
    Bands {
        transcript: Rect {
            height: transcript,
            ..area
        },
        composer: Rect {
            y: area.y.saturating_add(transcript),
            height: composer,
            ..area
        },
        footer: Rect {
            y: area.y.saturating_add(transcript).saturating_add(composer),
            height: footer,
            ..area
        },
    }
}

/// Writes one draft line across a row from the band's left edge, cut at its right
/// edge: a line wider than the frame is prose that runs on, not a hint that has to be
/// whole — §12's "whole or it is not drawn" is a rule about chrome — so it is cut
/// rather than dropped.
fn draft_line(buffer: &mut Buffer, area: Rect, line: &str) {
    for (x, glyph) in (area.x..area.right()).zip(line.chars()) {
        if let Some(cell) = buffer.cell_mut((x, area.y)) {
            cell.set_char(glyph);
            cell.set_style(Ink::Plain.style());
        }
    }
}

/// Writes spans of text across a row from its left edge, each in its own ink, and never
/// half of one: §12 gives a hint up whole or not at all, so a span that would not fit in
/// what is left of the row is not drawn rather than cut.
fn write(buffer: &mut Buffer, area: Rect, spans: &[(&str, Ink)]) {
    let mut x = area.x;
    for (text, ink) in spans {
        let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
        if x.saturating_add(width) > area.right() {
            return;
        }
        for glyph in text.chars() {
            if let Some(cell) = buffer.cell_mut((x, area.y)) {
                cell.set_char(glyph);
                cell.set_style(ink.style());
            }
            x += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    //! The one thing a golden frame cannot hold: the cell the terminal's own cursor is
    //! put on. A fixture is a buffer of glyphs and inks (ADR-0012) and a caret is
    //! neither, so where the caret goes is proved here and painted by the terminal.

    use super::*;

    /// A screen `cols` by `rows`, at the origin: the area a frame is painted into.
    fn screen(cols: u16, rows: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: cols,
            height: rows,
        }
    }

    #[test]
    fn the_terminal_cursor_lands_on_the_carets_row_of_the_draft() {
        let composer = {
            let mut composer = Composer::new();
            composer.paste("first line\nsecond line");
            composer
        };
        let view = View::of(composer);
        let area = screen(40, 12);

        // Twelve rows: the footer takes the last, and the three-row band — a hairline
        // and two lines of draft — sits on rows 8 to 10. The caret is eleven glyphs
        // into the second line.
        assert_eq!(view.composer().rows(), 3);
        assert_eq!(view.cursor(area), Some((11, 10)));
    }

    #[test]
    fn a_caret_run_past_the_bands_edge_has_no_cell_to_wait_on() {
        let view = View::of({
            let mut composer = Composer::new();
            composer.paste(&"x".repeat(60));
            composer
        });

        assert_eq!(
            view.cursor(screen(40, 12)),
            None,
            "column 60 is off a 40-column frame"
        );
        assert_eq!(view.cursor(screen(80, 12)), Some((60, 10)));
    }
}
