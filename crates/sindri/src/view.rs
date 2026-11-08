//! What one frame holds and where each part of it goes: the transcript above, the
//! queue's row when prompts are waiting, the composer's band — the hairline a draft is
//! typed under and the draft itself — and the footer's hints along the bottom edge (§12).
//!
//! `View::paint` is the one seam a frame is drawn through. The terminal calls it on
//! the frame the real screen is painted from, and a test calls it on a frame going to
//! ratatui's headless backend, so the picture a fixture holds and the picture a reader
//! sees are produced by the same code — the point of ADR-0012, and the reason a
//! visual decision can be graded by an agent at all.
//!
//! A frame is a stack of modes, and the mode on top is what is drawn: the composer at
//! the base, and a screen over it — the `?` screen, which replaces the bands with one list
//! of the keys in force painted from the tables that answer them (#45), the `@` picker
//! (#47), the `/model` screen (#48) and the settings modal (#49), each painted from the
//! value it walks — the picker's query and rows, the served names, the settings screen's
//! keys and the value each holds — so a screen never keeps a second copy of what answers
//! it. Where a screen is up there is no draft on the frame and no caret, because a key
//! pressed then would not reach one.
//!
//! A draft that reaches `Enter` is a prompt, except when it names a `/` command: those are
//! the frame's own to answer — a conversation started again, a skill loaded, a screen
//! opened, a model written — and never a run's (§12, §5). `commands` reads the line and
//! this file acts on it, so a command is one spelling of one act whichever way it was
//! typed. What a frame cannot do itself — ask a wire, open a config, write one — it hands
//! back as an outcome and the terminal, which owns the socket and the file, does.
//!
//! The transcript's rows are `rows`' to lay out and this file's to draw: a frame is
//! handed the entries a run has written and where the reader is scrolled to, and the
//! band paints the window of them the frame reaches, which is §12's rule that only
//! on-screen rows are laid out (ADR-0008). The turn in flight is drawn under those
//! entries as one more of them — the live one, the answer and the reasoning of the
//! newest turn as they land — so a run's picture is the facts it has written and the
//! turn it is still in, and this file is the sink the wire's fragments land in. The
//! composer's band is painted from the `Composer` it is handed, and the rows the box
//! takes are the `Composer`'s to decide: this file asks rather than counts (#41). The
//! rail draws into the columns it takes from the transcript's band, and how many columns
//! those are — and what it gives up when there are fewer — is `rail`'s to decide and not
//! this file's (#46): `paint` asks it for the split and hands it the facts it was given.
//! The queue takes one row above the composer while it holds anything and none at all
//! while it holds nothing — the rows are the `Queue`'s to decide as the box's are the
//! `Composer`'s — and that row is the whole of what a prompt typed during a run is drawn
//! as, because the prompt itself is a thing the reader typed and can have back with `↑`
//! (§12, ADR-0017).
//!
//! The marker at the head of the newest answer is `pulse`'s to say in words and this
//! file's to hand to the rows it heads, so the prose around it wraps rather than being cut
//! by it (#39). The mark is this file's too, and it is painted from the span `mark` hands
//! out (#50): every paint lays the transcript's rows out — in the columns the rail left
//! them — and gathers the lines they are drawn as, keeping the span the drag covers, so
//! the highlight a reader sees on this frame and the words `Ctrl-Y` hands the terminal are
//! one value and cannot disagree (§12). Each paints into a band of this frame and changes
//! its fixture deliberately in the commit that changes it.
//!
//! The footer's row is the one piece of chrome §57 gives an order of its own: the hints in
//! survival order, the count a frame's width is owed, and any that the row has no room for
//! dropped whole rather than cut — so a narrow frame says less and never a part of
//! something.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::config::{Config, Touched};
use sindri_core::provider::Deltas;
use sindri_core::provider::served::Listing;
use sindri_core::records::Entry;
use sindri_core::run::{Load, Loaded};
use sindri_core::skills::{Catalog, Skill};

use crate::commands::{self, Command};
use crate::composer::{Composer, HAIRLINE_ROWS, Key, Outcome, Prompt, State};
use crate::graphics::CellSize;
use crate::ink::Ink;
use crate::keys::{Act, Mode, Stack};
use crate::mark::{Cell, Mark, Span};
use crate::models::Models;
use crate::picker::Picker;
use crate::picture::Surface;
use crate::pictures::Pictures;
use crate::pulse::Pulse;
use crate::queue::Queue;
use crate::rail::Facts;
use crate::rows::{Row, Rows, Scroll};
use crate::settings::Settings;
use crate::stream::Stream;

/// §12's hints in survival order, most valuable first — the order a narrower footer gives
/// them up in, from the end (#57). The last one is the table's and no width the table
/// names draws it; it is here because the order is the contract, and an order with its
/// tail shaved off could not be extended without renumbering.
const HINTS: [&str; 8] = [
    "Enter send",
    "? keys",
    "Ctrl-C cancel",
    "Ctrl-Q quit",
    "/ commands",
    "@ files",
    "↑ queue",
    "Shift+Enter newline",
];

/// What lies between two hints on the footer's row: one gap, in `dim`, the same one the
/// rail puts between its facts.
const GAP: &str = " · ";

/// §57's table as the widths it turns at and the hints a footer holds from each, widest
/// first: seven from a hundred and twenty columns, five from a hundred.
const STEPS: [(u16, usize); 2] = [(120, 7), (100, 5)];

/// The hints §12 never takes away, whichever way the frame narrows: the key that sends a
/// prompt and the key that says which keys there are. The first two of `HINTS`, held at
/// every width and given up last.
const FLOOR: usize = 2;

/// The hairline a draft is typed under: one glyph, so the composer's top edge is a row
/// of them, drawn in `dim` because it is a fact about the frame rather than a thing said.
const HAIRLINE: char = '─';

/// The columns the keys screen leaves empty at its left edge, so its rows read as one
/// block rather than as the frame's own prose.
const KEYS_MARGIN: u16 = 2;

/// The gap between a key's name and what it does on the keys screen: two columns, so the
/// descriptions of every mode line up in one column an eye can run down.
const KEYS_GAP: usize = 2;

/// What the `@` picker's screen heads itself with, in the accent ink the keys screen heads
/// its own with.
const PICKER_TITLE: &str = "@ files";

/// The label the picker's query sits behind, so a line of glyphs under the title reads as
/// what the list is narrowed by rather than as a row of it.
const PICKER_QUERY: &str = "query: ";

/// What a picker whose rows are all gone says: a screen with a query and nothing under it
/// has to say the query is what emptied it.
const PICKER_EMPTY: &str = "no file answers";

/// The glyph before the row a key would act on, and the two cells every other row keeps
/// empty so what follows lines up in one column. The picker's selection and the settings
/// modal's key are one mark, because both say "this row is the one a key acts on".
const ROW_MARK: &str = "\u{203a} ";
const ROW_GAP: &str = "  ";

/// What the settings modal heads itself with, in the accent ink the other screens head
/// their own with.
const SETTINGS_TITLE: &str = "settings";

/// The gap between a key's name and the value it holds on the settings screen: two columns,
/// so the values line up in one column an eye can run down.
const SETTINGS_GAP: usize = 2;

/// What the `/model` screen heads itself with, in the same accent ink and the same margin
/// as the picker: the two screens are one shape, and a reader who has met one has met both.
const MODELS_TITLE: &str = "/model";

/// The label the screen's query sits behind, so a line of glyphs under the title reads as
/// what the rows are ranked by rather than as a row of them.
const MODELS_QUERY: &str = "query: ";

