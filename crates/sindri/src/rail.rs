//! The rail: the run's facts down the frame's right-hand columns, read as a ledger —
//! a label in a fixed field, dot leaders running out to one hanging edge, a hairline
//! between blocks — and given up, fact by fact, when the frame is too narrow to hold all
//! of it.
//!
//! The facts are handed in and never reached for: `Facts` is what the core knows about a
//! run — the dialect it asks over, the name it was served and whether that model thinks,
//! the turns against the cap, the enforcement in force, the workspace, the branch, the
//! session's GUID and its temp — and the one fact nothing produces yet is a caller's to
//! fill, so a rail draws a fact only when somebody handed it one.
//!
//! Two shapes and two questions. At a hundred columns and above the rail is 32 columns
//! down the frame's right-hand edge, one labelled fact per row, and it carries every fact
//! the frame knows. Below a hundred it is one strip along the bottom of the band, and it
//! carries a shorter list of its own — the run's size, the model, the confinement and the
//! branch — because a narrow frame has one row and a reader watching it wants the facts
//! that change. [`Facts::split`] is the rule between them: the band the transcript and the
//! rail share, cut where the width says, so the frame's paint asks for its share rather
//! than counting columns of its own, and the transcript is laid out at the width the rail
//! leaves it (ADR-0008).
//!
//! Nothing is ever drawn in part. A value with no cut rule of its own that leaves no room
//! for a leader in its column drops the fact rather than being cut off; a path keeps whole
//! components and marks the head it left with `…/`; a served name keeps whole
//! hyphen-separated segments from its head and marks the tail it left with `…`; and what
//! the room cannot hold is given up in one settled order — temp path, cache hits, session
//! GUID, branch, skills in force, context size, workspace, turns, wire, thinking, model —
//! beside a row saying how many went. That is the ledger's order; the sandbox posture is
//! absent from it because it never goes, and the strip gives its own four facts up from the
//! tail of its own list instead, where the count of what went is the last item of the row
//! and so the first thing the room takes. The degradation is a pure function of the room,
//! not a paint: what the rail keeps and the rows it makes are read by a test without a
//! buffer, which is how a rule about every frame width from 20 to 200 is proved rather than
//! eyeballed.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::policy::Enforcement;
use sindri_core::provider::Usage;
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

/// The share of a window at which the rail raises its voice: a run ninety percent
/// full is one turn from being no room at all, and telling that from the ordinary case is
/// the one thing the ctx row says twice.
const FULL: u8 = 90;

/// §46's blocks, top to bottom, each with the facts the contract puts in it: the dialect,
/// the model it served and whether it thinks, the turns it has taken, the run's size and
/// the cache's hit rate, where it stands, what it has loaded, how it is confined, and the
/// session it is writing.
const BLOCKS: [&[Kind]; 6] = [
    &[Kind::Wire, Kind::Model, Kind::Think],
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

/// The run's size, measured once per turn by the wire: the input it last reported, and the
/// window the config names, when a reader named one.
///
/// It is the size the last request carried and never the size of the next one, which grows
/// by whatever tool results the turn produced, so a run one turn from a full window is a run
/// whose window is nearly full and no more exactly than that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    /// The input tokens the wire last reported for the run.
    pub size: u64,
    /// How many tokens the model's window holds, when a reader named one.
    pub window: Option<u64>,
}

impl Ctx {
    /// The value the rail draws it as in `shape` — the size in spelled tokens, and the
    /// window beside it and the share of it the run fills when one is known — and the share
    /// that value ends in, so the row's text and the ink it is said in come from one
    /// derivation.
    ///
    /// The strip keeps the share and gives up the size when both are known: it is one row
    /// shared with three other facts, and the number that says how much room the next turn
    /// has left is the one a reader watches. A frame that knows no window has no share to
    /// keep, so both shapes draw the size there.
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

    /// How full the window is, when one is known: a window of no tokens is no share of
    /// anything, which `share` is the one place to say.
    ///
    /// The ink is set here and nowhere else, so no paint site holds a second opinion about
    /// what nearly full means.
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

/// How full the run's window is, and where the rail says so: the tail of the ctx row, which
/// is the one fact on the rail that says how much room the next turn has left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fill {
    /// How full the window is, in whole percent.
    percent: u8,
    /// The window it is a share of, in tokens.
    window: u64,
    /// The ink the share is drawn in: `muted` while there is room, and `accent` once the
    /// window is nearly full.
    ink: Ink,
}

