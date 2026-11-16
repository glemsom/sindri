//! The ledger's adapter: the band it is handed, and the rows the ledger lays on it drawn there.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Ink;
use crate::ledger::{self, Facts, Rail, RailBand};
use crate::palette::{Band, Edge, Palette};
use crate::pen;
use crate::pulse::Pulse;

/// The one rule the rail draws in its body: the face is closed by a line, and the blocks under it
/// are parted by air rather than by a rule each.
const HAIRLINE: char = '─';

pub fn paint(
    facts: &Facts,
    rail: Rail,
    band: RailBand,
    marker: Option<&Pulse>,
    buffer: &mut Buffer,
    palette: &Palette,
) {
    let area = band.area;
    if area.width == 0 || area.height == 0 {
        return;
    }
    pen::claim(buffer, area, palette.ground(Some(Band::Rail)));
    if rail == Rail::Ledger {
        elevation(buffer, area, palette);
        under_the_face(band, buffer, palette);
    }
    ledger::draw(
        &ledger::rows(facts, rail.into(), band.facts, marker),
        band.facts,
        buffer,
        palette,
    );
}

/// The rule that closes a face drawn over the rail's top rows, and nothing when the band holds no
/// picture or no row under one.
fn under_the_face(band: RailBand, buffer: &mut Buffer, palette: &Palette) {
    let Some(picture) = band.picture else {
        return;
    };
    let y = picture.y.saturating_add(picture.height);
    if y < band.area.bottom() {
        pen::rule(
            buffer,
            Rect {
                y,
                height: 1,
                ..band.area
            },
            HAIRLINE,
            Ink::Dim,
            palette,
        );
    }
}

/// The two ends a ledger's band carries light at: the rule down the column it meets the
/// transcript on, which is the rail's own column and never one the transcript gave up, and the
/// shadow under its last row.
fn elevation(buffer: &mut Buffer, area: Rect, palette: &Palette) {
    pen::claim(buffer, Rect { width: 1, ..area }, palette.edged(Edge::Lit));
    pen::claim(
        buffer,
        Rect {
            y: area.bottom() - 1,
            height: 1,
            ..area
        },
        palette.edged(Edge::Shadow),
    );
}

#[cfg(test)]
mod tests;
