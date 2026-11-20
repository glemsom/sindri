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

/// One glyph of a row as the mark reads it: the column it is drawn from, the glyph, and the columns
/// it covers — a glyph of a heading drawn at twice the cell is one word over two of them.
pub type Glyph = (u16, char, u16);

/// Two corners and not the cells they covered: a resize re-lays the band and a scroll
/// moves it, so a mark that remembered cells would point at whatever came to sit there.
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

    pub fn grown(self, down: bool) -> Mark {
        let row = match down {
            true => self.head.row.saturating_add(1),
            false => self.head.row.saturating_sub(1),
        };
        Mark {
            head: Cell { row, ..self.head },
            ..self
        }
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

    /// The columns of the frame the mark covers, which is the cells its glyphs are drawn in and
    /// never the glyphs themselves: one of them can be two columns wide.
    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn glyphs(&self) -> &str {
        &self.glyphs
    }
}

impl Span {
    /// The words the mark covers in the band `lines` drew: each row's glyphs and whether the band cut
    /// that row's tail off, because a cut row keeps every glyph past the mark's right edge.
    pub fn of(mark: Mark, lines: &[(Vec<Glyph>, bool)]) -> Span {
        let first = mark.anchor.row.min(mark.head.row);
        let last = mark.anchor.row.max(mark.head.row);
        let from = mark.anchor.column.min(mark.head.column);
        let to = mark.anchor.column.max(mark.head.column);
        let mut rows = Vec::new();
        for row in first..=last {
            let Some((line, cut)) = lines.get(usize::from(row)) else {
                continue;
            };
            if let Some(marked) = marked_row(row, line, from, to, *cut) {
                rows.push(marked);
            }
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
        self.rows.is_empty()
    }
}

fn marked_row(row: u16, line: &[Glyph], from: u16, to: u16, cut: bool) -> Option<Marked> {
    // A row the band cut has no cells past the mark that the row does not hold: the left edge the
    // mark chose still holds, and the right one gives way to the words the edge hid.
    let to = match cut {
        true => u16::MAX,
        false => to,
    };
    let kept: Vec<&Glyph> = line
        .iter()
        .filter(|(column, _, width)| column.saturating_add(*width) > from && *column <= to)
        .collect();
    let first = kept
        .iter()
        .position(|(_, glyph, _)| !glyph.is_whitespace())?;
    let last = kept
        .iter()
        .rposition(|(_, glyph, _)| !glyph.is_whitespace())?;
    let left = kept[first].0;
    let (end, _, width) = *kept[last];
    Some(Marked {
        row,
        left,
        width: end.saturating_add(width).saturating_sub(left),
        glyphs: kept[first..=last]
            .iter()
            .map(|(_, glyph, _)| *glyph)
            .collect(),
    })
}

#[cfg(test)]
mod tests;
