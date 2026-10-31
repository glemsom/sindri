//! The store itself: it measures a source at a column width, and holds the
//! protocols it has already rendered so an answer that is redrawn costs pixels
//! and not a decode. A mermaid fence is the same bargain as a file — it is
//! measured where it is laid out and rendered where it is drawn — so nothing
//! here knows which kind of source it is holding.

use std::path::PathBuf;
use std::time::SystemTime;

use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

use crate::graphics::Graphics;
use crate::picture::source::Source;
use crate::picture::svgs::Svgs;
use crate::picture::{PIXELS, Refusal, Worth};

/// How many rendered protocols stay live: each holds its pixels, so the store
/// keeps what a screenful of pictures needs and lets the oldest go.
pub(super) const LIVE: usize = 8;

pub struct Store {
    /// Where a relative `src` resolves: the workspace the frame was launched in.
    workspace: PathBuf,
    /// The answer a protocol is built from. It speaks Kitty, or there is no
    /// store: no sixel, no half-blocks, no fallback.
    picker: Picker,
    /// The svgs a fence body has been drawn into, keyed by the body alone: a
    /// diagram's column mapping comes after the drawing, so the same body at
    /// another width is the same svg and another block.
    svgs: Svgs,
    /// What has been rendered, oldest first: the last is the most recent, and
    /// the first is what makes room for a ninth.
    live: Vec<Live>,
    /// How many protocols have been built. A decode is the cost this store
    /// exists to avoid, so it is counted where a test can see it was avoided.
    pub(super) decodes: u64,
}

/// One protocol the store holds: the source it was rendered for, and the pixels
/// that source bought.
struct Live {
    key: Key,
    protocol: StatefulProtocol,
}

/// The whole of what a protocol is made from. A redraw asks again with the same
/// facts and is handed the same pixels; a file rewritten under a new answer, or
/// the same fence body drawn into another block, is not.
#[derive(Debug, PartialEq, Eq)]
enum Key {
    File {
        path: PathBuf,
        length: u64,
        mtime: SystemTime,
        columns: u16,
        rows: u16,
    },
    /// A fence is its own text: there is no file to stat, and the same body at
    /// the same block is the same diagram.
    Fence {
        body: String,
        columns: u16,
        rows: u16,
    },
}

impl Key {
    /// The key of a source as it is now: the lookup's one stat, and the only
    /// one.
    fn of(source: &Source, columns: u16, rows: u16) -> Option<Key> {
        match source {
            Source::File(path) => {
                let meta = std::fs::metadata(path).ok()?;
                Some(Key::File {
                    path: path.clone(),
                    length: meta.len(),
                    mtime: meta.modified().ok()?,
                    columns,
                    rows,
                })
            }
            Source::Fence(body) => Some(Key::Fence {
                body: body.clone(),
                columns,
                rows,
            }),
        }
    }
}

impl Store {
    /// The store, or nothing where the terminal cannot draw: a frame without a
    /// Kitty-capable answer has no pictures to hold.
    pub fn new(graphics: Option<&Graphics>, workspace: impl Into<PathBuf>) -> Option<Store> {
        let picker = graphics?.picker().clone();
        if picker.protocol_type() != ProtocolType::Kitty {
            return None;
        }
        Some(Store {
            workspace: workspace.into(),
            picker,
            svgs: Svgs::default(),
            live: Vec::new(),
            decodes: 0,
        })
    }

    /// What a markdown `src` is worth at `columns`: the rows it takes, read from
    /// its header and never from its pixels.
    pub fn measure(&self, src: &str, columns: u16) -> Worth {
        match Source::of(&self.workspace, src) {
            Ok(source) => self.weigh(source, columns),
            Err(refusal) => Worth::Refused(refusal),
        }
    }

    /// What a fence's body is worth at `columns`: the diagram it draws, measured
    /// from the svg the renderer would write, or the parser's own reason it
    /// draws none.
    pub fn fence(&self, body: &str, columns: u16) -> Worth {
        self.weigh(Source::Fence(body.to_owned()), columns)
    }

