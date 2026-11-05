//! The `/model` screen: a screenful of the names the wire this run speaks
//! offered, the one the cursor is on marked, and — while a fragment is typed —
//! the names that fragment reaches, in the ranking the model row states them in.
//! The rows the names would fill say why there are none where the names would
//! be: that the ask is still out, that the wire could not be asked, or that no
//! name reaches the fragment. The keys are the last row and never the row that
//! falls off, because `Enter` is the one key that writes.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget};

use crate::markdown::text::clip;
use crate::model::Model;
use crate::render::overlay::{self, More};
use crate::settings::Served;
use crate::theme;

const MARGIN: u16 = 4;
/// The narrowest a screen still reads at, and the widest: past the wide one a
/// name sits in columns nothing needs.
const NARROW: u16 = 30;
const WIDE: u16 = 110;

const HINTS: &str =
    "↑↓ move · type to filter · PgUp/PgDn page · Home/End first/last · Enter set · Esc close";

/// Draws the screen, and answers with the rows of names it showed, which is what
/// a page moves the cursor by.
pub fn draw(model: &Model, served: &Served, area: Rect, buf: &mut Buffer) -> usize {
    let width = area
        .width
        .saturating_sub(MARGIN * 2)
        .clamp(NARROW, WIDE)
        .min(area.width);
    let columns = width.saturating_sub(2) as usize;
    let boxed = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y,
        width,
        // A screenful: a choice as long as the wire's list is worth, and never
        // more room than the terminal has to give it.
        height: area.height,
    };

    // The fragment typed, where there is one, and the keys: pinned, because the
    // row a reader is typing into is never the row that scrolled away.
    let mut foot: Vec<Line<'static>> = Vec::new();
    if let Some(error) = model.error() {
        foot.push(Line::from(Span::styled(
            clip(&format!("not set · {error}"), columns),
            theme::bad(),
        )));
    }
    if !model.filter().is_empty() {
        foot.push(Line::from(Span::styled(
            clip(&format!("filter · {}", model.filter()), columns),
            theme::plain(),
        )));
    }
    foot.push(Line::from(Span::styled(
        clip(HINTS, columns),
        theme::faint(),
    )));

    let rows = boxed.height.saturating_sub(2) as usize;
    let room = rows.saturating_sub(foot.len());
    let names = model.list().names();
    let (from, to) = window(names.len(), model.list().chosen(), room);
    let mut lines: Vec<Line<'static>> = Vec::new();
    match names.is_empty() {
        true => lines.extend(empty(model, served, columns)),
        false => lines.extend(name_lines(model, columns, from, to)),
    }
    lines.truncate(rows);
    lines.extend(foot);

    let block = overlay::paged(
        &title(served),
        More {
            above: from > 0,
            below: to < names.len(),
        },
    );
    Clear.render(boxed, buf);
    let inner = block.inner(boxed);
    Paragraph::new(lines)
        .style(theme::raised())
        .render(inner, buf);
    block.render(boxed, buf);
    room
}

/// The box says which wire's names these are, because a listing is one wire at
/// one address and a name read at another says nothing.
fn title(served: &Served) -> String {
    match served.serving() {
        Some(serving) => format!("model · {}", serving.wire()),
        None => "model".to_string(),
    }
}

/// The window of the names the box has room for, standing around the one the
/// cursor is on: the name a keystroke stepped to is the name that is drawn.
fn window(names: usize, chosen: usize, room: usize) -> (usize, usize) {
    if room == 0 || names == 0 {
        return (0, 0);
    }
    let mut from = chosen.saturating_add(1).saturating_sub(room);
    let to = (from + room).min(names);
    if to == names {
        from = to.saturating_sub(room);
    }
    (from, to.max(from))
}

fn name_lines(model: &Model, columns: usize, from: usize, to: usize) -> Vec<Line<'static>> {
    let list = model.list();
    list.names()[from.min(to)..to]
        .iter()
        .enumerate()
        .map(|(offset, name)| {
            let marked = from + offset == list.chosen();
            let said = match marked {
                true => format!("▸ {name}"),
                false => format!("  {name}"),
            };
            let line = Line::from(Span::styled(clip(&said, columns), theme::plain()));
            match marked {
                true => line.patch_style(Style::new().add_modifier(Modifier::REVERSED)),
                false => line,
            }
        })
        .collect()
}

/// What the rows the names would fill say instead, in the order the screen can
/// know it: an ask that is out, a fragment nothing reaches, and a wire that could
/// not be asked — which is a hint and never a refusal, and says what sets a name
/// anyway.
fn empty(model: &Model, served: &Served, columns: usize) -> Vec<Line<'static>> {
    let wire = match served.serving() {
        Some(serving) => serving.wire(),
        None => "the wire",
    };
    let said = match (served.asking(), model.filter().is_empty()) {
        (true, _) => format!("asking {wire} for its names — this screen takes every key meanwhile"),
        (false, false) => format!("no name reaches {}", model.filter()),
        (false, true) => match served.reason() {
            Some(why) => {
                format!("{wire} could not be asked for its names ({why}) · /model <name> sets one")
            }
            None => format!("{wire} could not be asked for its names · /model <name> sets one"),
        },
    };
    vec![Line::from(Span::styled(
        clip(&said, columns),
        theme::faint(),
    ))]
}
