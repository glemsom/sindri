//! The frame half of the front end: what one moment of a run looks like when the
//! terminal draws it, and the ink every cell of it is in.
//!
//! The terminal itself is not here. Taking the alternate screen, going raw, reading
//! a key and painting the frame on the screen all live in `main.rs`'s `tui` module,
//! and no decision the core could make is here either (ADR-0005). What is here is
//! the picture, as a thing of its own, for one reason: a test can render it through
//! ratatui's headless backend and hold it to a golden frame (ADR-0012), where a
//! picture reachable only through a terminal could only ever be eyeballed.
//!
//! This is the same crate's other half rather than a second crate because the edge
//! this project draws is the one between the core and a terminal, and that edge is
//! untouched by a library target beside the binary.

pub mod ink; // the ink a cell is drawn in, asked for by name
pub mod view; // what one frame holds and where each part of it goes
