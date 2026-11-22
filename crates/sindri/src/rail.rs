use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::graphics::CellSize;
use crate::ink::Ink;
use crate::ledger::{self, Facts, Rail, RailBand};
use crate::palette::Palette;
use crate::pen;
use crate::plane::Shade;
use crate::pulse::Pulse;

const HAIRLINE: char = '─';

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
    let shade = Shade::rail(rail, band, pictures, palette);
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

fn under_the_face(band: RailBand, buffer: &mut Buffer, palette: &Palette) {
    let Some(picture) = band.picture else {
        return;
    };
    let y = picture.y.saturating_add(picture.height);
    if y < band.area.bottom() {
        let opened = band.area.x.saturating_add(1);
        pen::rule(
            buffer,
            Rect {
                x: opened,
                width: band.area.right().saturating_sub(opened),
                y,
                height: 1,
            },
            HAIRLINE,
            Ink::Dim,
            palette,
        );
    }
}

#[cfg(test)]
mod tests;
