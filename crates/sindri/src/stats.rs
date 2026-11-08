//! The stats screen: the facts the rail gives up, drawn whole on the frame's whole band.
//!
//! The rail is thirty-two columns wide and has to choose, so the route the wire answered on,
//! what the run has spent in tokens, when it started, when and why it stopped, the whole GUID,
//! the whole temp and the whole workspace have no home on it. This screen is that home, opened
//! by `/stats` and closed by `Esc`: a ledger of the same facts, grouped into the rail's blocks
//! and then into the blocks it cannot hold, laid out in as many columns as the band has room
//! for and never split across two.
//!
//! It reads what the frame was handed and nothing else — the rail's `Facts` for the meters and
//! the session, the record for everything the run wrote down, and the loaded-skill set for the
//! names — so the screen and the rail beside it cannot disagree about one run. A fact the run
//! knows nothing about is not drawn: no route for a wire that serves one endpoint, no cache
//! before the wire reported one, no end or exit while the run is going, no branch or commit
//! off a checkout.
//!
//! The layout is a pure function of the blocks and the band, so what a band of a given size
//! yields is read by a test without a buffer: [`Stats::screen`] places the blocks, counts what
//! did not fit, and [`Stats::paint`] writes the cells that placement describes.
//!
//! Why the screen exists at all, and why it is `/stats` and not `/status`, is ADR-0019's; the
//! acceptance criteria are the ticket's. What this module holds is the drawing of them.

use std::path::Path;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::records::SessionDocument;

use crate::ink::Ink;
use crate::rail::{Facts, head, put, tail, tokens};

/// The columns a column of this screen is never narrower than: a label field, its leaders and
/// enough value to read is forty columns, and a band that cannot hold one gets one anyway.
pub const MIN_COLUMN: u16 = 40;

/// The columns a row's label is drawn in, the rail's own field, so the two read the same.
const FIELD: u16 = 10;

/// The fewest leader dots a row is drawn with: a label butted against its value is not a
/// ledger row, and a value that leaves no room for one is not drawn.
const LEADER_MIN: usize = 1;

/// The glyph a row's leaders are drawn with.
const LEADER: char = '.';

/// What lies between two columns: one gap, so the value against one column's edge does not run
/// into the label at the next one's.
const GAP: u16 = 2;

/// The stats a frame was handed: the rail's facts, the run's record and the names it has
/// loaded, grouped into the blocks the screen draws in the order it reads them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// The blocks, in the order the screen reads them, each with the rows the run knows.
    blocks: Vec<Rows>,
}

/// One block of the screen: the word it is headed by and the rows under it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Rows {
    /// The heading, which is the ticket's own name for the block.
    label: &'static str,
    /// The rows, in the order the block reads them.
    rows: Vec<Row>,
}

/// One row of a block: a label and the value beside it, or a name standing on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    /// The label the row is read by, and nothing when the row is a name and no more.
    label: Option<&'static str>,
    /// The value, before the column it lands in cuts it.
    value: Value,
}

/// A value's own cut rule, which is the whole of what a column may do to it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    /// A word the run wrote down whole: drawn whole or not at all.
    Word(String),
    /// A path, which a narrow column cuts at its head and marks with `…/`.
    Path(String),
    /// A name, which a narrow column cuts at its tail and marks with `…`.
    Name(String),
}

impl Value {
    /// The value as a column `columns` wide draws it, or nothing when it has no rule that
    /// leaves anything to read: a word too wide for its room is not drawn in part, and a path
    /// or a name cut to nothing says nothing.
    fn drawn(&self, columns: usize) -> Option<String> {
        let text = match self {
            Value::Word(word) => word.clone(),
            Value::Path(path) => tail(path, columns),
            Value::Name(name) => head(name, columns),
        };
        (!text.is_empty() && text.chars().count() <= columns).then_some(text)
    }
}

impl Row {
    /// A row of a fixed word: a wire, a turn count, a spend, an instant.
    fn word(label: &'static str, value: impl Into<String>) -> Row {
        Row {
            label: Some(label),
            value: Value::Word(value.into()),
        }
    }

    /// A row of a path: the workspace the run stands in, and the temp it was given.
    fn path(label: &'static str, path: &Path) -> Row {
        Row {
            label: Some(label),
            value: Value::Path(path.to_string_lossy().into_owned()),
        }
    }

