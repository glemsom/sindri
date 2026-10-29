//! The terminal UI: the core's event stream, rendered (ADR-0011).
//!
//! The crate is a renderer and nothing else. It holds no turn loop, no retry
//! policy and no sandbox code: a front end implements [`driver::Driver`] to run
//! one prompt, and this crate draws what the run did while it does. The pieces
//! are [`App`] (what happened), [`render`] (where it goes) and [`run`] (the one
//! terminal the crate owns).

pub mod app;
pub mod composer;
pub mod driver;
pub mod entry;
pub mod face;
pub mod markdown;
mod msg;
pub mod rail;
pub mod render;
pub mod run;
pub mod theme;

pub use app::App;
pub use face::Face;
pub use rail::Launch;
pub use run::{Boot, Card, Ending};
