//! The run's facts down the frame's right-hand columns as a ledger, or on one strip along the
//! band's bottom row below a hundred. Nothing is drawn in part: a fact that leaves no room
//! for its leader is given up whole and counted.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::policy::Enforcement;
use sindri_core::provider::Usage;
use sindri_core::records::SessionDocument;
use sindri_core::session::Session;

use crate::ink::Ink;

pub const COLUMNS: u16 = 32;

/// The frame width the ledger gives way at: a hundred-column frame is the last that holds a
/// column.
pub const NARROW: u16 = 100;

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

/// The columns a label field holds, whatever the label says, so every label and every run of
/// leaders starts in one column.
const FIELD: u16 = 10;

/// The fewest leader dots a ledger row is drawn with: a value that leaves no room for one
/// drops the fact.
const LEADER_MIN: usize = 1;

/// The share of a window at which the rail raises its voice: a run that full is one turn from
/// being no room at all.
const FULL: u8 = 90;

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

const VALUE: usize = (COLUMNS - FIELD) as usize - LEADER_MIN;

/// The columns a session's GUID is drawn by: no rail column holds a whole one.
const SHORT: usize = 8;

const LEADER: char = '.';

/// What lies between two facts on the strip's one row, the gap the footer puts between its
/// hints.
const GAP: &str = " · ";

const HAIRLINE: char = '─';

/// The run's size as the last request reported it, and never as the next one will be: whatever
/// tool results the turn produced are not in it yet, so a run one turn from a full window is
/// that and no more exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    pub size: u64,
    pub window: Option<u64>,
}

impl Ctx {
    /// The value the rail draws, and the share it ends in, from one derivation so a row's text and
    /// its ink cannot disagree. The strip keeps the share and gives up the size when both are known:
    /// its one row is shared with three other facts and the share is the one a reader watches.
    fn spelled(&self, shape: Shape) -> (String, Option<Fill>) {
        let Some(fill) = self.fill() else {
            return (tokens(self.size), None);
        };
        let value = match shape {
            Shape::Ledger => format!(
                "{}/{} {}",
                tokens(self.size),
                tokens(fill.window),
                fill.said()
            ),
            Shape::Strip => fill.said(),
        };
        (value, Some(fill))
    }