    fn weigh(&self, source: Source, columns: u16) -> Worth {
        source
            .measured(&self.svgs)
            .and_then(|pixels| self.guarded(pixels))
            .map(|pixels| {
                let (span, rows) = self.block(pixels, columns, source.allowance());
                Worth::Drawn {
                    source,
                    columns: span,
                    rows,
                }
            })
            .unwrap_or_else(Worth::Refused)
    }

    /// The pixels a source claims, held to the cap before a decoder spends
    /// anything on them: a decompression bomb is a header too.
    fn guarded(&self, pixels: (u32, u32)) -> Result<(u32, u32), Refusal> {
        let (width, height) = pixels;
        if u64::from(width) * u64::from(height) > PIXELS {
            return Err(Refusal::TooManyPixels { width, height });
        }
        Ok(pixels)
    }

    /// The protocol that draws a measured source. A lookup stats a file and
    /// nothing else, so a redraw of the same answer costs nothing and an agent
    /// that rewrote `plot.png` under a new answer pays once.
    /// `None` where nothing was measured, or where the file has gone since.
    pub fn protocol(&mut self, worth: &Worth) -> Option<&mut StatefulProtocol> {
        let Worth::Drawn {
            source,
            columns,
            rows,
        } = worth
        else {
            return None;
        };
        let key = Key::of(source, *columns, *rows)?;
        let at = match self.live.iter().position(|live| live.key == key) {
            Some(at) => at,
            None => {
                let (cell_w, cell_h) = self.picker.font_size();
                let (width, height) = (
                    u32::from(*columns) * u32::from(cell_w.max(1)),
                    u32::from(*rows) * u32::from(cell_h.max(1)),
                );
                let pixels = source.rasterised(&self.svgs, width, height)?;
                let protocol = self.picker.new_resize_protocol(pixels);
                self.decodes += 1;
                if self.live.len() >= LIVE {
                    self.live.remove(0);
                }
                self.live.push(Live { key, protocol });
                self.live.len() - 1
            }
        };
        Some(&mut self.live[at].protocol)
    }

    /// How many sources have been rendered: a file decoded, a fence rasterised.
    /// The cost this store exists to avoid, read where a repaint has to prove it
    /// spent none.
    #[cfg(test)]
    pub(crate) fn decodes(&self) -> u64 {
        self.decodes
    }

    /// How many fence bodies have been put through the renderer: the other half
    /// of what a picture costs, read where a repaint has to prove it spent
    /// nothing.
    #[cfg(test)]
    pub(crate) fn renders(&self) -> u64 {
        self.svgs.renders()
    }

    /// How many protocols are live: each holds its pixels, so a test can see
    /// that a ninth made room for itself rather than growing without end.
    #[cfg(test)]
    pub(crate) fn held(&self) -> usize {
        self.live.len()
    }

    /// The block a source of `pixels` takes in `columns`: it fits the column,
    /// never upscales past it, keeps its aspect from the cell size, and is
    /// capped at the allowance its source names — a `cap` that is a ceiling,
    /// never a target, so under it the rows are the column's.
    fn block(&self, pixels: (u32, u32), columns: u16, cap: u16) -> (u16, u16) {
        let (cell_w, cell_h) = self.picker.font_size();
        let (cell_w, cell_h) = (u64::from(cell_w.max(1)), u64::from(cell_h.max(1)));
        let column = u64::from(columns.max(1));
        let (picture_w, picture_h) = (u64::from(pixels.0.max(1)), u64::from(pixels.1.max(1)));
        // The width it is drawn at: never past the column, never past itself.
        let pixels_wide = picture_w.min(column * cell_w);
        let pixels_tall = (picture_h * pixels_wide).div_ceil(picture_w);
        let span = pixels_wide.div_ceil(cell_w).clamp(1, column);
        let rows = pixels_tall.div_ceil(cell_h).clamp(1, u64::from(cap));
        (span as u16, rows as u16)
    }
}

#[cfg(test)]
mod tests;
