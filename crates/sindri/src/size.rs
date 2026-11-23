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
    fraction: Option<(u8, u8, u8, u8)>,
    words: String,
    look: Look,
}

impl Sized {
    pub fn new(left: u16, scale: u16, words: impl Into<String>, look: Look) -> Sized {
        assert!((1..=7).contains(&scale));
        let words = words
            .into()
            .chars()
            .filter(|glyph| !glyph.is_control())
            .collect::<String>();
        Sized {
            left,
            width: pen::cells(&words).saturating_mul(scale),
            scale,
            fraction: None,
            words,
            look,
        }
    }

    pub fn fractional(
        mut self,
        numerator: u8,
        denominator: u8,
        vertical: u8,
        horizontal: u8,
    ) -> Sized {
        assert!(numerator > 0 && numerator < denominator && denominator <= 15);
        assert!(vertical <= 2 && horizontal <= 2);
        self.fraction = Some((numerator, denominator, vertical, horizontal));
        self
    }

    pub fn fraction(&self) -> Option<(u8, u8, u8, u8)> {
        self.fraction
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

    pub fn sequence(&self) -> Vec<u8> {
        use unicode_segmentation::UnicodeSegmentation;
        if self.fraction.is_none() && self.words.is_ascii() {
            return format!(
                "\x1b]66;s={};{}\x1b\\",
                self.scale,
                self.words
                    .chars()
                    .filter(|glyph| !glyph.is_control())
                    .collect::<String>()
            )
            .into_bytes();
        }
        let mut metadata = format!("s={}", self.scale);
        if let Some((n, d, v, h)) = self.fraction {
            metadata.push_str(&format!(":n={n}:d={d}:v={v}:h={h}"));
        }
        let mut bytes = Vec::new();
        for glyph in self.words.graphemes(true) {
            let width = pen::cells(glyph);
            if width == 0 {
                continue;
            }
            bytes.extend_from_slice(format!("\x1b]66;{metadata}:w={width};").as_bytes());
            bytes.extend(glyph.bytes().filter(|byte| *byte >= 32 && *byte != 127));
            bytes.extend_from_slice(b"\x1b\\");
        }
        bytes
    }

    pub fn spelling(&self) -> String {
        let mut spelling = String::with_capacity(self.words.len() * usize::from(self.scale));
        use unicode_segmentation::UnicodeSegmentation;
        for glyph in self.words.graphemes(true) {
            spelling.push_str(glyph);
            spelling.push_str(&pen::blank(
                pen::cells(glyph).saturating_mul(self.scale.saturating_sub(1)),
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
            let fits = laid.row >= band.y
                && laid.left >= band.x
                && u32::from(laid.row) + u32::from(laid.run.scale) <= u32::from(band.bottom())
                && u32::from(laid.left) + u32::from(laid.run.width) <= u32::from(band.right());
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

    pub fn append(&mut self, other: Sizes) {
        self.laid.extend(other.laid);
    }

    pub fn laid(&self) -> &[Laid] {
        &self.laid
    }
}

#[cfg(test)]
mod tests;