impl Fill {
    /// The share as the rail writes it, `%` and all.
    fn said(&self) -> String {
        format!("{}%", self.percent)
    }
}

/// The share of a whole that a part is, in whole percent, rounded: capped at a hundred,
/// because a part already past its whole has no larger share to show and the rail cannot
/// claim room that is not there. A share of nothing is no percentage at all.
///
/// One rule for the two rows that are a percentage of something: the window a run fills, and
/// the part of its input the provider served from its own cache.
fn share(part: u64, whole: u64) -> Option<u8> {
    if whole == 0 {
        return None;
    }
    let percent = (u128::from(part) * 100 + u128::from(whole) / 2) / u128::from(whole);
    Some(u8::try_from(percent.min(100)).unwrap_or(100))
}

/// The size of a run in spelled tokens: whole under a thousand, and three figures with a
/// unit above it — `948`, `48.2k`, `1.02M` — because no reader counts six digits off a
/// rail, and a rail column has no room to make them read as one number.
fn tokens(count: u64) -> String {
    if count < 1_000 {
        return count.to_string();
    }
    // Tenths of a thousand, and hundredths of a million: the first carries three figures up
    // to nine hundred and ninety-nine thousand, and the second once rounding takes a count
    // past four.
    let tenths = count.saturating_add(50) / 100;
    if tenths < 10_000 {
        return scaled(tenths, 1, "k");
    }
    scaled(count.saturating_add(5_000) / 10_000, 2, "M")
}

/// An amount counted in a unit's smallest shown fraction, drawn as that unit's whole part,
/// a decimal point and what is left of the fraction: `482` tenths of a thousand is `48.2k`.
/// A zero left dangling at the end of the decimals is dropped, so two hundred thousand is
/// `200k` and not `200.0k`.
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

/// The facts the rail draws, as the core holds them: the dialect the run asks over and
/// the model it was served by name, the turns it has opened against the cap it obeys, the
/// run's size, the cache's hit rate, the workspace and the branch it stands on, the skills
/// in force, the sandbox posture, and the session's GUID and temp.
///
/// A fact the frame knows nothing about is `None` and is not drawn, never guessed at, so a
/// frame before its first run has no rail at all. The skills in force are the one fact
/// nothing produces yet, and a caller passes that one in until its ticket lands.
#[derive(Debug, Default, Clone)]
pub struct Facts {
    /// The dialect the run asks over, in the wire's own word: `anthropic`, `openai` or
    /// `copilot`, and never a fourth.
    pub wire: Option<String>,
    /// The served name of the model the run asks: a model identifier read off the wire and
    /// never invented, whole here, and cut to its head wherever the rail draws it, because
    /// no value column holds a dated name.
    pub model: Option<String>,
    /// Whether the model thinks before it answers, in the one word the rail draws it
    /// by: `high` or `off`, and never a third.
    pub think: Option<String>,
    /// The turns the run has opened, and the cap it obeys.
    pub turns: Option<(u32, u32)>,
    /// The run's size as the wire last measured it, and the window it is a share of when
    /// one is known.
    pub ctx: Option<Ctx>,
    /// How much of the run's input the provider served from its own cache, in whole
    /// percent (ADR-0010): a hit rate and not a fill, so it gets no meter and no threshold.
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
    /// record rewrites as facts land — the wire, the model it served and whether it thinks,
    /// the turns against the cap, the enforcement in force, the workspace and the branch.
    ///
    /// The run's size and the cache's hit rate are one ask's report and are handed in by
    /// [`Facts::measured`], and the skills in force are the one fact left for a caller.
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

    /// The size of the conversation as one ask's report measured it — the input that ask
    /// carried, the window a reader named a share of it, and the part of it the provider
    /// served from its own cache.
    ///
    /// The report is one turn's and never a running total, which is why the document cannot
    /// hand these in: the `usage` it keeps is every ask summed, and a sum of the inputs grows
    /// with each turn while the conversation's size is what the newest request carried. A
    /// report that states no size measures nothing, so the frame is handed back as it was.
    pub fn measured(mut self, ask: &Usage, window: Option<u64>) -> Facts {
        let Some(size) = ask.input else {
            return self;
        };
        self.ctx = Some(Ctx { size, window });
        self.cache = ask.cached_input.and_then(|cached| share(cached, size));
        self
    }

