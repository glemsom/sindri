//! The transcript column — the hero. Only the rows that fit are laid out, so an
//! unbounded conversation still has a bounded render tree (ADR-0011 §4): the
//! window is built from the newest row backwards and stops as soon as it has a
//! screenful, so scrolling costs what was scrolled and nothing more.

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

/// What the transcript column shows, and what it cost to show it.
pub struct View {
    /// The visible rows, oldest first.
    pub rows: Vec<Line<'static>>,
    /// The same rows as plain text, ANSI stripped: what a copy takes.
    pub plain: Vec<String>,
    /// The scroll offset actually used, clamped to the thread's own length.
    pub scroll: usize,
    /// Whether the window reaches the first row of the thread, so scrolling
    /// further up would show nothing new.
    pub at_top: bool,
    /// How many entries were laid out to build the window.
    pub laid_out: usize,
    /// The screen row holding the spill affordance, with the file it offers.
    pub spill: Option<(usize, PathBuf)>,
}

/// Draws the transcript window, and tells the frame what the mouse and the
/// clipboard can see of it.
pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    let view = window(app, area.width, area.height as usize);
    app.set_viewport(area, view.plain, view.spill.clone());
    Paragraph::new(view.rows).render(area, buf);
}

/// The rows of the thread that fit `height` rows at `width` columns, from the
/// scroll offset up to the newest row.
pub fn window(app: &mut App, width: u16, height: usize) -> View {
    let height = height.max(1);
    let width = width.max(1);
    let count = app.entries().len();
    app.note_width(width);

    // Walk back from the newest entry until the window and everything scrolled
    // past it are accounted for. Only these entries are ever laid out.
    let need = height.saturating_add(app.scroll());
    let mut acc = 0usize;
    let mut first = count;
    while first > 0 && acc < need {
        first -= 1;
        acc += app.measure(first, width) + usize::from(first > 0);
    }

    let mut rows: Vec<Line<'static>> = Vec::new();
    let mut spill = None;
    for index in first..count {
        if index > 0 {
            rows.push(Line::default());
        }
        let rows_here = entry_rows(&app.entries()[index], width);
        if let Some(path) = spill_of(&app.entries()[index]) {
            spill = Some((rows.len() + rows_here.len().saturating_sub(1), path));
        }
        rows.extend(rows_here);
    }

    // The window is the bottom of what was laid out, moved up by the scroll.
    let visible = height.min(rows.len());
    let scroll = app.scroll().min(rows.len().saturating_sub(visible));
    let end = rows.len() - scroll;
    let start = end.saturating_sub(height);
    let mut shown = rows.split_off(start);
    shown.truncate(height);

    let plain: Vec<String> = shown.iter().map(plain).collect();
    let laid_out = count.saturating_sub(first);
    app.settled(scroll, plain.len());
    // A row above the window is not on screen, and a row below it is not either.
    let spill = spill.and_then(|(row, path)| {
        row.checked_sub(start)
            .filter(|row| *row < shown.len())
            .map(|row| (row, path))
    });
    View {
        rows: shown,
        plain,
        scroll,
        at_top: first == 0 && start == 0,
        laid_out,
        spill,
    }
}

/// One row of styled text as the characters a reader would copy.
pub fn plain(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// The spill file an entry offers to open, when it offers one.
fn spill_of(entry: &Entry) -> Option<PathBuf> {
    match entry {
        Entry::Call(call) => call.spill_file().map(Path::to_path_buf),
        _ => None,
    }
}

/// One entry, as the rows it takes.
pub fn entry_rows(entry: &Entry, width: u16) -> Vec<Line<'static>> {
    match entry {
        Entry::Prompt(text) => markdown::text::wrap(
            markdown::text::glyphs(&[Span::styled(text.clone(), theme::faint())]),
            "› ",
            theme::prompt(),
            width as usize,
        ),
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
            markdown::text::wrap(
                markdown::text::glyphs(&[Span::styled(note.text.clone(), style)]),
                marker,
                style,
                width as usize,
            )
        }
    }
}

/// One call: a line while it runs with the tail of its output, then its exit
/// status, and the spill affordance when the output was too large to show
/// (ADR-0011 §9).
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
            for (side, line) in &call.tail {
                let style = match side {
                    Side::Stdout => theme::faint(),
                    // A command's complaint is not the same thing as its output.
                    Side::Stderr => theme::bad(),
                };
                let keep = width.saturating_sub(4) as usize;
                let shown: String = format!("  {line}").chars().take(keep).collect();
                rows.push(Line::from(Span::styled(format!("  │ {shown}"), style)));
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
            offer.chars().take(keep).collect::<String>(),
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
