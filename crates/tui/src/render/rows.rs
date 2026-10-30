//! One entry as rows: the shapes a turn takes in the transcript column — a
//! prompt on its band, an answer under its marker, a call under its line, a
//! note at the turn's own edge — and the gutter that says what belongs to
//! what (ADR-0011, ADR-0016).

use std::time::Duration;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::entry::{Entry, NoteKind, Prompt};
use crate::markdown;
use crate::pulse;
use crate::render::call;
use crate::theme;

/// The columns a turn's body hangs off the turn's own edge: the answer and the
/// calls it made start here, so a call reads as belonging to its turn.
pub const GUTTER: u16 = 2;

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
