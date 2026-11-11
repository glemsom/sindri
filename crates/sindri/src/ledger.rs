//! The run's facts, the one way each of them is spelled and the rows they are laid out in: the
//! wire, the served name, thinking, the turns, the window and its meter, the cache, the workspace,
//! the branch, the skills, the sandbox, the session and the temp.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::policy::Enforcement;
use sindri_core::provider::Usage;
use sindri_core::records::SessionDocument;
use sindri_core::session::Session;
use sindri_core::wire::Wire;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen;

/// The columns `text` draws in, a wide glyph counted as the two cells it takes: a value is
/// measured by what a row shows and never by how many glyphs it is made of.
fn drawn(text: &str) -> usize {
    usize::from(pen::cells(text))
}

/// The frame width the ledger gives way at: a hundred-column frame is the last that holds one.
pub const NARROW: u16 = 100;

/// The two shapes a fact is read in: the ledger's column beside a transcript, which has room for
/// a pair beside its meter, and the strip along a band's bottom row, which has room for the meter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Ledger,
    Strip,
}

impl Shape {
    pub fn of(width: u16) -> Shape {
        if width < NARROW {
            Shape::Strip
        } else {
            Shape::Ledger
        }
    }
}

/// The value a fact a launch can already name but a run has yet to fill is drawn as: an em dash
/// says the fact is owed and never that it is empty.
pub(crate) const DASH: &str = "—";

/// The columns a session's GUID is drawn by: no rail column holds a whole one.
const SHORT: usize = 8;

/// The cells a meter is drawn in: a share of a window is a picture, and six cells is as fine as
/// one row of a rail can say it.
pub(crate) const METER: usize = 6;

/// The cell a meter is full of, and the cell it has yet to fill.
pub(crate) const FULL_CELL: char = '█';
pub(crate) const EMPTY_CELL: char = '░';

/// The share of a window at which the rail raises its voice: a run that full is one turn from
/// being no room at all.
const FULL: u8 = 90;

/// The run's size as the last request reported it, and never as the next one will be: whatever
/// tool results the turn produced are not in it yet, so a run one turn from a full window is
/// that and no more exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub size: u64,
    pub window: Option<u64>,
}

impl Ctx {
    /// The size and the meter of the share of its window, from one derivation so a row's words and
    /// its meter cannot disagree. The ledger says the size against its window; the strip keeps only
    /// the meter, because its one row is shared with three other facts.
    pub(crate) fn spelled(&self, shape: Shape) -> Value {
        let Some(fill) = self.fill() else {
            return Value::new(tokens(self.size));
        };
        let text = match shape {
            Shape::Ledger => pair(tokens(self.size), tokens(fill.window)),
            Shape::Strip => String::new(),
        };
        Value::metered(text, fill)
    }

    /// The share of the window the run fills. A window of no tokens is no share of anything, and the
    /// ink is set here and nowhere else, so no paint site holds a second opinion.
    fn fill(&self) -> Option<Fill> {
        let window = self.window?;
        let percent = share(self.size, window)?;
        Some(Fill {
            percent,
            window,
            ink: if percent >= FULL {
                Ink::Accent
            } else {
                Ink::Plain
            },
        })
    }
}

/// The share of a window a run fills, as the meter that draws it: the percent the pair of numbers
/// cannot say in one glance, and the ink the filled cells are read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fill {
    percent: u8,
    window: u64,
    ink: Ink,
}

impl Fill {
    /// The cells the share fills, rounded to the nearest of the meter's six: a cell is a sixth of
    /// the window, and a share that fills no sixth of it is a meter with no cell lit.
    fn cells(&self) -> usize {
        (usize::from(self.percent) * METER + 50) / 100
    }

    fn draw(&self, buffer: &mut Buffer, x: u16, y: u16, palette: &Palette) -> u16 {
        let filled = self.cells();
        let mut x = x;
        for cell in 0..METER {
            let (glyph, ink) = match cell < filled {
                true => (FULL_CELL, self.ink),
                false => (EMPTY_CELL, Ink::Dim),
            };
            pen::glyph(buffer, x, y, glyph, ink, palette);
            x = x.saturating_add(1);
        }
        x
    }
}

/// One of the run's facts, by the word a label field holds it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Wire,
    Model,
    Think,
    Turns,
    Ctx,
    Cache,
    Workspace,
    Branch,
    Skills,
    Sandbox,
    Session,
    Temp,
}

impl Kind {
    /// The word the label field holds, so no two drawings can call one fact two things.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Kind::Wire => "wire",
            Kind::Model => "model",
            Kind::Think => "think",
            Kind::Turns => "turns",
            Kind::Ctx => "ctx",
            Kind::Cache => "cache",
            Kind::Workspace => "workspace",
            Kind::Branch => "branch",
            Kind::Skills => "skills",
            Kind::Sandbox => "sandbox",
            Kind::Session => "session",
            Kind::Temp => "temp",
        }
    }
}

/// What a launch knows before its first run: the wire and the model the config names, the
/// workspace and the branch it stands on, and the enforcement the policy will hold. A session, a
/// temp and a turn count belong to a run and are not here.
#[derive(Debug, Clone)]
pub struct Boot {
    pub wire: Option<Wire>,
    pub model: Option<String>,
    pub workspace: PathBuf,
    pub branch: Option<String>,
    pub enforcement: Enforcement,
}

/// The facts a run holds, as the core keeps them. A fact nobody can state is `None` and is not
/// drawn, never guessed at; a fact the launch names but a run has yet to fill is an em dash, which
/// is a value the frame holds and not an absence. No fact at all and the frame draws no rail.
#[derive(Debug, Default, Clone)]
pub struct Facts {
    /// The dialect the run asks over, in the wire's own word: `anthropic`, `openai` or `copilot`,
    /// and never a fourth.
    pub wire: Option<String>,
    /// The served name of the model, whole here and cut to its head wherever the rail draws it.
    pub model: Option<String>,
    pub think: Option<String>,
    pub turns: Option<(u32, u32)>,
    pub ctx: Option<Ctx>,
    /// How much of the run's input the provider served from its own cache: a hit rate and not a
    /// fill, so it gets no meter and no threshold.
    pub cache: Option<u8>,
    pub workspace: Option<PathBuf>,
    pub branch: Option<String>,
    /// Nothing produces this fact yet: a caller passes it in until its producer lands.
    pub skills: Option<usize>,
    pub sandbox: Option<Enforcement>,
    /// The run's own, and an em dash on a frame that has not run yet.
    pub session: Option<String>,
    /// The run's own, and an em dash on a frame that has not run yet.
    pub temp: Option<PathBuf>,
}

impl Facts {
    /// The facts a run holds, read off the two values the core keeps them in: the session the run is
    /// writing, and the document the record rewrites as facts land.
    pub fn of(session: &Session, document: &SessionDocument) -> Facts {
        Facts {
            wire: Some(document.wire.word().to_owned()),
            model: Some(document.model.clone()),
            think: Some(if document.thinking { "high" } else { "off" }.to_owned()),
            turns: Some((document.turns, document.dials.max_turns)),
            sandbox: Some(document.enforcement),
            workspace: Some(document.workspace.clone()),
            branch: document.branch.clone(),
            session: Some(session.guid().to_owned()),
            temp: Some(session.temp().to_path_buf()),
            ..Facts::default()
        }
    }