    /// A row of a name: the model the run was served, and a skill it has loaded.
    fn name(label: &'static str, name: impl Into<String>) -> Row {
        Row {
            label: Some(label),
            value: Value::Name(name.into()),
        }
    }

    /// A row of a name and no label: a skill the run has loaded, read by its own word from
    /// the column's left edge rather than by a field it would only repeat.
    fn bare(name: impl Into<String>) -> Row {
        Row {
            label: None,
            value: Value::Name(name.into()),
        }
    }

    /// The row as a column `width` columns wide draws it, or nothing when its value has no room
    /// there. A row with no label is a name standing on its own, so its value takes the whole
    /// column rather than starting after a field it does not use.
    fn drawn(&self, width: u16) -> Option<Drawn> {
        let room = match self.label {
            None => usize::from(width),
            Some(_) => usize::from(width).saturating_sub(usize::from(FIELD) + LEADER_MIN),
        };
        Some(Drawn {
            label: self.label.map(String::from),
            value: self.value.drawn(room)?,
        })
    }
}

/// One row of the screen as it is drawn: the label in its field and the value, already cut to
/// the column it lands in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Drawn {
    /// The label, and nothing when the row is a name on its own.
    label: Option<String>,
    /// The value the row is read by.
    value: String,
}

/// One block as a column holds it: the heading and the rows under it, whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Placed {
    /// The word the block is headed by.
    pub label: &'static str,
    /// Its rows, in order.
    pub rows: Vec<Drawn>,
}

/// One column of the screen: where it starts, how wide it is, and the blocks it holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Column {
    /// The column's left-hand edge.
    pub x: u16,
    /// The columns it may draw in.
    pub width: u16,
    /// The blocks it holds, top to bottom, whole.
    pub blocks: Vec<Placed>,
}

/// The screen a band of a given size is drawn as: the columns that hold anything, and how many
/// rows the band could not hold at all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Screen {
    /// The columns holding a block, left to right.
    pub columns: Vec<Column>,
    /// The rows the band could not hold, which its last column counts.
    pub went: usize,
}

impl Stats {
    /// The screen a frame's three values come to: the rail's facts for the meters and the
    /// session, the record for everything the run wrote down, and the names loaded so far.
    ///
    /// A block the run knows nothing about is not a block: a frame handed no fact and no record
    /// has nothing to draw, which is the title and no row.
    pub fn of(facts: &Facts, document: &SessionDocument, skills: &[String]) -> Stats {
        let mut run = vec![
            Row::word("wire", document.wire.word()),
            Row::name("model", document.model.clone()),
        ];
        if let Some(route) = document.route {
            run.push(Row::word("route", route.word()));
        }
        run.push(Row::word(
            "think",
            if document.thinking { "high" } else { "off" },
        ));

        let mut meters = Vec::new();
        if let Some((turns, cap)) = facts.turns {
            meters.push(Row::word("turns", format!("{turns}/{cap}")));
        }
        if let Some(ctx) = &facts.ctx {
            meters.push(Row::word("ctx", ctx.said()));
        }
        if let Some(cache) = facts.cache {
            meters.push(Row::word("cache", format!("{cache}%")));
        }

        let spent = document.usage;
        let mut spend = Vec::new();
        for (label, count) in [
            ("input", spent.input),
            ("output", spent.output),
            ("total", spent.total),
            ("cached", spent.cached_input),
            ("reasoning", spent.reasoning),
        ] {
            if let Some(count) = count {
                spend.push(Row::word(label, tokens(count)));
            }
        }

        let mut place = vec![Row::path("workspace", &document.workspace)];
        if let Some(branch) = &document.branch {
            place.push(Row::word("branch", branch.clone()));
        }
        if let Some(commit) = &document.commit {
            place.push(Row::word("commit", commit.clone()));
        }

        let mut stop = vec![Row::word("stopped", document.stopped.word())];
        if let Some(reason) = document.stopped.stop() {
            stop.push(Row::word("exit", reason.exit_code().to_string()));
        }
        stop.push(Row::word("start", document.start.clone()));
        if let Some(end) = &document.end {
            stop.push(Row::word("end", end.clone()));
        }

        let confine = vec![Row::word("sandbox", document.enforcement.word())];

        let names: Vec<Row> = skills.iter().map(Row::bare).collect();

        let mut session = Vec::new();
        if let Some(guid) = &facts.session {
            session.push(Row::word("session", guid.clone()));
        }
        if let Some(temp) = &facts.temp {
            session.push(Row::path("temp", temp));
        }

        Stats {
            blocks: [
                ("run", run),
                ("meters", meters),
                ("spend", spend),
                ("place", place),
                ("stop", stop),
                ("confine", confine),
                ("skills", names),
                ("session", session),
            ]
            .into_iter()
            .filter(|(_, rows)| !rows.is_empty())
            .map(|(label, rows)| Rows { label, rows })
            .collect(),
        }
    }

