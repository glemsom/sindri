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
use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::entry::{Entry, NoteKind, Prompt};
use crate::markdown;
use crate::pulse;
use crate::render::call;
use crate::theme;

/// The columns a turn's body hangs off the turn's own edge: the answer and the
/// calls it made start here, so a call reads as belonging to its turn.
const GUTTER: u16 = 2;
/// Two blank rows between turns, none inside one.
const TURN_GAP: usize = 2;

pub struct View {
    pub rows: Vec<Line<'static>>,
    pub plain: Vec<String>,
    pub laid_out: usize,
    pub spill: Vec<(usize, PathBuf)>,
    /// The drawn rows that are an expandable entry's own row, and the entry
    /// behind each: a click there opens it (ADR-0011 §9, issue #44).
    pub expand: Vec<(usize, usize)>,
}

pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    let view = window(app, area.width, area.height as usize);
    app.set_viewport(area, view.plain, view.spill.clone(), view.expand.clone());
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
    let mut expand: Vec<(usize, usize)> = Vec::new();
    for index in first..count {
        rows.extend((0..gap(&app.entries()[index])).map(|_| Line::default()));
        let rows_here = entry_rows(&app.entries()[index], width, app.now());
        if let Some(path) = spill_of(&app.entries()[index]) {
            spill.push((rows.len() + rows_here.len().saturating_sub(1), path));
        }
        if app.entries()[index].expandable() {
            expand.push((rows.len(), index));
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
    let expand = expand
        .into_iter()
        .filter_map(|(row, entry)| {
            row.checked_sub(start)
                .filter(|row| *row < shown.len())
                .map(|row| (row, entry))
        })
        .collect();
    View {
        rows: shown,
        plain,
        laid_out,
        spill,
        expand,
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

pub fn entry_rows(entry: &Entry, width: u16, now: Duration) -> Vec<Line<'static>> {
    let width = width.max(1);
    // The gutter's columns are not the body's: a call and its output wrap
    // where the column ends, not where the terminal does.
    let body = width.saturating_sub(GUTTER);
    match entry {
        Entry::Prompt(prompt) => prompt_rows(prompt, width),
        Entry::Answer {
            text,
            done,
            failed,
            took,
            ..
        } => answer_rows(text, *done, *failed, *took, body, now),
        Entry::Reasoning {
            text, done, open, ..
        } => {
            // A thought that is over is one row until the reader opens it,
            // and then it is the text it held (ADR-0011 §11, issue #44).
            if *done && !*open {
                return gutter(vec![Line::from(Span::styled("⌁ reasoned", theme::faint()))]);
            }
            let mut rows = markdown::lines(text, body);
            if !*done && !*open {
                let keep = rows.len().saturating_sub(3);
                rows = rows.split_off(keep);
            }
            gutter(rows)
        }
        Entry::Call(call) => gutter(call::rows(call, body, now)),
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

/// A turn's answer: the marker at its head, and the prose beneath it. The
/// marker is the run's own clock — a braille frame and a duration that counts
/// up while the answer streams, and then the outcome and the duration it
/// stopped at, which never move again (ADR-0011 §4).
fn answer_rows(
    text: &str,
    done: bool,
    failed: bool,
    took: Option<Duration>,
    body: u16,
    now: Duration,
) -> Vec<Line<'static>> {
    let (glyph, style, elapsed) = match (done, failed) {
        (false, _) => (pulse::frame(now), theme::prompt(), now),
        (true, true) => ("✗", theme::bad(), took.unwrap_or_default()),
        (true, false) => ("✓", theme::good(), took.unwrap_or_default()),
    };
    let mut rows = vec![Line::from(vec![
        Span::styled(format!("{glyph} "), style),
        Span::styled(pulse::said(elapsed), theme::faint()),
    ])];
    rows.extend(markdown::lines(text, body));
    gutter(rows)
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
