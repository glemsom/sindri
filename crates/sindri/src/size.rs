use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Look;
use crate::link;
use crate::palette::Palette;
use crate::pen::{self, Clip};

pub const SCALE: u16 = 2;

pub fn speaks(term: Option<&str>, program: Option<&str>) -> bool {
    link::named(term, program, &["kitty", "ghostty"])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sized {
    left: u16,
    width: u16,
    scale: u16,
    words: String,
    look: Look,
}

impl Sized {
    pub fn new(left: u16, scale: u16, words: impl Into<String>, look: Look) -> Sized {
        let words = words.into();
        Sized {
            left,
            width: pen::cells(&words).saturating_mul(scale),
            scale,
            words,
            look,
        }
    }

    pub fn covers(&self, at: u16) -> bool {
        self.left <= at && at < self.left.saturating_add(self.width)
    }

    pub(crate) fn shifted(&self, by: u16) -> Sized {
        Sized {
            left: self.left.saturating_add(by),
            ..self.clone()
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn scale(&self) -> u16 {
        self.scale
    }

    pub fn words(&self) -> &str {
        &self.words
    }

    pub fn spelling(&self) -> String {
        let mut spelling = String::with_capacity(self.words.len() * usize::from(self.scale));
        for glyph in self.words.chars() {
            spelling.push(glyph);
            spelling.push_str(&pen::blank(
                pen::cell(glyph).saturating_mul(self.scale.saturating_sub(1)),
            ));
        }
        spelling
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Laid {
    row: u16,
    left: u16,
    run: Sized,
}

impl Laid {
    pub fn row(&self) -> u16 {
        self.row
    }

    pub fn left(&self) -> u16 {
        self.left
    }

    pub fn run(&self) -> &Sized {
        &self.run
    }

    fn covers(&self, row: u16, column: u16) -> bool {
        self.row <= row
            && row < self.row.saturating_add(self.run.scale)
            && self.left <= column
            && column < self.left.saturating_add(self.run.width)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sizes {
    laid: Vec<Laid>,
}

impl Sizes {
    pub fn none() -> Sizes {
        Sizes::default()
    }

    pub fn lay(&mut self, row: u16, left: u16, runs: &[Sized]) {
        self.laid.extend(runs.iter().map(|run| Laid {
            row,
            left: left.saturating_add(run.left),
            run: run.clone(),
        }));
    }

    pub fn settle(&mut self, band: Rect, buffer: &mut Buffer, palette: &Palette) {
        self.laid.retain(|laid| {
            let fits = laid.row.saturating_add(laid.run.scale).saturating_sub(1) < band.bottom();
            if fits {
                pen::write(
                    buffer,
                    Rect {
                        x: laid.left,
                        y: laid.row,
                        width: laid.run.width,
                        height: 1,
                    },
                    &[(laid.run.spelling(), laid.run.look)],
                    Clip::Edge,
                    palette,
                );
            }
            fits
        });
    }

    pub fn covers(&self, row: u16, column: u16) -> Option<&Laid> {
        self.laid.iter().find(|laid| laid.covers(row, column))
    }

    pub fn laid(&self) -> &[Laid] {
        &self.laid
    }
}

#[cfg(test)]
mod tests;
