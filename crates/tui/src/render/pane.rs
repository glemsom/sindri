//! The pane: the run's own surface for a thought, at the tail of the transcript
//! and hanging off the same two-column gutter a turn's answer and its calls hang
//! off. It is the run's and not the reader's — it takes no key, cannot be
//! dismissed and dims nothing behind it — because a thought is a state of the
//! run, and the reader may well be reading an earlier answer while the run
//! thinks. Its text fills from the bottom, so a fragment never moves the line
//! the reader is on, and a row that leaves the top is cut and never dimmed.
//!
//! It has a motion, and the motion is a function of the clock: it unfurls from
//! its head one row a tick until it stands at `PANE_MIN`, follows its text from
//! there, and folds back down to its head a row a tick when the thought is over.
//! `rows` and `closing` are pure in the clock, so a frame is drawn at a moment
//! and a film of frames is asserted without knowing when any of them was drawn.

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

/// The height the clock has unfurled the pane to, `elapsed` since the thought
/// began: one row on the tick the first fragment lands — the head, and nothing
/// else — and one more a tick, never past the most a thought costs. The pane is
/// the lesser of this and what its text is worth, so the clock alone carries it
/// up to `PANE_MIN` and the text carries it from there. Pure in the clock, so a
/// film of frames is asserted rather than a feeling about one.
pub fn rows(elapsed: Duration) -> usize {
    (1 + pulse::ticks(elapsed)).min(PANE_ROWS)
}

/// The height the pane closes through, `elapsed` since the final reasoning
/// event: one tick of hold at the floor it settles at, then a row a tick until
/// none is left. The row it reaches one at is its last, and the scar is written
/// on that tick, so the two are on the glass together and the eye reads a
/// descent into the transcript rather than a disappearance and an arrival.
pub(crate) fn closing(elapsed: Duration) -> usize {
    PANE_MIN.saturating_sub(pulse::ticks(elapsed))
}

/// The rows the pane takes at `width` and `now`: the lesser of what its text is
/// worth and what the clock has unfurled it to, or the fold's own height once
/// the thought is over.
pub fn height(thought: &Thought, width: u16, now: Duration) -> usize {
    match thought.closed_for(now) {
        Some(since) => closing(since),
        None => settled(written(&thought.text, width).len())
            .min(rows(now.saturating_sub(thought.started))),
    }
}

/// What the pane stands at once its text has its say, `rows` of it written: the
/// head row and those, never past `PANE_ROWS` and never short of `PANE_MIN`.
fn settled(rows: usize) -> usize {
    (1 + rows).clamp(PANE_MIN, PANE_ROWS)
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
    let steps = settled(body.len());
    let rows = laid(thought, body, area.height as usize, app.now());
    let rows = ruled(rows, steps);
    buf.set_style(area, theme::raised());
    Paragraph::new(rows).render(area, buf);
}

/// The pane's rows: its head, then the text hung off the floor of the block —
/// the newest row on the pane's last row, the rows above it blank while the
/// thought is shorter than the pane, and the ones that no longer fit leaving
/// from the top. A folding pane is the same rows with fewer of them: the floor
/// never moves, so the rows the pane gives up are its oldest words and the head
/// above them, and no fragment ever moves the line the reader is on.
fn laid(
    thought: &Thought,
    body: Vec<Line<'static>>,
    room: usize,
    now: Duration,
) -> Vec<Line<'static>> {
    let rows_of_text = body.len();
    let kept = rows_of_text.min(room.saturating_sub(1));
    let mut rows = vec![head(thought, now)];
    rows.extend((0..room.saturating_sub(1) - kept).map(|_| Line::default()));
    rows.extend(body.into_iter().skip(rows_of_text - kept));
    rows
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
///
/// The walk is stepped over the height the pane settles at and not over the
/// rows it happens to have: each row gained lights one more cell of it, so the
/// rule finishes lighting exactly as the pane reaches its height, and a fold
/// gives the light back a row at a time.
fn ruled(rows: Vec<Line<'static>>, steps: usize) -> Vec<Line<'static>> {
    let steps = steps.saturating_sub(1).max(1) as f32;
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
    use super::{KEPT_LINES, closing, height, rows, tail};
    use crate::pulse;
    use crate::thought::Thought;
    use std::time::Duration;

    /// A thought holding `text`: what the pane is worth once the clock has
    /// unfurled it, which is what a height is measured against.
    fn thought(text: &str) -> Thought {
        Thought::new(1, text.to_string(), Duration::ZERO)
    }

    /// The clock `nth` ticks of the pulse in, which is what a pane's motion is
    /// counted in.
    fn ticks(nth: u64) -> Duration {
        pulse::TICK * nth as u32
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
                height(&thought(text), 80, ticks(9)),
                rows,
                "a thought of {text:?} costs the thread {rows} rows"
            );
        }
    }

    #[test]
    fn the_unfurl_gains_a_row_a_tick_up_to_the_most_a_thought_costs() {
        let unfurled: Vec<usize> = (0..9).map(|nth| rows(ticks(nth))).collect();
        assert_eq!(
            unfurled,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 8],
            "the pane's clock is its head on the first tick and a row more a tick, \
             never past the most a thought costs"
        );
    }

    #[test]
    fn the_fold_holds_a_tick_and_then_gives_a_row_back() {
        let folded: Vec<usize> = (0..6).map(|nth| closing(ticks(nth))).collect();
        assert_eq!(
            folded,
            vec![5, 4, 3, 2, 1, 0],
            "a finished thought holds its height for a tick and then folds away"
        );
    }

    #[test]
    fn a_closing_pane_is_as_tall_as_the_clock_says_and_no_taller() {
        let mut closed = thought("a\nb\nc\nd\ne\nf\ng\n");
        closed.closed = Some(ticks(2));
        assert_eq!(
            height(&closed, 80, ticks(2)),
            5,
            "the hold is the floor a pane settles at"
        );
        assert_eq!(
            height(&closed, 80, ticks(4)),
            3,
            "and the fold is the clock's, whatever the text is worth"
        );
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
