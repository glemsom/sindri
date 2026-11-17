//! A row of the composer's band: the gutter it opens with, and the runs it is drawn as.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen::{self, Clip};
use crate::rows::{INDENT, SIGIL};

/// Writes one row of the composer's band: the gutter it opens with, then each run in order in the ink
/// it carries, cut at the row's edge. The sigil opens the row a prompt's words begin on and every
/// other row keeps the gutter blank, so a draft and a waited prompt begin in the answer's first column.
pub fn row(buffer: &mut Buffer, area: Rect, opens: bool, runs: &[(&str, Ink)], palette: &Palette) {
    let gutter = match opens {
        true => SIGIL,
        false => INDENT,
    };
    let mut spans: Vec<(&str, Ink)> = Vec::with_capacity(runs.len().saturating_add(1));
    spans.push((gutter, Ink::Dim));
    spans.extend_from_slice(runs);
    pen::write(
        buffer,
        Rect {
            y: area.y,
            height: 1,
            ..area
        },
        &spans,
        Clip::Edge,
        palette,
    );
}

#[cfg(test)]
mod tests;
