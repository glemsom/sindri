//! A frame is drawn through one seam, `View::paint`, onto whatever buffer it is handed:
//! the terminal's screen or a test's headless backend, so a golden fixture and the
//! picture a reader sees are produced by the same code.

use std::ops::Range;
use std::path::Path;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use sindri_core::config::{Config, Touched};
use sindri_core::conversation::Conversation;
use sindri_core::provider::Deltas;
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;
use sindri_core::run::{Load, Loaded, Stop, Turn};
use sindri_core::skills::{Catalog, Skill};

use crate::band;
use crate::bill::Bill;
use crate::capability::Capabilities;
use crate::card;
use crate::commands::{self, Command};
use crate::composer::{Composer, Ends, Key, Outcome, PLACEHOLDER, Prompt, State};
use crate::draft;
use crate::face;
use crate::facts::{Facts, Focus};
use crate::files::Files;
use crate::graphics::CellSize;
use crate::ink::{Ink, Look};
use crate::kept::{Kept, Move};
use crate::keys::{self, Act, Body, Mode, Stack};
use crate::ledger::{self, Rail, RailBand, Share};
use crate::link::Links;
use crate::list::ScreenRow;
use crate::mark::{Cell, Mark, Span};
use crate::models::Models;
use crate::palette::{Band, Palette};
use crate::pen::{self, Clip};
use crate::picker::Picker;
use crate::picture::Surface;
use crate::pictures::Pictures;
use crate::plane::Plane;
use crate::pointer::{Notch, Pointer};
use crate::pulse::{self, Pulse};
use crate::queue::Queue;
use crate::rail;
use crate::rows::{INDENT, Row, Rows, Scroll};
use crate::settings::Settings;
use crate::size::Sizes;
use crate::splash;
use crate::stream::Stream;

#[derive(Clone, Copy)]
struct Hint {
    key: &'static str,
    meaning: Meaning,
    when: When,
}

#[derive(Clone, Copy)]
enum Meaning {
    Words(&'static str),
    /// The queue's count, which only the frame knows.
    Depth,
}

/// What a hint needs of the frame it stands in: a key that answers nothing here is no hint.
#[derive(Clone, Copy, PartialEq, Eq)]
enum When {
    Always,
    Draft(State),
    Queue,
}

impl Hint {
    fn words<'a>(&self, depth: &'a str) -> &'a str {
        match self.meaning {
            Meaning::Words(words) => words,
            Meaning::Depth => depth,
        }
    }

    fn cells(&self, depth: &str) -> u16 {
        pen::cells(self.key) + 1 + pen::cells(self.words(depth))
    }
}

/// The hints a frame can honour, in survival order, most valuable first: the order a narrower footer
/// gives them up in. `End` is not among them: the row above the composer names it wherever the frame
/// stands off the tail.
const HINTS: [Hint; 8] = [
    Hint {
        key: "Enter",
        meaning: Meaning::Words("send"),
        when: When::Draft(State::Idle),
    },
    Hint {
        key: "Ctrl-C",
        meaning: Meaning::Words("cancel"),
        when: When::Draft(State::Running),
    },
    Hint {
        key: "↑",
        meaning: Meaning::Depth,
        when: When::Queue,
    },
    Hint {
        key: "/",
        meaning: Meaning::Words("commands"),
        when: When::Always,
    },
    Hint {
        key: "@",
        meaning: Meaning::Words("files"),
        when: When::Always,
    },
    Hint {
        key: "?",
        meaning: Meaning::Words("keys"),
        when: When::Always,
    },
    Hint {
        key: "Ctrl-Q",
        meaning: Meaning::Words("quit"),
        when: When::Always,
    },
    Hint {
        key: "Ctrl-J",
        meaning: Meaning::Words("newline"),
        when: When::Always,
    },
];

const GAP: &str = " · ";

/// The ink a gap between two hints is drawn in: quieter than the key beside it, and the same on the
/// row that says where the frame stands.
const SEPARATOR: Look = Look::plain(Ink::Dim);

const HAIRLINE: char = '─';

/// The box a composer's band closes into: a corner at each end of its edge and of its foot, and the
/// upright down each side that the draft's words are set between.
const EDGE_OPEN: char = '┌';
const EDGE_CLOSE: char = '┐';
const FOOT_OPEN: char = '└';
const FOOT_CLOSE: char = '┘';
const UPRIGHT: char = '│';

/// The two words the band's edge says: what the band is for, and never what a key does.
const PROMPT_EDGE: &str = "prompt";
const RUNNING_EDGE: &str = "running";

/// The heavier cell the rule carries while a run is live: the head travels the rule's own columns.
const EDGE_HEAD: char = '━';

/// The band inside its box: the one column an upright takes at each side is not a column of the
/// draft, and the two are never a row's.
fn inside(band: Rect) -> Rect {
    Rect {
        x: band.x.saturating_add(1),
        width: band.width.saturating_sub(2),
        ..band
    }
}

/// The first row of the band the draft may not be drawn in: the queue's first row while prompts
/// wait, and the rows the band keeps under the draft when none do.
fn draft_end(band: Rect, queue: Rect, below: u16) -> u16 {
    queue.y.min(band.bottom().saturating_sub(below))
}

/// The name the commands card carries in its top border, in the shape `? keys` and `@ files`
/// wear theirs.
const COMMANDS_CARD: &str = "/ commands";

/// What one paint pass drew of the transcript, kept so the keys that follow are answered from the
/// frame the reader was shown rather than from a second laying out of it.
#[derive(Clone, Debug, Default)]
pub struct Drawn {
    band: u16,
    total: Option<u32>,
    laid: Vec<Laid>,
    span: Span,
}

impl Drawn {
    /// The record of a pass that drew no transcript: what an act replacing the transcript leaves.
    pub fn none() -> Drawn {
        Drawn::default()
    }

    /// The rows of transcript the pass drew in: what a page moves by, and how far back the deepest
    /// row may be taken.
    pub fn band(&self) -> u16 {
        self.band
    }

    /// The height the whole transcript held when the pass laid it out, and `None` where no pass has
    /// laid one out: what a row landing since the last one is given back against.
    pub fn total(&self) -> Option<u32> {
        self.total
    }

    /// The cells the mark covered, read off the rows the pass drew and never off the transcript.
    pub fn span(&self) -> &Span {
        &self.span
    }

    /// The entry laid on the band's row `row`, which is the only entry a mark grown over that row
    /// can open the fold of.
    pub fn entry(&self, row: u16) -> Option<usize> {
        self.laid
            .iter()
            .find(|laid| laid.band.contains(&row))
            .map(|laid| laid.entry)
    }

    /// The entry whose first row the band's row `row` is: the row a press acts on.
    fn first(&self, row: u16) -> Option<usize> {
        self.laid
            .iter()
            .find(|laid| laid.head && laid.band.start == row)
            .map(|laid| laid.entry)
    }

    /// The transcript's row the band's first row stands on, and none for a pass that drew no band.
    fn top(&self, scroll: Scroll) -> Option<u32> {
        let total = self.total()?;
        Some(
            total
                .saturating_sub(u32::from(self.band))
                .saturating_sub(scroll.back()),
        )
    }

    /// The rows the entries `of` were drawn in, and none where the band drew none of them.
    fn rows_of(&self, of: Range<usize>) -> Option<Range<u16>> {
        let drawn: Vec<&Laid> = self
            .laid
            .iter()
            .filter(|laid| of.contains(&laid.entry))
            .collect();
        Some(drawn.first()?.band.start..drawn.last()?.band.end)
    }
}

/// What one paint pass tells the terminal and what it drew: where it laid the frame out, the cell
/// it places the terminal's cursor in, the destinations the frame names, the runs it draws at more
/// than one cell, the rows a press chooses, and the transcript's own record. No cell carries any.
#[derive(Debug)]
pub struct Painted {
    links: Links,
    sizes: Sizes,
    presses: Vec<Press>,
    drawn: Drawn,
    layout: Option<Layout>,
    planes: Vec<Plane>,
    caret: Option<Position>,
}

impl Painted {
    /// A pass that has told the terminal nothing.
    pub fn none() -> Painted {
        Painted {
            links: Links::none(),
            sizes: Sizes::none(),
            presses: Vec::new(),
            drawn: Drawn::none(),
            layout: None,
            planes: Vec::new(),
            caret: None,
        }
    }

    /// The cell this frame places the terminal's cursor in.
    pub fn caret(&self) -> Option<Position> {
        self.caret
    }

    /// The planes this frame places under its bands, each carrying the identity it is held by.
    pub fn planes(&self) -> &[Plane] {
        &self.planes
    }

    /// The destinations this frame named, laid on the rows it drew.
    pub fn links(&self) -> &Links {
        &self.links
    }

    /// The runs this frame draws at more than one cell, laid on the rows it drew.
    pub fn sizes(&self) -> &Sizes {
        &self.sizes
    }

