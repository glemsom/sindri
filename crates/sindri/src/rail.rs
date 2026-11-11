//! The ledger's facts down the frame's right-hand columns as a ledger, or on one strip along the
//! band's bottom row below a hundred. Nothing is drawn in part: a fact whose value leaves no room
//! for it is given up whole and counted.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::Ink;
use crate::ledger::{
    Facts, Halves, Kind, Shape, Value, head, hit_rate, more, pair, short_id, tail,
};
use crate::palette::{Band, Palette};
use crate::pen;

pub const COLUMNS: u16 = 32;

/// The columns a label field holds, whatever the label says, so every label starts in one column
/// and every value hangs from the field's far side.
pub(crate) const FIELD: u16 = 10;

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

const VALUE: usize = (COLUMNS - FIELD) as usize;

/// What lies between two facts on the strip's one row, the gap the footer puts between its
/// hints.
const GAP: &str = " · ";

const HAIRLINE: char = '─';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share {
    pub transcript: Rect,
    /// Nothing when the frame knows no fact at all, and the transcript keeps the whole band.
    pub rail: Option<(Shape, Rect)>,
}

/// Cuts the band the transcript and the rail share: 32 columns down the right while the frame is
/// wide enough to hold a column, one strip along the band's bottom row below a hundred. A frame
/// that knows no fact draws no rail, and the transcript loses no room to one.
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

pub fn paint(facts: &Facts, shape: Shape, area: Rect, buffer: &mut Buffer, palette: &Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    buffer.set_style(area, palette.ground(Some(Band::Rail)));
    match shape {
        Shape::Ledger => ledger(facts, area, buffer, palette),
        Shape::Strip => strip(facts, area, buffer, palette),
    }
}

fn ledger(facts: &Facts, area: Rect, buffer: &mut Buffer, palette: &Palette) {
    let kept = kept(facts, Shape::Ledger, area);
    let mut y = area.y;
    for row in ledger_rows(&kept.facts, kept.went) {
        if y >= area.bottom() {
            return;
        }
        match row {
            Row::Fact(fact) => ledger_row(buffer, Rect { y, ..area }, fact, palette),
            Row::Hairline => hairline(buffer, Rect { y, ..area }, palette),
            Row::Went(went) => {
                pen::run(buffer, area.x, y, &more(went), Ink::Muted, palette);
            }
        }
        y = y.saturating_add(1);
    }
}

