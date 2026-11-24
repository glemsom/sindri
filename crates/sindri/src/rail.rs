use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::graphics::CellSize;
use crate::ink::Ink;
use crate::instrument;
use crate::ledger::{self, Big, Facts, Kind, Part, Rail, RailBand};
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
    use crate::size::Sizes;

    let mut sizes = Sizes::none();
    let left = band.facts.x + crate::rows::MARGIN;
    let right = band.facts.right().saturating_sub(crate::rows::MARGIN);
    for row in rows {
        let y = band.facts.y + row.y();
        for span in row.big().iter().cloned().chain(searched(row)) {
            if span.words.is_empty() {
                continue;
            }
            let run = run_of(&span);
            let from = left.saturating_add(span.at);
            if from.saturating_add(run.width()) > right {
                continue;
            }
            if run.scale() > 1 && y + 1 >= band.facts.bottom() {
                continue;
            }
            if span.big == Big::State && !untouched(buffer, from, y + 1, run.width()) {
                continue;
            }
            if matches!(span.big, Big::Figure | Big::Unit) {
                plain(row, buffer, palette, left, y);
            }
            sizes.lay(y, from, &[run]);
        }
    }
    sizes.settle(band.facts, buffer, palette);
    sizes
}

fn run_of(span: &ledger::Span) -> crate::size::Sized {
    use crate::ink::Look;
    use crate::size::Sized;

    let look = Look::plain(span.ink);
    match span.big {
        Big::Label => Sized::new(0, 1, &span.words, look).fractional(3, 4, 2, 0),
        Big::State | Big::Figure => Sized::new(0, 2, &span.words, look),
        Big::Unit => Sized::new(0, 2, &span.words, look).fractional(1, 4, 1, 0),
    }
}

// A bigger figure stands over the plain text of its value, so the whole of that text goes: a
// picture under a sized word shows through the glyph it covers.
fn plain(row: &ledger::Row, buffer: &mut Buffer, palette: &Palette, left: u16, y: u16) {
    for (x, part) in row.parts() {
        if let Part::Value(value) = part {
            pen::run(
                buffer,
                left.saturating_add(*x),
                y,
                &pen::blank(pen::cells(value.text())),
                value.ink(),
                palette,
            );
        }
    }
}

fn untouched(buffer: &Buffer, from: u16, y: u16, width: u16) -> bool {
    (from..from.saturating_add(width)).all(|column| {
        buffer
            .cell((column, y))
            .is_some_and(|cell| cell.symbol().trim().is_empty())
    })
}

// The spans here are read back off the value's text, the way the rail recovered them before a
// value stated its own; they stand only until a value says where its big text sits.
fn searched(row: &ledger::Row) -> Vec<ledger::Span> {
    use crate::ledger::Span;

    let Some((at, Part::Value(value))) = row
        .parts()
        .iter()
        .find(|(_, part)| matches!(part, Part::Value(_)))
    else {
        return Vec::new();
    };
    let at = *at;
    let text = value.text();
    match row.kind() {
        Some(Kind::Input) => {
            let words = text.trim();
            let split = words
                .find(|glyph: char| glyph.is_ascii_alphabetic())
                .unwrap_or(words.len());
            let (figure, unit) = words.split_at(split);
            if figure.is_empty() {
                return Vec::new();
            }
            let figure = Span {
                at,
                words: figure.to_owned(),
                ink: value.ink(),
                big: Big::Figure,
            };
            let past = at.saturating_add(run_of(&figure).width());
            let mut spans = vec![figure];
            if !unit.is_empty() {
                spans.push(Span {
                    at: past,
                    words: unit.to_owned(),
                    ink: value.ink(),
                    big: Big::Unit,
                });
            }
            spans
        }
        Some(Kind::State) => {
            let after = text.find(' ').map_or(0, |space| space + 1);
            vec![Span {
                at: at.saturating_add(pen::cells(&text[..after])),
                words: text[after..].to_owned(),
                ink: value.ink(),
                big: Big::State,
            }]
        }
        _ => Vec::new(),
    }
}
