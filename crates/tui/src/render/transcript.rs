//! The transcript column — the hero. Only the rows that fit are laid out, so an
//! unbounded conversation still has a bounded render tree (ADR-0011 §4).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::driver::{CallOver, Side};
use crate::entry::{Entry, NoteKind};
use crate::markdown;
use crate::theme;

/// Draws the transcript, anchored at its newest row.
pub fn draw(app: &App, area: Rect, buf: &mut Buffer) {
    Paragraph::new(tail(app, area.width, area.height)).render(area, buf);
}

/// The last rows of the transcript that fit, newest last.
pub fn tail(app: &App, width: u16, height: u16) -> Vec<Line<'static>> {
    let mut rows: Vec<Line<'static>> = Vec::new();
    for (index, entry) in app.entries().iter().enumerate().rev() {
        let mut block = entry_rows(entry, width);
        if index > 0 {
            block.push(Line::default());
        }
        block.append(&mut rows);
        rows = block;
        if rows.len() >= height as usize {
            break;
        }
    }
    let excess = rows.len().saturating_sub(height as usize);
    rows.split_off(excess)
}

/// One entry, as the rows it takes.
fn entry_rows(entry: &Entry, width: u16) -> Vec<Line<'static>> {
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
            markdown::text::wrap(
                markdown::text::glyphs(&[Span::styled(note.text.clone(), style)]),
                marker,
                style,
                width as usize,
            )
        }
    }
}

/// One call: a line while it runs with the tail of its output, and only its
/// exit status once it is over.
fn call_rows(call: &crate::entry::Call, width: u16) -> Vec<Line<'static>> {
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
    rows
}
