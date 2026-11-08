//! The rail: the run's facts down the frame's right-hand columns, read as a ledger —
//! a label in a fixed field, dot leaders running out to one hanging edge, a hairline
//! between blocks — and given up in one order when the frame is too narrow to hold
//! all of it (§12, ticket #46).
//!
//! The facts are handed in and never reached for: `Facts` is what the core knows about a
//! run — the dialect it asks over and the name it was served, the turns against the cap,
//! the enforcement in force, the workspace, the branch, the session's GUID and its temp —
//! and what nothing produces yet is a caller's to fill, so a rail draws a fact only when
//! somebody handed it one.
//!
//! Two shapes and one rule between them. At a hundred columns and above the rail is
//! §12's 32 columns down the frame's right-hand edge, one labelled fact per row; below a
//! hundred it is one strip along the bottom of the band, the same facts in the same
//! order. [`Facts::split`] is that rule — the band the transcript and the rail share, cut
//! where §12 says — so the frame's paint asks for its share rather than counting columns
//! of its own, and the transcript is laid out at the width the rail leaves it (ADR-0008).
//!
//! Nothing is ever drawn in part. A value that leaves no room for a leader in its column
//! drops the fact rather than being cut off, a path keeps whole components and marks the
//! head it left with `…/`, and what the room cannot hold is given up in §12's order —
//! temp path, cache hits, session GUID, branch, skills in force, context meter, workspace,
//! turns, wire, model — beside a row saying how many went. The sandbox posture is absent
//! from that order because §12 says it never goes. The degradation is a pure function of
//! the room, not a paint: what the rail keeps and the rows it makes are read by a test
//! without a buffer, which is how a rule about every frame width from 20 to 200 is proved
//! rather than eyeballed.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::policy::Enforcement;
use sindri_core::records::SessionDocument;
use sindri_core::session::Session;

use crate::ink::Ink;

/// §12's 32 columns: what the rail takes from the frame's right-hand edge while the
/// frame is wide enough to hold a column at all.
pub const COLUMNS: u16 = 32;

/// The frame width the ledger gives way at: §12 makes the rail one strip *below* a
/// hundred columns, so a hundred-column frame is the last that holds a column.
pub const NARROW: u16 = 100;

/// The two shapes the rail is drawn in, chosen by the width of the frame it sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// §12's 32 columns down the frame's right-hand edge, one labelled fact per row.
    Ledger,
    /// The same facts on one row along the bottom of the band, which is all a narrow
    /// frame has room for.
    Strip,
}

impl Shape {
    /// The shape a frame `width` columns wide is drawn in: one rule, in one place, so
    /// the frame's width and the rail's shape cannot be decided twice and disagree.
    pub fn of(width: u16) -> Shape {
        if width < NARROW {
            Shape::Strip
        } else {
            Shape::Ledger
        }
    }
}

/// The columns a label field holds, whatever the label says: §46's contract fixes it at
/// ten, so the labels start in one column and the runs of leaders start in the next.
const FIELD: u16 = 10;

/// The fewest leader dots a ledger row is drawn with: a label butted straight against a
/// value is not a ledger row, so a value that leaves no room for one drops the fact.
const LEADER_MIN: usize = 1;

/// §46's blocks, top to bottom, each with the facts the contract puts in it: the dialect
/// and the model it served, the run's three meters, where it stands, what it has loaded,
/// how it is confined, and the session it is writing.
const BLOCKS: [&[Kind]; 6] = [
    &[Kind::Wire, Kind::Model],
    &[Kind::Turns, Kind::Ctx, Kind::Cache],
    &[Kind::Workspace, Kind::Branch],
    &[Kind::Skills],
    &[Kind::Sandbox],
    &[Kind::Session, Kind::Temp],
];

/// The columns a value may take in the ledger: the rail's own width less the label field
/// less the one leader a row needs to be a ledger row at all.
const VALUE: usize = (COLUMNS - FIELD) as usize - LEADER_MIN;

/// The columns a session's GUID is drawn by, wherever the rail draws one: §46's contract
/// draws the session by its first eight, and no rail column holds a whole GUID.
const SHORT: usize = 8;

/// The glyph a dot leader is drawn with.
const LEADER: char = '.';

