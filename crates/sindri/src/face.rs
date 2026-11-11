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

/// The face measured into the cells `ceiling` leaves it, or nothing at all: decoration that
/// cannot be placed is left out rather than said in words.
pub fn measure(ceiling: Ceiling, surface: &Surface) -> Option<Picture> {
    if !surface.draws() {
        return None;
    }
    picture::carried(SOURCE, FACE_PNG, ceiling, surface).ok()
}