    /// The facts of the frame a launch opens on, before a session and a document exist. A run's
    /// own facts — its session, its temp, its turns — are not known yet, and each is an em dash
    /// rather than a guess at what the run will hold.
    pub fn boot(boot: Boot) -> Facts {
        Facts {
            wire: boot.wire.map(|wire| wire.word().to_owned()),
            model: boot.model,
            workspace: Some(boot.workspace),
            branch: boot.branch,
            sandbox: Some(boot.enforcement),
            session: Some(String::from(DASH)),
            temp: Some(PathBuf::from(DASH)),
            ..Facts::default()
        }
    }

    /// The size one ask's report measured, and the part of its input the provider served from cache.
    /// The document cannot hand these in: the `usage` it keeps is every ask summed, and a sum of the
    /// inputs grows with each turn while the conversation's size is what the newest request carried.
    pub fn measured(mut self, ask: &Usage, window: Option<u64>) -> Facts {
        let Some(size) = ask.input else {
            return self;
        };
        self.ctx = Some(Ctx { size, window });
        self.cache = ask.cached_input.and_then(|cached| share(cached, size));
        self
    }
}

/// A value as a row draws it: its own glyphs, the meter a share of a window is drawn as beside
/// them, and the ink the glyphs are read in. The rule lives here and nowhere else, so the rail's
/// ledger and strip and the stats screen that widens them place and colour a value alike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Value {
    text: String,
    fill: Option<Fill>,
    ink: Ink,
}

impl Value {
    pub(crate) fn new(text: impl Into<String>) -> Value {
        Value {
            text: text.into(),
            fill: None,
            ink: Ink::Plain,
        }
    }

    pub(crate) fn metered(text: impl Into<String>, fill: Fill) -> Value {
        Value {
            fill: Some(fill),
            ..Value::new(text)
        }
    }

    /// The same value in another ink, for the one fact that says something is moving: the eye lands
    /// on it without reading a word.
    pub(crate) fn inked(mut self, ink: Ink) -> Value {
        self.ink = ink;
        self
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn meter(&self) -> Option<Fill> {
        self.fill
    }

    /// The same value spelled in another cut: a column that narrows a value's glyphs keeps the
    /// meter beside them and the ink they are read in.
    pub(crate) fn respelled(&self, text: String) -> Value {
        Value {
            text,
            ..self.clone()
        }
    }

    /// The value's pair in the half widths its group shares, so a slash lands in one column.
    pub(crate) fn align(&mut self, halves: Halves) {
        self.text = padded(&self.text, halves);
    }

    /// The columns the value takes on a row, the column between a meter and its glyphs included.
    pub(crate) fn columns(&self) -> usize {
        let glyphs = drawn(&self.text);
        match (self.fill, glyphs) {
            (None, glyphs) => glyphs,
            (Some(_), 0) => METER,
            (Some(_), glyphs) => METER + 1 + glyphs,
        }
    }

    pub(crate) fn draw(&self, buffer: &mut Buffer, x: u16, y: u16, palette: &Palette) -> u16 {
        let mut x = x;
        if let Some(fill) = self.fill {
            x = fill.draw(buffer, x, y, palette);
            if !self.text.is_empty() {
                x = pen::run(buffer, x, y, " ", self.ink, palette);
            }
        }
        pen::run(buffer, x, y, &self.text, self.ink, palette)
    }
}

/// A number in two parts, a space either side of its slash: the digits read as a share of a whole
/// and not as one word, and the slash gets a column of its own to be read in.
pub(crate) fn pair(left: impl std::fmt::Display, right: impl std::fmt::Display) -> String {
    format!("{left} / {right}")
}

/// The share of a whole that a part is, in whole percent and capped at a hundred: a part past
/// its whole has no larger share to show, and a share of nothing is no percentage at all.
fn share(part: u64, whole: u64) -> Option<u8> {
    if whole == 0 {
        return None;
    }
    let percent = (u128::from(part) * 100 + u128::from(whole) / 2) / u128::from(whole);
    Some(u8::try_from(percent.min(100)).unwrap_or(100))
}

/// How much of a run's input the provider served from its own cache, in whole percent: a hit rate
/// and not a fill, so it draws no meter and has no ceiling to be a share of.
pub(crate) fn hit_rate(percent: u8) -> String {
    format!("{percent}%")
}

/// The size of a run in spelled tokens: `948`, `48.2k`, `1.02M`, because a rail column has no
/// room to make six digits read as one number.
pub(crate) fn tokens(count: u64) -> String {
    if count < 1_000 {
        return count.to_string();
    }
    // Tenths of a thousand, and hundredths of a million: the first carries three figures up to
    // nine hundred and ninety-nine thousand, and the second once rounding takes a count past
    // four.
    let tenths = count.saturating_add(50) / 100;
    if tenths < 10_000 {
        return scaled(tenths, 1, "k");
    }
    scaled(count.saturating_add(5_000) / 10_000, 2, "M")
}

/// An amount in a unit's smallest shown fraction, drawn as that unit's whole part, a point and
/// what is left of the fraction. A zero left dangling goes, so two hundred thousand is `200k`
/// and not `200.0k`.
fn scaled(parts: u64, places: u32, unit: &str) -> String {
    let divisor = 10u64.pow(places);
    let whole = parts / divisor;
    let fraction = parts % divisor;
    if fraction == 0 {
        return format!("{whole}{unit}");
    }
    let decimals = format!("{fraction:0width$}", width = places as usize);
    format!("{whole}.{}{unit}", decimals.trim_end_matches('0'))
}

/// The row that says how many facts the room could not hold, in the one wording both shapes and
/// the screen that widens it count them by.
pub(crate) fn more(count: usize) -> String {
    format!("+{count} more")
}

/// The columns a group's pairs share: the widest half either side of the slash, so a block's
/// numbers are read as a column of numbers and a column of wholes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Halves {
    left: usize,
    right: usize,
}

impl Halves {
    pub(crate) fn of<'a>(spellings: impl Iterator<Item = &'a str>) -> Halves {
        spellings
            .filter_map(|spelling| spelling.split_once(" / "))
            .fold(Halves::default(), |shared, (one, two)| Halves {
                left: shared.left.max(drawn(one)),
                right: shared.right.max(drawn(two)),
            })
    }
}

/// `3 / 250` in its group's half widths: both halves hang on the slash, and the right one is
/// filled to the left, so no value carries a space the hanging edge would have to swallow.
pub(crate) fn padded(spelling: &str, shared: Halves) -> String {
    let Some((one, two)) = spelling.split_once(" / ") else {
        return spelling.to_owned();
    };
    format!("{} / {}", hung(one, shared.left), hung(two, shared.right))
}