/// What a screen whose names have not landed says: the wire was asked, and the answer is
/// the one thing the screen is for (§10).
const MODELS_ASKING: &str = "asking the wire what it serves";

/// What a screen whose query leaves no name says: a query and nothing under it means the
/// query is what emptied the list.
const MODELS_EMPTY: &str = "no served name answers";

/// The gap between a served name and the wire's own prose for it: two columns, so the prose
/// of every row starts in one column an eye can run down.
const NAMES_GAP: usize = 2;

/// One frame's worth of facts, in the shape the terminal draws them. Boot knows none
/// of them — no run, no prompt, no transcript row, nothing loaded — so the frame is its
/// chrome and nothing else; a run hands over the entries it has written and where the
/// transcript is scrolled to, a keystroke hands over the draft, and this type is where
/// the tickets after this one put the facts they do have.
#[derive(Debug, Default, Clone)]
pub struct View {
    /// The transcript's rows, one entry at a time, at the width the band is drawn at.
    rows: Rows,
    /// Where the transcript's window sits: on the newest row, or held some rows above
    /// it while facts land below (#50's arrivals count from here).
    scroll: Scroll,
    /// The draft a prompt is typed into: the composer's band is painted from this
    /// value and from nothing else (#41).
    composer: Composer,
    /// The turn in flight: the partial answer and the partial reasoning of the run's
    /// newest turn, painted under the settled entries and laid out again alone as each
    /// fragment lands (§4, §12). Empty is the case of no turn in flight, which is every
    /// frame until #43 feeds the run's deltas in here.
    stream: Stream,
    /// The prompts typed while a run was live, waiting their turn above the composer's
    /// band, oldest first: what `↑` recalls and `Ctrl-X` drops and what the run after
    /// this one is taken from (§12, ADR-0017).
    queue: Queue,
    /// The modes this frame is in, the composer at the base and any screen opened over
    /// it: what answers a key and, while a screen is up, the whole of what is painted.
    stack: Stack,
    /// The marker the newest answer wears (#39): the run's clock while it is live, the form
    /// it settled on when it stopped, and nothing at all for a launch no run has touched.
    pulse: Option<Pulse>,
    /// The facts the rail draws down the frame's right-hand columns: what the core knows
    /// about the run and what a caller hands in for the facts nothing produces yet, in
    /// the values `rail` spells them with (#46).
    rail: Facts,
    /// The mark a drag has left, if one has: two corners of the band, kept so that a
    /// redraw keeps the highlight and `Ctrl-Y` copies again with no fresh selection
    /// (§12, #50).
    mark: Option<Mark>,
    /// The span the last paint drew the highlight from, and the very cells and
    /// words `Ctrl-Y` hands out: the picture and the copy are one value (§12, #50).
    marked: Span,
    /// The directory the `@` picker walks: the one the launch stands in, which is the
    /// workspace a run's commands run in. `None` is a frame that was told no directory,
    /// and it offers no file rather than inventing one (§12).
    workspace: Option<PathBuf>,
    /// The files the picker walks up when `@` is pressed, narrowed by the query typed into
    /// the screen: empty until a screen is opened, and walked afresh each time it is.
    picker: Picker,
    /// The names the wire serves, as `/model` offers them: the listing the terminal asked
    /// for, beside the query typed into the screen and the row the selection is on (§10).
    models: Models,
    /// The skills loaded so far in this conversation, in load order: the one set a human's
    /// `/skill:name` writes to, and the set clearing the history empties (§4, §5).
    loaded: Loaded,
    /// The home the user-scope skills are read from, when the launch has one, so the frame
    /// asks the catalog the same question a run asks it (§7, ADR-0014).
    home: Option<PathBuf>,
    /// The settings modal's rows: the config's keys and the text each of them holds, seeded
    /// from the config the frame was handed when the screen opened (§12, ADR-0009).
    settings: Settings,
    /// The pictures this frame has placed, each kept by its source and the cells it took:
    /// what draws a picture over the rows its block reserved, and the cache that keeps a
    /// scroll from sending one down the wire twice (ADR-0008).
    pictures: Pictures,
    /// The pixel size of one cell on the terminal this frame is drawn on, as the launch
    /// probed it once before its event loop and handed it over (§12, ADR-0003). A picture
    /// is measured into cells against this and rasterised at its pixels, so the frame that
    /// draws the picture and the terminal that places it agree about how big one cell is.
    /// A frame nobody probed for — a fixture, or a launch with no terminal — draws at the
    /// nominal cell.
    cell_size: CellSize,
}

impl View {
    /// The frame a launch opens on: the screen has just been taken, no run has started
    /// and nothing has been typed, so the transcript is empty, the composer shows its
    /// hairline over one empty row and the footer shows the hints its width is owed.
    pub fn boot() -> View {
        View::default()
    }

    /// The frame a run is in: the entries it has written, in order, and where the
    /// reader is standing in them. What a row of an entry looks like is `rows`'
    /// business, so a caller says what happened and never how wide or in which ink.
    pub fn of(entries: impl IntoIterator<Item = Entry>, scroll: Scroll) -> View {
        let mut rows = Rows::new();
        for entry in entries {
            rows.push(entry);
        }
        View {
            rows,
            scroll,
            ..View::default()
        }
    }

    /// The frame a draft somebody typed is painted on. A fixture builds one this way,
    /// so the accepted picture of a six-row composer is made through the same value a
    /// reader's six rows are.
    pub fn typing(composer: Composer) -> View {
        View {
            composer,
            ..View::default()
        }
    }

    /// The draft this frame is drawn around.
    pub fn composer(&self) -> &Composer {
        &self.composer
    }

    /// The prompts waiting above the composer, oldest first: what the queue's row counts
    /// and what the two keys §12 gives the queue reach into.
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// A prompt sent while a run is live: it waits above the composer instead of being
    /// dropped, and it runs once the run in flight stops, in the order it was typed
    /// (§12).
    pub fn enqueue(&mut self, prompt: Prompt) {
        self.queue.push(prompt);
    }

    /// The prompt whose turn it is: what the terminal starts the next run from when the
    /// run before it stops. Nothing waiting is no run to start.
    pub fn dequeue(&mut self) -> Option<Prompt> {
        self.queue.next_to_run()
    }

    /// The turn in flight: the runs of answer and reasoning that have landed since the
    /// run's newest turn started, in the order they landed.
    pub fn stream(&self) -> &Stream {
        &self.stream
    }

    /// The turn in flight is over: what it said is written down as entries by then
    /// (ADR-0007), so the live entry it was drawn as goes and the facts take its place.
    /// A run calls this when it settles a turn and pushes that turn's entries; a frame
    /// that kept the stream would draw the same answer twice.
    pub fn settle(&mut self) {
        self.stream = Stream::new();
    }

    /// Hands this frame the directory it stands in: the workspace `@` walks, and the one
    /// the run's own commands are given. A launch reads the same directory off the process
    /// (`Launch::open`), so the picker and the run cannot disagree about where they are.
    pub fn set_workspace(&mut self, workspace: PathBuf) {
        self.workspace = Some(workspace);
    }

    /// The files the picker last walked and the query narrowing them: what the screen
    /// paints, and what a selection takes its path from.
    pub fn picker(&self) -> &Picker {
        &self.picker
    }

    /// The names the wire served and the query narrowing them: what the `/model` screen
    /// paints, and what a choice takes its name from.
    pub fn models(&self) -> &Models {
        &self.models
    }

