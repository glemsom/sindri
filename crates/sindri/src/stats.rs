use std::path::Path;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::records::SessionDocument;

use crate::ink::Ink;
use crate::rail::{Facts, head, label_row, more, put, tail, tokens, value_room};

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
    value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    /// Drawn whole or not at all.
    Word(String),
    /// A narrow column cuts it at its head and marks the cut with `…/`.
    Path(String),
    /// A narrow column cuts it at its tail and marks the cut with `…`.
    Name(String),
}

impl Value {
    /// A word too wide for its room is not drawn in part, and a path or a name cut to nothing
    /// says nothing.
    fn drawn(&self, columns: usize) -> Option<String> {
        let text = match self {
            Value::Word(word) => word.clone(),
            Value::Path(path) => tail(path, columns),
            Value::Name(name) => head(name, columns),
        };
        (!text.is_empty() && text.chars().count() <= columns).then_some(text)
    }
}

impl Fact {
    fn word(label: &'static str, value: impl Into<String>) -> Fact {
        Fact {
            label: Some(label),
            value: Value::Word(value.into()),
        }
    }

    fn path(label: &'static str, path: &Path) -> Fact {
        Fact {
            label: Some(label),
            value: Value::Path(path.to_string_lossy().into_owned()),
        }
    }

    fn name(label: &'static str, name: impl Into<String>) -> Fact {
        Fact {
            label: Some(label),
            value: Value::Name(name.into()),
        }
    }

    fn bare(name: impl Into<String>) -> Fact {
        Fact {
            label: None,
            value: Value::Name(name.into()),
        }
    }

    fn drawn(&self, width: u16) -> Option<Drawn> {
        let room = match self.label {
            None => usize::from(width),
            Some(_) => value_room(width),
        };
        Some(Drawn {
            label: self.label.map(String::from),
            value: self.value.drawn(room)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Drawn {
    label: Option<String>,
    value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Placed {
    pub label: &'static str,
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
        let mut run = vec![
            Fact::word("wire", document.wire.word()),
            Fact::name("model", document.model.clone()),
        ];
        if let Some(route) = document.route {
            run.push(Fact::word("route", route.word()));
        }
        run.push(Fact::word(
            "think",
            if document.thinking { "high" } else { "off" },
        ));

        let mut meters = Vec::new();
        if let Some((turns, cap)) = facts.turns {
            meters.push(Fact::word("turns", format!("{turns}/{cap}")));
        }
        if let Some(ctx) = &facts.ctx {
            meters.push(Fact::word("ctx", ctx.said()));
        }
        if let Some(cache) = facts.cache {
            meters.push(Fact::word("cache", format!("{cache}%")));
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
                spend.push(Fact::word(label, tokens(count)));
            }
        }

        let mut place = vec![Fact::path("workspace", &document.workspace)];
        if let Some(branch) = &document.branch {
            place.push(Fact::word("branch", branch.clone()));
        }
        if let Some(commit) = &document.commit {
            place.push(Fact::word("commit", commit.clone()));
        }

        let mut stop = vec![Fact::word("stopped", document.stopped.word())];
        if let Some(reason) = document.stopped.stop() {
            stop.push(Fact::word("exit", reason.exit_code().to_string()));
        }
        stop.push(Fact::word("start", document.start.clone()));
        if let Some(end) = &document.end {
            stop.push(Fact::word("end", end.clone()));
        }

        let confine = vec![Fact::word("sandbox", document.enforcement.word())];

        let names: Vec<Fact> = skills.iter().map(Fact::bare).collect();

        let mut session = Vec::new();
        if let Some(guid) = &facts.session {
            session.push(Fact::word("session", guid.clone()));
        }
        if let Some(temp) = &facts.temp {
            session.push(Fact::path("temp", temp));
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
        let Some(column) = screen.columns.last() else {
            return;
        };
        if screen.went == 0 || area.height == 0 {
            return;
        }
        put(
            buffer,
            column.x,
            area.bottom() - 1,
            &more(screen.went),
            Ink::Muted,
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
            let Some(place) = (at..count).find(|column| room[*column] >= height) else {
                went += self.blocks[index..]
                    .iter()
                    .map(|block| block.rows.len())
                    .sum::<usize>();
                break;
            };
            went += block.rows.len() - drawn.len();
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

    fn columns(&self, width: u16) -> usize {
        let holds = (width.saturating_add(COLUMN_GAP)) / (MIN_COLUMN + COLUMN_GAP);
        usize::from(holds.max(1)).min(self.blocks.len())
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

fn drawn_row(buffer: &mut Buffer, column: &Column, y: u16, row: &Drawn) {
    let Some(label) = &row.label else {
        put(buffer, column.x, y, &row.value, Ink::Muted);
        return;
    };
    let area = Rect {
        x: column.x,
        y,
        width: column.width,
        height: 1,
    };
    let x = label_row(buffer, area, label, row.value.chars().count());
    put(buffer, x, y, &row.value, Ink::Muted);
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
    use crate::rail::Ctx;

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
                    match &row.label {
                        None => row.value.clone(),
                        Some(label) => format!("{label} {}", row.value),
                    }
                }))
            })
            .collect()
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
