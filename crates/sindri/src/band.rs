use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen::{self, Clip};
use crate::rows::{SIGIL, SPINE};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gutter {
    Lit,
    Open,
    Blank,
}

pub fn row(
    buffer: &mut Buffer,
    area: Rect,
    gutter: Gutter,
    runs: &[(&str, Ink)],
    palette: &Palette,
) {
    let (sigil, ink) = match gutter {
        Gutter::Lit => (SIGIL.trim_end(), Ink::Accent),
        Gutter::Open => (SIGIL.trim_end(), Ink::Dim),
        Gutter::Blank => (" ", Ink::Dim),
    };
    let mut spans: Vec<(&str, Ink)> = Vec::with_capacity(runs.len().saturating_add(2));
    spans.push((SPINE, Ink::Dim));
    spans.push((sigil, ink));
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