    /// Hands the frame the names the wire serves, once the ask has landed: the screen that
    /// said it was asking paints them instead (§10, §12).
    pub fn set_served(&mut self, listing: Listing) {
        self.models.served(listing);
    }

    /// The names loaded so far in this conversation, in load order: what `/skill:name`
    /// writes to and what `/new` empties (§4, §5).
    pub fn loaded(&self) -> &Loaded {
        &self.loaded
    }

    /// Hands this frame the home its user-scope skills are read from. A launch reads the
    /// same directory off the process when it builds the catalog a run's tools are derived
    /// from, so a name the frame accepts and a name a run could call are one catalog (§7).
    pub fn set_home(&mut self, home: Option<PathBuf>) {
        self.home = home;
    }

    /// The pixel size of one cell, which is what a picture is measured and drawn at.
    pub fn cell_size(&self) -> CellSize {
        self.cell_size
    }

    /// Hands this frame the cell its terminal really has, probed once by the launch and
    /// never asked here: a frame has no terminal to ask (§12, ADR-0003). What it buys is a
    /// picture drawn at the pixels the screen will place it in, rather than at a guess the
    /// terminal then stretches.
    pub fn set_cell_size(&mut self, cell_size: CellSize) {
        self.cell_size = cell_size;
    }

    /// Hands this frame the way its terminal places a picture — nothing at all for a terminal
    /// that speaks no protocol for them, which is the case every frame fixture is in.
    pub fn set_pictures(&mut self, pictures: Pictures) {
        self.pictures = pictures;
    }

    /// The pictures this frame has placed: what a test reads a raster count off, and what a
    /// launch hands the frame once it has decided whether its terminal places one.
    pub fn pictures(&self) -> &Pictures {
        &self.pictures
    }

    /// The surface a picture in this frame is measured against: the workspace a source
    /// resolves inside, the cell it is drawn at, and whether this terminal places one.
    fn surface(&self) -> Surface {
        Surface::of(
            self.workspace.as_deref(),
            self.cell_size,
            self.pictures.draws(),
        )
    }

    /// The settings modal's rows, as they stand: what the screen paints and what `Enter`
    /// would write.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Opens the settings modal over the draft, seeded with the config as it resolved (§12):
    /// every row is a key and the value in force, and a row nobody writes into is a row the
    /// visit does not touch, so the screen can show more than it may write (ADR-0009).
    ///
    /// The door is `/settings`'s: the frame holds no config, so `/settings` comes back as
    /// `Outcome::Settings` and the terminal — which holds the config it launched with —
    /// seeds the screen here.
    pub fn open_settings(&mut self, config: &Config) {
        self.settings = Settings::of(config);
        self.stack.open(Mode::Settings);
    }

    /// The write the settings modal asked for landed: the screen closes and the draft has
    /// the keys again, because the visit is over and what it wrote is in the file (§12).
    pub fn saved(&mut self) {
        if self.stack.top() == Mode::Settings {
            self.stack.close();
        }
    }

    /// The write the settings modal asked for did not land: the screen stays open with the
    /// reason on it, naming the key a reader has to change, so a refusal is read where the
    /// value that has to change is (§11).
    pub fn refuse(&mut self, reason: &str) {
        self.settings.refuse(reason);
    }

    /// The mode on top of the stack: what this frame is showing and what will answer
    /// the next key.
    pub fn mode(&self) -> Mode {
        self.stack.top()
    }

    /// Hands the frame one key and says what the terminal is to do about it: paint the
    /// frame again, send the draft, quit, or leave a run cancelled and the draft where
    /// it was. The mode on top decides — one row of its table, or its fallback — and the
    /// terminal acts (ADR-0005).
    pub fn key(&mut self, key: Key) -> Outcome {
        let Some(act) = self.stack.act(key) else {
            // A screen that names no row for the key has nothing to say about it, which
            // is how a glyph typed at a screen stays out of the draft under it.
            return Outcome::Unchanged;
        };
        match act {
            Act::Draft => self.draft(key),
            Act::Up => self.up(),
            Act::Copy => self.copy(),
            Act::ExtendDown => self.extend(true),
            Act::ExtendUp => self.extend(false),
            Act::Drop => self.drop_newest(),
            Act::Keys => self.keys(key),
            Act::Files => self.at(key),
            Act::Query => self.query(key),
            Act::Previous => self.step(Step::Previous),
            Act::Next => self.step(Step::Next),
            Act::Pick => self.pick(),
            Act::Choose => self.choose(),
            Act::Value => self.value(key),
            Act::Flip => self.flip(),
            Act::Clear => self.clear(),
            Act::Write => self.write(),
            Act::Close => self.close(),
        }
    }

    /// A drag over the transcript band, from the cell it began on to the cell it has been
    /// dragged to: the mark a reader's pointer leaves, which the next paint draws and
    /// `Ctrl-Y` hands out (§12, #50). A second drag replaces it, because a reader marking
    /// a new run of words has stopped meaning the old one.
    pub fn drag(&mut self, anchor: Cell, head: Cell) {
        self.mark = Some(Mark::drag(anchor, head));
    }

    /// The mark this frame holds, if the reader has dragged one out: the two corners a
    /// drag left behind, which outlive the paint that drew them.
    pub fn mark(&self) -> Option<Mark> {
        self.mark
    }

    /// `Ctrl-Y`: the words the mark covers leave the frame, or the newest answer does when
    /// nothing is marked (§12, #50).
    ///
    /// What leaves is the span the last paint drew the highlight from — the very cells a
    /// reader saw marked — so a copy is never a second reading of the drag that could
    /// disagree with the picture. A key that moves the mark answers `Outcome::Edited`, and
    /// the paint that follows is where the span is refigured: the two readings are one
    /// value, one paint apart at the most.
    fn copy(&self) -> Outcome {
        if !self.marked.is_empty() {
            return Outcome::Copy(self.marked.text());
        }
        match self.rows.last_answer() {
            Some(answer) => Outcome::Copy(answer),
            None => Outcome::Unchanged,
        }
    }

    /// `Shift+↓` and `Shift+↑`: the mark grows a whole row down the transcript, or gives
    /// one back (§12, #50). A mark grows by rows and never by cells, and the anchor stays
    /// where the drag began; with no mark there is nothing to grow and nothing is painted.
    fn extend(&mut self, down: bool) -> Outcome {
        let Some(mark) = self.mark else {
            return Outcome::Unchanged;
        };
        let grown = mark.grown(down);
        if grown == mark {
            return Outcome::Unchanged;
        }
        self.mark = Some(grown);
        Outcome::Edited
    }

    /// The draft's own keys, and the one place a `/` line stops being a prompt.
    ///
    /// A line that names a command is the frame's to answer, and it is answered here rather
    /// than by the terminal because what a command comes to is what the frame holds — a
    /// screen opened, a history cleared, a set emptied — and no run ever sees it (§12, §5).
    /// A line that names no command is a prompt like any other.
    fn draft(&mut self, key: Key) -> Outcome {
        match self.composer.key(key) {
            Outcome::Send(prompt) => match commands::read(prompt.text()) {
                Some(command) => self.answer(command),
                None => Outcome::Send(prompt),
            },
            outcome => outcome,
        }
    }

