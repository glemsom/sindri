//! The rail as a column, and as one strip when the terminal is too narrow for
//! one, with the face's space taken first so its absence closes the rail up
//! rather than leaving a hole. The rail is chrome, not a surface: it lets the
//! canvas through beside the words and draws no fill, so the frame holds one
//! panel the user owns — the composer — and the rail says its facts as a
//! ledger: labels in a fixed field inset by `RAIL_INSET` from the seam, dot
//! leaders across the gap, values hanging off one edge and a hairline between
//! the blocks.

use super::RAIL_INSET;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::face;
use crate::markdown::text;
use crate::rail::{Block, Fact, Fit, priority};
use crate::theme;

const FACE_FLOOR: u16 = face::ROWS;
/// The label field: nine columns, so `workspace` fits whole and the dot leader
/// begins beside it at the same column.
const LABEL: usize = 9;
/// The label's field and the space after it: where the room for a leader and
/// a value begins.
const LEDGER: usize = LABEL + 1;
/// The fewest cells of gap worth bridging with dots: a leader with nothing to
/// bridge is noise, so a value that fills its room draws none.
const LEADER_FLOOR: usize = 3;

pub fn column(app: &mut App, area: Rect, buf: &mut Buffer) {
    // The rail is transparent, like the transcript it sits beside: only its
    // hairline and the row its chip sits on are painted, so the rail never
    // competes with the composer for the one panel on screen.
    let width = area.width as usize;
    let drawn = facts(app, width, area.height as usize);
    let spare = area.height.saturating_sub(drawn.lines.len() as u16);
    let mut rows = area;
    if spare >= FACE_FLOOR
        && let Some(face) = app.face_mut()
    {
        // The rows the picture takes, not the rows the rail would allow: a
        // square crop in a rail of tall cells is shorter than it is wide, and
        // reserving the rest would leave a hole under the face.
        let height = face.rows(Rect {
            height: spare,
            ..area
        });
        // The face is a picture the rail only places, and its edge is
        // transparent: a pixel it did not paint shows the cell behind it. So the
        // rail paints the canvas it hands the picture before the picture lands,
        // rather than borrowing the fill the frame happens to have done.
        let placed = Rect { height, ..area };
        buf.set_style(placed, theme::canvas());
        face.render(placed, buf);
        rows = Rect {
            y: area.y + height,
            height: area.height - height,
            ..area
        };
    }
    if rows.height == 0 || rows.width == 0 {
        return;
    }
    let below = rows.y.saturating_sub(area.y) as usize;
    // What a row copies is the fact it states, never the dots that drew it:
    // the typography is the frame's, and the fact is the user's.
    let session = drawn.session;
    app.set_rail_view(
        rows,
        drawn.copies,
        session.start.saturating_sub(below)..session.end.saturating_sub(below),
    );
    Paragraph::new(drawn.lines).render(rows, buf);
}

/// One drawn row and the fact it states. The leader and the rule are the
/// frame's typography; only the fact is a copy's business.
struct Row {
    line: Line<'static>,
    copies: String,
}

impl Row {
    /// A row that states a fact, and the fact as it copies.
    fn said(line: Line<'static>, copies: impl Into<String>) -> Row {
        Row {
            line,
            copies: copies.into(),
        }
    }

    /// A row that states nothing — the hairline — and so copies nothing.
    fn bare(line: Line<'static>) -> Row {
        Row::said(line, String::new())
    }
}

/// The rows the rail drew, and what each one copies as.
struct Drawn {
    lines: Vec<Line<'static>>,
    copies: Vec<String>,
    /// Where the session fact sits among the rows: a click there copies the
    /// whole GUID, which the row has no room to draw.
    session: std::ops::Range<usize>,
}

