use std::path::Path;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::records::SessionDocument;
use sindri_core::run::State as RunState;

use crate::ink::Ink;
use crate::ledger::{
    Facts, Halves, Kind, METER, Shape, Value, head, hit_rate, more, pair, tail, tokens,
};
use crate::palette::Palette;
use crate::pen;
use crate::rail::{hairline, hanging, label_row, value_room};

/// A band that cannot hold one gets one anyway.
const MIN_COLUMN: u16 = 40;

/// Not the rail's gap, which separates two facts on one row: this one sits between columns.
const COLUMN_GAP: u16 = 2;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Block {
    label: &'static str,
    rows: Vec<Fact>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fact {
    /// `None` when the fact is a name and no more.
    label: Option<&'static str>,
    cut: Cut,
    value: Value,
}

/// How a narrow column takes a value: whole or not at all, or cut at the end the fact is read
/// from and marked where the cut lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut {
    Word,
    /// Cut at its head, marked with `…/`: the tail of a path is what a reader is looking for.
    Path,
    /// Cut at its tail, marked with `…`: the head of a served name is what a reader is looking for.
    Name,
}

impl Cut {
    /// A word too wide for its room is not drawn in part, and a path or a name cut to nothing
    /// says nothing.
    fn shorten(self, text: &str, room: usize) -> Option<String> {
        let cut = match self {
            Cut::Word => text.to_owned(),
            Cut::Path => tail(text, room),
            Cut::Name => head(text, room),
        };
        (!cut.is_empty() && usize::from(pen::cells(&cut)) <= room).then_some(cut)
    }
}

impl Fact {
    fn new(label: Option<&'static str>, cut: Cut, value: Value) -> Fact {
        Fact { label, cut, value }
    }

    fn word(label: &'static str, text: impl Into<String>) -> Fact {
        Fact::new(Some(label), Cut::Word, Value::new(text))
    }

    /// A fact whose value is already spelled, meter and ink and all: the size of a run is the one
    /// fact the rail spells and the screen only widens.
    fn valued(label: &'static str, value: Value) -> Fact {
        Fact::new(Some(label), Cut::Word, value)
    }

    fn path(label: &'static str, path: &Path) -> Fact {
        Fact::new(Some(label), Cut::Path, Value::new(path.to_string_lossy()))
    }

    fn name(label: &'static str, name: impl Into<String>) -> Fact {
        Fact::new(Some(label), Cut::Name, Value::new(name))
    }

    fn bare(name: impl Into<String>) -> Fact {
        Fact::new(None, Cut::Name, Value::new(name))
    }