/// What lies between two facts on the strip's one row, the same gap the footer puts
/// between its hints.
const GAP: &str = " · ";

/// The hairline that runs the rail's full width between two blocks.
const HAIRLINE: char = '─';

/// The facts the rail draws, as the core holds them: the dialect the run asks over and
/// the model it was served by name, the turns it has opened against the cap it obeys, the
/// two meters, the workspace and the branch it stands on, the skills in force, the sandbox
/// posture, and the session's GUID and temp.
///
/// A fact the frame knows nothing about is `None` and is not drawn, never guessed at
/// (§15), so a frame before its first run has no rail at all. Three of these nothing
/// produces yet — the context meter, the cache meter and the skills in force — and a
/// caller passes them in until the tickets that fill them land.
#[derive(Debug, Default, Clone)]
pub struct Facts {
    /// The dialect the run asks over, in the wire's own word: `anthropic`, `openai` or
    /// `copilot`, and never a fourth.
    pub wire: Option<String>,
    /// The model the run asks, by the name its wire served.
    pub model: Option<String>,
    /// The turns the run has opened, and the cap it obeys.
    pub turns: Option<(u32, u32)>,
    /// How much of the context window the run has spent, in whole percent.
    pub ctx: Option<u8>,
    /// How much of the run's input the provider served from its own cache, in whole
    /// percent (ADR-0010).
    pub cache: Option<u8>,
    /// The one directory the run stands in.
    pub workspace: Option<PathBuf>,
    /// The branch that directory's checkout is on, when it is on one.
    pub branch: Option<String>,
    /// How many skills are in force.
    pub skills: Option<usize>,
    /// How the run's commands are confined: the two-valued fact §12 draws.
    pub sandbox: Option<Enforcement>,
    /// The GUID of the session the run is writing, whole: the rail draws it short.
    pub session: Option<String>,
    /// The session temp, which the rail draws under that short GUID.
    pub temp: Option<PathBuf>,
}

/// The rail's share of the band it sits in with the transcript: what the transcript
/// keeps, and where the rail draws — in which of its two shapes, and in which rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share {
    /// The band the transcript is laid out and painted in.
    pub transcript: Rect,
    /// The rail's own room and the shape it is drawn in, or nothing when the frame knows
    /// no fact at all and the transcript keeps the whole band.
    pub rail: Option<(Shape, Rect)>,
}

impl Facts {
    /// The facts a run holds, read off the two values the core keeps them in: the session
    /// the run is writing, whose GUID and temp the rail draws, and the document the
    /// record rewrites as facts land — the wire and the model it served, the turns against
    /// the cap, the enforcement in force, the workspace and the branch.
    ///
    /// Three facts nothing produces yet — the context meter, the cache meter and the
    /// skills in force — are left for the caller to hand in, and so is every frame's
    /// `Facts` until the run loop hands one to a frame (#43).
    pub fn of(session: &Session, document: &SessionDocument) -> Facts {
        Facts {
            wire: Some(document.wire.word().to_owned()),
            model: Some(document.model.clone()),
            turns: Some((document.turns, document.dials.max_turns)),
            sandbox: Some(document.enforcement),
            workspace: Some(document.workspace.clone()),
            branch: document.branch.clone(),
            session: Some(session.guid().to_owned()),
            temp: Some(session.temp().to_path_buf()),
            ..Facts::default()
        }
    }