/// The rail's rows, in the order it reads them.
fn facts(app: &App, width: usize, height: usize) -> Drawn {
    let all = app.rail().facts(app.now());
    // A column shorter than the facts it has to state gives them up from the
    // bottom of the one priority order, keeps a row back for saying so, and
    // never loses the posture.
    let (facts, hidden) = keep(&all, height);
    let mut drawn = Drawn {
        lines: Vec::new(),
        copies: Vec::new(),
        session: 0..0,
    };
    let mut ruled = false;
    // A value is never wrapped: it is shortened to the room beside its label,
    // so a fact is one row however long it is.
    for fact in &facts {
        if fact.block() != Block::Spend && !ruled {
            // The rule opens the identity block: the numbers above it, what
            // the run *is* below it.
            drawn.push(rule(width));
            ruled = true;
        }
        if fact.copies_whole() {
            drawn.session = drawn.lines.len()..drawn.lines.len() + 1;
        }
        drawn.push(row(fact, width));
    }
    if hidden > 0 {
        drawn.push(more(hidden));
    }
    drawn
}

impl Drawn {
    fn push(&mut self, row: Row) {
        self.lines.push(row.line);
        self.copies.push(row.copies);
    }
}

/// The facts a column `height` rows tall can state, in the order it reads
/// them, and how many it had to leave out. A fact that has just gone needs a
/// row to say so, so the row is counted against the rail from the first loss
/// and not a row before it.
fn keep(facts: &[Fact], height: usize) -> (Vec<Fact>, usize) {
    let mut kept = facts.to_vec();
    let mut hidden = 0;
    while kept.len() > 1 && rows(&kept) + usize::from(hidden > 0) > height {
        let loser = kept
            .iter()
            .enumerate()
            .min_by_key(|(at, fact)| (priority(fact), *at))
            .map(|(at, _)| at);
        match loser {
            Some(at) => {
                kept.remove(at);
                hidden += 1;
            }
            None => break,
        }
    }
    (kept, hidden)
}

/// The rows a set of facts draws in: one each, and the rule between the
/// numbers and what the run is.
fn rows(facts: &[Fact]) -> usize {
    let spend = facts.iter().any(|fact| fact.block() == Block::Spend);
    let after = facts.iter().any(|fact| fact.block() != Block::Spend);
    facts.len() + usize::from(spend && after)
}

/// The one row that says the rail had room for only some of its facts, and
/// which way there is more: a short terminal loses facts one priority order at
/// a time, never in silence.
fn more(hidden: usize) -> Row {
    let said = format!("+{hidden} more");
    // Inside the inset, like a fact's label: the row is the column's, not a
    // fact's, but it starts where they start.
    let line = format!("{}{said}", " ".repeat(RAIL_INSET));
    Row::said(Line::from(Span::styled(line, theme::faint())), said)
}

fn row(fact: &Fact, width: usize) -> Row {
    match fact.block() {
        Block::Posture => {
            // The chip's row is one raised run: it begins at the rail's first
            // column and runs to its last, so the chip is right-aligned on the
            // edge the values hang off and flush to the rail's edge.
            let said = format!(" ⬢ {} ", fact.value());
            let pad = width.saturating_sub(said.width());
            Row::said(
                Line::from(vec![
                    Span::styled(" ".repeat(pad), theme::raised()),
                    Span::styled(said, chip()),
                ]),
                format!("⬢ {}", fact.value()),
            )
        }
        _ => {
            // The figure hangs off the rail's last column, so every value in
            // the ledger ends on one edge and can be read down it.
            let said = value(fact, width.saturating_sub(RAIL_INSET + LEDGER));
            let gap = width.saturating_sub(RAIL_INSET + LEDGER + said.width());
            let leader = leader(gap);
            // A figure that has just grown is the accent for a moment, so the
            // motion is visible rather than inferred; a quiet one is prose like
            // the rest of the frame.
            let style = if fact.fresh() {
                theme::prompt()
            } else {
                theme::plain()
            };
            Row::said(
                Line::from(vec![
                    Span::raw(" ".repeat(RAIL_INSET)),
                    Span::styled(format!("{:<LABEL$} ", fact.label()), theme::faint()),
                    leader,
                    Span::styled(said.clone(), style),
                ]),
                // The label and the value: the row a reader copies is the fact
                // `context 38%`, and never `context ···· 38%`.
                format!("{} {}", fact.label(), said),
            )
        }
    }
}

