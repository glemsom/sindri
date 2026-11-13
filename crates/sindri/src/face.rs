//! Sindri's face: the picture the binary carries instead of the workspace holding it. It is
//! measured from its own header and drawn as the decoration it is, so a frame whose terminal
//! places no picture is given neither a cell nor a word for it.

use crate::graphics::Ceiling;
use crate::picture::{self, Picture, Surface};

/// The face as the binary carries it, beside the bundled font: one file ships, and no path is
/// read at runtime for it.
const FACE_PNG: &[u8] = include_bytes!("../face.png");

/// The name the face is filed under, which is what a frame holds a placement by.
const SOURCE: &str = "face.png";

/// The most rows the face is ever drawn in at the top of a rail, however tall the rail beside it:
/// a decoration that took more would crowd out the facts it stands above.
pub const CAP: u16 = 12;

/// The face as a rail `cols` columns wide draws it over its top rows: measured into the columns it
/// is given and never taller than [`CAP`].
pub fn over_rail(cols: u16, surface: &Surface) -> Option<Picture> {
    measure(Ceiling::new(cols, CAP), surface)
}

/// The most rows the face is drawn in over the door's wordmark, and the fewest columns and rows it is
/// drawn there at all: more would be the screen rather than the head of it, less a smudge. A frame
/// that small is handed the door without the face and reserves no row for it.
const WORDMARK_CAP: u16 = 16;
const WORDMARK_FLOOR_COLS: u16 = 12;
const WORDMARK_FLOOR_ROWS: u16 = 8;

/// The face as the door draws it over its wordmark: measured into the columns of the block it stands
/// over and the rows the band leaves it, and never taller than [`WORDMARK_CAP`].
pub fn over_wordmark(cols: u16, rows: u16, surface: &Surface) -> Option<Picture> {
    let picture = measure(Ceiling::new(cols, rows.min(WORDMARK_CAP)), surface)?;
    let clears = picture.cols() >= WORDMARK_FLOOR_COLS && picture.rows() >= WORDMARK_FLOOR_ROWS;
    clears.then_some(picture)
}

/// The face measured into the cells `ceiling` leaves it, or nothing at all: decoration that
/// cannot be placed is left out rather than said in words.
pub fn measure(ceiling: Ceiling, surface: &Surface) -> Option<Picture> {
    if !surface.draws() {
        return None;
    }
    picture::carried(SOURCE, FACE_PNG, ceiling, surface).ok()
}