    /// The cells this screen is drawn as: every block a column of `area` holds, whole, and the
    /// count of the rows the band could not hold.
    pub fn paint(&self, area: Rect, buffer: &mut Buffer) {
        let screen = self.screen(area);
        for column in &screen.columns {
            let mut y = area.y;
            for block in &column.blocks {
                put(buffer, column.x, y, block.label, Ink::Dim);
                y = y.saturating_add(1);
                for row in &block.rows {
                    drawn_row(buffer, column, y, row);
                    y = y.saturating_add(1);
                }
            }
        }
        let (true, Some(column), true) = (screen.went > 0, screen.columns.last(), area.height > 0)
        else {
            return;
        };
        let said = format!("+{} more", screen.went);
        put(buffer, column.x, area.bottom() - 1, &said, Ink::Muted);
    }

    /// The blocks this screen holds, in the order the screen reads them.
    fn blocks(&self) -> impl ExactSizeIterator<Item = (&'static str, usize)> {
        self.blocks
            .iter()
            .map(|block| (block.label, block.rows.len()))
    }

    /// The screen a band of `area` is drawn as: the blocks flowed into the fewest columns that
    /// hold them all, and never more than the band has room for.
    ///
    /// A band that holds every row in one column draws one, however wide it is: the columns
    /// are there for the room they add, so a wider band narrows nothing. Only a band too small
    /// for the rows it was handed takes more columns, and only then does it count what the last
    /// of them could not hold — in the row above the count's own, because the count is a row of
    /// the band like any other.
    fn screen(&self, area: Rect) -> Screen {
        if area.width == 0 || area.height == 0 || self.blocks.is_empty() {
            return Screen::default();
        }
        let holds = self.columns(area.width);
        for count in 1..=holds {
            let screen = self.flow(area, count, column_width(area.width, count), area.height);
            if screen.went == 0 {
                return screen;
            }
        }
        let width = column_width(area.width, holds);
        match area.height {
            0 | 1 => self.flow(area, holds, width, area.height),
            height => self.flow(area, holds, width, height - 1),
        }
    }

    /// The blocks flowed into `count` columns `width` wide, with `rows` rows of room each.
    fn flow(&self, area: Rect, count: usize, width: u16, rows: u16) -> Screen {
        let mut columns: Vec<Column> = (0..count)
            .map(|at| Column {
                x: area
                    .x
                    .saturating_add((at as u16).saturating_mul(width.saturating_add(GAP))),
                width,
                blocks: Vec::new(),
            })
            .collect();
        let mut room = vec![rows; count];
        let mut went = 0;
        let mut at = 0;
        for block in &self.blocks {
            let drawn: Vec<Drawn> = block
                .rows
                .iter()
                .filter_map(|row| row.drawn(width))
                .collect();
            went += block.rows.len() - drawn.len();
            let height = u16::try_from(drawn.len())
                .unwrap_or(u16::MAX)
                .saturating_add(1);
            let Some(place) = (at..count).find(|column| room[*column] >= height) else {
                went += drawn.len();
                continue;
            };
            room[place] -= height;
            at = place;
            columns[place].blocks.push(Placed {
                label: block.label,
                rows: drawn,
            });
        }
        columns.retain(|column| !column.blocks.is_empty());
        Screen { columns, went }
    }

    /// The most columns a band `width` wide has room for: as many as hold a column of the least
    /// width this screen draws one at, and never more than there are blocks to fill them.
    fn columns(&self, width: u16) -> usize {
        let holds = (width.saturating_add(GAP)) / (MIN_COLUMN + GAP);
        usize::from(holds.max(1)).min(self.blocks.len())
    }
}