    /// What one `command` comes to, in the vocabulary the terminal acts on: the frame's own
    /// answer for everything a frame holds, and an outcome for the three things it does not —
    /// a name the save writes, the wire the names are read off, and the config the settings
    /// screen is seeded from.
    fn answer(&mut self, command: Command) -> Outcome {
        match command {
            Command::New => self.start_over(),
            Command::Skill(name) => self.skill(&name),
            Command::Model => self.model(),
            Command::ModelNamed(name) => Outcome::Model(name),
            Command::Settings => Outcome::Settings,
            Command::Help => self.screen(Mode::Keys),
            Command::Quit => Outcome::Quit,
        }
    }

    /// `/new`: the conversation starts again — the transcript's rows, the turn in flight,
    /// the marker at the head of its newest answer and the mark over it are all this frame
    /// held of it — and the loaded-skill set goes with them, because a load is once per
    /// conversation, and the conversation is the history (§4, §13).
    ///
    /// The run in flight, if there is one, is left and not stopped: it was superseded
    /// rather than ended, so its session keeps saying `running` and it reaches its own stop
    /// reason in its own record (§13). `/new` writes no record at all.
    fn start_over(&mut self) -> Outcome {
        self.rows = Rows::new();
        self.scroll = Scroll::Tail;
        self.stream = Stream::new();
        self.pulse = None;
        self.mark = None;
        self.marked = Span::default();
        self.loaded.clear();
        Outcome::Edited
    }

    /// `/skill:name`, and `/skill name` with it, which is the same line (§5): one skill,
    /// loaded into the one set the conversation keeps.
    ///
    /// The catalog is discovered afresh, the way `@` walks the workspace afresh, so a
    /// `SKILL.md` written a moment ago can be named now. Which names a person may ask for is
    /// the catalog's decision and never this file's — `user-invocable: false` is a door only
    /// the model has — and a name the catalog does not offer loads nothing at all, because a
    /// name in this set is what a later model load answers `already_loaded` from: a name put
    /// there by hand would answer for a body nothing had ever handed over (§5).
    fn skill(&mut self, name: &str) -> Outcome {
        let Some(workspace) = self.workspace.clone() else {
            return Outcome::Unchanged;
        };
        let catalog = Catalog::discover(&workspace, self.home.as_deref());
        if !catalog.get(name).is_some_and(Skill::human_invocable) {
            return Outcome::Unchanged;
        }
        match self.loaded.load(name) {
            Load::Loaded => Outcome::Edited,
            // A second load is `already_loaded` and carries no body.
            Load::AlreadyLoaded => Outcome::Unchanged,
        }
    }

    /// `/model`: the screen of the names the wire serves, opened afresh and asking. A
    /// listing read a moment ago goes with the screen that showed it, because names read off
    /// a wire are evidence about the pair that was asked and about the moment it answered
    /// (§10).
    fn model(&mut self) -> Outcome {
        self.models = Models::new();
        self.stack.open(Mode::Models);
        Outcome::Served
    }

    /// `/help`: one screen opened over the draft, which is the whole of what a reader is
    /// handed — the screen answers its own keys from there (§12, ADR-0017). `/settings` is the same
    /// act and not the same call: its rows are the config's, which the frame holds no copy
    /// of, so it opens in the terminal (`Outcome::Settings`, `View::open_settings`).
    fn screen(&mut self, mode: Mode) -> Outcome {
        self.stack.open(mode);
        Outcome::Edited
    }

    /// `Enter` at the `/model` screen: the name under the selection is the model, and the
    /// screen closes. A list the query left empty has no name to choose, so the screen stays
    /// open to be widened again rather than closing on a key that did nothing.
    fn choose(&mut self) -> Outcome {
        let Some(name) = self.models.chosen().map(str::to_owned) else {
            return Outcome::Unchanged;
        };
        self.stack.close();
        Outcome::Model(name)
    }

    /// `?`: the screen that lists the keys in force.
    ///
    /// It is a key only while the draft is empty, for the reason `↑` is the queue's only
    /// on the first line: the composer is a text box, and a prompt that could not hold a
    /// question mark would be a composer that cannot ask a question. With a draft the
    /// key is a glyph like every other, and the frame's keys are a `Ctrl-C` away.
    fn keys(&mut self, key: Key) -> Outcome {
        if !self.composer.text().is_empty() {
            return self.composer.key(key);
        }
        self.stack.open(Mode::Keys);
        Outcome::Edited
    }

    /// `@`: the files of the workspace, walked afresh and offered as one screen (§12).
    ///
    /// It is a key only where it starts a word — the head of the draft, or after the
    /// space, tab or line break that leaves one — for the reason `?` is a key only while
    /// the draft is empty: this is a text box, and a prompt that could not hold
    /// `someone@host` would be a prompt that cannot name an address. An `@` inside a word
    /// is a glyph like every other, so the two ways to want both are a space before the
    /// key and typing the glyph on.
    ///
    /// A frame that was told no workspace offers no file rather than the process's own
    /// directory: what a launch stands in is handed in (`View::set_workspace`), and a
    /// picker walking somewhere else would be offering paths no run could read.
    fn at(&mut self, key: Key) -> Outcome {
        if !self.composer.at_word_start() {
            return self.composer.key(key);
        }
        let Some(root) = self.workspace.clone() else {
            return Outcome::Unchanged;
        };
        self.picker = Picker::walk(&root);
        self.stack.open(Mode::Files);
        Outcome::Edited
    }

