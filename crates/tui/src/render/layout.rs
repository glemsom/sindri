//! The rows one frame of the transcript lays out, and what the reader's place
//! in the thread costs: the entries it measures, the entries it never touches,
//! and the ends it clamps at (ADR-0013, issue #45).

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
    pub(crate) measured: usize,
    pub(crate) spill: Vec<(usize, PathBuf)>,
    pub(crate) expand: Vec<(usize, usize)>,
    /// The pictures this frame laid out: a row map like `spill` and `expand`,
    /// because a picture is drawn where its rows are and is nothing else
    /// (ADR-0016 §6, §9).
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
    /// every entry after them, measured but never laid out (issue #45).
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
    /// back on a long thread costs a page, not the thread (issue #45).
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
        // answer and not this one's (ADR-0016 §6).
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
/// page costs a page (issue #45).
pub(crate) fn resolve(app: &mut App, width: u16, count: usize, ask: Ask, follow: &mut bool) {
    match ask {
        Ask::Home => {
            *follow = false;
            app.window.at = (0, 0);
        }
        Ask::End => *follow = true,
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

/// Moves the window's top `rows` rows back into the thread, across as few
/// entries as it crosses: the first turn is the ceiling (issue #45).
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
/// followed (issue #45).
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