/// The columns each of `count` columns of a band `width` wide is drawn in: the band's width
/// less the gaps between them, shared out whole and never narrower than nothing.
fn column_width(width: u16, count: usize) -> u16 {
    let Some(count) = u16::try_from(count).ok().filter(|count| *count > 0) else {
        return 0;
    };
    width
        .saturating_sub(GAP.saturating_mul(count - 1))
        .checked_div(count)
        .unwrap_or(0)
}

/// Writes one row into `column` at `y`: a name standing on its own from the column's left
/// edge, or a label in the field, its leaders out to the value, and the value against the
/// column's right-hand edge.
fn drawn_row(buffer: &mut Buffer, column: &Column, y: u16, row: &Drawn) {
    let Some(label) = &row.label else {
        put(buffer, column.x, y, &row.value, Ink::Muted);
        return;
    };
    let value = row.value.chars().count();
    let start = usize::from(column.width).saturating_sub(value);
    let field = format!("{label:<width$}", width = usize::from(FIELD));
    put(buffer, column.x, y, &field, Ink::Dim);
    let leaders: String =
        std::iter::repeat_n(LEADER, start.saturating_sub(usize::from(FIELD))).collect();
    put(buffer, column.x + FIELD, y, &leaders, Ink::Dim);
    let x = column
        .x
        .saturating_add(u16::try_from(start).unwrap_or(column.width));
    put(buffer, x, y, &row.value, Ink::Muted);
}

#[cfg(test)]
mod tests {
    //! The screen's own rules, one test each: every fact the run knows has its row, a fact it
    //! knows nothing about has none, a narrow column cuts what the run cut and nothing else,
    //! and what a band cannot hold is counted rather than drawn in part. The placement is read
    //! as the value it is, so only the count's row is asked of a buffer.

    use std::path::PathBuf;

    use sindri_core::policy::{Enforcement, PolicyRecord};
    use sindri_core::provider::Route;
    use sindri_core::records::{Dials, SessionDocument, Usage};
    use sindri_core::run::{State as RunState, Stop};
    use sindri_core::wire::Wire;

    use super::*;
    use crate::rail::Ctx;

    /// The GUID of the session the screen's run is writing: the whole of it, because the
    /// screen is the one place a reader can see all thirty-six characters.
    const GUID: &str = "9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61";

    /// The directory the run stands in.
    const WORKSPACE: &str = "/home/glemsom/src/sindri";

    /// The served name a real run is given, which the rail and the screen both have to cut.
    const SERVED: &str = "claude-sonnet-4-5-20250929";

    /// A gateway's name, which runs past any column a frame holds.
    const GATEWAY: &str = "anthropic.claude-3-5-sonnet-20241022-v2:0";

