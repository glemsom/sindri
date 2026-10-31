//! A raw Kitty image, placed by the frame itself: `a=T,f=100` once, then `a=p`
//! for nothing on every frame after it. `ratatui-image` emits a virtual
//! placement plus Unicode placeholders, which is right where an image owns the
//! cells it is drawn in — the face, the pictures, the seam — and useless where
//! an image is a layer glyphs sit on: placeholders paint the cells they cover,
//! and there is no layer to put them on. So this module speaks the protocol
//! directly, and keeps the discipline `render/seam.rs` keeps: the pixels go out
//! when the rect they were made for changes, and never again.
//!
//! One `Placement` is one image: it holds the id the terminal knows that image
//! by, and the rect it was last transmitted for. A caller that draws two images
//! holds two. It names no colour and decides no look — it is given pixels and a
//! rect, and says where the pixels go.

use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::{ImageFormat, RgbaImage};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui_image::picker::ProtocolType;

use crate::graphics::Graphics;

/// The most base64 characters one escape may carry: the protocol's ceiling, and
/// the reason one image is several escapes. A multiple of four, so a split lands
/// on a quantum and no chunk but the last carries padding.
const CHUNK: usize = 4096;

/// The id the next image is transmitted under: one counter for the process, so no
/// two images share an id and a resized one is never handed the id its
/// predecessor is still known by.
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// One image the frame draws as pixels, and the rect it was last sent for: the
/// same rect again is a placement, a few dozen bytes, and any other is an image
/// the terminal does not hold yet.
pub struct Placement {
    id: u32,
    sent: Option<Rect>,
}

impl Placement {
    /// The place an image is drawn in, or `None` where this terminal draws no
    /// raw image at all: its answer is not Kitty graphics, or it is tmux, which
    /// hands a Kitty terminal's own variables to the program it runs and then
    /// swallows what that program draws kitten-shaped. Nothing is written there,
    /// so a caller falls back to its glyphs without knowing why.
    ///
    /// The answer is asked once, at the same moment the terminal is asked what it
    /// speaks: a frame is built on one of them and never meets the other.
    pub fn new(graphics: &Graphics) -> Option<Placement> {
        if !speaks(graphics, tmux()) {
            return None;
        }
        Some(Placement {
            id: next_id(),
            sent: None,
        })
    }

    /// Places `image` in `at`, on layer `z`: `-1` draws above the cells and below
    /// the glyphs, which is what a wash under text wants, and `0` is right for a
    /// stroke drawn where no glyph lives.
    ///
    /// The pixels are transmitted the first time and after every change of rect,
    /// because an image is made for the cells it fills; every other frame emits
    /// the placement alone. An image is what this `Placement` is for: hand it
    /// different pixels at the same rect and nothing is sent, so the terminal goes
    /// on showing what it was given.
    pub fn place(&mut self, image: &RgbaImage, at: Rect, z: i32, buf: &mut Buffer) {
        // An escape is a symbol hundreds of columns wide by its characters, and
        // `Buffer::diff` drops the cell after one: the seam answers that with a
        // margin two cells wide (`render::GAP_COLUMNS`). A rect with no second
        // cell has none to spend, so drawing it would cost the frame a cell
        // beside it — the caller keeps its glyphs instead.
        if at.width < 2 || at.height == 0 {
            return;
        }
        let mut sequence = String::new();
        if self.sent != Some(at) {
            // The id the terminal still knows the old image by is left behind
            // rather than written over: a transmit under a live id is refused,
            // and pixels of a size nobody asked for are not this image.
            let id = match self.sent {
                Some(_) => next_id(),
                None => self.id,
            };
            let Some(sent) = transmit(image, id) else {
                return;
            };
            sequence.push_str(&sent);
            self.id = id;
            self.sent = Some(at);
        }
        sequence.push_str(&placing(self.id, z));
        cover(at, sequence, buf);
    }
}

/// Whether the terminal is spoken to at all: it answered Kitty graphics, and it
/// is not tmux standing in front of a terminal that did.
fn speaks(graphics: &Graphics, tmux: bool) -> bool {
    !tmux && graphics.picker().protocol_type() == ProtocolType::Kitty
}

/// Whether `$TMUX` is set at all, whatever it says: a variable is a name here,
/// not a path, and a terminal that cannot decode its own environment is still
/// inside tmux.
fn tmux() -> bool {
    std::env::var_os("TMUX").is_some()
}

fn next_id() -> u32 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// The escapes that send `image`'s pixels to the terminal, under `id`: a PNG,
/// chunked, quietly. `None` where it cannot be encoded, which a caller survives
/// by drawing nothing this frame rather than by drawing an image it cannot send.
fn transmit(image: &RgbaImage, id: u32) -> Option<String> {
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png).ok()?;
    let payload = STANDARD.encode(png.into_inner());
    let chunks: Vec<&[u8]> = payload.as_bytes().chunks(CHUNK).collect();
    let mut sequence = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let more = u8::from(index + 1 < chunks.len());
        sequence.push_str("\u{1b}_G");
        if index == 0 {
            // `q=2` silences every reply: nothing on this side of the frame
            // reads stdin for answers, and an answer is a keystroke eaten.
            sequence.push_str(&format!("a=T,f=100,q=2,i={id},"));
        }
        sequence.push_str(&format!("m={more};"));
        // Base64 is ASCII, so a chunk is a `str` by construction.
        sequence.push_str(std::str::from_utf8(chunk).ok()?);
        sequence.push_str("\u{1b}\\");
    }
    Some(sequence)
}

/// The escape that puts an image the terminal already holds on layer `z`, in the
/// cell the frame's cursor is on. `C=1` because ratatui positions that cursor
/// itself: a placement may put pixels on the screen and never move the pen.
fn placing(id: u32, z: i32) -> String {
    format!("\u{1b}_Ga=p,i={id},C=1,q=2,z={z}\u{1b}\\")
}

/// Writes `sequence` into the one cell the image is anchored at, and marks every
/// cell it covers: what a cell holds is a symbol, so the escape rides in one, and
/// the rest are `skip` so a glyph is never painted over the pixels it drew. The
/// cell that carries it is the frame's too — a skipped cell is never sent, so an
/// escape in one would never reach the terminal.
fn cover(at: Rect, sequence: String, buf: &mut Buffer) {
    for y in at.top()..at.bottom() {
        for x in at.left()..at.right() {
            let Some(cell) = buf.cell_mut((x, y)) else {
                continue;
            };
            if (x, y) == (at.x, at.y) {
                cell.set_symbol(&sequence);
            } else {
                cell.set_symbol(" ");
                cell.set_skip(true);
            }
        }
    }
}

#[cfg(test)]
mod tests;