/// `text` hung from `columns`, padded on the left by the columns it does not draw in: a half
/// holding a wide glyph spends two columns a glyph, so it is padded by what it leaves behind.
fn hung(text: &str, columns: usize) -> String {
    let room = columns.saturating_sub(drawn(text));
    let mut hung = pen::blank(u16::try_from(room).unwrap_or(u16::MAX));
    hung.push_str(text);
    hung
}

/// The tail of a path in at most `columns`, with `…/` in front when the head did not fit. Nothing
/// is ever cut mid-component: a tail that cannot hold even the last component is empty, which
/// drops the fact.
pub(crate) fn tail(path: &str, columns: usize) -> String {
    if drawn(path) <= columns {
        return path.to_owned();
    }
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    for start in 0..parts.len() {
        let tail = format!("…/{}", parts[start..].join("/"));
        if drawn(&tail) <= columns {
            return tail;
        }
    }
    String::new()
}

/// The head of a served name in at most `columns`: whole hyphen-separated segments, and never a
/// part of one. A name keeps its head, where the family lives, and gives up its stamps, because
/// widening the rail only moves the width a name fails at. A head that fits no mark is empty.
pub(crate) fn head(name: &str, columns: usize) -> String {
    if drawn(name) <= columns {
        return name.to_owned();
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut width = 0;
    for (index, part) in name.split('-').enumerate() {
        let next = width + drawn(part) + usize::from(index > 0);
        if next + 1 > columns {
            break;
        }
        kept.push(part);
        width = next;
    }
    if kept.is_empty() {
        return String::new();
    }
    format!("{}…", kept.join("-"))
}

/// A session's GUID by its first eight columns, or whole when it is shorter: no rail column holds
/// thirty-six.
pub(crate) fn short_id(guid: &str) -> &str {
    match guid.char_indices().nth(SHORT) {
        Some((end, _)) => &guid[..end],
        None => guid,
    }
}

/// The columns the ledger takes down a frame's right-hand edge.
pub const COLUMNS: u16 = 32;

/// The columns a label field holds, whatever the label says, so every label starts in one column
/// and every value hangs from the field's far side.
pub(crate) const FIELD: u16 = 10;

/// The columns a fact's value is cut to, whatever shape reads it: a ledger's field comes off the
/// front of its column, and a strip's copy of a name is cut the same way.
const VALUE: usize = (COLUMNS - FIELD) as usize;

/// What lies between two facts on the strip's one row, the gap the footer puts between its hints.
pub(crate) const GAP: &str = " · ";

const HAIRLINE: char = '─';

/// The ledger's blocks, top to bottom, each with the facts that belong in it: the order is a
/// contract, and a fact's place in it is fixed.
const BLOCKS: [&[Kind]; 6] = [
    &[Kind::Wire, Kind::Model, Kind::Think],
    &[Kind::Turns, Kind::Ctx, Kind::Cache],
    &[Kind::Workspace, Kind::Branch],
    &[Kind::Skills],
    &[Kind::Sandbox],
    &[Kind::Session, Kind::Temp],
];

/// The four facts the strip carries, in the order it reads them: its one row asks a different
/// question than the ledger's blocks do, so its list is its own.
const STRIP_FACTS: [Kind; 4] = [Kind::Ctx, Kind::Model, Kind::Sandbox, Kind::Branch];

/// The order the strip gives its facts up in: read from the tail, so the run's size — the head, the
/// one a reader watches — is what it never gives up.
const STRIP_SACRIFICE: [Kind; 3] = [Kind::Branch, Kind::Sandbox, Kind::Model];

/// The ledger's sacrifice order, the first to go first. The sandbox posture is deliberately absent,
/// because it never goes, so no loop over this list can reach it.
const SACRIFICE: [Kind; 11] = [
    Kind::Temp,
    Kind::Cache,
    Kind::Session,
    Kind::Branch,
    Kind::Skills,
    Kind::Ctx,
    Kind::Workspace,
    Kind::Turns,
    Kind::Wire,
    Kind::Think,
    Kind::Model,
];

/// What the transcript and the rail share of a band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share {
    pub transcript: Rect,
    /// Nothing when the frame knows no fact at all, and the transcript keeps the whole band.
    pub rail: Option<(Shape, Rect)>,
}

/// Cuts the band the transcript and the rail share: thirty-two columns down the right while the
/// frame is wide enough to hold a column, one strip along the band's bottom row below a hundred. A
/// frame that knows no fact draws no rail, and the transcript loses no room to one.
pub fn split(facts: &Facts, band: Rect) -> Share {
    let shape = Shape::of(band.width);
    if band.width == 0 || band.height == 0 || known(facts, shape).is_empty() {
        return Share {
            transcript: band,
            rail: None,
        };
    }
    let (transcript, rail) = match shape {
        Shape::Ledger => {
            let columns = COLUMNS.min(band.width);
            let x = band.x.saturating_add(band.width - columns);
            (
                Rect {
                    width: band.width - columns,
                    ..band
                },
                Rect {
                    x,
                    height: band.height,
                    width: columns,
                    ..band
                },
            )
        }
        Shape::Strip => (
            Rect {
                height: band.height - 1,
                ..band
            },
            Rect {
                y: band.y.saturating_add(band.height - 1),
                height: 1,
                ..band
            },
        ),
    };
    Share {
        transcript,
        rail: Some((shape, rail)),
    }
}

/// The rows `shape` lays on `area`, in the order it reads them: every run a row draws and the column
/// of the band each starts at.
pub(crate) fn rows(facts: &Facts, shape: Shape, area: Rect) -> Vec<Row> {
    let kept = kept(facts, shape, area);
    match shape {
        Shape::Ledger => ledger_lines(&kept.facts, kept.went)
            .into_iter()
            .map(|line| match line {
                Line::Fact(fact) => labelled(area, fact),
                Line::Hairline => hairline_row(area),
                Line::Went(went) => Row::run(area.x, more(went), Ink::Muted),
            })
            .collect(),
        Shape::Strip => vec![strip(&kept, area)],
    }
}

/// One row of a shape, laid out: the runs it draws and the column of the band each starts at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    parts: Vec<(u16, Part)>,
}

/// What a row draws at one column: glyphs in an ink, or a fact's value in its own glyphs, meter and
/// ink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Run(String, Ink),
    Value(Value),
}

impl Row {
    fn run(at: u16, text: impl Into<String>, ink: Ink) -> Row {
        Row {
            parts: vec![(at, Part::Run(text.into(), ink))],
        }
    }

    /// Adds a part at `x` and answers the column after it.
    fn push(&mut self, x: u16, part: Part) -> u16 {
        let next = x.saturating_add(part.columns());
        self.parts.push((x, part));
        next
    }

    /// What this row draws, and the column of the band each part starts at.
    pub(crate) fn parts(&self) -> &[(u16, Part)] {
        &self.parts
    }
}