    fn drawn(&self, width: u16) -> Option<Drawn> {
        let room = match self.label {
            None => usize::from(width),
            Some(_) => value_room(width),
        };
        let room = match self.value.meter() {
            None => room,
            Some(_) => room.checked_sub(METER + 1)?,
        };
        Some(Drawn {
            label: self.label.map(String::from),
            value: self
                .value
                .respelled(self.cut.shorten(self.value.text(), room)?),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Drawn {
    label: Option<String>,
    value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Placed {
    pub label: &'static str,
    /// Whether a rule stands over this block, which is what a block below the first in its column
    /// pays for.
    pub ruled: bool,
    pub rows: Vec<Drawn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Column {
    pub x: u16,
    pub width: u16,
    pub blocks: Vec<Placed>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Screen {
    pub columns: Vec<Column>,
    /// Counted by its last column.
    pub went: usize,
}

impl Stats {
    /// A block the run knows nothing about is not a block: a frame handed no fact and no record
    /// has nothing to draw, which is the title and no row.
    pub fn of(facts: &Facts, document: &SessionDocument, skills: &[String]) -> Stats {
        let mut run = Vec::new();
        if let Some(wire) = &facts.wire {
            run.push(Fact::word(Kind::Wire.label(), wire.clone()));
        }
        if let Some(model) = &facts.model {
            run.push(Fact::name(Kind::Model.label(), model.clone()));
        }
        if let Some(route) = document.route {
            run.push(Fact::word("route", route.word()));
        }
        if let Some(think) = &facts.think {
            run.push(Fact::word(Kind::Think.label(), think.clone()));
        }

        let mut meters = Vec::new();
        if let Some((turns, cap)) = facts.turns {
            meters.push(Fact::word(Kind::Turns.label(), pair(turns, cap)));
        }
        if let Some(ctx) = &facts.ctx {
            meters.push(Fact::valued(Kind::Ctx.label(), ctx.spelled(Shape::Ledger)));
        }
        if let Some(cache) = facts.cache {
            meters.push(Fact::word(Kind::Cache.label(), hit_rate(cache)));
        }
        aligned(&mut meters);

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
                spend.push(Fact::word(label, tokens(count)));
            }
        }

        let mut place = Vec::new();
        if let Some(workspace) = &facts.workspace {
            place.push(Fact::path(Kind::Workspace.label(), workspace));
        }
        if let Some(branch) = &facts.branch {
            place.push(Fact::word(Kind::Branch.label(), branch.clone()));
        }
        if let Some(commit) = &document.commit {
            place.push(Fact::word("commit", commit.clone()));
        }

        let moving = match document.stopped {
            RunState::Running => Ink::Accent,
            RunState::Stopped(_) => Ink::Plain,
        };
        let mut stop = vec![Fact::valued(
            "stopped",
            Value::new(document.stopped.word()).inked(moving),
        )];
        if let Some(reason) = document.stopped.stop() {
            stop.push(Fact::word("exit", reason.exit_code().to_string()));
        }
        stop.push(Fact::word("start", document.start.clone()));
        if let Some(end) = &document.end {
            stop.push(Fact::word("end", end.clone()));
        }

        let confine = facts
            .sandbox
            .map(|enforcement| Fact::word(Kind::Sandbox.label(), enforcement.word()))
            .into_iter()
            .collect();

        let names: Vec<Fact> = skills.iter().map(Fact::bare).collect();

        let mut session = Vec::new();
        if let Some(guid) = &facts.session {
            session.push(Fact::word(Kind::Session.label(), guid.clone()));
        }
        if let Some(temp) = &facts.temp {
            session.push(Fact::path(Kind::Temp.label(), temp));
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
            .map(|(label, rows)| Block { label, rows })
            .collect(),
        }
    }

    pub fn paint(&self, area: Rect, buffer: &mut Buffer, palette: &Palette) {
        let screen = self.screen(area);
        for column in &screen.columns {
            let mut y = area.y;
            for block in &column.blocks {
                if block.ruled {
                    let rule = Rect {
                        x: column.x,
                        y,
                        width: column.width,
                        height: 1,
                    };
                    hairline(buffer, rule, palette);
                    y = y.saturating_add(1);
                }
                pen::run(buffer, column.x, y, block.label, Ink::Dim, palette);
                y = y.saturating_add(1);
                for row in &block.rows {
                    drawn_row(buffer, column, y, row, palette);
                    y = y.saturating_add(1);
                }
            }
        }
        let Some(column) = screen.columns.last() else {
            return;
        };
        if screen.went == 0 || area.height == 0 {
            return;
        }
        pen::run(
            buffer,
            column.x,
            area.bottom() - 1,
            &more(screen.went),
            Ink::Muted,
            palette,
        );
    }

    #[cfg(test)]
    fn blocks(&self) -> impl ExactSizeIterator<Item = (&'static str, usize)> {
        self.blocks
            .iter()
            .map(|block| (block.label, block.rows.len()))
    }

    /// The columns are there for the room they add, so a wider band never narrows one; and a
    /// block's facts are whole, so a band that gives one up gives up every block after it, and
    /// the last column counts the tail of the screen rather than a hole in the middle of it.
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

    fn flow(&self, area: Rect, count: usize, width: u16, rows: u16) -> Screen {
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
        for (index, block) in self.blocks.iter().enumerate() {
            let drawn: Vec<Drawn> = block
                .rows
                .iter()
                .filter_map(|row| row.drawn(width))
                .collect();
            if drawn.is_empty() {
                went += block.rows.len();
                continue;
            }
            let height = u16::try_from(drawn.len())
                .unwrap_or(u16::MAX)
                .saturating_add(1);
            // A block below the first in its column is separated from it by a rule, and the rule
            // takes a row of the column it stands in.
            let needs = |column: &Column| height + u16::from(!column.blocks.is_empty());
            let Some(place) = (at..count).find(|column| room[*column] >= needs(&columns[*column]))
            else {
                went += self.blocks[index..]
                    .iter()
                    .map(|block| block.rows.len())
                    .sum::<usize>();
                break;
            };
            went += block.rows.len() - drawn.len();
            room[place] -= needs(&columns[place]);
            at = place;
            let ruled = !columns[place].blocks.is_empty();
            columns[place].blocks.push(Placed {
                label: block.label,
                ruled,
                rows: drawn,
            });
        }
        columns.retain(|column| !column.blocks.is_empty());
        Screen { columns, went }
    }

    fn columns(&self, width: u16) -> usize {
        let holds = (width.saturating_add(COLUMN_GAP)) / (MIN_COLUMN + COLUMN_GAP);
        usize::from(holds.max(1)).min(self.blocks.len())
    }
}

/// Spells a block's pairs in one pair of half widths, so every slash in the block lands in one
/// column and the digits either side of it line up.
fn aligned(rows: &mut [Fact]) {
    let shared = Halves::of(rows.iter().map(|row| row.value.text()));
    for row in rows.iter_mut() {
        row.value.align(shared);
    }
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

fn drawn_row(buffer: &mut Buffer, column: &Column, y: u16, row: &Drawn, palette: &Palette) {
    let area = Rect {
        x: column.x,
        y,
        width: column.width,
        height: 1,
    };
    let x = match &row.label {
        Some(label) => label_row(buffer, area, label, row.value.columns(), palette),
        None => hanging(area, row.value.columns()),
    };
    row.value.draw(buffer, x, y, palette);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sindri_core::policy::{Enforcement, PolicyRecord};
    use sindri_core::provider::Route;
    use sindri_core::records::{Dials, SessionDocument, Usage};
    use sindri_core::run::{State as RunState, Stop};
    use sindri_core::wire::Wire;

    use super::*;
    use crate::ledger::Ctx;
    use crate::readback::{column_of, inks};

    const PALETTE: Palette = Palette::TRUECOLOR;

    const GUID: &str = "9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61";

    const WORKSPACE: &str = "/home/glemsom/src/sindri";

    const SERVED: &str = "claude-sonnet-4-5-20250929";

    const GATEWAY: &str = "anthropic.claude-3-5-sonnet-20241022-v2:0";

    fn band(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    fn temp() -> PathBuf {
        PathBuf::from(format!(
            "/home/glemsom/.local/state/sindri/sessions/{GUID}/tmp"
        ))
    }

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

    fn read(screen: &Screen) -> Vec<String> {
        screen
            .columns
            .iter()
            .flat_map(|column| column.blocks.iter())
            .flat_map(|block| {
                std::iter::once(block.label.to_owned()).chain(block.rows.iter().map(|row| {
                    let words: Vec<&str> = row.value.text().split_whitespace().collect();
                    match &row.label {
                        None => words.join(" "),
                        Some(label) => format!("{label} {}", words.join(" ")),
                    }
                }))
            })
            .collect()
    }

    /// The body of the screen as the frame paints it: the ground under every cell, then the rows.
    fn painted(stats: &Stats, area: Rect) -> Buffer {
        let mut buffer = Buffer::empty(area);
        buffer.set_style(area, PALETTE.ground(None));
        stats.paint(area, &mut buffer, &PALETTE);
        buffer
    }

    /// The row a labelled fact of `value` is drawn on in a column `width` wide: its label in the
    /// field every label is drawn in, and the value in the room the row leaves it.
    fn row_of(label: &str, value: &str, width: u16) -> String {
        format!(
            "{label:<field$}{value:>room$}",
            field = usize::from(crate::rail::FIELD),
            room = value_room(width)
        )
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
                "turns 6 / 250",
                "ctx 48.2k / 200k",
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
                "temp …/.local/state/sindri/sessions/9f2c7a41-6a1e-4b3d-8f5c-0d2e7a4b9c61/tmp",
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

        let facts = Facts {
            wire: Some(String::from("anthropic")),
            model: Some(String::from("claude-sonnet")),
            think: Some(String::from("off")),
            workspace: Some(PathBuf::from(WORKSPACE)),
            sandbox: Some(Enforcement::Bubblewrap),
            ..Facts::default()
        };
        let stats = Stats::of(&facts, &document, &[]);
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
    fn the_screen_draws_the_facts_the_ledger_holds_and_never_reads_one_off_the_record_again() {
        let elsewhere = Facts {
            wire: Some(String::from("openai")),
            model: Some(String::from("gpt-5")),
            think: Some(String::from("off")),
            branch: Some(String::from("main")),
            ..facts()
        };

        let rows = read(&Stats::of(&elsewhere, &document(), &[]).screen(band(81, 40)));

        assert!(
            rows.contains(&String::from("wire openai")),
            "the record says anthropic and the ledger says openai, and the screen draws what \
             the ledger holds: {rows:?}"
        );
        assert!(rows.contains(&String::from("model gpt-5")), "{rows:?}");
        assert!(rows.contains(&String::from("think off")), "{rows:?}");
        assert!(rows.contains(&String::from("branch main")), "{rows:?}");

        let blank = Facts {
            model: None,
            ..elsewhere
        };
        let rows = read(&Stats::of(&blank, &document(), &[]).screen(band(81, 40)));

        assert!(
            !rows.iter().any(|row| row.starts_with("model ")),
            "a served name the ledger does not hold has no row, whatever the record names: \
             {rows:?}"
        );
    }

    #[test]
    fn a_narrow_column_cuts_a_path_at_its_head_and_a_name_at_its_tail_and_draws_no_other_part() {
        let stats = Stats::of(
            &Facts {
                model: Some(String::from(GATEWAY)),
                ..facts()
            },
            &document(),
            &[String::from("a-skill-name-that-runs-past-any-column-here")],
        );

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
                        row.value.columns()
                            + match row.label {
                                None => 0,
                                Some(_) => usize::from(column.width) - value_room(column.width),
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
        stats.paint(area, &mut buffer, &PALETTE);
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
            let holds = (width + COLUMN_GAP) / (MIN_COLUMN + COLUMN_GAP);

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
    fn a_row_sets_its_label_dim_and_its_value_plain_on_the_columns_hanging_edge() {
        let stats = Stats::of(&facts(), &document(), &[]);
        let area = band(81, 40);

        let buffer = painted(&stats, area);

        assert_eq!(
            glyphs(&buffer, 1),
            row_of("wire", "anthropic", 81),
            "the label in its field and the value against the column's last column"
        );
        assert_eq!(
            glyphs(&buffer, 2),
            row_of("model", SERVED, 81),
            "and a value of another width ends in that same column"
        );
        assert_eq!(
            inks(&buffer, 1, &PALETTE),
            [(Ink::Dim, 10), (Ink::Plain, 71)],
            "the field is `dim` and the value is the fact, so it is `plain`"
        );
    }

    #[test]
    fn a_hairline_separates_two_fact_groups() {
        let stats = Stats::of(&facts(), &document(), &[]);
        let area = band(81, 40);

        let buffer = painted(&stats, area);

        let rows: Vec<String> = (0..area.height).map(|y| glyphs(&buffer, y)).collect();
        let rule = "─".repeat(usize::from(area.width));
        assert_eq!(
            rows[5], rule,
            "the run block is closed by a rule across the column: {rows:#?}"
        );
        assert_eq!(
            rows[4],
            row_of("think", "high", 81),
            "the fact above the rule is the last of the run block"
        );
        assert_eq!(
            rows[6].trim_end(),
            "meters",
            "and the rule stands between that fact and the name of the group under it"
        );
        assert_eq!(
            inks(&buffer, 5, &PALETTE),
            [(Ink::Dim, 81)],
            "the rule is drawn in the ink a group's edge is read in, and nothing else is"
        );
    }

    #[test]
    fn a_share_of_a_window_is_a_meter_and_a_hit_rate_is_no_meter_at_all() {
        let stats = Stats::of(&facts(), &document(), &[]);
        let area = band(81, 40);

        let buffer = painted(&stats, area);

        assert_eq!(
            glyphs(&buffer, 8),
            row_of("ctx", "█░░░░░ 48.2k / 200k", 81),
            "a quarter of the window fills one of the meter's six cells, beside the size it is \
             a share of"
        );
        assert_eq!(
            glyphs(&buffer, 9),
            row_of("cache", "88%", 81),
            "a hit rate has no ceiling to be a share of, so it is the number and no meter"
        );
        assert_eq!(
            column_of(&glyphs(&buffer, 7), '/'),
            column_of(&glyphs(&buffer, 8), '/'),
            "and the turns above the size line their slash up with it, so the digits either \
             side are a column"
        );
    }

    #[test]
    fn a_name_with_no_label_of_its_own_hangs_on_the_same_edge() {
        let names = [String::from("bash"), String::from("records")];
        let stats = Stats::of(&facts(), &document(), &names);
        let area = band(81, 40);

        let buffer = painted(&stats, area);

        let rows: Vec<String> = (0..area.height).map(|y| glyphs(&buffer, y)).collect();
        let names: Vec<&String> = rows
            .iter()
            .filter(|row| row.trim() == "bash" || row.trim() == "records")
            .collect();
        assert_eq!(
            names,
            [&row_of("", "bash", 81), &row_of("", "records", 81)],
            "a value is a value however its row is labelled, so a loaded name reads in the \
             column its siblings do: {rows:#?}"
        );
    }

    #[test]
    fn the_fact_that_says_the_run_is_moving_wears_the_accent() {
        let moving = SessionDocument {
            stopped: RunState::Running,
            end: None,
            ..document()
        };
        let area = band(81, 40);

        for (stopped, ink) in [
            (RunState::Running, Ink::Accent),
            (RunState::Stopped(Stop::Final), Ink::Plain),
        ] {
            let document = SessionDocument {
                stopped,
                ..moving.clone()
            };
            let stats = Stats::of(&facts(), &document, &[]);
            let buffer = painted(&stats, area);

            let row = (0..area.height)
                .find(|y| glyphs(&buffer, *y).trim_end().starts_with("stopped"))
                .expect("the screen draws the state the run stopped in");
            let at = u16::try_from(81 - "running".len()).expect("a column of the row");
            assert_eq!(
                Ink::of(
                    buffer.cell((at, row)).expect("a cell of the state"),
                    &PALETTE
                ),
                Some(ink),
                "a run still going is the one fact on the screen that says something is \
                 happening, so it is read in the accent and not in the plain ink"
            );
        }
    }

    #[test]
    fn a_screen_handed_nothing_draws_no_row() {
        let area = band(120, 40);
        let stats = Stats::default();
        let mut buffer = Buffer::empty(area);

        stats.paint(area, &mut buffer, &PALETTE);

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
