//! One paint of a view at a size, and one screen read off the bytes a launch drew at a size, read
//! the way a test asks: the cells, the palette, the words, the caret. `dead_code` is allowed because
//! every test binary compiles this harness and each one reads a different part of it.
#![allow(dead_code)]

use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use sindri::ink::{Ink, Look};
use sindri::list::QUERY;
use sindri::palette::Palette;
use sindri::pictures::PLACEHOLDER;
use sindri::view::{Painted, View};

const SNAPSHOTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// A view painted once on a headless screen, or a screen read off the bytes a launch drew: the
/// cells, the palette a paint drew them in, what a pass told the terminal of the cursor it placed,
/// and the cell the caret is in after the frame.
pub struct Frame {
    cells: Buffer,
    palette: Option<Palette>,
    told: Painted,
    caret: Option<(u16, u16)>,
}

/// `view` painted once on a headless screen of that size: the one pass every reader here reads.
pub fn painted(view: &mut View, cols: u16, rows: u16) -> Frame {
    let palette = view.capabilities().palette();
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    let mut told = Painted::none();
    let completed = terminal
        .draw(|screen| {
            told = view.paint(screen.area(), screen.buffer_mut());
        })
        .expect("the frame is painted");
    Frame {
        cells: completed.buffer.clone(),
        palette: Some(palette),
        caret: told.caret().map(|at| (at.y, at.x)),
        told,
    }
}

/// The frame the bytes `drawn` left in a terminal of that size, read as the cells it holds, and the
/// cell the caret was in when it went quiet. No pass here told a terminal anything, so the cells
/// carry the colours it drew them in and there is no palette to name them by.
pub fn read(drawn: &str, cols: u16, rows: u16) -> Frame {
    let mut terminal =
        avt::Terminal::new(usize::from(cols), usize::from(rows)).expect("a screen of that size");
    terminal.feed(drawn);
    let mut cells = Buffer::empty(Rect::new(0, 0, cols, rows));
    for (y, row) in terminal.screen().visible_rows().enumerate() {
        let y = u16::try_from(y).expect("a row of the screen");
        cells.set_string(0, y, row.text(), Style::default());
    }
    let caret = terminal
        .cursor()
        .position()
        // A parser counts a cursor as (col, row) where a frame's cell is (row, column).
        .map(|(col, row)| (narrow(row), narrow(col)));
    Frame {
        cells,
        palette: None,
        told: Painted::none(),
        caret,
    }
}

/// A row or a column a terminal counts: a terminal wider or taller than a `u16` names is no frame a
/// test reads.
fn narrow(at: usize) -> u16 {
    u16::try_from(at).expect("a row or a column of the screen")
}

/// A frame of cells written outside a paint, for the rules a real view cannot reach.
pub fn holding(cells: Buffer, palette: Palette) -> Frame {
    Frame {
        cells,
        palette: Some(palette),
        told: Painted::none(),
        caret: None,
    }
}

impl Frame {
    pub fn cells(&self) -> &Buffer {
        &self.cells
    }

    pub fn area(&self) -> Rect {
        self.cells.area
    }

    pub fn cell(&self, x: u16, y: u16) -> &Cell {
        self.cells
            .cell((x, y))
            .unwrap_or_else(|| panic!("row {y}, column {x} is outside the frame it is in"))
    }

    pub fn palette(&self) -> &Palette {
        self.palette.as_ref().expect(
            "a frame read off a terminal's bytes carries the colours it drew and no palette to \
             name them, so none of its cells is a look",
        )
    }

    /// The cell the terminal's caret is in after the frame, and none where it placed none.
    pub fn caret(&self) -> Option<(u16, u16)> {
        self.caret
    }

    /// One row of the cells as glyphs, one to a column and full width, from the frame's left edge.
    pub fn row(&self, y: u16) -> String {
        let area = self.cells.area;
        (area.x..area.right()).map(|x| self.glyph(x, y)).collect()
    }

    pub fn rows(&self) -> Vec<String> {
        (self.cells.area.y..self.cells.area.bottom())
            .map(|y| self.row(y))
            .collect()
    }

    /// Every row as the words it says: the row it is, cut at the trailing blanks.
    pub fn words(&self) -> Vec<String> {
        self.rows()
            .iter()
            .map(|row| row.trim_end().to_owned())
            .collect()
    }

    pub fn glyph(&self, x: u16, y: u16) -> char {
        glyph(self.cell(x, y), x, y)
    }

    pub fn look(&self, x: u16, y: u16) -> Look {
        let cell = self.cell(x, y);
        Look::of(cell, self.palette()).unwrap_or_else(|| {
            panic!(
                "row {y}, column {x} is drawn in a style that is not a look: foreground {:?}, \
                 background {:?}, modifiers {:?} — every cell of a frame is drawn in a look, so a \
                 golden names looks and a cell no look names cannot be written down",
                cell.fg, cell.bg, cell.modifier
            )
        })
    }

