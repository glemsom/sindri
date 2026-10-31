//! One entry as rows: the shapes a turn takes in the transcript column — a
//! prompt on its band, an answer under its marker, a call under its line, a
//! note at the turn's own edge — and the gutter that says what belongs to
//! what. A frame asks for the rows its window holds and is told how many the
//! entry is worth: the rows of an answer nobody sees are counted and not built.

use std::time::Duration;

use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::entry::{Entry, NoteKind, Prompt};
use crate::markdown;
use crate::picture::Store;
use crate::pulse;
use crate::render::call;
use crate::render::pictures::Laid;
use crate::theme;

/// The columns a turn's body hangs off the turn's own edge: the answer and the
/// calls it made start here, so a call reads as belonging to its turn.
pub const GUTTER: u16 = 2;

/// One entry as the frame lays it out: the rows, and the pictures among them —
/// the row each block starts at, the column of the entry's body it hangs off,
/// and what the store measured.
pub struct Rows {
    /// The rows the window asked for, re-based to the first of them.
    pub rows: Vec<Line<'static>>,
    pub pictures: Vec<Laid>,
    /// How many rows the whole entry is worth, gap excluded: what the frame
    /// places the window against, whether or not it built them.
    pub total: usize,
}

/// Which rows of an entry a frame wants: `skip` rows into it, and `want` of them
/// from there. An entry's block is the gap before it and its own rows, and the
/// rows outside the ask are counted and not built — that is what makes a frame
/// cost the window and not the answer.
#[derive(Debug, Clone, Copy)]
pub struct Want {
    pub skip: usize,
    pub want: usize,
}

impl Want {
    /// Every row of the entry: a caller that is measuring it rather than drawing
    /// a window of it.
    pub const ALL: Want = Want {
        skip: 0,
        want: usize::MAX,
    };
}

/// What a body is laid out in: the columns it has, the frame's clock and what
/// measures a picture. One value because every entry's rows are made of the same
/// three, and an answer that had each of them as an argument had one too many.
#[derive(Clone, Copy)]
struct Body<'a> {
    columns: u16,
    now: Duration,
    store: Option<&'a Store>,
}

/// How a streamed answer stands: whether it has stopped, whether it stopped
/// badly, and the duration it stopped at — the three the marker is drawn from.
#[derive(Clone, Copy)]
struct State {
    done: bool,
    failed: bool,
    took: Option<Duration>,
}

impl Rows {
    /// The rows of an entry that places no picture: a prompt, a call, a note, a
    /// thought folded away.
    fn plain(rows: Vec<Line<'static>>, want: Want) -> Rows {
        let total = rows.len();
        Rows {
            rows: kept(rows, want),
            pictures: Vec::new(),
            total,
        }
    }
}