impl Part {
    /// The columns this part takes on its row.
    pub(crate) fn columns(&self) -> u16 {
        match self {
            Part::Run(text, _) => pen::cells(text),
            Part::Value(value) => u16::try_from(value.columns()).unwrap_or(u16::MAX),
        }
    }

    /// Writes this part at a column of a row.
    pub(crate) fn draw(&self, buffer: &mut Buffer, x: u16, y: u16, palette: &Palette) {
        match self {
            Part::Run(text, ink) => {
                pen::run(buffer, x, y, text, *ink, palette);
            }
            Part::Value(value) => {
                value.draw(buffer, x, y, palette);
            }
        }
    }
}

/// A fact's row on the ledger: its label in the field from the row's left edge, and its value hung
/// on the far edge of the band.
fn labelled(area: Rect, fact: &Fact) -> Row {
    Row {
        parts: vec![
            (area.x, Part::Run(field(fact.kind.label()), Ink::Dim)),
            (
                hanging(area, fact.value.columns()),
                Part::Value(fact.value.clone()),
            ),
        ],
    }
}

/// A label padded out to the field, so every label starts in one column.
fn field(label: &str) -> String {
    format!("{label:<width$}", width = usize::from(FIELD))
}

/// The rule between two blocks, which costs the block under it a row of the band it is drawn on.
fn hairline_row(area: Rect) -> Row {
    Row::run(area.x, hairline_glyphs(area.width), Ink::Dim)
}

/// The run of glyphs a rule one row wide of `width` columns is drawn as.
fn hairline_glyphs(width: u16) -> String {
    std::iter::repeat_n(HAIRLINE, usize::from(width)).collect()
}

/// The strip's one row: its facts read left to right with a gap between two, and the count of what
/// went at the end when the room holds it.
fn strip(kept: &Kept, area: Rect) -> Row {
    let mut row = Row { parts: Vec::new() };
    let mut x = area.x;
    for fact in &kept.facts {
        if !row.parts.is_empty() {
            x = row.push(x, Part::Run(GAP.to_owned(), Ink::Dim));
        }
        x = row.push(x, Part::Run(format!("{} ", fact.kind.label()), Ink::Dim));
        x = row.push(x, Part::Value(fact.value.clone()));
    }
    // A strip too narrow to hold the count beside the facts it keeps is the one row without one.
    if kept.counted {
        if !row.parts.is_empty() {
            x = row.push(x, Part::Run(GAP.to_owned(), Ink::Dim));
        }
        row.push(x, Part::Run(more(kept.went), Ink::Muted));
    }
    row
}

/// One fact as a shape draws it: the fact itself, which names its label field and its place in the
/// sacrifice order, and the value beside it.
struct Fact {
    kind: Kind,
    value: Value,
}

impl Fact {
    fn new(kind: Kind, value: impl Into<String>) -> Fact {
        Fact {
            kind,
            value: Value::new(value),
        }
    }
}

struct Kept {
    facts: Vec<Fact>,
    went: usize,
    /// Whether the strip says how many went: the ledger's count is one of its rows, and the band's
    /// height is what holds it.
    counted: bool,
}

/// One row of the ledger, before it is a row of runs.
enum Line<'a> {
    Fact(&'a Fact),
    Hairline,
    Went(usize),
}

/// The ledger gives its facts up in a settled order beside the row that says how many went; the
/// strip gives them up from the tail of its own list, but its count is the last item of its one
/// row, so the room takes the count first.
fn kept(facts: &Facts, shape: Shape, area: Rect) -> Kept {
    let known = known(facts, shape);
    let total = known.len();
    let mut kept: Vec<Fact> = known
        .into_iter()
        .filter(|fact| shape == Shape::Ledger || STRIP_FACTS.contains(&fact.kind))
        .filter(|fact| fits(fact, shape, area.width))
        .collect();
    let order: &[Kind] = match shape {
        Shape::Ledger => &SACRIFICE,
        Shape::Strip => &STRIP_SACRIFICE,
    };
    // The ledger's count is one of its rows, so the room it takes is part of what the facts are
    // given up for; the strip's is measured once the facts are in, below.
    let with_count = shape == Shape::Ledger;
    let mut went = total - kept.len();
    while !holds(&kept, went, with_count, shape, area) {
        let Some(victim) = order
            .iter()
            .find_map(|kind| kept.iter().position(|fact| fact.kind == *kind))
        else {
            // Nothing the shape may give up is left — the ledger keeps the sandbox posture and the
            // strip the run's size — so a rail narrower than that fact has nothing left to give up.
            break;
        };
        kept.remove(victim);
        went = went.saturating_add(1);
    }
    let counted = went > 0 && holds(&kept, went, true, shape, area);
    Kept {
        facts: kept,
        went,
        counted,
    }
}

/// Every fact this frame knows, in the order the shape reads them, each value spelled the way that
/// shape spells it: a session by the first eight of its GUID, a path by the tail that fits, a
/// served name by the head.
fn known(facts: &Facts, shape: Shape) -> Vec<Fact> {
    let short = facts.session.as_deref().map(short_id);
    let mut rows = Vec::new();
    if let Some(wire) = &facts.wire {
        rows.push(Fact::new(Kind::Wire, wire.clone()));
    }
    if let Some(model) = &facts.model {
        rows.push(Fact::new(Kind::Model, head(model, VALUE)));
    }
    if let Some(think) = &facts.think {
        rows.push(Fact::new(Kind::Think, think.clone()));
    }
    if let Some((turns, cap)) = facts.turns {
        rows.push(Fact::new(Kind::Turns, pair(turns, cap)));
    }
    if let Some(ctx) = &facts.ctx {
        rows.push(Fact {
            kind: Kind::Ctx,
            value: ctx.spelled(shape),
        });
    }
    if let Some(cache) = facts.cache {
        rows.push(Fact::new(Kind::Cache, hit_rate(cache)));
    }
    if let Some(workspace) = &facts.workspace {
        let value = tail(&workspace.to_string_lossy(), VALUE);
        if !value.is_empty() {
            rows.push(Fact::new(Kind::Workspace, value));
        }
    }
    if let Some(branch) = &facts.branch {
        rows.push(Fact::new(Kind::Branch, branch.clone()));
    }
    if let Some(skills) = facts.skills {
        rows.push(Fact::new(Kind::Skills, skills.to_string()));
    }
    if let Some(sandbox) = facts.sandbox {
        rows.push(Fact::new(Kind::Sandbox, sandbox.rail_label().to_owned()));
    }
    if let Some(short) = short {
        rows.push(Fact::new(Kind::Session, short.to_owned()));
    }
    if let Some(temp) = &facts.temp {
        let path = temp.to_string_lossy();
        // The temp's path carries the session's whole GUID, so it is drawn under the eight-column
        // name the session row above it uses.
        let named = match (short, facts.session.as_deref()) {
            (Some(short), Some(guid)) => path.replace(guid, short),
            _ => path.into_owned(),
        };
        let value = tail(&named, VALUE);
        if !value.is_empty() {
            rows.push(Fact::new(Kind::Temp, value));
        }
    }
    match shape {
        Shape::Ledger => aligned(&mut rows),
        Shape::Strip => rows.sort_by_key(|fact| strip_place(fact.kind)),
    }
    rows
}