    /// A band at the origin: the room a frame hands the screen.
    fn band(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// The session temp of a run, which carries the session's whole GUID.
    fn temp() -> PathBuf {
        PathBuf::from(format!(
            "/home/glemsom/.local/state/sindri/sessions/{GUID}/tmp"
        ))
    }

    /// The facts a rail draws, as a run that has taken six turns holds them.
    fn facts() -> Facts {
        Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from(SERVED)),
            think: Some(String::from("high")),
            turns: Some((6, 250)),
            ctx: Some(Ctx {
                size: 48_200,
                window: Some(200_000),
            }),
            cache: Some(88),
            workspace: Some(PathBuf::from(WORKSPACE)),
            branch: Some(String::from("spec/1-capability")),
            skills: Some(2),
            sandbox: Some(Enforcement::Bubblewrap),
            session: Some(String::from(GUID)),
            temp: Some(temp()),
        }
    }

    /// The record a stopped run wrote, as the core holds it: filled in by hand so the screen's
    /// wiring is read off the run's own fields and not off a real run.
    fn document() -> SessionDocument {
        SessionDocument {
            session: String::from(GUID),
            wire: Wire::Anthropic,
            model: String::from(SERVED),
            enforcement: Enforcement::Bubblewrap,
            policy: PolicyRecord {
                enforcement: Enforcement::Bubblewrap,
                writable: vec![String::from(WORKSPACE)],
                timeout_seconds: 120,
                command_budget_ms: 3_600_000,
                output_budget_bytes: 8 * 1024 * 1024,
                description: String::from("the boundary the model was told about"),
            },
            workspace: PathBuf::from(WORKSPACE),
            branch: Some(String::from("spec/1-capability")),
            commit: Some(String::from("16becb5")),
            dials: Dials {
                max_turns: 250,
                command_budget_ms: 3_600_000,
                output_budget_bytes: 8 * 1024 * 1024,
            },
            start: String::from("2026-01-01T00:00:00Z"),
            end: Some(String::from("2026-01-01T00:04:00Z")),
            turns: 6,
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

    /// The screen a frame draws, as the rows a reader reads it by: each block's heading and
    /// then its rows, a row's label and value as one line.
    fn read(screen: &Screen) -> Vec<String> {
        screen
            .columns
            .iter()
            .flat_map(|column| column.blocks.iter())
            .flat_map(|block| {
                std::iter::once(block.label.to_owned()).chain(block.rows.iter().map(|row| {
                    match &row.label {
                        None => row.value.clone(),
                        Some(label) => format!("{label} {}", row.value),
                    }
                }))
            })
            .collect()
    }

    /// One row of a painted buffer as the glyphs it holds.
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
    fn every_fact_the_run_knows_has_its_row_in_the_order_the_screen_reads_them() {
        let stats = Stats::of(
            &facts(),
            &document(),
            &[String::from("bash"), String::from("records")],
        );

        assert_eq!(
            stats.blocks().collect::<Vec<_>>(),
            vec![
                ("run", 4),
                ("meters", 3),
                ("spend", 5),
                ("place", 3),
                ("stop", 4),
                ("confine", 1),
                ("skills", 2),
                ("session", 2),
            ],
            "the blocks are the rail's own order followed by the ones it cannot hold, each \
             with the rows the run knows"
        );

        let screen = stats.screen(band(81, 40));

        assert_eq!(
            read(&screen),
            vec![
                "run",
                "wire anthropic",
                "model claude-sonnet-4-5-20250929",
                "route chat",
                "think high",
                "meters",
                "turns 6/250",
                "ctx 48.2k/200k 24%",
                "cache 88%",
                "spend",
                "input 48.2k",
                "output 3.1k",
                "total 51.3k",
                "cached 12k",
                "reasoning 900",
                "place",
                "workspace /home/glemsom/src/sindri",
                "branch spec/1-capability",
                "commit 16becb5",
                "stop",
                "stopped final",
                "exit 0",
                "start 2026-01-01T00:00:00Z",
                "end 2026-01-01T00:04:00Z",
                "confine",
                "sandbox bubblewrap",
                "skills",
                "bash",
                "records",
                "session",
                "session 9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61",
                "temp …/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
            ],
            "every fact on the ticket's table has its row, drawn whole, and nothing on the \
             band went"
        );
        assert_eq!(screen.went, 0, "a band this tall holds the screen whole");
        assert_eq!(
            screen.columns.len(),
            1,
            "and one column of it is all it takes"
        );
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
            ..document()
        };

        let stats = Stats::of(&Facts::default(), &document, &[]);
        let screen = stats.screen(band(81, 40));
        let rows = read(&screen);

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
            rows.iter()
                .all(|row| row != "route chat" && row != "cache 88%"),
            "and no block of a fact it knows nothing about is headed on the screen: {rows:?}"
        );
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
        let document = SessionDocument {
            model: String::from(GATEWAY),
            ..document()
        };
        let long = String::from("a-skill-name-that-runs-past-any-column-here");
        let stats = Stats::of(&facts(), &document, &[long]);

        let screen = stats.screen(band(40, 40));
        let rows = read(&screen);

        assert_eq!(
            screen.columns.first().map(|column| column.width),
            Some(40),
            "a band this wide holds one column, and that column is all of it"
        );
        assert!(
            rows.contains(&String::from("model anthropic.claude-3-5-sonnet…")),
            "a served name too long for its column keeps its head and marks the tail: {rows:?}"
        );
        assert!(
            rows.contains(&String::from("a-skill-name-that-runs-past-any-column…")),
            "a loaded name is cut the same way: {rows:?}"
        );
        assert!(
            !rows.iter().any(|row| row.starts_with("session ")),
            "the GUID is thirty-six characters and the column holds twenty-nine, so the row \
             is not drawn in part: {rows:?}"
        );
        for column in &screen.columns {
            for block in &column.blocks {
                for row in &block.rows {
                    assert!(
                        row.value.chars().count()
                            + match row.label {
                                None => 0,
                                Some(_) => usize::from(FIELD) + LEADER_MIN,
                            }
                            <= usize::from(column.width),
                        "every row lands inside its column: {row:?} in {} columns",
                        column.width
                    );
                }
            }
        }
    }

    #[test]
    fn a_block_is_drawn_whole_and_what_the_band_cannot_hold_is_counted_at_its_last_columns_foot() {
        let stats = Stats::of(&facts(), &document(), &[String::from("bash")]);
        let area = band(120, 12);
        let screen = stats.screen(area);

        assert_eq!(
            screen.columns.len(),
            2,
            "a hundred and twenty columns hold two columns of this screen"
        );
        let placed: Vec<(&str, Vec<&str>)> = screen
            .columns
            .iter()
            .map(|column| {
                (
                    "",
                    column
                        .blocks
                        .iter()
                        .map(|block| block.label)
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(
            placed
                .iter()
                .map(|(_, blocks)| blocks.clone())
                .collect::<Vec<_>>(),
            vec![vec!["run", "meters"], vec!["spend", "place"],],
            "the blocks flow into the columns whole, and a block that does not fit moves on \
             rather than being split"
        );
        assert_eq!(
            screen.went, 8,
            "the stop, confine, skills and session rows are what the band could not hold"
        );

        let mut buffer = Buffer::empty(area);
        stats.paint(area, &mut buffer);
        let foot = glyphs(&buffer, area.bottom() - 1);
        assert_eq!(
            foot[61..61 + 8].trim_end(),
            "+8 more",
            "the count sits at the foot of the last column, in the rail's own words: {foot:?}"
        );
    }

    #[test]
    fn a_column_is_never_narrower_than_forty_columns_and_never_runs_past_the_band() {
        let stats = Stats::of(&facts(), &document(), &[String::from("bash")]);

        for width in MIN_COLUMN..=200 {
            let screen = stats.screen(band(width, 40));
            let holds = (width + GAP) / (MIN_COLUMN + GAP);

            assert!(
                screen.columns.len() <= usize::from(holds).min(stats.blocks.len()),
                "a band {width} columns wide is cut into no more columns than it holds a whole \
                 one in"
            );
            for column in &screen.columns {
                assert!(
                    column.width >= MIN_COLUMN,
                    "a column of a {width}-column band is {} wide, and forty is the least this \
                     screen draws one at",
                    column.width
                );
                assert!(
                    column.x + column.width <= width,
                    "the column at {} is {} wide, and a {width}-column band does not hold it",
                    column.x,
                    column.width
                );
            }
        }
    }

    #[test]
    fn a_band_wide_enough_for_one_column_draws_one_however_wide_it_is() {
        let stats = Stats::of(&facts(), &document(), &[String::from("bash")]);

        for width in [84u16, 120, 200, 320] {
            let screen = stats.screen(band(width, 54));

            assert_eq!(
                screen.columns.len(),
                1,
                "a {width}-column band holds the whole screen in one column, so it draws one: \
                 columns are there for the room they add and never to narrow a fact"
            );
            assert_eq!(
                screen.columns[0].width, width,
                "and that one column takes the band it was given"
            );
            assert!(
                read(&screen)
                    .iter()
                    .any(|row| row == "session 9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61"),
                "so the GUID is drawn in all thirty-six characters on a wide band too"
            );
        }
    }

    #[test]
    fn a_screen_handed_nothing_draws_no_row() {
        let area = band(120, 40);
        let stats = Stats::default();
        let mut buffer = Buffer::empty(area);

        stats.paint(area, &mut buffer);

        assert_eq!(
            stats.screen(area),
            Screen::default(),
            "there is nothing to place"
        );
        for y in area.y..area.bottom() {
            assert_eq!(
                glyphs(&buffer, y).trim_end(),
                "",
                "a screen handed no record and no fact draws no row"
            );
        }
    }
}