    /// What this pass drew of the transcript, which is what the frame's own keys are answered from.
    pub fn drawn(&self) -> &Drawn {
        &self.drawn
    }

    /// Where this pass drew the frame: the bands it shared out and the body it gave a card, and
    /// nothing at all for a pass that has laid no frame out yet.
    pub fn layout(&self) -> Option<Layout> {
        self.layout
    }

    /// The row a press at `at` landed on, as the list its screen keeps names it, when it landed on one.
    fn press(&self, at: Cell) -> Option<usize> {
        self.presses
            .iter()
            .find(|press| press.covers(at))
            .map(|press| press.choice)
    }
}

/// The commands card the frame keeps between keys, and the draft the reader waved it away for: the
/// card stands only for the words it was read from, and only while no dismissal holds them.
#[derive(Clone, Debug, Default)]
struct Cards {
    /// The draft the reader waved the card away for: it stays closed while the draft holds those
    /// words, and a draft that changes gives it back.
    dismissed: Option<String>,
    card: Option<commands::Card>,
}

impl Cards {
    /// The card the words `draft` hold, read again where the card the frame keeps was read from other
    /// words: a card stands only for its own words, so the row a press put the mark on stands with them.
    fn follow(&mut self, draft: &str) -> Option<&mut commands::Card> {
        let words = match draft.starts_with('/') {
            true => draft.trim(),
            false => "",
        };
        if self.card.as_ref().map(commands::Card::words) != Some(words) {
            self.card = match self.dismissed.as_deref() == Some(words) {
                true => None,
                false => commands::Card::of(words),
            };
        }
        self.card.as_mut()
    }

    /// A movement of the card's mark `way`, and whether it moved: a card with no row that way gives the
    /// key back to the draft it stands over.
    fn walk(&mut self, draft: &str, way: Move) -> bool {
        self.follow(draft).is_some_and(|card| card.walked(way))
    }

    /// The card the reader waved away: it stays closed while the draft holds those words.
    fn dismiss(&mut self, draft: &str) {
        self.card = None;
        self.dismissed = Some(draft.trim().to_owned());
    }
}

/// How far into the record's transcript a frame has read, and the session whose record it read it
/// from: `entries` is how many of them it has read, and a record of another session is read from
/// its first.
#[derive(Clone, Debug, Default)]
struct ReadMark {
    session: Option<String>,
    entries: usize,
}

#[derive(Debug, Clone)]
pub struct View {
    rows: Rows,
    scroll: Scroll,
    /// The rows that have landed below the frame since it last stood at the tail.
    landed: u32,
    /// What the last paint pass drew: what a page, a copy and a fold read rather than laying the
    /// transcript out a second time.
    drawn: Drawn,
    composer: Composer,
    stream: Stream,
    queue: Queue,
    conversation: Conversation,
    superseded: bool,
    stack: Stack,
    /// Never one fact at a time: a frame's screen follows from the value it holds whole.
    facts: Facts,
    /// How far into the record's transcript this frame has read, so the entries a pass owes it are
    /// the entries after that mark and no row is drawn twice.
    read: ReadMark,
    mark: Option<Mark>,
    /// The workspace's files, walked once and read by the count the edge hangs on its far corner.
    files: Files,
    picker: Picker,
    models: Models,
    loaded: Loaded,
    settings: Settings,
    /// The commands card the draft opens, kept between keys.
    cards: Cards,
    capabilities: Capabilities,
}

/// One entry the last paint drew, the rows of the band it stands on, and whether the band cut its
/// first row off: an entry taller than the band is drawn without the row it opens with.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Laid {
    entry: usize,
    band: Range<u16>,
    head: bool,
}

/// The record a frame draws and the row the reader stands at in it: the third of the three values a
/// frame is built from.
#[derive(Clone, Debug, Default)]
pub struct Transcript {
    entries: Vec<Entry>,
    scroll: Scroll,
}

impl Transcript {
    /// The record `entries` hold, with the frame at its tail.
    pub fn of(entries: impl IntoIterator<Item = Entry>) -> Transcript {
        Transcript {
            entries: entries.into_iter().collect(),
            scroll: Scroll::Tail,
        }
    }

    /// No record at all.
    pub fn none() -> Transcript {
        Transcript::default()
    }

    /// The same record with the frame standing `scroll` rows off its tail.
    pub fn scrolled(self, scroll: Scroll) -> Transcript {
        Transcript { scroll, ..self }
    }
}

impl View {
    /// The one place a frame is built, in a launch and in a test alike: what its terminal can do, the
    /// facts it draws, and the record it draws them from. A frame built with no facts is the splash
    /// screen, and every other screen follows from the value it is handed.
    pub fn of(capabilities: Capabilities, facts: Facts, transcript: Transcript) -> View {
        let Transcript { entries, scroll } = transcript;
        let mut rows = Rows::new();
        for entry in entries {
            rows.push(entry);
        }
        let mut view = View {
            rows,
            scroll,
            landed: 0,
            drawn: Drawn::none(),
            composer: Composer::new(),
            stream: Stream::new(),
            queue: Queue::default(),
            conversation: Conversation::default(),
            superseded: false,
            stack: Stack::default(),
            models: Models::new(),
            facts: Facts::default(),
            read: ReadMark::default(),
            mark: None,
            files: Files::default(),
            picker: Picker::default(),
            loaded: Loaded::default(),
            settings: Settings::default(),
            cards: Cards::default(),
            capabilities,
        };
        view.handed(facts);
        view
    }

    pub fn composer(&self) -> &Composer {
        &self.composer
    }

    /// How far back from the tail the transcript is drawn.
    pub fn scroll(&self) -> Scroll {
        self.scroll
    }

