//! The terminal UI: the core's event stream, rendered. The crate
//! is a renderer and nothing else. It holds no turn loop, no retry policy and
//! no sandbox code: a front end implements [`driver::Driver`] to run one
//! prompt, and this crate draws what the run did while it does.

pub mod app; // the frame's state: transcript, composer, queue and rail
pub mod card; // the card: what a broken boot has to say
pub mod command; // the `/` commands and the one catalogue of them
pub mod completion; // the completion offer
pub mod composer; // the composer: the prompt being typed, as rows and a cursor
pub mod copy; // copying text out of the frame
pub mod driver; // the seam a front end implements
pub mod entry; // one row of the transcript, and the state a tool call carries
mod expand; // opening a call, and the reasoning beside it, in place
pub mod export; // `/export`: the thread the frame is holding, written as markdown
pub mod face; // the face of Sindri, where the terminal speaks Kitty
pub mod graphics; // what this terminal can draw, and how big a cell it draws it in
mod heard; // what the run says to the frame
mod heights; // what each entry is worth in rows
mod keys; // the hands: what one key does to the frame
mod mark; // the mark a drag leaves on the transcript
pub mod markdown; // the answer as a reader sees it
mod msg; // what reaches the frame from outside it
pub mod overlay; // the overlays that take the screen while they are open
pub mod picture; // the store that knows what a picture's source is worth
pub mod pulse; // the pulse: the clock a live run keeps
mod queue; // the queue: the prompts typed while a run had the frame
pub mod rail; // the rail: the facts a user must never lose sight of
pub mod render; // the frame's drawing: every surface as rows on the theme's canvas
pub mod run; // the one place this crate owns a terminal
pub mod scroll; // where a reader is in something taller than the box holding it
pub mod seam; // the seam's light strip: the picture the seam is made of
pub mod selection; // selecting rendered rows with the mouse
pub mod settings; // `/settings`: the one config file, edited through the one schema
pub mod spill; // the box a file too large to show, or a record, is opened in
pub mod theme; // one dark theme, in truecolour
pub mod window; // the window onto the transcript

pub use app::App;
pub use face::Face;
pub use rail::Launch;
pub use run::{Card, Ending};
