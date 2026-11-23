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
use crate::rows::{MARGIN, margined, stop_mark};

fn drawn(text: &str) -> usize {
    usize::from(pen::cells(text))
}

pub const NARROW: u16 = FLOOR + TRANSCRIPT;

pub(crate) const FLOOR: u16 = 32;
pub(crate) const CEILING: u16 = 44;

const TRANSCRIPT: u16 = 68;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rail {
    Ledger,
    Strip,
}

impl Rail {
    pub fn of(width: u16) -> Rail {
        if width < NARROW {
            Rail::Strip
        } else {
            Rail::Ledger
        }
    }

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

pub(crate) const DASH: &str = "—";

const SHORT: usize = 8;

pub(crate) const METER: usize = 6;

pub(crate) const FULL_CELL: char = '█';
pub(crate) const EMPTY_CELL: char = '░';

const FULL: u8 = 90;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub size: u64,
    pub window: Option<u64>,
}

impl Ctx {
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

    pub(crate) fn fill(&self) -> Option<Fill> {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fill {
    percent: u8,
    window: u64,
    ink: Ink,
}

impl Fill {
    pub(crate) fn percent(&self) -> u8 {
        self.percent
    }

    /// Whether the run is near the ceiling it is drawn in the accent past: the one turn the meter's
    /// own cells make, which the gauge under it makes too.
    pub(crate) fn turned(&self) -> bool {
        self.percent >= FULL
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hang {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub struct Boot {
    pub wire: Option<Wire>,
    pub model: Option<String>,
    pub workspace: PathBuf,
    pub branch: Option<String>,
    pub enforcement: Enforcement,
}

impl Boot {
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

#[derive(Debug, Default, Clone)]
pub struct Facts {
    pub wire: Option<String>,
    pub model: Option<String>,
    pub think: Option<String>,
    pub turns: Option<(u32, u32)>,
    pub ctx: Option<Ctx>,
    pub cache: Option<u8>,
    pub workspace: Option<PathBuf>,
    pub branch: Option<String>,
    pub sandbox: Option<Enforcement>,
    pub session: Option<String>,
    pub temp: Option<PathBuf>,
    pub route: Option<Route>,
    pub spend: Usage,
    pub commit: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub state: Option<State>,
    pub loaded: Loaded,
}

impl Facts {
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

    pub fn recorded(mut self, document: &SessionDocument) -> Facts {
        self.route = document.route;
        self.spend = document.usage;
        self.commit = document.commit.clone();
        self.start = Some(document.start.clone());
        self.end = document.end.clone();
        self.state = Some(document.stopped.clone());
        self
    }

    pub fn loading(mut self, loaded: Loaded) -> Facts {
        self.loaded = loaded;
        self
    }

    pub fn measured(mut self, usage: &Usage, window: Option<u64>) -> Facts {
        let Some(size) = usage.input else {
            return self;
        };
        self.ctx = Some(Ctx { size, window });
        self.cache = usage.cached_input.and_then(|cached| share(cached, size));
        self
    }
}

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

    pub(crate) fn inked(mut self, ink: Ink) -> Value {
        self.ink = ink;
        self
    }

    pub(crate) fn ink(&self) -> Ink {
        self.ink
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn meter(&self) -> Option<Fill> {
        self.fill
    }

    pub(crate) fn respelled(&self, text: String) -> Value {
        Value {
            text,
            ..self.clone()
        }
    }

    pub(crate) fn align(&mut self, halves: Halves) {
        self.text = padded(&self.text, halves);
    }

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

pub(crate) fn pair(left: impl std::fmt::Display, right: impl std::fmt::Display) -> String {
    format!("{left} / {right}")
}

pub(crate) fn share(part: u64, whole: u64) -> Option<u8> {
    if whole == 0 {
        return None;
    }
    let percent = (u128::from(part) * 100 + u128::from(whole) / 2) / u128::from(whole);
    Some(u8::try_from(percent.min(100)).unwrap_or(100))
}

pub(crate) fn hit_rate(percent: u8) -> String {
    format!("{percent}%")
}

pub(crate) fn tokens(count: u64) -> String {
    if count < 1_000 {
        return count.to_string();
    }
    let tenths = count.saturating_add(50) / 100;
    if tenths < 10_000 {
        return scaled(tenths, 1, "k");
    }
    scaled(count.saturating_add(5_000) / 10_000, 2, "M")
}

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

pub(crate) fn more(count: usize) -> String {
    format!("+{count} more")
}

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

pub(crate) fn padded(spelling: &str, shared: Halves) -> String {
    let Some((one, two)) = spelling.split_once(" / ") else {
        return spelling.to_owned();
    };
    format!("{} / {}", hung(one, shared.left), hung(two, shared.right))
}

fn hung(text: &str, columns: usize) -> String {
    let room = columns.saturating_sub(drawn(text));
    let mut hung = pen::blank(u16::try_from(room).unwrap_or(u16::MAX));
    hung.push_str(text);
    hung
}

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

pub(crate) fn short_id(guid: &str) -> &str {
    match guid.char_indices().nth(SHORT) {
        Some((end, _)) => &guid[..end],
        None => guid,
    }
}

pub(crate) const FIELD: u16 = 10;

const VALUE: usize = (FLOOR - FIELD) as usize;

pub(crate) const GAP: &str = " · ";

const HAIRLINE: char = '─';

const BLOCKS: [&[Kind]; 7] = [
    &[Kind::State],
    &[Kind::Wire, Kind::Model, Kind::Think],
    &[Kind::Turns, Kind::Ctx, Kind::Cache],
    &[Kind::Workspace, Kind::Branch],
    &[Kind::Skills],
    &[Kind::Sandbox],
    &[Kind::Session, Kind::Temp],
];

const FOOT: [Kind; 3] = [Kind::Input, Kind::Output, Kind::Total];

const STRIP_FACTS: [Kind; 5] = [
    Kind::Ctx,
    Kind::State,
    Kind::Model,
    Kind::Sandbox,
    Kind::Branch,
];

const STRIP_SACRIFICE: [Kind; 4] = [Kind::Branch, Kind::Sandbox, Kind::Model, Kind::State];

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailBand {
    pub area: Rect,
    pub picture: Option<Rect>,
    pub facts: Rect,
}

impl RailBand {
    pub fn of(area: Rect) -> RailBand {
        RailBand {
            area,
            picture: None,
            facts: area,
        }
    }

    pub fn with_picture(mut self, cols: u16, rows: u16) -> RailBand {
        let opened = self.area.x.saturating_add(1);
        let across = self.area.width.saturating_sub(1);
        let cols = cols.min(across);
        let rows = rows.min(self.area.height);
        let x = opened.saturating_add(across.saturating_sub(cols) / 2);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share {
    pub bands: Rect,
    pub rail: Option<Rail>,
}

impl Share {
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

pub(crate) fn rows(facts: &Facts, shape: Shape, band: Rect, marker: Option<&Pulse>) -> Vec<Row> {
    let area = Rect::new(0, 0, margined(band).width, band.height);
    match shape {
        Shape::Screen => sheet_rows(facts, area),
        Shape::Ledger => rail_rows(facts, Rail::Ledger, area, marker),
        Shape::Strip => rail_rows(facts, Rail::Strip, area, marker),
    }
}

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
                        Line::Fact(fact) => labelled(row, fact, Hang::Left),
                        Line::Air => Row {
                            y: row.y,
                            kind: None,
                            parts: Vec::new(),
                        },
                        Line::Went(went) => Row::run(row.x, row.y, more(went), Ink::Muted),
                    }
                })
                .collect();
            for (at, fact) in foot.iter().enumerate() {
                let back = u16::try_from(foot.len() - at).unwrap_or(u16::MAX);
                rows.push(labelled(
                    Rect {
                        y: area.bottom().saturating_sub(back),
                        ..area
                    },
                    fact,
                    Hang::Left,
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
                rows.push(labelled(Rect { y, ..at }, fact, fact.hang));
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    y: u16,
    kind: Option<Kind>,
    parts: Vec<(u16, Part)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Run(String, Ink),
    Value(Value),
}

impl Row {
    fn run(x: u16, y: u16, text: impl Into<String>, ink: Ink) -> Row {
        Row {
            y,
            kind: None,
            parts: vec![(x, Part::Run(text.into(), ink))],
        }
    }

    pub(crate) fn y(&self) -> u16 {
        self.y
    }

    pub(crate) fn kind(&self) -> Option<Kind> {
        self.kind
    }

    fn push(&mut self, x: u16, part: Part) -> u16 {
        let next = x.saturating_add(part.columns());
        self.parts.push((x, part));
        next
    }

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

fn labelled(area: Rect, fact: &Fact, hang: Hang) -> Row {
    let mut parts = Vec::new();
    let mut from = area.x;
    if let Some(label) = fact.label {
        parts.push((area.x, Part::Run(pen::pad(label, FIELD), Ink::Dim)));
        from = area.x.saturating_add(FIELD);
    }
    let x = match hang {
        Hang::Right => pen::hanging(area, pen::columns(fact.value.columns()))
            .expect("a fact is fitted to the row it is laid in"),
        Hang::Left => from,
    };
    parts.push((x, Part::Value(fact.value.clone())));
    Row {
        y: area.y,
        kind: Some(fact.kind),
        parts,
    }
}

fn hairline_row(area: Rect) -> Row {
    Row::run(area.x, area.y, pen::filled(area.width, HAIRLINE), Ink::Dim)
}

fn strip(kept: &Kept, area: Rect) -> Row {
    let mut row = Row {
        y: area.y,
        kind: None,
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
    if kept.counted {
        if !row.parts.is_empty() {
            x = row.push(x, Part::Run(GAP.to_owned(), Ink::Dim));
        }
        row.push(x, Part::Run(more(kept.went), Ink::Muted));
    }
    row
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut {
    Word,
    Path,
    Name,
}

impl Cut {
    fn shorten(self, text: &str, room: usize) -> Option<String> {
        let cut = match self {
            Cut::Word => return (drawn(text) <= room).then(|| text.to_owned()),
            Cut::Path => tail(text, room),
            Cut::Name => head(text, room),
        };
        (!cut.is_empty()).then_some(cut)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fact {
    kind: Kind,
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

    fn named(kind: Kind, name: impl Into<String>) -> Fact {
        Fact {
            cut: Cut::Name,
            ..Fact::new(kind, name)
        }
    }

    fn path(kind: Kind, path: &str) -> Fact {
        Fact {
            cut: Cut::Path,
            ..Fact::new(kind, path)
        }
    }

    fn valued(kind: Kind, value: Value) -> Fact {
        Fact {
            value,
            ..Fact::new(kind, "")
        }
    }

    fn bare(kind: Kind, name: impl Into<String>) -> Fact {
        Fact {
            label: None,
            hang: Hang::Left,
            ..Fact::named(kind, name)
        }
    }

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
    counted: bool,
}

enum Line<'a> {
    Fact(&'a Fact),
    Air,
    Went(usize),
}

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
    let with_count = rail == Rail::Ledger;
    let mut went = total - kept.len();
    while !holds(&kept, went, with_count, rail, area) {
        let Some(kind) = order
            .iter()
            .copied()
            .find(|kind| kept.iter().any(|fact| fact.kind == *kind))
        else {
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

fn value_columns(fact: &Fact, width: u16) -> usize {
    match fact.label {
        None => usize::from(width),
        Some(_) => value_room(width),
    }
}

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

const MIN_COLUMN: u16 = 40;

const COLUMN_GAP: u16 = 2;

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

#[derive(Default)]
struct Flow {
    columns: Vec<Column>,
    went: usize,
}

struct Column {
    x: u16,
    width: u16,
    blocks: Vec<Placed>,
}

struct Placed {
    label: &'static str,
    ruled: bool,
    rows: Vec<Fact>,
}

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

pub(crate) fn value_room(width: u16) -> usize {
    usize::from(width).saturating_sub(usize::from(FIELD))
}

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