/// The ledger's rows in the order they are read, with a hairline between two blocks that both kept
/// a fact and the count of what went at the foot when anything did.
fn ledger_lines(kept: &[Fact], went: usize) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut written = 0;
    for block in BLOCKS {
        let facts: Vec<&Fact> = kept
            .iter()
            .filter(|fact| block.contains(&fact.kind))
            .collect();
        if facts.is_empty() {
            continue;
        }
        if written > 0 {
            lines.push(Line::Hairline);
        }
        lines.extend(facts.into_iter().map(Line::Fact));
        written += 1;
    }
    if went > 0 {
        lines.push(Line::Went(went));
    }
    lines
}

/// Whether one fact is drawn whole in the room its row leaves it — the ledger's value column beside
/// the label field, the strip's what one item of its row takes — and whether a cut to nothing says
/// nothing at all.
fn fits(fact: &Fact, shape: Shape, room: u16) -> bool {
    let value = fact.value.columns();
    if value == 0 {
        return false;
    }
    let room = match shape {
        Shape::Ledger => value_room(room),
        Shape::Strip => usize::from(room).saturating_sub(drawn(fact.kind.label()) + 1),
    };
    value <= room
}

fn strip_place(kind: Kind) -> usize {
    STRIP_FACTS
        .iter()
        .position(|read| *read == kind)
        .unwrap_or(usize::MAX)
}

fn holds(facts: &[Fact], went: usize, counted: bool, shape: Shape, area: Rect) -> bool {
    match shape {
        Shape::Ledger => ledger_lines(facts, went).len() <= usize::from(area.height),
        Shape::Strip => strip_columns(facts, went, counted) <= usize::from(area.width),
    }
}

fn strip_columns(facts: &[Fact], went: usize, counted: bool) -> usize {
    let mut taken = 0;
    for (index, fact) in facts.iter().enumerate() {
        if index > 0 {
            taken += drawn(GAP);
        }
        taken += drawn(fact.kind.label()) + 1 + fact.value.columns();
    }
    if counted {
        if !facts.is_empty() {
            taken += drawn(GAP);
        }
        taken += drawn(&more(went));
    }
    taken
}

/// The label of one row in its field, and the column a value `value_width` columns wide starts at:
/// the rule both the ledger's shapes and the screen that widens it draw a row by, so values end on
/// one hanging edge wherever they are read.
pub(crate) fn label_row(
    buffer: &mut Buffer,
    area: Rect,
    label: &str,
    value_width: usize,
    palette: &Palette,
) -> u16 {
    pen::run(buffer, area.x, area.y, &field(label), Ink::Dim, palette);
    hanging(area, value_width)
}

/// The column a value `value_width` columns wide starts at in a row `area` wide: the one hanging
/// edge, which a labelled row and a row that is a value and no label share.
pub(crate) fn hanging(area: Rect, value_width: usize) -> u16 {
    let start = usize::from(area.width).saturating_sub(value_width);
    area.x
        .saturating_add(u16::try_from(start).unwrap_or(area.width))
}

/// The rule between two fact groups, which costs the group under it a row of the band it is drawn
/// on.
pub(crate) fn hairline(buffer: &mut Buffer, area: Rect, palette: &Palette) {
    let rule = hairline_glyphs(area.width);
    pen::run(buffer, area.x, area.y, &rule, Ink::Dim, palette);
}

/// The columns a labelled row drawn in `width` columns leaves its value: the field comes off the
/// front, and a value left less than that is not drawn at all.
pub(crate) fn value_room(width: u16) -> usize {
    usize::from(width).saturating_sub(usize::from(FIELD))
}

/// Spells each block's pairs in one pair of half widths, so every slash in the block lands in one
/// column and the digits either side of it line up.
fn aligned(facts: &mut [Fact]) {
    for block in BLOCKS {
        let shared = Halves::of(
            facts
                .iter()
                .filter(|fact| block.contains(&fact.kind))
                .map(|fact| fact.value.text()),
        );
        for fact in facts.iter_mut().filter(|fact| block.contains(&fact.kind)) {
            fact.value.align(shared);
        }
    }
}

#[cfg(test)]
mod tests {
    //! The facts a run holds and the one way each is spelled: the value a launch's frame is owed
    //! and a run has yet to fill, the size in spelled tokens, the head of a name and the tail of a
    //! path, each measured by the columns it draws in.

    use std::path::{Path, PathBuf};

    use sindri_core::policy::{Enforcement, PolicyRecord};
    use sindri_core::provider::Usage as Report;
    use sindri_core::records::{Dials, SessionDocument, Usage};
    use sindri_core::run::State as RunState;
    use sindri_core::session::Session;
    use sindri_core::wire::Wire;

    use super::*;
    use crate::pen;

    /// The columns a rail's row leaves a value: its thirty-two less the ten its label field holds.
    const ROOM: usize = 22;

    #[test]
    fn the_facts_a_launch_holds_before_a_run_are_what_a_prompt_is_pointed_at() {
        let facts = Facts::boot(boot());

        assert_eq!(
            (
                facts.wire.as_deref(),
                facts.model.as_deref(),
                facts.workspace.as_deref(),
                facts.branch.as_deref(),
                facts.sandbox,
            ),
            (
                Some("anthropic"),
                Some("claude-sonnet-4-5"),
                Some(Path::new("/srv/workspace")),
                Some("main"),
                Some(Enforcement::Bubblewrap),
            ),
            "the wire, the model, the workspace, the branch and the enforcement are the \
             launch's own facts"
        );
        assert_eq!(DASH, "—", "the mark a fact owed is drawn as is an em dash");
        assert_eq!(
            (facts.session.as_deref(), facts.temp.as_deref()),
            (Some(DASH), Some(Path::new(DASH))),
            "a session and a temp are a run's: neither exists yet, so each is an em dash and \
             never a guess"
        );
        assert_eq!(
            (
                facts.think,
                facts.turns,
                facts.ctx,
                facts.cache,
                facts.skills
            ),
            (None, None, None, None, None),
            "a launch knows no thinking, no turn count, no size, no hit rate and no skill: they \
             arrive with a run, and no drawing guesses at one"
        );
    }

    #[test]
    fn a_session_is_named_by_the_first_eight_of_its_guid_and_whole_when_it_is_shorter() {
        assert_eq!(
            short_id("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"),
            "9f2c7a41",
            "no rail column holds a GUID of thirty-six"
        );
        assert_eq!(
            short_id("9f2c"),
            "9f2c",
            "a name already short is left whole"
        );
        assert_eq!(short_id(""), "", "and no name at all is no columns of one");
    }

