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
    terminal: (Option<CellSize>, Option<&mut crate::size::Sizes>),
    buffer: &mut Buffer,
    palette: &Palette,
) -> Option<Shade> {
    let (pictures, sized) = terminal;
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
    if let Some(sized) = sized {
        sized.append(sizes(facts, rail, band, marker, buffer, palette));
    }
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

fn sizes(
    facts: &Facts,
    rail: Rail,
    band: RailBand,
    marker: Option<&Pulse>,
    buffer: &mut Buffer,
    palette: &Palette,
) -> crate::size::Sizes {
    use crate::ink::Look;
    use crate::ledger::Part;
    use crate::size::{Sized, Sizes};

    let mut sizes = Sizes::none();
    if rail != Rail::Ledger {
        return sizes;
    }
    let rows = ledger::rows(facts, rail.into(), band.facts, marker);
    for row in &rows {
        for (x, part) in row.parts() {
            if let Part::Value(value) = part {
                if row
                    .parts()
                    .iter()
                    .any(|(_, part)| matches!(part, Part::Run(label, _) if label.trim() == "input"))
                {
                    let words = value.text().trim();
                    let split = words
                        .find(|glyph: char| glyph.is_ascii_alphabetic())
                        .unwrap_or(words.len());
                    let (figure, unit) = words.split_at(split);
                    let run = Sized::new(0, 2, figure, Look::plain(value.ink()));
                    let unit_run =
                        Sized::new(0, 2, unit, Look::plain(value.ink())).fractional(1, 4, 1, 0);
                    let left = band.facts.x + crate::rows::MARGIN + ledger::FIELD;
                    let y = band.facts.y + row.y();
                    let width = run.width() + unit_run.width();
                    let old = band.facts.x + crate::rows::MARGIN + x;
                    if !figure.is_empty()
                        && left + width <= old
                        && y + 1 < band.facts.bottom()
                        && (left..left + width).all(|x| {
                            buffer
                                .cell((x, y + 1))
                                .is_some_and(|cell| cell.symbol().trim().is_empty())
                        })
                    {
                        pen::run(
                            buffer,
                            old,
                            y,
                            &pen::blank(pen::cells(value.text())),
                            value.ink(),
                            palette,
                        );
                        sizes.lay(y, left, std::slice::from_ref(&run));
                        if !unit.is_empty() {
                            sizes.lay(y, left + run.width(), &[unit_run]);
                        }
                    }
                }
                if facts
                    .state
                    .as_ref()
                    .is_some_and(|state| value.text().contains(state.word()))
                    && row.y() == 0
                {
                    let run = Sized::new(0, 2, value.text(), Look::plain(value.ink()));
                    let left = band.facts.x + crate::rows::MARGIN + x;
                    let y = band.facts.y + row.y();
                    let right = band.facts.right().saturating_sub(crate::rows::MARGIN);
                    if left.saturating_add(run.width()) <= right
                        && y + 1 < band.facts.bottom()
                        && (left..left + run.width()).all(|x| {
                            buffer
                                .cell((x, y + 1))
                                .is_some_and(|cell| cell.symbol().trim().is_empty())
                        })
                    {
                        sizes.lay(y, left, &[run]);
                    }
                }
            }
            if let Part::Run(words, Ink::Dim) = part {
                let words = words.trim_end();
                if words.is_empty() {
                    continue;
                }
                let run = Sized::new(0, 1, words, Look::plain(Ink::Dim)).fractional(3, 4, 2, 0);
                sizes.lay(
                    band.facts.y + row.y(),
                    band.facts.x + crate::rows::MARGIN + x,
                    &[run],
                );
            }
        }
    }
    sizes.settle(band.facts, buffer, palette);
    sizes
}
