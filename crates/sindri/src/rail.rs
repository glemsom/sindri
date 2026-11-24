use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::graphics::CellSize;
use crate::ink::Ink;
use crate::instrument;
use crate::ledger::{self, Facts, Kind, Rail, RailBand};
use crate::palette::Palette;
use crate::pen;
use crate::plane::{Bar, Placement};
use crate::pulse::Pulse;
use crate::rows::margined;

const HAIRLINE: char = '─';

pub fn paint(
    facts: &Facts,
    rail: Rail,
    band: RailBand,
    marker: Option<&Pulse>,
    terminal: (Option<CellSize>, Option<&mut crate::size::Sizes>),
    buffer: &mut Buffer,
    palette: &Palette,
) -> Vec<Placement> {
    let (pictures, sized) = terminal;
    let area = band.area;
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }
    let mut placements = Vec::new();
    if let Some(placement) = Placement::rail(rail, band, pictures, palette) {
        placement.claim(buffer, palette);
        placements.push(placement);
    }
    if rail == Rail::Ledger {
        under_the_face(band, buffer, palette);
    }
    let rows = ledger::rows(facts, rail.into(), band.facts, marker);
    ledger::draw(&rows, band.facts, buffer, palette);
    placements.extend(instruments(facts, rail, band, pictures, &rows, palette));
    if rail == Rail::Ledger
        && let Some(sized) = sized
    {
        sized.append(sizes(&rows, band, buffer, palette));
    }
    placements
}

fn instruments(
    facts: &Facts,
    rail: Rail,
    band: RailBand,
    cell: Option<CellSize>,
    rows: &[ledger::Row],
    palette: &Palette,
) -> Vec<Placement> {
    let Some(cell) = cell.filter(|_| rail == Rail::Ledger) else {
        return Vec::new();
    };
    let columns = margined(band.facts);
    let cells = |at: Range<u16>| Rect {
        x: columns.x,
        y: band.facts.y.saturating_add(at.start),
        width: columns.width,
        height: at.end - at.start,
    };
    let stated = |kind: Kind| {
        rows.iter()
            .find(|row| row.kind() == Some(kind))
            .map(|row| cells(row.y()..row.y().saturating_add(1)))
    };
    let mut placements = Vec::new();
    if let (Some(area), Some(gauge)) = (stated(Kind::Ctx), instrument::Gauge::of(facts)) {
        placements.extend(Placement::gauge(area, Some(cell), gauge, palette));
    }
    let spend = instrument::Spend::of(&facts.spend);
    for (kind, bar, share) in [
        (Kind::Input, Bar::Input, spend.and_then(|spend| spend.input)),
        (
            Kind::Output,
            Bar::Output,
            spend.and_then(|spend| spend.output),
        ),
    ] {
        if let (Some(area), Some(share)) = (stated(kind), share) {
            placements.extend(Placement::bar(area, Some(cell), bar, share, palette));
        }
    }
    if let Some(room) = instrument::bezel(rows) {
        placements.extend(Placement::bezel(cells(room), Some(cell), palette));
    }
    placements
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
    rows: &[ledger::Row],
    band: RailBand,
    buffer: &mut Buffer,
    palette: &Palette,
) -> crate::size::Sizes {
    use crate::ink::Look;
    use crate::ledger::Part;
    use crate::size::{Sized, Sizes};

    let mut sizes = Sizes::none();
    let left = band.facts.x + crate::rows::MARGIN;
    let right = band.facts.right().saturating_sub(crate::rows::MARGIN);
    for row in rows {
        let y = band.facts.y + row.y();
        for (x, part) in row.parts() {
            if let Part::Value(value) = part {
                if row.kind() == Some(Kind::Input) {
                    let words = value.text().trim();
                    let split = words
                        .find(|glyph: char| glyph.is_ascii_alphabetic())
                        .unwrap_or(words.len());
                    let (figure, unit) = words.split_at(split);
                    let run = Sized::new(0, 2, figure, Look::plain(value.ink()));
                    let unit_run =
                        Sized::new(0, 2, unit, Look::plain(value.ink())).fractional(1, 4, 1, 0);
                    let from = left + x;
                    let width = run.width() + unit_run.width();
                    // A picture is two cells deep, so the foot's first figure stands over the row
                    // its next figure is stated on.
                    if !figure.is_empty()
                        && from.saturating_add(width) <= right
                        && y + 1 < band.facts.bottom()
                    {
                        pen::run(
                            buffer,
                            from,
                            y,
                            &pen::blank(pen::cells(value.text())),
                            value.ink(),
                            palette,
                        );
                        sizes.lay(y, from, std::slice::from_ref(&run));
                        if !unit.is_empty() {
                            sizes.lay(y, from + run.width(), &[unit_run]);
                        }
                    }
                }
                if row.kind() == Some(Kind::State) {
                    let text = value.text();
                    let after = text.find(' ').map_or(0, |space| space + 1);
                    let run = Sized::new(0, 2, &text[after..], Look::plain(value.ink()));
                    let from = left + x + pen::cells(&text[..after]);
                    if from.saturating_add(run.width()) <= right
                        && y + 1 < band.facts.bottom()
                        && (from..from + run.width()).all(|column| {
                            buffer
                                .cell((column, y + 1))
                                .is_some_and(|cell| cell.symbol().trim().is_empty())
                        })
                    {
                        sizes.lay(y, from, &[run]);
                    }
                }
            }
            if let Part::Run(words, Ink::Dim) = part {
                let words = words.trim_end();
                if words.is_empty() {
                    continue;
                }
                let run = Sized::new(0, 1, words, Look::plain(Ink::Dim)).fractional(3, 4, 2, 0);
                sizes.lay(y, left + x, &[run]);
            }
        }
    }
    sizes.settle(band.facts, buffer, palette);
    sizes
}
