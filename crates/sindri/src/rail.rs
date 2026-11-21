//! The ledger's adapter: the band it is handed, and the rows the ledger lays on it drawn there.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::graphics::CellSize;
use crate::ink::Ink;
use crate::ledger::{self, Facts, Rail, RailBand};
use crate::palette::Palette;
use crate::pen;
use crate::plane::Shade;
use crate::pulse::Pulse;

/// The one rule the rail draws in its body: the face is closed by a line, and the blocks under it
/// are parted by air rather than by a rule each.
const HAIRLINE: char = '─';

/// The band the rail draws, handed the cell a picture is measured at where this terminal places
/// one: the shading the band stands on is worked out from it and handed back with the band.
pub fn paint(
    facts: &Facts,
    rail: Rail,
    band: RailBand,
    marker: Option<&Pulse>,
    pictures: Option<CellSize>,
    buffer: &mut Buffer,
    palette: &Palette,
) -> Option<Shade> {
    let area = band.area;
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let shade = Shade::rail(rail, area, pictures, palette);
    if let Some(shade) = shade {
        shade.claim(buffer, palette);
    }
    if rail == Rail::Ledger {
        under_the_face(band, buffer, palette);
    }
    ledger::draw(
        &ledger::rows(facts, rail.into(), band.facts, marker),
        band.facts,
        buffer,
        palette,
    );
    shade
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

#[cfg(test)]
mod tests;
