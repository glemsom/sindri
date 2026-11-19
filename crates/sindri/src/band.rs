//! A row of the composer's band: the gutter it opens with, and the runs it is drawn as.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen::{self, Clip};
use crate::rows::{INDENT, SIGIL};

/// The gutter a row of the band opens with: `Lit` is the sigil of the row the caret waits in, `Open`
/// the sigil of a row a prompt's words begin on, and `Blank` the indent under one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gutter {
    Lit,
    Open,
    Blank,
}

/// The column of the gutter's blank cell, counted from the band's first column inside its box: a
/// draft holding nothing waits there, so no glyph of the placeholder is reversed out of its block.
pub fn blank() -> u16 {
    pen::cells(SIGIL).saturating_sub(1)
}

/// Writes one row of the composer's band: the gutter it opens with, then each run in order in the ink
/// it carries, cut at the row's edge.
pub fn row(
    buffer: &mut Buffer,
    area: Rect,
    gutter: Gutter,
    runs: &[(&str, Ink)],
    palette: &Palette,
) {
    let (column, ink) = match gutter {
        Gutter::Lit => (SIGIL, Ink::Accent),
        Gutter::Open => (SIGIL, Ink::Dim),
        Gutter::Blank => (INDENT, Ink::Dim),
    };
    let mut spans: Vec<(&str, Ink)> = Vec::with_capacity(runs.len().saturating_add(1));
    spans.push((column, ink));
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