    /// Moves the frame to `scroll`: the tail is a frame with nothing below it to count, so the rows
    /// that landed below the one the reader left go with it.
    fn scrolled(&mut self, scroll: Scroll) {
        self.scroll = scroll;
        if scroll.is_tail() {
            self.landed = 0;
        }
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn enqueue(&mut self, prompt: Prompt) {
        self.queue.push(prompt);
    }

    pub fn dequeue(&mut self) -> Option<Prompt> {
        self.queue.next_to_run()
    }

    pub fn stream(&self) -> &Stream {
        &self.stream
    }

    /// The turns the run a draft is about to start joins, in the order they ended.
    pub fn earlier(&self) -> Vec<Turn> {
        self.conversation.turns().to_vec()
    }

    /// Keeps the prompt the run that stopped was asked with beside the answer it got, so the prompt
    /// after it joins the conversation at the turn it was asked — unless `/new` was pressed while
    /// that run was in flight, whose words belong to the conversation it emptied.
    pub fn kept(&mut self, ended: &Turn) {
        if !self.superseded {
            self.conversation.join(ended.clone());
        }
    }

    /// The turn in flight is over: what it said is written down as entries, so the live entry it
    /// was drawn as goes, and the splash screen with it. A run starting now is asked under the
    /// conversation that stands, so whatever `/new` superseded is behind it.
    pub fn settle(&mut self) {
        self.rows.retire();
        self.stream = Stream::new();
        self.scrolled(Scroll::Tail);
        self.composer.set_state(State::Idle);
        self.facts = self.facts.clone().asked();
        self.superseded = false;
    }

    pub fn picker(&self) -> &Picker {
        &self.picker
    }

    pub fn models(&self) -> &Models {
        &self.models
    }

    /// The commands card the draft opens, unless the reader waved it away for this very draft.
    pub fn card(&mut self) -> Option<&commands::Card> {
        let card = self.cards.follow(self.composer.text())?;
        Some(card)
    }

    pub fn loaded(&self) -> &Loaded {
        &self.loaded
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// The terminal a frame draws on, handed again after it is built: the settings visit that moved
    /// the config's face switch, and a test that draws one frame again on a terminal whose one fact
    /// differs. Not the way a frame is built.
    pub fn drawn_on(&mut self, capabilities: Capabilities) {
        self.capabilities = capabilities;
    }

    fn surface(&self) -> Surface {
        Surface::of(
            self.facts.workspace(),
            self.capabilities.cell(),
            self.capabilities.pictures().draws(),
            self.capabilities.sizes_text(),
        )
    }

    /// The surface Sindri's face is measured against: a frame the reader turned the face off for is
    /// handed the surface of a terminal that places no picture, so no row is reserved for one.
    fn face_surface(&self) -> Surface {
        match self.capabilities.draws_face() {
            true => self.surface(),
            false => Surface::none(),
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn open_settings(&mut self, config: &Config) {
        self.settings = Settings::of(config);
        self.stack.open(Mode::Settings);
    }

    pub fn saved(&mut self) {
        if self.stack.top() == Mode::Settings {
            self.stack.close();
        }
    }

    pub fn refuse(&mut self, reason: &str) {
        self.settings.refuse(reason);
    }

    /// The stop the run ended on, laid in the transcript where the run ended: the entry the record
    /// keeps, wearing the same word as the marker beside it, and the frame handed back to its tail
    /// where the reader is owed the ending.
    pub fn stopped(&mut self, stop: &Stop) {
        self.rows.stopped(stop.clone());
        self.scrolled(Scroll::Tail);
    }

    /// A run that could not start is the entry a stop is, and the prompt it was asked with is never
    /// lost: an empty composer takes its words back as the draft, and a draft the reader has typed
    /// since keeps its place while the prompt waits in the queue again.
    pub fn run_refused(&mut self, refusal: &str, prompt: Prompt) {
        self.settle();
        self.stopped(&Stop::Error(SecretSet::default().redact(refusal)));
        match self.composer.text().is_empty() {
            true => self.composer.recall(prompt),
            false => self.enqueue(prompt),
        }
    }

    pub fn mode(&self) -> Mode {
        self.stack.top()
    }

    pub fn key(&mut self, key: Key) -> Outcome {
        let Some(act) = self.stack.act(key) else {
            return Outcome::Unchanged;
        };
        match act {
            Act::Draft => self.draft(key),
            Act::Up => self.up(),
            Act::Copy => self.copy(),
            Act::Fold => self.fold(),
            Act::ExtendDown => self.extend(true),
            Act::ExtendUp => self.extend(false),
            Act::Drop => self.drop_newest(),
            Act::Keys => self.keys(key),
            Act::Files => self.at(key),
            Act::Query => self.typed(key),
            Act::Previous => self.step(Move::Previous),
            Act::Next => self.step(Move::Next),
            Act::Pick => self.pick(),
            Act::Choose => self.choose(),
            Act::Flip => self.flip(),
            Act::Clear => self.clear(),
            Act::Write => self.write(),
            Act::Close => self.close(),
            Act::Back => self.page(Page::Back),
            Act::Forward => self.page(Page::Forward),
            Act::Tail => self.tail(key),
            Act::Before => self.prompt(key, Move::Previous),
            Act::After => self.prompt(key, Move::Next),
        }
    }

    pub fn drag(&mut self, anchor: Cell, head: Cell) {
        self.mark = Some(Mark::drag(anchor, head));
    }

    pub fn mark(&self) -> Option<Mark> {
        self.mark
    }

    /// `Ctrl-Y`: the words the mark covers leave the frame, or the newest answer does when
    /// nothing is marked. What leaves is the span the last paint drew, not a second reading.
    fn copy(&self) -> Outcome {
        let span = self.drawn.span();
        if !span.is_empty() {
            return Outcome::Copy(span.text());
        }
        match self.rows.last_answer() {
            Some(answer) => Outcome::Copy(answer),
            None => Outcome::Unchanged,
        }
    }

    /// `Ctrl-O`: the fold of the newest settled work, a frame to paint where it moved and nothing
    /// where it did not.
    fn fold(&mut self) -> Outcome {
        match self.rows.toggle_newest() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// The mark grows a row, and the fold of the entry that row stands on opens: a row a mark is
    /// grown over is a row the reader asked to read whole.
    fn extend(&mut self, down: bool) -> Outcome {
        let Some(mark) = self.mark else {
            return Outcome::Unchanged;
        };
        let grown = mark.grown(down);
        let grew = grown != mark;
        self.mark = Some(grown);
        let opened = self.unfold(grown.head().row);
        match grew || opened {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// The fold of the band row `row`, opened if it was folded: the work the row stands on, or the
    /// result's own fold inside a work the reader already opened. A row the last paint drew nothing
    /// on, and a row of an entry with no fold, open nothing.
    fn unfold(&mut self, row: u16) -> bool {
        let Some(entry) = self.drawn.entry(row) else {
            return false;
        };
        self.rows.open(entry)
    }

    /// A draft: `↓` walks the card the words open, where the mark has a row below it, and every other key
    /// falls to the box the words are typed in.
    fn draft(&mut self, key: Key) -> Outcome {
        if key == Key::Down && self.cards.walk(self.composer.text(), Move::Next) {
            return Outcome::Edited;
        }
        match self.composer.key(key) {
            Outcome::Send(prompt) => self.sent(prompt),
            outcome => outcome,
        }
    }

    /// A draft sent: a line that names a command is answered, and one that only begins a word the
    /// commands card holds is the marked row's line instead.
    fn sent(&mut self, prompt: Prompt) -> Outcome {
        if let Some(command) = commands::read(prompt.text()) {
            return self.answer(command);
        }
        let Some(line) = self.cards.follow(prompt.text()).map(|card| card.spelling()) else {
            return Outcome::Send(prompt);
        };
        match commands::read(&line) {
            Some(command) => self.answer(command),
            None => {
                self.composer.insert(&line);
                Outcome::Edited
            }
        }
    }

    fn answer(&mut self, command: Command) -> Outcome {
        match command {
            Command::New => self.start_over(),
            Command::Skill(name) => self.skill(&name),
            Command::Model => self.model(),
            Command::ModelNamed(name) => Outcome::Model(name),
            Command::Settings => Outcome::Settings,
            Command::Help => self.screen(Mode::Keys),
            Command::Stats => self.screen(Mode::Stats),
            Command::Quit => Outcome::Quit,
        }
    }

    /// `/new`: the conversation starts again — rows, turn in flight, marker and mark — and the
    /// loaded-skill set and the turns before it go with it. A run in flight is superseded and never
    /// stopped: it reaches its own stop reason, and joins nothing.
    fn start_over(&mut self) -> Outcome {
        self.rows = Rows::new();
        // The words the record already holds are the conversation `/new` emptied, so the mark moves
        // to the document's end and no row of them comes back.
        self.read.entries = self.facts.transcript().map_or(0, <[Entry]>::len);
        self.scrolled(Scroll::Tail);
        self.stream = Stream::new();
        self.facts = self.facts.clone().pulsed(None);
        self.mark = None;
        self.drawn = Drawn::none();
        self.loaded.clear();
        self.conversation.clear();
        self.superseded = true;
        Outcome::Edited
    }

    /// `/skill:name` and `/skill name` are one line: one skill into the one set the conversation
    /// keeps, discovered afresh. A name put in this set by hand would answer `already_loaded`
    /// to a later model load for a body nothing had ever handed over.
    fn skill(&mut self, name: &str) -> Outcome {
        let Some(workspace) = self.facts.workspace().map(Path::to_path_buf) else {
            return Outcome::Unchanged;
        };
        let catalog = Catalog::discover(&workspace, self.facts.home());
        if !catalog.get(name).is_some_and(Skill::human_invocable) {
            return Outcome::Unchanged;
        }
        match self.loaded.load(name) {
            Load::Loaded => Outcome::Edited,
            Load::AlreadyLoaded => Outcome::Unchanged,
        }
    }

    fn model(&mut self) -> Outcome {
        self.handed(self.facts.clone().unserved());
        self.models = Models::new();
        self.stack.open(Mode::Models);
        Outcome::Served
    }

    fn screen(&mut self, mode: Mode) -> Outcome {
        self.stack.open(mode);
        Outcome::Edited
    }

    fn choose(&mut self) -> Outcome {
        let Some(name) = self.models.chosen().map(str::to_owned) else {
            return Outcome::Unchanged;
        };
        self.stack.close();
        Outcome::Model(name)
    }

    /// `?`, a key only while the draft is empty: a composer that could not hold a question
    /// mark would be a composer that cannot ask a question.
    fn keys(&mut self, key: Key) -> Outcome {
        if !self.composer.text().is_empty() {
            return self.composer.key(key);
        }
        self.stack.open(Mode::Keys);
        Outcome::Edited
    }

    /// `@`, a key only where it starts a word, for the reason `?` is one only on an empty draft: a
    /// prompt that could not hold `someone@host` cannot name an address, and a frame told no
    /// workspace offers no file rather than walking the process's own directory.
    fn at(&mut self, key: Key) -> Outcome {
        if !self.composer.at_word_start() {
            return self.composer.key(key);
        }
        let Some(root) = self.facts.workspace() else {
            return Outcome::Unchanged;
        };
        // A run may have written a file since the last walk, and the edge counts against this one
        // for as long as the screen stands.
        self.files = Files::walk(root);
        self.picker = Picker::over(self.files.clone());
        self.stack.open(Mode::Files);
        Outcome::Edited
    }

    /// The list the frame walks: the card the draft's words open where the screen on top names no
    /// body, and otherwise the list that screen keeps — the keys screen's are keys nothing walks,
    /// and the ledger's are laid whole.
    fn kept_list(&mut self) -> Option<&mut dyn Kept> {
        match self.stack.top().body() {
            None => Some(self.cards.follow(self.composer.text())?),
            Some(Body::Files) => Some(&mut self.picker),
            Some(Body::Models) => Some(&mut self.models),
            Some(Body::Settings) => Some(&mut self.settings),
            Some(Body::Keys | Body::Ledger) => None,
        }
    }

    fn typed(&mut self, key: Key) -> Outcome {
        View::stepped(self.kept_list().is_some_and(|list| list.typed(key)))
    }

    fn step(&mut self, way: Move) -> Outcome {
        View::stepped(self.kept_list().is_some_and(|list| list.walked(way)))
    }

    /// A frame to paint where something moved and nothing where it did not.
    fn stepped(moved: bool) -> Outcome {
        match moved {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    fn pick(&mut self) -> Outcome {
        let Some(word) = self.picker.word() else {
            return Outcome::Unchanged;
        };
        self.composer.insert(&word);
        self.stack.close();
        Outcome::Edited
    }

    /// `Space` at the settings modal: a flip on a toggle and a glyph everywhere else, because a
    /// path or a header may hold a space.
    fn flip(&mut self) -> Outcome {
        match self.settings.space() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// `Ctrl-U` at the settings modal empties the row, and an emptied value is what the save
    /// unsets: the environment speaks for that key again.
    fn clear(&mut self) -> Outcome {
        match self.settings.clear() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    fn write(&mut self) -> Outcome {
        let touched: Touched = self.settings.touched();
        if touched.is_empty() {
            self.stack.close();
            return Outcome::Edited;
        }
        Outcome::Save(touched)
    }

    fn close(&mut self) -> Outcome {
        if self.stack.modal().is_none() && self.cards.follow(self.composer.text()).is_some() {
            self.cards.dismiss(self.composer.text());
            return Outcome::Edited;
        }
        let closed = self.stack.top();
        self.stack.close();
        if self.stack.top() == closed {
            Outcome::Unchanged
        } else {
            Outcome::Edited
        }
    }

    /// `End` off the tail is the run's tail; at the tail it is the draft's own end of line.
    fn tail(&mut self, key: Key) -> Outcome {
        if self.scroll.is_tail() {
            return self.draft(key);
        }
        self.scrolled(Scroll::Tail);
        Outcome::Edited
    }

    /// A band of the transcript in one direction: the frame is moved where there is a row to move
    /// it to, and a key pressed at the first row or at the tail repaints nothing.
    fn page(&mut self, by: Page) -> Outcome {
        let rows = self.page_rows();
        let at = match by {
            Page::Back => self
                .scroll
                .back()
                .saturating_add(rows)
                .min(self.rows.deepest(self.drawn.band())),
            Page::Forward => self.scroll.back().saturating_sub(rows),
        };
        if at == self.scroll.back() {
            return Outcome::Unchanged;
        }
        self.scrolled(Scroll::of(at));
        Outcome::Edited
    }

    /// `[` and `]`: the prompt before or after the one the frame's first band row stands on. A draft
    /// holding any words is the draft's own key, exactly as `?` is.
    fn prompt(&mut self, key: Key, way: Move) -> Outcome {
        if !self.composer.text().is_empty() {
            return self.composer.key(key);
        }
        match self.prompt_scroll(way) {
            Some(scroll) if scroll != self.scroll => {
                self.scrolled(scroll);
                Outcome::Edited
            }
            _ => Outcome::Unchanged,
        }
    }

    /// Where the frame stands to put the prompt `way` prompts on from the one its first band row
    /// stands in, and none where there is no prompt that way. A prompt the rows under it are too few
    /// to lift to the band's first row leaves the frame at the tail, and the key stops there.
    fn prompt_scroll(&mut self, way: Move) -> Option<Scroll> {
        let tops = self.rows.prompts();
        let at = self.prompt_of(&self.drawn, &tops)?;
        let target = match way {
            Move::Previous => at.checked_sub(1)?,
            Move::Next => at.checked_add(1).filter(|next| *next < tops.len())?,
        };
        let total = self.drawn.total()?;
        // A frame off the tail spends one of the band's rows saying where it stands.
        let deepest = total.saturating_sub(self.page_rows().saturating_sub(1));
        Some(Scroll::of(deepest.saturating_sub(tops[target])))
    }

    /// The prompt the frame's first band row stands in, counted from zero.
    fn prompt_of(&self, drawn: &Drawn, tops: &[u32]) -> Option<usize> {
        let top = drawn.top(self.scroll)?;
        tops.iter().rposition(|row| *row <= top)
    }

    /// `prompt 2 / 7` for the prompt the frame stands in, and none where the conversation holds one
    /// prompt or none.
    fn standing(&self, drawn: &Drawn, tops: &[u32]) -> Option<String> {
        if tops.len() < 2 {
            return None;
        }
        let at = self.prompt_of(drawn, tops)?;
        Some(format!("prompt {} / {}", at.saturating_add(1), tops.len()))
    }

    /// The band the transcript is drawn in at the tail, one row more than while the row saying where
    /// the frame stands is not drawn over it: a page moves by this many rows, and a prompt is lifted
    /// to the band's first row against it.
    fn page_rows(&self) -> u32 {
        u32::from(self.drawn.band().saturating_add(self.notice_rows()))
    }

    /// `↑`: the row above in the card the draft opens, where the mark has one — and, where it has none,
    /// the line above the caret, or the newest queued prompt.
    fn up(&mut self) -> Outcome {
        if self.cards.walk(self.composer.text(), Move::Previous) {
            return Outcome::Edited;
        }
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

    fn drop_newest(&mut self) -> Outcome {
        if self.queue.recall().is_none() {
            return Outcome::Unchanged;
        }
        Outcome::Edited
    }

    pub fn paste(&mut self, text: &str) {
        if self.stack.modal().is_some() {
            return;
        }
        self.composer.paste(text);
    }

    /// The facts this frame draws, handed whole: a frame handed no facts is the splash screen, and
    /// one handed a run's is that run's frame. The draft answers a cancel out of the value, and the
    /// served names are a screen's own copy, so both are taken here and nowhere else.
    pub fn handed(&mut self, facts: Facts) {
        self.composer.set_state(match facts.live() {
            true => State::Running,
            false => State::Idle,
        });
        self.models = std::mem::take(&mut self.models).served_by(facts.served().cloned());
        if self.files.root() != facts.workspace() {
            self.files = Files::of(facts.workspace());
        }
        self.facts = facts;
        self.read_record();
    }

    /// Draws the entries the record's transcript holds that this frame has not drawn yet: the words
    /// a record wrote are the band's from then on, whatever the run in flight says after them, and
    /// the splash screen goes with the first of them.
    fn read_record(&mut self) {
        let (Some(record), Some(transcript)) = (self.facts.record(), self.facts.transcript())
        else {
            return;
        };
        // A record of another session is a transcript of its own, read from its first entry and
        // drawn under the rows already here rather than over them.
        let session = record.session.clone();
        let from = match self.read.session.as_deref() == Some(session.as_str()) {
            true => self.read.entries,
            false => 0,
        };
        let fresh = transcript[from..].to_vec();
        self.read.session = Some(session);
        self.read.entries = transcript.len();
        if fresh.is_empty() {
            return;
        }
        // The words the stream was drawing are rows once the record holds them: one word in one row.
        self.rows.retire();
        self.stream = Stream::new();
        for entry in fresh {
            self.rows.push(entry);
        }
        self.facts = self.facts.clone().asked();
    }

    pub fn facts(&self) -> &Facts {
        &self.facts
    }

    /// What the terminal's pointer report is answered with: a notch moves the transcript through the
    /// one key table, and a press chooses the row it landed on.
    pub fn pointer(&mut self, painted: &Painted, report: Pointer) -> Outcome {
        if !self.capabilities.reports_pointer() {
            return Outcome::Unchanged;
        }
        match report {
            Pointer::Wheel(Notch::Up) => self.key(Key::PageUp),
            Pointer::Wheel(Notch::Down) => self.key(Key::PageDown),
            Pointer::Press(at) => self.pressed(painted, at),
        }
    }

    /// A press on a row of a card: the row is chosen, and then the key it hangs from is sent, which
    /// is `Enter` for every row a card draws. A press on the transcript's band opens or marks what it
    /// stood on, and a card standing over the frame leaves it no row of the band to act on.
    fn pressed(&mut self, painted: &Painted, at: Cell) -> Outcome {
        let Some(layout) = painted.layout() else {
            return Outcome::Unchanged;
        };
        if layout.card.is_none() && layout.transcript.contains(Position::new(at.column, at.row)) {
            return self.pressed_row(painted.drawn(), layout.transcript, at);
        }
        let Some(row) = painted.press(at) else {
            return Outcome::Unchanged;
        };
        if !self.chosen(row) {
            return Outcome::Unchanged;
        }
        self.key(Key::Enter)
    }

    /// A press on the transcript's band, read off the rows the pass drew: the head row of a settled
    /// work folds it, an answer's first row marks the exchange it ended, and any other row is no act.
    fn pressed_row(&mut self, drawn: &Drawn, band: Rect, at: Cell) -> Outcome {
        let Some(row) = at.row.checked_sub(band.y) else {
            return Outcome::Unchanged;
        };
        let Some(entry) = drawn.first(row) else {
            return Outcome::Unchanged;
        };
        if self.folded(drawn, entry, row) {
            return Outcome::Edited;
        }
        match self.marked(drawn, entry, band.width) {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// The work entry `at` stands in, folded or opened where the press landed on its head row: a
    /// frame to paint when the band draws one row of it and the press stood there.
    fn folded(&mut self, drawn: &Drawn, at: usize, row: u16) -> bool {
        let Some(work) = self.rows.work_of(at) else {
            return false;
        };
        let Some(rows) = drawn.rows_of(work) else {
            return false;
        };
        rows.start == row && self.rows.toggle(at)
    }

    /// The exchange the answer `at` ended, marked from the prompt that asked it through the row the
    /// run ended on: a frame to paint where the band drew the answer's first row.
    fn marked(&mut self, drawn: &Drawn, at: usize, width: u16) -> bool {
        if !self.rows.is_answer(at) {
            return false;
        }
        let Some(exchange) = self.rows.exchange(at) else {
            return false;
        };
        let Some(rows) = drawn.rows_of(exchange) else {
            return false;
        };
        self.mark = Some(Mark::drag(
            Cell::new(rows.start, 0),
            Cell::new(rows.end.saturating_sub(1), width.saturating_sub(1)),
        ));
        true
    }

    /// The row a press landed on, chosen: `true` when it is a row the list the frame keeps still
    /// holds, which is the draft's own card while no screen stands over it.
    fn chosen(&mut self, at: usize) -> bool {
        self.kept_list().is_some_and(|list| list.pressed(at))
    }

    /// Paints this frame into `area`, and hands back what the pass told the terminal and where it
    /// drew. Painting is what tells the row cache how wide the transcript is, so a resize
    /// invalidates it here and the same pass reflows; the composer's rows are measured as it draws.
    pub fn paint(&mut self, area: Rect, buffer: &mut Buffer) -> Painted {
        let mut painted = Painted::none();
        pen::claim(buffer, area, self.capabilities.palette().ground(None));
        self.composer.set_room(View::room(area));
        let mut layout = bands(
            area,
            self.composer.rows(),
            self.queue.rows(),
            self.composer.ends(),
            self.notice_rows(),
        );
        // The rail's share is taken from the transcript's band before the transcript is laid out
        // in what is left: a row's width is the width it is drawn at.
        let share = self.split(layout.transcript);
        layout.transcript = share.transcript;
        layout.rail = share.rail;
        // The band the rail's plane is drawn under, which the transcript claims as the terminal's
        // own ground before it writes a word on it.
        let mut under = None;
        if let Some((shape, band)) = share.rail {
            // A ledger carries the face wherever it is drawn, and a strip is one row of facts with
            // no band to carry one in: a terminal that places no picture, or a config that turned
            // it off, is handed no cells and reserves no row.
            let picture = match shape {
                Rail::Ledger => face::over_rail(band.area.width, &self.face_surface()),
                Rail::Strip => None,
            };
            let band = match &picture {
                Some(picture) => band.with_picture(picture.cols(), picture.rows()),
                None => band,
            };
            if let Some(plane) = rail::paint(
                self.facts.rail(),
                shape,
                band,
                self.facts.pulse(),
                self.capabilities.pictures().cell(),
                buffer,
                &self.capabilities.palette(),
            ) {
                under = Some(plane.area());
                painted.planes.push(plane);
            }
            if let (Some(picture), Some(cells)) = (&picture, band.picture) {
                let cell = self.capabilities.cell();
                self.capabilities
                    .pictures_mut()
                    .place(picture, cell, cells, buffer);
            }
        }
        let drawn = self.transcript(
            share.transcript,
            under,
            buffer,
            &mut painted.links,
            &mut painted.sizes,
        );
        if self.facts.splash() {
            splash::paint(
                share.transcript,
                buffer,
                &self.capabilities.palette(),
                &mut painted.links,
                &mut painted.sizes,
            );
        }
        self.notice_band(layout.notice, buffer, &drawn);
        if let Some(plane) = self.composer_band(layout.composer, layout.queue, buffer) {
            painted.planes.push(plane);
        }
        self.footer(layout.footer, buffer);
        // A screen is drawn over the frame it was opened from rather than in place of it: the
        // reader sees the transcript they will return to, and sees that it is out of reach.
        let screen = self.stack.top();
        let Some((name, leaves, body)) = self.over(area, screen) else {
            // A heading is drawn as the cells the sizing leaves once the band it stands in is
            // known: one with no row of that band under it is the words it says.
            painted
                .sizes
                .settle(share.transcript, buffer, &self.capabilities.palette());
            self.highlight(share.transcript, buffer, drawn.span());
            self.note(&mut painted, drawn);
            painted.caret = self
                .caret(layout.composer, layout.queue, None)
                .map(|(x, y)| Position::new(x, y));
            painted.layout = Some(layout);
            return painted;
        };
        // The band under a card takes no key, so the frame under it is no way to anywhere — and no
        // heading of it is drawn large, because the scrim is read as the words it holds.
        painted = Painted::none();
        self.highlight(share.transcript, buffer, drawn.span());
        self.note(&mut painted, drawn);
        let rows = body.held();
        let bounds = card::bounds(area, rows);
        let held = card::paint(
            area,
            name,
            leaves,
            rows,
            buffer,
            self.edge(self.capabilities.palette().accent_edge()),
            &self.capabilities.palette(),
        );
        painted.presses = body.paint(held, buffer, &self.capabilities.palette());
        painted.caret = body
            .caret(held)
            .or_else(|| self.caret(layout.composer, layout.queue, Some(bounds)))
            .map(|(x, y)| Position::new(x, y));
        layout.card = Some(held);
        painted.layout = Some(layout);
        painted
    }

    /// Keeps what the pass drew for the keys it answers, and hands the same record back with what
    /// the pass told the terminal.
    fn note(&mut self, painted: &mut Painted, drawn: Drawn) {
        self.drawn = drawn.clone();
        painted.drawn = drawn;
    }

    /// The card this frame carries over itself: the screen on top of the stack, or — a draft
    /// beginning with a slash being no screen — the commands card its words open.
    fn over(
        &mut self,
        area: Rect,
        screen: Mode,
    ) -> Option<(&'static str, &'static str, ScreenBody)> {
        if let Some((name, leaves)) = screen.labels() {
            let width = card::body_width(area);
            return Some((name, leaves, self.body(screen, width)?));
        }
        let body = self.cards.follow(self.composer.text())?.body();
        let leaves = Mode::Composer.leaves()?;
        Some((COMMANDS_CARD, leaves, ScreenBody::Rows(body)))
    }

    /// The body the screen's description says it holds, built by the screen that keeps its rows at
    /// `width`, the columns a card hands a body and draws them in again: the card is as tall as this,
    /// and its own count row says which rows the frame had no room for.
    pub fn body(&self, screen: Mode, width: u16) -> Option<ScreenBody> {
        Some(match screen.body()? {
            Body::Keys => ScreenBody::Rows(self.stack.body(width)),
            Body::Files => ScreenBody::Rows(self.picker.body()),
            Body::Models => ScreenBody::Rows(self.models.body()),
            Body::Settings => ScreenBody::Rows(self.settings.body(width)),
            Body::Ledger => ScreenBody::Ledger(Sheet::laid(&self.stats(), width)),
        })
    }

    /// The band the transcript and the rail share. A splash screen is drawn with a ledger beside
    /// it or with no rail at all: a strip is a live run's one row, and no run is live at the
    /// splash screen.
    fn split(&self, band: Rect) -> Share {
        if self.facts.splash() && Rail::of(band.width) != Rail::Ledger {
            return Share {
                transcript: band,
                rail: None,
            };
        }
        ledger::split(self.facts.rail(), band)
    }

    /// The transcript is drawn in the well, the darkest of the three bands. The turn in flight is
    /// the one entry laid out again on every paint; every other row comes from the window of the
    /// cache this band reaches.
    fn transcript(
        &mut self,
        area: Rect,
        under: Option<Rect>,
        buffer: &mut Buffer,
        links: &mut Links,
        sizes: &mut Sizes,
    ) -> Drawn {
        let palette = self.capabilities.palette();
        pen::claim(buffer, area, palette.ground(Some(Band::Well)));
        if let Some(cells) = under.map(|under| area.intersection(under)) {
            pen::claim(buffer, cells, palette.ground(None));
        }
        // A narrower band lays the same history out in more rows, and that is no row landing.
        let reflowed = self.rows.cols() != area.width;
        self.rows.resize(area.width);
        // How many rows a picture takes is part of the rows an entry was laid out as, so the
        // surface reaches the cache before a window is measured.
        self.rows.retune(self.surface());
        self.rows.stand(self.facts.ground());
        // Laid into the rows it heads, so the prose the marker introduces is never cut by it. It
        // heads the turn in flight's answer while that has said a word or the run is live, and the
        // newest answer the record holds once the run is over — a band holding none gets a row.
        let head = self.facts.pulse().map(Pulse::head);
        let own = self.facts.live() || self.stream.says_answer() || !self.rows.holds_answer();
        let (recorded, streamed) = match own {
            true => (None, head),
            false => (head, None),
        };
        self.rows.heading(recorded);
        self.rows.live(&self.stream, streamed.as_ref());
        let total = self.rows.height();
        // A window measured from the tail follows a run that grows; one the reader took back keeps
        // the rows it holds, so a row that lands is given back to the count rather than rolled
        // under them.
        let grown = match self.drawn.total() {
            Some(seen) if !reflowed => total.saturating_sub(seen),
            _ => 0,
        };
        let back = match self.scroll.is_tail() {
            true => 0,
            false => self
                .scroll
                .back()
                .saturating_add(grown)
                .min(self.rows.deepest(area.height)),
        };
        self.scrolled(Scroll::of(back));
        if !self.scroll.is_tail() {
            self.landed = self.landed.saturating_add(grown);
        }
        let thumb = self.rows.thumb(self.scroll, area.height);
        let window = self.rows.window(self.scroll, area.height);
        let mut y = area.y;
        let mut lines = Vec::new();
        let mut drawn = Vec::new();
        for piece in window.pieces() {
            let top = y.saturating_sub(area.y);
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
                    &self.capabilities.palette(),
                    links,
                    sizes,
                );
                if let Some(thumb) = &thumb {
                    thumb.paint(area, y, buffer, &self.capabilities.palette());
                }
                // Written after the row's words and the thumb, so a block's first cell is the
                // picture's: a refused picture places nothing and leaves the thumb that cell.
                let cell = self.capabilities.cell();
                place(self.capabilities.pictures_mut(), cell, row, area, y, buffer);
                // Gathered only when there is a mark: a mark's cells are counted in the rows this
                // band draws.
                if self.mark.is_some() {
                    lines.push(row.glyph_columns());
                }
                y += 1;
            }
            drawn.push(Laid {
                entry: piece.entry,
                band: top..y.saturating_sub(area.y),
                head: piece.skip == 0,
            });
        }
        // What this band placed is all the cache keeps: a picture scrolled past is a raster
        // nobody is looking at.
        self.capabilities.pictures_mut().retain_shown();
        let span = match self.mark {
            Some(mark) => Span::of(mark, &lines),
            None => Span::default(),
        };
        Drawn {
            band: area.height,
            total: Some(total),
            laid: drawn,
            span,
        }
    }

    /// Paints the mark over the rows it covers, from the span `Ctrl-Y` hands out: what is
    /// marked and what leaves are one value.
    fn highlight(&self, area: Rect, buffer: &mut Buffer, span: &Span) {
        for marked in span.rows() {
            let Some(y) = area.y.checked_add(marked.row()) else {
                continue;
            };
            if y >= area.bottom() {
                continue;
            }
            let left = area.x.saturating_add(marked.left());
            let right = left.saturating_add(marked.width()).min(area.right());
            pen::claim(
                buffer,
                Rect {
                    x: left,
                    y,
                    width: right.saturating_sub(left),
                    height: 1,
                },
                Ink::Mark.style(&self.capabilities.palette()),
            );
        }
    }

    /// How many rows a frame off the tail spends saying so: the one row above the composer, and
    /// none at the tail, where there is nothing to say.
    fn notice_rows(&self) -> u16 {
        u16::from(!self.scroll.is_tail())
    }

    /// How far back from the tail the frame stands, in words: a frame at the tail has nothing to say.
    fn notice(&self) -> Option<String> {
        counted(self.scroll.back(), "back")
    }

    /// The row saying which prompt the frame stands on, where it stands, how many rows have landed
    /// below it since and the key that follows the run again. A frame too narrow for the whole of it
    /// gives the prompt up first, and a frame too narrow for the rest draws none of it: the row is
    /// whole or it is not there.
    fn notice_band(&mut self, area: Rect, buffer: &mut Buffer, drawn: &Drawn) {
        let Some(place) = self.notice() else {
            return;
        };
        // A band of no rows stands on the composer's edge row: the row is given up, never drawn
        // over the surface under it.
        if area.height == 0 {
            return;
        }
        let landed = counted(self.landed, "landed");
        let standing = {
            let tops = self.rows.prompts();
            self.standing(drawn, &tops)
        };
        let mut spans: Vec<(&str, Look)> = Vec::new();
        if let Some(standing) = standing.as_deref() {
            spans.push((standing, keys::MEANING));
            spans.push((GAP, SEPARATOR));
        }
        // The prompt is the row's first fact, so a row too narrow for it keeps the rest and gives
        // the count up whole.
        let counted_from = spans.len();
        spans.push((place.as_str(), keys::MEANING));
        if let Some(landed) = landed.as_deref() {
            spans.push((GAP, SEPARATOR));
            spans.push((landed, keys::MEANING));
        }
        spans.push((GAP, SEPARATOR));
        spans.extend(keyed("End", "follows the run"));
        if counted_from > 0 && pen::width(&spans) > area.width {
            spans.drain(..counted_from);
        }
        whole_row(buffer, area, area.y, &spans, &self.capabilities.palette());
    }

    /// The composer's band drawn, and the plane it stands on where this terminal places one and the
    /// palette spells a pixel: its rows are shaded from its head down to the rail's own tone, cell by
    /// cell where no plane stands under them, one claim per row, so no cell loses its own ink.
    fn composer_band(&self, band: Rect, queue: Rect, buffer: &mut Buffer) -> Option<Plane> {
        let palette = self.capabilities.palette();
        let caret = self.caret(band, queue, None);
        let plane = self
            .capabilities
            .pictures()
            .cell()
            .and_then(|cell| Plane::composer(band, cell, caret, &palette));
        match plane.is_some() {
            true => pen::claim(buffer, band, palette.ground(None)),
            false => {
                for row in 0..band.height {
                    pen::claim(
                        buffer,
                        Rect {
                            y: band.y.saturating_add(row),
                            height: 1,
                            ..band
                        },
                        palette.raised(row, band.height),
                    );
                }
            }
        }
        self.composer_inside(band, queue, buffer);
        self.composer_box(band, buffer);
        plane
    }

    /// The draft, its placeholder, the rows it keeps back and the queue's prompts, all drawn in the
    /// columns the box leaves them: one column in from each upright, the gutter its sigil stands in
    /// before the words.
    fn composer_inside(&self, band: Rect, queue: Rect, buffer: &mut Buffer) {
        let ends = self.composer.ends();
        let top = band.y.saturating_add(ends.above);
        let draft = Rect {
            y: top,
            height: draft_end(band, queue, ends.below).saturating_sub(top),
            ..inside(band)
        };
        let prompts = inside(queue);
        let empty = self.composer.text().is_empty();
        // The placeholder is no row of the draft, so it lights no sigil and keeps the dim ink.
        let waited = (!empty).then(|| usize::from(self.composer.caret().0));
        let scroll = self.composer.scroll();
        let rows: Vec<Vec<(&str, Ink)>> = match empty {
            true => vec![vec![(PLACEHOLDER, Ink::Dim)]],
            false => self
                .composer
                .lines()
                .into_iter()
                .enumerate()
                .map(|(row, line)| draft::runs(line, row == 0 && scroll == 0, &self.files))
                .collect(),
        };
        let mut under = draft.y;
        for (row, runs) in rows.iter().enumerate() {
            let Some(offset) = u16::try_from(row).ok() else {
                break;
            };
            let Some(y) = draft.y.checked_add(offset) else {
                break;
            };
            if y >= draft.bottom() {
                break;
            }
            let gutter = match (row == 0 && scroll == 0, waited == Some(row)) {
                (true, true) => band::Gutter::Lit,
                (true, false) => band::Gutter::Open,
                (false, _) => band::Gutter::Blank,
            };
            band::row(
                buffer,
                Rect { y, ..draft },
                gutter,
                runs,
                &self.capabilities.palette(),
            );
            under = y.saturating_add(1);
        }
        if let Some(kept) = self.composer.kept_back()
            && under < draft.bottom()
        {
            let counted = match kept {
                1 => String::from("1 row kept back"),
                kept => format!("{kept} rows kept back"),
            };
            band::row(
                buffer,
                Rect { y: under, ..draft },
                band::Gutter::Blank,
                &[(&counted, Ink::Dim)],
                &self.capabilities.palette(),
            );
        }

        if prompts.height == 0 {
            return;
        }
        let shown = self.queue.shown(prompts.height);
        let mut at = prompts.y;
        if let Some(kept) = shown.kept_back {
            let counted = format!("{kept} more queued");
            band::row(
                buffer,
                Rect { y: at, ..prompts },
                band::Gutter::Blank,
                &[(&counted, Ink::Dim)],
                &self.capabilities.palette(),
            );
            at = at.saturating_add(1);
        }
        for line in &shown.prompts {
            band::row(
                buffer,
                Rect { y: at, ..prompts },
                band::Gutter::Open,
                &[(line.as_str(), Ink::Dim)],
                &self.capabilities.palette(),
            );
            at = at.saturating_add(1);
        }
    }

    /// Closes the band into one box, drawn after its contents so no glyph of the draft stands in an
    /// upright's column: the rule a band of one row is drawn with, and otherwise a corner at each
    /// end of its edge and of its foot and an upright down each side between them.
    fn composer_box(&self, band: Rect, buffer: &mut Buffer) {
        if band.height == 0 {
            return;
        }
        let palette = self.capabilities.palette();
        let edge = Rect { height: 1, ..band };
        if band.height <= 1 || inside(band).width == 0 {
            pen::rule(buffer, edge, HAIRLINE, self.composer_edge(), &palette);
            return;
        }
        let foot = Rect {
            y: band.bottom().saturating_sub(1),
            ..edge
        };
        self.composer_edge_row(edge, buffer);
        boxed_row(
            buffer,
            foot,
            (FOOT_OPEN, FOOT_CLOSE),
            palette.dim_edge(),
            &palette,
        );
        for y in edge.y.saturating_add(1)..foot.y {
            pen::glyph(buffer, band.x, y, UPRIGHT, palette.dim_edge(), &palette);
            pen::glyph(
                buffer,
                band.right().saturating_sub(1),
                y,
                UPRIGHT,
                palette.dim_edge(),
                &palette,
            );
        }
    }

    /// The band's edge row: its word just inside the opening corner, one blank column, the rule
    /// running on to the far corner with a live run's head on one of its own columns, and the draft's
    /// count hung there. A band with no room for a whole word or count keeps its plain rule.
    fn composer_edge_row(&self, row: Rect, buffer: &mut Buffer) {
        let palette = self.capabilities.palette();
        let look = self.composer_edge();
        corners(buffer, row, (EDGE_OPEN, EDGE_CLOSE), look, &palette);
        let room = inside(row);
        let word = self.edge_word();
        let cells = pen::cells(word);
        let named = cells.saturating_add(2) <= room.width;
        let opening = match named {
            true => cells.saturating_add(1),
            false => 0,
        };
        if named {
            pen::write(
                buffer,
                Rect {
                    width: cells,
                    ..room
                },
                &[(word, look)],
                Clip::Edge,
                &palette,
            );
            pen::glyph(
                buffer,
                room.x.saturating_add(cells),
                row.y,
                ' ',
                look,
                &palette,
            );
        }
        let count = self.draft_count();
        let counted = pen::width(&count);
        // The count hangs on the far corner only where the rule keeps a column below it: a frame
        // too narrow for the whole count draws none of it, and the rule keeps the row.
        let hung = pen::hanging(room, counted)
            .filter(|first| *first > room.x.saturating_add(opening))
            .map(|_| counted)
            .unwrap_or(0);
        let rule = Rect {
            x: room.x.saturating_add(opening),
            width: room.width.saturating_sub(opening).saturating_sub(hung),
            ..room
        };
        pen::rule(buffer, rule, HAIRLINE, look, &palette);
        if self.facts.live() && rule.width > 0 {
            let step = self.edge_step() % u64::from(rule.width);
            let at = rule
                .x
                .saturating_add(pen::columns(usize::try_from(step).unwrap_or(0)));
            pen::glyph(buffer, at, row.y, EDGE_HEAD, look, &palette);
        }
        if hung > 0 {
            pen::hang(buffer, room, &count, &palette);
        }
    }

    /// The draft's count as the edge hangs it: every part of the bill in the muted ink a fact's
    /// words are drawn in, the gap between them dim, and no span at all for a draft billing nothing.
    fn draft_count(&self) -> Vec<(String, Ink)> {
        let Some(bill) = Bill::of(self.composer.text(), &self.files) else {
            return Vec::new();
        };
        let mut spans: Vec<(String, Ink)> = Vec::new();
        for (index, part) in bill.parts().into_iter().enumerate() {
            if index > 0 {
                spans.push((String::from(GAP), Ink::Dim));
            }
            spans.push((part, Ink::Muted));
        }
        spans
    }

    /// The word the band's edge says: the band only takes the draft, or a run is live.
    fn edge_word(&self) -> &'static str {
        match self.facts.live() {
            true => RUNNING_EDGE,
            false => PROMPT_EDGE,
        }
    }

    /// The step the edge's head stands on: the pulse's own, and the first of the cycle while no live
    /// pulse has said anything yet.
    fn edge_step(&self) -> u64 {
        match self.facts.pulse() {
            Some(Pulse::Live { elapsed_ms }) => pulse::step(*elapsed_ms),
            Some(Pulse::Settled { .. }) | None => 0,
        }
    }

    /// The look the composer's edge row is drawn in: the accent lit while a run is live, the accent
    /// faint while the band only waits, and the dim edge while a screen stands over the band.
    fn composer_edge(&self) -> Look {
        let palette = self.capabilities.palette();
        if self.stack.modal().is_some() {
            return palette.dim_edge();
        }
        self.edge(match self.facts.live() {
            true => palette.lit_edge(),
            false => palette.accent_edge(),
        })
    }

    /// The look an edge that takes the keys is drawn in, given what the terminal has reported about
    /// who is reading: the look it earns while the window is read, and the dim edge while it is not.
    fn edge(&self, look: Look) -> Look {
        match self.facts.focus() {
            Focus::Focused => look,
            Focus::Away => self.capabilities.palette().dim_edge(),
        }
    }

    /// The cell the draft's caret stands in: none while a screen stands over the band, because the
    /// draft behind it takes no key, and none on a cell the card a draft opens holds. A screen's own
    /// caret is its body's to place; a frame with no card, and the composer's plane's glow, ask this.
    fn caret(&self, band: Rect, queue: Rect, card: Option<Rect>) -> Option<(u16, u16)> {
        if self.stack.modal().is_some() {
            return None;
        }
        let boxed = inside(band);
        let ends = self.composer.ends();
        let (row, column) = self.composer.caret();
        let y = band.y.checked_add(ends.above)?.checked_add(row)?;
        // A caret past the box's last column waits on that column, the way a terminal keeps the
        // cursor at its right margin rather than giving it up — and never on an upright.
        let x = boxed
            .x
            .checked_add(pen::cells(INDENT).saturating_add(column))?
            .min(boxed.right().saturating_sub(1));
        if boxed.width == 0 || y >= draft_end(band, queue, ends.below) {
            return None;
        }
        match card.is_some_and(|card| card.contains(Position::new(x, y))) {
            true => None,
            false => Some((x, y)),
        }
    }

    /// The columns the draft's words have: the band's inside, less the gutter an answer is set from.
    fn room(band: Rect) -> u16 {
        inside(band).width.saturating_sub(pen::cells(INDENT))
    }

    /// The facts the screen draws: the ones the frame holds, and the ones the record wrote. A
    /// frame with no record has no screen, because what a run spent is the record's to say.
    fn stats(&self) -> ledger::Facts {
        let Some(document) = self.facts.record() else {
            return ledger::Facts::default();
        };
        self.facts
            .rail()
            .clone()
            .recorded(document, self.loaded.names())
    }

    fn hints(&self) -> Vec<Hint> {
        HINTS
            .into_iter()
            .filter(|hint| self.honours(hint.when))
            .collect()
    }

    fn honours(&self, when: When) -> bool {
        match when {
            When::Always => true,
            When::Draft(state) => self.composer.state() == state,
            When::Queue => !self.queue.is_empty(),
        }
    }

    fn footer(&self, area: Rect, buffer: &mut Buffer) {
        let depth = self.queue.label().unwrap_or_default();
        hints_row(
            buffer,
            area,
            &self.hints(),
            &depth,
            &self.capabilities.palette(),
        );
    }
}

impl Deltas for View {
    fn text(&mut self, fragment: &str) {
        self.stream.text(fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.stream.reasoning(fragment);
    }
}

/// The direction a page of the transcript moves the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Back,
    Forward,
}

/// Where a paint pass put each part of the frame: the bands it shared the rows out in, the cells it
/// gave the transcript, the rail it stood beside them, and the body it gave a card when one was
/// drawn. The band the caret is placed in is this pass's, and never a second laying out of it.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    /// The rows the transcript was drawn in, after the rail took its share.
    pub transcript: Rect,
    /// The rows the queue was given at the foot of the composer's band, and none while nothing waits.
    pub queue: Rect,
    /// The one row saying how far back from the tail the frame stands, and none at the tail.
    pub notice: Rect,
    /// The band's own rows: its edge, the draft under it, the queue's rows and its foot.
    pub composer: Rect,
    /// The frame's last row: the hints of what a key does this frame.
    pub footer: Rect,
    /// The cells a card's body was drawn in, and none while no card was drawn.
    pub card: Option<Rect>,
    /// The rail's shape and the band it drew in beside the transcript, and none while the frame
    /// drew no rail.
    pub rail: Option<(Rail, RailBand)>,
}

/// Places the picture `row` stands on over the block of cells the transcript reserved. A
/// block the band does not hold whole is not placed at all: a placement writes into every
/// row of it, and half a picture would be drawn over the composer under it.
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

fn bands(area: Rect, composer_rows: u16, queue_rows: u16, ends: Ends, notice_rows: u16) -> Layout {
    let footer = area.height.min(1);
    let above = area.height.saturating_sub(footer);
    let composer = composer_rows.min(above);
    let queue = queue_rows.min(above.saturating_sub(composer));
    let room = above.saturating_sub(composer).saturating_sub(queue);
    let notice = notice_rows.min(room.saturating_sub(1));
    let transcript = room.saturating_sub(notice);
    let notice_y = area.y.saturating_add(transcript);
    let composer_y = notice_y.saturating_add(notice);
    let band_bottom = composer_y.saturating_add(composer);
    // A queued prompt stands inside the box, under the draft and above the foot, so the foot is the
    // band's last row and the queue's rows are the ones just above it.
    let queue_y = match queue {
        0 => band_bottom,
        _ => band_bottom.saturating_sub(ends.below),
    };
    Layout {
        transcript: Rect {
            height: transcript,
            ..area
        },
        queue: Rect {
            y: queue_y,
            height: queue,
            ..area
        },
        notice: Rect {
            y: notice_y,
            height: notice,
            ..area
        },
        composer: Rect {
            y: composer_y,
            height: composer.saturating_add(queue),
            ..area
        },
        footer: Rect {
            y: band_bottom.saturating_add(queue),
            height: footer,
            ..area
        },
        card: None,
        rail: None,
    }
}

/// A screen's sheet, laid once at the width its card hands it: the rows it holds are what the card is
/// tall and what the card draws, and only the band a paint gives them caps them.
pub struct Sheet(Vec<ledger::Row>);

impl Sheet {
    /// The screen's whole sheet at `width`: every block it can draw, with no band to cap it, because a
    /// card caps the rows it draws and never the rows it is handed.
    fn laid(facts: &ledger::Facts, width: u16) -> Sheet {
        Sheet(ledger::rows(
            facts,
            ledger::Shape::Screen,
            Rect::new(0, 0, width, u16::MAX),
            None,
        ))
    }

    /// The rows the sheet holds, which the card it is drawn on is this many plus its two borders tall.
    pub fn height(&self) -> u16 {
        u16::try_from(self.0.len()).unwrap_or(u16::MAX)
    }

    /// Draws the sheet in `body`, the band the card gives it: the rows the card holds are the cap, so
    /// one past them is not drawn.
    pub fn draw(&self, body: Rect, buffer: &mut Buffer, palette: &Palette) {
        ledger::draw(&self.0, body, buffer, palette);
    }
}

/// What a screen holds: the rows it laid at the width the card hands it, which the card is this many
/// rows tall and which the card caps only as it draws them.
pub enum ScreenBody {
    Rows(Vec<ScreenRow>),
    Ledger(Sheet),
}

impl ScreenBody {
    /// How many rows the screen holds, which the card is this many plus its two borders tall.
    fn held(&self) -> u16 {
        match self {
            ScreenBody::Rows(rows) => u16::try_from(rows.len()).unwrap_or(u16::MAX),
            ScreenBody::Ledger(sheet) => sheet.height(),
        }
    }

    /// The cell a row of this body waits for the glyphs typed at it in: the first waiting row the
    /// body has room for, kept on its last column the way a terminal keeps its cursor at the margin.
    fn caret(&self, body: Rect) -> Option<(u16, u16)> {
        let ScreenBody::Rows(rows) = self else {
            return None;
        };
        if body.width == 0 || body.height == 0 {
            return None;
        }
        let waiting = rows
            .iter()
            .take(usize::from(body.height))
            .position(|row| row.caret.is_some())?;
        let x = body
            .x
            .saturating_add(rows[waiting].caret?)
            .min(body.right() - 1);
        let y = body.y.checked_add(u16::try_from(waiting).ok()?)?;
        Some((x, y))
    }

    /// Draws the screen in `body`, and never in a row the card did not give it, with the rows a press
    /// chooses laid where they were written.
    fn paint(&self, body: Rect, buffer: &mut Buffer, palette: &Palette) -> Vec<Press> {
        match self {
            ScreenBody::Rows(rows) => {
                let mut presses = Vec::new();
                for (row, y) in rows.iter().zip(body.y..body.bottom()) {
                    let area = Rect {
                        y,
                        height: 1,
                        ..body
                    };
                    pen::write(buffer, area, &row.spans, row.clip, palette);
                    if let Some(choice) = row.choice {
                        presses.push(Press { area, choice });
                    }
                }
                presses
            }
            ScreenBody::Ledger(sheet) => {
                sheet.draw(body, buffer, palette);
                Vec::new()
            }
        }
    }
}

/// One row of a card a press chooses, and the cells it was written into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Press {
    area: Rect,
    choice: usize,
}

impl Press {
    /// Whether a press at `at` landed on this row: the row it is drawn on, and a column of it.
    fn covers(&self, at: Cell) -> bool {
        at.row == self.area.y && (self.area.x..self.area.right()).contains(&at.column)
    }
}

/// Writes one row of a band, each span in its own look. A row the band's right edge has no room
/// for is not drawn at all: a hint or a fact is whole or it is not there.
fn whole_row<S: AsRef<str>, D: Copy + Into<Look>>(
    buffer: &mut Buffer,
    area: Rect,
    y: u16,
    spans: &[(S, D)],
    palette: &Palette,
) {
    pen::write(
        buffer,
        Rect {
            y,
            height: 1,
            ..area
        },
        spans,
        Clip::Whole,
        palette,
    );
}

/// One ruled row of the box: a corner at each end and the rule between them, all in one look.
fn boxed_row(
    buffer: &mut Buffer,
    row: Rect,
    (open, close): (char, char),
    look: Look,
    palette: &Palette,
) {
    corners(buffer, row, (open, close), look, palette);
    pen::rule(buffer, inside(row), HAIRLINE, look, palette);
}

fn corners(
    buffer: &mut Buffer,
    row: Rect,
    (open, close): (char, char),
    look: Look,
    palette: &Palette,
) {
    pen::glyph(buffer, row.x, row.y, open, look, palette);
    pen::glyph(
        buffer,
        row.right().saturating_sub(1),
        row.y,
        close,
        look,
        palette,
    );
}

/// One row of hints, in survival order.
fn hints_row(buffer: &mut Buffer, area: Rect, hints: &[Hint], depth: &str, palette: &Palette) {
    let mut spans: Vec<(&str, Look)> = Vec::new();
    for (index, hint) in kept(hints, area.width, depth).into_iter().enumerate() {
        if index > 0 {
            spans.push((GAP, SEPARATOR));
        }
        spans.extend(keyed(hint.key, hint.words(depth)));
    }
    pen::write(buffer, area, &spans, Clip::Whole, palette);
}

/// A count of rows in words and what they are rows of: `1 row back`, `4 rows landed`, and nothing at
/// all for none of them.
fn counted(rows: u32, of: &str) -> Option<String> {
    match rows {
        0 => None,
        1 => Some(format!("1 row {of}")),
        rows => Some(format!("{rows} rows {of}")),
    }
}

/// A key and the words that say what pressing it does, in the two looks a key and its meaning are
/// drawn in: the footer's hints and the row above the composer are drawn by one shape.
fn keyed<'a>(key: &'a str, meaning: &'a str) -> [(&'a str, Look); 3] {
    [
        (key, keys::KEY),
        (" ", keys::MEANING),
        (meaning, keys::MEANING),
    ]
}

/// The hints a footer `width` columns wide draws, in survival order, stopping at the first that
/// does not fit, gap included: a later and shorter hint is never taken in its place, because the
/// order is the whole of what the table says.
fn kept(hints: &[Hint], width: u16, depth: &str) -> Vec<Hint> {
    let gap = pen::cells(GAP);
    let mut kept: Vec<Hint> = Vec::new();
    let mut taken: u16 = 0;
    for hint in hints {
        let room = hint.cells(depth) + if kept.is_empty() { 0 } else { gap };
        if taken.saturating_add(room) > width {
            break;
        }
        taken += room;
        kept.push(*hint);
    }
    kept
}

#[cfg(test)]
mod tests;
