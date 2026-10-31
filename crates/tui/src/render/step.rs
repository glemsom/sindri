//! Where the window goes: what a reader's ask lands on — a page back or forward,
//! the first turn, the tail — and how far apart two places in the thread are.
//! Moving by rows measures only the entries it crosses, so a page costs a page.

use crate::app::App;
use crate::window::Ask;

use super::layout::gap;

/// Carries out what the reader asked for: a page back or forward, the first
/// turn, or the tail. Moving by rows measures only the entries it crosses, so a
/// page costs a page.
pub(crate) fn resolve(app: &mut App, width: u16, count: usize, ask: Ask, follow: &mut bool) {
    // What a height change owes the place is paid before the reader's own
    // request is answered: a following window has no place of its own to move,
    // and an ask is the reader's and comes last.
    let moved = app.window.take_shift();
    if moved != 0 && !*follow {
        app.window.at = step(app, width, app.window.at, moved);
    }
    match ask {
        Ask::Home => {
            *follow = false;
            app.window.at = (0, 0);
        }
        Ask::End => *follow = true,
        // The entry itself, and its own row first: the output an expansion
        // opens grows below the row the reader pressed `Enter` on.
        Ask::Hold(entry) => {
            *follow = false;
            app.window.at = (entry, 0);
        }
        Ask::Rows(rows) if rows > 0 => {
            *follow = false;
            app.window.at = back(app, width, app.window.at, rows.unsigned_abs());
        }
        Ask::Rows(rows) if rows < 0 && !*follow => {
            match forward(app, width, app.window.at, rows.unsigned_abs()) {
                Some(at) => app.window.at = at,
                None => *follow = true,
            }
        }
        _ => {}
    }
    if app.window.at.0 >= count {
        *follow = true;
    }
}

/// The place `delta` rows away: `back` into the thread and `forward` towards
/// its tail. A shift that runs past the newest row leaves the place alone
/// rather than following the tail, because the place is the reader's and the
/// tail is `Ask::End`'s.
fn step(app: &mut App, width: u16, at: (usize, usize), delta: isize) -> (usize, usize) {
    let rows = delta.unsigned_abs();
    if delta > 0 {
        back(app, width, at, rows)
    } else {
        forward(app, width, at, rows).unwrap_or(at)
    }
}

/// Moves the window's top `rows` rows back into the thread, across as few
/// entries as it crosses: the first turn is the ceiling.
pub(crate) fn back(app: &mut App, width: u16, at: (usize, usize), rows: usize) -> (usize, usize) {
    let (mut entry, mut row) = at;
    let mut left = rows;
    loop {
        let step = row.min(left);
        row -= step;
        left -= step;
        if left == 0 {
            return (entry, row);
        }
        if entry == 0 {
            return (0, 0);
        }
        entry -= 1;
        row = gap(&app.entries()[entry]) + app.measure(entry, width);
    }
}

/// Moves the window's top `rows` rows towards the tail, or `None` when it runs
/// past the newest row: past the newest row is the tail, and the tail is
/// followed.
pub(crate) fn forward(
    app: &mut App,
    width: u16,
    at: (usize, usize),
    rows: usize,
) -> Option<(usize, usize)> {
    let count = app.entries().len();
    let (mut entry, mut row) = at;
    let mut left = rows;
    while left > 0 {
        if entry >= count {
            return None;
        }
        let rest = gap(&app.entries()[entry]) + app.measure(entry, width);
        let rest = rest.saturating_sub(row);
        if left < rest {
            return Some((entry, row + left));
        }
        left -= rest;
        entry += 1;
        row = 0;
    }
    (entry < count).then_some((entry, row))
}

/// The rows from one place in the thread to another, positive towards the
/// tail. A mark is drawn on the glass and covers a place in the thread, so this
/// is what says how far that place moved when the window did — the note a copy
/// leaves at the tail pushes every row above it up — and only the entries
/// between the two places are measured for it.
pub(crate) fn rows_between(
    app: &mut App,
    width: u16,
    from: (usize, usize),
    to: (usize, usize),
) -> isize {
    let forward = from <= to;
    let (back, front) = if forward { (from, to) } else { (to, from) };
    let rows = if back.0 == front.0 {
        front.1.saturating_sub(back.1)
    } else {
        let mut rows = entry_height(app, width, back.0).saturating_sub(back.1);
        for index in back.0 + 1..front.0 {
            rows += entry_height(app, width, index);
        }
        rows + front.1
    };
    let rows = rows as isize;
    if forward { rows } else { -rows }
}

/// The rows one entry is drawn on, gap and all: the measure the window moves
/// by. An entry the thread no longer has — a `/new` between two frames — is
/// worth nothing rather than a panic.
fn entry_height(app: &mut App, width: u16, index: usize) -> usize {
    if index >= app.entries().len() {
        return 0;
    }
    gap(&app.entries()[index]) + app.measure(index, width)
}
