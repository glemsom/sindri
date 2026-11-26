#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub row: u16,
    pub column: u16,
}

impl Cell {
    pub fn new(row: u16, column: u16) -> Cell {
        Cell { row, column }
    }
}

pub type Glyph = (u16, char, u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    anchor: Cell,
    head: Cell,
}

impl Mark {
    pub fn drag(anchor: Cell, head: Cell) -> Mark {
        Mark { anchor, head }
    }

    pub fn anchor(self) -> Cell {
        self.anchor
    }

    pub fn head(self) -> Cell {
        self.head
    }

    pub fn first(self) -> u16 {
        self.anchor.row.min(self.head.row)
    }

    pub fn last(self) -> u16 {
        self.anchor.row.max(self.head.row)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    rows: Vec<Marked>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marked {
    row: u16,
    left: u16,
    width: u16,
    glyphs: String,
}

impl Marked {
    pub fn row(&self) -> u16 {
        self.row
    }

    pub fn left(&self) -> u16 {
        self.left
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn glyphs(&self) -> &str {
        &self.glyphs
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    glyphs: Vec<Glyph>,
    cut: bool,
}

impl Line {
    pub fn of(glyphs: Vec<Glyph>, cut: bool) -> Line {
        Line { glyphs, cut }
    }
}

impl Span {
    pub fn of(mark: Mark, lines: &[Line]) -> Span {
        let first = mark.first();
        let last = mark.last();
        let (start, end) =
            if (mark.anchor.row, mark.anchor.column) <= (mark.head.row, mark.head.column) {
                (mark.anchor, mark.head)
            } else {
                (mark.head, mark.anchor)
            };
        let mut rows = Vec::new();
        for row in first..=last {
            let Some(line) = lines.get(usize::from(row)) else {
                continue;
            };
            let from = if row == first { start.column } else { 0 };
            let to = if row == last { end.column } else { u16::MAX };
            rows.push(marked_row(row, &line.glyphs, from, to));
        }
        Span { rows }
    }

    pub fn rows(&self) -> &[Marked] {
        &self.rows
    }

    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(|marked| marked.glyphs.as_str())
            .collect::<Vec<&str>>()
            .join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.text().is_empty()
    }
}

fn marked_row(row: u16, line: &[Glyph], from: u16, to: u16) -> Marked {
    let kept: Vec<&Glyph> = line
        .iter()
        .filter(|(column, _, width)| column.saturating_add(*width) > from && *column <= to)
        .collect();
    let left = kept.first().map_or(from, |glyph| glyph.0);
    let end = kept
        .last()
        .map_or(left, |glyph| glyph.0.saturating_add(glyph.2));
    Marked {
        row,
        left,
        width: end.saturating_sub(left),
        glyphs: kept.iter().map(|(_, glyph, _)| *glyph).collect(),
    }
}

#[cfg(test)]
mod tests;