    /// The size as the ledger spells it: every drawing of a size outside the rail's own rows spells
    /// it here, so a size reads one way wherever it is said.
    pub(crate) fn said(&self) -> String {
        self.spelled(Shape::Ledger).0
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
                Ink::Muted
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fill {
    percent: u8,
    window: u64,
    /// `muted` while there is room, `accent` once the window is nearly full.
    ink: Ink,
}

impl Fill {
    fn said(&self) -> String {
        format!("{}%", self.percent)
    }
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

/// The facts the rail draws, as the core holds them. A fact the frame knows nothing about is
/// `None` and is not drawn, never guessed at, so a frame before its first run has no rail.
#[derive(Debug, Default, Clone)]
pub struct Facts {
    /// The dialect the run asks over, in the wire's own word: `anthropic`, `openai` or `copilot`,
    /// and never a fourth.
    pub wire: Option<String>,
    /// The served name of the model, whole here and cut to its head wherever the rail draws it.
    pub model: Option<String>,
    /// Whether the model thinks: `high` or `off`, and never a third.
    pub think: Option<String>,
    pub turns: Option<(u32, u32)>,
    pub ctx: Option<Ctx>,
    /// How much of the run's input the provider served from its own cache: a hit rate and not a
    /// fill, so it gets no meter and no threshold.
    pub cache: Option<u8>,
    pub workspace: Option<PathBuf>,
    pub branch: Option<String>,
    pub skills: Option<usize>,
    pub sandbox: Option<Enforcement>,
    pub session: Option<String>,
    pub temp: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share {
    pub transcript: Rect,
    /// Nothing when the frame knows no fact at all, and the transcript keeps the whole band.
    pub rail: Option<(Shape, Rect)>,
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

    /// Cuts the band the transcript and the rail share: 32 columns down the right while the frame is
    /// wide enough to hold a column, one strip along the band's bottom row below a hundred. A frame
    /// that knows no fact draws no rail, and the transcript loses no room to one.
    pub fn split(&self, band: Rect) -> Share {
        let shape = Shape::of(band.width);
        if band.width == 0 || band.height == 0 || self.known(shape).is_empty() {
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

    pub fn paint(&self, shape: Shape, area: Rect, buffer: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        match shape {
            Shape::Ledger => self.ledger(area, buffer),
            Shape::Strip => self.strip(area, buffer),
        }
    }

    fn ledger(&self, area: Rect, buffer: &mut Buffer) {
        let kept = self.kept(Shape::Ledger, area);
        let mut y = area.y;
        for row in ledger_rows(&kept.facts, kept.went) {
            if y >= area.bottom() {
                return;
            }
            match row {
                Row::Fact(fact) => ledger_row(buffer, Rect { y, ..area }, fact),
                Row::Hairline => {
                    let hairline: String =
                        std::iter::repeat_n(HAIRLINE, usize::from(area.width)).collect();
                    put(buffer, area.x, y, &hairline, Ink::Dim);
                }
                Row::Went(went) => {
                    put(buffer, area.x, y, &more(went), Ink::Muted);
                }
            }
            y = y.saturating_add(1);
        }
    }

    fn strip(&self, area: Rect, buffer: &mut Buffer) {
        let kept = self.kept(Shape::Strip, area);
        let mut x = area.x;
        for (index, fact) in kept.facts.iter().enumerate() {
            if index > 0 {
                x = put(buffer, x, area.y, GAP, Ink::Dim);
            }
            x = put(buffer, x, area.y, fact.kind.label(), Ink::Dim);
            x = put(buffer, x, area.y, " ", Ink::Dim);
            x = fact.draw(buffer, x, area.y);
        }
        // A strip too narrow to hold the count beside the facts it keeps is the one rail
        // without one.
        if kept.counted {
            if !kept.facts.is_empty() {
                x = put(buffer, x, area.y, GAP, Ink::Dim);
            }
            put(buffer, x, area.y, &more(kept.went), Ink::Muted);
        }
    }

    /// The facts this rail keeps and how many it gave up. The ledger gives them up in a settled
    /// order beside the row that says how many went; the strip gives them up from the tail of its
    /// own list, but its count is the last item of its one row, so the room takes the count first.
    fn kept(&self, shape: Shape, area: Rect) -> Kept {
        let known = self.known(shape);
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
        // The ledger's count is one of its rows, so the room it takes is part of what the facts
        // are given up for; the strip's is measured once the facts are in, below.
        let with_count = shape == Shape::Ledger;
        let mut went = total - kept.len();
        while !holds(&kept, went, with_count, shape, area) {
            let Some(victim) = order
                .iter()
                .find_map(|kind| kept.iter().position(|fact| fact.kind == *kind))
            else {
                // Nothing the shape may give up is left — the ledger keeps the sandbox posture and
                // the strip the run's size — so a rail narrower than that fact has nothing left to
                // give up.
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

    /// Every fact this frame knows, in the order the shape reads them, each value spelled the way
    /// that shape spells it: a session by the first eight of its GUID, a path by the tail that fits,
    /// a served name by the head.
    fn known(&self, shape: Shape) -> Vec<Fact> {
        let short = self.session.as_deref().map(short_id);
        let mut facts = Vec::new();
        if let Some(wire) = &self.wire {
            facts.push(Fact::new(Kind::Wire, wire.clone()));
        }
        if let Some(model) = &self.model {
            facts.push(Fact::new(Kind::Model, head(model, VALUE)));
        }
        if let Some(think) = &self.think {
            facts.push(Fact::new(Kind::Think, think.clone()));
        }
        if let Some((turns, cap)) = self.turns {
            facts.push(Fact::new(Kind::Turns, format!("{turns}/{cap}")));
        }
        if let Some(ctx) = &self.ctx {
            let (value, fill) = ctx.spelled(shape);
            facts.push(Fact {
                fill,
                ..Fact::new(Kind::Ctx, value)
            });
        }
        if let Some(cache) = self.cache {
            facts.push(Fact::new(Kind::Cache, format!("{cache}%")));
        }
        if let Some(workspace) = &self.workspace {
            let value = tail(&workspace.to_string_lossy(), VALUE);
            if !value.is_empty() {
                facts.push(Fact::new(Kind::Workspace, value));
            }
        }
        if let Some(branch) = &self.branch {
            facts.push(Fact::new(Kind::Branch, branch.clone()));
        }
        if let Some(skills) = self.skills {
            facts.push(Fact::new(Kind::Skills, skills.to_string()));
        }
        if let Some(sandbox) = self.sandbox {
            facts.push(Fact::new(Kind::Sandbox, sandbox.rail_label().to_owned()));
        }
        if let Some(short) = short {
            facts.push(Fact::new(Kind::Session, short.to_owned()));
        }
        if let Some(temp) = &self.temp {
            let path = temp.to_string_lossy();
            // The temp's path carries the session's whole GUID, so it is drawn under the
            // eight-column name the session row above it uses.
            let named = match (short, self.session.as_deref()) {
                (Some(short), Some(guid)) => path.replace(guid, short),
                _ => path.into_owned(),
            };
            let value = tail(&named, VALUE);
            if !value.is_empty() {
                facts.push(Fact::new(Kind::Temp, value));
            }
        }
        if shape == Shape::Strip {
            facts.sort_by_key(|fact| strip_place(fact.kind));
        }
        facts
    }
}

/// One fact as the rail draws it: which fact, which is also its label field and its place in the
/// sacrifice order, and the value beside it.
struct Fact {
    kind: Kind,
    value: String,
    fill: Option<Fill>,
}

impl Fact {
    fn new(kind: Kind, value: impl Into<String>) -> Fact {
        Fact {
            kind,
            value: value.into(),
            fill: None,
        }
    }

    fn draw(&self, buffer: &mut Buffer, x: u16, y: u16) -> u16 {
        let Some(fill) = self.fill else {
            return put(buffer, x, y, &self.value, Ink::Muted);
        };
        let share = fill.said();
        let head: String = self
            .value
            .chars()
            .take(
                self.value
                    .chars()
                    .count()
                    .saturating_sub(share.chars().count()),
            )
            .collect();
        let x = put(buffer, x, y, &head, Ink::Muted);
        put(buffer, x, y, &share, fill.ink)
    }
}

struct Kept {
    facts: Vec<Fact>,
    went: usize,
    /// Whether the strip says how many went: the ledger's count is one of its rows, and the band's
    /// height is what holds it.
    counted: bool,
}

/// One row of the ledger, before it is a row of glyphs. Pure, so the height a set of facts needs
/// is a number a test reads without a buffer.
enum Row<'a> {
    Fact(&'a Fact),
    Hairline,
    Went(usize),
}

/// The ledger's rows in the order they are read, with a hairline between two blocks that both
/// kept a fact and the count of what went at the foot when anything did.
fn ledger_rows(kept: &[Fact], went: usize) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
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
            rows.push(Row::Hairline);
        }
        rows.extend(facts.into_iter().map(Row::Fact));
        written += 1;
    }
    if went > 0 {
        rows.push(Row::Went(went));
    }
    rows
}

/// Whether one fact is drawn whole on a row `width` columns wide: a label in its field, a value
/// against the edge, and at least one leader between them. A value that leaves no room for the
/// leader, or that is cut to nothing, drops the fact rather than being drawn in part.
fn fits(fact: &Fact, shape: Shape, width: u16) -> bool {
    let value = fact.value.chars().count();
    if value == 0 {
        return false;
    }
    match shape {
        Shape::Ledger => usize::from(FIELD) + LEADER_MIN + value <= usize::from(width),
        Shape::Strip => columns(fact.kind.label()) + 1 + value <= usize::from(width),
    }
}

fn strip_place(kind: Kind) -> usize {
    STRIP_FACTS
        .iter()
        .position(|read| *read == kind)
        .unwrap_or(usize::MAX)
}

fn holds(facts: &[Fact], went: usize, counted: bool, shape: Shape, area: Rect) -> bool {
    match shape {
        Shape::Ledger => ledger_rows(facts, went).len() <= usize::from(area.height),
        Shape::Strip => strip_columns(facts, went, counted) <= usize::from(area.width),
    }
}

fn strip_columns(facts: &[Fact], went: usize, counted: bool) -> usize {
    let mut taken = 0;
    for (index, fact) in facts.iter().enumerate() {
        if index > 0 {
            taken += columns(GAP);
        }
        taken += columns(fact.kind.label()) + 1 + columns(&fact.value);
    }
    if counted {
        if !facts.is_empty() {
            taken += columns(GAP);
        }
        taken += columns(&more(went));
    }
    taken
}

fn ledger_row(buffer: &mut Buffer, area: Rect, fact: &Fact) {
    let x = label_row(buffer, area, fact.kind.label(), columns(&fact.value));
    fact.draw(buffer, x, area.y);
}

/// The label of one ledger row in its field, the leaders out to a value `value_width` columns
/// wide, and the column that value starts at: the rule both this rail and the screen that
/// widens it draw a row by, so values end on one hanging edge wherever they are read.
pub(crate) fn label_row(buffer: &mut Buffer, area: Rect, label: &str, value_width: usize) -> u16 {
    let start = usize::from(area.width).saturating_sub(value_width);
    let field = format!("{label:<width$}", width = usize::from(FIELD));
    put(buffer, area.x, area.y, &field, Ink::Dim);
    let leaders: String =
        std::iter::repeat_n(LEADER, start.saturating_sub(usize::from(FIELD))).collect();
    put(buffer, area.x + FIELD, area.y, &leaders, Ink::Dim);
    area.x
        .saturating_add(u16::try_from(start).unwrap_or(area.width))
}

/// The row that says how many facts the room could not hold, in the one wording both shapes and
/// the screen that widens it count them by.
pub(crate) fn more(count: usize) -> String {
    format!("+{count} more")
}

/// The columns a labelled row drawn in `width` columns leaves its value: the field and one
/// leader come off the front.
pub(crate) fn value_room(width: u16) -> usize {
    usize::from(width).saturating_sub(usize::from(FIELD) + LEADER_MIN)
}

/// The columns `text` takes: one per glyph, because a cell is one glyph wide and `·` is three
/// bytes and one column.
fn columns(text: &str) -> usize {
    text.chars().count()
}

pub(crate) fn put(buffer: &mut Buffer, x: u16, y: u16, text: &str, ink: Ink) -> u16 {
    let mut x = x;
    for glyph in text.chars() {
        if let Some(cell) = buffer.cell_mut((x, y)) {
            cell.set_char(glyph);
            cell.set_style(ink.style());
        }
        x = x.saturating_add(1);
    }
    x
}

/// The tail of a path in at most `columns`, with `…/` in front when the head did not fit. Nothing
/// is ever cut mid-component: a tail that cannot hold even the last component is empty, which
/// drops the fact.
pub(crate) fn tail(path: &str, columns: usize) -> String {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return path.to_owned();
    }
    let mut kept: Vec<&str> = Vec::new();
    for (index, part) in parts.iter().enumerate().rev() {
        let width = kept
            .iter()
            .map(|kept| kept.chars().count() + 1)
            .sum::<usize>()
            + part.chars().count()
            + if index > 0 { 2 } else { 0 };
        if width > columns {
            break;
        }
        kept.insert(0, part);
    }
    if kept.is_empty() {
        return String::new();
    }
    if kept.len() == parts.len() {
        return path.to_owned();
    }
    format!("…/{}", kept.join("/"))
}

/// The head of a served name in at most `columns`: whole hyphen-separated segments, and never one
/// drawn in part. A name keeps its head, where the family lives, and gives up the stamps that
/// follow it. A name whose first segment does not fit beside its own mark is empty.
pub(crate) fn head(name: &str, columns: usize) -> String {
    if name.chars().count() <= columns {
        return name.to_owned();
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut width = 0;
    for (index, part) in name.split('-').enumerate() {
        let next = width + part.chars().count() + usize::from(index > 0);
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
fn short_id(guid: &str) -> &str {
    match guid.char_indices().nth(SHORT) {
        Some((end, _)) => &guid[..end],
        None => guid,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
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
    /// The word the label field holds, so the rail and the screen that widens it cannot call one fact
    /// two things.
    fn label(self) -> &'static str {
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

/// The four facts the strip carries, in the order it reads them: its one row asks a different
/// question than the ledger's blocks do, so its list is its own.
const STRIP_FACTS: [Kind; 4] = [Kind::Ctx, Kind::Model, Kind::Sandbox, Kind::Branch];

/// The order the strip gives its facts up in: read from the tail, so the run's size — the head,
/// the one a reader watches — is what it never gives up.
const STRIP_SACRIFICE: [Kind; 3] = [Kind::Branch, Kind::Sandbox, Kind::Model];

/// The rail's sacrifice order, the first to go first. The sandbox posture is deliberately absent,
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

#[cfg(test)]
mod tests {
    //! The rail's own rules, one test each: the order it gives facts up in, the label field and its
    //! leaders, the blocks and their hairlines, the strip, and the row that counts what went. Where
    //! a cell's glyph and ink are the question, the buffer the ledger paints into is the answer.

    use std::path::Path;

    use sindri_core::policy::PolicyRecord;
    use sindri_core::provider::Usage as Report;
    use sindri_core::records::{Dials, SessionDocument, Usage};
    use sindri_core::run::State as RunState;
    use sindri_core::session::Session;
    use sindri_core::wire::Wire;

    use super::*;

    fn band(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

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

    fn inks(buffer: &Buffer, y: u16) -> Vec<(Ink, u16)> {
        let mut runs: Vec<(Ink, u16)> = Vec::new();
        for x in buffer.area.x..buffer.area.right() {
            let cell = buffer.cell((x, y)).expect("a cell of the band");
            let ink = Ink::of(cell).expect("the rail paints in inks and nothing else");
            match runs.last_mut() {
                Some((last, count)) if *last == ink => *count += 1,
                _ => runs.push((ink, 1)),
            }
        }
        runs
    }

    /// The dated Anthropic name a real run is served, which is wider than a value column.
    const SERVED: &str = "claude-sonnet-4-5-20250929";

    /// What the rail draws of [`SERVED`]: its head, and the mark saying the date stamp went.
    const CUT: &str = "claude-sonnet-4-5…";

    const BRANCH: &str = "spec/1-capability";

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

    /// The contract as a test states it rather than as the code holds it: every fact the rail draws,
    /// so a fact's wholeness and the count of what went are read off the screen, not off the module's
    /// own tables.
    const CONTRACT: [(&str, &str); 12] = [
        ("wire", "anthropic"),
        ("model", CUT),
        ("think", "high"),
        ("turns", "3/250"),
        ("ctx", "48.2k"),
        ("cache", "88%"),
        ("workspace", "…/.worktrees/46-rail"),
        ("branch", "spec/1-capability"),
        ("skills", "2"),
        ("sandbox", "bwrap"),
        ("session", "9f2c7a41"),
        ("temp", "…/9f2c7a41/tmp"),
    ];

    /// A document filled in by hand, so the rail's wiring is read off the document's fields and not
    /// off a real run.
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

    #[test]
    fn the_ledger_takes_the_right_hand_columns_and_the_strip_the_bands_bottom_row() {
        let wide = frame().split(band(120, 37));
        assert_eq!(
            wide.transcript,
            band(88, 37),
            "the transcript keeps the frame's width less §12's thirty-two columns"
        );
        assert_eq!(
            wide.rail,
            Some((
                Shape::Ledger,
                Rect {
                    x: 88,
                    ..band(COLUMNS, 37)
                }
            )),
            "and the rail draws down the frame's right-hand edge, band high"
        );

        let narrow = frame().split(band(99, 21));
        assert_eq!(
            narrow.transcript,
            band(99, 20),
            "below a hundred columns the strip takes one row of the band and no columns"
        );
        assert_eq!(
            narrow.rail,
            Some((
                Shape::Strip,
                Rect {
                    y: 20,
                    height: 1,
                    ..band(99, 1)
                }
            )),
            "on the band's bottom row, above the composer"
        );
    }

    #[test]
    fn a_frame_that_knows_no_fact_leaves_the_whole_band_to_the_transcript() {
        let share = Facts::default().split(band(120, 37));

        assert_eq!(
            share.transcript,
            band(120, 37),
            "a rail with nothing in it is not drawn, and nothing narrows for it"
        );
        assert_eq!(share.rail, None, "there is no rail row to see");
    }

    #[test]
    fn the_facts_a_session_holds_are_the_ones_the_rail_draws_it_by() {
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

    #[test]
    fn the_size_and_the_cache_a_turns_report_carries_are_the_facts_the_rail_draws() {
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

    fn ctx_line(value: &str) -> String {
        let leaders = usize::from(COLUMNS) - usize::from(FIELD) - value.chars().count();
        format!("ctx       {}{value}", ".".repeat(leaders))
    }

    #[test]
    fn a_runs_size_is_spelled_in_tokens_and_never_counted_out() {
        for (size, value) in [
            (0, "0"),
            (948, "948"),
            (999, "999"),
            (1_000, "1k"),
            (48_200, "48.2k"),
            (200_000, "200k"),
            (999_949, "999.9k"),
            (999_950, "1M"),
            (1_020_000, "1.02M"),
            (1_100_000, "1.1M"),
        ] {
            let facts = Facts {
                ctx: Some(Ctx { size, window: None }),
                ..Facts::default()
            };
            let mut buffer = Buffer::empty(band(COLUMNS, 1));

            facts.paint(Shape::Ledger, band(COLUMNS, 1), &mut buffer);

            assert_eq!(
                glyphs(&buffer, 0).trim_end(),
                ctx_line(value),
                "{size} tokens is drawn as {value} and never as the count itself"
            );
        }
    }

    #[test]
    fn a_run_with_a_window_draws_the_window_and_the_share_of_it_it_fills() {
        for (size, window, value) in [
            (0, 200_000, "0/200k 0%"),
            (48_200, 0, "48.2k"),
            (48_200, 200_000, "48.2k/200k 24%"),
            (100_000, 1_020_000, "100k/1.02M 10%"),
            (200_000, 200_000, "200k/200k 100%"),
            (250_000, 200_000, "250k/200k 100%"),
        ] {
            let facts = Facts {
                ctx: Some(Ctx {
                    size,
                    window: Some(window),
                }),
                ..Facts::default()
            };
            let mut buffer = Buffer::empty(band(COLUMNS, 1));

            facts.paint(Shape::Ledger, band(COLUMNS, 1), &mut buffer);

            assert_eq!(
                glyphs(&buffer, 0).trim_end(),
                ctx_line(value),
                "{size} tokens against a window of {window} is drawn as {value}: a run past \
                 its window has no larger share to claim, and a window of no tokens is no \
                 window at all"
            );
        }
    }

    #[test]
    fn a_nearly_full_window_is_the_one_share_the_rail_raises_its_voice_about() {
        for (size, value, share, ink) in [
            (178_000, "178k/200k 89%", "89%", Ink::Muted),
            (180_000, "180k/200k 90%", "90%", Ink::Accent),
            (200_000, "200k/200k 100%", "100%", Ink::Accent),
            (240_000, "240k/200k 100%", "100%", Ink::Accent),
        ] {
            let facts = Facts {
                ctx: Some(Ctx {
                    size,
                    window: Some(200_000),
                }),
                ..Facts::default()
            };
            let mut ledger = Buffer::empty(band(COLUMNS, 1));

            facts.paint(Shape::Ledger, band(COLUMNS, 1), &mut ledger);

            let said = value.chars().count() as u16;
            let tail = share.chars().count() as u16;
            let expected: Vec<(Ink, u16)> = if ink == Ink::Muted {
                vec![(Ink::Muted, said)]
            } else {
                vec![(Ink::Muted, said - tail), (ink, tail)]
            };
            assert_eq!(
                &inks(&ledger, 0)[1..],
                expected,
                "{value} is drawn muted but for its share, which is {} at ninety percent \
                 and above",
                ink.name()
            );

            let mut strip = Buffer::empty(band(40, 1));

            facts.paint(Shape::Strip, band(40, 1), &mut strip);

            assert_eq!(
                Ink::of(strip.cell((4, 0)).expect("a cell of the share")),
                Some(ink),
                "the strip draws that share and nothing else, and reads it in the same ink \
                 the ledger does: one threshold, in one place"
            );
        }
    }

    #[test]
    fn a_frame_handed_no_size_draws_no_ctx_row_and_counts_nothing_for_it() {
        let mut unknown = frame();
        unknown.ctx = None;
        let mut roomy = Buffer::empty(band(COLUMNS, 17));

        unknown.paint(Shape::Ledger, band(COLUMNS, 17), &mut roomy);

        let rows: Vec<String> = (0..17).map(|y| glyphs(&roomy, y)).collect();
        assert_eq!(
            rows[..7],
            [
                "wire      .............anthropic",
                "model     ....claude-sonnet-4-5…",
                "think     ..................high",
                "────────────────────────────────",
                "turns     .................3/250",
                "cache     ...................88%",
                "────────────────────────────────",
            ],
            "a frame before the first reply has no size, so it draws no ctx row and the \
             block closes up under the turns"
        );
        assert!(
            !rows.iter().any(|row| row.contains("more")),
            "and the rail gave nothing up, so it counts nothing: no row may say so"
        );

        let band = band(COLUMNS, 8);
        let mut tight = Buffer::empty(band);

        frame().paint(Shape::Ledger, band, &mut tight);
        let with = glyphs(&tight, 7);

        unknown.paint(Shape::Ledger, band, &mut tight);
        let without = glyphs(&tight, 7);

        assert_eq!(
            (with.trim_end(), without.trim_end()),
            ("+7 more", "+6 more"),
            "the count is of the facts the frame knew: the size joining the frame is one \
             more fact given up at this height and no fact the rail draws, so its absence \
             leaves the count one shorter and never one longer"
        );
    }

    #[test]
    fn the_ledger_draws_whether_the_model_thinks_below_the_model_it_belongs_to() {
        let facts = Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from("claude-sonnet")),
            think: Some(String::from("high")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 3));

        facts.paint(Shape::Ledger, band(COLUMNS, 3), &mut buffer);

        let rows: Vec<String> = (0..3).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire      .............anthropic",
                "model     .........claude-sonnet",
                "think     ..................high",
            ],
            "the model block holds the fact, one row below the model it is about"
        );
    }

    #[test]
    fn the_ledger_reads_its_blocks_in_the_contracts_order_with_a_hairline_between_two() {
        let mut buffer = Buffer::empty(band(COLUMNS, 17));

        frame().paint(Shape::Ledger, band(COLUMNS, 17), &mut buffer);

        let rows: Vec<String> = (0..17).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire      .............anthropic",
                "model     ....claude-sonnet-4-5…",
                "think     ..................high",
                "────────────────────────────────",
                "turns     .................3/250",
                "ctx       .................48.2k",
                "cache     ...................88%",
                "────────────────────────────────",
                "workspace ..…/.worktrees/46-rail",
                "branch    .....spec/1-capability",
                "────────────────────────────────",
                "skills    .....................2",
                "────────────────────────────────",
                "sandbox   .................bwrap",
                "────────────────────────────────",
                "session   ..............9f2c7a41",
                "temp      ........…/9f2c7a41/tmp",
            ],
            "six blocks, a hairline between two of them, nothing given up at this width"
        );
        assert_eq!(
            inks(&buffer, 3),
            [(Ink::Dim, 32)],
            "the hairline runs the rail's width"
        );
        assert_eq!(
            inks(&buffer, 4),
            [(Ink::Dim, 27), (Ink::Muted, 5)],
            "label, leaders, then the value"
        );
    }

    #[test]
    fn a_frame_handed_no_wire_draws_no_wire_row() {
        let facts = Facts {
            wire: None,
            model: Some(String::from("claude-sonnet")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 2));

        facts.paint(Shape::Ledger, band(COLUMNS, 2), &mut buffer);

        assert_eq!(
            (glyphs(&buffer, 0).trim_end(), glyphs(&buffer, 1).trim_end()),
            ("model     .........claude-sonnet", ""),
            "a rail handed no wire has no wire row, and nothing went, so no row counts it"
        );
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
    fn a_ledger_row_draws_the_wire_by_its_word_and_nothing_beside_it() {
        for (word, row) in [
            ("anthropic", "wire      .............anthropic"),
            ("openai", "wire      ................openai"),
            ("copilot", "wire      ...............copilot"),
        ] {
            let facts = Facts {
                wire: Some(String::from(word)),
                ..Facts::default()
            };
            let mut buffer = Buffer::empty(band(COLUMNS, 1));

            facts.paint(Shape::Ledger, band(COLUMNS, 1), &mut buffer);

            assert_eq!(
                glyphs(&buffer, 0),
                row,
                "the label in its field, leaders out to the word, and nothing else"
            );
        }
    }

    #[test]
    fn a_fact_with_no_cut_rule_of_its_own_is_dropped_when_no_leader_fits() {
        let facts = Facts {
            branch: Some(String::from("spec/a-branch-name-that-cannot-fit")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 2));

        facts.paint(Shape::Ledger, band(COLUMNS, 2), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0),
            "+1 more                         ",
            "a branch wider than the column leaves no room for a leader in a thirty-two \
             column rail, so the fact goes and the row says so"
        );
        assert!(
            !glyphs(&buffer, 0).contains("spec/a"),
            "and never the start of it in the fact's place"
        );
    }

    #[test]
    fn a_served_name_longer_than_the_column_is_drawn_by_its_head_and_its_mark() {
        let facts = Facts {
            model: Some(String::from("claude-sonnet-4-5-20250929")),
            ..Facts::default()
        };
        let mut ledger = Buffer::empty(band(COLUMNS, 1));

        facts.paint(Shape::Ledger, band(COLUMNS, 1), &mut ledger);

        assert_eq!(
            glyphs(&ledger, 0),
            "model     ....claude-sonnet-4-5…",
            "a dated name takes the value column whole, and the mark says the date stamp went"
        );

        let mut strip = Buffer::empty(band(40, 1));

        facts.paint(Shape::Strip, band(40, 1), &mut strip);

        assert_eq!(
            glyphs(&strip, 0).trim_end(),
            "model claude-sonnet-4-5…",
            "and the strip's copy of the same name is cut the same way"
        );
    }

    #[test]
    fn a_served_name_with_no_segment_to_keep_is_dropped_and_counted() {
        let facts = Facts {
            model: Some(String::from("aservednamethatnobreaksanywhere")),
            ..Facts::default()
        };
        let mut ledger = Buffer::empty(band(COLUMNS, 2));

        facts.paint(Shape::Ledger, band(COLUMNS, 2), &mut ledger);

        assert_eq!(
            glyphs(&ledger, 0),
            "+1 more                         ",
            "a name cut to nothing says nothing, so the fact goes and the row counts it"
        );
        assert!(
            !glyphs(&ledger, 0).contains("aserved"),
            "and never the start of it in the fact's place"
        );

        let mut strip = Buffer::empty(band(40, 1));

        facts.paint(Shape::Strip, band(40, 1), &mut strip);

        assert_eq!(
            glyphs(&strip, 0).trim_end(),
            "+1 more",
            "and the strip gives up the same fact, and says so in the same words"
        );
    }

    #[test]
    fn a_band_too_short_for_every_block_gives_facts_up_in_a12s_order() {
        let mut buffer = Buffer::empty(band(COLUMNS, 8));

        frame().paint(Shape::Ledger, band(COLUMNS, 8), &mut buffer);

        let rows: Vec<String> = (0..8).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire      .............anthropic",
                "model     ....claude-sonnet-4-5…",
                "think     ..................high",
                "────────────────────────────────",
                "turns     .................3/250",
                "────────────────────────────────",
                "sandbox   .................bwrap",
                "+7 more                         ",
            ],
            "temp, cache, session, branch, skills, ctx and workspace went, in that \
             order, and the foot of the rail says seven"
        );
    }

    #[test]
    fn a_band_that_can_only_hold_the_model_block_draws_the_model_without_its_think_row() {
        let mut buffer = Buffer::empty(band(COLUMNS, 4));

        frame().paint(Shape::Ledger, band(COLUMNS, 4), &mut buffer);

        let rows: Vec<String> = (0..4).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "model     ....claude-sonnet-4-5…",
                "────────────────────────────────",
                "sandbox   .................bwrap",
                "+10 more                        ",
            ],
            "thinking is given up before the model it sits under, so the model keeps its row"
        );
    }

    #[test]
    fn a_strip_carries_the_share_of_a_named_window_and_gives_up_the_size_beside_it() {
        let facts = Facts {
            ctx: Some(Ctx {
                size: 48_200,
                window: Some(200_000),
            }),
            ..Facts::default()
        };
        let mut ledger = Buffer::empty(band(COLUMNS, 1));

        facts.paint(Shape::Ledger, band(COLUMNS, 1), &mut ledger);

        assert_eq!(
            glyphs(&ledger, 0),
            "ctx       ........48.2k/200k 24%",
            "the ledger's ctx row says the size and the share of the window it is"
        );

        let mut strip = Buffer::empty(band(40, 1));

        facts.paint(Shape::Strip, band(40, 1), &mut strip);

        assert_eq!(
            glyphs(&strip, 0).trim_end(),
            "ctx 24%",
            "the strip's one row has no room for the size the share is a share of, so it \
             keeps the number that says how much of the window is left"
        );
    }

    #[test]
    fn forty_columns_hold_the_fact_a_reader_watches_and_the_model_they_are() {
        let mut buffer = Buffer::empty(band(40, 1));

        frame().paint(Shape::Strip, band(40, 1), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0),
            "ctx 48.2k · model claude-sonnet-4-5…    ",
            "forty columns hold the run's size and the model, because the four facts of the \
             strip's list are all it carries and the count does not fit beside the two"
        );
    }

    #[test]
    fn ninety_nine_columns_hold_the_four_facts_and_count_the_ones_they_left_out() {
        let mut facts = frame();
        facts.ctx = Some(Ctx {
            size: 48_200,
            window: Some(200_000),
        });
        let mut buffer = Buffer::empty(band(99, 1));

        facts.paint(Shape::Strip, band(99, 1), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            format!("ctx 24% · model {CUT} · sandbox bwrap · branch {BRANCH} · +8 more"),
            "ninety-nine columns hold the whole list and the count of the eight facts the \
             strip does not carry at all"
        );
    }

    #[test]
    fn the_strip_gives_its_facts_up_from_the_tail_of_its_own_list() {
        let mut counted = Buffer::empty(band(47, 1));

        frame().paint(Shape::Strip, band(47, 1), &mut counted);

        assert_eq!(
            glyphs(&counted, 0).trim_end(),
            "ctx 48.2k · model claude-sonnet-4-5… · +10 more",
            "the branch and the sandbox posture went, and the count that fits says so"
        );

        let mut facts_win = Buffer::empty(band(60, 1));

        frame().paint(Shape::Strip, band(60, 1), &mut facts_win);

        assert_eq!(
            glyphs(&facts_win, 0).trim_end(),
            "ctx 48.2k · model claude-sonnet-4-5… · sandbox bwrap",
            "three facts beat the count: the room the count would take is the room the \
             sandbox posture is drawn in"
        );
    }

    #[test]
    fn a_strip_with_one_fact_draws_it_and_a_frame_with_none_draws_no_strip() {
        let one = Facts {
            sandbox: Some(Enforcement::Bubblewrap),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(40, 1));

        one.paint(Shape::Strip, band(40, 1), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            "sandbox bwrap",
            "the one fact the frame knows is drawn, and no count stands where nothing went"
        );

        let none = Facts::default().split(band(40, 24));

        assert_eq!(none.rail, None, "a frame that knows no fact draws no strip");
        assert_eq!(
            none.transcript,
            band(40, 24),
            "and the transcript keeps the whole band"
        );

        let elsewhere = Facts {
            wire: Some(String::from("anthropic")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(40, 1));

        elsewhere.paint(Shape::Strip, band(40, 1), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            "+1 more",
            "a fact the strip does not carry is counted rather than drawn, because the \
             strip's four are the whole of what its row reads"
        );
    }

    #[test]
    fn the_strip_carries_its_own_list_in_the_order_a_reader_watches_it() {
        let order: Vec<&str> = STRIP_FACTS.iter().map(|kind| kind.label()).collect();
        let gives_up: Vec<&str> = STRIP_SACRIFICE.iter().map(|kind| kind.label()).collect();
        let tail: Vec<Kind> = STRIP_FACTS.iter().skip(1).rev().copied().collect();

        assert_eq!(
            order,
            ["ctx", "model", "sandbox", "branch"],
            "the thing you watch, the thing you are, the thing that confines you, and the \
             thing you are standing on"
        );
        assert_eq!(
            STRIP_SACRIFICE.to_vec(),
            tail,
            "and what the strip gives up is that list without its head, read from the \
             tail, so the head is the one fact it never goes without"
        );
        assert!(
            !gives_up.contains(&"ctx"),
            "the run's size is the strip's head and never appears in the order it gives \
             its facts up in"
        );
        assert!(
            !STRIP_FACTS.contains(&Kind::Turns),
            "turns stays out of the list: the strip has no room to say both how full the \
             conversation is and how many turns it took to fill"
        );
    }

    #[test]
    fn no_fact_is_ever_drawn_partially_at_any_width_between_twenty_and_two_hundred() {
        for width in 20..=200u16 {
            let shape = Shape::of(width);
            let area = match shape {
                Shape::Ledger => band(COLUMNS, 40),
                Shape::Strip => band(width, 1),
            };
            let mut buffer = Buffer::empty(area);
            frame().paint(shape, area, &mut buffer);
            let rows: Vec<String> = (0..area.height).map(|y| glyphs(&buffer, y)).collect();

            let mut went = 0;
            for (label, value) in CONTRACT {
                match rows.iter().find(|row| row.contains(label)) {
                    Some(row) => assert!(
                        row.contains(value),
                        "at {width} columns the {label} row reads {row:?}, which is not \
                         the whole {value:?}"
                    ),
                    None => went += 1,
                }
            }

            let more = format!("+{went} more");
            let screen = rows.join("\n");
            if went == 0 {
                assert!(
                    !screen.contains("more"),
                    "at {width} columns nothing went, so no row may count what did"
                );
                continue;
            }
            if screen.contains(&more) {
                continue;
            }
            assert_eq!(
                shape,
                Shape::Strip,
                "at {width} columns the ledger dropped the {more} row it had room to draw"
            );
            let drawn = columns(screen.trim_end());
            assert!(
                drawn + columns(GAP) + columns(&more) > usize::from(width),
                "at {width} columns the strip dropped the {more} count it had room to draw \
                 beside the {drawn} columns it kept"
            );
        }
    }

    /// The value a row draws beside `label`: the field and its leaders skipped and the padding after
    /// the value dropped, so a test reads the fact the way a reader does.
    fn drawn(row: &str, label: &str) -> Option<String> {
        let rest = row.split_once(label)?.1;
        Some(
            rest.trim_start_matches([' ', '.'])
                .chars()
                .take_while(|glyph| !glyph.is_whitespace() && *glyph != '·')
                .collect(),
        )
    }

    #[test]
    fn no_served_name_is_drawn_in_part_at_any_width_between_twenty_and_two_hundred() {
        assert_no_width_draws_a_name_in_part("claude-sonnet-4-5");
        assert_no_width_draws_a_name_in_part(SERVED);
        assert_no_width_draws_a_name_in_part("anthropic.claude-3-5-sonnet-20241022-v2:0");
    }

    /// One served name at every width: whole when it fits, its head and the mark when it does not,
    /// and never a part without a mark or wider than the row it is drawn in.
    fn assert_no_width_draws_a_name_in_part(name: &str) {
        let mut cut = 0;
        for width in 20..=200u16 {
            let shape = Shape::of(width);
            let area = match shape {
                Shape::Ledger => band(COLUMNS, 1),
                Shape::Strip => band(width, 1),
            };
            let facts = Facts {
                model: Some(String::from(name)),
                ..Facts::default()
            };
            let mut buffer = Buffer::empty(area);

            facts.paint(shape, area, &mut buffer);
            let row = glyphs(&buffer, 0);
            let Some(value) = drawn(&row, "model") else {
                continue;
            };

            assert!(
                value == name || value.ends_with('…'),
                "at {width} columns {name:?} is drawn as {value:?}, which is neither the \
                 whole name nor a name with the mark that says the rest went"
            );
            assert!(
                name.starts_with(value.trim_end_matches('…')),
                "at {width} columns {name:?} is drawn as {value:?}: the head survives and \
                 the tail is what goes"
            );
            assert!(
                fits(&Fact::new(Kind::Model, value.clone()), shape, area.width),
                "at {width} columns {name:?} is drawn as {value:?}, which the {shape:?} row \
                 has no room for"
            );
            if value.ends_with('…') {
                cut += 1;
            }
        }
        if name.chars().count() <= VALUE {
            assert_eq!(
                cut, 0,
                "{name:?} fits a value column, so no width in the sweep may cut it"
            );
        } else {
            assert!(
                cut > 0,
                "{name:?} is wider than the value column, so some width in the sweep draws \
                 it cut: a sweep that never did would prove nothing"
            );
        }
    }

    #[test]
    fn a_strip_row_is_the_four_facts_of_its_list_beside_each_other_with_one_gap() {
        // 220 columns hold the four facts and the count beside them, so the order is read
        // without the room deciding any of it.
        let mut buffer = Buffer::empty(band(220, 1));

        frame().paint(Shape::Strip, band(220, 1), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            format!("ctx 48.2k · model {CUT} · sandbox bwrap · branch {BRANCH} · +8 more"),
            "one row, the strip's own four facts in its own order, a gap between two of \
             them, and the count of the eight it does not carry even with the room to"
        );

        let mut narrower = Buffer::empty(band(88, 1));

        frame().paint(Shape::Strip, band(88, 1), &mut narrower);

        assert_eq!(
            glyphs(&narrower, 0).trim_end(),
            format!("ctx 48.2k · model {CUT} · sandbox bwrap · branch {BRANCH}"),
            "eighty-eight columns hold the four facts and no count: the count's own \
             eleven columns are the ones this row does not have"
        );
    }

    #[test]
    fn a_fact_is_drawn_in_the_words_the_rail_reads_it_in() {
        let facts: Vec<(&str, String)> = frame()
            .known(Shape::Ledger)
            .into_iter()
            .map(|fact| (fact.kind.label(), fact.value))
            .collect();

        assert_eq!(
            facts,
            [
                ("wire", String::from("anthropic")),
                ("model", String::from(CUT)),
                ("think", String::from("high")),
                ("turns", String::from("3/250")),
                ("ctx", String::from("48.2k")),
                ("cache", String::from("88%")),
                ("workspace", String::from("…/.worktrees/46-rail")),
                ("branch", String::from("spec/1-capability")),
                ("skills", String::from("2")),
                ("sandbox", String::from("bwrap")),
                ("session", String::from("9f2c7a41")),
                ("temp", String::from("…/9f2c7a41/tmp")),
            ]
            .map(|(label, value)| (label, value)),
            "the contract's blocks, each value in the words §12 reads it in"
        );
    }

    #[test]
    fn a_path_keeps_the_components_that_fit_and_marks_the_elision_in_front() {
        assert_eq!(
            tail("/glemsom/src/sindri", VALUE),
            "/glemsom/src/sindri",
            "a path that fits is drawn whole, with no mark to explain"
        );
        assert_eq!(
            tail("/very/long/path/that/cannot/fit/whole", VALUE),
            "…/cannot/fit/whole",
            "a path that does not fit keeps its tail and says the head went"
        );
        assert_eq!(
            tail("/the-name-that-will-not-fit-anywhere", VALUE),
            "",
            "no component fits the column, so the fact has no value to draw"
        );
    }

    #[test]
    fn a_served_name_keeps_the_segments_that_fit_and_marks_the_cut_at_its_tail() {
        assert_eq!(
            head("claude-sonnet-4-5-20250929", VALUE),
            "claude-sonnet-4-5…",
            "a dated name keeps the family a reader can name, and marks the date stamp it \
             left behind"
        );
        assert_eq!(
            head("claude-sonnet-4-5", VALUE),
            "claude-sonnet-4-5",
            "a name that fits is drawn whole, with no mark to explain"
        );
        assert_eq!(
            head("anthropic.claude-3-5-sonnet-20241022-v2:0", VALUE),
            "anthropic.claude-3-5…",
            "a gateway name is cut at a segment boundary, and never mid-segment"
        );
        assert_eq!(
            head("claude-sonnet-4-5-20250929", "claude".len() + 1),
            "claude…",
            "the mark takes a column of its own, so the head stops one short of the room"
        );
        assert_eq!(
            head("aservednamethatnobreaksanywhere", VALUE),
            "",
            "a name whose first segment does not fit beside the mark has no head to keep"
        );
    }

    #[test]
    fn a_ledger_row_sets_its_label_in_a_ten_column_field_and_its_value_on_one_hanging_edge() {
        let facts = Facts {
            model: Some(String::from("claude-sonnet")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 1));

        facts.paint(Shape::Ledger, band(COLUMNS, 1), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0),
            "model     .........claude-sonnet",
            "the label in its field, the leaders out of it, the value against the edge"
        );
        assert_eq!(
            inks(&buffer, 0),
            [(Ink::Dim, 19), (Ink::Muted, 13)],
            "the field and its leaders are consulted in `dim`; the value is the fact"
        );
    }

    #[test]
    fn a_hundred_columns_is_the_last_frame_the_rail_is_a_column_in() {
        assert_eq!(
            Shape::of(100),
            Shape::Ledger,
            "§12 makes the rail one strip *below* a hundred columns, so a hundred still \
             holds the ledger"
        );
        assert_eq!(Shape::of(99), Shape::Strip, "and ninety-nine is below it");
        assert_eq!(Shape::of(20), Shape::Strip, "the narrowest frame there is");
        assert_eq!(Shape::of(200), Shape::Ledger, "the widest");
    }

    #[test]
    fn the_rail_gives_facts_up_in_the_order_a12_settled_and_the_sandbox_never_goes() {
        let order: Vec<&str> = SACRIFICE.iter().map(|kind| kind.label()).collect();

        assert_eq!(
            order,
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
            "§12's sacrifice order, the first to go first"
        );
        assert!(
            !SACRIFICE.contains(&Kind::Sandbox),
            "§12 keeps the sandbox posture at every width, so it is not in the order"
        );
    }
}
