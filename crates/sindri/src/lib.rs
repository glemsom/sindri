pub mod commands; // the `/` commands: a draft line the frame answers itself
pub mod composer; // the draft a prompt is typed into, as a value rather than a widget
pub mod decode; // the bytes a key arrives as, answered as the key or the paste
pub mod graphics; // an SVG becomes the pixels a picture is drawn in
pub mod ink; // the look a cell is drawn in, asked for by name: the ink it is in
pub mod keys; // the mode stack and one key table per mode
pub mod mark; // the cells a reader has marked, and the words they hold
pub mod markdown; // markdown becomes the rows a transcript entry is drawn as
pub mod mermaid; // a mermaid fence becomes a picture, or is refused with a reason the caption can print
pub mod models; // the names the wire serves, ranked as a query is typed into the screen
pub mod palette; // the colours the seven inks and the three bands are drawn in, resolved once at boot
pub mod picker; // the files of the workspace, walked with `ignore`
pub mod picture; // a picture an answer points at, resolved to a file inside the workspace and measured
pub mod pictures; // the pictures a frame has placed: every source the terminal was told about
pub mod pulse; // the marker at the head of the newest answer: the braille glyph a live run turns
pub mod queue; // the prompts typed while a run is live, waiting above the composer in the order
pub mod rail; // the run's facts down the frame's right-hand columns, read as a ledger
#[cfg(test)]
mod readback; // what a painted buffer holds, read back: the inks of a row and the column of a glyph
pub mod rows; // the transcript's rows: one entry of the core's records laid out as the rows it is
pub mod settings; // the config file's keys as one list, the value each of them holds
pub mod stats; // the facts the rail gives up, drawn whole on the frame's whole band
pub mod stream; // the turn in flight: the answer and the thought of the run's newest turn, fed a
pub mod view; // what one frame holds and where each part of it goes
