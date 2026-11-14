//! The run's facts, the one way each of them is spelled and the rows they are laid out in: what the
//! wire, the record and the launch say, and what the rail and the screen draw of it, cut to the room
//! each row leaves.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::policy::Enforcement;
use sindri_core::provider::Route;
use sindri_core::provider::Usage as Report;
use sindri_core::records::{SessionDocument, Usage};
use sindri_core::run::State;
use sindri_core::session::Session;
use sindri_core::wire::Wire;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen;
use crate::pulse::Pulse;
use crate::rows::stop_mark;

/// The columns `text` draws in, a wide glyph counted as the two cells it takes: a value is
/// measured by what a row shows and never by how many glyphs it is made of.
fn drawn(text: &str) -> usize {
    usize::from(pen::cells(text))
}

/// The frame width the ledger gives way at: the narrowest that holds the transcript's floor beside
/// the rail's own, below which the rail is one strip.
pub const NARROW: u16 = FLOOR + TRANSCRIPT;

/// The narrowest the rail's column is drawn, and the widest: a frame that holds a ledger is handed
/// the floor, and one wide enough for the ceiling is handed no more.
pub(crate) const FLOOR: u16 = 32;
pub(crate) const CEILING: u16 = 44;

/// The columns the transcript keeps before the rail takes any more than its floor: the rail grows
/// into what a frame holds over this, so the transcript's own width moves by a column at a time.
const TRANSCRIPT: u16 = 68;

/// The two shapes a frame's rail is drawn in: the ledger's column beside a transcript, which has
/// room for a pair beside its meter, and the strip along a band's bottom row, which has room for
/// the meter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rail {
    Ledger,
    Strip,
}

impl Rail {
    /// The rail a frame `width` columns wide draws: the strip below [`NARROW`] columns, and the
    /// ledger's column from it up.
    pub fn of(width: u16) -> Rail {
        if width < NARROW {
            Rail::Strip
        } else {
            Rail::Ledger
        }
    }

    /// The columns a frame `width` wide is drawn down its right-hand edge: what it holds over the
    /// transcript between its floor and its ceiling. A wide frame draws a served name whole where a
    /// narrow one cuts it to its head, and a frame narrower than the floor is handed all of itself.
    pub const fn columns(width: u16) -> u16 {
        let over = width.saturating_sub(TRANSCRIPT);
        let columns = if over < FLOOR {
            FLOOR
        } else if over > CEILING {
            CEILING
        } else {
            over
        };
        if columns > width { width } else { columns }
    }
}

/// What a drawing reads its facts in: one of the rail's shapes, or the screen, which is the same
/// ledger widened to as many columns as the band holds a whole one in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Ledger,
    Strip,
    Screen,
}

