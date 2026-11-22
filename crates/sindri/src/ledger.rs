//! The run's facts, the one way each of them is spelled and the rows they are laid out in: what the
//! wire, the record and the launch say, and what the rail and the screen draw of it, cut to the room
//! each row leaves.

use std::path::{Path, PathBuf};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::config::Config;
use sindri_core::policy::Enforcement;
use sindri_core::provider::Route;
use sindri_core::records::{SessionDocument, Usage, branch};
use sindri_core::run::{Loaded, State};
use sindri_core::session::Session;
use sindri_core::wire::Wire;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen;
use crate::pulse::Pulse;
use crate::rows::{MARGIN, stop_mark};

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

impl Boot {
    /// The run a prompt will ask for, as a launch knows it before a session exists.
    pub fn of(config: &Config, enforcement: Enforcement, workspace: &Path) -> Boot {
        Boot {
            wire: config.provider,
            model: config.model.clone(),
            workspace: workspace.to_path_buf(),
            branch: branch(workspace),
            enforcement,
        }
    }
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
    /// The names loaded so far, in load order: the launch's own set, which a rail column gives the
    /// count of and the screen a row for each.
    pub loaded: Loaded,
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
    pub fn recorded(mut self, document: &SessionDocument) -> Facts {
        self.route = document.route;
        self.spend = document.usage;
        self.commit = document.commit.clone();
        self.start = Some(document.start.clone());
        self.end = document.end.clone();
        self.state = Some(document.stopped.clone());
        self
    }

    /// The one set the frame draws and `/new` empties: every write to it is a write the rail sees.
    pub fn loading(mut self, loaded: Loaded) -> Facts {
        self.loaded = loaded;
        self
    }

