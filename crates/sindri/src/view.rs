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
use crate::palette::Palette;
use crate::pen::{self, Clip};
use crate::picker::Picker;
use crate::picture::Surface;
use crate::pictures::Pictures;
use crate::plane::{Placement, Planes};
use crate::pointer::{self, Notch, Pointer};
use crate::pulse::{self, Pulse};
use crate::queue::Queue;
use crate::rail;
use crate::rows::{Fence, INDENT, MARGIN, Measure, Row, Rows, Scroll, Sideways, margined};
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
    Depth,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum When {
    Always,
    Card,
    Draft(State),
    Queue,
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

const HINTS: [Hint; 11] = [
    Hint {
        key: "Tab",
        meaning: Meaning::Words("complete"),
        when: When::Card,
    },
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

const SEPARATOR: Look = Look::plain(Ink::Dim);

const HAIRLINE: char = '─';

const EDGE_HEAD: char = '━';

fn draft_end(band: Rect, queue: Rect, below: u16) -> u16 {
    queue.y.min(band.bottom().saturating_sub(below))
}

const COMMANDS_CARD: &str = "/ commands";

#[derive(Clone, Debug, Default)]
pub struct Drawn {
    band: u16,
    total: Option<u32>,
    laid: Vec<Laid>,
    span: Span,
}

impl Drawn {
    pub fn none() -> Drawn {
        Drawn::default()
    }

    pub fn band(&self) -> u16 {
        self.band
    }

    pub fn total(&self) -> Option<u32> {
        self.total
    }

    pub fn span(&self) -> &Span {
        &self.span
    }

    pub fn entry(&self, row: u16) -> Option<usize> {
        self.laid
            .iter()
            .find(|laid| laid.band.contains(&row))
            .map(|laid| laid.entry)
    }

    fn first(&self, row: u16) -> Option<usize> {
        self.laid
            .iter()
            .find(|laid| laid.head && laid.band.start == row)
            .map(|laid| laid.entry)
    }

    fn top(&self, scroll: Scroll) -> Option<u32> {
        let total = self.total()?;
        Some(
            total
                .saturating_sub(u32::from(self.band))
                .saturating_sub(scroll.back()),
        )
    }

    fn rows_of(&self, of: Range<usize>) -> Option<Range<u16>> {
        let drawn: Vec<&Laid> = self
            .laid
            .iter()
            .filter(|laid| of.contains(&laid.entry))
            .collect();
        Some(drawn.first()?.band.start..drawn.last()?.band.end)
    }
}

#[derive(Debug)]
pub struct Painted {
    links: Links,
    sizes: Sizes,
    presses: Vec<Press>,
    drawn: Drawn,
    layout: Option<Layout>,
    placements: Vec<Placement>,
    caret: Option<Position>,
    face: Option<face::Face>,
}

impl Painted {
    pub fn none() -> Painted {
        Painted {
            links: Links::none(),
            sizes: Sizes::none(),
            presses: Vec::new(),
            drawn: Drawn::none(),
            layout: None,
            placements: Vec::new(),
            caret: None,
            face: None,
        }
    }

    pub fn caret(&self) -> Option<Position> {
        self.caret
    }

    pub fn placements(&self) -> &[Placement] {
        &self.placements
    }

    pub fn face(&self) -> Option<&face::Face> {
        self.face.as_ref()
    }

    fn shaded(&mut self, placement: Placement) {
        if placement.places() {
            self.placements.push(placement);
        }
    }

    pub fn links(&self) -> &Links {
        &self.links
    }

    pub fn sizes(&self) -> &Sizes {
        &self.sizes
    }

    pub fn drawn(&self) -> &Drawn {
        &self.drawn
    }

    pub fn layout(&self) -> Option<Layout> {
        self.layout
    }

    fn press(&self, at: Cell) -> Option<usize> {
        self.presses
            .iter()
            .find(|press| press.covers(at))
            .map(|press| press.choice)
    }
}

#[derive(Clone, Debug, Default)]
struct Cards {
    dismissed: Option<String>,
    card: Option<commands::Card>,
    catalog: Option<Catalog>,
}

impl Cards {
    fn prepare(&mut self, draft: &str, facts: &Facts) -> Option<&mut commands::Card> {
        if commands::skill_tail(draft).is_some() && self.catalog.is_none() {
            self.catalog = facts
                .workspace()
                .map(|workspace| Catalog::discover(workspace, facts.home()));
            self.card = None;
        }
        let card = self.follow(draft)?;
        card.loading(facts.loaded());
        Some(card)
    }

    fn follow(&mut self, draft: &str) -> Option<&mut commands::Card> {
        let words = match draft.starts_with('/') {
            true => draft,
            false => "",
        };
        if commands::skill_tail(words).is_none() {
            self.catalog = None;
        }
        if self.card.as_ref().map(commands::Card::words) != Some(words) {
            self.card = match self.dismissed.as_deref() == Some(words) {
                true => None,
                false => match &self.catalog {
                    Some(catalog) => commands::Card::with_catalog(words, catalog),
                    None => commands::Card::of(words),
                },
            };
        }
        self.card.as_mut()
    }

    fn walk(&mut self, draft: &str, way: Move) -> bool {
        self.follow(draft).is_some_and(|card| card.walked(way))
    }

    fn dismiss(&mut self, draft: &str) {
        self.card = None;
        self.dismissed = Some(draft.to_owned());
    }
}

#[derive(Clone, Debug, Default)]
struct ReadMark {
    session: Option<String>,
    entries: usize,
}

#[derive(Debug, Clone)]
pub struct View {
    rows: Rows,
    scroll: Scroll,
    landed: u32,
    drawn: Drawn,
    planes: Planes,
    composer: Composer,
    queue: Queue,
    conversation: Conversation,
    superseded: bool,
    stack: Stack,
    facts: Facts,
    read: ReadMark,
    mark: Option<Mark>,
    fence: Option<MarkedFence>,
    files: Files,
    picker: Picker,
    models: Models,
    settings: Settings,
    cards: Cards,
    capabilities: Capabilities,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Laid {
    entry: usize,
    band: Range<u16>,
    head: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MarkedFence {
    entry: usize,
    fence: usize,
    cut: bool,
    whole: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Transcript {
    entries: Vec<Entry>,
    scroll: Scroll,
}

impl Transcript {
    pub fn of(entries: impl IntoIterator<Item = Entry>) -> Transcript {
        Transcript {
            entries: entries.into_iter().collect(),
            scroll: Scroll::Tail,
        }
    }

    pub fn none() -> Transcript {
        Transcript::default()
    }

    pub fn scrolled(self, scroll: Scroll) -> Transcript {
        Transcript { scroll, ..self }
    }
}

impl View {
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

    pub fn scroll(&self) -> Scroll {
        self.scroll
    }

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

    pub fn earlier(&self) -> Vec<Turn> {
        self.conversation.turns().to_vec()
    }

    pub fn kept(&mut self, ended: &Turn) {
        if !self.superseded {
            self.conversation.join(ended.clone());
        }
    }

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

    pub fn card(&mut self) -> Option<&commands::Card> {
        let card = self.cards.prepare(self.composer.text(), &self.facts)?;
        Some(card)
    }

    pub fn planes(&self) -> &Planes {
        &self.planes
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn drawn_on(&mut self, capabilities: Capabilities) {
        self.capabilities = capabilities;
    }

    pub fn celled(&mut self, cell: Option<CellSize>) {
        let Some(cell) = cell else { return };
        if cell != self.capabilities.cell() {
            self.capabilities = self.capabilities.clone().celled(cell);
        }
    }

    fn surface(&self) -> Surface {
        Surface::of(
            self.facts.workspace(),
            self.capabilities.cell(),
            self.capabilities.pictures().draws(),
            self.capabilities.sizes_text(),
        )
    }

    fn motion(&self) -> face::Motion {
        if !self.capabilities.animates() {
            return face::Motion::Still;
        }
        face::Motion::Turning(pulse::turning(self.facts.pulse()))
    }

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

    pub fn stopped(&mut self, stop: &Stop) {
        self.rows.stopped(stop.clone());
        self.scrolled(Scroll::Tail);
    }

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
        let outcome = match act {
            Act::Draft => self.draft(key),
            Act::Complete => self.complete(),
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
            Act::Forward if self.stack.top() == Mode::Settings && self.settings.refresh() => {
                self.handed(self.facts.clone().unserved());
                Outcome::Served
            }
            Act::Forward => self.page(Page::Forward),
            Act::Tail => self.tail(key),
            Act::Before => self.prompt(key, Move::Previous),
            Act::After => self.prompt(key, Move::Next),
        };
        self.cards.follow(self.composer.text());
        outcome
    }

    pub fn drag(&mut self, anchor: Cell, head: Cell) {
        self.mark = Some(Mark::drag(anchor, head));
    }

    pub fn mark(&self) -> Option<Mark> {
        self.mark
    }

    fn copy(&self) -> Outcome {
        if let Some(marked) = self.fence.filter(|marked| marked.whole) {
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

    fn fold(&mut self) -> Outcome {
        match self.rows.toggle_newest() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

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

    fn sideways(&mut self, way: Sideways) -> Outcome {
        let Some(marked) = self.fence else {
            return Outcome::Unchanged;
        };
        match self.rows.sideways(marked.entry, marked.fence, way) {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    fn unfold(&mut self, row: u16) -> bool {
        let Some(entry) = self.drawn.entry(row) else {
            return false;
        };
        self.rows.open(entry)
    }

    fn complete(&mut self) -> Outcome {
        if !self.composer.on_first_line() {
            return Outcome::Unchanged;
        }
        let Some(card) = self.cards.follow(self.composer.text()) else {
            return Outcome::Unchanged;
        };
        let Some(line) = card.spelling_of(card.selected()) else {
            return Outcome::Unchanged;
        };
        self.composer.rewrite(&line);
        let end = line.find(char::is_whitespace).unwrap_or(line.len());
        while self.composer.cursor() > end {
            self.composer.key(Key::Left);
        }
        Outcome::Edited
    }

    fn draft(&mut self, key: Key) -> Outcome {
        if key == Key::Enter && self.facts.workspace().is_none() {
            return Outcome::Unchanged;
        }
        if key == Key::Enter
            && self
                .cards
                .prepare(self.composer.text(), &self.facts)
                .is_some_and(|card| card.refuses())
        {
            return Outcome::Unchanged;
        }
        if key == Key::Enter {
            let command = commands::read(self.composer.text()).or_else(|| {
                self.cards
                    .follow(self.composer.text())
                    .and_then(|card| commands::read(&card.spelling()))
            });
            if let Some(Command::Skill(name)) = command {
                let outcome = self.skill(&name);
                if outcome == Outcome::Edited {
                    self.composer.key(key);
                }
                return outcome;
            }
        }
        if key == Key::Down && self.cards.walk(self.composer.text(), Move::Next) {
            return Outcome::Edited;
        }
        match self.composer.key(key) {
            Outcome::Send(prompt) => self.sent(prompt),
            outcome => outcome,
        }
    }

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

    fn start_over(&mut self) -> Outcome {
        self.rows = Rows::new();
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

    fn keys(&mut self, key: Key) -> Outcome {
        if !self.composer.text().is_empty() {
            return self.composer.key(key);
        }
        self.stack.open(Mode::Keys);
        Outcome::Edited
    }

    fn at(&mut self, key: Key) -> Outcome {
        if !self.composer.at_word_start() {
            return self.composer.key(key);
        }
        let Some(root) = self.facts.workspace() else {
            return Outcome::Unchanged;
        };
        self.files = Files::walk(root);
        self.picker = Picker::over(self.files.clone());
        self.stack.open(Mode::Files);
        Outcome::Edited
    }

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

    fn flip(&mut self) -> Outcome {
        if self.stack.top() == Mode::Settings
            && !self.settings.editing()
            && !Config::SCHEMA
                .get(self.settings.current())
                .is_some_and(|field| field.switch())
        {
            return Outcome::Unchanged;
        }
        match self.settings.space() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    fn clear(&mut self) -> Outcome {
        match self.settings.clear() {
            true => Outcome::Edited,
            false => Outcome::Unchanged,
        }
    }

    fn write(&mut self) -> Outcome {
        if self.settings.current() < self.settings.rows().count() {
            let asking = self.settings.activate();
            return if asking {
                self.handed(self.facts.clone().unserved());
                Outcome::Served
            } else {
                Outcome::Edited
            };
        }
        if self.settings.current() == self.settings.rows().count() + 1 {
            self.stack.close();
            return Outcome::Edited;
        }
        let touched: Touched = self.settings.touched();
        if touched.is_empty() {
            self.stack.close();
            return Outcome::Edited;
        }
        Outcome::Save(touched)
    }

    fn close(&mut self) -> Outcome {
        if self.stack.top() == Mode::Settings && self.settings.cancel_editor() {
            return Outcome::Edited;
        }
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

    fn tail(&mut self, key: Key) -> Outcome {
        if self.scroll.is_tail() {
            return self.draft(key);
        }
        self.scrolled(Scroll::Tail);
        Outcome::Edited
    }

    fn page(&mut self, by: Page) -> Outcome {
        let rows = self.page_rows();
        self.travel(by, rows)
    }

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

    fn prompt_scroll(&mut self, way: Move) -> Option<Scroll> {
        let tops = self.rows.prompts();
        let at = self.prompt_of(&self.drawn, &tops)?;
        let target = match way {
            Move::Previous => at.checked_sub(1)?,
            Move::Next => at.checked_add(1).filter(|next| *next < tops.len())?,
        };
        let total = self.drawn.total()?;
        let deepest = total.saturating_sub(self.page_rows().saturating_sub(1));
        Some(Scroll::of(deepest.saturating_sub(tops[target])))
    }

    fn prompt_of(&self, drawn: &Drawn, tops: &[u32]) -> Option<usize> {
        let top = drawn.top(self.scroll)?;
        tops.iter().rposition(|row| *row <= top)
    }

    fn standing(&self, drawn: &Drawn, tops: &[u32]) -> Option<String> {
        if tops.len() < 2 {
            return None;
        }
        let at = self.prompt_of(drawn, tops)?;
        Some(format!("prompt {} / {}", at.saturating_add(1), tops.len()))
    }

    fn page_rows(&self) -> u32 {
        u32::from(self.drawn.band().saturating_add(self.notice_rows()))
    }

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
        self.cards.follow(self.composer.text());
    }

    pub fn handed(&mut self, facts: Facts) {
        self.composer.set_state(match facts.live() {
            true => State::Running,
            false => State::Idle,
        });
        self.models = std::mem::take(&mut self.models).served_by(facts.served().cloned());
        if let Some(listing) = facts.served().cloned() {
            self.settings.listing(listing);
        }
        if self.files.root() != facts.workspace() {
            self.files = Files::of(facts.workspace());
        }
        self.facts = facts;
        self.read_record();
    }

    fn read_record(&mut self) {
        let (Some(record), Some(transcript)) = (self.facts.record(), self.facts.transcript())
        else {
            return;
        };
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
        self.rows.retire();
        for entry in fresh {
            self.rows.push(entry);
        }
        self.facts = self.facts.clone().asked();
    }

    pub fn facts(&self) -> &Facts {
        &self.facts
    }

    pub fn pointer(&mut self, painted: &Painted, report: Pointer) -> Outcome {
        if !self.capabilities.reports_pointer() {
            return Outcome::Unchanged;
        }
        match report {
            Pointer::Wheel(notch) => self.notch(notch),
            Pointer::Press(at) => self.pressed(painted, at),
        }
    }

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

    fn pressed_fold(&mut self, drawn: &Drawn, at: usize, row: u16) -> bool {
        let Some(rows) = drawn.rows_of(at..at + 1) else {
            return false;
        };
        let Some(row) = row.checked_sub(rows.start) else {
            return false;
        };
        self.rows.press(at, row)
    }

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

    fn chosen(&mut self, at: usize) -> bool {
        self.kept_list().is_some_and(|list| list.pressed(at))
    }

    pub fn paint(&mut self, area: Rect, buffer: &mut Buffer) -> Painted {
        let painted = self.draw(area, buffer);
        self.planes.hold(painted.placements());
        painted
    }

    fn draw(&mut self, area: Rect, buffer: &mut Buffer) -> Painted {
        if self.composer.on_first_line() {
            self.cards.prepare(self.composer.text(), &self.facts);
        } else {
            self.cards.card = None;
        }
        let mut painted = Painted::none();
        pen::claim(buffer, area, self.capabilities.palette().ground(None));
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
        let mut rail_sizes = Sizes::none();
        if let Some((shape, band)) = layout.rail {
            let picture = match shape {
                Rail::Ledger => {
                    face::over_rail(band.area.width.saturating_sub(1), &self.face_surface())
                }
                Rail::Strip => None,
            };
            let band = match &picture {
                Some(picture) => band.with_picture(picture.cols(), picture.rows()),
                None => band,
            };
            for placement in rail::paint(
                self.facts.rail(),
                shape,
                band,
                self.facts.pulse(),
                (
                    self.capabilities.pictures().cell(),
                    self.capabilities.sizes_text().then_some(&mut rail_sizes),
                ),
                buffer,
                &self.capabilities.palette(),
            ) {
                painted.shaded(placement);
            }
            if let (Some(picture), Some(cells)) = (picture, band.picture) {
                painted.face = Some(face::Face::new(
                    picture,
                    cells,
                    self.capabilities.cell(),
                    self.motion(),
                ));
            }
        }
        let drawn = self.transcript(layout.transcript, buffer, &mut painted);
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
        self.composer_band(layout.composer, layout.queue, buffer, &mut painted);
        self.footer(layout.footer, buffer);
        let screen = self.stack.top();
        let Some((name, leaves, body)) = self.over(area, screen) else {
            painted
                .sizes
                .settle(layout.transcript, buffer, &self.capabilities.palette());
            painted.sizes.append(rail_sizes);
            self.highlight(layout.transcript, buffer, drawn.span());
            self.note(&mut painted, drawn);
            painted.caret = self
                .caret(layout.composer, layout.queue, None)
                .map(|(x, y)| Position::new(x, y));
            painted.layout = Some(layout);
            return painted;
        };
        let face = painted.face.take();
        painted = Painted {
            face,
            ..Painted::none()
        };
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

    fn note(&mut self, painted: &mut Painted, drawn: Drawn) {
        self.drawn = drawn.clone();
        painted.drawn = drawn;
    }

    fn over(
        &mut self,
        area: Rect,
        screen: Mode,
    ) -> Option<(&'static str, &'static str, ScreenBody)> {
        if let Some((name, leaves)) = screen.labels() {
            let width = card::body_width(area);
            return Some((name, leaves, self.body(screen, width)?));
        }
        if !self.composer.on_first_line() {
            return None;
        }
        let body = self
            .cards
            .prepare(self.composer.text(), &self.facts)?
            .body();
        let leaves = Mode::Composer.leaves()?;
        Some((COMMANDS_CARD, leaves, ScreenBody::Rows(body)))
    }

    pub fn body(&self, screen: Mode, width: u16) -> Option<ScreenBody> {
        Some(match screen.body()? {
            Body::Keys => ScreenBody::Rows(self.stack.body(width)),
            Body::Files => ScreenBody::Rows(self.picker.body()),
            Body::Models => ScreenBody::Rows(self.models.body()),
            Body::Settings => ScreenBody::Rows(self.settings.body(width)),
            Body::Ledger => ScreenBody::Ledger(Sheet::laid(&self.stats(), width)),
        })
    }

    fn split(&self, area: Rect) -> Share {
        if self.facts.splash() && Rail::of(area.width) != Rail::Ledger {
            return Share {
                bands: area,
                rail: None,
            };
        }
        ledger::split(self.facts.rail(), area)
    }

    fn transcript(&mut self, area: Rect, buffer: &mut Buffer, painted: &mut Painted) -> Drawn {
        let palette = self.capabilities.palette();
        if let Some(placement) =
            Placement::transcript(area, self.capabilities.pictures().cell(), &palette)
        {
            placement.claim(buffer, &palette);
            painted.shaded(placement);
        }
        let cols = area.width.saturating_sub(MARGIN);
        let reflowed = self.rows.measure(Measure::new(
            cols,
            self.surface().band(area.height),
            self.facts.ground(),
        ));
        let head = self.facts.pulse().map(Pulse::head);
        let own = self.facts.live() || self.rows.says_answer() || !self.rows.holds_answer();
        let (recorded, streamed) = match own {
            true => (None, head),
            false => (head, None),
        };
        self.rows.heading(recorded);
        self.rows.live(streamed.as_ref());
        let total = self.rows.height();
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
        let mut placed = Vec::new();
        for piece in window.pieces() {
            let top = y.saturating_sub(area.y);
            let rows = self.rows.rows(piece.entry);
            let first = usize::from(piece.skip);
            let shown = &rows[first..first + usize::from(piece.rows)];
            for (offset, row) in shown.iter().enumerate() {
                row.paint(
                    Rect {
                        height: 1,
                        y,
                        ..area
                    },
                    buffer,
                    &self.capabilities.palette(),
                    &mut painted.links,
                    &mut painted.sizes,
                );
                if let Some(thumb) = &thumb {
                    thumb.paint(area, y, buffer, &self.capabilities.palette());
                }
                if row.picture().is_some() {
                    placed.push((piece.entry, first + offset, y));
                }
                if let Some(mark) = self.mark {
                    lines.push(Line::of(row.glyph_columns(), row.cut(cols)));
                    let here = row.fence_id().map(|fence| (piece.entry, fence));
                    let line = row.fence();
                    lined.push((
                        line.map(|line| (piece.entry, line.id())),
                        line.map_or(0, Fence::gutter),
                    ));
                    let band = y.saturating_sub(area.y);
                    if (mark.first()..=mark.last()).contains(&band) {
                        self.fence = match self.fence {
                            None => {
                                fence_under(piece.entry, rows, row, cols).map(|fence| MarkedFence {
                                    whole: !strays,
                                    ..fence
                                })
                            }
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
        // Written once every row of the band is painted: a placement covers its rows by writing a
        // sequence into the first cell of each, and a row painted after it would leave the words
        // where that sequence belongs. A refused picture places nothing and leaves the row's own.
        let cell = self.capabilities.cell();
        for (entry, at, y) in placed {
            if let Some(row) = self.rows.rows(entry).get(at) {
                place(self.capabilities.pictures_mut(), cell, row, area, y, buffer);
            }
        }
        self.capabilities.pictures_mut().retain_shown();
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

    fn notice_rows(&self) -> u16 {
        u16::from(!self.scroll.is_tail())
    }

    fn notice(&self) -> Option<String> {
        counted(self.scroll.back(), "back")
    }

    fn notice_band(&mut self, area: Rect, buffer: &mut Buffer, drawn: &Drawn) {
        let Some(place) = self.notice() else {
            return;
        };
        let band = margined(area);
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
        let counted_from = spans.len();
        spans.push((place.as_str(), keys::MEANING));
        if let Some(landed) = landed.as_deref() {
            spans.push((GAP, SEPARATOR));
            spans.push((landed, keys::MEANING));
        }
        spans.push((GAP, SEPARATOR));
        spans.extend(keyed("End", "follows the run"));
        if counted_from > 0 && pen::width(&spans) > band.width {
            spans.drain(..counted_from);
        }
        whole_row(buffer, band, area.y, &spans, &self.capabilities.palette());
    }

    fn composer_band(&self, band: Rect, queue: Rect, buffer: &mut Buffer, painted: &mut Painted) {
        let palette = self.capabilities.palette();
        let cell = self.capabilities.pictures().cell();
        if let Some(placement) = Placement::composer(band, cell, &palette) {
            placement.claim(buffer, &palette);
            painted.shaded(placement);
            if let Some(caret) = self.caret(band, queue, None).map(|(x, _)| x)
                && let Some(light) = Placement::caret(band, cell, caret, &palette)
            {
                painted.shaded(light);
            }
        }
        self.composer_inside(band, queue, buffer);
        self.composer_head(band, buffer);
    }

    fn composer_inside(&self, band: Rect, queue: Rect, buffer: &mut Buffer) {
        let ends = self.composer.ends();
        let top = band.y.saturating_add(ends.above);
        let draft = Rect {
            y: top,
            height: draft_end(band, queue, ends.below).saturating_sub(top),
            width: band.width.saturating_sub(MARGIN),
            ..band
        };
        let prompts = Rect {
            width: queue.width.saturating_sub(MARGIN),
            ..queue
        };
        let empty = self.composer.text().is_empty();
        let waited =
            (!empty && self.band_takes_keys()).then(|| usize::from(self.composer.caret().0));
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

    fn composer_head(&self, band: Rect, buffer: &mut Buffer) {
        if band.height == 0 {
            return;
        }
        let palette = self.capabilities.palette();
        let look = self.composer_edge();
        let row = Rect { height: 1, ..band };
        let count = self.draft_count();
        let counted = pen::width(&count);
        let edge = Rect {
            width: row.width.saturating_sub(MARGIN),
            ..row
        };
        let hung = pen::hanging(edge, counted)
            .filter(|first| *first > edge.x)
            .map(|_| counted)
            .unwrap_or(0);
        let room = Rect {
            width: match hung {
                0 => row.width,
                hung => row.width.saturating_sub(hung).saturating_sub(MARGIN),
            },
            ..row
        };
        if !palette.draws_edge() {
            pen::rule(buffer, room, HAIRLINE, palette.dim_edge(), &palette);
        }
        if self.facts.live() && room.width > 0 {
            let step = self.edge_step() % u64::from(room.width);
            let at = room
                .x
                .saturating_add(pen::columns(usize::try_from(step).unwrap_or(0)));
            pen::glyph(buffer, at, row.y, EDGE_HEAD, look, &palette);
        }
        if hung > 0 {
            pen::hang(buffer, edge, &count, &palette);
        }
    }

    fn band_takes_keys(&self) -> bool {
        self.stack.modal().is_none() && self.facts.focus() == Focus::Focused
    }

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

    fn edge_step(&self) -> u64 {
        match self.facts.pulse() {
            Some(Pulse::Live { elapsed_ms }) => pulse::step(*elapsed_ms),
            Some(Pulse::Settled { .. }) | None => 0,
        }
    }

    fn composer_edge(&self) -> Look {
        let palette = self.capabilities.palette();
        match self.band_takes_keys() && palette.draws_edge() {
            true => Look::plain(Ink::Accent),
            false => palette.dim_edge(),
        }
    }

    fn edge(&self, look: Look) -> Look {
        match self.facts.focus() {
            Focus::Focused => look,
            Focus::Away => self.capabilities.palette().dim_edge(),
        }
    }

    fn caret(&self, band: Rect, queue: Rect, card: Option<Rect>) -> Option<(u16, u16)> {
        if self.stack.modal().is_some() {
            return None;
        }
        let ends = self.composer.ends();
        let (row, column) = self.composer.caret();
        let y = band.y.checked_add(ends.above)?.checked_add(row)?;
        let x = band
            .x
            .checked_add(pen::cells(INDENT).saturating_add(column))?
            .min(band.right().saturating_sub(MARGIN.saturating_add(1)));
        if band.width == 0 || y >= draft_end(band, queue, ends.below) {
            return None;
        }
        match card.is_some_and(|card| card.contains(Position::new(x, y))) {
            true => None,
            false => Some((x, y)),
        }
    }

    fn room(area: Rect) -> u16 {
        area.width
            .saturating_sub(pen::cells(INDENT))
            .saturating_sub(MARGIN)
    }

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
            When::Card => self.stack.top() == Mode::Composer && self.cards.card.is_some(),
            When::Draft(state) => {
                self.composer.state() == state
                    && (state != State::Idle
                        || !self.cards.card.as_ref().is_some_and(|card| card.refuses()))
            }
            When::Queue => !self.queue.is_empty(),
            When::Fence => self.fence.is_some_and(|marked| marked.cut),
        }
    }

    fn footer(&self, area: Rect, buffer: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let depth = self.queue.label().unwrap_or_default();
        hints_row(
            buffer,
            margined(area),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub transcript: Rect,
    pub queue: Rect,
    pub notice: Rect,
    pub composer: Rect,
    pub footer: Rect,
    pub card: Option<Rect>,
    pub rail: Option<(Rail, RailBand)>,
}

/// Places the picture `row` stands on over the block of cells the transcript reserved, down to the
/// last row the band holds. A block whose head the band has scrolled past is left as the words it
/// was written as, because the protocol sends a picture with the first row of its block.
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
    if block.right() > area.right() {
        return;
    }
    let shown = Rect {
        height: block.height.min(area.bottom().saturating_sub(y)),
        ..block
    };
    pictures.place(picture, cell, shown, buffer);
}

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

pub struct Sheet(Vec<ledger::Row>);

impl Sheet {
    fn laid(facts: &ledger::Facts, width: u16) -> Sheet {
        Sheet(ledger::rows(
            facts,
            ledger::Shape::Screen,
            Rect::new(0, 0, width, u16::MAX),
            None,
        ))
    }

    pub fn height(&self) -> u16 {
        u16::try_from(self.0.len()).unwrap_or(u16::MAX)
    }

    pub fn draw(&self, body: Rect, buffer: &mut Buffer, palette: &Palette) {
        ledger::draw(&self.0, body, buffer, palette);
    }
}

pub enum ScreenBody {
    Rows(Vec<ScreenRow>),
    Ledger(Sheet),
}

impl ScreenBody {
    fn held(&self) -> u16 {
        match self {
            ScreenBody::Rows(rows) => u16::try_from(rows.len()).unwrap_or(u16::MAX),
            ScreenBody::Ledger(sheet) => sheet.height(),
        }
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Press {
    area: Rect,
    choice: usize,
}

impl Press {
    fn covers(&self, at: Cell) -> bool {
        at.row == self.area.y && (self.area.x..self.area.right()).contains(&at.column)
    }
}

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

fn widened(mark: Mark, taken: Option<(u16, u16, u16)>) -> Mark {
    match taken {
        Some((first, last, gutter)) => {
            Mark::drag(Cell::new(first, gutter), Cell::new(last, u16::MAX))
        }
        None => mark,
    }
}

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

fn counted(rows: u32, of: &str) -> Option<String> {
    match rows {
        0 => None,
        1 => Some(format!("1 row {of}")),
        rows => Some(format!("{rows} rows {of}")),
    }
}

fn keyed<'a>(key: &'a str, meaning: &'a str) -> [(&'a str, Look); 3] {
    [
        (key, keys::KEY),
        (" ", keys::MEANING),
        (meaning, keys::MEANING),
    ]
}

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
