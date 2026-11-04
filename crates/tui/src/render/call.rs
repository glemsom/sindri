//! One tool call's rows: its status, its line count, its command on the one row
//! it is given, and whatever the reader has opened of it. The command is drawn
//! flattened and cut — a row's reading of it, never the command itself, which
//! the entry keeps as the model wrote it. A call is expandable whether or not it
//! spilled — the spill file is an optimisation on top of that, never the only
//! way back to the output — and an expansion reads the entry's bounded tail,
//! so it costs the cap rather than the stream.

use std::time::Duration;

use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::driver::Side;
use crate::entry::{Call, CallOver};
use crate::markdown;
use crate::pulse;
use crate::theme;

/// The columns a call's status takes before its command, so a running call and
/// the same call collapsed put the command in the same place.
const STATUS: usize = 8;
/// The columns a call's duration takes after its status: a finished call and a
/// running one put the command in the same place, and the duration counts up
/// or stands still in a field that does not move.
const DURATION: usize = 6;

pub fn rows(call: &Call, width: u16, now: Duration) -> Vec<Line<'static>> {
    let command = call
        .command
        .as_deref()
        .unwrap_or("(the call could not be read)");
    // A marker column, then a status column of fixed width: a call that is
    // still running and the same call collapsed leave the command where it
    // was, and the glyph says GOOD or BAD without a word of hue alone.
    // A refused call's reason is longer than the column and
    // pushes its own command right; a call that never ran has no earlier row
    // for that to disagree with.
    let (glyph, status, accent) = match (&call.over, &call.skill) {
        // A load has no exit and no clock to run: its row says what it made
        // true, and what it read is behind the fold. A repeat is neither a
        // success nor a failure — nothing moved — so it gets a dot rather than
        // a tick or a cross.
        (Some(_), Some(skill)) if skill.in_force() => {
            ("✓", skill.said().to_string(), theme::good())
        }
        (Some(_), Some(skill)) if skill.failed() => ("✗", skill.said().to_string(), theme::bad()),
        (Some(_), Some(skill)) => ("·", skill.said().to_string(), theme::faint()),
        (None, Some(_)) => ("⏺", "loading".to_string(), theme::faint()),
        (None, None) => ("⏺", "running".to_string(), theme::faint()),
        (Some(CallOver::Exited(0)), None) => ("✓", "exit 0".to_string(), theme::good()),
        (Some(CallOver::Exited(code)), None) => ("✗", format!("exit {code}"), theme::bad()),
        (Some(CallOver::TimedOut), None) => ("✗", "timed out".to_string(), theme::bad()),
        (Some(CallOver::Cancelled), None) => ("✗", "cancelled".to_string(), theme::bad()),
        (Some(CallOver::Refused(detail)), None) => {
            ("✗", format!("sandbox: {detail}"), theme::bad())
        }
        // A call with nothing to watch: it was announced and the loop is past
        // it. The dot is the same one a repeat gets — nothing here says a human
        // looked, because nothing here can know.
        (Some(CallOver::Announced), None) => ("·", "announced".to_string(), theme::faint()),
    };
    // An open call says so with its marker: the row a key opens is the row
    // that shows it is open.
    let marker = if call.expanded() { "▾ " } else { "⎿ " };
    // How long the call has been running, or how long it took: frozen at the
    // duration it ended on, so a finished call is not still counting.
    // A load and an `open` spend no clock: the column stays, empty, so the
    // three kinds of call still line their names up on one column. An `open`
    // that showed the zero it measured would be a duration nothing took.
    let took = if call.skill.is_some() || call.over == Some(CallOver::Announced) {
        String::new()
    } else {
        pulse::said(call.elapsed(now))
    };
    let mut spans = vec![
        Span::styled(marker, theme::faint()),
        Span::styled(format!("{glyph} {status:<STATUS$} "), accent),
        Span::styled(format!("{took:<DURATION$} "), theme::faint()),
    ];
    // What is behind the fold, so a collapsed call is not a dead end: the
    // count and the duration are the two facts that say whether opening it is
    // worth a key.
    if call.lines() > 0 {
        spans.push(Span::styled(behind(call.lines()), theme::faint()));
        spans.push(Span::raw(" "));
    }
    // The command is drawn on the one row it has: every run of whitespace in
    // it is one space, so a heredoc does not draw the frame a row out of place,
    // and a command the column cannot hold ends in `…` rather than running past
    // the edge. The room left for it is measured from the spans above rather
    // than assumed from the two fixed columns, so a refused call — whose reason
    // is longer than its field — still cuts its command at the column's edge
    // instead of past it.
    let room = usize::from(width).saturating_sub(used_cells(&spans));
    spans.push(Span::styled(one_row(command, room), theme::plain()));
    let mut rows = vec![Line::from(spans)];

    let shown = if call.expanded() {
        if call.dropped() > 0 {
            let keep = width.saturating_sub(2) as usize;
            let lost = format!("… {} earlier lines not kept", call.dropped());
            rows.push(Line::from(Span::styled(
                markdown::text::clip(&lost, keep),
                theme::faint(),
            )));
        }
        call.kept().iter().collect::<Vec<_>>()
    } else if call.running() {
        call.tail.iter().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    rows.extend(
        shown
            .into_iter()
            .map(|(side, line)| output(side, line, width)),
    );

    if let Some(path) = call.spill_file() {
        let keep = width.saturating_sub(2) as usize;
        let offer = format!("… output too large · Ctrl-O opens {}", path.display());
        rows.push(Line::from(Span::styled(
            markdown::text::clip(&offer, keep),
            theme::faint(),
        )));
    }
    rows
}

