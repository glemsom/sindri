//! The pane: the run's own surface for a thought, at the tail of the transcript
//! and hanging off the same two-column gutter a turn's answer and its calls hang
//! off. It is the run's and not the reader's — it takes no key, cannot be
//! dismissed and dims nothing behind it — because a thought is a state of the
//! run, and the reader may well be reading an earlier answer while the run
//! thinks. Its text fills from the bottom, so a fragment never moves the line
//! the reader is on, and a row that leaves the top is cut and never dimmed.

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::markdown;
use crate::pulse;
use crate::render::rows::GUTTER;
use crate::theme;
use crate::thought::Thought;

/// The most a thought may cost the thread: enough to read a sentence and its
/// context, and never enough to become the screen.
pub const PANE_ROWS: usize = 8;
/// The least the pane settles at once it has a line of text: five rows, so a
/// blank pane is never the whole of what a thought looks like.
pub const PANE_MIN: usize = 5;

/// The rows the thought's own text may take: the head row spends one of
/// `PANE_ROWS`, and the rest are the body's.
const BODY_ROWS: usize = PANE_ROWS - 1;

/// The columns the left rule and the cell after it take, between the gutter and
/// the words.
const RULE_COLUMNS: usize = 2;

/// The source lines of a thought that are wrapped: the rows above the pane are
/// cut, so a thought that has been running for minutes is not wrapped whole at
/// every tick, and a fragment costs the words it landed with.
const KEPT_LINES: usize = 64;

/// The rows the pane takes at `width`: the head row and the thought's own rows,
/// never past `PANE_ROWS` and never short of `PANE_MIN`.
pub fn height(thought: &Thought, width: u16) -> usize {
    (1 + written(&thought.text, width).len()).clamp(PANE_MIN, PANE_ROWS)
}

/// The pane, drawn on the raised surface of the rows the layout gave it. The
/// frame behind it is not dimmed: a dim is an overlay's lie about who has the
/// screen, and a thought is the run's own.
pub fn draw(app: &App, area: Rect, buf: &mut Buffer) {
    let Some(thought) = app.thought() else {
        return;
    };
    if area.height == 0 {
        return;
    }
    let body = written(&thought.text, area.width);
    let rows_of_text = body.len();
    // The text fills from the bottom: the newest row sits on the pane's last
    // row, the rows above it are blank while the thought is shorter than the
    // pane, and the rows that no longer fit leave from the top.
    let room = (area.height as usize).saturating_sub(1);
    let kept = rows_of_text.min(room);
    let mut rows = vec![head(thought, app.now())];
    rows.extend((0..room - kept).map(|_| Line::default()));
    rows.extend(body.into_iter().skip(rows_of_text - kept));
    let rows = ruled(rows);
    buf.set_style(area, theme::raised());
    Paragraph::new(rows).render(area, buf);
}

/// The thought's words at the bottom of the pane, oldest first: its tail,
/// wrapped to the columns the gutter and the rule leave, and no more rows than
/// the pane may show.
fn written(text: &str, width: u16) -> Vec<Line<'static>> {
    let columns = (width as usize).saturating_sub(GUTTER as usize + RULE_COLUMNS);
    let mut rows = markdown::text::wrap(
        markdown::text::glyphs(&[Span::styled(tail(text).to_string(), theme::faint())]),
        "",
        theme::faint(),
        columns,
    );
    let over = rows.len().saturating_sub(BODY_ROWS);
    rows.drain(..over);
    rows
}

/// The last `KEPT_LINES` of a thought, cut at a line break so the rows that
/// arrive above the pane are the only ones the cut is not wrapping.
fn tail(text: &str) -> &str {
    text.rmatch_indices('\n')
        .nth(KEPT_LINES)
        .map_or(text, |(at, _)| &text[at + 1..])
}

/// The pane's first row: what the run is doing, and for how long — the elapsed
/// read through `pulse::said`, the clock's own words.
fn head(thought: &Thought, now: Duration) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            "⌁ thinking   {}",
            pulse::said(now.saturating_sub(thought.started))
        ),
        theme::prompt(),
    ))
}

/// The pane's rows with the left rule in front of them: a walk from `PROMPT` to
/// `HEADING`, one cell per row, so a border gets a gradient without a renderer
/// writing a colour down.
fn ruled(rows: Vec<Line<'static>>) -> Vec<Line<'static>> {
    let steps = rows.len().saturating_sub(1).max(1) as f32;
    let pad = " ".repeat(GUTTER as usize);
    rows.into_iter()
        .enumerate()
        .map(|(row, line)| {
            let walk = theme::ramp(theme::PROMPT, theme::HEADING, row as f32 / steps);
            let mut spans = vec![
                Span::raw(pad.clone()),
                Span::styled("│", Style::new().fg(walk)),
                Span::raw(" "),
            ];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{KEPT_LINES, height, tail};
    use crate::thought::Thought;
    use std::time::Duration;

    fn thought(text: &str) -> Thought {
        Thought {
            turn: 1,
            text: text.to_string(),
            started: Duration::ZERO,
        }
    }

    #[test]
    fn a_pane_is_five_rows_at_least_and_eight_at_most() {
        for (text, rows) in [
            ("", 5),
            ("one line", 5),
            ("a\nb\nc\nd", 5),
            ("a\nb\nc\nd\ne\nf\ng\nh\ni\n", 8),
        ] {
            assert_eq!(
                height(&thought(text), 80),
                rows,
                "a thought of {text:?} costs the thread {rows} rows"
            );
        }
    }

    #[test]
    fn only_the_tail_of_a_long_thought_is_wrapped() {
        let long: String = (0..KEPT_LINES * 3)
            .map(|line| format!("line {line}\n"))
            .collect();
        let cut = tail(&long);
        assert_eq!(
            cut.matches('\n').count(),
            KEPT_LINES,
            "the rows above the pane are cut before they are wrapped"
        );
        assert!(
            cut.ends_with(&format!("line {}\n", KEPT_LINES * 3 - 1)),
            "and the cut keeps the newest words, which are the ones on screen"
        );
        assert_eq!(
            tail("short"),
            "short",
            "a thought that fits is wrapped whole"
        );
    }
}