    #[test]
    fn the_facts_a_session_holds_are_the_ones_a_frame_draws_it_by() {
        let root = tempfile::tempdir().expect("a temp state home");
        let session = Session::create(root.path()).expect("a session");
        let document = document(session.guid());

        let facts = Facts::of(&session, &document);

        assert_eq!(
            facts.wire.as_deref(),
            Some("anthropic"),
            "the dialect the document was written for, in the wire's own word"
        );
        assert_eq!(facts.model.as_deref(), Some("claude-sonnet"));
        assert_eq!(facts.turns, Some((3, 250)), "the turns opened, and the cap");
        assert_eq!(facts.sandbox, Some(Enforcement::Bubblewrap));
        assert_eq!(
            facts.workspace.as_deref(),
            Some(Path::new("/home/glemsom/src/sindri")),
            "the directory the run stood in"
        );
        assert_eq!(facts.branch.as_deref(), Some("spec/1-capability"));
        assert_eq!(
            facts.session.as_deref(),
            Some(session.guid()),
            "the session's own GUID, which the rail draws short"
        );
        assert_eq!(
            facts.temp.as_deref(),
            Some(session.temp()),
            "and the temp the sandbox maps to TMPDIR"
        );
        assert_eq!(
            (facts.ctx, facts.cache, facts.skills),
            (None, None, None),
            "a document holds what the run spent summed over every turn, so the size of one \
             ask and the cache's hit rate are handed in and not read off it"
        );
    }

    /// The one launch a boot frame is drawn from: a workspace that is no checkout of this test's,
    /// so a fact nobody can know yet has nowhere to be read from but the rail itself.
    fn boot() -> Boot {
        Boot {
            wire: Some(Wire::Anthropic),
            model: Some(String::from("claude-sonnet-4-5")),
            workspace: PathBuf::from("/srv/workspace"),
            branch: Some(String::from("main")),
            enforcement: Enforcement::Bubblewrap,
        }
    }

    #[test]
    fn the_size_and_the_cache_a_turns_report_carries_are_the_facts_a_frame_draws() {
        let root = tempfile::tempdir().expect("a temp state home");
        let session = Session::create(root.path()).expect("a session");
        let document = document(session.guid());
        let asked = Report {
            input: Some(48_200),
            cached_input: Some(12_050),
            ..Report::default()
        };
        let uncached = Report {
            input: Some(48_200),
            ..Report::default()
        };

        let facts = Facts::of(&session, &document).measured(&asked, None);

        assert_eq!(
            facts.ctx,
            Some(Ctx {
                size: 48_200,
                window: None
            }),
            "the input the turn carried, and no window until a reader names one"
        );
        assert_eq!(
            facts.cache,
            Some(25),
            "a quarter of every input token came from the provider's own cache"
        );

        assert_eq!(
            Facts::of(&session, &document)
                .measured(&uncached, None)
                .cache,
            None,
            "a report that names no cached token has no hit rate to draw"
        );
    }

    #[test]
    fn a_report_that_states_no_size_measures_nothing() {
        let root = tempfile::tempdir().expect("a temp state home");
        let session = Session::create(root.path()).expect("a session");
        let document = document(session.guid());
        let reported = Report {
            input: Some(48_200),
            cached_input: Some(4_820),
            ..Report::default()
        };

        assert_eq!(
            Facts::of(&session, &document)
                .measured(&reported, Some(200_000))
                .ctx,
            Some(Ctx {
                size: 48_200,
                window: Some(200_000)
            }),
            "the window a reader named, beside the size this report measured"
        );

        assert_eq!(
            (
                Facts::of(&session, &document)
                    .measured(&Report::default(), Some(200_000))
                    .ctx,
                Facts::of(&session, &document)
                    .measured(&Report::default(), None)
                    .cache
            ),
            (None, None),
            "a report that carries no token is no size to be a share of a window and no \
             input to make a hit rate of, so the row is absent rather than nought"
        );
    }

    #[test]
    fn the_facts_a_session_holds_say_whether_the_model_thinks() {
        for (thinking, word) in [(true, "high"), (false, "off")] {
            let root = tempfile::tempdir().expect("a temp state home");
            let session = Session::create(root.path()).expect("a session");
            let mut document = document(session.guid());
            document.thinking = thinking;

            let facts = Facts::of(&session, &document);

            assert_eq!(
                facts.think.as_deref(),
                Some(word),
                "a document that says {thinking} is drawn as {word} and never a third word"
            );
        }
    }

    #[test]
    fn the_facts_of_a_run_take_the_dialect_its_document_names() {
        let root = tempfile::tempdir().expect("a temp state home");
        let session = Session::create(root.path()).expect("a session");

        for (wire, word) in [
            (Wire::Anthropic, "anthropic"),
            (Wire::Openai, "openai"),
            (Wire::Copilot, "copilot"),
        ] {
            let mut document = document(session.guid());
            document.wire = wire;

            let facts = Facts::of(&session, &document);

            assert_eq!(
                facts.wire.as_deref(),
                Some(word),
                "the dialect the document names, in the word the rail draws it by"
            );
        }
    }

    #[test]
    fn a_pair_holding_a_wide_glyph_still_lands_its_slash_beside_the_slash_above_it() {
        let shared = Halves::of(["9 / 250", "日本語 / 2"].into_iter());

        let narrow = padded("9 / 250", shared);
        let wide = padded("日本語 / 2", shared);
        let half = |spelling: &str| {
            let (one, _) = spelling.split_once('/').expect("a pair");
            pen::cells(one.trim_end())
        };

        assert_eq!(
            half(&narrow),
            half(&wide),
            "a half holding three wide glyphs draws in more columns than a half holding one \
             digit, so it is padded by the columns it leaves and not by the glyphs it is made \
             of: {narrow:?} and {wide:?}"
        );
        assert_eq!(
            half(&wide),
            u16::try_from(shared.left).expect("a half width"),
            "and the half is exactly its group's width, so the two slashes stand in one column"
        );
    }

    #[test]
    fn a_path_keeps_the_components_that_fit_and_marks_the_elision_in_front() {
        assert_eq!(
            tail("/glemsom/src/sindri", ROOM),
            "/glemsom/src/sindri",
            "a path that fits is drawn whole, with no mark to explain"
        );
        assert_eq!(
            tail("/very/long/path/that/cannot/fit/whole", ROOM),
            "…/cannot/fit/whole",
            "a path that does not fit keeps its tail and says the head went"
        );
        assert_eq!(
            tail("/the-name-that-will-not-fit-anywhere", ROOM),
            "",
            "no component fits the column, so the fact has no value to draw"
        );
        assert_eq!(
            tail("/w/日本語", 8),
            "…/日本語",
            "a component of wide glyphs takes the columns it draws in, so the tail that fits \
             eight is the one whose glyphs take eight and never the one whose count does"
        );
    }