    /// Cuts the band the transcript and the rail share: §12 puts the rail down the frame's
    /// right-hand 32 columns while the frame is wide enough to hold a column, and on one
    /// strip along the band's bottom row below a hundred columns. A frame that knows no
    /// fact draws no rail, and the transcript loses no room to one.
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

    /// The strip: its own four facts on the band's one row, each as its label and its value
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
            x = fact.draw(buffer, x, area.y);
        }
        // A strip too narrow to hold the count beside the facts it keeps is the one rail
        // without one.
        if kept.counted {
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

    /// The facts this rail keeps and how many it gave up: one whose value will not fit the
    /// row it is drawn in goes before anything else, and what is left is given up until the
    /// room holds it.
    ///
    /// The shapes part here. The ledger draws every fact it knows, one block after another,
    /// and gives them up in §12's settled order beside the row that says how many went. The
    /// strip carries the four facts of its own list and no others, and it gives them up from
    /// the tail of that list — but the count of what went is the last item of its one row,
    /// so the room takes the count first, and only then a fact, because a fact a reader can
    /// see says more than a number that only says how much they cannot.
    fn kept(&self, shape: Shape, area: Rect) -> Kept {
        let known = self.known(shape);
        let total = known.len();
        let mut kept: Vec<Fact> = known
            .into_iter()
            .filter(|fact| shape == Shape::Ledger || STRIP.contains(&fact.kind))
            .filter(|fact| fits(fact, shape, area.width))
            .collect();
        if shape == Shape::Strip {
            kept.sort_by_key(|fact| strip_place(fact.kind));
        }
        let order: &[Kind] = match shape {
            Shape::Ledger => &SACRIFICE,
            Shape::Strip => &STRIP_SACRIFICE,
        };
        let mut went = total - kept.len();
        let mut uncounted = false;
        loop {
            let counted = went > 0 && !uncounted;
            if holds(&kept, went, counted, shape, area) {
                break;
            }
            if counted && holds(&kept, went, false, shape, area) {
                uncounted = true;
                continue;
            }
            let Some(victim) = order
                .iter()
                .find_map(|kind| kept.iter().position(|fact| fact.kind == *kind))
            else {
                // Nothing the shape may give up is left — the ledger keeps the sandbox
                // posture and the strip the run's size — so a rail narrower than that fact
                // has nothing left to give up.
                break;
            };
            kept.remove(victim);
            went = went.saturating_add(1);
        }
        Kept {
            facts: kept,
            went,
            counted: went > 0 && !uncounted,
        }
    }

    /// Every fact this frame knows, block by block, each value spelled the way the shape it
    /// is drawn in spells it: a session by the first eight of its GUID, a path by the tail
    /// that fits a value column, a served name by the head, and the run's size by the share
    /// of its window wherever the strip has room for only one of the two.
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
    /// How full the window is, when this fact's value ends in a share of one.
    fill: Option<Fill>,
}

impl Fact {
    /// A fact drawn as one run of `muted`: the shape every value but the ctx row's takes.
    fn new(kind: Kind, value: impl Into<String>) -> Fact {
        Fact {
            kind,
            value: value.into(),
            fill: None,
        }
    }