/// One line of a call's output. stdout and stderr differ by a glyph as well as
/// by hue: a line read on a terminal that lost its colour still tells a
/// warning from a result.
fn output(side: &Side, line: &str, width: u16) -> Line<'static> {
    let (marker, style) = match side {
        Side::Stdout => ("│", theme::faint()),
        Side::Stderr => ("⚠", theme::bad()),
    };
    let keep = width.saturating_sub(4) as usize;
    Line::from(vec![
        Span::styled(format!("  {marker} "), style),
        Span::styled(markdown::text::clip(line, keep), style),
    ])
}

/// The cells a row's spans already take, so the command is given what is left
/// of the column: a status the sandbox made longer than its field pushes the
/// command right, and the cut follows it rather than the field it was meant to
/// fit.
fn used_cells(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

/// A command on the one row a call is given: every run of whitespace in it one
/// space, and a `…` where the column ends before the command does. The row is
/// then an excerpt of the command and says so; the command itself is untouched,
/// so the entry, the record and an export still hold every line of it.
fn one_row(command: &str, room: usize) -> String {
    let flat = command.split_whitespace().collect::<Vec<_>>().join(" ");
    if UnicodeWidthStr::width(flat.as_str()) <= room {
        return flat;
    }
    if room == 0 {
        return String::new();
    }
    let cut = markdown::text::clip(&flat, room - 1);
    format!("{}…", cut.trim_end())
}

fn behind(lines: usize) -> String {
    if lines == 1 {
        "1 line".to_string()
    } else {
        format!("{lines} lines")
    }
}

#[cfg(test)]
mod tests {
    use super::{behind, one_row};

    #[test]
    fn a_count_says_line_or_lines_and_never_lines_of_one() {
        assert_eq!(behind(1), "1 line");
        assert_eq!(behind(14), "14 lines");
        assert_eq!(behind(400), "400 lines");
    }

    #[test]
    fn every_run_of_whitespace_in_a_command_is_one_space_on_its_row() {
        assert_eq!(
            one_row("for f in a b; do\n\t  echo \"$f\"\ndone", 80),
            "for f in a b; do echo \"$f\" done",
            "a line break, a tab and a run of spaces all separate two words once"
        );
        assert_eq!(
            one_row("  cargo   test  ", 80),
            "cargo test",
            "and the ends of the command hold no cell of the row"
        );
    }

    #[test]
    fn a_command_wider_than_its_room_is_cut_to_an_ellipsis_that_still_fits() {
        assert_eq!(
            one_row("cargo test --workspace", 16),
            "cargo test --wo…",
            "the cut keeps the room it was given and says it is an excerpt"
        );
        assert_eq!(
            one_row("cargo test", 10),
            "cargo test",
            "a command that fits is never marked as cut"
        );
        assert_eq!(
            one_row("cargo test", 0),
            "",
            "and a column with no room left draws nothing rather than past itself"
        );
    }
}
