//! The rows one frame of the transcript lays out, and what the reader's place
//! in the thread costs: the entries it measures, the entries it never touches,
//! the rows of an entry the window does not hold, which are counted and never
//! built, and the ends it clamps at.

use std::path::{Path, PathBuf};

use ratatui::text::Line;

use crate::app::App;
use crate::entry::Entry;
use crate::render::pictures::Laid;
use crate::render::rows::{Want, entry_rows};
use crate::window::Below;

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
    /// How many rows this frame built: the window's own, and none of the answer
    /// above or below it. A frame costs what the window holds.
    pub(crate) built: usize,
    /// How many rows of the first entry's block the window's top row skipped
    /// over: the rows of the answer above the window, which the frame counted
    /// and did not build, and which the reader's place is still measured by.
    pub(crate) top_skip: usize,
    /// The last entry built and how many rows of its block were built: the rows
    /// of it that the window did not hold are below the window like the entries
    /// after it, and no other row of the frame says so.
    last: Option<(usize, usize)>,
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
            built: 0,
            top_skip: 0,
            last: None,
        }
    }

    /// Lays out from `skip` rows into `entry`'s block until the column is full,
    /// and answers with the rows below the window: the rows of the entry that
    /// filled the column and were not built, and every entry after them, measured
    /// but never built.
    pub(crate) fn hold(&mut self, entry: usize, skip: usize, count: usize, height: usize) -> usize {
        let mut index = entry;
        while index < count && self.rows.len() < height {
            let want = height - self.rows.len();
            self.build(index, if index == entry { skip } else { 0 }, want);
            index += 1;
        }
        // What is below the window is the rows of the entry that filled the
        // column and were not built, and every entry after it: counted whether
        // or not the frame built a row of it, and counted from the height kept
        // for that entry rather than walked for again.
        let rest = match self.last {
            Some((last, built)) => {
                let space = gap(&self.app.entries()[last]);
                let height = self.app.measure(last, self.width);
                (space + height).saturating_sub(built)
            }
            None => 0,
        };
        rest + self.below(index, count)
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
        // The window's top row sits somewhere inside `first`'s block, so the
        // rows above it are skipped there and nowhere else: the entries below it
        // are built from their own first row.
        let mut skip = acc.saturating_sub(height);
        for index in first..count {
            if self.rows.len() >= height {
                break;
            }
            let want = height - self.rows.len();
            self.build(index, skip, want);
            skip = 0;
        }
        first
    }

    /// One entry's block, skipping `skip` of its rows and building `want` of
    /// them: the rows a click can land on are recorded where they were built, in
    /// the frame's own coordinates. The rows outside the ask are counted, not
    /// built — the entry's block is what the window holds of it.
    fn build(&mut self, index: usize, skip: usize, want: usize) {
        self.measured += 1;
        let at = self.rows.len();
        if self.marks.is_empty() {
            self.top_skip = skip;
        }
        self.marks.push((index, at));
        let entry = &self.app.entries()[index];
        let gap = gap(entry);
        // The gap is part of the entry's block, and blank rows cost nothing to
        // build: what is skipped first is the gap.
        let gap_rows = gap.saturating_sub(skip).min(want);
        let store = self.app.pictures();
        let now = self.app.now();
        let done = entry_rows(
            entry,
            self.width,
            now,
            store,
            Want {
                skip: skip.saturating_sub(gap),
                want: want - gap_rows,
            },
        );
        // Where the rows a click can land on are, measured in the entry's block
        // and kept only where the window holds them: the entry's own first row,
        // and the last row of its output.
        let built = gap_rows + done.rows.len();
        let row_of = |row: usize| (row < built).then(|| at + row);
        let last = gap
            .saturating_add(done.total)
            .checked_sub(1)
            .and_then(|last| last.checked_sub(skip))
            .and_then(row_of);
        if let (Some(path), Some(row)) = (spill_of(entry), last) {
            self.spill.push((row, path));
        }
        let opening = entry
            .expandable()
            .then_some(gap)
            .and_then(|at| at.checked_sub(skip));
        if let Some(row) = opening.and_then(row_of) {
            self.expand.push((row, index));
        }
        // A picture is drawn at the row its block starts at, and `entry_rows`
        // has already cut the blocks the window's top row sits inside: its rows
        // are the frame's from there, and whether any of it is drawn is the
        // window's answer and not this one's.
        for picture in done.pictures {
            self.pictures.push(Laid {
                row: at + gap_rows + picture.row,
                ..picture
            });
        }
        let block: Vec<Line<'static>> = (0..gap_rows)
            .map(|_| Line::default())
            .chain(done.rows)
            .collect();
        self.built += block.len();
        self.last = Some((index, block.len()));
        self.rows.extend(block);
    }
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
