//! The transcript column — the hero. Only the rows that fit are laid out, so
//! an unbounded conversation still has a bounded render tree (ADR-0011): the
//! window is built from the newest row backwards and stops as soon as it has
//! a screenful, so scrolling costs what was scrolled and nothing more.

use std::path::{Path, PathBuf};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::driver::{CallOver, Side};
use crate::entry::{Call, Entry, NoteKind};
use crate::markdown;
use crate::theme;

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
        acc += app.measure(first, width) + usize::from(first > 0);
    }

    let mut rows: Vec<Line<'static>> = Vec::new();
    let mut spill: Vec<(usize, PathBuf)> = Vec::new();
    for index in first..count {
        if index > 0 {
            rows.push(Line::default());
        }
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
    match entry {
        Entry::Prompt(text) => vec![Line::from(vec![
            Span::styled("› ", theme::prompt()),
            Span::styled(text.clone(), theme::faint()),
        ])],
        Entry::Answer { text, .. } => markdown::lines(text, width),
        Entry::Reasoning { text, done, .. } => {
            if *done {
                return vec![Line::from(Span::styled("⌁ reasoned", theme::faint()))];
            }
            let mut rows = markdown::lines(text, width);
            let keep = rows.len().saturating_sub(3);
            rows.split_off(keep)
        }
        Entry::Call(call) => call_rows(call, width),
        Entry::Note(note) => {
            let (marker, style) = match note.kind {
                NoteKind::Error => ("! ", theme::bad()),
                NoteKind::Info => ("· ", theme::faint()),
            };
            // The marker takes its columns out of the row before the words
            // do, or the last word of a wrapped note is cut in half.
            markdown::text::wrap(
                markdown::text::glyphs(&[Span::styled(note.text.clone(), style)]),
                marker,
                style,
                (width as usize).saturating_sub(marker.chars().count()),
            )
        }
    }
}

fn call_rows(call: &Call, width: u16) -> Vec<Line<'static>> {
    let command = call
        .command
        .as_deref()
        .unwrap_or("(the call could not be read)");
    let mut rows = Vec::new();
    match &call.over {
        None => {
            rows.push(Line::from(vec![
                Span::styled("⏺ ", theme::faint()),
                Span::styled("bash ", theme::faint()),
                Span::styled(command.to_string(), theme::faint()),
            ]));
            // stdout and stderr differ by a glyph as well as by hue: a tail
            // read on a terminal that lost its colour still tells a warning
            // from a result (ADR-0016).
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
        Some(over) => {
            let (marker, status, style) = match over {
                CallOver::Exited(0) => ("✓ ", "exit 0".to_string(), theme::good()),
                CallOver::Exited(code) => ("✗ ", format!("exit {code}"), theme::bad()),
                CallOver::TimedOut => ("✗ ", "timed out".to_string(), theme::bad()),
                CallOver::Cancelled => ("✗ ", "cancelled".to_string(), theme::bad()),
                CallOver::Refused(detail) => ("✗ ", format!("sandbox: {detail}"), theme::bad()),
            };
            rows.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{status}  "), style),
                Span::styled(command.to_string(), theme::faint()),
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
