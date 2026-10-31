//! The rows one frame of the transcript lays out, and what the reader's place
//! in the thread costs: the entries it measures, the entries it never touches,
//! and the ends it clamps at.

use std::path::{Path, PathBuf};

use ratatui::text::Line;

use crate::app::App;
use crate::entry::Entry;
use crate::render::pictures::Laid;
use crate::render::rows::entry_rows;
use crate::window::{Ask, Below};

/// One frame's layout: the rows it built, where each entry's block of them
/// starts, and the rows a click can land on. A block is the gap before an entry
/// and the entry itself, because that is what a reader scrolls over.
pub(crate) struct Frame<'a> {
    pub(crate) app: &'a mut App,
    pub(crate) width: u16,
    pub(crate) rows: Vec<Line<'static>>,
    pub(crate) marks: Vec<(usize, usize)>,
    /// How many entries this frame laid out or asked the height of.
    pub(crate) measured: usize,
    pub(crate) spill: Vec<(usize, PathBuf)>,
    pub(crate) expand: Vec<(usize, usize)>,
    /// The pictures this frame laid out: a row map like `spill` and `expand`,
    /// because a picture is drawn where its rows are and is nothing else.
    pub(crate) pictures: Vec<Laid>,
}

impl<'a> Frame<'a> {
    pub(crate) fn new(app: &'a mut App, width: u16) -> Self {
        Frame {
            app,
            width,
            rows: Vec::new(),
            marks: Vec::new(),
            spill: Vec::new(),
            expand: Vec::new(),
            pictures: Vec::new(),
            measured: 0,
        }
    }

    /// Lays out from `skip` rows into `entry`'s block until the column is full,
    /// and answers with the rows below the window: the ones built past it and
    /// every entry after them, measured but never laid out.
    pub(crate) fn hold(&mut self, entry: usize, skip: usize, count: usize, height: usize) -> usize {
        let mut index = entry;
        while index < count && self.rows.len() < height {
            self.build(index, if index == entry { skip } else { 0 });
            index += 1;
        }
        self.rows.len().saturating_sub(height) + self.below(index, count)
    }

    /// The rows of `first..count`: measured as what an entry is worth, never
    /// laid out. A run appends at the tail, so a count is added to when the only
    /// change is at the newest entry, and walked again when it is not — a page
    /// back on a long thread costs a page, not the thread.
    fn below(&mut self, first: usize, count: usize) -> usize {
        let newest = count.saturating_sub(1);
        let rows = match count.checked_sub(1) {
            Some(last) => self.app.measure(last, self.width),
            None => 0,
        };
        let mark = Below {
            width: self.width,
            first,
            newest,
            rows,
        };
        let kept = self.app.window.below.take();
        let counted = match kept {
            Some((last, below)) if last == mark => below,
            Some((last, below)) if self.grown(&last, &mark) => {
                below + self.rows_of(last.newest + 1, count)
            }
            _ => self.rows_of(first, count),
        };
        self.app.window.below = Some((mark, counted));
        counted
    }

    /// Whether the only thing that happened is that rows arrived at the tail:
    /// the same window, the same width, and the entry that was newest unchanged
    /// but no longer the newest.
    fn grown(&mut self, last: &Below, mark: &Below) -> bool {
        last.width == mark.width
            && last.first == mark.first
            && last.newest < mark.newest
            && self.app.measure(last.newest, self.width) == last.rows
    }

    /// What the entries in `first..count` are worth in rows, gap included.
    fn rows_of(&mut self, first: usize, count: usize) -> usize {
        let mut rows = 0;
        for index in first..count {
            self.measured += 1;
            rows += gap(&self.app.entries()[index]) + self.app.measure(index, self.width);
        }
        rows
    }

    /// Lays the newest rows out back to a screenful, and answers with the entry
    /// the window starts at.
    pub(crate) fn tail(&mut self, count: usize, height: usize) -> usize {
        let mut first = count;
        let mut acc = 0;
        while first > 0 && acc < height {
            first -= 1;
            acc += gap(&self.app.entries()[first]) + self.app.measure(first, self.width);
        }
        for index in first..count {
            self.build(index, 0);
        }
        first
    }

    /// One entry's block, skipping `skip` of its rows. The rows a click can land
    /// on are recorded where they were built, in the frame's own coordinates.
    fn build(&mut self, index: usize, skip: usize) {
        self.measured += 1;
        let at = self.rows.len();
        self.marks.push((index, at));
        let entry = &self.app.entries()[index];
        let gap = gap(entry);
        let store = self.app.pictures();
        let done = entry_rows(entry, self.width, self.app.now(), store);
        if let Some(path) = spill_of(entry) {
            self.spill
                .push((at + gap + done.rows.len().saturating_sub(1), path));
        }
        if entry.expandable() {
            self.expand.push((at + gap, index));
        }
        // A picture is drawn at the row its block starts at. A block the window
        // skipped into is cut at the window's top row and kept: its rows are the
        // frame's from there, and whether any of it is drawn is the window's
        // answer and not this one's.
        for picture in done.pictures {
            let offset = gap + picture.row;
            let above = skip.saturating_sub(offset).min(picture.block);
            if above == picture.block {
                continue;
            }
            self.pictures.push(Laid {
                row: at + offset.saturating_sub(skip),
                cut: above,
                block: picture.block - above,
                ..picture
            });
        }
        let mut block: Vec<Line<'static>> = (0..gap).map(|_| Line::default()).collect();
        block.extend(done.rows);
        block.drain(..skip.min(block.len()));
        self.rows.extend(block);
    }
}

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

/// The entry the window's first row belongs to.
pub(crate) fn place(marks: &[(usize, usize)], start: usize) -> usize {
    marks
        .iter()
        .rev()
        .find(|(_, offset)| *offset <= start)
        .map(|(entry, _)| *entry)
        .unwrap_or_default()
}

pub(crate) fn offset_of(marks: &[(usize, usize)], entry: usize) -> usize {
    marks
        .iter()
        .find(|(at, _)| *at == entry)
        .map(|(_, offset)| *offset)
        .unwrap_or_default()
}

/// The offset a frame draws at: the built rows renumbered to the window.
pub(crate) fn drawn<T>(marks: Vec<(usize, T)>, start: usize, visible: usize) -> Vec<(usize, T)> {
    marks
        .into_iter()
        .filter_map(|(row, at)| {
            row.checked_sub(start)
                .filter(|row| *row < visible)
                .map(|row| (row, at))
        })
        .collect()
}

/// Two blank rows between turns, none inside one.
const TURN_GAP: usize = 2;

/// The blank rows before an entry: two when a turn opens, and none inside one.
/// The first entry of the thread opens nothing, so it says nothing.
pub(crate) fn gap(entry: &Entry) -> usize {
    match entry {
        Entry::Prompt(_) => TURN_GAP,
        _ => 0,
    }
}

fn spill_of(entry: &Entry) -> Option<PathBuf> {
    match entry {
        Entry::Call(call) => call.spill_file().map(Path::to_path_buf),
        _ => None,
    }
}
