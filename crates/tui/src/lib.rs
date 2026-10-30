//! The terminal UI: the core's event stream, rendered (ADR-0011). The crate
//! is a renderer and nothing else. It holds no turn loop, no retry policy and
//! no sandbox code: a front end implements [`driver::Driver`] to run one
//! prompt, and this crate draws what the run did while it does.

pub mod app;
mod clock;
pub mod command;
pub mod completion;
pub mod composer;
pub mod copy;
pub mod driver;
pub mod entry;
pub mod export;
pub mod face;
mod heard;
mod keys;
pub mod markdown;
mod msg;
pub mod overlay;
pub mod rail;
pub mod render;
pub mod run;
pub mod selection;
pub mod sessions;
pub mod settings;
pub mod theme;

pub use app::App;
pub use face::Face;
pub use rail::Launch;
pub use run::{Card, Ending};