    /// Writes the value from `x` across a row, the share at its tail in the fill's ink and
    /// the rest in `muted`, and hands back the column after it.
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

/// What the rail keeps and what it gave up: the facts it draws, in the order it reads
/// them, and the count of those it could not hold.
struct Kept {
    /// The facts that survive the room, in the order the shape reads them.
    facts: Vec<Fact>,
    /// How many of the facts the frame knew the rail could not hold.
    went: usize,
    /// Whether the rail says how many went: the ledger always does when one did, and the
    /// strip only while its one row has room for the count beside the facts it kept.
    counted: bool,
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
/// leader drops the fact rather than being cut, and so does a value with nothing left in
/// it — a fact cut to nothing says nothing.
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

/// Where a fact sits in the strip's list: the place the strip reads it in, and the place it
/// is given up in counted from the other end.
fn strip_place(kind: Kind) -> usize {
    STRIP
        .iter()
        .position(|read| *read == kind)
        .unwrap_or(usize::MAX)
}

/// Whether the room holds what the rail kept: the ledger's rows within the band's height,
/// or the strip's one row within its width.
fn holds(facts: &[Fact], went: usize, counted: bool, shape: Shape, area: Rect) -> bool {
    match shape {
        Shape::Ledger => ledger_rows(facts, went).len() <= usize::from(area.height),
        Shape::Strip => strip_columns(facts, went, counted) <= usize::from(area.width),
    }
}

/// The columns the strip's one row takes, the count of what went included when `counted`
/// says the row carries one.
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
    fact.draw(buffer, x, area.y);
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

/// The head of a served name in at most `columns`: as many of the name's whole
/// hyphen-separated segments as fit while the `…` that marks what went is counted in, and
/// never a segment drawn in part.
///
/// A served name runs past any rail column — a dated Anthropic name is twenty-six and a
/// gateway's is forty-one — and widening the rail only moves the number it fails at. So a
/// name keeps its head, where the family a reader names lives, and gives up the stamps
/// that follow it: `claude-sonnet-4-5…` and never `…-20250929`. A gateway that puts its
/// vendor in front of the family keeps that prefix instead, because the rule is the head.
/// A name whose first segment does not fit beside its own mark is empty, which drops the
/// fact.
fn head(name: &str, columns: usize) -> String {
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
    /// Whether the model thinks before it answers.
    Think,
    /// The turns the run has opened, against the cap it obeys.
    Turns,
    /// The run's size, as the wire last measured it.
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

/// The four facts the strip carries, in the order it reads them: the thing you watch, the
/// thing you are, the thing that confines you, and the thing you are standing on. The
/// ledger draws these four inside its own blocks and in its own order; the strip has one
/// row and a different question on it, so its list is its own.
const STRIP: [Kind; 4] = [Kind::Ctx, Kind::Model, Kind::Sandbox, Kind::Branch];

/// The order the strip gives its facts up in: [`STRIP`] without its head, read from the
/// tail, so a reader who has to lose one loses the branch they are standing on before the
/// confinement that holds them, and the run's size — the head, the thing they watch — is
/// the one it never gives up.
const STRIP_SACRIFICE: [Kind; 3] = [Kind::Branch, Kind::Sandbox, Kind::Model];

/// §12's sacrifice order, the first to go first: what the rail gives up as the room
/// runs out. The sandbox posture is deliberately absent, because §12 says it never
/// goes, so no loop over this list can reach it.
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
    //! The rail's own rules, one test each: the order it gives facts up in, the label
    //! field and its leaders, the blocks and their hairlines, the strip a narrow frame
    //! gets instead of the columns, and the row that counts what went. None of them
    //! needs a terminal — where a cell's glyph and ink are the question, the buffer the
    //! ledger paints into is the answer, and no fixture is asked about a rule a loop can
    //! state (§12, ADR-0012).

    use std::path::Path;

    use sindri_core::policy::PolicyRecord;
    use sindri_core::provider::Usage as Report;
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

    /// The name the frame's run was served, as the wire served it: the dated Anthropic
    /// name a real run is given, which is wider than a value column.
    const SERVED: &str = "claude-sonnet-4-5-20250929";

    /// What the rail draws of [`SERVED`]: its head, and the mark saying the date stamp
    /// went. Written down here because it is the contract the rail is held to.
    const CUT: &str = "claude-sonnet-4-5…";

    /// The branch the frame's run stands on, as wide as a real one.
    const BRANCH: &str = "spec/1-capability";

    /// The facts a frame of a run holds, as the rails draw them: one of each of §46's
    /// blocks, with a workspace and a temp as long as the real ones.
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

    /// §46's contract, as a test states it rather than as the code holds it: every fact
    /// the rail draws, its label and its value, so a fact's wholeness and the count of
    /// what went can be read off the screen and not off the module's own tables.
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

    /// The ctx row a value deserves, as the rail draws it: the label in its field, leaders
    /// out to the rail's edge, and the value against it.
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
    }

    #[test]
    fn the_strip_carries_its_own_list_in_the_order_a_reader_watches_it() {
        let order: Vec<&str> = STRIP.iter().map(|kind| kind.label()).collect();

        assert_eq!(
            order,
            ["ctx", "model", "sandbox", "branch"],
            "the thing you watch, the thing you are, the thing that confines you, and the \
             thing you are standing on"
        );
        assert!(
            !STRIP.contains(&Kind::Turns),
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

    /// The value a row draws beside `label`, read off the painted row: the label's field
    /// and its leaders skipped and the padding after the value dropped, so a test reads the
    /// fact the way a reader reads it. A row that draws no such label carries no value.
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

    /// One served name at every width the rail is drawn at: whole when it fits, its head
    /// and the mark that says the rest went when it does not, and never part of it without
    /// a mark or wider than the row it is drawn in.
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
        // 220 columns is the whole row, gap for gap, so no fact is given up to prove order.
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