/// What bridges the label's field to the value: dots across the whole gap
/// where there is one worth bridging, and blank cells where the value leaves
/// too little room for a leader to say anything.
fn leader(gap: usize) -> Span<'static> {
    if gap < LEADER_FLOOR {
        Span::raw(" ".repeat(gap))
    } else {
        Span::styled("·".repeat(gap), theme::leader())
    }
}

/// The value, shortened to `room` display cells — the column the rail has
/// beside the label's field: an id or a path keeps both its ends, so nothing is
/// ever broken mid-token across rows, and a wide glyph is two cells either
/// wholly in or wholly out.
fn value(fact: &Fact, room: usize) -> String {
    match fact.fit() {
        Fit::Middle => text::middle(fact.value(), room),
        Fit::End => text::clip(fact.value(), room),
    }
}

/// The rule that opens the identity block: a hairline across the rail's whole
/// width, so it groups the blocks instead of dividing the column from the
/// page.
fn rule(width: usize) -> Row {
    Row::bare(Line::from(Span::styled("─".repeat(width), theme::faint())))
}

/// A posture is a chip: stated on the raised tier, so it reads as the one
/// thing on the rail that is not a label and a value — and the only row of the
/// rail that carries a colour behind it. The first frame's posture line
/// borrows the same one.
pub(crate) fn chip() -> ratatui::style::Style {
    theme::raised().patch(theme::good())
}

pub fn strip(app: &App, area: Rect, buf: &mut Buffer) {
    // The strip is one row, and what it says already fits it: what was said is
    // shortened or dropped where it was chosen rather than cut here. It carries
    // the ledger's inset too, so the line's first word is off the seam as every
    // fact's is, and the room is asked for without it.
    let room = (area.width as usize).saturating_sub(RAIL_INSET);
    let line = format!(
        "{}{}",
        " ".repeat(RAIL_INSET),
        app.rail().strip(room, app.now())
    );
    Paragraph::new(Line::from(Span::styled(line, theme::faint()))).render(area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Launch;

    fn launch() -> Launch {
        Launch {
            session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
            temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
            branch: Some("t107-the-canvas".to_string()),
            sandbox: "bwrap · 1 writable".to_string(),
            workspace: "~/git/sindri".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            max_turns: 250,
            context_limit: Some(200_000),
        }
    }

    #[test]
    fn a_value_of_wide_glyphs_still_ends_on_the_rails_last_column() {
        // A CJK workspace draws two cells a glyph, so a leader measured in
        // chars leaves a row wider than the rail — and a row that outgrows its
        // column wraps, which the rail may not do.
        let app = App::new(
            Launch {
                workspace: "東京都/レポ".to_string(),
                model: "gpt-東".to_string(),
                ..launch()
            },
            None,
        );
        let width = 32;
        let facts = app.rail().facts(app.now());
        for fact in &facts {
            let drawn = row(fact, width);
            assert_eq!(
                drawn.line.width(),
                width,
                "{:?} fills the rail's {width} columns: {:?}",
                fact.label(),
                drawn.line
            );
        }
    }

    #[test]
    fn the_rows_the_face_takes_are_canvas_before_the_picture_lands() {
        // The face is a picture the rail only places: its edge is transparent,
        // and what shows through is the cell behind it, so the rows it takes are
        // the theme's before the picture lands.
        let mut app = App::new(launch(), crate::face::Face::kitty());
        let area = Rect::new(64, 0, 32, 40);
        let mut buf = Buffer::empty(area);
        column(&mut app, area, &mut buf);
        let placed: Vec<u16> = (0..area.height)
            .filter(|y| buf[(area.x, *y)].symbol().contains('\u{10EEEE}'))
            .collect();
        assert!(!placed.is_empty(), "the face is placed in the rail's top");
        for y in placed {
            for x in area.x..area.right() {
                let cell = &buf[(x, y)];
                assert_eq!(
                    cell.bg,
                    theme::CANVAS,
                    "({x}, {y}) is behind the face, and the theme names the canvas there"
                );
                assert_eq!(
                    cell.fg,
                    theme::PLAIN,
                    "({x}, {y}) carries the canvas's ink and none of the picture's"
                );
            }
        }
    }
}
