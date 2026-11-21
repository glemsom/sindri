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
use sindri_core::run::{Load, Stop, Turn};
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
use crate::mark::{Cell, Line, Mark, Span};
use crate::models::Models;
use crate::palette::{Band, Edge, Palette};
use crate::pen::{self, Clip};
use crate::picker::Picker;
use crate::picture::Surface;
use crate::pictures::Pictures;
use crate::plane::{Planes, Shade};
use crate::pointer::{self, Notch, Pointer};
use crate::pulse::{self, Pulse};
use crate::queue::Queue;
use crate::rail;
use crate::rows::{Fence, INDENT, Measure, Row, Rows, Scroll, Sideways};
use crate::settings::Settings;
use crate::size::Sizes;
use crate::splash;

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
    /// A fence standing under the mark, which is the one frame the sideways keys walk.
    Fence,
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

/// The hints a frame can honour, in survival order, most valuable first: the order the last row of
/// the band gives them up in. `End` is not among them: the row above the composer names it wherever
/// the frame stands off the tail.
const HINTS: [Hint; 10] = [
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
        key: "Shift+→",
        meaning: Meaning::Words("the fence on"),
        when: When::Fence,
    },
    Hint {
        key: "Shift+←",
        meaning: Meaning::Words("the fence back"),
        when: When::Fence,
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

/// The heavier cell a head row carries while a run is live: the pulse travels the row's own columns.
const EDGE_HEAD: char = '━';

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
    shades: Vec<Shade>,
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
            shades: Vec::new(),
            caret: None,
        }
    }

    /// The cell this frame places the terminal's cursor in.
    pub fn caret(&self) -> Option<Position> {
        self.caret
    }

    /// The bands this pass places a plane under, as the shading the terminal is handed for them:
    /// two passes holding the same shading are the same bands.
    pub fn shades(&self) -> &[Shade] {
        &self.shades
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
    /// The band images this frame holds, each against the shading of its last pass: a pass that
    /// shades the same bands makes none.
    planes: Planes,
    composer: Composer,
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
    /// The fence the mark stands over in the frame the last pass drew, and none while it covers no
    /// line of one: the fence a copy takes whole, and the one the sideways keys walk where the band
    /// cut a line of it.
    fence: Option<MarkedFence>,
    /// The workspace's files, walked once and read by the count the head hangs on its far edge.
    files: Files,
    picker: Picker,
    models: Models,
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

/// The fence a mark stands over: the entry it is drawn in, the block it is a line of, whether the
/// band cut a line of it — so there is something past the edge to walk to — and whether the mark
/// stands wholly on it, which is what makes a copy take the fence rather than the cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MarkedFence {
    entry: usize,
    fence: usize,
    cut: bool,
    whole: bool,
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
            planes: Planes::default(),
            composer: Composer::new(),
            queue: Queue::default(),
            conversation: Conversation::default(),
            superseded: false,
            stack: Stack::default(),
            models: Models::new(),
            facts: Facts::default(),
            read: ReadMark::default(),
            mark: None,
            fence: None,
            files: Files::default(),
            picker: Picker::default(),
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

    /// The turn in flight is over: what it said is written down as entries, so the turn and the
    /// live entry it was drawn as go, and the splash screen with them. A run starting now is asked
    /// under the conversation that stands, so whatever `/new` superseded is behind it.
    pub fn settle(&mut self) {
        self.rows.retire();
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

    /// The band images this frame holds, and how many it has made.
    pub fn planes(&self) -> &Planes {
        &self.planes
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
            Act::SidewaysBack => self.sideways(Sideways::Back),
            Act::SidewaysForward => self.sideways(Sideways::Forward),
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

    /// `Ctrl-Y`: the words the mark covers leave the frame, or the newest answer does when nothing
    /// is marked. A mark on a fence takes the fence's source rather than the cells, because no band
    /// draws its line breaks; one on a folded prompt takes every word the reader typed.
    fn copy(&self) -> Outcome {
        if let Some(marked) = self.fence.filter(|marked| marked.whole) {
            // A fence with no line drawn yet has nothing to hand back — and never the cells its
            // name is drawn in.
            let source = self
                .rows
                .source(marked.entry, marked.fence)
                .unwrap_or_default();
            return match source.is_empty() {
                true => Outcome::Unchanged,
                false => Outcome::Copy(source),
            };
        }
        let span = self.drawn.span();
        if !span.is_empty() {
            if let Some(entry) = self.covered(span)
                && let Some(words) = self.rows.folded_prompt(entry)
            {
                return Outcome::Copy(words);
            }
            return Outcome::Copy(span.text());
        }
        match self.rows.last_answer() {
            Some(answer) => Outcome::Copy(answer),
            None => Outcome::Unchanged,
        }
    }

    /// The one entry every row of `span` is drawn from, and none where the mark crosses from one
    /// entry into another: a copy takes a folded prompt whole only where the mark stands on it.
    fn covered(&self, span: &Span) -> Option<usize> {
        let mut one = None;
        for marked in span.rows() {
            let entry = self.drawn.entry(marked.row())?;
            if one.is_some_and(|seen| seen != entry) {
                return None;
            }
            one = Some(entry);
        }
        one
    }

    /// `Ctrl-O`: the fold of the newest settled work or of the newest prompt that keeps rows back, a
    /// frame to paint where it moved and nothing where it did not.
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

    /// `Shift+←` and `Shift+→`: the fence the mark stands on walks sideways, every row of it and no
    /// other, so the reader reaches the rest of a line the band cut.
    fn sideways(&mut self, way: Sideways) -> Outcome {
        let Some(marked) = self.fence else {
            return Outcome::Unchanged;
        };
        match self.rows.sideways(marked.entry, marked.fence, way) {
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
    /// falls to the band the words are typed in.
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
        self.facts = self.facts.clone().pulsed(None);
        self.mark = None;
        self.fence = None;
        self.drawn = Drawn::none();
        self.facts.loaded().clear();
        self.conversation.clear();
        self.superseded = true;
        Outcome::Edited
    }

    /// `/skill:name` and `/skill name` are one line: one skill into the one set the launch keeps,
    /// discovered afresh. A name put in this set by hand would answer `already_loaded` to a later
    /// model load for a body nothing had ever handed over.
    fn skill(&mut self, name: &str) -> Outcome {
        let Some(workspace) = self.facts.workspace().map(Path::to_path_buf) else {
            return Outcome::Unchanged;
        };
        let catalog = Catalog::discover(&workspace, self.facts.home());
        if !catalog.get(name).is_some_and(Skill::human_invocable) {
            return Outcome::Unchanged;
        }
        match self.facts.loaded().load(name) {
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
        // A run may have written a file since the last walk, and the head counts against this one
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

    /// A band of the transcript in one direction: the rows the band was drawn in, and the row the
    /// notice takes while it stands over it.
    fn page(&mut self, by: Page) -> Outcome {
        let rows = self.page_rows();
        self.travel(by, rows)
    }

    /// The transcript `rows` rows in one direction: moved where there is a row to move it to, never
    /// further back than the band's first row and never on past the tail, and repainting nothing
    /// when it moves nothing. A page key and a notch of the wheel both come through this one move.
    fn travel(&mut self, by: Page, rows: u32) -> Outcome {
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
        // The words the turn was drawing are rows once the record holds them: one word in one row.
        self.rows.retire();
        for entry in fresh {
            self.rows.push(entry);
        }
        self.facts = self.facts.clone().asked();
    }

    pub fn facts(&self) -> &Facts {
        &self.facts
    }

    /// What the terminal's pointer report is answered with: a notch moves the transcript three rows
    /// and a press chooses the row it landed on.
    pub fn pointer(&mut self, painted: &Painted, report: Pointer) -> Outcome {
        if !self.capabilities.reports_pointer() {
            return Outcome::Unchanged;
        }
        match report {
            Pointer::Wheel(notch) => self.notch(notch),
            Pointer::Press(at) => self.pressed(painted, at),
        }
    }

    /// A notch of the wheel, moved the `pointer::ROWS` rows a notch moves where the page key it is
    /// read through moves a band: taken only where that key is, so a card over the frame takes neither.
    fn notch(&mut self, notch: Notch) -> Outcome {
        let key = match notch {
            Notch::Up => Key::PageUp,
            Notch::Down => Key::PageDown,
        };
        match self.stack.act(key) {
            Some(Act::Back) => self.travel(Page::Back, pointer::ROWS),
            Some(Act::Forward) => self.travel(Page::Forward, pointer::ROWS),
            _ => Outcome::Unchanged,
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
        let Some(entry) = drawn.entry(row) else {
            return Outcome::Unchanged;
        };
        if self.pressed_fold(drawn, entry, row) {
            return Outcome::Edited;
        }
        let Some(entry) = drawn.first(row) else {
            return Outcome::Unchanged;
        };
        match self.marked(drawn, entry, band.width) {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    /// The fold the press on row `row` of entry `at` moves, where that row is one a fold answers to:
    /// the one row a settled work is drawn as, the row counting a prompt's rows kept back, and the
    /// row a prompt's words begin on.
    fn pressed_fold(&mut self, drawn: &Drawn, at: usize, row: u16) -> bool {
        let Some(rows) = drawn.rows_of(at..at + 1) else {
            return false;
        };
        let Some(row) = row.checked_sub(rows.start) else {
            return false;
        };
        self.rows.press(at, row)
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
    /// drew. Painting is what hands the row cache the band's measure, so a measure that moved drops
    /// every row here and the same pass lays them again; the composer's rows are measured as it draws.
    pub fn paint(&mut self, area: Rect, buffer: &mut Buffer) -> Painted {
        let painted = self.draw(area, buffer);
        self.planes.hold(painted.shades());
        painted
    }

    /// The pass itself, drawn into `area` and handed back with what it told the terminal, before the
    /// images its bands stand on are held against it.
    fn draw(&mut self, area: Rect, buffer: &mut Buffer) -> Painted {
        let mut painted = Painted::none();
        pen::claim(buffer, area, self.capabilities.palette().ground(None));
        // The rail is a fact about the frame, so its columns are cut from the transcript before the
        // bands are laid and the draft is measured at the width its band fills.
        let share = self.split(area);
        self.composer.set_room(View::room(area));
        let mut layout = bands(
            area,
            share.bands,
            self.composer.rows(),
            self.queue.rows(),
            self.composer.ends(),
            self.notice_rows(),
        );
        layout.rail = share.rail_band(area, layout.composer.y, &mut layout.transcript);
        // The band the rail's plane is drawn under, which the transcript claims as the terminal's
        // own ground once its rows are down.
        let mut under = None;
        if let Some((shape, band)) = layout.rail {
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
            if let Some(shade) = rail::paint(
                self.facts.rail(),
                shape,
                band,
                self.facts.pulse(),
                self.capabilities.pictures().cell(),
                buffer,
                &self.capabilities.palette(),
            ) {
                under = Some(shade.area());
                painted.shades.push(shade);
            }
            if let (Some(picture), Some(cells)) = (&picture, band.picture) {
                let cell = self.capabilities.cell();
                self.capabilities
                    .pictures_mut()
                    .place(picture, cell, cells, buffer);
            }
        }
        let drawn = self.transcript(
            layout.transcript,
            under,
            buffer,
            &mut painted.links,
            &mut painted.sizes,
        );
        if self.facts.splash() {
            splash::paint(
                layout.transcript,
                buffer,
                &self.capabilities.palette(),
                &mut painted.links,
                &mut painted.sizes,
            );
        }
        self.notice_band(layout.notice, buffer, &drawn);
        if let Some(shade) = self.composer_band(layout.composer, layout.queue, buffer) {
            painted.shades.push(shade);
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
                .settle(layout.transcript, buffer, &self.capabilities.palette());
            self.highlight(layout.transcript, buffer, drawn.span());
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
        self.highlight(layout.transcript, buffer, drawn.span());
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

    /// What the frame gives the rail. A splash screen is drawn with a ledger beside it or with no
    /// rail at all: a strip is a live run's one row, and no run is live at the splash screen.
    fn split(&self, area: Rect) -> Share {
        if self.facts.splash() && Rail::of(area.width) != Rail::Ledger {
            return Share {
                bands: area,
                rail: None,
            };
        }
        ledger::split(self.facts.rail(), area)
    }

    /// The transcript is drawn in the well, the darkest of the grounds a frame lays down. The turn in
    /// flight is the one entry laid out again on every paint; every other row comes from the window
    /// of the cache this band reaches.
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
        let reflowed = self.rows.measure(Measure::new(
            area.width,
            self.surface(),
            self.facts.ground(),
        ));
        // Laid into the rows it heads, so the prose the marker introduces is never cut by it. It
        // heads the turn in flight's answer while that has said a word or the run is live, and the
        // newest answer the record holds once the run is over — a band holding none gets a row.
        let head = self.facts.pulse().map(Pulse::head);
        let own = self.facts.live() || self.rows.says_answer() || !self.rows.holds_answer();
        let (recorded, streamed) = match own {
            true => (None, head),
            false => (head, None),
        };
        self.rows.heading(recorded);
        self.rows.live(streamed.as_ref());
        let total = self.rows.height();
        // A window measured from the tail follows a run that grows; one the reader took back keeps
        // the rows it holds, so a row that lands is given back to the count rather than rolled
        // under them — and a narrower band's extra rows are no row landing.
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
        self.fence = None;
        let mut y = area.y;
        let mut lines = Vec::new();
        let mut lined = Vec::new();
        let mut strays = false;
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
                if let Some(mark) = self.mark {
                    lines.push(Line::of(row.glyph_columns(), row.cut(area.width)));
                    let here = row.fence_id().map(|fence| (piece.entry, fence));
                    let line = row.fence();
                    lined.push((
                        line.map(|line| (piece.entry, line.id())),
                        line.map_or(0, Fence::gutter),
                    ));
                    let band = y.saturating_sub(area.y);
                    if (mark.first()..=mark.last()).contains(&band) {
                        self.fence = match self.fence {
                            None => fence_under(piece.entry, rows, row, area.width).map(|fence| {
                                MarkedFence {
                                    whole: !strays,
                                    ..fence
                                }
                            }),
                            Some(marked) => Some(MarkedFence {
                                whole: marked.whole && here == Some((marked.entry, marked.fence)),
                                ..marked
                            }),
                        };
                        strays |= here.is_none();
                    }
                }
                y += 1;
            }
            drawn.push(Laid {
                entry: piece.entry,
                band: top..y.saturating_sub(area.y),
                head: piece.skip == 0,
            });
        }
        // Set now the rows and their pictures are down, so a row standing on a band of its own never
        // paints over the seam the rail's plane widens across, and never the ink it wrote there.
        if let Some(cells) = under.map(|under| area.intersection(under)) {
            pen::beneath(buffer, cells, palette.ground(None));
        }
        // What this band placed is all the cache keeps: a picture scrolled past is a raster
        // nobody is looking at.
        self.capabilities.pictures_mut().retain_shown();
        // The highlight is the fence a copy takes whole, and the drag itself wherever it takes none.
        let taken = match self.fence {
            Some(marked) if marked.whole => extent(&lined, (marked.entry, marked.fence)),
            _ => None,
        };
        let span = match self.mark {
            Some(mark) => Span::of(widened(mark, taken), &lines),
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

    /// The row saying which prompt the frame stands on, where, how many rows landed below it and the
    /// key that follows the run again: a narrow frame gives the prompt up before it gives the row up,
    /// which is whole or not drawn at all.
    fn notice_band(&mut self, area: Rect, buffer: &mut Buffer, drawn: &Drawn) {
        let Some(place) = self.notice() else {
            return;
        };
        // A band of no rows stands on the composer's head row: the row is given up, never drawn
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

    /// The composer's band drawn, and the shading it stands on where this terminal places a picture
    /// and the palette spells a pixel: its rows are shaded from its head down to the rail's own tone,
    /// cell by cell where no plane stands under them, one claim per row, so no cell loses its own ink.
    fn composer_band(&self, band: Rect, queue: Rect, buffer: &mut Buffer) -> Option<Shade> {
        let palette = self.capabilities.palette();
        let caret = self.caret(band, queue, None).map(|(x, _)| x);
        let shade = self
            .capabilities
            .pictures()
            .cell()
            .and_then(|cell| Shade::composer(band, cell, caret, &palette));
        match shade.is_some() {
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
        self.composer_head(band, buffer);
        shade
    }

    /// The draft, its placeholder, the rows it keeps back and the queue's prompts, each drawn from the
    /// band's first column: the gutter its sigil stands in sets every row's words in one column.
    fn composer_inside(&self, band: Rect, queue: Rect, buffer: &mut Buffer) {
        let ends = self.composer.ends();
        let top = band.y.saturating_add(ends.above);
        let draft = Rect {
            y: top,
            height: draft_end(band, queue, ends.below).saturating_sub(top),
            ..band
        };
        let prompts = queue;
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

    /// The band's head row: the light a band meets the frame on, and the one rule it is drawn as
    /// where the palette draws no edge, a card covers the band or the window is away. The draft's
    /// count hangs on the row's far edge over either, and a live run's pulse walks it.
    fn composer_head(&self, band: Rect, buffer: &mut Buffer) {
        if band.height == 0 {
            return;
        }
        let palette = self.capabilities.palette();
        let look = self.composer_edge();
        let row = Rect { height: 1, ..band };
        let count = self.draft_count();
        let counted = pen::width(&count);
        // The count hangs on the far edge only where the row keeps a column of its own: a frame too
        // narrow for the whole count draws none of it, and the rule keeps the row.
        let hung = pen::hanging(row, counted)
            .filter(|first| *first > row.x)
            .map(|_| counted)
            .unwrap_or(0);
        let room = Rect {
            width: row.width.saturating_sub(hung),
            ..row
        };
        match self.head_lit() {
            true => pen::claim(buffer, row, palette.edged(Edge::Lit)),
            false => pen::rule(buffer, room, HAIRLINE, look, &palette),
        }
        if self.facts.live() && room.width > 0 {
            let step = self.edge_step() % u64::from(room.width);
            let at = room
                .x
                .saturating_add(pen::columns(usize::try_from(step).unwrap_or(0)));
            pen::glyph(buffer, at, row.y, EDGE_HEAD, look, &palette);
        }
        if hung > 0 {
            pen::hang(buffer, row, &count, &palette);
        }
    }

    /// Whether the band takes the keys: no screen stands over it and the window is the one being read.
    fn band_takes_keys(&self) -> bool {
        self.stack.modal().is_none() && self.facts.focus() == Focus::Focused
    }

    /// Whether the head row is claimed in the lit edge rather than ruled in the dim one: a band that
    /// takes the keys in a palette that draws light.
    fn head_lit(&self) -> bool {
        self.band_takes_keys() && self.capabilities.palette().draws_edge()
    }

    /// The draft's count as the head hangs it: every part of the bill in the muted ink a fact's
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

    /// The step the head row's pulse stands on: the pulse's own, and the first of the cycle while no
    /// live pulse has said anything yet.
    fn edge_step(&self) -> u64 {
        match self.facts.pulse() {
            Some(Pulse::Live { elapsed_ms }) => pulse::step(*elapsed_ms),
            Some(Pulse::Settled { .. }) | None => 0,
        }
    }

    /// The look the pulse and the rule of the composer's head row are drawn in: the accent at rest,
    /// and the dim edge while the band takes no keys.
    fn composer_edge(&self) -> Look {
        let palette = self.capabilities.palette();
        match self.band_takes_keys() {
            true => palette.accent_edge(),
            false => palette.dim_edge(),
        }
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
        let ends = self.composer.ends();
        let (row, column) = self.composer.caret();
        let y = band.y.checked_add(ends.above)?.checked_add(row)?;
        // A caret past the band's last column waits on that column, the way a terminal keeps the
        // cursor at its right margin rather than giving it up.
        let x = band
            .x
            .checked_add(pen::cells(INDENT).saturating_add(column))?
            .min(band.right().saturating_sub(1));
        if band.width == 0 || y >= draft_end(band, queue, ends.below) {
            return None;
        }
        match card.is_some_and(|card| card.contains(Position::new(x, y))) {
            true => None,
            false => Some((x, y)),
        }
    }

    /// The columns the draft's words have: the frame's own width, which its band fills, less the
    /// gutter an answer is set from.
    fn room(area: Rect) -> u16 {
        area.width.saturating_sub(pen::cells(INDENT))
    }

    /// The facts the screen draws: the ones the frame holds, and the ones the record wrote. A
    /// frame with no record has no screen, because what a run spent is the record's to say.
    fn stats(&self) -> ledger::Facts {
        let Some(document) = self.facts.record() else {
            return ledger::Facts::default();
        };
        self.facts.rail().clone().recorded(document)
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
            When::Fence => self.fence.is_some_and(|marked| marked.cut),
        }
    }

    /// The hints' row: the band's last row, which stands under no rail and so is the whole of what
    /// the frame holds in it.
    fn footer(&self, area: Rect, buffer: &mut Buffer) {
        if area.height == 0 {
            return;
        }
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
        self.rows.text(fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.rows.reasoning(fragment);
    }
}

/// The direction the transcript moves: back towards its first row, or on towards the tail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Back,
    Forward,
}

/// Where a paint pass put each part of the frame: the bands it shared the rows out in, the cells it
/// gave the transcript, the rail it stood above the composer, and the body it gave a card when one
/// was drawn. The band the caret is placed in is this pass's, and never a second laying out of it.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    /// The rows the transcript was drawn in, in the columns the rail left it.
    pub transcript: Rect,
    /// The rows the queue was given under the draft and above the hints, and none while nothing waits.
    pub queue: Rect,
    /// The one row saying how far back from the tail the frame stands, and none at the tail.
    pub notice: Rect,
    /// The band's own rows, in the frame's own columns: its head, the draft under it, the queue's
    /// rows and its footer.
    pub composer: Rect,
    /// The band's last row, in the frame's own columns: the footer, where the hints of what a key
    /// does this frame stand.
    pub footer: Rect,
    /// The cells a card's body was drawn in, and none while no card was drawn.
    pub card: Option<Rect>,
    /// The rail's shape and the band it drew in above the composer, and none while the frame drew
    /// no rail.
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

/// Every band a frame lays in `area`, with the transcript and the notice in `beside`'s columns:
/// the composer, its queue and the hints hold every column of the frame.
fn bands(
    area: Rect,
    beside: Rect,
    composer_rows: u16,
    queue_rows: u16,
    ends: Ends,
    notice_rows: u16,
) -> Layout {
    let composer = composer_rows.min(area.height);
    let queue = queue_rows.min(area.height.saturating_sub(composer));
    let room = area.height.saturating_sub(composer).saturating_sub(queue);
    let notice = notice_rows.min(room.saturating_sub(1));
    let transcript = room.saturating_sub(notice);
    let notice_y = area.y.saturating_add(transcript);
    let composer_y = notice_y.saturating_add(notice);
    let band_bottom = composer_y.saturating_add(composer);
    // A queued prompt stands under the draft and above the footer, so the footer is the band's last
    // row and the queue's rows are the ones just above it.
    let queue_y = match queue {
        0 => band_bottom,
        _ => band_bottom.saturating_sub(ends.below),
    };
    let rows = composer.saturating_add(queue);
    Layout {
        transcript: Rect {
            height: transcript,
            ..beside
        },
        queue: Rect {
            y: queue_y,
            height: queue,
            ..area
        },
        notice: Rect {
            y: notice_y,
            height: notice,
            ..beside
        },
        composer: Rect {
            y: composer_y,
            height: rows,
            ..area
        },
        footer: Rect {
            y: composer_y.saturating_add(rows).saturating_sub(1),
            height: u16::from(rows > 1),
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

/// The first and last band rows that are lines of `fence`, and the columns its bar leaves: a band
/// read row by row as the fence each row is a line of and the gutter it stands behind. Those rows
/// run above and below the one the drag was made on, and they are the rows a copy takes whole.
fn extent(
    lined: &[(Option<(usize, usize)>, u16)],
    fence: (usize, usize),
) -> Option<(u16, u16, u16)> {
    let mut taken: Option<(u16, u16, u16)> = None;
    for (band, (here, gutter)) in lined.iter().enumerate() {
        if *here != Some(fence) {
            continue;
        }
        let band = u16::try_from(band).unwrap_or(u16::MAX);
        taken = Some(match taken {
            None => (band, band, *gutter),
            Some((first, _, wide)) => (first, band, wide.max(*gutter)),
        });
    }
    taken
}

/// The mark a fence the drag stands wholly on is taken as: every row of it the band drew, from the
/// columns its bar leaves to the band's edge, so the highlight is the fence a copy hands back.
fn widened(mark: Mark, taken: Option<(u16, u16, u16)>) -> Mark {
    match taken {
        Some((first, last, gutter)) => {
            Mark::drag(Cell::new(first, gutter), Cell::new(last, u16::MAX))
        }
        None => mark,
    }
}

/// The fence the row `at` of `entry` stands in, where one does, and whether the band cut a line of
/// it: the rows a sideways key walks, and the sign that there is anything past the edge to walk to.
fn fence_under(entry: usize, rows: &[Row], at: &Row, room: u16) -> Option<MarkedFence> {
    let fence = at.fence_id()?;
    Some(MarkedFence {
        entry,
        fence,
        cut: rows
            .iter()
            .any(|line| line.line_of(fence) && line.cut(room)),
        whole: true,
    })
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