    #[test]
    fn a_served_name_keeps_the_segments_that_fit_and_marks_the_cut_at_its_tail() {
        assert_eq!(
            head("claude-sonnet-4-5-20250929", ROOM),
            "claude-sonnet-4-5…",
            "a dated name keeps the family a reader can name, and marks the date stamp it \
             left behind"
        );
        assert_eq!(
            head("claude-sonnet-4-5", ROOM),
            "claude-sonnet-4-5",
            "a name that fits is drawn whole, with no mark to explain"
        );
        assert_eq!(
            head("anthropic.claude-3-5-sonnet-20241022-v2:0", ROOM),
            "anthropic.claude-3-5…",
            "a gateway name is cut at a segment boundary, and never mid-segment"
        );
        assert_eq!(
            head("claude-sonnet-4-5-20250929", "claude".len() + 1),
            "claude…",
            "the mark takes a column of its own, so the head stops one short of the room"
        );
        assert_eq!(
            head("aservednamethatnobreaksanywhere", ROOM),
            "",
            "a name whose first segment does not fit beside the mark has no head to keep"
        );
    }

    #[test]
    fn a_served_name_holding_a_wide_glyph_is_cut_by_the_columns_it_draws_in() {
        let wide = "claude-sonnet-日本語-日本-20250929";

        let cut = head(wide, ROOM);

        assert_eq!(
            cut, "claude-sonnet-日本語…",
            "a segment of six glyphs is twelve columns, so the second one leaves the head no \
             room: the cut is by what the glyphs draw in and not by how many there are"
        );
        assert!(
            pen::cells(&cut) <= u16::try_from(ROOM).expect("a value column"),
            "and the head it keeps draws in no more columns than the value column holds"
        );
        assert_eq!(
            head("日本語日本語日本語日本語", ROOM),
            "",
            "a name of wide glyphs and no segment to break at is wider than its column and \
             keeps no head at all, rather than a head wider than the rail"
        );
    }

    fn document(guid: &str) -> SessionDocument {
        SessionDocument {
            session: guid.to_owned(),
            wire: Wire::Anthropic,
            model: String::from("claude-sonnet"),
            enforcement: Enforcement::Bubblewrap,
            policy: PolicyRecord {
                enforcement: Enforcement::Bubblewrap,
                writable: vec![String::from("/home/glemsom/src/sindri")],
                timeout_seconds: 120,
                command_budget_ms: 3_600_000,
                output_budget_bytes: 8 * 1024 * 1024,
                description: String::from("the boundary the model was told about"),
            },
            workspace: PathBuf::from("/home/glemsom/src/sindri"),
            branch: Some(String::from("spec/1-capability")),
            commit: Some(String::from("16becb5")),
            dials: Dials {
                max_turns: 250,
                command_budget_ms: 3_600_000,
                output_budget_bytes: 8 * 1024 * 1024,
            },
            start: String::from("2026-01-01T00:00:00Z"),
            end: None,
            turns: 3,
            usage: Usage::default(),
            stopped: RunState::Running,
            route: None,
            thinking: false,
        }
    }

