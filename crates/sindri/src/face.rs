use crate::graphics::Ceiling;
use crate::picture::{self, Picture, Surface};

const FACE_PNG: &[u8] = include_bytes!("../face.png");

const SOURCE: &str = "face.png";

pub const CAP: u16 = 12;

pub fn over_rail(cols: u16, surface: &Surface) -> Option<Picture> {
    measure(Ceiling::new(cols, CAP), surface)
}

pub fn measure(ceiling: Ceiling, surface: &Surface) -> Option<Picture> {
    if !surface.draws() {
        return None;
    }
    picture::carried(SOURCE, FACE_PNG, ceiling, surface).ok()
}
