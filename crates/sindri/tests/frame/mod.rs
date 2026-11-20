#![allow(dead_code)]

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect};
use sindri::ink::{Ink, Look};
use sindri::list::QUERY;
use sindri::palette::Palette;
use sindri::pictures::PLACEHOLDER;
use sindri::view::{Layout, Painted, View};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// A view painted once on a headless screen: the cells the pass left, the palette it drew them in,
/// and what the pass told the terminal of the cursor it placed.
pub struct Frame {
    cells: Buffer,
    palette: Palette,
    told: Painted,
}

/// `view` painted once on a headless screen of that size: the one pass every reader here reads.
pub fn painted(view: &mut View, cols: u16, rows: u16) -> Frame {
    let palette = view.capabilities().palette();
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    let mut told = Painted::none();
    let completed = terminal
        .draw(|frame| {
            told = view.paint(frame.area(), frame.buffer_mut());
        })
        .expect("the frame is painted");
    Frame {
        cells: completed.buffer.clone(),
        palette,
        told,
    }
}

/// A frame of cells written outside a paint, for the rules a real view cannot reach.
pub fn holding(cells: Buffer, palette: Palette, told: Painted) -> Frame {
    Frame {
        cells,
        palette,
        told,
    }
}

impl Frame {
    pub fn cells(&self) -> &Buffer {
        &self.cells
    }

    pub fn cell(&self, x: u16, y: u16) -> &Cell {
        self.cells
            .cell((x, y))
            .unwrap_or_else(|| panic!("row {y}, column {x} is outside the frame it is in"))
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
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
        Look::of(cell, &self.palette).unwrap_or_else(|| {
            panic!(
                "row {y}, column {x} is drawn in a style that is not a look: foreground {:?}, \
                 background {:?}, modifiers {:?} — every cell of a frame is drawn in a look, so a \
                 golden names looks and a cell no look names cannot be written down",
                cell.fg, cell.bg, cell.modifier
            )
        })
    }

    pub fn ink(&self, x: u16, y: u16) -> Option<Ink> {
        Ink::of(self.cell(x, y), &self.palette)
    }

    pub fn told(&self) -> &Painted {
        &self.told
    }

    pub fn caret(&self) -> Option<Position> {
        self.told.caret()
    }

    pub fn layout(&self) -> Option<Layout> {
        self.told.layout()
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

    /// The golden `golden` that the frame `name` draws: the snapshot the suite holds is this one
    /// text, blessed from here and from no other file.
    pub fn snapshot(&self, name: &str, golden: &str) {
        insta::with_settings!({ snapshot_path => FIXTURES, prepend_module_to_snapshot => false }, {
            insta::assert_snapshot!(golden, self.text(name));
        });
    }

    /// Holds the caret the pass placed against the cell the cells draw a card's query row at: a
    /// golden frame writes down cells and no positions, so the two are read apart.
    pub fn assert_caret(&self, golden: &str) {
        if let Err(disagreed) = carets_agree(self.query_row(), self.told.caret()) {
            panic!("frame {golden}: {disagreed}");
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
