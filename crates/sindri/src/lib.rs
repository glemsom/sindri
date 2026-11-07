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

pub mod commands; // the `/` commands: a draft line the frame answers itself
pub mod composer; // the draft a prompt is typed into, as a value rather than a widget
pub mod ink; // the look a cell is drawn in, asked for by name: the ink it is in
pub mod keys; // the mode stack and one key table per mode
pub mod mark; // the cells a reader has marked, and the words they hold
pub mod markdown; // markdown becomes the rows a transcript entry is drawn as
pub mod models; // the names the wire serves, ranked as a query is typed into the screen
pub mod picker; // the files of the workspace, walked with `ignore`
pub mod pulse; // the marker at the head of the newest answer: the braille glyph a live run turns
pub mod queue; // the prompts typed while a run is live, waiting above the composer in the order
pub mod rail; // the run's facts down the frame's right-hand columns, read as a ledger
pub mod rows; // the transcript's rows: one entry of the core's records laid out as the rows it is
pub mod settings; // the config file's keys as one list, the value each of them holds
pub mod stream; // the turn in flight: the answer and the thought of the run's newest turn, fed a
pub mod view; // what one frame holds and where each part of it goes
