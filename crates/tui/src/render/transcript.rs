//! The transcript column — the hero. A conversation is laid out as the
//! document it is: two rows between turns and none inside one, a turn's body
//! hanging two columns off its own edge, and the prompt at full strength on a
//! band of its own (ADR-0016).
//!
//! Only the rows that fit are laid out, so an unbounded conversation still has
//! a bounded render tree (ADR-0011): the window is built from the newest row
//! backwards and stops as soon as it has a screenful, so scrolling costs what
//! was scrolled and nothing more.

use std::path::{Path, PathBuf};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::driver::{CallOver, Side};
use crate::entry::{Call, Entry, NoteKind, Prompt};
use crate::markdown;
use crate::theme;

/// The columns a turn's body hangs off the turn's own edge: the answer and the
/// calls it made start here, so a call reads as belonging to its turn.
const GUTTER: u16 = 2;
/// Two blank rows between turns, none inside one.
const TURN_GAP: usize = 2;
/// The columns a call's status takes before its command, so a running call and
/// the same call collapsed put the command in the same place.
const STATUS: usize = 8;

pub struct View {
    pub rows: Vec<Line<'static>>,
    pub plain: Vec<String>,
    pub laid_out: usize,
    pub spill: Vec<(usize, PathBuf)>,
}

pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    let view = window(app, area.width, area.height as usize);
    app.set_viewport(area, view.plain, view.spill.clone());
    Paragraph::new(view.rows).render(area, buf);
}

pub fn window(app: &mut App, width: u16, height: usize) -> View {
    let height = height.max(1);
    let width = width.max(1);
    let count = app.entries().len();
    app.note_width(width);

    let need = height.saturating_add(app.scroll());
    let mut acc = 0usize;
    let mut first = count;
    while first > 0 && acc < need {
        first -= 1;
        acc += app.measure(first, width) + gap(&app.entries()[first]);
    }

    let mut rows: Vec<Line<'static>> = Vec::new();
    let mut spill: Vec<(usize, PathBuf)> = Vec::new();
    for index in first..count {
        rows.extend((0..gap(&app.entries()[index])).map(|_| Line::default()));
        let rows_here = entry_rows(&app.entries()[index], width);
        if let Some(path) = spill_of(&app.entries()[index]) {
            spill.push((rows.len() + rows_here.len().saturating_sub(1), path));
        }
        rows.extend(rows_here);
    }

    let visible = height.min(rows.len());
    let scroll = app.scroll().min(rows.len().saturating_sub(visible));
    let end = rows.len() - scroll;
    let start = end.saturating_sub(height);
    let mut shown = rows.split_off(start);
    shown.truncate(height);

    let plain: Vec<String> = shown.iter().map(plain).collect();
    let laid_out = count.saturating_sub(first);
    app.settled(scroll, plain.len());
    let spill = spill
        .into_iter()
        .filter_map(|(row, path)| {
            row.checked_sub(start)
                .filter(|row| *row < shown.len())
                .map(|row| (row, path))
        })
        .collect();
    View {
        rows: shown,
        plain,
        laid_out,
        spill,
    }
}

/// The blank rows before an entry: two when a turn opens, and none inside one.
/// The first entry of the thread opens nothing, so it says nothing.
fn gap(entry: &Entry) -> usize {
    match entry {
        Entry::Prompt(_) => TURN_GAP,
        _ => 0,
    }
}

pub fn plain(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn spill_of(entry: &Entry) -> Option<PathBuf> {
    match entry {
        Entry::Call(call) => call.spill_file().map(Path::to_path_buf),
        _ => None,
    }
}

pub fn entry_rows(entry: &Entry, width: u16) -> Vec<Line<'static>> {
    let width = width.max(1);
    // The gutter's columns are not the body's: a call and its output wrap
    // where the column ends, not where the terminal does.
    let body = width.saturating_sub(GUTTER);
    match entry {
        Entry::Prompt(prompt) => prompt_rows(prompt, width),
        Entry::Answer { text, .. } => gutter(markdown::lines(text, body)),
        Entry::Reasoning { text, done, .. } => {
            if *done {
                return gutter(vec![Line::from(Span::styled("⌁ reasoned", theme::faint()))]);
            }
            let mut rows = markdown::lines(text, body);
            let keep = rows.len().saturating_sub(3);
            gutter(rows.split_off(keep))
        }
        Entry::Call(call) => gutter(call_rows(call, body)),
        Entry::Note(note) => {
            let (marker, style) = match note.kind {
                NoteKind::Error => ("! ", theme::bad()),
                NoteKind::Info => ("· ", theme::faint()),
            };
            // A note is the frame speaking, not the turn: it keeps the
            // turn's own edge. The marker takes its columns out of the row
            // before the words do, or the last word of a wrapped note is cut
            // in half.
            markdown::text::wrap(
                markdown::text::glyphs(&[Span::styled(note.text.clone(), style)]),
                marker,
                style,
                (width as usize).saturating_sub(marker.chars().count()),
            )
        }
    }
}