/// The rows of `rows` the ask holds, re-based to the first of them: an entry
/// that lays its body out whole — a prompt, a call, a thought — cuts the window
/// it was asked for here, because laying one out is cheap and laying it out
/// twice is not the point.
fn kept(rows: Vec<Line<'static>>, want: Want) -> Vec<Line<'static>> {
    let to = want.skip.saturating_add(want.want).min(rows.len());
    if want.skip >= to {
        return Vec::new();
    }
    rows.into_iter().take(to).skip(want.skip).collect()
}

/// The rows one entry takes at `width`, or the rows of it the ask holds. `store`
/// is what measures a picture, and it is handed down rather than built here: the
/// row count a frame lays out is the store's answer, never a file the renderer
/// read.
pub fn entry_rows(
    entry: &Entry,
    width: u16,
    now: Duration,
    store: Option<&Store>,
    want: Want,
) -> Rows {
    let width = width.max(1);
    // The gutter's columns are not the body's: a call and its output wrap
    // where the column ends, not where the terminal does.
    let body = width.saturating_sub(GUTTER);
    match entry {
        Entry::Prompt(prompt) => prompt_rows(prompt, width, want),
        Entry::Answer {
            text,
            done,
            failed,
            took,
            ..
        } => answer_rows(
            text,
            State {
                done: *done,
                failed: *failed,
                took: *took,
            },
            Body {
                columns: body,
                now,
                store,
            },
            want,
        ),
        Entry::Reasoning {
            text, done, open, ..
        } => {
            // A thought that is over is one row until the reader opens it,
            // and then it is the text it held.
            if *done && !*open {
                return Rows::plain(
                    gutter(vec![Line::from(Span::styled("⌁ reasoned", theme::faint()))]),
                    want,
                );
            }
            let written = markdown::lines(text, body, store);
            let mut rows = written.rows;
            // A thought still being thought is shown as its last rows, and a
            // picture above them is not on screen to be drawn. A thought is one
            // entry and not an answer, so it is laid out whole and cut here.
            let mut dropped = 0;
            if !*done && !*open {
                dropped = rows.len().saturating_sub(3);
                rows = rows.split_off(dropped);
            }
            // What the entry is worth is the rows it draws: a thought still
            // arriving is the three at its end, whatever it will be worth whole.
            let total = rows.len();
            Rows {
                pictures: laid(written.pictures, 0, dropped, want),
                rows: gutter(kept(rows, want)),
                total,
            }
        }
        Entry::Call(call) => Rows::plain(gutter(call::rows(call, body, now)), want),
        Entry::Note(note) => {
            let (marker, style) = match note.kind {
                NoteKind::Error => ("! ", theme::bad()),
                NoteKind::Info => ("· ", theme::faint()),
            };
            // A note is the frame speaking, not the turn: it keeps the
            // turn's own edge. The marker takes its columns out of the row
            // before the words do, or the last word of a wrapped note is cut
            // in half.
            Rows::plain(
                markdown::text::wrap(
                    markdown::text::glyphs(&[Span::styled(note.text.clone(), style)]),
                    marker,
                    style,
                    (width as usize).saturating_sub(marker.chars().count()),
                ),
                want,
            )
        }
    }
}

/// The pictures a written body placed, re-based to the rows the window holds:
/// `above` is the rows of the entry before the body — the answer's marker and
/// nothing for a thought — `dropped` the body's rows the frame dropped off its
/// top, the ask is the rows of the entry it wants, the gutter is the body's
/// first two columns, and the writer's own inset is the quote the block sits in.
/// A block the window's top row sits inside is kept, with the rows of it that
/// were skipped counted as `cut`: a picture is whole in the window or drawn
/// nowhere, and that is the row count's answer to give, not the graphic's.
fn laid(written: Vec<markdown::Picture>, above: usize, dropped: usize, want: Want) -> Vec<Laid> {
    written
        .into_iter()
        .filter_map(|picture| {
            let at = (picture.at + above).checked_sub(dropped)?;
            let above = want.skip.saturating_sub(at).min(picture.rows);
            (above < picture.rows).then(|| Laid {
                row: at.saturating_sub(want.skip),
                column: GUTTER + picture.inset,
                cut: above,
                block: picture.rows - above,
                copy: picture.copy,
                src: picture.src,
                worth: picture.worth,
            })
        })
        .collect()
}

/// The rows a markdown body hangs below: the answer's own marker row, which is
/// a row of the entry like any other.
const MARKER_ROWS: usize = 1;

/// A turn's answer: the marker at its head, and the prose beneath it. The
/// marker is the run's own clock — a braille frame and a duration that counts
/// up while the answer streams, and then the outcome and the duration it
/// stopped at, which never move again.
fn answer_rows(text: &str, state: State, body: Body<'_>, want: Want) -> Rows {
    let State { done, failed, took } = state;
    let Body {
        columns,
        now,
        store,
    } = body;
    let (glyph, style, elapsed) = match (done, failed) {
        (false, _) => (pulse::frame(now), theme::prompt(), now),
        (true, true) => ("✗", theme::bad(), took.unwrap_or_default()),
        (true, false) => ("✓", theme::good(), took.unwrap_or_default()),
    };
    let marker = Line::from(vec![
        Span::styled(format!("{glyph} "), style),
        Span::styled(pulse::said(elapsed), theme::faint()),
    ]);
    // The marker is a row of the entry, so it takes the first of what the ask
    // holds and the answer's own rows start one row into it.
    let mut rows = Vec::new();
    let mut skip = want.skip;
    let mut count = want.want;
    if skip == 0 && count > 0 {
        rows.push(marker);
        count -= 1;
    } else if skip > 0 {
        skip -= MARKER_ROWS;
    }
    // The answer is laid out for the rows the window holds and counted for the
    // rest: this is the whole of what a long answer costs a frame.
    let written = markdown::lines_in(text, columns, store, skip..skip.saturating_add(count));
    rows.extend(written.rows);
    Rows {
        rows: gutter(rows),
        pictures: laid(written.pictures, MARKER_ROWS, 0, want),
        total: MARKER_ROWS + written.total,
    }
}

/// How many rows an entry is worth at `width`, counted and never built: the
/// number a window is placed against, a scroll-back moves by and the new-rows
/// badge adds up.
pub fn entry_height(entry: &Entry, width: u16, now: Duration, store: Option<&Store>) -> usize {
    let width = width.max(1);
    let body = width.saturating_sub(GUTTER);
    match entry {
        // An answer is the one entry whose rows can outrun any window, so its
        // count is its own: the markdown is counted and no fence in it painted.
        Entry::Answer { text, .. } => MARKER_ROWS + markdown::height(text, body, store),
        _ => entry_rows(entry, width, now, store, Want::ALL).total,
    }
}

/// A turn's opening: the header that names the human, and the prompt itself at
/// full strength across a band of its own.
fn prompt_rows(prompt: &Prompt, width: u16, want: Want) -> Rows {
    let band = width.saturating_sub(GUTTER);
    let mut rows = vec![Line::from(vec![
        Span::styled("YOU", theme::prompt()),
        Span::styled(format!("  {}  {}", prompt.at, prompt.cwd), theme::faint()),
    ])];
    let mut words = markdown::text::wrap(
        markdown::text::glyphs(&[Span::styled(prompt.text.clone(), theme::plain())]),
        "",
        theme::plain(),
        band as usize,
    );
    for (index, row) in words.iter_mut().enumerate() {
        // The marker column is fixed: a wrapped prompt keeps its own columns.
        let marker = if index == 0 { "› " } else { "  " };
        row.spans.insert(0, Span::styled(marker, theme::prompt()));
    }
    rows.extend(words.into_iter().map(|row| banded(row, width)));
    Rows::plain(rows, want)
}

/// A turn's body, two columns in from the turn's own edge.
fn gutter(rows: Vec<Line<'static>>) -> Vec<Line<'static>> {
    let pad = " ".repeat(GUTTER as usize);
    rows.into_iter()
        .map(|mut row| {
            row.spans.insert(0, Span::raw(pad.clone()));
            row
        })
        .collect()
}

/// One row of the prompt's band: the words as they are, the surface filled to
/// the column's edge so the prompt reads as a block and not as a line.
fn banded(row: Line<'static>, width: u16) -> Line<'static> {
    let used: usize = row
        .spans
        .iter()
        .map(|span| span.content.as_ref().width())
        .sum();
    let mut spans: Vec<Span<'static>> = row
        .spans
        .into_iter()
        .map(|span| {
            let style = span.style.bg(theme::PANEL);
            Span::styled(span.content, style)
        })
        .collect();
    spans.push(Span::styled(
        " ".repeat((width as usize).saturating_sub(used)),
        theme::surface(),
    ));
    Line::from(spans)
}