    pub fn ink(&self, x: u16, y: u16) -> Option<Ink> {
        Ink::of(self.cell(x, y), self.palette())
    }

    pub fn told(&self) -> &Painted {
        &self.told
    }

    /// The column `glyph` is drawn in on row `y`, counted from the frame's first column.
    pub fn column_of(&self, y: u16, glyph: char) -> Option<u16> {
        let area = self.cells.area;
        self.row(y)
            .chars()
            .position(|said| said == glyph)
            .and_then(|at| u16::try_from(at).ok())
            .map(|at| area.x.saturating_add(at))
    }

    /// The row of the frame whose cells say `said`, counted from the band's first row.
    pub fn row_holding(&self, band: Rect, said: &str) -> u16 {
        for y in band.y..band.bottom() {
            if self.row(y).contains(said) {
                return y.saturating_sub(band.y);
            }
        }
        panic!("the frame draws {said:?} on no row of the band it was given");
    }

    /// The row and the column the words `said` begin at.
    pub fn drawn_at(&self, said: &str) -> (u16, u16) {
        let wanted: Vec<char> = said.chars().collect();
        let area = self.cells.area;
        for y in area.y..area.bottom() {
            let row: Vec<char> = self.row(y).chars().collect();
            if let Some(at) = row
                .windows(wanted.len())
                .position(|held| held == wanted.as_slice())
            {
                return (
                    y,
                    area.x.saturating_add(u16::try_from(at).expect("a column")),
                );
            }
        }
        panic!("the frame draws {said:?} on no row of it");
    }

    /// The golden text: the name and size of the frame, then every row as its glyphs between two
    /// bars and the runs of looks they are drawn in, so a frame one cell apart is a frame one word
    /// apart.
    pub fn text(&self, name: &str) -> String {
        let area = self.cells.area;
        let mut text = format!("{name} at {}x{}", area.width, area.height);
        for y in area.y..area.bottom() {
            let mut glyphs = String::new();
            let mut runs: Vec<(Look, u16)> = Vec::new();
            for x in area.x..area.right() {
                let look = self.look(x, y);
                glyphs.push(self.glyph(x, y));
                match runs.last_mut() {
                    Some((last, count)) if *last == look => *count += 1,
                    _ => runs.push((look, 1)),
                }
            }
            let runs: Vec<String> = runs
                .iter()
                .map(|(look, count)| format!("{}:{count}", look.name()))
                .collect();
            text.push_str(&format!("\n{y:02} |{glyphs}| {}", runs.join(" ")));
        }
        text
    }

    /// The golden `fixture` that the frame `name` draws: the snapshot the suite holds is this one
    /// text, blessed from here and from no other file.
    pub fn snapshot(&self, name: &str, fixture: &str) {
        insta::with_settings!({ snapshot_path => SNAPSHOTS, prepend_module_to_snapshot => false }, {
            insta::assert_snapshot!(fixture, self.text(name));
        });
    }

    /// Holds the caret the pass placed against the cell the cells draw a card's query row at: a
    /// golden frame writes down cells and no positions, so the two are read apart.
    pub fn assert_caret(&self, fixture: &str) {
        if let Err(disagreed) = carets_agree(self.query_row(), self.told.caret()) {
            panic!("frame {fixture}: {disagreed}");
        }
    }

    /// The cell a card's query row is typed at, and none while the frame drew no card.
    pub fn query_row(&self) -> Option<(u16, u16)> {
        drawn_query(&self.cells, self.told.layout()?.card?)
    }
}

/// Why the two readings are no one caret: a query row the pass placed no cursor in, or one it
/// placed the cursor elsewhere in. A frame that draws no query row places its cursor where it likes.
pub fn carets_agree(named: Option<(u16, u16)>, placed: Option<Position>) -> Result<(), String> {
    let placed = placed.map(|at| (at.x, at.y));
    match (named, placed) {
        (Some(named), Some(placed)) if named != placed => Err(format!(
            "its cells type the query at {named:?} where the pass placed the cursor at {placed:?}"
        )),
        (Some(named), None) => Err(format!(
            "its cells type the query at {named:?} and the pass placed no cursor there"
        )),
        _ => Ok(()),
    }
}