fn strip(facts: &Facts, area: Rect, buffer: &mut Buffer, palette: &Palette) {
    let kept = kept(facts, Shape::Strip, area);
    let mut x = area.x;
    for (index, fact) in kept.facts.iter().enumerate() {
        if index > 0 {
            x = pen::run(buffer, x, area.y, GAP, Ink::Dim, palette);
        }
        x = pen::run(buffer, x, area.y, fact.kind.label(), Ink::Dim, palette);
        x = pen::run(buffer, x, area.y, " ", Ink::Dim, palette);
        x = fact.value.draw(buffer, x, area.y, palette);
    }
    // A strip too narrow to hold the count beside the facts it keeps is the one rail
    // without one.
    if kept.counted {
        if !kept.facts.is_empty() {
            x = pen::run(buffer, x, area.y, GAP, Ink::Dim, palette);
        }
        pen::run(buffer, x, area.y, &more(kept.went), Ink::Muted, palette);
    }
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
        // The temp's path carries the session's whole GUID, so it is drawn under the
        // eight-column name the session row above it uses.
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

/// One fact as the rail draws it: the fact itself, which names its label field and its place in
/// the sacrifice order, and the value beside it.
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

/// One row of the ledger, before it is a row of glyphs.
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

/// Whether one fact is drawn whole on a row `room` columns wide: a label in its field and a value
/// hanging from the far edge of it. A value with no room between the two, or one cut to nothing,
/// drops the fact rather than being drawn in part.
fn fits(fact: &Fact, shape: Shape, room: u16) -> bool {
    let value = fact.value.columns();
    if value == 0 {
        return false;
    }
    match shape {
        Shape::Ledger => value <= value_room(room),
        Shape::Strip => columns(fact.kind.label()) + 1 + value <= usize::from(room),
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
        taken += columns(fact.kind.label()) + 1 + fact.value.columns();
    }
    if counted {
        if !facts.is_empty() {
            taken += columns(GAP);
        }
        taken += columns(&more(went));
    }
    taken
}

fn ledger_row(buffer: &mut Buffer, area: Rect, fact: &Fact, palette: &Palette) {
    let x = label_row(
        buffer,
        area,
        fact.kind.label(),
        fact.value.columns(),
        palette,
    );
    fact.value.draw(buffer, x, area.y, palette);
}

/// The label of one row in its field, and the column a value `value_width` columns wide starts at:
/// the rule both this rail and the screen that widens it draw a row by, so values end on one
/// hanging edge wherever they are read.
pub(crate) fn label_row(
    buffer: &mut Buffer,
    area: Rect,
    label: &str,
    value_width: usize,
    palette: &Palette,
) -> u16 {
    let field = format!("{label:<width$}", width = usize::from(FIELD));
    pen::run(buffer, area.x, area.y, &field, Ink::Dim, palette);
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
    let rule: String = std::iter::repeat_n(HAIRLINE, usize::from(area.width)).collect();
    pen::run(buffer, area.x, area.y, &rule, Ink::Dim, palette);
}

/// The columns a labelled row drawn in `width` columns leaves its value: the field comes off the
/// front, and a value left less than that is not drawn at all.
pub(crate) fn value_room(width: u16) -> usize {
    usize::from(width).saturating_sub(usize::from(FIELD))
}

/// The columns `text` draws in, from the pen that draws it.
pub(crate) fn columns(text: &str) -> usize {
    usize::from(pen::cells(text))
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
    //! The rail's own rules, one test each: the order it gives facts up in, the label field and the
    //! hanging edge, the blocks and their hairlines, the strip, and the row that counts what went. Where
    //! a cell's glyph and ink are the question, the buffer the ledger paints into is the answer.

    use std::path::PathBuf;

    use sindri_core::policy::Enforcement;

    use super::*;
    use crate::ledger::{Ctx, FULL_CELL, METER, tokens};
    use crate::readback::{column_of, inks};

    const PALETTE: Palette = Palette::TRUECOLOR;

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

    /// The dated Anthropic name a real run is served, which is wider than a value column.
    const SERVED: &str = "claude-sonnet-4-5-20250929";

    /// What the rail draws of [`SERVED`]: its head, and the mark saying the date stamp went.
    const CUT: &str = "claude-sonnet-4-5…";

    const BRANCH: &str = "spec/1-capability";

    /// The row a ctx fact of `value` is drawn on: its label in the ten-column field, and the value
    /// against the rail's last column.
    fn ctx_line(value: &str) -> String {
        let room = usize::from(COLUMNS) - usize::from(FIELD);
        format!(
            "{:<width$}{value:>room$}",
            "ctx",
            width = usize::from(FIELD)
        )
    }

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

    /// Every fact the rail draws, its label and its value, stated here and not read off the module's
    /// own tables, so a test cannot pass by agreeing with the code it is testing.
    const CONTRACT: [(&str, &str); 12] = [
        ("wire", "anthropic"),
        ("model", CUT),
        ("think", "high"),
        ("turns", "3 / 250"),
        ("ctx", "48.2k"),
        ("cache", "88%"),
        ("workspace", "…/.worktrees/46-rail"),
        ("branch", "spec/1-capability"),
        ("skills", "2"),
        ("sandbox", "bwrap"),
        ("session", "9f2c7a41"),
        ("temp", "…/9f2c7a41/tmp"),
    ];

    #[test]
    fn the_ledger_takes_the_right_hand_columns_and_the_strip_the_bands_bottom_row() {
        let wide = split(&frame(), band(120, 37));
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

        let narrow = split(&frame(), band(99, 21));
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
        let share = split(&Facts::default(), band(120, 37));

        assert_eq!(
            share.transcript,
            band(120, 37),
            "a rail with nothing in it is not drawn, and nothing narrows for it"
        );
        assert_eq!(share.rail, None, "there is no rail row to see");
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

            paint(
                &facts,
                Shape::Ledger,
                band(COLUMNS, 1),
                &mut buffer,
                &PALETTE,
            );

            assert_eq!(
                glyphs(&buffer, 0),
                ctx_line(value),
                "{size} tokens is drawn as {value} and never as the count itself"
            );
        }
    }

    #[test]
    fn a_run_with_a_window_draws_the_window_and_the_meter_of_the_share_it_fills() {
        for (size, window, value) in [
            (0, 200_000, "░░░░░░ 0 / 200k"),
            (5_000, 200_000, "░░░░░░ 5k / 200k"),
            (48_200, 0, "48.2k"),
            (48_200, 200_000, "█░░░░░ 48.2k / 200k"),
            (100_000, 1_020_000, "█░░░░░ 100k / 1.02M"),
            (200_000, 200_000, "██████ 200k / 200k"),
            (250_000, 200_000, "██████ 250k / 200k"),
        ] {
            let facts = Facts {
                ctx: Some(Ctx {
                    size,
                    window: Some(window),
                }),
                ..Facts::default()
            };
            let mut buffer = Buffer::empty(band(COLUMNS, 1));

            paint(
                &facts,
                Shape::Ledger,
                band(COLUMNS, 1),
                &mut buffer,
                &PALETTE,
            );

            assert_eq!(
                glyphs(&buffer, 0),
                ctx_line(value),
                "{size} tokens against a window of {window} is drawn as {value}: a share under \
                 half a cell of the meter lights none of it, a run past its window has no larger \
                 share to claim, and a window of no tokens is no window at all"
            );
        }
    }

    #[test]
    fn a_nearly_full_window_is_the_one_share_the_rail_raises_its_voice_about() {
        for (size, meter, ink) in [
            (178_000, "█████░", Ink::Plain),
            (180_000, "█████░", Ink::Accent),
            (200_000, "██████", Ink::Accent),
            (240_000, "██████", Ink::Accent),
        ] {
            let facts = Facts {
                ctx: Some(Ctx {
                    size,
                    window: Some(200_000),
                }),
                ..Facts::default()
            };
            let mut ledger = Buffer::empty(band(COLUMNS, 1));

            paint(
                &facts,
                Shape::Ledger,
                band(COLUMNS, 1),
                &mut ledger,
                &PALETTE,
            );

            let filled = meter.chars().filter(|cell| *cell == FULL_CELL).count();
            assert_eq!(
                glyphs(&ledger, 0),
                ctx_line(&format!("{meter} {} / 200k", tokens(size))),
                "{size} tokens of two hundred thousand fills {filled} of the meter's six cells"
            );
            // the meter stands where a value of "178k / 200k" leaves it, one column from the edge
            let at = usize::from(COLUMNS) - METER - 1 - "178k / 200k".len();
            for cell in 0..METER {
                let column = u16::try_from(at + cell).unwrap_or(u16::MAX);
                let drawn = Ink::of(
                    ledger.cell((column, 0)).expect("a cell of the meter"),
                    &PALETTE,
                );
                assert_eq!(
                    drawn,
                    Some(match cell < filled {
                        true => ink,
                        false => Ink::Dim,
                    }),
                    "cell {cell} of the meter {size} tokens fills is drawn in {drawn:?}: a share \
                     under ninety percent is read in `{}` and one at it in the accent, so a run \
                     one turn from a full window is the one the eye lands on",
                    ink.name()
                );
            }

            let mut strip = Buffer::empty(band(40, 1));

            paint(&facts, Shape::Strip, band(40, 1), &mut strip, &PALETTE);

            assert_eq!(
                Ink::of(strip.cell((4, 0)).expect("a cell of the meter"), &PALETTE),
                Some(ink),
                "the strip draws the same meter and reads it in the same ink the ledger does: \
                 one threshold, in one place"
            );
        }
    }

    #[test]
    fn a_frame_handed_no_size_draws_no_ctx_row_and_counts_nothing_for_it() {
        let mut unknown = frame();
        unknown.ctx = None;
        let mut roomy = Buffer::empty(band(COLUMNS, 17));

        paint(
            &unknown,
            Shape::Ledger,
            band(COLUMNS, 17),
            &mut roomy,
            &PALETTE,
        );

        let rows: Vec<String> = (0..17).map(|y| glyphs(&roomy, y)).collect();
        assert_eq!(
            rows[..7],
            [
                "wire                   anthropic",
                "model         claude-sonnet-4-5…",
                "think                       high",
                "────────────────────────────────",
                "turns                    3 / 250",
                "cache                        88%",
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

        paint(&frame(), Shape::Ledger, band, &mut tight, &PALETTE);
        let with = glyphs(&tight, 7);

        paint(&unknown, Shape::Ledger, band, &mut tight, &PALETTE);
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

        paint(
            &facts,
            Shape::Ledger,
            band(COLUMNS, 3),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..3).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire                   anthropic",
                "model              claude-sonnet",
                "think                       high",
            ],
            "the model block holds the fact, one row below the model it is about"
        );
    }

    #[test]
    fn the_ledger_reads_its_blocks_in_the_contracts_order_with_a_hairline_between_two() {
        let mut buffer = Buffer::empty(band(COLUMNS, 17));

        paint(
            &frame(),
            Shape::Ledger,
            band(COLUMNS, 17),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..17).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire                   anthropic",
                "model         claude-sonnet-4-5…",
                "think                       high",
                "────────────────────────────────",
                "turns                    3 / 250",
                "ctx                        48.2k",
                "cache                        88%",
                "────────────────────────────────",
                "workspace   …/.worktrees/46-rail",
                "branch         spec/1-capability",
                "────────────────────────────────",
                "skills                         2",
                "────────────────────────────────",
                "sandbox                    bwrap",
                "────────────────────────────────",
                "session                 9f2c7a41",
                "temp              …/9f2c7a41/tmp",
            ],
            "six blocks, a hairline between two of them, nothing given up at this width"
        );
        assert_eq!(
            inks(&buffer, 3, &PALETTE),
            [(Ink::Dim, 32)],
            "the hairline runs the rail's width"
        );
        assert_eq!(
            inks(&buffer, 4, &PALETTE),
            [(Ink::Dim, 10), (Ink::Plain, 22)],
            "the label field is `dim`, the ground stands between it and the value, and the \
             value is `plain`"
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

        paint(
            &facts,
            Shape::Ledger,
            band(COLUMNS, 2),
            &mut buffer,
            &PALETTE,
        );

        assert_eq!(
            (glyphs(&buffer, 0).trim_end(), glyphs(&buffer, 1).trim_end()),
            ("model              claude-sonnet", ""),
            "a rail handed no wire has no wire row, and nothing went, so no row counts it"
        );
    }

    #[test]
    fn a_ledger_row_draws_the_wire_by_its_word_and_nothing_beside_it() {
        for (word, row) in [
            ("anthropic", "wire                   anthropic"),
            ("openai", "wire                      openai"),
            ("copilot", "wire                     copilot"),
        ] {
            let facts = Facts {
                wire: Some(String::from(word)),
                ..Facts::default()
            };
            let mut buffer = Buffer::empty(band(COLUMNS, 1));

            paint(
                &facts,
                Shape::Ledger,
                band(COLUMNS, 1),
                &mut buffer,
                &PALETTE,
            );

            assert_eq!(
                glyphs(&buffer, 0),
                row,
                "the label in its field and the word against the rail's last column, and nothing else"
            );
        }
    }

    #[test]
    fn a_fact_with_no_cut_rule_of_its_own_is_dropped_when_no_column_is_left_for_it() {
        let facts = Facts {
            branch: Some(String::from("spec/a-branch-name-that-cannot-fit")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 2));

        paint(
            &facts,
            Shape::Ledger,
            band(COLUMNS, 2),
            &mut buffer,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&buffer, 0),
            "+1 more                         ",
            "a branch wider than a thirty-two column rail leaves no room for its value, so \
             the fact goes and the row says so"
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

        paint(
            &facts,
            Shape::Ledger,
            band(COLUMNS, 1),
            &mut ledger,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&ledger, 0),
            "model         claude-sonnet-4-5…",
            "a dated name takes the value column whole, and the mark says the date stamp went"
        );

        let mut strip = Buffer::empty(band(40, 1));

        paint(&facts, Shape::Strip, band(40, 1), &mut strip, &PALETTE);

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

        paint(
            &facts,
            Shape::Ledger,
            band(COLUMNS, 2),
            &mut ledger,
            &PALETTE,
        );

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

        paint(&facts, Shape::Strip, band(40, 1), &mut strip, &PALETTE);

        assert_eq!(
            glyphs(&strip, 0).trim_end(),
            "+1 more",
            "and the strip gives up the same fact, and says so in the same words"
        );
    }

    #[test]
    fn a_band_too_short_for_every_block_gives_facts_up_in_the_sacrifice_order() {
        let mut buffer = Buffer::empty(band(COLUMNS, 8));

        paint(
            &frame(),
            Shape::Ledger,
            band(COLUMNS, 8),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..8).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "wire                   anthropic",
                "model         claude-sonnet-4-5…",
                "think                       high",
                "────────────────────────────────",
                "turns                    3 / 250",
                "────────────────────────────────",
                "sandbox                    bwrap",
                "+7 more                         ",
            ],
            "temp, cache, session, branch, skills, ctx and workspace went, in that \
             order, and the foot of the rail says seven"
        );
    }

    #[test]
    fn a_band_that_can_only_hold_the_model_block_draws_the_model_without_its_think_row() {
        let mut buffer = Buffer::empty(band(COLUMNS, 4));

        paint(
            &frame(),
            Shape::Ledger,
            band(COLUMNS, 4),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..4).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "model         claude-sonnet-4-5…",
                "────────────────────────────────",
                "sandbox                    bwrap",
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

        paint(
            &facts,
            Shape::Ledger,
            band(COLUMNS, 1),
            &mut ledger,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&ledger, 0),
            "ctx          █░░░░░ 48.2k / 200k",
            "the ledger's ctx row says the size against the window it is a share of, and draws \
             that share as a meter beside them"
        );

        let mut strip = Buffer::empty(band(40, 1));

        paint(&facts, Shape::Strip, band(40, 1), &mut strip, &PALETTE);

        assert_eq!(
            glyphs(&strip, 0).trim_end(),
            "ctx █░░░░░",
            "the strip's one row has no room for the size or the window it is a share of, so \
             it keeps the meter that says how much of the window is left"
        );
    }

    #[test]
    fn forty_columns_hold_the_fact_a_reader_watches_and_the_model_they_are() {
        let mut buffer = Buffer::empty(band(40, 1));

        paint(&frame(), Shape::Strip, band(40, 1), &mut buffer, &PALETTE);

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

        paint(&facts, Shape::Strip, band(99, 1), &mut buffer, &PALETTE);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            format!("ctx █░░░░░ · model {CUT} · sandbox bwrap · branch {BRANCH} · +8 more"),
            "ninety-nine columns hold the whole list and the count of the eight facts the \
             strip does not carry at all"
        );
    }

    #[test]
    fn the_strip_gives_its_facts_up_from_the_tail_of_its_own_list() {
        let mut counted = Buffer::empty(band(47, 1));

        paint(&frame(), Shape::Strip, band(47, 1), &mut counted, &PALETTE);

        assert_eq!(
            glyphs(&counted, 0).trim_end(),
            "ctx 48.2k · model claude-sonnet-4-5… · +10 more",
            "the branch and the sandbox posture went, and the count that fits says so"
        );

        let mut facts_win = Buffer::empty(band(60, 1));

        paint(
            &frame(),
            Shape::Strip,
            band(60, 1),
            &mut facts_win,
            &PALETTE,
        );

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

        paint(&one, Shape::Strip, band(40, 1), &mut buffer, &PALETTE);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            "sandbox bwrap",
            "the one fact the frame knows is drawn, and no count stands where nothing went"
        );

        let none = split(&Facts::default(), band(40, 24));

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

        paint(&elsewhere, Shape::Strip, band(40, 1), &mut buffer, &PALETTE);

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
            paint(&frame(), shape, area, &mut buffer, &PALETTE);
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

    /// The value a row draws beside `label`, read off the painted row: the field skipped and the
    /// padding after the value dropped.
    fn drawn(row: &str, label: &str) -> Option<String> {
        let rest = row.split_once(label)?.1;
        Some(
            rest.trim_start_matches(' ')
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

            paint(&facts, shape, area, &mut buffer, &PALETTE);
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

        paint(&frame(), Shape::Strip, band(220, 1), &mut buffer, &PALETTE);

        assert_eq!(
            glyphs(&buffer, 0).trim_end(),
            format!("ctx 48.2k · model {CUT} · sandbox bwrap · branch {BRANCH} · +8 more"),
            "one row, the strip's own four facts in its own order, a gap between two of \
             them, and the count of the eight it does not carry even with the room to"
        );

        let mut narrower = Buffer::empty(band(88, 1));

        paint(&frame(), Shape::Strip, band(88, 1), &mut narrower, &PALETTE);

        assert_eq!(
            glyphs(&narrower, 0).trim_end(),
            format!("ctx 48.2k · model {CUT} · sandbox bwrap · branch {BRANCH}"),
            "eighty-eight columns hold the four facts and no count: the count's own \
             eleven columns are the ones this row does not have"
        );
    }

    #[test]
    fn a_fact_is_drawn_in_the_words_the_rail_reads_it_in() {
        let facts: Vec<(&str, String)> = known(&frame(), Shape::Ledger)
            .into_iter()
            .map(|fact| (fact.kind.label(), fact.value.text().to_owned()))
            .collect();

        assert_eq!(
            facts,
            [
                ("wire", String::from("anthropic")),
                ("model", String::from(CUT)),
                ("think", String::from("high")),
                ("turns", String::from("3 / 250")),
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
    fn a_ledger_row_sets_its_label_dim_and_its_value_plain_on_one_hanging_edge() {
        let facts = Facts {
            model: Some(String::from("claude-sonnet")),
            think: Some(String::from("high")),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 2));

        paint(
            &facts,
            Shape::Ledger,
            band(COLUMNS, 2),
            &mut buffer,
            &PALETTE,
        );

        assert_eq!(
            glyphs(&buffer, 0),
            "model              claude-sonnet",
            "the label in its ten-column field and the value against the rail's last column"
        );
        assert_eq!(
            glyphs(&buffer, 1),
            "think                       high",
            "and a value of another width ends in that same column: one hanging edge, and \
             no run of leaders out to it"
        );
        assert_eq!(
            inks(&buffer, 0, &PALETTE),
            [(Ink::Dim, 10), (Ink::Plain, 22)],
            "the field is `dim` and the value is the fact, so it is `plain`"
        );
    }

    #[test]
    fn no_width_of_rail_draws_a_run_of_leaders() {
        for width in 20..=200u16 {
            let shape = Shape::of(width);
            let area = match shape {
                Shape::Ledger => band(COLUMNS, 40),
                Shape::Strip => band(width, 1),
            };
            let mut buffer = Buffer::empty(area);

            paint(&frame(), shape, area, &mut buffer, &PALETTE);

            let screen: Vec<String> = (0..area.height).map(|y| glyphs(&buffer, y)).collect();
            assert!(
                !screen
                    .iter()
                    .any(|row| row.contains("..") || row.contains(". ")),
                "at {width} columns a row reads {screen:?}: the space between a label and its \
                 value is blank, and no leader run stands in it"
            );
        }
    }

    #[test]
    fn a_window_is_drawn_as_a_meter_beside_the_size_it_is_a_share_of() {
        let facts = Facts {
            turns: Some((3, 250)),
            ctx: Some(Ctx {
                size: 48_200,
                window: Some(200_000),
            }),
            cache: Some(88),
            ..Facts::default()
        };
        let mut buffer = Buffer::empty(band(COLUMNS, 3));

        paint(
            &facts,
            Shape::Ledger,
            band(COLUMNS, 3),
            &mut buffer,
            &PALETTE,
        );

        let rows: Vec<String> = (0..3).map(|y| glyphs(&buffer, y)).collect();
        assert_eq!(
            rows,
            [
                "turns                   3 /  250",
                "ctx          █░░░░░ 48.2k / 200k",
                "cache                        88%",
            ],
            "a quarter of the window is one cell of the meter and the size is the number beside \
             it, and the hit rate is no meter at all"
        );
        assert_eq!(
            column_of(&rows[0], '/'),
            column_of(&rows[1], '/'),
            "the two halves of a pair line up: every slash in the block lands in one column"
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
    fn the_rail_gives_facts_up_in_the_sacrifice_order_and_the_sandbox_never_goes() {
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