impl From<Rail> for Shape {
    fn from(rail: Rail) -> Shape {
        match rail {
            Rail::Ledger => Shape::Ledger,
            Rail::Strip => Shape::Strip,
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
    /// the meter, because its one row is shared with four other facts.
    pub(crate) fn spelled(&self, shape: Shape) -> Value {
        let Some(fill) = self.fill() else {
            return Value::new(tokens(self.size));
        };
        let text = match shape {
            Shape::Ledger | Shape::Screen => pair(tokens(self.size), tokens(fill.window)),
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
    Route,
    Input,
    Output,
    Total,
    Cached,
    Reasoning,
    Commit,
    State,
    Stopped,
    Exit,
    Start,
    End,
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
            Kind::Route => "route",
            Kind::Input => "input",
            Kind::Output => "output",
            Kind::Total => "total",
            Kind::Cached => "cached",
            Kind::Reasoning => "reasoning",
            Kind::Commit => "commit",
            Kind::State => "state",
            Kind::Stopped => "stopped",
            Kind::Exit => "exit",
            Kind::Start => "start",
            Kind::End => "end",
        }
    }

    /// Which way a fact of this kind hangs: the kinds whose value is a number shape the digits,
    /// and the kinds whose value is a word are said as one phrase with the label.
    fn hang(self) -> Hang {
        match self {
            Kind::Turns
            | Kind::Ctx
            | Kind::Cache
            | Kind::Skills
            | Kind::Input
            | Kind::Output
            | Kind::Total
            | Kind::Cached
            | Kind::Reasoning
            | Kind::Exit => Hang::Right,
            _ => Hang::Left,
        }
    }
}

/// Which edge of its row a fact's value hangs on: a number against the row's last column, so the
/// digits of two numbers line up, and a word from the label, so the two read as one phrase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hang {
    Left,
    Right,
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
    pub sandbox: Option<Enforcement>,
    /// The run's own, and an em dash on a frame that has not run yet.
    pub session: Option<String>,
    /// The run's own, and an em dash on a frame that has not run yet.
    pub temp: Option<PathBuf>,
    /// The endpoint an OpenAI-shaped wire served.
    pub route: Option<Route>,
    /// Every ask of the run summed, which is the spend and no conversation's size.
    pub spend: Usage,
    pub commit: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    /// The run's state, in the word it is running under or the stop it ended on.
    pub state: Option<State>,
    /// The skills the run loaded, in load order: a rail column has room for how many and the screen
    /// for every name.
    pub loaded: Vec<String>,
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
            state: Some(document.stopped.clone()),
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

    /// The facts a run's record wrote, added to the ones a frame was handed: what the run spent and
    /// the state it ended on are the record's to say, and no drawing reads one off it again.
    pub fn recorded(mut self, document: &SessionDocument, loaded: &[String]) -> Facts {
        self.route = document.route;
        self.spend = document.usage;
        self.commit = document.commit.clone();
        self.start = Some(document.start.clone());
        self.end = document.end.clone();
        self.state = Some(document.stopped.clone());
        self.loaded = loaded.to_vec();
        self
    }

    /// The size one ask's report measured, and the part of its input the provider served from cache.
    /// The document cannot hand these in: the `usage` it keeps is every ask summed, and a sum of the
    /// inputs grows with each turn while the conversation's size is what the newest request carried.
    pub fn measured(mut self, ask: &Report, window: Option<u64>) -> Facts {
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

/// The columns a label field holds, whatever the label says, so every label starts in one column
/// and every value hangs from the field's far side.
pub(crate) const FIELD: u16 = 10;

/// The columns the strip's one row cuts a value to: the room the rail's floor leaves beside its
/// field, which is what a row carrying five facts can count on.
const VALUE: usize = (FLOOR - FIELD) as usize;

/// What lies between two facts on the strip's one row, the gap the footer puts between its hints.
pub(crate) const GAP: &str = " · ";

const HAIRLINE: char = '─';

/// The ledger's blocks, top to bottom, each with the facts that belong in it: the order is a
/// contract, and a fact's place in it is fixed. The state stands first, because what the run is
/// doing is what a reader looks for before what it holds.
const BLOCKS: [&[Kind]; 7] = [
    &[Kind::State],
    &[Kind::Wire, Kind::Model, Kind::Think],
    &[Kind::Turns, Kind::Ctx, Kind::Cache],
    &[Kind::Workspace, Kind::Branch],
    &[Kind::Skills],
    &[Kind::Sandbox],
    &[Kind::Session, Kind::Temp],
];

/// The five facts the strip carries, in the order it reads them: its one row asks a different
/// question than the ledger's blocks do, so its list is its own.
const STRIP_FACTS: [Kind; 5] = [
    Kind::Ctx,
    Kind::State,
    Kind::Model,
    Kind::Sandbox,
    Kind::Branch,
];

/// The order the strip gives its facts up in: the state goes after every fact it may, so what the
/// run is doing outlives the model, the posture and the branch; and the size it never gives up
/// stands ahead of it.
const STRIP_SACRIFICE: [Kind; 4] = [Kind::Branch, Kind::Sandbox, Kind::Model, Kind::State];

/// The ledger's sacrifice order, the first to go first. The sandbox posture is deliberately absent,
/// because it never goes, so no loop over this list can reach it; the state goes last, after every
/// fact the rail may give up.
const SACRIFICE: [Kind; 12] = [
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
    Kind::State,
];

/// The rail's band as a picture cuts it: the cells a picture is drawn over at the top of the rail
/// and the rows the ledger's own rows are laid in beneath them. A rail that draws no picture
/// reserves nothing and gives the facts its whole band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailBand {
    /// Every row the rail paints: its ground and its edges are drawn over all of it.
    pub area: Rect,
    /// The cells the picture covers, centred in the rail's columns, and nothing when none is drawn.
    pub picture: Option<Rect>,
    /// The rows the facts are laid in: the band under the picture, or the whole band.
    pub facts: Rect,
}

impl RailBand {
    /// The rail's whole band, with no row of it reserved.
    pub fn of(area: Rect) -> RailBand {
        RailBand {
            area,
            picture: None,
            facts: area,
        }
    }

    /// The same band with a picture `cols` by `rows` drawn over its top, centred in its columns, a
    /// row under it kept for the rule that closes the face, and every fact laid beneath that. A
    /// picture larger than the band is cut to it rather than drawn past the rail's own rows.
    pub fn with_picture(mut self, cols: u16, rows: u16) -> RailBand {
        let cols = cols.min(self.area.width);
        let rows = rows.min(self.area.height);
        let x = self
            .area
            .x
            .saturating_add(self.area.width.saturating_sub(cols) / 2);
        self.picture = Some(Rect {
            x,
            y: self.area.y,
            width: cols,
            height: rows,
        });
        let under = rows.saturating_add(1).min(self.area.height);
        self.facts = Rect {
            y: self.area.y.saturating_add(under),
            height: self.area.height.saturating_sub(under),
            ..self.area
        };
        self
    }
}

/// What the transcript and the rail share of a band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share {
    pub transcript: Rect,
    /// Nothing when the frame knows no fact at all, and the transcript keeps the whole band.
    pub rail: Option<(Rail, RailBand)>,
}

/// Cuts the band the transcript and the rail share: the rail's own columns down the right while the
/// frame is wide enough to hold a column, one strip along the band's bottom row below it. A frame
/// that knows no fact draws no rail, and the transcript loses no room to one.
pub fn split(facts: &Facts, band: Rect) -> Share {
    let shape = Rail::of(band.width);
    if band.width == 0 || band.height == 0 || known(facts, shape.into(), None).is_empty() {
        return Share {
            transcript: band,
            rail: None,
        };
    }
    let (transcript, rail) = match shape {
        Rail::Ledger => {
            let columns = Rail::columns(band.width);
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
        Rail::Strip => (
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
        rail: Some((shape, RailBand::of(rail))),
    }
}

/// The rows `shape` lays on `area`, in the order it reads them: the row of the band each stands on
/// and the column each part starts at, never more of them than the band holds. A live rail's state
/// row wears the marker's step, and a shape handed none draws none.
pub(crate) fn rows(facts: &Facts, shape: Shape, area: Rect, marker: Option<&Pulse>) -> Vec<Row> {
    match shape {
        Shape::Screen => sheet_rows(facts, area),
        Shape::Ledger => rail_rows(facts, Rail::Ledger, area, marker),
        Shape::Strip => rail_rows(facts, Rail::Strip, area, marker),
    }
}

/// Draws the rows a shape laid, and nothing else: the one place a row's parts reach a frame, so an
/// adapter is the band it is handed and no rule of its own.
pub(crate) fn draw(rows: &[Row], buffer: &mut Buffer, palette: &Palette) {
    for row in rows {
        for (x, part) in row.parts() {
            part.draw(buffer, *x, row.y(), palette);
        }
    }
}

/// The ledger's rows, or the strip's one: the blocks the rail reads, an empty row between two that
/// both kept a fact, and the count of what went in a row of its own.
fn rail_rows(facts: &Facts, rail: Rail, area: Rect, marker: Option<&Pulse>) -> Vec<Row> {
    let kept = kept(facts, rail, area, marker);
    match rail {
        Rail::Ledger => ledger_lines(&kept.facts, kept.went)
            .into_iter()
            .take(usize::from(area.height))
            .enumerate()
            .map(|(offset, line)| {
                let row = Rect {
                    y: area
                        .y
                        .saturating_add(u16::try_from(offset).unwrap_or(u16::MAX)),
                    ..area
                };
                match line {
                    Line::Fact(fact) => labelled(row, fact),
                    Line::Air => Row {
                        y: row.y,
                        parts: Vec::new(),
                    },
                    Line::Went(went) => Row::run(row.x, row.y, more(went), Ink::Muted),
                }
            })
            .collect(),
        Rail::Strip => vec![strip(&kept, area)],
    }
}

/// The screen's rows: each column's blocks in the order the ledger reads them, a rule over every
/// block below the first in its column, and the count of what the band could not hold on the row
/// under the last column's last one.
fn sheet_rows(facts: &Facts, area: Rect) -> Vec<Row> {
    let sheet = sheet(&known(facts, Shape::Screen, None), area);
    let mut rows = Vec::new();
    for column in &sheet.columns {
        let mut y = area.y;
        for block in &column.blocks {
            let at = Rect {
                x: column.x,
                y,
                width: column.width,
                height: 1,
            };
            if block.ruled {
                rows.push(hairline_row(at));
                y = y.saturating_add(1);
            }
            rows.push(Row::run(at.x, y, block.label, Ink::Dim));
            y = y.saturating_add(1);
            for fact in &block.rows {
                rows.push(labelled(Rect { y, ..at }, fact));
                y = y.saturating_add(1);
            }
        }
    }
    if sheet.went > 0
        && let Some(column) = sheet.columns.last()
    {
        let y = rows.last().map_or(area.y, |row| row.y().saturating_add(1));
        rows.push(Row::run(column.x, y, more(sheet.went), Ink::Muted));
    }
    rows
}

/// One row of a shape, laid out: the row of the band it stands on, the runs it draws and the column
/// of the band each starts at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    y: u16,
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
    fn run(x: u16, y: u16, text: impl Into<String>, ink: Ink) -> Row {
        Row {
            y,
            parts: vec![(x, Part::Run(text.into(), ink))],
        }
    }

    pub(crate) fn y(&self) -> u16 {
        self.y
    }

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
    pub(crate) fn columns(&self) -> u16 {
        let columns = match self {
            Part::Run(text, _) => drawn(text),
            Part::Value(value) => value.columns(),
        };
        u16::try_from(columns).unwrap_or(u16::MAX)
    }

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

/// A fact's row: its label in the field from the row's left edge, and its value hanging the way the
/// value hangs — a number on the far edge of the band, a word from the field. A fact whose label is
/// no word hangs its value from the row's left edge.
fn labelled(area: Rect, fact: &Fact) -> Row {
    let mut parts = Vec::new();
    let mut from = area.x;
    if let Some(label) = fact.label {
        parts.push((area.x, Part::Run(pen::pad(label, FIELD), Ink::Dim)));
        from = area.x.saturating_add(FIELD);
    }
    let x = match fact.hang {
        Hang::Right => pen::hanging(area, pen::columns(fact.value.columns()))
            .expect("a fact is fitted to the row it is laid in"),
        Hang::Left => from,
    };
    parts.push((x, Part::Value(fact.value.clone())));
    Row { y: area.y, parts }
}

/// The rule between two blocks, which costs the block under it a row of the band it is drawn on.
fn hairline_row(area: Rect) -> Row {
    Row::run(area.x, area.y, pen::filled(area.width, HAIRLINE), Ink::Dim)
}

/// The strip's one row: its facts read left to right with a gap between two, and the count of what
/// went at the end when the room holds it.
fn strip(kept: &Kept, area: Rect) -> Row {
    let mut row = Row {
        y: area.y,
        parts: Vec::new(),
    };
    let mut x = area.x;
    for fact in &kept.facts {
        if !row.parts.is_empty() {
            x = row.push(x, Part::Run(GAP.to_owned(), Ink::Dim));
        }
        if let Some(label) = fact.label {
            x = row.push(x, Part::Run(format!("{label} "), Ink::Dim));
        }
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

/// How a narrow column takes a value: a word whole or not at all, a path cut at its head where a
/// reader looks for its tail, a name cut at its tail where the family lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut {
    Word,
    /// Cut at its head, marked with `…/`: the tail of a path is what a reader is looking for.
    Path,
    /// Cut at its tail, marked with `…`: the head of a served name is what a reader is looking for.
    Name,
}

impl Cut {
    /// The value cut to the room a row leaves it, or none when nothing of it fits: a word too wide
    /// for its room is not drawn in part, and a path or a name cut to nothing says nothing.
    fn shorten(self, text: &str, room: usize) -> Option<String> {
        let cut = match self {
            Cut::Word => return (drawn(text) <= room).then(|| text.to_owned()),
            Cut::Path => tail(text, room),
            Cut::Name => head(text, room),
        };
        (!cut.is_empty()).then_some(cut)
    }
}

/// One fact as a shape draws it: the kind that names its place in the blocks, the rule its value is
/// cut by when the room runs out, and the value beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Fact {
    kind: Kind,
    /// `None` for a value that says its own name: a loaded skill hangs where a value hangs, with no
    /// field in front of it.
    label: Option<&'static str>,
    cut: Cut,
    hang: Hang,
    value: Value,
}

impl Fact {
    fn new(kind: Kind, value: impl Into<String>) -> Fact {
        Fact {
            kind,
            label: Some(kind.label()),
            cut: Cut::Word,
            hang: kind.hang(),
            value: Value::new(value),
        }
    }

    /// A fact whose value keeps its head and gives up its tail when the room runs out.
    fn named(kind: Kind, name: impl Into<String>) -> Fact {
        Fact {
            cut: Cut::Name,
            ..Fact::new(kind, name)
        }
    }

    /// A fact whose value keeps its tail: a path is read from the end a reader knows.
    fn path(kind: Kind, path: &str) -> Fact {
        Fact {
            cut: Cut::Path,
            ..Fact::new(kind, path)
        }
    }

    /// A value already spelled, meter and ink and all.
    fn valued(kind: Kind, value: Value) -> Fact {
        Fact {
            value,
            ..Fact::new(kind, "")
        }
    }

    /// A name that is its own label: a loaded skill is drawn where a value hangs and no field in
    /// front of it.
    fn bare(kind: Kind, name: impl Into<String>) -> Fact {
        Fact {
            label: None,
            hang: Hang::Left,
            ..Fact::named(kind, name)
        }
    }

    /// The fact cut to the room a row leaves its value, meter and all — the one rule a value's cut
    /// is, with the rail's column and the screen's as the two cases it takes.
    fn fitted(&self, room: usize) -> Option<Fact> {
        let room = match self.value.meter() {
            None => room,
            Some(_) => room.checked_sub(METER + 1)?,
        };
        let value = self
            .value
            .respelled(self.cut.shorten(self.value.text(), room)?);
        (value.columns() > 0).then(|| Fact {
            value,
            ..self.clone()
        })
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
    Air,
    Went(usize),
}

/// The ledger gives its facts up in a settled order beside the row that says how many went; the
/// strip gives them up from the tail of its own list, but its count is the last item of its one
/// row, so the room takes the count first.
fn kept(facts: &Facts, rail: Rail, area: Rect, marker: Option<&Pulse>) -> Kept {
    let known = known(facts, rail.into(), marker);
    let total = known.len();
    let mut kept: Vec<Fact> = known
        .into_iter()
        .filter(|fact| rail == Rail::Ledger || STRIP_FACTS.contains(&fact.kind))
        .filter_map(|fact| {
            let room = match rail {
                Rail::Ledger => value_columns(&fact, area.width),
                Rail::Strip => VALUE,
            };
            let fitted = fact.fitted(room)?;
            let holds = rail != Rail::Strip || strip_item(&fitted) <= usize::from(area.width);
            holds.then_some(fitted)
        })
        .collect();
    let order: &[Kind] = match rail {
        Rail::Ledger => &SACRIFICE,
        Rail::Strip => &STRIP_SACRIFICE,
    };
    // The ledger's count is one of its rows, so the room it takes is part of what the facts are
    // given up for; the strip's is measured once the facts are in, below.
    let with_count = rail == Rail::Ledger;
    let mut went = total - kept.len();
    while !holds(&kept, went, with_count, rail, area) {
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
    let counted = went > 0 && holds(&kept, went, true, rail, area);
    Kept {
        facts: kept,
        went,
        counted,
    }
}

/// Every fact this frame knows, in the order the shape reads them, each value spelled whole the way
/// that shape spells it. What the room then takes off a value is the cut's business, not this
/// spelling's.
fn known(facts: &Facts, shape: Shape, marker: Option<&Pulse>) -> Vec<Fact> {
    let short = facts.session.as_deref().map(short_id);
    let mut stated = Vec::new();
    if let Some(wire) = &facts.wire {
        stated.push(Fact::new(Kind::Wire, wire.clone()));
    }
    if let Some(model) = &facts.model {
        stated.push(Fact::named(Kind::Model, model.clone()));
    }
    if shape == Shape::Screen
        && let Some(route) = facts.route
    {
        stated.push(Fact::new(Kind::Route, route.word()));
    }
    if let Some(think) = &facts.think {
        stated.push(Fact::new(Kind::Think, think.clone()));
    }
    if let Some((turns, cap)) = facts.turns {
        stated.push(Fact::new(Kind::Turns, pair(turns, cap)));
    }
    if let Some(ctx) = &facts.ctx {
        stated.push(Fact::valued(Kind::Ctx, ctx.spelled(shape)));
    }
    if let Some(cache) = facts.cache {
        stated.push(Fact::new(Kind::Cache, hit_rate(cache)));
    }
    if shape == Shape::Screen {
        for (kind, count) in [
            (Kind::Input, facts.spend.input),
            (Kind::Output, facts.spend.output),
            (Kind::Total, facts.spend.total),
            (Kind::Cached, facts.spend.cached_input),
            (Kind::Reasoning, facts.spend.reasoning),
        ] {
            if let Some(count) = count {
                stated.push(Fact::new(kind, tokens(count)));
            }
        }
    }
    if let Some(workspace) = &facts.workspace {
        stated.push(Fact::path(Kind::Workspace, &workspace.to_string_lossy()));
    }
    if let Some(branch) = &facts.branch {
        stated.push(Fact::new(Kind::Branch, branch.clone()));
    }
    if shape == Shape::Screen
        && let Some(commit) = &facts.commit
    {
        stated.push(Fact::new(Kind::Commit, commit.clone()));
    }
    if shape == Shape::Screen {
        for name in &facts.loaded {
            stated.push(Fact::bare(Kind::Skills, name.clone()));
        }
    } else if !facts.loaded.is_empty() {
        stated.push(Fact::new(Kind::Skills, facts.loaded.len().to_string()));
    }
    if shape == Shape::Screen
        && let Some(state) = &facts.state
    {
        let ink = match state.is_running() {
            true => Ink::Accent,
            false => Ink::Plain,
        };
        stated.push(Fact::valued(
            Kind::Stopped,
            Value::new(state.word()).inked(ink),
        ));
        if let Some(stop) = state.stop() {
            stated.push(Fact::new(Kind::Exit, stop.exit_code().to_string()));
        }
        for instant in [(Kind::Start, &facts.start), (Kind::End, &facts.end)] {
            if let Some(at) = instant.1 {
                stated.push(Fact::new(instant.0, at.clone()));
            }
        }
    }
    if shape != Shape::Screen
        && let Some(state) = &facts.state
    {
        stated.push(Fact::valued(Kind::State, state_value(state, marker)));
    }
    if let Some(sandbox) = facts.sandbox {
        let word = match shape {
            Shape::Ledger | Shape::Strip => sandbox.rail_label(),
            Shape::Screen => sandbox.word(),
        };
        stated.push(Fact::new(Kind::Sandbox, word.to_owned()));
    }
    match shape {
        // A screen holds the whole of what a rail shortens: no column of it is thirty-two wide.
        Shape::Screen => {
            if let Some(guid) = &facts.session {
                stated.push(Fact::new(Kind::Session, guid.clone()));
            }
            if let Some(temp) = &facts.temp {
                stated.push(Fact::path(Kind::Temp, &temp.to_string_lossy()));
            }
        }
        Shape::Ledger | Shape::Strip => {
            if let Some(short) = short {
                stated.push(Fact::new(Kind::Session, short.to_owned()));
            }
            if let Some(temp) = &facts.temp {
                let path = temp.to_string_lossy();
                // The temp's path carries the session's whole GUID, so it is drawn under the
                // eight-column name the session row above it uses.
                let named = match (short, facts.session.as_deref()) {
                    (Some(short), Some(guid)) => path.replace(guid, short),
                    _ => path.into_owned(),
                };
                stated.push(Fact::path(Kind::Temp, &named));
            }
        }
    }
    match shape {
        Shape::Ledger => aligned(&mut stated, BLOCKS.iter().copied()),
        Shape::Screen => aligned(&mut stated, SCREEN_BLOCKS.iter().map(|(_, kinds)| *kinds)),
        Shape::Strip => stated.sort_by_key(|fact| strip_place(fact.kind)),
    }
    stated
}

/// The run's own row: the state's own word under the glyph the frame or the stop earns. A live row
/// takes the marker's step and a stopped row the mark that stop earns, so no two drawings of one
/// run disagree about what it is doing.
fn state_value(state: &State, marker: Option<&Pulse>) -> Value {
    let Some(stop) = state.stop() else {
        let word = match marker.and_then(Pulse::stepping) {
            Some(glyph) => format!("{glyph} {}", state.word()),
            None => state.word().to_owned(),
        };
        return Value::new(word).inked(Ink::Accent);
    };
    let (glyph, ink) = stop_mark(stop);
    Value::new(format!("{glyph} {}", state.word())).inked(ink)
}

/// The ledger's rows in the order they are read, with an empty row between two blocks that both
/// kept a fact and the count of what went at the foot when anything did.
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
            lines.push(Line::Air);
        }
        lines.extend(facts.into_iter().map(Line::Fact));
        written += 1;
    }
    if went > 0 {
        lines.push(Line::Went(went));
    }
    lines
}

/// The columns a fact's value is cut to on one row `width` wide: the whole column for a value with
/// no label field in front of it, and the room a labelled row leaves beside the field.
fn value_columns(fact: &Fact, width: u16) -> usize {
    match fact.label {
        None => usize::from(width),
        Some(_) => value_room(width),
    }
}

/// The columns one fact takes on the strip's row: its label, the column between the two, and the
/// value beside them.
fn strip_item(fact: &Fact) -> usize {
    fact.label.map_or(0, |label| drawn(label) + 1) + fact.value.columns()
}

fn strip_place(kind: Kind) -> usize {
    STRIP_FACTS
        .iter()
        .position(|read| *read == kind)
        .unwrap_or(usize::MAX)
}

fn holds(facts: &[Fact], went: usize, counted: bool, rail: Rail, area: Rect) -> bool {
    match rail {
        Rail::Ledger => ledger_lines(facts, went).len() <= usize::from(area.height),
        Rail::Strip => strip_columns(facts, went, counted) <= usize::from(area.width),
    }
}

fn strip_columns(facts: &[Fact], went: usize, counted: bool) -> usize {
    let mut taken = 0;
    for (index, fact) in facts.iter().enumerate() {
        if index > 0 {
            taken += drawn(GAP);
        }
        taken += strip_item(fact);
    }
    if counted {
        if !facts.is_empty() {
            taken += drawn(GAP);
        }
        taken += drawn(&more(went));
    }
    taken
}

/// The columns a screen's block is never narrower than: a band that cannot hold a whole one gets one
/// anyway.
const MIN_COLUMN: u16 = 40;

/// Not the strip's gap, which separates two facts on one row: this one stands between two columns.
const COLUMN_GAP: u16 = 2;

/// The screen's blocks in the order it reads them, each with the word it is headed by and the facts
/// that belong in it.
const SCREEN_BLOCKS: [(&str, &[Kind]); 8] = [
    ("run", &[Kind::Wire, Kind::Model, Kind::Route, Kind::Think]),
    ("meters", &[Kind::Turns, Kind::Ctx, Kind::Cache]),
    (
        "spend",
        &[
            Kind::Input,
            Kind::Output,
            Kind::Total,
            Kind::Cached,
            Kind::Reasoning,
        ],
    ),
    ("place", &[Kind::Workspace, Kind::Branch, Kind::Commit]),
    ("stop", &[Kind::Stopped, Kind::Exit, Kind::Start, Kind::End]),
    ("confine", &[Kind::Sandbox]),
    ("skills", &[Kind::Skills]),
    ("session", &[Kind::Session, Kind::Temp]),
];

/// The screen's blocks as the columns hold them: a block is drawn whole where it fits or moved on to
/// the next column, never split, so a block's name stands over every row it belongs to.
#[derive(Default)]
struct Sheet {
    columns: Vec<Column>,
    /// The rows no column could hold, counted under the last column's last row.
    went: usize,
}

/// One column of the screen: the column of the band it starts at, its width, and the blocks it holds.
struct Column {
    x: u16,
    width: u16,
    blocks: Vec<Placed>,
}

/// One block as a column holds it: the word it is read under, whether a rule stands over it, and the
/// facts that were left after the cut.
struct Placed {
    label: &'static str,
    ruled: bool,
    rows: Vec<Fact>,
}

/// The screen's blocks flowed into columns, the narrowest the band holds a whole column in: the
/// columns are there for the room they add, so a wider band never narrows one, and a block whole is
/// what a column holds or what it hands on.
fn sheet(known: &[Fact], area: Rect) -> Sheet {
    let blocks = blocks(known);
    if area.width == 0 || area.height == 0 || blocks.is_empty() {
        return Sheet::default();
    }
    let holds = columns(area.width, blocks.len());
    for count in 1..=holds {
        let sheet = flow(
            &blocks,
            area,
            count,
            column_width(area.width, count),
            area.height,
        );
        if sheet.went == 0 {
            return sheet;
        }
    }
    let width = column_width(area.width, holds);
    match area.height {
        0 | 1 => flow(&blocks, area, holds, width, area.height),
        height => flow(&blocks, area, holds, width, height - 1),
    }
}

/// The screen's blocks, in the order it reads them, each with the facts that belong in it: a block no
/// fact is left in is no block.
fn blocks(known: &[Fact]) -> Vec<(&'static str, Vec<&Fact>)> {
    SCREEN_BLOCKS
        .iter()
        .map(|(label, kinds)| {
            let facts: Vec<&Fact> = known
                .iter()
                .filter(|fact| kinds.contains(&fact.kind))
                .collect();
            (*label, facts)
        })
        .filter(|(_, facts)| !facts.is_empty())
        .collect()
}

/// Lays the blocks on `count` columns of `width`, each column `rows` high, and counts the rows no
/// column could hold: a block that does not fit the column it stands in moves on to the next, and
/// the last column counts the tail of the screen rather than a hole in the middle of it.
fn flow(
    blocks: &[(&'static str, Vec<&Fact>)],
    area: Rect,
    count: usize,
    width: u16,
    rows: u16,
) -> Sheet {
    let mut columns: Vec<Column> = (0..count)
        .map(|at| Column {
            x: area
                .x
                .saturating_add((at as u16).saturating_mul(width.saturating_add(COLUMN_GAP))),
            width,
            blocks: Vec::new(),
        })
        .collect();
    let mut room = vec![rows; count];
    let mut went = 0;
    let mut at = 0;
    for (index, (label, facts)) in blocks.iter().enumerate() {
        let drawn: Vec<Fact> = facts
            .iter()
            .filter_map(|fact| fact.fitted(value_columns(fact, width)))
            .collect();
        if drawn.is_empty() {
            went += facts.len();
            continue;
        }
        let height = u16::try_from(drawn.len())
            .unwrap_or(u16::MAX)
            .saturating_add(1);
        // A block below the first in its column is separated from it by a rule, and the rule takes
        // a row of the column it stands in.
        let needs = |column: &Column| height + u16::from(!column.blocks.is_empty());
        let Some(place) = (at..count).find(|column| room[*column] >= needs(&columns[*column]))
        else {
            went += blocks[index..]
                .iter()
                .map(|(_, facts)| facts.len())
                .sum::<usize>();
            break;
        };
        went += facts.len() - drawn.len();
        room[place] -= needs(&columns[place]);
        at = place;
        let ruled = !columns[place].blocks.is_empty();
        columns[place].blocks.push(Placed {
            label,
            ruled,
            rows: drawn,
        });
    }
    columns.retain(|column| !column.blocks.is_empty());
    Sheet { columns, went }
}

/// The columns a band `width` wide is cut into: as many as hold a whole one, and never more than
/// there are blocks to fill.
fn columns(width: u16, blocks: usize) -> usize {
    let holds = (width.saturating_add(COLUMN_GAP)) / (MIN_COLUMN + COLUMN_GAP);
    usize::from(holds.max(1)).min(blocks)
}

fn column_width(width: u16, count: usize) -> u16 {
    let Some(count) = u16::try_from(count).ok().filter(|count| *count > 0) else {
        return 0;
    };
    width
        .saturating_sub(COLUMN_GAP.saturating_mul(count - 1))
        .checked_div(count)
        .unwrap_or(0)
}

/// The columns a labelled row drawn in `width` columns leaves its value: the field comes off the
/// front, and a value left less than that is not drawn at all.
pub(crate) fn value_room(width: u16) -> usize {
    usize::from(width).saturating_sub(usize::from(FIELD))
}

/// Spells each block's pairs in one pair of half widths, so every slash in the block lands in one
/// column and the digits either side of it line up.
fn aligned<'a>(facts: &mut [Fact], blocks: impl IntoIterator<Item = &'a [Kind]>) {
    for block in blocks {
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
    use sindri_core::provider::Route;
    use sindri_core::provider::Usage as Report;
    use sindri_core::records::{Dials, SessionDocument, Usage};
    use sindri_core::redact::SecretSet;
    use sindri_core::run::{Budget, State as RunState, Stop};
    use sindri_core::session::Session;
    use sindri_core::wire::Wire;

    use super::*;
    use crate::pen;
    use crate::readback::{column_of, inks};

    /// The columns a rail's row leaves a value: the rail's floor less the ten its label field
    /// holds.
    const ROOM: usize = (FLOOR - FIELD) as usize;

    /// The band a ledger test stands in: the rail at its floor, which is what the narrowest frame
    /// that holds one is handed.
    const COLUMNS: u16 = FLOOR;

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
                facts.loaded.len()
            ),
            (None, None, None, None, 0),
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
            (facts.ctx, facts.cache, facts.loaded.len()),
            (None, None, 0),
            "a document holds what the run spent summed over every turn, so the size of one \
             ask and the cache's hit rate are handed in and not read off it"
        );
    }

    #[test]
    fn the_facts_a_record_wrote_join_the_facts_the_frame_was_handed() {
        let document = SessionDocument {
            route: Some(Route::Chat),
            usage: Usage {
                input: Some(48_200),
                output: Some(3_100),
                ..Usage::default()
            },
            end: Some(String::from("2026-01-01T00:04:00Z")),
            stopped: RunState::Stopped(Stop::Final),
            ..document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")
        };

        let facts = Facts::default().recorded(&document, &[String::from("bash")]);

        assert_eq!(
            (
                facts.route,
                facts.spend,
                facts.state.as_ref().map(RunState::word)
            ),
            (Some(Route::Chat), document.usage, Some("final")),
            "the route the run took, what it spent and the state it stopped in are a record's \
             own facts"
        );
        assert_eq!(
            (
                facts.commit.as_deref(),
                facts.start.as_deref(),
                facts.end.as_deref()
            ),
            (
                Some("16becb5"),
                Some("2026-01-01T00:00:00Z"),
                Some("2026-01-01T00:04:00Z")
            ),
            "and so are the commit it stood on and the instants it was written at"
        );
        assert_eq!(facts.loaded, ["bash"], "and the names it loaded, whole");
        assert!(
            facts.wire.is_none() && facts.model.is_none(),
            "a record is no source of the facts the frame was handed: it adds the ones it wrote"
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
            end: Some(String::from("2026-01-01T00:04:00Z")),
            turns: 3,
            usage: Usage {
                input: Some(48_200),
                output: Some(3_100),
                total: Some(51_300),
                cached_input: Some(12_000),
                reasoning: Some(900),
            },
            stopped: RunState::Stopped(Stop::Final),
            route: Some(Route::Chat),
            thinking: true,
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

    /// A gateway's name: no column of a narrow band holds it, and it breaks at its segments.
    const GATEWAY: &str = "anthropic.claude-3-5-sonnet-20241022-v2:0";

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
            loaded: vec![String::from("bash"), String::from("records")],
            sandbox: Some(Enforcement::Bubblewrap),
            session: Some(String::from("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")),
            temp: Some(PathBuf::from(
                "/home/glemsom/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
            )),
            ..Facts::default()
        }
    }

    /// The one stop that carries a line of the run's own words.
    fn wire_refused() -> Stop {
        Stop::Error(SecretSet::default().redact("the wire went away"))
    }

    /// The facts a frame holds while its run is live.
    fn live() -> Facts {
        Facts {
            state: Some(RunState::Running),
            ..frame()
        }
    }

    /// A shape's rows on `area`, painted as the adapter paints them: the ground under every cell,
    /// then the rows the ledger laid.
    fn painted_shape(facts: &Facts, shape: Shape, area: Rect, marker: Option<&Pulse>) -> Buffer {
        let palette = Palette::TRUECOLOR;
        let mut buffer = Buffer::empty(area);
        pen::claim(&mut buffer, area, palette.ground(None));
        draw(&rows(facts, shape, area, marker), &mut buffer, &palette);
        buffer
    }

    #[test]
    fn the_rail_s_first_row_says_the_run_is_running_and_steps_with_the_marker() {
        let first = |at: Option<u64>| {
            laid(
                &live(),
                Shape::Ledger,
                band(COLUMNS, 19),
                at.map(Pulse::live).as_ref(),
            )
            .into_iter()
            .next()
            .expect("the rail's first row")
        };

        assert_eq!(
            first(Some(0)),
            stated(&[(0, "state     "), (10, "⠋ running")]),
            "a live run's first row says the state it is in, under the glyph the frame is on"
        );
        assert_eq!(
            first(Some(160)),
            stated(&[(0, "state     "), (10, "⠹ running")]),
            "and two steps of eighty milliseconds later the row has stepped with the frame"
        );
        assert_eq!(
            first(None),
            stated(&[(0, "state     "), (10, "running")]),
            "a rail handed no marker says the state and no glyph, never a step it has not taken"
        );

        let buffer = painted_shape(
            &live(),
            Shape::Ledger,
            band(COLUMNS, 19),
            Some(&Pulse::live(0)),
        );
        assert_eq!(
            Ink::of(
                buffer.cell((10, 0)).expect("the state's first cell"),
                &Palette::TRUECOLOR
            ),
            Some(Ink::Accent),
            "the row says something is happening, so it is read in the accent ink"
        );
    }

    #[test]
    fn a_stopped_run_s_row_is_the_stop_s_own_word_under_the_mark_that_stop_earns() {
        for (stop, glyph, ink) in [
            (Stop::Final, '✓', Ink::Ok),
            (Stop::MaxTurns, '✗', Ink::Err),
            (Stop::Budget(Budget::Clock), '✗', Ink::Err),
            (Stop::Cancelled, '✗', Ink::Err),
            (wire_refused(), '✗', Ink::Err),
        ] {
            let facts = Facts {
                state: Some(RunState::Stopped(stop.clone())),
                ..frame()
            };
            let area = band(COLUMNS, 19);
            let value = format!("{glyph} {}", stop.word());

            assert_eq!(
                laid(&facts, Shape::Ledger, area, None)[0],
                stated(&[(0, "state     "), (10, value.as_str())]),
                "a run stopped on {} says so in its own word, under the mark that stop wears",
                stop.word()
            );
            assert_eq!(
                Ink::of(
                    painted_shape(&facts, Shape::Ledger, area, None)
                        .cell((10, 0))
                        .expect("the state's first cell"),
                    &Palette::TRUECOLOR
                ),
                Some(ink),
                "and the mark is read in the ink that stop earns"
            );
        }
    }

    #[test]
    fn a_live_run_wears_no_stop_s_mark_whatever_the_frame_s_marker_settled_on() {
        let area = band(COLUMNS, 19);
        let settled = Pulse::settled(&Stop::MaxTurns, 1_240);

        assert_eq!(
            laid(&live(), Shape::Ledger, area, Some(&settled))[0],
            stated(&[(0, "state     "), (10, "running")]),
            "a run that is still going says running, and never the word its marker settled on"
        );
        assert_eq!(
            Ink::of(
                painted_shape(&live(), Shape::Ledger, area, Some(&settled))
                    .cell((10, 0))
                    .expect("the state's first cell"),
                &Palette::TRUECOLOR
            ),
            Some(Ink::Accent),
            "and it is read in the accent, so the row and the head never say two steps"
        );
    }

    #[test]
    fn the_state_is_the_last_fact_the_rail_gives_up_and_the_sandbox_outlives_it() {
        let facts = Facts {
            state: Some(RunState::Running),
            sandbox: Some(Enforcement::Bubblewrap),
            ..Facts::default()
        };

        assert_eq!(
            laid(&facts, Shape::Ledger, band(COLUMNS, 3), None),
            [
                stated(&[(0, "state     "), (10, "running")]),
                stated(&[]),
                stated(&[(0, "sandbox   "), (10, "bwrap")]),
            ],
            "a three-row rail holds the run's state, the air under it and the posture that \
             confines it"
        );
        assert_eq!(
            laid(&facts, Shape::Ledger, band(COLUMNS, 2), None),
            [
                stated(&[(0, "sandbox   "), (10, "bwrap")]),
                stated(&[(0, "+1 more")]),
            ],
            "and a two-row rail gives the state up before the sandbox, which never goes"
        );
    }

    #[test]
    fn the_strip_carries_the_state_beside_the_size_and_lets_it_go_after_every_fact_it_may() {
        let facts = Facts {
            state: Some(RunState::Running),
            ctx: Some(Ctx {
                size: 48_200,
                window: Some(200_000),
            }),
            ..frame()
        };
        let marker = Pulse::live(0);
        let strip = |width: u16| {
            glyphs(
                &painted_shape(&facts, Shape::Strip, band(width, 1), Some(&marker)),
                0,
            )
        };

        assert_eq!(
            strip(80),
            format!(
                "ctx █░░░░░ · state ⠋ running · model claude-sonnet-4-5… · sandbox bwrap{}",
                pen::blank(9)
            ),
            "a strip under the rail's threshold carries the run's state beside the size it \
             watches, and holds it where the branch it carries no room for has gone"
        );
        assert_eq!(
            strip(40),
            format!("ctx █░░░░░ · state ⠋ running · +11 more{}", pen::blank(1)),
            "and a narrower one keeps the state where the model has gone, counting what went"
        );
        assert_eq!(
            strip(27),
            format!("ctx █░░░░░ · +12 more{}", pen::blank(6)),
            "a strip too narrow to hold the size and the state beside it keeps the size alone, \
             because the state is the last fact the strip lets go and never the first"
        );

        for width in 28..NARROW {
            let row = strip(width);
            assert!(
                row.contains("state ⠋ running"),
                "a {width}-column strip holds the run's size and its state, so it says what \
                 the run is doing: {row:?}"
            );
            assert!(
                pen::cells(&row) <= width,
                "and no fact is laid past the {width} columns it was given: {row:?}"
            );
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
    fn laid(
        facts: &Facts,
        shape: Shape,
        area: Rect,
        marker: Option<&Pulse>,
    ) -> Vec<Vec<(u16, String)>> {
        rows(facts, shape, area, marker)
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

    /// The screen's rows on `area`, each read as the row of the band it is drawn on, the column
    /// each of its parts starts at, and the glyphs they draw.
    fn screen(facts: &Facts, area: Rect) -> Vec<(u16, Vec<(u16, String)>)> {
        rows(facts, Shape::Screen, area, None)
            .iter()
            .map(|row| {
                (
                    row.y(),
                    row.parts()
                        .iter()
                        .map(|(at, part)| {
                            let glyphs = match part {
                                Part::Run(text, _) => text.clone(),
                                Part::Value(value) => value.text().to_owned(),
                            };
                            (*at, glyphs)
                        })
                        .collect(),
                )
            })
            .collect()
    }

    /// The screen's rows as a reader sees them: the glyphs of each row, trimmed, and a rule across
    /// the row stated as the rule it is rather than as its eighty glyphs.
    fn read(facts: &Facts, area: Rect) -> Vec<String> {
        screen(facts, area)
            .into_iter()
            .map(|(_, parts)| {
                let glyphs: Vec<&str> = parts.iter().map(|(_, glyphs)| glyphs.as_str()).collect();
                if glyphs
                    .iter()
                    .all(|run| run.chars().all(|glyph| glyph == HAIRLINE))
                {
                    return String::from("rule");
                }
                glyphs
                    .iter()
                    .flat_map(|run| run.split_whitespace())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect()
    }

    /// Every fact the screen draws: the rail's own with a window named, and the ones the record
    /// wrote, over the two skills the run loaded.
    fn run() -> Facts {
        Facts {
            ctx: Some(Ctx {
                size: 48_200,
                window: Some(200_000),
            }),
            ..frame()
        }
        .recorded(
            &document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"),
            &[String::from("bash"), String::from("records")],
        )
    }

    /// The screen of a run painted into a band, as the adapter paints it: the ground under every
    /// cell, then the rows the ledger laid.
    fn painted(facts: &Facts, area: Rect) -> Buffer {
        let palette = Palette::TRUECOLOR;
        let mut buffer = Buffer::empty(area);
        pen::claim(&mut buffer, area, palette.ground(None));
        draw(
            &rows(facts, Shape::Screen, area, None),
            &mut buffer,
            &palette,
        );
        buffer
    }

    /// A row `width` columns wide whose value says its own name: a bare name is no number, so it
    /// starts the row it stands on.
    fn bare_row(value: &str, width: u16) -> String {
        let mut row = String::from(value);
        row.push_str(&pen::blank(width.saturating_sub(pen::cells(value))));
        row
    }

    /// A labelled row `width` columns wide whose value is a number: the label in the field and the
    /// value on the far edge.
    fn hung_row(label: &str, value: &str, width: u16) -> String {
        let at = pen::hanging(band(width, 1), pen::cells(value)).expect("a row that holds it");
        format!(
            "{}{}{value}",
            pen::pad(label, FIELD),
            pen::blank(at.saturating_sub(FIELD))
        )
    }

    /// A labelled row `width` columns wide whose value is a word: the label in the field and the
    /// value reading on from it.
    fn said_row(label: &str, value: &str, width: u16) -> String {
        let mut row = format!("{}{value}", pen::pad(label, FIELD));
        row.push_str(&pen::blank(width.saturating_sub(pen::cells(&row))));
        row
    }

    /// One row of a painted band, as a reader sees its glyphs.
    fn glyphs(buffer: &Buffer, y: u16) -> String {
        (buffer.area.x..buffer.area.right())
            .map(|x| {
                buffer
                    .cell((x, y))
                    .unwrap_or_else(|| panic!("row {y}, column {x} is outside the band"))
                    .symbol()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn a_fact_the_run_knows_nothing_about_is_not_drawn_and_its_block_goes_with_it() {
        let document = SessionDocument {
            route: None,
            branch: None,
            commit: None,
            end: None,
            usage: Usage::default(),
            stopped: RunState::Running,
            thinking: false,
            model: String::from("claude-sonnet"),
            ..document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")
        };
        let facts = Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from("claude-sonnet")),
            think: Some(String::from("off")),
            workspace: Some(PathBuf::from("/home/glemsom/src/sindri")),
            sandbox: Some(Enforcement::Bubblewrap),
            ..Facts::default()
        }
        .recorded(&document, &[]);

        let rows = read(&facts, band(81, 40));

        for gone in [
            "route", "meters", "spend", "branch", "commit", "exit", "end", "skills", "session",
        ] {
            assert!(
                !rows
                    .iter()
                    .any(|row| row == gone || row.starts_with(&format!("{gone} "))),
                "the run knows no {gone}, so the screen draws no row for it: {rows:?}"
            );
        }
        assert!(
            rows.contains(&String::from("think off")),
            "whether the model thinks is a fact the run always knows: {rows:?}"
        );
        assert!(
            rows.contains(&String::from("stopped running")),
            "a run still going says so, and says nothing about when it ended: {rows:?}"
        );
    }

    #[test]
    fn a_narrow_column_cuts_a_path_at_its_head_and_a_name_at_its_tail_and_draws_no_other_part() {
        let facts = Facts {
            model: Some(String::from(GATEWAY)),
            ..frame()
        }
        .recorded(
            &document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"),
            &[String::from("a-skill-name-that-runs-past-any-column-here")],
        );
        let area = band(40, 40);

        let drawn = read(&facts, area);

        assert!(
            drawn.contains(&String::from("model anthropic.claude-3-5-sonnet…")),
            "a served name too long for its column keeps its head and marks the tail: {drawn:?}"
        );
        assert!(
            drawn.contains(&String::from("a-skill-name-that-runs-past-any-column…")),
            "a loaded name is cut the same way: {drawn:?}"
        );
        assert!(
            !drawn.iter().any(|row| row.starts_with("session ")),
            "the GUID is thirty-six columns and the row holds thirty, so it is not drawn in \
             part: {drawn:?}"
        );
        for row in rows(&facts, Shape::Screen, area, None) {
            for (at, part) in row.parts() {
                assert!(
                    usize::from(*at) + usize::from(part.columns()) <= usize::from(area.width),
                    "every row of a narrow column lands inside it: {at} for {} columns in a \
                     {}-column band",
                    part.columns(),
                    area.width
                );
            }
        }
    }

    #[test]
    fn a_block_is_drawn_whole_and_what_the_band_cannot_hold_is_counted_under_its_last_column() {
        let facts = Facts { ..frame() }.recorded(
            &document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"),
            &[String::from("bash")],
        );
        let area = band(120, 12);

        let laid = screen(&facts, area);
        let names: Vec<(&str, u16)> = laid
            .iter()
            .filter_map(|(_, parts)| match parts.as_slice() {
                [(x, glyphs)] if glyphs.chars().any(char::is_alphabetic) => {
                    Some((glyphs.as_str(), *x))
                }
                _ => None,
            })
            .collect();

        assert_eq!(
            names,
            [
                ("run", 0),
                ("meters", 0),
                ("spend", 61),
                ("place", 61),
                ("+8 more", 61),
            ],
            "the blocks flow into the columns whole, a block that does not fit moves on rather \
             than being split, and the count of what went stands under the last column"
        );
        let (y, parts) = laid.last().cloned().expect("the band counts what went");
        assert_eq!(
            parts,
            vec![(61, String::from("+8 more"))],
            "the count of what went belongs to the last column, under its last row: {laid:?}"
        );
        assert_eq!(
            y,
            laid[laid.len() - 2].0 + 1,
            "and the screen's rows are contiguous, so the card drawn on them is as tall as the \
             rows it holds: {laid:?}"
        );
    }

    #[test]
    fn a_band_wide_enough_for_one_column_draws_one_however_wide_it_is() {
        let facts = run();

        for width in [84u16, 120, 200, 320] {
            let rows = read(&facts, band(width, 54));

            assert!(
                rows.contains(&String::from(
                    "session 9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"
                )),
                "a {width}-column band holds the whole screen in one column, so it draws one \
                 and the GUID whole: columns are there for the room they add and never to \
                 narrow a fact: {rows:?}"
            );
            assert_eq!(
                screen(&facts, band(width, 54))
                    .iter()
                    .filter_map(|(_, parts)| match parts.as_slice() {
                        [(_, glyphs)] if glyphs == "session" => Some(()),
                        _ => None,
                    })
                    .count(),
                1,
                "and one column of the blocks is all a {width}-column band draws"
            );
        }
    }

    #[test]
    fn no_row_of_a_screen_is_laid_past_the_band_it_is_drawn_in() {
        for width in 20..=200u16 {
            for height in 1..=20 {
                for row in rows(&run(), Shape::Screen, band(width, height), None) {
                    assert!(
                        row.y() < height,
                        "a row stands on row {} of a band {height} high",
                        row.y()
                    );
                    for (at, part) in row.parts() {
                        assert!(
                            usize::from(*at) + usize::from(part.columns()) <= usize::from(width),
                            "a part runs from {at} for {} columns on a {width}-column band",
                            part.columns()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_screen_draws_the_facts_the_ledger_holds_and_never_reads_one_off_the_record_again() {
        let elsewhere = Facts {
            wire: Some(String::from("openai")),
            model: Some(String::from("gpt-5")),
            think: Some(String::from("off")),
            branch: Some(String::from("main")),
            ..frame()
        }
        .recorded(&document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"), &[]);
        let drawn = read(&elsewhere, band(81, 40));

        for held in ["wire openai", "model gpt-5", "think off", "branch main"] {
            assert!(
                drawn.contains(&String::from(held)),
                "the record names another wire, another model and another branch, and the \
                 screen draws what the frame holds: {drawn:?}"
            );
        }

        let blank = Facts {
            model: None,
            ..frame()
        }
        .recorded(&document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"), &[]);
        let drawn = read(&blank, band(81, 40));

        assert!(
            !drawn.iter().any(|row| row.starts_with("model ")),
            "a served name the frame does not hold has no row, whatever the record names: \
             {drawn:?}"
        );
    }

    #[test]
    fn a_name_with_no_label_of_its_own_starts_the_row_it_stands_on() {
        let area = band(81, 40);

        let buffer = painted(&run(), area);

        let names: Vec<String> = (0..area.height)
            .map(|y| glyphs(&buffer, y))
            .filter(|row| row.trim() == "bash" || row.trim() == "records")
            .collect();
        assert_eq!(
            names,
            [bare_row("bash", 81), bare_row("records", 81)],
            "a name with no label of its own is no number, so it starts the row it stands on \
             with no field and no run of blanks in front of it"
        );
    }

    #[test]
    fn a_column_is_never_narrower_than_a_whole_one_and_a_screen_handed_nothing_draws_no_row() {
        for width in 20..=200u16 {
            let mut starts: Vec<u16> = rows(&run(), Shape::Screen, band(width, 40), None)
                .iter()
                .filter_map(|row| match row.parts() {
                    [(x, Part::Run(label, Ink::Dim))]
                        if !label.chars().all(|glyph| glyph == HAIRLINE) =>
                    {
                        Some(*x)
                    }
                    _ => None,
                })
                .collect();
            starts.sort_unstable();
            starts.dedup();
            let holds = usize::from((width.saturating_add(COLUMN_GAP)) / (MIN_COLUMN + COLUMN_GAP));

            assert!(
                starts.len() <= holds.max(1),
                "a {width}-column band holds {holds} whole columns, and the screen draws \
                 {}: {starts:?}",
                starts.len()
            );
            for start in starts {
                assert!(
                    start == 0 || start + MIN_COLUMN <= width,
                    "a column starts at {start} on a {width}-column band, and forty columns is \
                     the least this screen draws one at"
                );
            }
        }

        assert_eq!(
            rows(&Facts::default(), Shape::Screen, band(120, 40), None),
            [],
            "a screen handed no fact and no record has nothing to place"
        );
    }

    #[test]
    fn a_screen_row_sets_its_label_dim_and_its_value_plain_where_the_value_hangs() {
        let area = band(81, 40);

        let buffer = painted(&run(), area);

        let wire = glyphs(&buffer, 1);
        assert_eq!(
            wire,
            said_row("wire", "anthropic", 81),
            "the label in its field and the word reading on from it"
        );
        assert_eq!(
            inks(&buffer, 1, &Palette::TRUECOLOR),
            [(Ink::Dim, 10), (Ink::Plain, 71)],
            "the field is `dim` and the value is the fact, so it is `plain`"
        );
        assert_eq!(
            column_of(&glyphs(&buffer, 2), '/'),
            None,
            "the model's row is no pair"
        );
        assert_eq!(
            column_of(&glyphs(&buffer, 7), '/'),
            column_of(&glyphs(&buffer, 8), '/'),
            "the turns above the size line their slash up with it, so the digits either side \
             are a column"
        );
        assert_eq!(
            glyphs(&buffer, 5),
            pen::filled(area.width, '─'),
            "the rule spans the column between the run's last fact and the name of the block \
             under it"
        );
        assert!(
            glyphs(&buffer, 4).trim_end().starts_with("think"),
            "the fact above the rule is the last of the run block"
        );
        assert_eq!(glyphs(&buffer, 6).trim_end(), "meters");
        assert_eq!(
            inks(&buffer, 5, &Palette::TRUECOLOR),
            [(Ink::Dim, 81)],
            "the rule between two blocks is drawn in the ink a block's edge is read in"
        );
    }

    #[test]
    fn a_share_of_a_window_is_a_meter_and_a_hit_rate_is_no_meter_at_all() {
        let area = band(81, 40);

        let buffer = painted(&run(), area);

        let empty = u16::try_from(METER - 1).expect("the meter's cells fit a row");
        let meter = format!("{FULL_CELL}{}", pen::filled(empty, EMPTY_CELL));
        assert_eq!(
            glyphs(&buffer, 8),
            hung_row("ctx", &format!("{meter} 48.2k / 200k"), 81),
            "a quarter of the window fills one of the meter's six cells, beside the size it is \
             a share of"
        );
        assert_eq!(
            read(&run(), area)[9],
            "cache 88%",
            "a hit rate has no ceiling to be a share of, so it is the number and no meter"
        );
    }

    #[test]
    fn the_fact_that_says_the_run_is_moving_wears_the_accent() {
        let area = band(81, 40);
        let moved = SessionDocument {
            end: None,
            ..document("9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61")
        };

        for (stopped, ink) in [
            (RunState::Running, Ink::Accent),
            (RunState::Stopped(Stop::Final), Ink::Plain),
        ] {
            let facts = Facts { ..frame() }.recorded(
                &SessionDocument {
                    stopped,
                    ..moved.clone()
                },
                &[],
            );
            let buffer = painted(&facts, area);
            let row = (0..area.height)
                .find(|y| glyphs(&buffer, *y).trim_end().starts_with("stopped"))
                .expect("the screen draws the state the run stopped in");
            let at = (FIELD, row);

            assert_eq!(
                Ink::of(
                    buffer.cell(at).expect("a cell of the state"),
                    &Palette::TRUECOLOR
                ),
                Some(ink),
                "a run still going is the one fact on the screen that says something is \
                 happening, so it is read in the accent and not in the plain ink"
            );
        }
    }

    #[test]
    fn every_fact_a_run_holds_has_its_row_in_the_block_the_screen_reads_it_in() {
        assert_eq!(
            read(&run(), band(81, 40)),
            [
                "run",
                "wire anthropic",
                "model claude-sonnet-4-5-20250929",
                "route chat",
                "think high",
                "rule",
                "meters",
                "turns 3 / 250",
                "ctx 48.2k / 200k",
                "cache 88%",
                "rule",
                "spend",
                "input 48.2k",
                "output 3.1k",
                "total 51.3k",
                "cached 12k",
                "reasoning 900",
                "rule",
                "place",
                "workspace /home/glemsom/Documents/git/sindri/.worktrees/46-rail",
                "branch spec/1-capability",
                "commit 16becb5",
                "rule",
                "stop",
                "stopped final",
                "exit 0",
                "start 2026-01-01T00:00:00Z",
                "end 2026-01-01T00:04:00Z",
                "rule",
                "confine",
                "sandbox bubblewrap",
                "rule",
                "skills",
                "bash",
                "records",
                "rule",
                "session",
                "session 9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61",
                "temp …/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
            ],
            "every fact the run holds has its row, whole, in the block the screen reads it in: the \
             route it took, what it spent, the commit it stood on, the state it stopped in with its \
             instants, the names it loaded, and the whole of what a rail column shortens"
        );
    }

    #[test]
    fn the_screen_names_a_block_and_lays_a_row_for_each_fact_with_its_value_on_the_hanging_edge() {
        let facts = Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from(SERVED)),
            think: Some(String::from("high")),
            ..Facts::default()
        };

        assert_eq!(
            screen(&facts, band(81, 40)),
            [
                (0, stated(&[(0, "run")])),
                (1, stated(&[(0, "wire      "), (10, "anthropic")])),
                (2, stated(&[(0, "model     "), (10, SERVED)])),
                (3, stated(&[(0, "think     "), (10, "high")])),
            ],
            "a block is named in the column's first row and each fact below it carries its value \
             on the column's last column, whole where the column holds it"
        );
    }

    #[test]
    fn the_ledger_lays_its_blocks_in_the_order_they_are_read_with_air_between_two() {
        assert_eq!(
            laid(&frame(), Shape::Ledger, band(COLUMNS, 17), None),
            [
                stated(&[(0, "wire      "), (10, "anthropic")]),
                stated(&[(0, "model     "), (10, CUT)]),
                stated(&[(0, "think     "), (10, "high")]),
                stated(&[]),
                stated(&[(0, "turns     "), (25, "3 / 250")]),
                stated(&[(0, "ctx       "), (27, "48.2k")]),
                stated(&[(0, "cache     "), (29, "88%")]),
                stated(&[]),
                stated(&[(0, "workspace "), (10, "…/.worktrees/46-rail")]),
                stated(&[(0, "branch    "), (10, "spec/1-capability")]),
                stated(&[]),
                stated(&[(0, "skills    "), (31, "2")]),
                stated(&[]),
                stated(&[(0, "sandbox   "), (10, "bwrap")]),
                stated(&[]),
                stated(&[(0, "session   "), (10, "9f2c7a41")]),
                stated(&[(0, "temp      "), (10, "…/9f2c7a41/tmp")]),
            ],
            "six blocks, an empty row between two of them, and nothing given up at this height"
        );

        let rules: Vec<usize> = laid(&frame(), Shape::Ledger, band(COLUMNS, 17), None)
            .iter()
            .enumerate()
            .filter(|(_, parts)| {
                parts
                    .iter()
                    .any(|(_, glyphs)| glyphs.chars().all(|glyph| glyph == HAIRLINE))
            })
            .map(|(row, _)| row)
            .collect();
        assert!(
            rules.is_empty(),
            "the rail draws a rule on rows {rules:?}: the blocks are parted by air, and the \
             column's only rules are the one under the face and the shadow under its last row"
        );
    }

    #[test]
    fn the_strip_lays_the_facts_it_carries_on_one_row_with_one_gap_between_two() {
        assert_eq!(
            laid(&frame(), Shape::Strip, band(40, 1), None),
            [stated(&[
                (0, "ctx "),
                (4, "48.2k"),
                (9, " · "),
                (12, "model "),
                (18, CUT),
            ])],
            "the strip's one row reads the facts of its own list, and the room left is not \
             enough for the count of the eight it does not carry"
        );
    }

    #[test]
    fn a_band_too_short_gives_facts_up_in_the_sacrifice_order_and_the_last_row_counts_what_went() {
        assert_eq!(
            laid(&frame(), Shape::Ledger, band(COLUMNS, 8), None),
            [
                stated(&[(0, "wire      "), (10, "anthropic")]),
                stated(&[(0, "model     "), (10, CUT)]),
                stated(&[(0, "think     "), (10, "high")]),
                stated(&[]),
                stated(&[(0, "turns     "), (25, "3 / 250")]),
                stated(&[]),
                stated(&[(0, "sandbox   "), (10, "bwrap")]),
                stated(&[(0, "+7 more")]),
            ],
            "temp, cache, session, branch, skills, ctx and workspace went, in that order, and the \
             foot of the ledger counts the seven"
        );
    }

    #[test]
    fn a_numbers_digits_line_up_on_the_last_column_and_a_words_read_on_from_its_label() {
        let rows = laid(&frame(), Shape::Ledger, band(COLUMNS, 17), None);
        let value = |label: &str| -> (u16, String) {
            rows.iter()
                .find_map(|parts| match parts.as_slice() {
                    [(0, field), (at, glyphs)] if field.trim_end() == label => {
                        Some((*at, glyphs.clone()))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("the {label} row of the ledger"))
        };

        for label in ["turns", "ctx", "cache", "skills"] {
            let (at, glyphs) = value(label);
            assert_eq!(
                usize::from(at) + usize::from(pen::cells(&glyphs)),
                usize::from(COLUMNS),
                "{label}'s {glyphs:?} does not end in the rail's last column: a number hangs \
                 against that edge so its digits line up with every other number's"
            );
        }

        for label in [
            "wire",
            "model",
            "think",
            "workspace",
            "branch",
            "sandbox",
            "session",
            "temp",
        ] {
            let (at, _) = value(label);
            assert_eq!(
                at, FIELD,
                "{label}'s value starts in {at}: a word hangs from its label, so the two read \
                 as one phrase with no run of blanks between them"
            );
        }
    }

    #[test]
    fn a_name_that_goes_for_want_of_room_is_drawn_whole_once_the_rail_grew_to_hold_it() {
        for width in 20..=200u16 {
            let rail = Rail::of(width);
            let room = match rail {
                Rail::Ledger => Rail::columns(width),
                Rail::Strip => width,
            };
            let drawn_whole = match rail {
                Rail::Ledger => value_room(room) >= pen::cells(UNNAMED) as usize,
                Rail::Strip => false,
            };
            let expected = match drawn_whole {
                true => stated(&[(0, "model     "), (FIELD, UNNAMED)]),
                false => stated(&[(0, "+1 more")]),
            };

            let drawn = laid(&unnamed(), rail.into(), band(room, 2), None);
            assert_eq!(
                drawn,
                [expected],
                "at {width} columns the {rail:?} draws a name with no hyphen to break at as \
                 {drawn:?}: a name no column of it holds whole is given up and counted, and one a \
                 grown rail holds is drawn whole and never in part"
            );
        }
    }

    /// A served name with no hyphen to break at, so no cut of it can keep a segment of it.
    const UNNAMED: &str = "aservednamethatnobreaksanywhere";

    fn unnamed() -> Facts {
        Facts {
            model: Some(String::from(UNNAMED)),
            ..Facts::default()
        }
    }

    #[test]
    fn no_row_is_laid_past_the_band_it_is_drawn_in() {
        for width in 20..=200u16 {
            let rail = Rail::of(width);
            let room = match rail {
                Rail::Ledger => Rail::columns(width),
                Rail::Strip => width,
            };
            for height in 1..=20 {
                assert_laid_inside(&frame(), rail.into(), band(room, height), room);
            }
        }
    }

    /// Every row a shape lays on `area` starts inside the band and ends inside it, and the band's
    /// height holds them all.
    fn assert_laid_inside(facts: &Facts, shape: Shape, area: Rect, room: u16) {
        let laid = rows(facts, shape, area, None);

        assert!(
            laid.len() <= usize::from(area.height),
            "the {shape:?} lays {} rows on a band {} high and {room} wide",
            laid.len(),
            area.height
        );
        for row in &laid {
            for (at, part) in row.parts() {
                assert!(
                    usize::from(*at) + usize::from(part.columns()) <= usize::from(room),
                    "the {shape:?} lays a part running from {at} for {} columns on a band {room} \
                     wide.",
                    part.columns()
                );
            }
        }
    }

    #[test]
    fn the_share_is_the_ledgers_columns_beside_the_transcript_or_the_strips_bottom_row() {
        assert_eq!(
            split(&frame(), band(120, 37)),
            Share {
                transcript: band(120 - CEILING, 37),
                rail: Some((
                    Rail::Ledger,
                    RailBand::of(Rect {
                        x: 120 - CEILING,
                        ..band(CEILING, 37)
                    })
                )),
            },
            "a frame wide enough for the rail's ceiling holds a ledger that wide, band high, and \
             the transcript keeps what is left"
        );
        assert_eq!(
            split(&frame(), band(NARROW, 37)),
            Share {
                transcript: band(NARROW - FLOOR, 37),
                rail: Some((
                    Rail::Ledger,
                    RailBand::of(Rect {
                        x: NARROW - FLOOR,
                        ..band(FLOOR, 37)
                    })
                )),
            },
            "and the narrowest frame that holds a ledger is the transcript's own floor beside the \
             rail's"
        );
        assert_eq!(
            split(&frame(), band(NARROW - 1, 21)),
            Share {
                transcript: band(NARROW - 1, 20),
                rail: Some((
                    Rail::Strip,
                    RailBand::of(Rect {
                        y: 20,
                        height: 1,
                        ..band(NARROW - 1, 1)
                    })
                )),
            },
            "below that the strip takes the band's bottom row and no columns"
        );
    }

    #[test]
    fn the_band_under_a_picture_centres_it_and_lays_the_facts_beneath_the_rows_it_takes() {
        let area = band(COLUMNS, 37);
        assert_eq!(
            RailBand::of(area).with_picture(19, 11),
            RailBand {
                area,
                picture: Some(Rect {
                    x: 6,
                    y: 0,
                    width: 19,
                    height: 11,
                }),
                facts: Rect {
                    y: 12,
                    height: 25,
                    ..area
                },
            },
            "a nineteen-cell picture is centred in the rail's columns, one row under it is kept \
             for the rule that closes the face, and the facts take every row under that"
        );
        assert_eq!(
            RailBand::of(area).picture,
            None,
            "and a band with no picture drawn over it reserves no row"
        );
    }

    #[test]
    fn a_picture_larger_than_the_band_is_cut_to_it_rather_than_laid_outside_the_rail() {
        let area = band(COLUMNS, 4);
        let band = RailBand::of(area).with_picture(40, 9);

        assert_eq!(
            band.picture,
            Some(area),
            "a picture wider and taller than the band is drawn over the whole of it"
        );
        assert_eq!(
            band.facts.height, 0,
            "and leaves the facts no row, so none is laid outside the rail"
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
    fn the_rail_s_column_is_a_fact_about_the_frame_between_a_floor_and_a_ceiling() {
        assert_eq!(
            Rail::columns(NARROW),
            FLOOR,
            "the narrowest frame that holds a ledger takes the rail's floor and no less"
        );
        assert!(
            Rail::columns(NARROW + 24) > FLOOR,
            "and a wider frame gives the rail more of itself, so the column is a fact about \
             the frame and not a width written down"
        );
        assert_eq!(
            Rail::columns(400),
            CEILING,
            "but never more than the ceiling, however wide the real terminal is"
        );
        assert_eq!(
            Rail::columns(20),
            20,
            "and a frame narrower than the floor is handed all of itself, so the column is never \
             wider than the band it is drawn in"
        );
    }

    #[test]
    fn a_wide_frame_draws_the_whole_served_name_where_the_narrowest_ledger_cuts_it_to_its_head() {
        let facts = Facts {
            model: Some(String::from(SERVED)),
            ..Facts::default()
        };

        assert_eq!(
            model_drawn(&facts, NARROW),
            CUT,
            "the rail's floor leaves a value column narrower than a dated name, and the head \
             with its mark is what is drawn"
        );
        assert_eq!(
            model_drawn(&facts, NARROW + 60),
            SERVED,
            "and a wider frame's rail holds the served name whole: the value column is the \
             rail's own, so widening the rail is what stops the cut"
        );
    }

    /// The model's value as a frame `frame` columns wide draws it, read off the rail beside the
    /// transcript.
    fn model_drawn(facts: &Facts, frame: u16) -> String {
        let share = split(facts, band(frame, 24));
        let (shape, rail) = share.rail.expect("a rail in a frame that knows a fact");
        rows(facts, shape.into(), rail.facts, None)
            .iter()
            .flat_map(Row::parts)
            .find_map(|(_, part)| match part {
                Part::Value(value) => Some(value.text().to_owned()),
                Part::Run(..) => None,
            })
            .expect("the model's own row")
    }

    #[test]
    fn the_width_a_frame_loses_its_ledger_at_is_the_rails_floor_beside_the_transcripts() {
        assert_eq!(
            NARROW,
            FLOOR + TRANSCRIPT,
            "the threshold is derived: the narrowest frame that holds a ledger is the rail's own \
             floor beside the columns the transcript keeps"
        );
        assert_eq!(
            Rail::of(NARROW),
            Rail::Ledger,
            "so the narrowest frame the transcript still has its floor in holds the ledger"
        );
        assert_eq!(
            Rail::of(NARROW - 1),
            Rail::Strip,
            "and one column under it is the strip, which costs the transcript no columns"
        );
        assert_eq!(Rail::of(20), Rail::Strip, "the narrowest frame there is");
        assert_eq!(Rail::of(200), Rail::Ledger, "the widest");
    }

    #[test]
    fn the_transcript_moves_a_column_at_a_time_and_gives_up_the_rail_only_where_the_shape_changes()
    {
        let mut crossing = None;
        for width in 21..=200u16 {
            let narrow = split(&frame(), band(width - 1, 40)).transcript.width;
            let wide = split(&frame(), band(width, 40)).transcript.width;
            if Rail::of(width - 1) != Rail::of(width) {
                crossing = Some((width, narrow, wide));
                continue;
            }
            assert!(
                wide.abs_diff(narrow) <= 1,
                "at {width} columns the transcript is {wide} wide where {narrow} was a column \
                 ago: a keystroke reflows a whole rail's worth of prose"
            );
        }
        let (at, strip, ledger) = crossing.expect("a width where the strip becomes the ledger");
        assert_eq!(
            at, NARROW,
            "the transcript reflows at the one width the shape changes at"
        );
        assert_eq!(
            Rail::columns(at),
            FLOOR,
            "the rail appears at its narrowest and never at a grown column, so the one reflow a \
             resize to this width costs is the least a rail can cost"
        );
        assert!(
            strip - ledger < Rail::columns(at),
            "at {at} columns the transcript gives up {} of its {strip} columns, which is the \
             rail's whole column of {}: what a keystroke at this width reflows is less than the \
             rail it makes room for",
            strip - ledger,
            Rail::columns(at)
        );
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
                "model",
                "state"
            ],
            "the sacrifice order, the first to go first, and the state the last fact it lets go"
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
            ["ctx", "state", "model", "sandbox", "branch"],
            "the thing you watch, what it is doing, the thing you are, the thing that confines \
             you, and the thing you are standing on"
        );

        let tail = STRIP_FACTS.map(Kind::label);
        let tail = &tail[1..];
        assert_eq!(
            STRIP_SACRIFICE.map(Kind::label),
            [tail[3], tail[2], tail[1], tail[0]],
            "and what the strip gives up is that list without its head, read from the tail, so \
             the run's size is the one fact it never goes without and the state is the last it \
             lets go"
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
            let rail = Rail::of(width);
            let room = match rail {
                Rail::Ledger => Rail::columns(width),
                Rail::Strip => width,
            };
            let shape = Shape::from(rail);
            let facts = Facts {
                model: Some(String::from(name)),
                ..Facts::default()
            };
            let fits_in = match rail {
                Rail::Ledger => value_room(Rail::columns(width)),
                Rail::Strip => VALUE,
            };
            let value = rows(&facts, shape, band(room, 1), None)
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
                Fact::named(Kind::Model, value.clone())
                    .fitted(fits_in)
                    .is_some(),
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
