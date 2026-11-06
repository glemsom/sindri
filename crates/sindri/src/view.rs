//! What one frame holds and where each part of it goes: the transcript above, the
//! composer's strip with the hairline a draft is typed under, and the footer's hints
//! along the bottom edge (§12).
//!
//! `View::paint` is the one seam a frame is drawn through. The terminal calls it on
//! the frame the real screen is painted from, and a test calls it on a frame going to
//! ratatui's headless backend, so the picture a fixture holds and the picture a reader
//! sees are produced by the same code — the point of ADR-0012, and the reason a
//! visual decision can be graded by an agent at all.
//!
//! The transcript's rows are `rows`' to lay out and this file's to draw: a frame is
//! handed the entries a run has written and where the reader is scrolled to, and the
//! band paints the window of them the frame reaches, which is §12's rule that only
//! on-screen rows are laid out (ADR-0008). The rail's ledger and its 32 columns are
//! ticket #46's, the composer's key map and its two-to-six rows are #41's, the pulse is
//! #39's, and marking and copy are #50's. Each paints into the band below — or, in the
//! rail's case, into the split this file does not yet make — and changes its fixture
//! deliberately in the commit that changes it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::records::Entry;

use crate::ink::Ink;
use crate::rows::{Rows, Scroll};

/// The composer's rows at their floor. §12 gives it two to six, and a frame nobody has
/// typed into shows the two — the hairline a draft sits under and one row of it — so
/// the transcript above keeps the rows a taller box would spend on an empty draft.
const COMPOSER: u16 = 2;

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
/// of them — no run, no prompt, no transcript row, nothing loaded — so the frame is
/// its chrome and nothing else; a run hands over the entries it has written and where
/// the transcript is scrolled to, and this type is where the tickets after this one put
/// the facts they do have.
#[derive(Debug, Default, Clone)]
pub struct View {
    /// The transcript's rows, one entry at a time, at the width the band is drawn at.
    rows: Rows,
    /// Where the transcript's window sits: on the newest row, or held some rows above
    /// it while facts land below (#50's arrivals count from here).
    scroll: Scroll,
}

impl View {
    /// The frame a launch opens on: the screen has just been taken, no run has started
    /// and nothing has been typed, so the transcript is empty, the composer shows its
    /// hairline and the footer shows the hints that are never given up.
    pub fn boot() -> View {
        View::default()
    }

    /// The frame a run is in: the entries it has written, in order, and where the
    /// reader is standing in them. What a row of an entry looks like is `rows`'
    /// business, so a caller says what happened and never how wide or in which ink.
    pub fn of(entries: impl IntoIterator<Item = Entry>, scroll: Scroll) -> View {
        let mut rows = Rows::new();
        for entry in entries {
            rows.push(entry);
        }
        View { rows, scroll }
    }

    /// Paints this frame into `area`: every band of it, in the ink each band is drawn
    /// in, from the top of the screen to the bottom edge.
    ///
    /// Painting is what tells the row cache how wide the transcript is, so a resize
    /// invalidates it here and the same pass reflows: the width a row was laid out at is
    /// the width a band drew it at, and no caller gets to say otherwise (ADR-0008).
    pub fn paint(&mut self, area: Rect, buffer: &mut Buffer) {
        let bands = bands(area);
        self.transcript(bands.transcript, buffer);
        self.composer(bands.composer, buffer);
        self.footer(bands.footer, buffer);
    }

    /// The transcript: what the run has said so far, newest last, painted from the rows
    /// the cache holds for the window this band reaches and for no other entry. Where a
    /// reader scrolled back is told what arrived below by #50; the pulse at the head of
    /// the newest answer is #39's.
    fn transcript(&mut self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
        self.rows.resize(area.width);
        let window = self.rows.window(self.scroll, area.height);
        let mut y = area.y;
        for piece in window.pieces() {
            let rows = self.rows.rows(piece.entry);
            let first = usize::from(piece.skip);
            for row in &rows[first..first + usize::from(piece.rows)] {
                row.paint(
                    Rect {
                        height: 1,
                        y,
                        ..area
                    },
                    buffer,
                );
                y += 1;
            }
        }
    }

    /// The composer: the hairline, and the row of draft below it. `Enter` sending and
    /// `Ctrl-Q` quitting are #41's contract, and this paints the edge of the box they
    /// are typed into and nothing inside it.
    fn composer(&self, area: Rect, buffer: &mut Buffer) {
        let hairline = Rect { height: 1, ..area };
        for x in hairline.x..hairline.right() {
            if let Some(cell) = buffer.cell_mut((x, hairline.y)) {
                cell.set_char(HAIRLINE);
                cell.set_style(Ink::Dim.style());
            }
        }
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
/// the transcript takes what is left, because the footer and the composer hold a fixed
/// number of rows and the transcript is the band a run grows into (§12).
fn bands(area: Rect) -> Bands {
    let footer = area.height.min(1);
    let composer = COMPOSER.min(area.height - footer);
    let transcript = area.height - footer - composer;
    Bands {
        transcript: Rect {
            height: transcript,
            ..area
        },
        composer: Rect {
            y: area.y + transcript,
            height: composer,
            ..area
        },
        footer: Rect {
            y: area.y + transcript + composer,
            height: footer,
            ..area
        },
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