    /// Cuts the band the transcript and the rail share: §12 puts the rail down the frame's
    /// right-hand 32 columns while the frame is wide enough to hold a column, and on one
    /// strip along the band's bottom row below a hundred columns. A frame that knows no
    /// fact draws no rail, and the transcript loses no room to one.
    pub fn split(&self, band: Rect) -> Share {
        if band.width == 0 || band.height == 0 || self.known().is_empty() {
            return Share {
                transcript: band,
                rail: None,
            };
        }
        let shape = Shape::of(band.width);
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

    /// Paints the rail into `area` in `shape`: the ledger's rows or the strip's one, in
    /// the inks a ledger is read in.
    pub fn paint(&self, shape: Shape, area: Rect, buffer: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        match shape {
            Shape::Ledger => self.ledger(area, buffer),
            Shape::Strip => self.strip(area, buffer),
        }
    }

    /// The ledger: one fact per row, a label in its field, leaders out to a value against
    /// the rail's edge, and a hairline between two blocks.
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
                    put(buffer, area.x, y, &format!("+{went} more"), Ink::Muted);
                }
            }
            y = y.saturating_add(1);
        }
    }

    /// The strip: the same facts on the band's one row, each as its label and its value
    /// with a gap between them, because a narrow frame has a row to spare and no column.
    /// What went is counted at the end of the row when the row has room for the count.
    fn strip(&self, area: Rect, buffer: &mut Buffer) {
        let kept = self.kept(Shape::Strip, area);
        let mut x = area.x;
        for (index, fact) in kept.facts.iter().enumerate() {
            if index > 0 {
                x = put(buffer, x, area.y, GAP, Ink::Dim);
            }
            x = put(buffer, x, area.y, fact.kind.label(), Ink::Dim);
            x = put(buffer, x, area.y, " ", Ink::Dim);
            x = put(buffer, x, area.y, &fact.value, Ink::Muted);
        }
        // A strip too narrow to hold the count beside the fact that never goes is the one
        // rail without one.
        if kept.went > 0 && strip_columns(&kept.facts, kept.went) <= usize::from(area.width) {
            if !kept.facts.is_empty() {
                x = put(buffer, x, area.y, GAP, Ink::Dim);
            }
            put(
                buffer,
                x,
                area.y,
                &format!("+{} more", kept.went),
                Ink::Muted,
            );
        }
    }

    /// The facts this rail keeps and how many it gave up: one whose value will not fit
    /// the column it is drawn in goes before anything else, and what is left is given up
    /// in §12's order until the rail's room holds the rest and the row that counts what
    /// went (#46).
    fn kept(&self, shape: Shape, area: Rect) -> Kept {
        let known = self.known();
        let total = known.len();
        let mut kept: Vec<Fact> = known
            .into_iter()
            .filter(|fact| fits(fact, shape, area.width))
            .collect();
        let mut went = total - kept.len();
        while !holds(&kept, went, shape, area) {
            let Some(victim) = SACRIFICE
                .iter()
                .find_map(|kind| kept.iter().position(|fact| fact.kind == *kind))
            else {
                // Only the sandbox is left and it never goes: a rail narrower than its last
                // fact has nothing left to give up.
                break;
            };
            kept.remove(victim);
            went = went.saturating_add(1);
        }
        Kept { facts: kept, went }
    }

    /// Every fact this frame knows, in the order §46's blocks read them, each value
    /// spelled the way the rail spells it: a session by the first eight of its GUID, a
    /// path by the tail that fits a value column.
    fn known(&self) -> Vec<Fact> {
        let short = self.session.as_deref().map(short_id);
        let mut facts = Vec::new();
        if let Some(wire) = &self.wire {
            facts.push(Fact {
                kind: Kind::Wire,
                value: wire.clone(),
            });
        }
        if let Some(model) = &self.model {
            facts.push(Fact {
                kind: Kind::Model,
                value: model.clone(),
            });
        }
        if let Some((turns, cap)) = self.turns {
            facts.push(Fact {
                kind: Kind::Turns,
                value: format!("{turns}/{cap}"),
            });
        }
        if let Some(ctx) = self.ctx {
            facts.push(Fact {
                kind: Kind::Ctx,
                value: format!("{ctx}%"),
            });
        }
        if let Some(cache) = self.cache {
            facts.push(Fact {
                kind: Kind::Cache,
                value: format!("{cache}%"),
            });
        }
        if let Some(workspace) = &self.workspace {
            let value = tail(&workspace.to_string_lossy(), VALUE);
            if !value.is_empty() {
                facts.push(Fact {
                    kind: Kind::Workspace,
                    value,
                });
            }
        }
        if let Some(branch) = &self.branch {
            facts.push(Fact {
                kind: Kind::Branch,
                value: branch.clone(),
            });
        }
        if let Some(skills) = self.skills {
            facts.push(Fact {
                kind: Kind::Skills,
                value: skills.to_string(),
            });
        }
        if let Some(sandbox) = self.sandbox {
            facts.push(Fact {
                kind: Kind::Sandbox,
                value: sandbox.rail_label().to_owned(),
            });
        }
        if let Some(short) = short {
            facts.push(Fact {
                kind: Kind::Session,
                value: short.to_owned(),
            });
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
                facts.push(Fact {
                    kind: Kind::Temp,
                    value,
                });
            }
        }
        facts
    }
}