/// The cell a card's query row is typed at, read off a frame's cells: the row of `body` that draws
/// the words `query: `, and the column past the last glyph drawn after them — a blank the reader
/// typed at the end of a query is that body's own fill by then.
fn drawn_query(cells: &Buffer, body: Rect) -> Option<(u16, u16)> {
    let area = cells.area;
    let words: Vec<char> = QUERY.chars().collect();
    for y in body.y..body.bottom() {
        let row: Vec<(u16, char)> = (area.x..area.right())
            .map(|x| {
                let cell = cells.cell((x, y)).expect("a cell of the row");
                (x, glyph(cell, x, y))
            })
            .collect();
        let Some(at) = row.windows(words.len()).position(|window| {
            window
                .iter()
                .map(|(_, drawn)| *drawn)
                .eq(words.iter().copied())
        }) else {
            continue;
        };
        let typed: Vec<(u16, char)> = row[at..]
            .iter()
            .copied()
            .take_while(|(x, _)| *x < body.right())
            .collect();
        let last = typed.iter().rposition(|(_, drawn)| *drawn != ' ')?;
        let column = typed
            .get((last + 1).max(words.len()))
            .map(|(x, _)| *x)
            .unwrap_or_else(|| body.right().saturating_sub(1));
        return Some((column, y));
    }
    None
}

/// The glyph one cell is written as: the skip a picture hides behind, its placeholder and the escape
/// sequence a protocol is written in are all the blank of the band they sit on, because a picture is
/// not glyphs and its rows are what a frame shows.
fn glyph(cell: &Cell, x: u16, y: u16) -> char {
    let symbol = cell.symbol();
    if cell.skip || symbol.contains(PLACEHOLDER) || symbol.contains('\u{1b}') {
        return ' ';
    }
    let mut characters = symbol.chars();
    match (characters.next(), characters.next()) {
        (Some(glyph), None) => glyph,
        _ => panic!(
            "row {y}, column {x} holds {symbol:?}, which is not one glyph: a frame counts one \
             glyph per column, so a glyph wider than one column — and the cell it ate — has \
             nowhere to be written down"
        ),
    }
}

/// The text the golden `fixture` holds, and none while no fixture of that name is blessed.
pub fn accepted(fixture: &str) -> Option<String> {
    let path = PathBuf::from(SNAPSHOTS).join(format!("{fixture}.snap"));
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != "---" {
        return Some(text);
    }
    let frame: Vec<&str> = lines.skip_while(|line| *line != "---").skip(1).collect();
    Some(frame.join("\n"))
}

/// Where the golden `picture` and the frame `drawn` first disagree, and none where they agree.
pub fn first_difference(picture: &str, drawn: &str) -> Option<String> {
    let picture: Vec<&str> = picture.lines().collect();
    let drawn: Vec<&str> = drawn.lines().collect();

    for (line, (expected, actual)) in picture.iter().zip(&drawn).enumerate() {
        if expected == actual {
            continue;
        }
        return Some(match (row_of(expected), row_of(actual)) {
            (Some(expected), Some(actual)) => differing_cell(&expected, &actual),
            _ => format!(
                "line {} reads {expected:?} where the frame draws {actual:?}",
                line + 1
            ),
        });
    }

    (picture.len() != drawn.len()).then(|| {
        format!(
            "the fixture holds {} lines and the frame draws {}",
            picture.len(),
            drawn.len()
        )
    })
}

fn differing_cell(expected: &Row, actual: &Row) -> String {
    let columns = expected.glyphs.len().max(actual.glyphs.len());
    for column in 0..columns {
        let (held, drawn) = (cell(expected, column), cell(actual, column));
        if held != drawn {
            return format!(
                "row {}, column {column}: the fixture holds {held} and the frame draws {drawn}",
                expected.number
            );
        }
    }
    format!(
        "row {}: every cell of it agrees and its line does not, which this harness wrote wrong",
        expected.number
    )
}

fn cell(row: &Row, column: usize) -> String {
    match (row.glyphs.get(column), row.looks.get(column)) {
        (Some(glyph), Some(look)) => format!("{glyph:?} in {}", look.name()),
        _ => String::from("no cell at all"),
    }
}

pub struct Row {
    pub number: u16,
    pub glyphs: Vec<char>,
    pub looks: Vec<Look>,
}

pub fn row_of(line: &str) -> Option<Row> {
    let (number, rest) = line.split_once(' ')?;
    let (glyphs, runs) = rest.strip_prefix('|')?.rsplit_once('|')?;
    let looks = looks_in(runs.trim())?;
    let glyphs: Vec<char> = glyphs.chars().collect();
    (glyphs.len() == looks.len()).then_some(Row {
        number: number.parse().ok()?,
        glyphs,
        looks,
    })
}

fn looks_in(runs: &str) -> Option<Vec<Look>> {
    let mut looks = Vec::new();
    for run in runs.split_whitespace() {
        let (look, count) = run.split_once(':')?;
        let look = Look::from_name(look)?;
        let count: u16 = count.parse().ok()?;
        looks.extend(std::iter::repeat_n(look, usize::from(count)));
    }
    Some(looks)
}
