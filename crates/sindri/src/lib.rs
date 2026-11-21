pub mod band; // a row of the composer's band: the gutter it opens with and the runs it is drawn as
pub mod bill; // the draft's own count: the files its words name, and the command it opens with
pub mod capability; // what the terminal a frame draws on can do: its cell, its colours, its sizing, its pointer, its pictures and the face over the rail
pub mod card; // a screen's card: its border, its name, the key that leaves, over the frame it dims
pub mod commands; // the `/` commands: a draft line the frame answers itself
pub mod composer; // the draft a prompt is typed into, as a value rather than a widget
pub mod decode; // the bytes a key arrives as, answered as the key or the paste
pub mod draft; // the inks one row of the draft is drawn in: the command it opens with, the files its words name, and the spans it sets in backticks
pub mod face; // the picture the binary carries: Sindri's face, measured and never said in words
pub mod facts; // the facts a frame draws, gathered as one value and handed whole, and the screen they are drawn as
pub mod files; // the files of a workspace, walked as `git` reads them
pub mod graphics; // an SVG becomes the pixels a picture is drawn in
pub mod ink; // the look a cell is drawn in, asked for by name: its ink, its faces, its underline
pub mod kept; // the list a screen keeps: its query, its mark, the rows a query leaves, and the cap a card shows
pub mod keys; // the mode stack and one key table per mode
pub mod ledger; // the run's facts, the one way each value is spelled, and the rows they are laid out in
pub mod link; // the destination a run of cells names, written by the backend that draws the frame
pub mod list; // the list a card draws: its query, the line it says when it has nothing, one row per item
pub mod mark; // the cells a reader has marked, and the words they hold
pub mod markdown; // markdown becomes the rows a transcript entry is drawn as
pub mod mermaid; // a mermaid fence becomes a picture, or is refused with a reason the caption can print
pub mod models; // the names the wire serves, ranked as a query is typed into the screen
pub mod palette; // the colours the inks, the bands and the edges are drawn in, resolved once at boot
pub mod pen; // the one place a glyph run is written into a frame, and the one place a string's width is measured
pub mod picker; // the `@` screen: the files of the workspace, narrowed by the query typed into it
pub mod picture; // a picture an answer points at, resolved to a file inside the workspace and measured
pub mod pictures; // the pictures a frame has placed: every source the terminal was told about
pub mod plane; // the images a frame places under its bands: the pixels of a band, and the bytes a delete says
pub mod pointer; // the pointer a frame takes: the wheel's notch and the cell a press landed on
pub mod pulse; // the marker at the head of the newest answer: the braille glyph a live run turns
pub mod queue; // the prompts typed while a run is live, waiting under the draft in the order
pub mod rail; // the ledger's adapter: the rows it lays, drawn on the band it is handed
#[cfg(test)]
mod readback; // what a painted buffer holds, read back: the inks of a row and the column of a glyph
pub mod rows; // the transcript's rows: one entry of the core's records laid out as the rows it is
pub mod settings; // the config file's keys as one list, the value each of them holds
pub mod size; // the runs of a row a terminal draws at twice the cell, measured by the cells they leave
pub mod spell; // a call's arguments as the fact the tool names: the words a reader would type
pub mod splash; // the frame a launch opens on: a wordmark, what the product is, and three commands
pub mod stream; // the turn in flight: the answer and the thought of the run's newest turn, fed a
pub mod view; // what one frame holds and where each part of it goes