/// One fact as the rail draws it: which of §12's facts it is, which is also the label
/// its field holds and its place in §12's sacrifice order, and the value beside it.
struct Fact {
    /// Which fact this is.
    kind: Kind,
    /// The value, in the rail's own words.
    value: String,
}

/// What the rail keeps and what it gave up: the facts it draws, in the order it reads
/// them, and the count of those it could not hold.
struct Kept {
    /// The facts that survive the room, in block order.
    facts: Vec<Fact>,
    /// How many of the facts the frame knew the rail could not hold.
    went: usize,
}

/// One row of the ledger, before it is a row of glyphs: a fact, the hairline between two
/// blocks, or §12's row counting what went. Pure, so the height a set of facts needs is a
/// number a test reads without a buffer.
enum Row<'a> {
    /// One labelled fact, drawn in its field with its leaders and its value.
    Fact(&'a Fact),
    /// The hairline that runs the rail's width between two blocks.
    Hairline,
    /// The foot of the rail, saying how many facts it gave up.
    Went(usize),
}

/// The ledger's rows in the order they are read: each block's facts, a hairline between
/// two blocks that both kept one, and the row that counts what went at the foot when
/// anything did (§46's block order, §12's `+7 more`).
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

/// Whether one fact is drawn whole on a row `width` columns wide: a label in its field, a
/// value against the edge, and at least one leader between them, because a label butted
/// straight against a value is not a ledger row. A value that leaves no room for the
/// leader drops the fact rather than being cut (#46).
fn fits(fact: &Fact, shape: Shape, width: u16) -> bool {
    let value = fact.value.chars().count();
    match shape {
        Shape::Ledger => usize::from(FIELD) + LEADER_MIN + value <= usize::from(width),
        Shape::Strip => columns(fact.kind.label()) + 1 + value <= usize::from(width),
    }
}

/// Whether the room holds what the rail kept: the ledger's rows within the band's height,
/// or the strip's one row within its width.
fn holds(facts: &[Fact], went: usize, shape: Shape, area: Rect) -> bool {
    match shape {
        Shape::Ledger => ledger_rows(facts, went).len() <= usize::from(area.height),
        Shape::Strip => strip_columns(facts, went) <= usize::from(area.width),
    }
}

/// The columns the strip's one row takes, the count of what went included.
fn strip_columns(facts: &[Fact], went: usize) -> usize {
    let mut taken = 0;
    for (index, fact) in facts.iter().enumerate() {
        if index > 0 {
            taken += columns(GAP);
        }
        taken += columns(fact.kind.label()) + 1 + columns(&fact.value);
    }
    if went > 0 {
        if !facts.is_empty() {
            taken += columns(GAP);
        }
        taken += columns(&format!("+{went} more"));
    }
    taken
}

/// One row of the ledger: the label left in its field, dot leaders filling the room up
/// to the value, and the value against the rail's right-hand edge — the one hanging edge
/// §46's contract asks for.
fn ledger_row(buffer: &mut Buffer, area: Rect, fact: &Fact) {
    let value = fact.value.chars().count();
    let start = usize::from(area.width).saturating_sub(value);
    let label = format!("{:<width$}", fact.kind.label(), width = usize::from(FIELD));
    put(buffer, area.x, area.y, &label, Ink::Dim);
    let leaders: String =
        std::iter::repeat_n(LEADER, start.saturating_sub(usize::from(FIELD))).collect();
    put(buffer, area.x + FIELD, area.y, &leaders, Ink::Dim);
    let x = area
        .x
        .saturating_add(u16::try_from(start).unwrap_or(area.width));
    put(buffer, x, area.y, &fact.value, Ink::Muted);
}

/// The columns `text` takes: one per glyph, whatever the glyph's bytes say, because a
/// cell of a frame is one glyph wide and `·` is three bytes and one column.
fn columns(text: &str) -> usize {
    text.chars().count()
}