    /// The size one ask's report measured, and the part of its input the provider served from cache.
    /// The document cannot hand these in: the `usage` it keeps is every ask summed, and a sum of the
    /// inputs grows with each turn while the conversation's size is what the newest request carried.
    pub fn measured(mut self, usage: &Usage, window: Option<u64>) -> Facts {
        let Some(size) = usage.input else {
            return self;
        };
        self.ctx = Some(Ctx { size, window });
        self.cache = usage.cached_input.and_then(|cached| share(cached, size));
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

/// The facts the ledger lays on the rows at its foot, read from the top down: what the run has
/// spent while the reader types.
const FOOT: [Kind; 3] = [Kind::Input, Kind::Output, Kind::Total];

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
const SACRIFICE: [Kind; 15] = [
    Kind::Input,
    Kind::Output,
    Kind::Total,
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

/// What the frame's bands and its rail share of it: the cells the transcript and the notice are laid
/// in, and the rail's shape — nothing at all for a frame that knows no fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share {
    /// The cells the transcript and the notice are laid in, which is the frame less the rail's
    /// columns.
    pub bands: Rect,
    /// The rail's shape, and nothing at all for a frame that knows no fact.
    pub rail: Option<Rail>,
}

impl Share {
    /// The rail's band on the frame `area`, and the rows the bands beside it keep: a ledger's band
    /// is the frame's right-hand columns down to `head`, and none at all where that leaves it no
    /// row; a strip takes the transcript's last row, which is cut off the `transcript` handed in.
    pub fn rail_band(
        self,
        area: Rect,
        head: u16,
        transcript: &mut Rect,
    ) -> Option<(Rail, RailBand)> {
        let rail = self.rail?;
        match rail {
            Rail::Ledger => {
                let columns = Rail::columns(area.width);
                let height = head.saturating_sub(area.y);
                if height == 0 {
                    return None;
                }
                Some((
                    rail,
                    RailBand::of(Rect {
                        x: area.right().saturating_sub(columns),
                        y: area.y,
                        width: columns,
                        height,
                    }),
                ))
            }
            Rail::Strip => {
                let height = transcript.height.checked_sub(1)?;
                let y = transcript.y.saturating_add(height);
                *transcript = Rect {
                    height,
                    ..*transcript
                };
                Some((
                    rail,
                    RailBand::of(Rect {
                        y,
                        height: 1,
                        ..area
                    }),
                ))
            }
        }
    }
}

/// Cuts the frame the bands and the rail share: the rail's own columns down the right while the
/// frame is wide enough to hold a column, a strip along the transcript's bottom row below it. A
/// frame that knows no fact draws no rail, and the transcript loses no column to one.
pub fn split(facts: &Facts, area: Rect) -> Share {
    let shape = Rail::of(area.width);
    if area.width == 0 || area.height == 0 || known(facts, shape.into(), None).is_empty() {
        return Share {
            bands: area,
            rail: None,
        };
    }
    let bands = match shape {
        Rail::Ledger => Rect {
            width: area.width - Rail::columns(area.width),
            ..area
        },
        Rail::Strip => area,
    };
    Share {
        bands,
        rail: Some(shape),
    }
}

/// The rows `shape` lays in a band `band` wide and tall, from that band's own origin: the row each
/// stands on and the column each part starts at, never more of them than the band holds. `draw` is
/// what places them where the band sits, in the columns the frame's margin leaves.
pub(crate) fn rows(facts: &Facts, shape: Shape, band: Rect, marker: Option<&Pulse>) -> Vec<Row> {
    let area = Rect::new(0, 0, band.width.saturating_sub(inset()), band.height);
    match shape {
        Shape::Screen => sheet_rows(facts, area),
        Shape::Ledger => rail_rows(facts, Rail::Ledger, area, marker),
        Shape::Strip => rail_rows(facts, Rail::Strip, area, marker),
    }
}

/// Draws the rows a shape laid at the origin it is handed, and never a row the band did not give
/// it: the one place a row's parts reach a frame, so an adapter is the band it is handed and no rule
/// of its own.
pub(crate) fn draw(rows: &[Row], at: Rect, buffer: &mut Buffer, palette: &Palette) {
    for row in rows.iter().take(usize::from(at.height)) {
        let y = at.y.saturating_add(row.y());
        for (x, part) in row.parts() {
            part.draw(
                buffer,
                at.x.saturating_add(MARGIN).saturating_add(*x),
                y,
                palette,
            );
        }
    }
}

/// The columns a band gives up to the frame's margin: two at its left edge and two at its right.
const fn inset() -> u16 {
    MARGIN.saturating_mul(2)
}

/// The ledger's rows, or the strip's one: the blocks the rail reads, an empty row between two that
/// both kept a fact, and the count of what went in a row of its own.
fn rail_rows(facts: &Facts, rail: Rail, area: Rect, marker: Option<&Pulse>) -> Vec<Row> {
    let kept = kept(facts, rail, area, marker);
    match rail {
        Rail::Ledger => {
            let foot = foot_facts(&kept.facts);
            let laid = u16::try_from(foot.len()).unwrap_or(u16::MAX);
            let mut rows: Vec<Row> = ledger_lines(&kept.facts, kept.went)
                .into_iter()
                .take(usize::from(area.height.saturating_sub(laid)))
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
                .collect();
            // The foot is read up from the floor, so its last fact stands on the row the shadow is
            // drawn across.
            for (at, fact) in foot.iter().enumerate() {
                let back = u16::try_from(foot.len() - at).unwrap_or(u16::MAX);
                rows.push(labelled(
                    Rect {
                        y: area.bottom().saturating_sub(back),
                        ..area
                    },
                    fact,
                ));
            }
            rows
        }
        Rail::Strip => vec![strip(&kept, area)],
    }
}

fn foot_facts(kept: &[Fact]) -> Vec<&Fact> {
    FOOT.iter()
        .filter_map(|kind| kept.iter().find(|fact| fact.kind == *kind))
        .collect()
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

/// One row of a shape, laid out: the row it stands on and the column each of its runs starts at,
/// both counted from the band's own origin, and what each run draws.
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
        let Some(kind) = order
            .iter()
            .copied()
            .find(|kind| kept.iter().any(|fact| fact.kind == *kind))
        else {
            // Nothing the shape may give up is left — the ledger keeps the sandbox posture and the
            // strip the run's size — so a rail narrower than that fact has nothing left to give up.
            break;
        };
        let before = kept.len();
        kept.retain(|fact| fact.kind != kind);
        went = went.saturating_add(before - kept.len());
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
    for (kind, count) in [
        (Kind::Input, facts.spend.input),
        (Kind::Output, facts.spend.output),
        (Kind::Total, facts.spend.total),
        (Kind::Cached, facts.spend.cached_input),
        (Kind::Reasoning, facts.spend.reasoning),
    ] {
        let spelled = match shape {
            Shape::Screen => true,
            Shape::Ledger => FOOT.contains(&kind),
            Shape::Strip => false,
        };
        if let Some(count) = count.filter(|_| spelled) {
            stated.push(Fact::new(kind, tokens(count)));
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
    let names = facts.loaded.names();
    if shape == Shape::Screen {
        for name in &names {
            stated.push(Fact::bare(Kind::Skills, name.clone()));
        }
    } else if !names.is_empty() {
        stated.push(Fact::new(Kind::Skills, names.len().to_string()));
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
        Shape::Ledger => aligned(
            &mut stated,
            BLOCKS.iter().copied().chain(std::iter::once(&FOOT[..])),
        ),
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
        Rail::Ledger => {
            let above = ledger_lines(facts, went).len();
            let foot = foot_facts(facts).len();
            let air = usize::from(above > 0 && foot > 0);
            above + air + foot <= usize::from(area.height)
        }
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

/// The screen's blocks flowed into the columns that hold them, and the rows no column could: a block is
/// never split, so a block's name stands over every row it belongs to.
#[derive(Default)]
struct Flow {
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
fn sheet(known: &[Fact], area: Rect) -> Flow {
    let blocks = blocks(known);
    if area.width == 0 || area.height == 0 || blocks.is_empty() {
        return Flow::default();
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
) -> Flow {
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
    Flow { columns, went }
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
mod tests;
