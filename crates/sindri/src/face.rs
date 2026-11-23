use ratatui::layout::Rect;

use crate::graphics::{Ceiling, CellSize};
use crate::picture::{self, Picture, Surface};
use crate::pulse::Turning;

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

/// The identity the face's picture is held under, above every plane's.
pub const ID: u32 = 337;

/// The light each frame past the root is drawn in, in the order the terminal turns them.
const BREATH: [f32; 3] = [0.8, 0.6, 0.8];

/// The frames the face is animated through, the first being the picture as it is carried.
pub fn frames(root: &image::RgbaImage) -> Vec<image::RgbaImage> {
    let mut frames = Vec::with_capacity(BREATH.len() + 1);
    frames.push(root.clone());
    for light in BREATH {
        let mut frame = root.clone();
        for pixel in frame.pixels_mut() {
            for channel in &mut pixel.0[..3] {
                *channel = (f32::from(*channel) * light).round() as u8;
            }
        }
        frames.push(frame);
    }
    frames
}

/// The bytes that give the face's image back, leaving no placement of it behind.
pub fn delete() -> Vec<u8> {
    format!("\x1b_Ga=d,d=i,i={ID},q=2\x1b\\").into_bytes()
}

/// What the frame asks of the terminal's animation of the face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// The terminal answers no query for frames and animation: the picture is the face, still.
    Still,
    /// The terminal keeps the face's frames: the loop they are turned in, and none where a run
    /// settled on the root frame.
    Turning(Option<Turning>),
}

/// The face one frame asks the terminal for: the picture, the cells it stands on, and the motion.
#[derive(Clone, Debug)]
pub struct Face {
    picture: Picture,
    area: Rect,
    cell: CellSize,
    motion: Motion,
}

impl Face {
    pub fn new(picture: Picture, area: Rect, cell: CellSize, motion: Motion) -> Face {
        Face {
            picture,
            area,
            cell,
            motion,
        }
    }

    pub fn picture(&self) -> &Picture {
        &self.picture
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn cell(&self) -> CellSize {
        self.cell
    }

    pub fn motion(&self) -> Motion {
        self.motion
    }
}