/// Writes `text` from `x` across a row, one glyph per cell, in `ink`, and hands back the
/// column after it: every row here is measured before it is written, so this only has to
/// stop at the frame's edge.
fn put(buffer: &mut Buffer, x: u16, y: u16, text: &str, ink: Ink) -> u16 {
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

/// The tail of a path in at most `columns`: as many of the path's own components as fit,
/// with `…/` in front when the head did not fit beside them.
///
/// A state-home path is longer than any rail column, so a rail that printed one whole
/// would print no temp at all — and the head of a path says less about which temp it is
/// than its tail does. Nothing is ever cut mid-component: a name with its middle gone is
/// the `san…` §46 forbids, and a tail that cannot hold even the last component is empty,
/// which drops the fact.
fn tail(path: &str, columns: usize) -> String {
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

/// A session's GUID by its first eight columns, or whole when it is shorter than that:
/// the first eight name one session directory as well as any other eight columns do, and
/// no rail column holds thirty-six.
fn short_id(guid: &str) -> &str {
    match guid.char_indices().nth(SHORT) {
        Some((end, _)) => &guid[..end],
        None => guid,
    }
}

/// Which fact a row of the rail carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// The dialect the run asks over.
    Wire,
    /// The model the run asks, by the name its wire served.
    Model,
    /// The turns the run has opened, against the cap it obeys.
    Turns,
    /// How much of the context window the run has spent.
    Ctx,
    /// How much of the run's input the provider served from its own cache.
    Cache,
    /// The one directory the run stands in.
    Workspace,
    /// The branch that directory's checkout is on.
    Branch,
    /// How many skills are in force.
    Skills,
    /// How the run's commands are confined.
    Sandbox,
    /// The GUID of the session the run is writing.
    Session,
    /// The session's temp.
    Temp,
}

impl Kind {
    /// The word the label field holds: the name the ticket's contract gives the fact,
    /// so the rail and the ticket cannot call one fact two things (#46).
    fn label(self) -> &'static str {
        match self {
            Kind::Wire => "wire",
            Kind::Model => "model",
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

/// §12's sacrifice order, the first to go first: what the rail gives up as the room
/// runs out. The sandbox posture is deliberately absent, because §12 says it never
/// goes, so no loop over this list can reach it.
const SACRIFICE: [Kind; 10] = [
    Kind::Temp,
    Kind::Cache,
    Kind::Session,
    Kind::Branch,
    Kind::Skills,
    Kind::Ctx,
    Kind::Workspace,
    Kind::Turns,
    Kind::Wire,
    Kind::Model,
];

#[cfg(test)]
mod tests {
    //! The rail's own rules, one test each: the order it gives facts up in, the label
    //! field and its leaders, the blocks and their hairlines, the strip a narrow frame
    //! gets instead of the columns, and the row that counts what went. None of them
    //! needs a terminal — where a cell's glyph and ink are the question, the buffer the
    //! ledger paints into is the answer, and no fixture is asked about a rule a loop can
    //! state (§12, ADR-0012).

    use std::path::Path;

    use sindri_core::policy::PolicyRecord;
    use sindri_core::records::{Dials, SessionDocument, Usage};
    use sindri_core::run::State as RunState;
    use sindri_core::session::Session;
    use sindri_core::wire::Wire;

    use super::*;

    /// A rectangle at the origin: the band a frame hands the rail, or the room the
    /// rail takes from it.
    fn band(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// One row of a painted buffer as the glyphs it holds, so a ledger row is read the
    /// way a reader reads it rather than the way it was written.
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

    /// The runs of ink along one row, left to right, each with the cells it covers: a
    /// row's inks are stated as the runs an eye sees and not cell by cell.
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

    /// The facts a frame of a run holds, as the rails draw them: one of each of §46's
    /// blocks, with a workspace and a temp as long as the real ones.
    fn frame() -> Facts {
        Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from("claude-sonnet")),
            turns: Some((3, 250)),
            ctx: Some(21),
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

    /// §46's contract, as a test states it rather than as the code holds it: every fact
    /// the rail draws, its label and its value, so a fact's wholeness and the count of
    /// what went can be read off the screen and not off the module's own tables.
    const CONTRACT: [(&str, &str); 11] = [
        ("wire", "anthropic"),
        ("model", "claude-sonnet"),
        ("turns", "3/250"),
        ("ctx", "21%"),
        ("cache", "88%"),
        ("workspace", "…/.worktrees/46-rail"),
        ("branch", "spec/1-capability"),
        ("skills", "2"),
        ("sandbox", "bwrap"),
        ("session", "9f2c7a41"),
        ("temp", "…/9f2c7a41/tmp"),
    ];

    /// The session document a run writes, as the core holds one: filled in by hand so the
    /// rail's wiring is read off §13's fields and not off a real run.
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
            "nothing produces these three yet, so the rail is handed them and not told \
             them by the core"
        );
    }

    #[test]
    fn the_ledger_reads_its_blocks_in_the_contracts_order_with_a_hairline_between_two() {
        let mut buffer = Buffer::empty(band(COLUMNS, 16));

        frame().paint(Shape::Ledger, band(COLUMNS, 16), &mut buffer);

        let rows: Vec<String> = (0..16).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire      .............anthropic",
                "model     .........claude-sonnet",
                "────────────────────────────────",
                "turns     .................3/250",
                "ctx       ...................21%",
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
            inks(&buffer, 2),
            [(Ink::Dim, 32)],
            "the hairline runs the rail's width"
        );
        assert_eq!(
            inks(&buffer, 3),
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
    fn a_value_with_no_room_for_a_leader_drops_the_fact_rather_than_being_cut() {
        let facts = Facts {
            model: Some(String::from("sonnet-with-a-very-long-name")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 2));

        facts.paint(Shape::Ledger, band(COLUMNS, 2), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0),
            "+1 more                         ",
            "a served name wider than the column leaves no room for a leader in a \
             thirty-two column rail, so the fact goes and the row says so"
        );
        assert!(
            !glyphs(&buffer, 0).contains("sonnet"),
            "and never the start of it in the fact's place"
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
                "model     .........claude-sonnet",
                "────────────────────────────────",
                "turns     .................3/250",
                "────────────────────────────────",
                "sandbox   .................bwrap",
                "+7 more                         ",
                "                                ",
            ],
            "temp, cache, session, branch, skills, ctx and workspace went, in that \
             order, and the foot of the rail says seven"
        );
    }

    #[test]
    fn a_narrow_frame_gives_facts_up_in_a12s_order_and_counts_what_went() {
        let mut buffer = Buffer::empty(band(40, 1));

        frame().paint(Shape::Strip, band(40, 1), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0),
            "sandbox bwrap · +10 more                ",
            "forty columns hold the fact that never goes and the count of the ten that did"
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
            let kept = CONTRACT
                .iter()
                .filter(|(label, _)| screen.contains(label))
                .count();
            let foot = "sandbox bwrap".len() + GAP.len() + more.len();
            assert!(
                went != 0 && kept <= 1 && foot > usize::from(width),
                "at {width} columns the rail dropped the {more} row it had room to draw"
            );
        }
    }

    #[test]
    fn a_strip_row_is_the_facts_beside_each_other_each_with_its_label_and_one_gap() {
        let mut buffer = Buffer::empty(band(200, 1));

        frame().paint(Shape::Strip, band(200, 1), &mut buffer);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            "wire anthropic · model claude-sonnet · turns 3/250 · ctx 21% · cache 88% · \
             workspace …/.worktrees/46-rail · branch spec/1-capability · skills 2 · sandbox \
             bwrap · session 9f2c7a41 · temp …/9f2c7a41/tmp",
            "one row, the same facts in the same order, a gap between two of them"
        );
    }

    #[test]
    fn a_fact_is_drawn_in_the_words_the_rail_reads_it_in() {
        let facts: Vec<(&str, String)> = frame()
            .known()
            .into_iter()
            .map(|fact| (fact.kind.label(), fact.value))
            .collect();

        assert_eq!(
            facts,
            [
                ("wire", String::from("anthropic")),
                ("model", String::from("claude-sonnet")),
                ("turns", String::from("3/250")),
                ("ctx", String::from("21%")),
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
