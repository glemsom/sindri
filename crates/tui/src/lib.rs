//! The terminal UI: the core's event stream, rendered (ADR-0011).
//!
//! The crate is a renderer and nothing else. It holds no turn loop, no retry
//! policy and no sandbox code: a front end implements [`driver::Driver`] to run
//! one prompt, and this crate draws what the run did while it does.
//!
//! - [`App`] is what happened — the [`entry`] rows, the [`composer`], the queue
//!   and the [`rail`]'s facts — and it is driven by keys and by the run.
//! - [`render`] is where it goes: [`markdown`] for an answer, and a `render`
//!   module per region (transcript, rail, composer, card).
//! - [`run`] is the one terminal the crate owns: raw mode, the alternate
//!   screen, the input thread and the frame loop.
//! - [`driver`] is the seam a front end implements, and [`face`] is the one
//!   optional ornament.

pub mod app;
pub mod command;
pub mod completion;
pub mod composer;
pub mod copy;
pub mod driver;
pub mod entry;
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
pub mod theme;

pub use app::App;
pub use face::Face;
pub use rail::Launch;
pub use run::{Card, Ending};