/// A turn's opening: the header that names the human, and the prompt itself at
/// full strength across a band of its own.
fn prompt_rows(prompt: &Prompt, width: u16) -> Vec<Line<'static>> {
    let band = width.saturating_sub(GUTTER);
    let mut rows = vec![Line::from(vec![
        Span::styled("YOU", theme::prompt()),
        Span::styled(format!("  {}  {}", prompt.at, prompt.cwd), theme::faint()),
    ])];
    let mut words = markdown::text::wrap(
        markdown::text::glyphs(&[Span::styled(prompt.text.clone(), Style::new())]),
        "",
        Style::new(),
        band as usize,
    );
    for (index, row) in words.iter_mut().enumerate() {
        // The marker column is fixed: a wrapped prompt keeps its own columns.
        let marker = if index == 0 { "› " } else { "  " };
        row.spans.insert(0, Span::styled(marker, theme::prompt()));
    }
    rows.extend(words.into_iter().map(|row| banded(row, width)));
    rows
}

/// A turn's body, two columns in from the turn's own edge.
fn gutter(rows: Vec<Line<'static>>) -> Vec<Line<'static>> {
    let pad = " ".repeat(GUTTER as usize);
    rows.into_iter()
        .map(|mut row| {
            row.spans.insert(0, Span::raw(pad.clone()));
            row
        })
        .collect()
}

/// One row of the prompt's band: the words as they are, the surface filled to
/// the column's edge so the prompt reads as a block and not as a line.
fn banded(row: Line<'static>, width: u16) -> Line<'static> {
    let used: usize = row
        .spans
        .iter()
        .map(|span| span.content.as_ref().width())
        .sum();
    let mut spans: Vec<Span<'static>> = row
        .spans
        .into_iter()
        .map(|span| {
            let style = span.style.bg(theme::PANEL);
            Span::styled(span.content, style)
        })
        .collect();
    spans.push(Span::styled(
        " ".repeat((width as usize).saturating_sub(used)),
        theme::surface(),
    ));
    Line::from(spans)
}

fn call_rows(call: &Call, width: u16) -> Vec<Line<'static>> {
    let command = call
        .command
        .as_deref()
        .unwrap_or("(the call could not be read)");
    // A marker column, then a status column of fixed width: a call that is
    // still running and the same call collapsed leave the command where it
    // was, and the glyph says GOOD or BAD without a word of hue alone
    // (ADR-0016). A refused call's reason is longer than the column and
    // pushes its own command right; a call that never ran has no earlier row
    // for that to disagree with.
    let (glyph, status, accent) = match &call.over {
        None => ("⏺", "running".to_string(), theme::faint()),
        Some(CallOver::Exited(0)) => ("✓", "exit 0".to_string(), theme::good()),
        Some(CallOver::Exited(code)) => ("✗", format!("exit {code}"), theme::bad()),
        Some(CallOver::TimedOut) => ("✗", "timed out".to_string(), theme::bad()),
        Some(CallOver::Cancelled) => ("✗", "cancelled".to_string(), theme::bad()),
        Some(CallOver::Refused(detail)) => ("✗", format!("sandbox: {detail}"), theme::bad()),
    };
    let mut rows = vec![Line::from(vec![
        Span::styled("⎿ ", theme::faint()),
        Span::styled(format!("{glyph} {status:<STATUS$} "), accent),
        Span::styled(command.to_string(), Style::new()),
    ])];
    if call.running() {
        // stdout and stderr differ by a glyph as well as by hue: a tail read
        // on a terminal that lost its colour still tells a warning from a
        // result (ADR-0016).
        for (side, line) in &call.tail {
            let (marker, style) = match side {
                Side::Stdout => ("│", theme::faint()),
                Side::Stderr => ("⚠", theme::bad()),
            };
            let keep = width.saturating_sub(4) as usize;
            let shown = markdown::text::clip(line, keep);
            rows.push(Line::from(vec![
                Span::styled(format!("  {marker} "), style),
                Span::styled(shown, style),
            ]));
        }
    }
    if let Some(path) = call.spill_file() {
        let keep = width.saturating_sub(2) as usize;
        let offer = format!("… output too large · Ctrl-O opens {}", path.display());
        rows.push(Line::from(Span::styled(
            markdown::text::clip(&offer, keep),
            theme::faint(),
        )));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::plain;
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};

    #[test]
    fn a_styled_row_copies_as_its_characters_with_no_style_in_them() {
        let line = Line::from(vec![
            Span::styled(
                "bold",
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" plain"),
        ]);
        assert_eq!(
            plain(&line),
            "bold plain",
            "a copy is the characters, never the styles around them"
        );
    }

    #[test]
    fn trailing_space_is_not_something_anyone_copies() {
        let line = Line::from("a  ");
        assert_eq!(plain(&line), "a");
    }
}