    /// A glyph typed at a screen's filter box, or the backspace that takes one back: each
    /// screen's query is its own (`picker::Picker`, `models::Models`), and the mode on top is
    /// which one is being typed into.
    fn query(&mut self, key: Key) -> Outcome {
        let changed = match self.stack.top() {
            Mode::Models => self.models.key(key),
            _ => self.picker.key(key),
        };
        match changed {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// `↑`, `↓`, `Tab` and `Shift+Tab` at a list: the picker's and the model screen's
    /// selection walks the rows and stops at the ends, the settings modal's key walks its
    /// rows round, and whichever list is up, a key that moved nothing is a frame that is not
    /// painted.
    fn step(&mut self, step: Step) -> Outcome {
        let moved = match (self.stack.top(), step) {
            (Mode::Models, Step::Previous) => self.models.up(),
            (Mode::Models, Step::Next) => self.models.down(),
            (Mode::Settings, Step::Previous) => self.settings.up(),
            (Mode::Settings, Step::Next) => self.settings.down(),
            (_, Step::Previous) => self.picker.up(),
            (_, Step::Next) => self.picker.down(),
        };
        match moved {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// `Enter` at the picker: the chosen path goes into the draft as one shell word and the
    /// screen closes. A list the query left empty has no row to take, so there is nothing
    /// to insert and the screen stays open to be widened again — the alternative is a
    /// screen closing on a key that did nothing.
    fn pick(&mut self) -> Outcome {
        let Some(word) = self.picker.word() else {
            return Outcome::Unchanged;
        };
        self.composer.insert(&word);
        self.stack.close();
        Outcome::Edited
    }

    /// A glyph typed at the settings modal, or the backspace that takes one back: the value
    /// of the row the keys act on is the settings screen's own text, so it answers in its own
    /// map (`settings::Settings`) the way the picker's query does.
    fn value(&mut self, key: Key) -> Outcome {
        match self.settings.key(key) {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// `Space` at the settings modal: one key with two meanings. On a toggle it flips the
    /// value, and on every other key it is a glyph like any other, because a path or a header
    /// may hold a space and a screen that could not hold one would be a screen that cannot
    /// name a path (§12).
    fn flip(&mut self) -> Outcome {
        match self.settings.space() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// `Ctrl-U` at the settings modal: the row's value is emptied, and an emptied value is
    /// what the save unsets — the environment speaking again for that key (§11).
    fn clear(&mut self) -> Outcome {
        match self.settings.clear() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// `Enter` at the settings modal: the rows a reader wrote into are the patch, and the
    /// terminal writes it through the core's one save, so the frame never opens a file.
    ///
    /// A visit that touched nothing has nothing to write, and `Enter` is still the reader
    /// saying they are done: the screen closes as it would after a write, because a key of
    /// theirs that left the screen open would be a screen they have to close twice.
    fn write(&mut self) -> Outcome {
        let touched: Touched = self.settings.touched();
        if touched.is_empty() {
            self.stack.close();
            return Outcome::Edited;
        }
        Outcome::Save(touched)
    }

    /// `Esc`: the screen on top closes and the mode under it has the keys again. The
    /// composer is the base of the stack and never closes, so there the key does
    /// nothing and nothing is repainted.
    fn close(&mut self) -> Outcome {
        let closed = self.stack.top();
        self.stack.close();
        if self.stack.top() == closed {
            Outcome::Unchanged
        } else {
            Outcome::Edited
        }
    }

    /// `↑` — one key with two homes, and this is the only place it is decided (§12).
    ///
    /// On the composer's first line the thing above the caret is the queue rather than
    /// another row of the draft, so the arrow recalls the newest queued prompt; anywhere
    /// else it moves the caret a row up, which is the composer's own map, and the queue
    /// is left where it is. With nothing queued there is nothing to recall and the frame
    /// is the one it was, so the arrow is not a key that repaints for nothing.
    fn up(&mut self) -> Outcome {
        if !self.composer.on_first_line() {
            return self.composer.key(Key::Up);
        }
        match self.queue.recall() {
            Some(prompt) => {
                self.composer.recall(prompt);
                Outcome::Edited
            }
            None => Outcome::Unchanged,
        }
    }

    /// `Ctrl-X`: the newest queued prompt is dropped, and the ones behind it keep their
    /// order — the promise a recall makes is that the prompt comes back whole, and the
    /// promise this key makes is that it does not come back at all (§12).
    fn drop_newest(&mut self) -> Outcome {
        if self.queue.recall().is_none() {
            return Outcome::Unchanged;
        }
        Outcome::Edited
    }

    /// Hands the draft a pasted prompt: one draft, its line breaks kept, and no path
    /// back to something a run could take. A screen has no draft, so what a clipboard
    /// holds while one is open is dropped rather than typed where nobody can see it.
    pub fn paste(&mut self, text: &str) {
        if self.stack.modal().is_some() {
            return;
        }
        self.composer.paste(text);
    }

    /// Hands this frame the facts its rail draws: the dialect the run asks over and the
    /// model it was served, the turns against the cap, the two meters, the workspace and
    /// its branch, the skills in force, the sandbox posture, and the session's GUID and
    /// temp. They are the core's own values, so the rail cannot say about a run what the
    /// run was never handed (#46).
    pub fn set_facts(&mut self, facts: Facts) {
        self.rail = facts;
    }

    /// Says whether a run is live, which is the one fact `Ctrl-C`'s rule reads: the run
    /// loop says so when it starts and when it stops (#43), and nothing else may.
    pub fn set_state(&mut self, state: State) {
        self.composer.set_state(state);
    }

    /// Hands the frame the marker its newest answer wears (§12): a live one carrying the
    /// run's elapsed time, or the one a stopped run settled on, or nothing at all.
    ///
    /// The elapsed time is the caller's, because the clock lives in `tui` and not in a
    /// frame: a fixture can paint a marker at 4.2 s, and a settled marker's duration is the
    /// number the run stopped at rather than one read when the frame is painted (#39).
    pub fn set_pulse(&mut self, pulse: Option<Pulse>) {
        self.pulse = pulse;
    }

    /// Paints this frame into `area`: every band of it, in the ink each band is drawn
    /// in, from the top of the screen to the bottom edge.
    ///
    /// Painting is what tells the row cache how wide the transcript is, so a resize
    /// invalidates it here and the same pass reflows: the width a row was laid out at is
    /// the width a band drew it at, and no caller gets to say otherwise (ADR-0008). The
    /// composer's rows are read from the draft, so the band a draft grows into is
    /// measured when the frame is drawn and never cached.
    pub fn paint(&mut self, area: Rect, buffer: &mut Buffer) {
        // A screen is the whole frame: the bands under it are not painted, so no draft a key
        // cannot reach is on the screen pretending otherwise.
        if let Some(modal) = self.stack.modal() {
            match modal {
                Mode::Files => self.files_screen(area, buffer),
                Mode::Models => self.models_screen(area, buffer),
                Mode::Settings => self.settings_screen(area, buffer),
                // The `?` screen is what a screen with no picture of its own is drawn as.
                Mode::Composer | Mode::Keys => self.keys_screen(area, buffer),
            }
            return;
        }
        let bands = bands(area, self.composer.rows(), self.queue.rows());
        // The rail's share is taken from the transcript's band before the transcript is laid
        // out in what is left: a row's width is the width it is drawn at.
        let share = self.rail.split(bands.transcript);
        if let Some((shape, rail)) = share.rail {
            self.rail.paint(shape, rail, buffer);
        }
        self.transcript(share.transcript, buffer);
        self.queue_band(bands.queue, buffer);
        self.composer_band(bands.composer, buffer);
        self.footer(bands.footer, buffer);
    }

    /// The transcript: what the run has said so far, newest last, painted from the rows
    /// the cache holds for the window this band reaches and for no other entry. Where a
    /// reader scrolled back is told what arrived below by #50.
    ///
    /// The turn in flight is the last of the entries the window reaches and the only one
    /// laid out again as the frame is painted, which is the whole of what makes an answer
    /// arrive on screen with no relaid history around it (ADR-0008).
    fn transcript(&mut self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
        self.rows.resize(area.width);
        // How many rows a picture takes is part of the rows an entry was laid out as, so the
        // surface they were laid out on reaches the cache before a window is measured.
        self.rows.retune(self.surface());
        // Laid into the rows it heads rather than painted over them: the answer's first row is
        // wrapped around it, and the prose it introduces is never cut by the mark.
        let head = self.pulse.as_ref().map(Pulse::head);
        self.rows.live(&self.stream, head.as_ref());
        let window = self.rows.window(self.scroll, area.height);
        let mut y = area.y;
        let mut lines = Vec::new();
        for piece in window.pieces() {
            let rows = self.rows.rows(piece.entry);
            let first = usize::from(piece.skip);
            for row in &rows[first..first + usize::from(piece.rows)] {
                row.paint(
                    Rect {
                        height: 1,
                        y,
                        ..area
                    },
                    buffer,
                );
                // Placed after the row's words, because it is drawn over them: a terminal that
                // places no picture leaves the reader the words.
                place(&mut self.pictures, self.cell_size, row, area, y, buffer);
                // Gathered only when there is a mark: a mark's cells are counted in the rows
                // this band draws.
                if self.mark.is_some() {
                    lines.push(row.glyphs());
                }
                y += 1;
            }
        }
        // What this band placed is all the cache keeps: a picture scrolled past is a raster
        // nobody is looking at, and a width dragged across a range would leave one behind at
        // every size it passed through.
        self.pictures.retain_shown();
        self.marked = match self.mark {
            Some(mark) => Span::of(mark, &lines),
            None => Span::default(),
        };
        self.highlight(area, buffer);
    }

    /// Paints the mark over the rows it covers, in the ink a reader's selection is read
    /// in: the cells of the span `Ctrl-Y` hands out, so what is marked is what leaves
    /// (§12, #50).
    fn highlight(&self, area: Rect, buffer: &mut Buffer) {
        for marked in self.marked.rows() {
            let Some(y) = area.y.checked_add(marked.row()) else {
                continue;
            };
            if y >= area.bottom() {
                continue;
            }
            let left = area.x.saturating_add(marked.left());
            let right = left.saturating_add(marked.width()).min(area.right());
            for x in left..right {
                if let Some(cell) = buffer.cell_mut((x, y)) {
                    cell.set_style(Ink::Mark.style());
                }
            }
        }
    }

    /// The queue: the prompts typed while a run is live, said as one row — `3 queued` —
    /// on the frame's top row above the composer's hairline, so a reader whose `Enter`
    /// ran nothing sees that it was kept and how many are waiting in front of it (§12).
    ///
    /// It is drawn in `dim`, the ink of a fact a reader consults rather than reads, and
    /// the band is empty — no row at all — while nothing waits: the composer moves back
    /// up to where it was and the transcript takes the row.
    fn queue_band(&self, area: Rect, buffer: &mut Buffer) {
        let Some(label) = self.queue.label() else {
            return;
        };
        write(buffer, area, &[(&label, Ink::Dim)]);
    }

    /// The composer: the hairline across the band's top row and the draft's lines
    /// below it — the rows the box takes, from the line it has scrolled to. Where the
    /// cursor sits inside a line is not painted here but handed to the terminal (see
    /// `View::cursor`), because a cell of a frame is a glyph in an ink and a caret is
    /// neither.
    fn composer_band(&self, area: Rect, buffer: &mut Buffer) {
        let hairline = Rect { height: 1, ..area };
        for x in hairline.x..hairline.right() {
            if let Some(cell) = buffer.cell_mut((x, hairline.y)) {
                cell.set_char(HAIRLINE);
                cell.set_style(Ink::Dim.style());
            }
        }

        let draft = Rect {
            y: area.y.saturating_add(HAIRLINE_ROWS),
            height: area.height.saturating_sub(HAIRLINE_ROWS),
            ..area
        };
        for (row, line) in self.composer.lines().iter().enumerate() {
            let Some(offset) = u16::try_from(row).ok() else {
                break;
            };
            let Some(y) = draft.y.checked_add(offset) else {
                break;
            };
            if y >= draft.bottom() {
                break;
            }
            draft_line(buffer, Rect { y, ..draft }, line);
        }
    }

    /// Where the terminal's own cursor belongs, so a reader sees where typing lands:
    /// the caret's cell on its row of the draft, and nothing when the caret has run
    /// past the band's edge.
    ///
    /// The caret is the terminal's cursor and never a cell of the frame. An ink names a
    /// foreground and nothing else (ADR-0012), so a caret drawn as a cell would be a
    /// letter tinted `accent` — invisible on the space it usually sits on — and a
    /// reverse-video block cannot be written into a fixture at all.
    pub fn cursor(&self, area: Rect) -> Option<(u16, u16)> {
        // No draft is on screen while one is open, so there is no cell for the caret to
        // wait on: it would be a cursor floating over a list.
        if self.stack.modal().is_some() {
            return None;
        }
        let bands = bands(area, self.composer.rows(), self.queue.rows());
        let (row, column) = self.composer.caret();
        let y = bands
            .composer
            .y
            .checked_add(HAIRLINE_ROWS)?
            .checked_add(row)?;
        let x = area.x.checked_add(column)?;
        (column < area.width && y < bands.composer.bottom()).then_some((x, y))
    }

    /// The `?` screen: every key in force, the mode that answers first at the top and
    /// the modes under it after — because the top one is where the reader is, and the
    /// rest are why their keys are not doing anything.
    ///
    /// It is painted from the tables themselves rather than from prose about them, so
    /// the screen and the keys cannot disagree, and a mode added later is a variant, a
    /// table and a heading here with nothing else to remember (§12).
    fn keys_screen(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
        let modes: Vec<Mode> = self.stack.modes().collect();
        let name_width = modes
            .iter()
            .flat_map(|mode| mode.table())
            .map(|binding| binding.name.chars().count() + KEYS_GAP)
            .max()
            .unwrap_or(0);
        let mut y = area.y;
        screen_row(buffer, area, y, &[("? keys", Ink::Accent)]);
        for mode in modes {
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            screen_row(buffer, area, y, &[(mode.name(), Ink::Dim)]);
            for binding in mode.table() {
                y = y.saturating_add(1);
                if y >= area.bottom() {
                    return;
                }
                let name = format!("{:<name_width$}", binding.name);
                screen_row(
                    buffer,
                    area,
                    y,
                    &[(&name, Ink::Muted), (binding.does, Ink::Plain)],
                );
            }
        }
    }

    /// The `@` picker: the query the list is narrowed by, then the rows it leaves nearest
    /// first, the selection marked. The marker is drawn rather than the row tinted alone,
    /// because a row painted the accent ink tells a reader that something here is chosen and
    /// not which row it is on a list of eight.
    ///
    /// A path wider than the frame is cut at its right edge instead of dropping the row:
    /// §12's "whole or it is not drawn" is a rule about hints and facts, and a reader
    /// pointing at a path wants the row it is nearest to, not silence.
    fn files_screen(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
        let mut y = area.y;
        screen_row(buffer, area, y, &[(PICKER_TITLE, Ink::Accent)]);
        y = y.saturating_add(1);
        if y < area.bottom() {
            screen_row(
                buffer,
                area,
                y,
                &[(PICKER_QUERY, Ink::Dim), (self.picker.query(), Ink::Muted)],
            );
        }
        let rows = self.picker.rows();
        if rows.is_empty() {
            y = y.saturating_add(1);
            if y < area.bottom() {
                screen_row(buffer, area, y, &[(PICKER_EMPTY, Ink::Dim)]);
            }
            return;
        }
        for (at, path) in rows.iter().enumerate() {
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            let (marker, ink) = match at == self.picker.selected() {
                true => (ROW_MARK, Ink::Accent),
                false => (ROW_GAP, Ink::Plain),
            };
            cut_row(buffer, screen_band(area), y, &[(marker, ink), (path, ink)]);
        }
    }

    /// The `/model` screen: the query the names are ranked by, then the names themselves,
    /// the selection marked and the wire's own prose beside each identifier so a reader
    /// chooses between names they can read (§10, §12).
    ///
    /// A wire that has not answered yet is one line saying it was asked, because the screen
    /// opens at once and the names arrive when they arrive; a wire that could not answer is
    /// §10's hint in that same place, never a refusal, since a model named by hand still
    /// runs.
    fn models_screen(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
        let mut y = area.y;
        screen_row(buffer, area, y, &[(MODELS_TITLE, Ink::Accent)]);
        y = y.saturating_add(1);
        if y < area.bottom() {
            screen_row(
                buffer,
                area,
                y,
                &[(MODELS_QUERY, Ink::Dim), (self.models.query(), Ink::Muted)],
            );
        }
        let rows = self.models.rows();
        let said = match self.models.hint() {
            _ if self.models.waiting() => Some(MODELS_ASKING),
            Some(hint) => Some(hint),
            None if rows.is_empty() => Some(MODELS_EMPTY),
            None => None,
        };
        if let Some(said) = said {
            y = y.saturating_add(1);
            if y < area.bottom() {
                screen_row(buffer, area, y, &[(said, Ink::Dim)]);
            }
            return;
        }
        let widest = rows
            .iter()
            .map(|(id, _)| id.chars().count() + NAMES_GAP)
            .max()
            .unwrap_or(0);
        for (at, (id, label)) in rows.iter().enumerate() {
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            let (marker, ink) = match at == self.models.selected() {
                true => (ROW_MARK, Ink::Accent),
                false => (ROW_GAP, Ink::Plain),
            };
            let name = format!("{id:<widest$}");
            cut_row(
                buffer,
                screen_band(area),
                y,
                &[(marker, ink), (&name, ink), (label, Ink::Dim)],
            );
        }
    }

    /// The settings modal: every key of the config file as one row, the value it holds beside
    /// it, the row a key would act on marked — and, when a write was refused, the reason
    /// under the list, naming the key that has to change (§11, §12).
    ///
    /// The screen is painted from the rows `settings` walks rather than from a list of its
    /// own, so a key the schema gains is a row here with nothing to remember, and a value
    /// that is too wide for the frame is cut at its right edge rather than dropping the row:
    /// §12's "whole or it is not drawn" is a rule about hints and facts, and a reader fixing a
    /// value wants the key they are fixing.
    fn settings_screen(&self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(area, Ink::Plain.style());
        let key_width = Config::KEYS
            .iter()
            .map(|key| key.chars().count() + SETTINGS_GAP)
            .max()
            .unwrap_or(0);
        let mut y = area.y;
        screen_row(buffer, area, y, &[(SETTINGS_TITLE, Ink::Accent)]);
        for (at, (key, text)) in self.settings.rows().enumerate() {
            y = y.saturating_add(1);
            if y >= area.bottom() {
                return;
            }
            let (marker, name_ink, value_ink) = match at == self.settings.current() {
                true => (ROW_MARK, Ink::Accent, Ink::Accent),
                false => (ROW_GAP, Ink::Muted, Ink::Plain),
            };
            let name = format!("{:<key_width$}", key);
            cut_row(
                buffer,
                screen_band(area),
                y,
                &[(marker, name_ink), (&name, name_ink), (text, value_ink)],
            );
        }
        let Some(refusal) = self.settings.refusal() else {
            return;
        };
        // A blank row under the list, so the refusal reads as a verdict on the visit and not
        // as the last key's value.
        y = y.saturating_add(2);
        if y >= area.bottom() {
            return;
        }
        cut_row(buffer, screen_band(area), y, &[(refusal, Ink::Err)]);
    }

    /// The footer: the hints §12's table gives a frame this wide, on the bottom edge, in
    /// survival order and separated by one gap.
    fn footer(&self, area: Rect, buffer: &mut Buffer) {
        let mut spans: Vec<(&str, Ink)> = Vec::new();
        for (index, hint) in kept(area.width).into_iter().enumerate() {
            if index > 0 {
                spans.push((GAP, Ink::Dim));
            }
            spans.push((hint, Ink::Muted));
        }
        write(buffer, area, &spans);
    }
}

/// The frame as the sink of the turn in flight: a fragment of answer or of reasoning
/// lands in this frame's stream, and the next paint draws it (§4). This is the door #43
/// takes — the run is handed the frame and streams into it — and it is why a fragment
/// never has to be written down to be seen (ADR-0007).
impl Deltas for View {
    fn text(&mut self, fragment: &str) {
        self.stream.text(fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.stream.reasoning(fragment);
    }
}

/// Which way `↑` and `↓` move the picker's selection, said once so the two rows of its
/// table are one arm of the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// The row above the selection.
    Previous,
    /// The row below it.
    Next,
}

/// The bands a frame is cut into, top to bottom.
struct Bands {
    /// Everything above the composer: the transcript's rows, and the columns `rail`
    /// takes from them, or the row it takes below a hundred columns (#46).
    transcript: Rect,
    /// The queue's row, when anything is waiting above the composer — none at all while
    /// the queue holds nothing, so the band has no rows to give.
    queue: Rect,
    /// The composer's rows of draft, with the hairline on the first of them.
    composer: Rect,
    /// The footer's single row of hints, on the bottom edge.
    footer: Rect,
}

/// Places the picture `row` stands on, if it stands on one, over the block of cells the
/// transcript reserved for it.
///
/// A block the band does not hold whole is not placed at all: a placement writes into every row
/// of the block, and a picture half inside the transcript's band would be drawn over the
/// composer under it. What is left is the row's own words, which is the honest picture of a
/// picture that is only half on the screen (§12).
///
/// It is a free function because the rows it is told about are borrowed from the cache the
/// frame owns: a method would ask for the frame twice.
fn place(
    pictures: &mut Pictures,
    cell: CellSize,
    row: &Row,
    area: Rect,
    y: u16,
    buffer: &mut Buffer,
) {
    let Some(picture) = row.picture() else {
        return;
    };
    let block = Rect {
        x: area.x,
        y,
        width: picture.cols(),
        height: picture.rows(),
    };
    if block.right() > area.right() || block.bottom() > area.bottom() {
        return;
    }
    pictures.place(picture, cell, block, buffer);
}

/// Cuts `area` into its four bands. The chrome is measured up from the bottom edge and
/// the transcript takes what is left, because the footer holds one row, the queue one
/// row while anything waits, and the composer as many as the draft asks for (two to six),
/// while the transcript is the band a run grows into (§12).
fn bands(area: Rect, composer_rows: u16, queue_rows: u16) -> Bands {
    let footer = area.height.min(1);
    let above = area.height.saturating_sub(footer);
    let composer = composer_rows.min(above);
    let queue = queue_rows.min(above.saturating_sub(composer));
    let transcript = above.saturating_sub(composer).saturating_sub(queue);
    let queue_y = area.y.saturating_add(transcript);
    Bands {
        transcript: Rect {
            height: transcript,
            ..area
        },
        queue: Rect {
            y: queue_y,
            height: queue,
            ..area
        },
        composer: Rect {
            y: queue_y.saturating_add(queue),
            height: composer,
            ..area
        },
        footer: Rect {
            y: queue_y.saturating_add(queue).saturating_add(composer),
            height: footer,
            ..area
        },
    }
}

/// Writes one row of a screen at the screen's margin, each span in its own ink. Like the
/// footer's hints, a span that would run past the frame's right edge is not drawn at
/// all: a screen's row is whole or it is not there.
fn screen_row(buffer: &mut Buffer, area: Rect, y: u16, spans: &[(&str, Ink)]) {
    write(
        buffer,
        Rect {
            y,
            ..screen_band(area)
        },
        spans,
    );
}

/// The columns a screen's rows are drawn in: the frame inset by the margin that makes them
/// read as one block rather than as the frame's own prose.
fn screen_band(area: Rect) -> Rect {
    let margin = KEYS_MARGIN.min(area.width);
    Rect {
        x: area.x.saturating_add(margin),
        width: area.width.saturating_sub(margin),
        height: 1,
        ..area
    }
}

/// Writes one draft line across a row from the band's left edge, cut at its right
/// edge: a line wider than the frame is prose that runs on, not a hint that has to be
/// whole — §12's "whole or it is not drawn" is a rule about chrome — so it is cut
/// rather than dropped.
fn draft_line(buffer: &mut Buffer, area: Rect, line: &str) {
    cut_row(buffer, area, area.y, &[(line, Ink::Plain)]);
}

/// Writes spans of text across one row from its left edge, each in its own ink, and cuts the
/// last of them at the row's right edge — the writer a line of prose gets, where `write` is
/// the one chrome gets.
fn cut_row(buffer: &mut Buffer, area: Rect, y: u16, spans: &[(&str, Ink)]) {
    let mut x = area.x;
    for (text, ink) in spans {
        for glyph in text.chars() {
            if x >= area.right() {
                return;
            }
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_char(glyph);
                cell.set_style(ink.style());
            }
            x += 1;
        }
    }
}

/// The hints a footer `width` columns wide draws, in survival order: as many as §57's
/// table names for that width, and none the row has no room for.
///
/// The two rules cut in one direction. The table decides how many hints a width is owed;
/// the row decides whether the next of them fits, counted with the gap that introduces it
/// so a hint that goes never leaves its gap behind. A hint that does not fit ends the row
/// rather than being skipped for a shorter one further down the order: a footer that
/// dropped `Ctrl-C cancel` to keep `@ files` would have given a key up in the wrong
/// direction, and the order is the whole of what the table says.
fn kept(width: u16) -> Vec<&'static str> {
    let gap = GAP.chars().count();
    let mut kept: Vec<&'static str> = Vec::new();
    let mut taken = 0;
    for hint in HINTS.iter().take(wanted(width)) {
        let room = hint.chars().count() + if kept.is_empty() { 0 } else { gap };
        if taken + room > usize::from(width) {
            break;
        }
        taken += room;
        kept.push(hint);
    }
    kept
}

/// How many hints §57's table owes a footer `width` columns wide: seven from a hundred and
/// twenty, five from a hundred, and never fewer than the two §12 never takes away.
fn wanted(width: u16) -> usize {
    STEPS
        .iter()
        .find(|(from, _)| width >= *from)
        .map_or(FLOOR, |(_, hints)| *hints)
}

/// Writes spans of text across a row from its left edge, each in its own ink, and never
/// half of one: §12 gives a hint up whole or not at all, so a span that would not fit in
/// what is left of the row is not drawn rather than cut.
fn write(buffer: &mut Buffer, area: Rect, spans: &[(&str, Ink)]) {
    let mut x = area.x;
    for (text, ink) in spans {
        let width = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
        if x.saturating_add(width) > area.right() {
            return;
        }
        for glyph in text.chars() {
            if let Some(cell) = buffer.cell_mut((x, area.y)) {
                cell.set_char(glyph);
                cell.set_style(ink.style());
            }
            x += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    //! The one thing a golden frame cannot hold: the cell the terminal's own cursor is
    //! put on. A fixture is a buffer of glyphs and inks (ADR-0012) and a caret is
    //! neither, so where the caret goes is proved here and painted by the terminal.

    use super::*;

    /// A screen `cols` by `rows`, at the origin: the area a frame is painted into.
    fn screen(cols: u16, rows: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: cols,
            height: rows,
        }
    }

    /// The footer's row of a frame `cols` wide and `rows` tall, painted through the same
    /// `paint` the terminal is handed: the bottom row of the screen, read back as the
    /// glyphs a reader sees.
    fn footer(cols: u16, rows: u16) -> String {
        let mut view = View::boot();
        let area = screen(cols, rows);
        let mut buffer = Buffer::empty(area);
        view.paint(area, &mut buffer);
        let y = area.bottom() - 1;
        (area.x..area.right())
            .map(|x| buffer.cell((x, y)).expect("a cell of the frame").symbol())
            .collect()
    }

    #[test]
    fn a_hundred_and_twenty_columns_hold_the_seven_hints_the_table_names() {
        assert_eq!(
            footer(120, 24).trim_end(),
            "Enter send · ? keys · Ctrl-C cancel · Ctrl-Q quit · / commands · @ files · ↑ queue",
            "§57's table gives a 120-column footer seven hints, in survival order and \
             separated by one gap"
        );
    }

    #[test]
    fn five_hints_are_kept_from_a_hundred_columns_and_ninety_nine_keeps_two() {
        assert_eq!(
            footer(100, 24).trim_end(),
            "Enter send · ? keys · Ctrl-C cancel · Ctrl-Q quit · / commands",
            "a hundred columns is the narrowest frame §57 gives five hints, so the two \
             that go are the queue's and the picker's"
        );
        assert_eq!(
            footer(99, 24).trim_end(),
            "Enter send · ? keys",
            "one column under it and the footer is back to the two the floor holds"
        );
    }

    #[test]
    fn sixty_columns_keep_the_two_hints_no_width_takes_away() {
        assert_eq!(
            footer(60, 24).trim_end(),
            "Enter send · ? keys",
            "the floor is the floor at sixty columns as it is at ninety-nine"
        );
    }

    #[test]
    fn no_hint_is_cut_mid_word_at_any_width_between_twenty_and_two_hundred() {
        for width in 20..=200u16 {
            let drawn = footer(width, 24);
            let row = drawn.trim_end();
            let parts: Vec<&str> = if row.is_empty() {
                Vec::new()
            } else {
                row.split(GAP).collect()
            };

            assert_eq!(
                parts.as_slice(),
                &HINTS[..parts.len()],
                "at {width} columns the footer reads {row:?}, which is not a run of whole \
                 hints from the front of the table"
            );
        }
    }

    #[test]
    fn the_first_two_hints_are_drawn_at_every_width_from_twenty_columns() {
        for width in 20..=200u16 {
            let row = footer(width, 24);
            let hints: Vec<&str> = row.trim_end().split(GAP).collect();

            assert_eq!(
                hints.get(..FLOOR),
                Some(&HINTS[..FLOOR]),
                "at {width} columns the footer reads {:?}: the two hints §12 never takes \
                 away are drawn whole at every width, and 60 is not a special case of that",
                row.trim_end()
            );
        }
    }

    #[test]
    fn a_hint_the_row_has_no_room_for_is_not_drawn_at_all() {
        // Narrower than any frame a footer is drawn at, the row wins over the floor: a hint is
        // whole or not drawn, and the gap that would have introduced it goes with it.
        assert_eq!(
            footer(15, 24).trim_end(),
            "Enter send",
            "fifteen columns hold the first hint and neither the gap nor the second"
        );
        assert_eq!(
            footer(8, 24).trim_end(),
            "",
            "eight columns hold no hint at all, which is what the floor gives way to"
        );
    }

    #[test]
    fn the_terminal_cursor_lands_on_the_carets_row_of_the_draft() {
        let composer = {
            let mut composer = Composer::new();
            composer.paste("first line\nsecond line");
            composer
        };
        let view = View::typing(composer);
        let area = screen(40, 12);

        assert_eq!(view.composer().rows(), 3);
        assert_eq!(view.cursor(area), Some((11, 10)));
    }

    #[test]
    fn a_screen_has_no_caret_for_the_terminal_to_put_anywhere() {
        let mut view = View::boot();
        let area = screen(40, 12);
        assert!(
            view.cursor(area).is_some(),
            "the empty draft's caret has a cell before the screen opens"
        );

        view.key(Key::Char('?'));

        assert_eq!(
            view.cursor(area),
            None,
            "a screen is the whole frame and has no draft in it, so the caret is not left \
             floating over a list"
        );
    }

    #[test]
    fn a_caret_run_past_the_bands_edge_has_no_cell_to_wait_on() {
        let view = View::typing({
            let mut composer = Composer::new();
            composer.paste(&"x".repeat(60));
            composer
        });

        assert_eq!(
            view.cursor(screen(40, 12)),
            None,
            "column 60 is off a 40-column frame"
        );
        assert_eq!(view.cursor(screen(80, 12)), Some((60, 10)));
    }
}
