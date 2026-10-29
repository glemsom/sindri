//! Tables: columns sized to their contents and aligned under their headers, so
//! a table of files reads as a table rather than as pipe-separated noise.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::theme;

/// A table as it was written: rows of cells, the header row first.
#[derive(Debug, Default)]
pub struct Table {
    /// Every row's cells, in order.
    pub rows: Vec<Vec<String>>,
    /// How many leading rows are the header.
    pub head: usize,
}

impl Table {
    /// One cell, filled in as the parser walks the table.
    pub fn cell(&mut self, row: usize, text: String) {
        if self.rows.len() <= row {
            self.rows.push(Vec::new());
        }
        if let Some(cells) = self.rows.get_mut(row) {
            cells.push(text);
        }
    }
}

/// The table as rows, one line per row, with a rule under the header.
pub fn lines(table: &Table, width: usize) -> Vec<Line<'static>> {
    let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Vec::new();
    }
    let mut widths = vec![0usize; columns];
    for row in &table.rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width_found) = widths.get_mut(index) {
                *width_found = (*width_found).max(cell.width());
            }
        }
    }
    // A table wider than the column is cut at the last column that fits, so the
    // rows stay aligned instead of folding into porridge.
    let mut used = 0;
    let mut keep = 0;
    for width_found in &widths {
        used += width_found + 3;
        if used > width {
            break;
        }
        keep += 1;
    }
    let keep = keep.max(1);

    let mut out = Vec::new();
    for (index, row) in table.rows.iter().enumerate() {
        let is_head = index < table.head;
        let mut spans = Vec::new();
        for column in 0..keep {
            let cell = row.get(column).map(String::as_str).unwrap_or_default();
            let cells = widths.get(column).copied().unwrap_or(0);
            let padded = format!(
                "{cell}{:padding$}",
                "",
                padding = cells.saturating_sub(cell.width())
            );
            if column > 0 {
                spans.push(Span::styled(" │ ", theme::faint()));
            }
            spans.push(if is_head {
                Span::styled(padded, Style::new().add_modifier(Modifier::BOLD))
            } else {
                Span::styled(padded, Style::default())
            });
        }
        out.push(Line::from(spans));
        if is_head && index + 1 == table.head {
            let rule: Vec<Span<'static>> = widths
                .iter()
                .take(keep)
                .map(|cells| Span::styled("─".repeat(*cells), theme::faint()))
                .collect();
            out.push(Line::from(intersperse(rule)));
        }
    }
    out
}

fn intersperse(spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    for (index, span) in spans.into_iter().enumerate() {
        if index > 0 {
            out.push(Span::styled("─┼─", theme::faint()));
        }
        out.push(span);
    }
    out
}
