//! One tool call's rows: its status, its line count, and whatever the reader
//! has opened of it (ADR-0011 §9, §4). A call is expandable whether or not it
//! spilled — the spill file is an optimisation on top of that, never the only
//! way back to the output — and an expansion reads the entry's bounded tail,
//! so it costs the cap rather than the stream (issue #44).

use std::time::Duration;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::driver::{CallOver, Side};
use crate::entry::Call;
use crate::markdown;
use crate::pulse;
use crate::theme;

/// The columns a call's status takes before its command, so a running call and
/// the same call collapsed put the command in the same place.
const STATUS: usize = 8;
/// The columns a call's duration takes after its status: a finished call and a
/// running one put the command in the same place, and the duration counts up
/// or stands still in a field that does not move (ADR-0011 §4).
const DURATION: usize = 6;

pub fn rows(call: &Call, width: u16, now: Duration) -> Vec<Line<'static>> {
    let command = call
        .command
        .as_deref()
        .unwrap_or("(the call could not be read)");
    // A marker column, then a status column of fixed width: a call that is
    // still running and the same call collapsed leave the command where it
    // was, and the glyph says GOOD or BAD without a word of hue alone
    // (ADR-0016). A refused call's reason is longer than the column and
    // pushes its own command right; a call that never ran has no earlier row
    // for that to disagree with.
    let (glyph, status, accent) = match &call.over {
        None => ("⏺", "running".to_string(), theme::faint()),
        Some(CallOver::Exited(0)) => ("✓", "exit 0".to_string(), theme::good()),
        Some(CallOver::Exited(code)) => ("✗", format!("exit {code}"), theme::bad()),
        Some(CallOver::TimedOut) => ("✗", "timed out".to_string(), theme::bad()),
        Some(CallOver::Cancelled) => ("✗", "cancelled".to_string(), theme::bad()),
        Some(CallOver::Refused(detail)) => ("✗", format!("sandbox: {detail}"), theme::bad()),
    };
    // An open call says so with its marker: the row a key opens is the row
    // that shows it is open (issue #44).
    let marker = if call.expanded() { "▾ " } else { "⎿ " };
    // How long the call has been running, or how long it took: frozen at the
    // duration it ended on, so a finished call is not still counting.
    let took = pulse::said(call.elapsed(now));
    let mut spans = vec![
        Span::styled(marker, theme::faint()),
        Span::styled(format!("{glyph} {status:<STATUS$} "), accent),
        Span::styled(format!("{took:<DURATION$} "), theme::faint()),
    ];
    // What is behind the fold, so a collapsed call is not a dead end: the
    // count and the duration are the two facts that say whether opening it is
    // worth a key (issue #44).
    if call.lines() > 0 {
        spans.push(Span::styled(behind(call.lines()), theme::faint()));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(command.to_string(), Style::new()));
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
/// warning from a result (ADR-0016).
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

fn behind(lines: usize) -> String {
    if lines == 1 {
        "1 line".to_string()
    } else {
        format!("{lines} lines")
    }
}

#[cfg(test)]
mod tests {
    use super::behind;

    #[test]
    fn a_count_says_line_or_lines_and_never_lines_of_one() {
        assert_eq!(behind(1), "1 line");
        assert_eq!(behind(14), "14 lines");
        assert_eq!(behind(400), "400 lines");
    }
}