    /// The band a shape is laid on, which is the share the adapter hands in.
    fn band(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// The dated Anthropic name a real run is served, which is wider than a value column.
    const SERVED: &str = "claude-sonnet-4-5-20250929";

    /// What the ledger draws of [`SERVED`]: its head, and the mark saying the date stamp went.
    const CUT: &str = "claude-sonnet-4-5…";

    /// Every fact a frame can draw, so a layout test reads a full ledger rather than a slice of one.
    fn frame() -> Facts {
        Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from(SERVED)),
            think: Some(String::from("high")),
            turns: Some((3, 250)),
            ctx: Some(Ctx {
                size: 48_200,
                window: None,
            }),
            cache: Some(88),
            workspace: Some(PathBuf::from(
                "/home/glemsom/Documents/git/sindri/.worktrees/46-rail",
            )),
            branch: Some(String::from("spec/1-capability")),
            skills: Some(2),
            sandbox: Some(Enforcement::Bubblewrap),
            session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
            temp: Some(PathBuf::from(
                "/home/glemsom/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
            )),
        }
    }

    /// One row as a test states it: the column each of its parts starts in, and the glyphs it draws.
    fn stated(parts: &[(u16, &str)]) -> Vec<(u16, String)> {
        parts
            .iter()
            .map(|(at, text)| (*at, String::from(*text)))
            .collect()
    }

    /// The rows a shape lays on `area`, each read as the columns its parts start in and the glyphs
    /// they draw.
    fn laid(facts: &Facts, shape: Shape, area: Rect) -> Vec<Vec<(u16, String)>> {
        rows(facts, shape, area)
            .iter()
            .map(|row| {
                row.parts()
                    .iter()
                    .map(|(at, part)| {
                        let drawn = match part {
                            Part::Run(text, _) => text.clone(),
                            Part::Value(value) => value.text().to_owned(),
                        };
                        (*at, drawn)
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_ledger_lays_its_blocks_in_the_contracts_order_with_a_hairline_between_two() {
        let hairline = "─".repeat(usize::from(COLUMNS));

        assert_eq!(
            laid(&frame(), Shape::Ledger, band(COLUMNS, 17)),
            [
                stated(&[(0, "wire      "), (23, "anthropic")]),
                stated(&[(0, "model     "), (14, CUT)]),
                stated(&[(0, "think     "), (28, "high")]),
                stated(&[(0, &hairline)]),
                stated(&[(0, "turns     "), (25, "3 / 250")]),
                stated(&[(0, "ctx       "), (27, "48.2k")]),
                stated(&[(0, "cache     "), (29, "88%")]),
                stated(&[(0, &hairline)]),
                stated(&[(0, "workspace "), (12, "…/.worktrees/46-rail")]),
                stated(&[(0, "branch    "), (15, "spec/1-capability")]),
                stated(&[(0, &hairline)]),
                stated(&[(0, "skills    "), (31, "2")]),
                stated(&[(0, &hairline)]),
                stated(&[(0, "sandbox   "), (27, "bwrap")]),
                stated(&[(0, &hairline)]),
                stated(&[(0, "session   "), (24, "9f2c7a41")]),
                stated(&[(0, "temp      "), (18, "…/9f2c7a41/tmp")]),
            ],
            "six blocks, a hairline between two of them, and nothing given up at this height"
        );
    }

    #[test]
    fn the_strip_lays_its_four_facts_on_one_row_with_one_gap_between_two() {
        assert_eq!(
            laid(&frame(), Shape::Strip, band(40, 1)),
            [stated(&[
                (0, "ctx "),
                (4, "48.2k"),
                (9, " · "),
                (12, "model "),
                (18, CUT),
            ])],
            "the strip's one row reads its own four facts, and the room left is not enough for the \
             count of the eight it does not carry"
        );
    }

    #[test]
    fn a_band_too_short_gives_facts_up_in_the_sacrifice_order_and_the_last_row_counts_what_went() {
        let hairline = "─".repeat(usize::from(COLUMNS));

        assert_eq!(
            laid(&frame(), Shape::Ledger, band(COLUMNS, 8)),
            [
                stated(&[(0, "wire      "), (23, "anthropic")]),
                stated(&[(0, "model     "), (14, CUT)]),
                stated(&[(0, "think     "), (28, "high")]),
                stated(&[(0, &hairline)]),
                stated(&[(0, "turns     "), (25, "3 / 250")]),
                stated(&[(0, &hairline)]),
                stated(&[(0, "sandbox   "), (27, "bwrap")]),
                stated(&[(0, "+7 more")]),
            ],
            "temp, cache, session, branch, skills, ctx and workspace went, in that order, and the \
             foot of the ledger counts the seven"
        );
    }

    #[test]
    fn the_ledger_and_the_strip_give_up_the_same_fact_for_the_same_reason() {
        let unnamed = Facts {
            model: Some(String::from("aservednamethatnobreaksanywhere")),
            ..Facts::default()
        };

        assert_eq!(
            laid(&unnamed, Shape::Ledger, band(COLUMNS, 2)),
            [stated(&[(0, "+1 more")])],
            "a name cut to nothing says nothing, so the ledger drops it and counts it"
        );
        assert_eq!(
            laid(&unnamed, Shape::Strip, band(40, 1)),
            [stated(&[(0, "+1 more")])],
            "and the strip gives up the same fact, in the same words"
        );
    }

    #[test]
    fn no_row_is_laid_past_the_band_it_is_drawn_in() {
        for width in 20..=200u16 {
            let shape = Shape::of(width);
            let (room, height) = match shape {
                Shape::Ledger => (COLUMNS, 40),
                Shape::Strip => (width, 1),
            };
            let laid = rows(&frame(), shape, band(room, height));

            assert!(
                laid.len() <= usize::from(height),
                "at {width} columns the {shape:?} lays {} rows on a band {height} high",
                laid.len()
            );
            for row in &laid {
                for (at, part) in row.parts() {
                    assert!(
                        usize::from(*at) + usize::from(part.columns()) <= usize::from(room),
                        "at {width} columns the {shape:?} lays a part running from {at} for {} \
                         columns on a band {room} wide",
                        part.columns()
                    );
                }
            }
        }
    }

    #[test]
    fn the_share_is_the_ledgers_columns_beside_the_transcript_or_the_strips_bottom_row() {
        assert_eq!(
            split(&frame(), band(120, 37)),
            Share {
                transcript: band(88, 37),
                rail: Some((
                    Shape::Ledger,
                    Rect {
                        x: 88,
                        ..band(COLUMNS, 37)
                    }
                )),
            },
            "a hundred columns and up holds a ledger down the frame's right-hand edge, band high"
        );
        assert_eq!(
            split(&frame(), band(99, 21)),
            Share {
                transcript: band(99, 20),
                rail: Some((
                    Shape::Strip,
                    Rect {
                        y: 20,
                        height: 1,
                        ..band(99, 1)
                    }
                )),
            },
            "below a hundred the strip takes the band's bottom row and no columns"
        );
    }

    #[test]
    fn a_frame_that_knows_no_fact_leaves_the_whole_band_to_the_transcript() {
        assert_eq!(
            split(&Facts::default(), band(120, 37)),
            Share {
                transcript: band(120, 37),
                rail: None,
            },
            "a ledger with nothing in it is not drawn, and nothing narrows for it"
        );
    }

    #[test]
    fn a_hundred_columns_is_the_last_frame_the_ledger_is_a_column_in() {
        assert_eq!(
            Shape::of(100),
            Shape::Ledger,
            "the strip is what a frame below a hundred columns gets, so a hundred still holds the \
             ledger"
        );
        assert_eq!(Shape::of(99), Shape::Strip, "and ninety-nine is below it");
        assert_eq!(Shape::of(20), Shape::Strip, "the narrowest frame there is");
        assert_eq!(Shape::of(200), Shape::Ledger, "the widest");
    }

    #[test]
    fn the_ledger_gives_facts_up_in_the_sacrifice_order_and_the_sandbox_never_goes() {
        assert_eq!(
            SACRIFICE.map(Kind::label),
            [
                "temp",
                "cache",
                "session",
                "branch",
                "skills",
                "ctx",
                "workspace",
                "turns",
                "wire",
                "think",
                "model"
            ],
            "the sacrifice order, the first to go first"
        );
        assert!(
            !SACRIFICE.contains(&Kind::Sandbox),
            "the sandbox posture is kept at every width, so no loop over the order can reach it"
        );
    }

    #[test]
    fn the_strip_carries_its_own_list_in_the_order_a_reader_watches_it() {
        assert_eq!(
            STRIP_FACTS.map(Kind::label),
            ["ctx", "model", "sandbox", "branch"],
            "the thing you watch, the thing you are, the thing that confines you, and the thing \
             you are standing on"
        );

        let tail = STRIP_FACTS.map(Kind::label);
        let tail = &tail[1..];
        assert_eq!(
            STRIP_SACRIFICE.map(Kind::label),
            [tail[2], tail[1], tail[0]],
            "and what the strip gives up is that list without its head, read from the tail, so \
             the run's size is the one fact it never goes without"
        );
        assert!(
            !STRIP_FACTS.contains(&Kind::Turns),
            "turns stays out of the list: the strip has no room to say both how full the \
             conversation is and how many turns it took to fill"
        );
    }

    #[test]
    fn no_served_name_is_laid_in_part_at_any_width_between_twenty_and_two_hundred() {
        for name in [
            "claude-sonnet-4-5",
            SERVED,
            "anthropic.claude-3-5-sonnet-20241022-v2:0",
        ] {
            assert_no_width_lays_a_name_in_part(name);
        }
    }

    /// One served name at every width: whole when it fits, its head and the mark when it does not,
    /// and never a piece of a segment on any width.
    fn assert_no_width_lays_a_name_in_part(name: &str) {
        let mut cut = 0;
        for width in 20..=200u16 {
            let shape = Shape::of(width);
            let room = match shape {
                Shape::Ledger => COLUMNS,
                Shape::Strip => width,
            };
            let facts = Facts {
                model: Some(String::from(name)),
                ..Facts::default()
            };
            let value = rows(&facts, shape, band(room, 1))
                .iter()
                .flat_map(Row::parts)
                .find_map(|(_, part)| match part {
                    Part::Value(value) => Some(value.text().to_owned()),
                    Part::Run(..) => None,
                });
            let Some(value) = value else {
                continue;
            };

            assert!(
                value == name || value.ends_with('…'),
                "at {width} columns {name:?} is laid as {value:?}, which is neither the whole \
                 name nor a name with the mark that says the rest went"
            );
            assert!(
                name.starts_with(value.trim_end_matches('…')),
                "at {width} columns {name:?} is laid as {value:?}: the head survives and the \
                 tail is what goes"
            );
            assert!(
                fits(&Fact::new(Kind::Model, value.clone()), shape, room),
                "at {width} columns {name:?} is laid as {value:?}, which no row of the \
                 {shape:?} has room for"
            );
            if value.ends_with('…') {
                cut += 1;
            }
        }
        if drawn(name) <= VALUE {
            assert_eq!(
                cut, 0,
                "{name:?} fits a value column, so no width in the sweep may cut it"
            );
        } else {
            assert!(
                cut > 0,
                "{name:?} is wider than the value column, so some width in the sweep lays it cut"
            );
        }
    }
}
