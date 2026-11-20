//! The rows an entry is drawn as, cached beside the measure they were laid out at: a row's height is
//! a fact of that measure and of nothing else, so a measure that moved drops every one of them.

use std::collections::BTreeSet;
use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::open::{Ground, Opening};
use sindri_core::provider::Deltas;
use sindri_core::records::Entry;
use sindri_core::redact::Redacted;
use sindri_core::run::{Exit, Stop};
use sindri_core::tools::OPEN;

use crate::ink::{Ink, Look};
use crate::link::{Link, Links, Way};
use crate::mark;
use crate::markdown;
use crate::palette::{Band, Palette};
use crate::pen::{self, Clip};
use crate::picture::{self, Surface};
use crate::pulse::{self, Head};
use crate::size::{self, Sized, Sizes};
use crate::spell;
use crate::stream::{Said, Stream};

/// The gutter every row of every entry opens with, two columns of it: the glyph that says what the
/// row is and the blank after it, so every entry's words begin in one column.
pub(crate) const INDENT: &str = "  ";

/// The sigil the reader's own words are set from, two columns of it: the row a prompt is asked on and
/// the draft that asks the next one alike.
pub(crate) const SIGIL: &str = "› ";

/// The spine a prompt's words are quoted by: one column down the left edge of every row they occupy.
const SPINE: &str = "┃";

const THOUGHT: &str = "│ ";

const PENDING: char = '…';
const CHECK: char = '✓';
const CROSS: char = '✗';
const KEPT: char = '⋯';

/// The one cell that says a fence's line carries more words past the band's edge.
const CUT: char = '…';

/// The rows of a result's head that a folded result keeps.
const FOLD_HEAD: usize = 4;

/// The fewest rows a fold is worth keeping back, so a count is always a plural: a result that would
/// keep back fewer is drawn whole rather than spending a row to hand the reader nothing.
const FOLD_KEPT: usize = 3;

/// Whether what holds this fold is drawn whole, or as the head a reader is handed to begin with: a
/// result's own, or the whole work a settled exchange's row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fold {
    Closed,
    Open,
}

/// Where an entry stands in the work the band draws: a prompt, the answer a run ended on and a stop
/// stand alone, and the thoughts, calls, results and answers the run went past are one work. Its
/// fold is held on the entry that opens it, and is never the fold a result keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Alone,
    Opens(Fold),
    Within,
}

impl Place {
    /// Whether this entry stands in a work at all: it opens one, or it is drawn in one.
    fn in_work(self) -> bool {
        matches!(self, Place::Opens(_) | Place::Within)
    }
}

/// One row of the transcript: never wider than the width it was laid out at, bar the one glyph that
/// is itself wider than that whole width and no layout can split — and bar a fence's line, which is
/// the shape the model wrote it in and no band reflows one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    spans: Vec<(String, Look)>,
    /// The runs of this row's cells that are ways to somewhere, measured from its left edge: a
    /// destination is no cell's own, so it is kept beside the words and written by the frame.
    links: Vec<Link>,
    /// The picture this row stands on, when it is the first row of a block one is drawn
    /// over: the placement goes to the terminal, the row's words are what a copy takes.
    picture: Option<picture::Picture>,
    /// The runs of this row a terminal draws at more than one cell: the row is measured by the
    /// cells they leave, and its spans say the words they are written as.
    sizes: Vec<Sized>,
    /// The fence this row is a line of, when it is one: a line of a fence is laid whole and the
    /// reader may walk it sideways, where every other row is wrapped to the band.
    fence: Option<Fence>,
    /// The line of source this row was drawn from, on the rows of a fence and on no other: a copy
    /// of a fence takes the words it was written as, and never the guide or the cut drawn over them.
    source: Option<String>,
    /// The fence whose tag row this is, on that row and no other: a mark over a fence's name takes
    /// the fence whole, where the row itself is no line of it — it never walks and is never cut.
    names: Option<usize>,
    /// The band this row is drawn on, when it stands on one of its own rather than on the ground
    /// the band it is read in lays under every row of it.
    band: Option<Band>,
}

/// A fence line: the block it is a line of, the columns of gutter its words are set past, and how
/// far the reader has walked it sideways. A fence line is never reflowed, so a band too narrow for
/// it carries the words past its edge and the cut marks the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fence {
    id: usize,
    gutter: u16,
    walk: u16,
}

impl Fence {
    pub fn id(self) -> usize {
        self.id
    }

    pub fn gutter(self) -> u16 {
        self.gutter
    }

    pub fn walk(self) -> u16 {
        self.walk
    }
}

impl Row {
    /// A span of no glyphs is dropped here, so a band too narrow to hold a gutter beside
    /// one glyph of prose leaves no empty span behind.
    pub fn of<V, I>(spans: I) -> Row
    where
        V: Into<Look>,
        I: IntoIterator<Item = (String, V)>,
    {
        Row {
            spans: spans
                .into_iter()
                .map(|(text, look)| (text, look.into()))
                .filter(|(text, _)| !text.is_empty())
                .collect(),
            links: Vec::new(),
            picture: None,
            sizes: Vec::new(),
            fence: None,
            source: None,
            names: None,
            band: None,
        }
    }

    /// The same row with one more run drawn at more than one cell: the run covers the cells of the
    /// words its span says, so a row of them is measured by the footprint and not by the glyphs.
    pub fn sized(mut self, run: Sized) -> Row {
        self.sizes.push(run);
        self
    }

    pub fn pictured_over(mut self, picture: picture::Picture) -> Row {
        self.picture = Some(picture);
        self
    }

    pub fn picture(&self) -> Option<&picture::Picture> {
        self.picture.as_ref()
    }

    pub fn blank() -> Row {
        Row {
            spans: Vec::new(),
            links: Vec::new(),
            picture: None,
            sizes: Vec::new(),
            fence: None,
            source: None,
            names: None,
            band: None,
        }
    }

    /// This row standing on `band`: its whole width is claimed in that band's ground before its
    /// words are written, so the row is read as a surface of its own.
    pub fn standing(mut self, band: Band) -> Row {
        self.band = Some(band);
        self
    }

    /// The band this row stands on, and none for a row drawn on the ground it was handed.
    pub fn stands_on(&self) -> Option<Band> {
        self.band
    }

    /// This row as a line of fence `id`, its words set past `gutter` columns and walked nowhere yet.
    pub fn fenced(mut self, id: usize, gutter: u16) -> Row {
        self.fence = Some(Fence {
            id,
            gutter,
            walk: 0,
        });
        self
    }

    pub fn fence(&self) -> Option<Fence> {
        self.fence
    }

    /// This row as the line of source it was drawn from: what a copy of the fence it belongs to
    /// hands back, where the row's own cells are what the band made of that line.
    pub fn sourced(mut self, source: impl Into<String>) -> Row {
        self.source = Some(source.into());
        self
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// This row as the tag row of fence `fence`, which names it and is no line of it.
    pub fn naming(mut self, fence: usize) -> Row {
        self.names = Some(fence);
        self
    }

    /// The fence this row stands in, as a line of it or as the row that names it: a mark over any
    /// row of a fence is a mark over the fence.
    pub fn fence_id(&self) -> Option<usize> {
        self.fence.map(Fence::id).or(self.names)
    }

    /// Whether this row is a line of fence `id`, which is what a walk and a copy of it are made of.
    pub fn line_of(&self, id: usize) -> bool {
        self.fence.is_some_and(|line| line.id() == id)
    }

    /// This row walked `by` columns sideways, which only the rows of one fence ever are.
    pub(crate) fn walked(&mut self, by: u16) {
        if let Some(fence) = &mut self.fence {
            fence.walk = by;
        }
    }

    /// Whether a band `room` columns wide cuts this row's words at its edge, which only a fence's
    /// line ever has more of than the band holds.
    pub fn cut(&self, room: u16) -> bool {
        self.fence
            .is_some_and(|fence| u32::from(fence.walk) + u32::from(room) < u32::from(self.width()))
    }

    /// The run of this row's cells that begins `left` columns in and is `width` columns wide is a
    /// way to `destination`: the words are drawn as they are, and the frame writes the way around
    /// them. A row that names none is drawn as it was.
    pub fn linking(mut self, left: u16, width: u16, destination: impl Into<String>) -> Row {
        self.links.push(Link::new(left, width, destination));
        self
    }

    /// The columns this row is drawn in: its spans measured as they are written, and the cells a
    /// sized run leaves around them, which is what a row of them takes of the band.
    pub fn width(&self) -> u16 {
        let written = self
            .spans
            .iter()
            .map(|(text, _)| pen::cells(text))
            .fold(0, u16::saturating_add);
        written.saturating_add(
            self.sizes
                .iter()
                .map(|run| run.width().saturating_sub(pen::cells(run.words())))
                .fold(0, u16::saturating_add),
        )
    }

    pub fn spans(&self) -> &[(String, Look)] {
        &self.spans
    }

    /// This row moved `by` columns right: the ways it names and the cells a terminal draws at more
    /// than one are measured from its left edge, so they move with the words while the picture
    /// placed over it stays where the row does.
    fn shifted(self, by: u16) -> Row {
        Row {
            spans: self.spans,
            links: self.links.iter().map(|link| link.shifted(by)).collect(),
            sizes: self.sizes.iter().map(|run| run.shifted(by)).collect(),
            picture: self.picture,
            fence: self.fence,
            source: self.source,
            names: self.names,
            band: self.band,
        }
    }

    /// This row opening with `gutter` the way every row of an entry does, its words set past it.
    pub fn guttered(self, gutter: &str, ink: Ink) -> Row {
        if self.spans.is_empty() {
            return self;
        }
        let mut row = self.shifted(pen::cells(gutter));
        if !gutter.is_empty() {
            row.spans
                .insert(0, (String::from(gutter), Look::plain(ink)));
        }
        row
    }

    /// This row with `other`'s words set after its own, and the ways `other` names moved past the
    /// columns this row covers: two cells of a table are one row, and each keeps its own ways.
    pub(crate) fn followed_by(self, other: &Row) -> Row {
        let shifts = self.width();
        let mut moved = other.clone().shifted(shifts);
        let mut row = self;
        row.spans.append(&mut moved.spans);
        row.links.append(&mut moved.links);
        row.sizes.append(&mut moved.sizes);
        row
    }

    /// This row with `head`'s words standing in the place of the span it opens with, and a blank
    /// after them: `by` is how many columns past that span the head takes, which is the shift every
    /// run the row keeps moves by.
    pub(crate) fn headed_by(self, head: &Row, by: u16) -> Row {
        let mut row = self.shifted(by);
        let mut spans = head.spans.clone();
        spans.push((String::from(" "), Look::plain(Ink::Plain)));
        spans.extend(row.spans.split_off(row.spans.len().min(1)));
        row.spans = spans;
        row
    }

    pub fn glyphs(&self) -> String {
        self.spans.iter().map(|(text, _)| text.as_str()).collect()
    }

    /// The glyphs this row says, each with the column it is drawn from and the columns it covers: a
    /// glyph of a run drawn at more than one cell is one word over that many.
    pub fn glyph_columns(&self) -> Vec<mark::Glyph> {
        let mut glyphs = Vec::new();
        let mut column = 0u16;
        for (text, _) in &self.spans {
            for glyph in text.chars() {
                let scale = self
                    .sizes
                    .iter()
                    .find(|run| run.covers(column))
                    .map_or(1, size::Sized::scale);
                let width = pen::cell(glyph).saturating_mul(scale);
                glyphs.push((column, glyph, width));
                column = column.saturating_add(width);
            }
        }
        glyphs
    }

    pub fn paint(
        &self,
        area: Rect,
        buffer: &mut Buffer,
        palette: &Palette,
        links: &mut Links,
        sizes: &mut Sizes,
    ) {
        if let Some(band) = self.band {
            pen::claim(buffer, area, palette.ground(Some(band)));
        }
        // A fence walked sideways keeps its gutter where it is and carries its words back by what
        // the reader walked past, so the bar still stands in the column it always did.
        match self.fence.filter(|fence| fence.walk > 0) {
            Some(fence) => {
                let (gutter, words) = gutter_and_words(&self.spans, fence.gutter);
                let mut spans = gutter;
                spans.extend(walked(&words, fence.walk));
                pen::write(buffer, area, &spans, Clip::Edge, palette);
            }
            None => pen::write(buffer, area, &self.spans, Clip::Edge, palette),
        }
        if area.width > 0 && self.cut(area.width) {
            pen::glyph(
                buffer,
                area.right().saturating_sub(1),
                area.y,
                CUT,
                Ink::Dim,
                palette,
            );
        }
        links.lay(area.y, area.x, &self.links);
        sizes.lay(area.y, area.x, &self.sizes);
    }
}

/// The words a row is drawn as, and the look each of them is written in.
type Spans = Vec<(String, Look)>;

/// `spans` split at `columns` of their left edge: the gutter before it and the words after, a span
/// that straddles the column cut in two so no glyph is drawn twice.
fn gutter_and_words(spans: &[(String, Look)], columns: u16) -> (Spans, Spans) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut at = 0u16;
    for (text, look) in spans {
        let room = columns.saturating_sub(at);
        if room == 0 {
            right.push((text.clone(), *look));
            continue;
        }
        let width = pen::cells(text);
        if width <= room {
            left.push((text.clone(), *look));
            at = at.saturating_add(width);
            continue;
        }
        let (head, tail) = split_at(text, usize::from(room));
        left.push((head, *look));
        right.push((String::from(tail), *look));
        at = columns;
    }
    (left, right)
}

/// `spans` with the first `columns` of them dropped, whole glyphs only: a glyph that straddles the
/// boundary goes with them rather than being drawn by halves.
fn walked(spans: &[(String, Look)], columns: u16) -> Spans {
    let mut past = columns;
    let mut kept = Vec::new();
    for (text, look) in spans {
        let (dropped, rest) = drop_columns(text, past);
        past = past.saturating_sub(dropped);
        if !rest.is_empty() {
            kept.push((String::from(rest), *look));
        }
    }
    kept
}

/// `text` without its first `columns` columns, and how many columns that took: the glyph that
/// straddles the boundary goes with the ones before it.
fn drop_columns(text: &str, columns: u16) -> (u16, &str) {
    let mut at = 0u16;
    for (index, glyph) in text.char_indices() {
        if at >= columns {
            return (at, &text[index..]);
        }
        at = at.saturating_add(pen::cell(glyph));
    }
    (at, "")
}

pub fn layout(entry: &Entry, cols: u16) -> Vec<Row> {
    layout_on(entry, cols, &Surface::none(), None)
}

/// The same entry on a `surface`, with the `ground` a run judges a target from: a path a call names
/// is a way only where that judgement holds it openable, and a frame handed none lays every path as
/// the words it was written as.
pub fn layout_on(entry: &Entry, cols: u16, surface: &Surface, ground: Option<&Ground>) -> Vec<Row> {
    laid(entry, cols, surface, ground, Fold::Closed, None).0
}

/// The rows of one entry in the fold it is drawn as, and how many rows of it that fold keeps back:
/// a result's fold is a fact of its last layout, and a width change re-lays it.
fn laid(
    entry: &Entry,
    cols: u16,
    surface: &Surface,
    ground: Option<&Ground>,
    fold: Fold,
    head: Option<&Head>,
) -> (Vec<Row>, Option<usize>) {
    let cols = cols.max(1);
    match entry {
        // A turn is no row of the band, and a stop that ended on the answer under it says nothing
        // that answer does not: the record writes both, and the drawing leaves them out.
        Entry::Turn { .. } | Entry::Stop { stop: Stop::Final } => (Vec::new(), None),
        Entry::Call {
            name, arguments, ..
        } => (
            opening(
                &name.to_string(),
                &arguments.to_string(),
                cols,
                ground,
                None,
            ),
            None,
        ),
        Entry::Result {
            name,
            text,
            bytes,
            elapsed_ms,
            exit,
            ..
        } => folded(
            result_rows(
                Some(&name.to_string()),
                *exit,
                &text.to_string(),
                *bytes,
                *elapsed_ms,
                cols,
            ),
            cols,
            fold,
        ),
        Entry::Assistant { text, .. } => (
            answer_on(
                &text.to_string(),
                cols,
                head,
                surface,
                markdown::Wholeness::Whole,
            ),
            None,
        ),
        Entry::Reasoning { text, .. } => (thought(&text.to_string(), cols), None),
        Entry::Prompt { text } => (asked(&text.to_string(), cols), None),
        Entry::Stop { stop } => (stopped(stop, cols), None),
    }
}

/// A result's rows as the reader is handed them: the rows of its head, one row counting the rows
/// kept back, and its last row — unless the reader opened the fold, or it holds the whole result.
/// The count comes back beside them for as long as the result has a fold at all.
fn folded(mut rows: Vec<Row>, cols: u16, fold: Fold) -> (Vec<Row>, Option<usize>) {
    let Some(kept) = rows
        .len()
        .checked_sub(FOLD_HEAD + 1)
        .filter(|kept| *kept > FOLD_KEPT)
    else {
        return (rows, None);
    };
    if fold == Fold::Open {
        return (rows, Some(kept));
    }
    let last = rows
        .pop()
        .expect("a result whose fold keeps a row back has a last row under its head");
    rows.truncate(FOLD_HEAD);
    rows.push(kept_back(kept, cols));
    rows.push(last);
    (rows, Some(kept))
}

/// The row that says how many rows of a result the fold keeps back, drawn as any other row of that
/// entry: the marker in the gutter and the count in the dim ink of the fold's own words, a rung
/// quieter than the output it stands among.
fn kept_back(kept: usize, cols: u16) -> Row {
    let words = format!("{kept} rows kept back");
    marked(&KEPT.to_string(), Ink::Dim, vec![(words, Ink::Dim)], cols)
}

/// What a work holds: the calls it made, the files its opens read and the time its results took, as
/// the one row a settled work is drawn as says them.
#[derive(Debug, Default)]
struct Tally {
    calls: usize,
    files: BTreeSet<String>,
    elapsed_ms: u64,
}

impl Tally {
    /// The words the one row a settled work is drawn as says: each number only where the work holds
    /// it, in the order a reader reads them.
    fn words(&self) -> String {
        let mut said: Vec<String> = Vec::new();
        if self.calls > 0 {
            said.push(format!("{} call{}", self.calls, plural(self.calls)));
        }
        if !self.files.is_empty() {
            said.push(format!(
                "{} file{}",
                self.files.len(),
                plural(self.files.len())
            ));
        }
        if self.elapsed_ms > 0 {
            said.push(took(self.elapsed_ms));
        }
        said.join(" · ")
    }
}

/// The time a work took, in the words of a fact: a work under a second says the milliseconds it
/// took, because a tenth of a second is no reading a reader can tell from none.
fn took(ms: u64) -> String {
    match ms {
        ms if ms < 1_000 => format!("{ms} ms"),
        ms => pulse::format_duration_spaced(ms),
    }
}

fn plural(count: usize) -> &'static str {
    match count {
        1 => "",
        _ => "s",
    }
}

/// The rows a stop is drawn as: the mark it wears with its own word beside it, and one line of the
/// run's own words under it when the stop is an error, since a reader is owed one line and no more.
fn stopped(stop: &Stop, cols: u16) -> Vec<Row> {
    let (marker, ink) = stop_mark(stop);
    let mut rows = vec![marked(
        &marker.to_string(),
        ink,
        vec![(String::from(stop.word()), Ink::Dim)],
        cols,
    )];
    if let Stop::Error(message) = stop {
        let said = message.to_string();
        let line = said.lines().next().unwrap_or_default();
        rows.extend(body(line, cols, None, Ink::Muted));
    }
    rows
}

/// The rows a prompt is drawn as: the blank row that separates its exchange from the one above it,
/// and then the reader's own words down the spine of a quoted block — the blank row `Rows` drops for
/// the first exchange, which has nothing above it.
fn asked(text: &str, cols: u16) -> Vec<Row> {
    let opening = format!("{SPINE}{}", SIGIL.trim_end());
    let (gutter, room) = margin(&opening, cols);
    let mut rows = vec![Row::blank()];
    rows.extend(
        wrap(text, room)
            .into_iter()
            .enumerate()
            .map(|(at, line)| quoted(at == 0 && !gutter.is_empty(), line, cols)),
    );
    rows
}

/// One row of a prompt's words: the spine down its left edge, the sigil standing in the column
/// beside it on the row the prompt is asked on and a blank on every row under it, and the words set
/// past both. A band too narrow to hold that gutter beside one word gives the gutter up.
fn quoted(opens: bool, line: String, cols: u16) -> Row {
    let marker = match opens {
        true => SIGIL.trim_end(),
        false => " ",
    };
    let gutter = pen::cells(SPINE).saturating_add(pen::cells(marker));
    let row = match gutter.saturating_add(pen::cells(&line)) > cols {
        true => Row::of([(line, Ink::Plain)]),
        false => Row::of([
            (String::from(SPINE), Ink::Dim),
            (String::from(marker), Ink::Plain),
            (line, Ink::Plain),
        ]),
    };
    row.standing(Band::Prompt)
}

fn thought(text: &str, cols: u16) -> Vec<Row> {
    let (gutter, room) = margin(THOUGHT, cols);
    wrap(text, room)
        .into_iter()
        .map(|line| words_past(gutter, Ink::Dim, Ink::Muted, line, cols))
        .collect()
}

/// The rows an answer is drawn as, set in the answer's gutter — except the row the run's marker
/// heads, which stands the marker and its blank column in that gutter and lays that row's words
/// out beside them.
fn answer_on(
    text: &str,
    cols: u16,
    head: Option<&Head>,
    surface: &Surface,
    wholeness: markdown::Wholeness,
) -> Vec<Row> {
    let (gutter, room) = margin(INDENT, cols);
    // The marker's own words count against the width; the gutter it stands in the place of does
    // not, and a marker the band cannot hold beside one word of the answer is given up whole.
    let reserve = head.map_or(0, |head| {
        head.width()
            .saturating_add(1)
            .saturating_sub(pen::cells(gutter))
    });
    let (head, reserve) = match reserve < room {
        true => (head, reserve),
        false => (None, 0),
    };
    let mut rows = markdown::rows_in(text, cols, reserve, gutter, surface, wholeness).into_iter();
    let mut laid: Vec<Row> = Vec::new();
    if let Some(head) = head
        && let Some(first) = rows.next()
    {
        laid.push(first.headed_by(&head_row(head, cols), reserve));
    }
    laid.extend(rows);
    laid
}

/// The row a run's marker heads: the mark stands in the gutter and the words it says begin where
/// the entry's own do.
fn head_row(head: &Head, cols: u16) -> Row {
    let Some(((glyph, ink), words)) = head.spans().split_first() else {
        return Row::blank();
    };
    let words = words
        .iter()
        .map(|(text, ink)| (String::from(text.trim_start()), *ink))
        .collect();
    marked(glyph, *ink, words, cols)
}

/// A row whose marker stands in the gutter: the glyph in the gutter's first column, the gutter's
/// blank after it, and the words fitted to the columns beside them. A band too narrow to hold both
/// gives up the marker rather than a word.
fn marked(marker: &str, ink: Ink, words: Vec<(String, Ink)>, cols: u16) -> Row {
    let (gutter, room) = margin(INDENT, cols);
    let words = fitted(words, room);
    match gutter.is_empty() {
        true => words,
        false => Row::of([(String::from(marker), ink), (String::from(" "), Ink::Plain)])
            .followed_by(&words),
    }
}

/// The gutter a body opens with at `cols`, and the columns its words have beside it: a band no
/// wider than the gutter holds none of it, and the marker is given up rather than the words.
fn margin(gutter: &str, cols: u16) -> (&str, u16) {
    let width = pen::cells(gutter);
    match cols > width {
        true => (gutter, cols - width),
        false => ("", cols),
    }
}

/// The rows a call opens with: the mark it wears and the tool's name in the gutter, and under them
/// the words the call is drawn as. The mark is the check or the cross of the verdict that settled
/// it, and the `…` of a call still out.
fn opening(
    name: &str,
    arguments: &str,
    cols: u16,
    ground: Option<&Ground>,
    exit: Option<Exit>,
) -> Vec<Row> {
    let words = spell::of(name, arguments);
    let (marker, ink) = match exit {
        Some(exit) => (mark(exit).to_string(), ink_of(exit)),
        None => (PENDING.to_string(), Ink::Dim),
    };
    let mut rows = vec![marked(
        &marker,
        ink,
        vec![(String::from(name), Ink::Dim)],
        cols,
    )];
    rows.extend(body(
        &words,
        cols,
        way(name, arguments, &words, ground),
        Ink::Plain,
    ));
    rows
}

/// The way a call names, when the run judged it one: the words the row says are the path the run
/// read, which is how its cells are found, and the file the judgement resolved them to. A call that
/// names nothing the run would open is no way at all.
fn way(name: &str, arguments: &str, written: &str, ground: Option<&Ground>) -> Option<Way> {
    if name != OPEN || written.is_empty() {
        return None;
    }
    match ground?.judge(arguments).ok()? {
        Opening::File(path) => Some(Way::of(written, &path)),
        Opening::Url(_) => None,
    }
}

/// The rows a result is drawn as: the row that says the verdict, the duration and the size, and the
/// body it wrote under them. `name` is the tool's own where no call row above already says it.
fn result_rows(
    name: Option<&str>,
    exit: Option<Exit>,
    text: &str,
    bytes: u64,
    elapsed_ms: u64,
    cols: u16,
) -> Vec<Row> {
    let mut rows = vec![settled(name, exit, bytes, elapsed_ms, cols)];
    rows.extend(body(text, cols, None, Ink::Muted));
    rows
}

/// The row a result opens with: the verdict, how long the call took and how much it wrote, and the
/// name of its tool when `name` gives one. A result with no verdict says the numbers alone rather
/// than guessing `exit 0` from the text beside it.
fn settled(name: Option<&str>, exit: Option<Exit>, bytes: u64, elapsed_ms: u64, cols: u16) -> Row {
    let numbers = match name {
        Some(name) => format!("{name}  {elapsed_ms} ms · {bytes} bytes"),
        None => format!("{elapsed_ms} ms · {bytes} bytes"),
    };
    match exit {
        Some(exit) => marked(
            &mark(exit).to_string(),
            ink_of(exit),
            vec![(format!("{}  {numbers}", exit.word()), Ink::Dim)],
            cols,
        ),
        None => {
            let (gutter, room) = margin(INDENT, cols);
            fitted(vec![(numbers, Ink::Dim)], room).guttered(gutter, Ink::Plain)
        }
    }
}

/// The rows a call and the result that settled it are drawn as: the call's own words first, wearing
/// the mark its verdict settled on, and the result's rows under them, which say no name the call's
/// row above already holds. The fold is the result's rows' and never the call's.
fn answered(
    call: &Entry,
    result: &Entry,
    cols: u16,
    surface: &Surface,
    ground: Option<&Ground>,
    fold: Fold,
) -> (Vec<Row>, Option<usize>) {
    let cols = cols.max(1);
    let (
        Entry::Call {
            name, arguments, ..
        },
        Entry::Result {
            text,
            bytes,
            elapsed_ms,
            exit,
            ..
        },
    ) = (call, result)
    else {
        let (mut rows, _) = laid(call, cols, surface, ground, fold, None);
        let (under, kept) = laid(result, cols, surface, ground, fold, None);
        rows.extend(under);
        return (rows, kept);
    };
    let mut rows = opening(
        &name.to_string(),
        &arguments.to_string(),
        cols,
        ground,
        *exit,
    );
    let (under, kept) = folded(
        result_rows(None, *exit, &text.to_string(), *bytes, *elapsed_ms, cols),
        cols,
        fold,
    );
    rows.extend(under);
    (rows, kept)
}

/// The check a run that answered settles on and the cross every other stop wears: the marker at the
/// head of an answer and the stop's own entry read this pair, so no two drawings of one stop disagree.
pub(crate) fn stop_mark(stop: &Stop) -> (char, Ink) {
    match stop {
        Stop::Final => (CHECK, Ink::Ok),
        Stop::MaxTurns | Stop::Budget(_) | Stop::Cancelled | Stop::Error(_) => (CROSS, Ink::Err),
    }
}

fn mark(exit: Exit) -> char {
    if exit.is_ok() { CHECK } else { CROSS }
}

/// Two inks for four states: the words tell `✗ exit N` from `✗ timed out`, never a
/// shade of red.
fn ink_of(exit: Exit) -> Ink {
    if exit.is_ok() { Ink::Ok } else { Ink::Err }
}

fn fitted(spans: Vec<(String, Ink)>, cols: u16) -> Row {
    let mut room = usize::from(cols.max(1));
    let mut kept = Vec::new();
    for (text, ink) in spans {
        if room == 0 {
            break;
        }
        let (head, rest) = split_at(&text, room);
        room = room.saturating_sub(usize::from(pen::cells(&head)));
        kept.push((head, ink));
        if !rest.is_empty() {
            break;
        }
    }
    Row::of(kept)
}

/// The words of a tool's arguments or its result, set in the gutter a body is set from and in the
/// ink that kind of body is drawn in. A body that names a way to a file is laid with that path's
/// own cells a way there, and every other body is laid as the wrapping lays it.
fn body(text: &str, cols: u16, way: Option<Way>, ink: Ink) -> Vec<Row> {
    if text.is_empty() {
        return Vec::new();
    }
    let (gutter, room) = margin(INDENT, cols);
    let named = way.and_then(|way| markdown::body(text, room, &way));
    match named {
        Some(rows) => rows
            .into_iter()
            .map(|row| row.guttered(gutter, ink))
            .collect(),
        None => wrap(text, room)
            .into_iter()
            .map(|line| words_past(gutter, ink, ink, line, cols))
            .collect(),
    }
}

/// One row of `line` set past `gutter`, the gutter in `mark`'s ink and the words in `words`'': the
/// gutter is given up when the line holds a glyph wider than the room it left, because a marker is
/// given up before a word is cut.
fn words_past(gutter: &str, mark: Ink, words: Ink, line: String, cols: u16) -> Row {
    let drawn = pen::cells(gutter).saturating_add(pen::cells(&line));
    let gutter = match drawn > cols {
        true => "",
        false => gutter,
    };
    Row::of([(String::from(gutter), mark), (line, words)])
}

pub(crate) fn wrap(text: &str, cols: u16) -> Vec<String> {
    wrap_under(text, cols, cols)
}

pub(crate) fn wrap_under(text: &str, first: u16, room: u16) -> Vec<String> {
    let first = usize::from(first.max(1));
    let room = usize::from(room.max(1));
    let mut rows: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let mut row = String::new();
        let mut width = 0;
        for word in line.split_whitespace() {
            let mut word = word;
            loop {
                let limit = if rows.is_empty() { first } else { room };
                let gap = usize::from(!row.is_empty());
                if width + gap + usize::from(pen::cells(word)) <= limit {
                    if gap == 1 {
                        row.push(' ');
                        width += 1;
                    }
                    row.push_str(word);
                    width += usize::from(pen::cells(word));
                    break;
                }
                if !row.is_empty() {
                    rows.push(std::mem::take(&mut row));
                    width = 0;
                    continue;
                }
                let (head, rest) = split_at(word, limit);
                rows.push(head);
                word = rest;
                if word.is_empty() {
                    break;
                }
            }
        }
        rows.push(row);
    }
    rows
}

/// The longest head of `word` that fits in `room` columns, and what is left of it. A
/// first glyph wider than the room is taken anyway, or the word would never be broken.
pub(crate) fn split_at(word: &str, room: usize) -> (String, &str) {
    let mut width = 0;
    for (at, glyph) in word.char_indices() {
        let wide = usize::from(pen::cell(glyph));
        if at > 0 && width + wide > room {
            return (word[..at].to_owned(), &word[at..]);
        }
        width += wide;
    }
    (word.to_owned(), "")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scroll {
    #[default]
    Tail,
    Back(u32),
}

impl Scroll {
    /// How many rows back from the tail this window stands.
    pub fn back(self) -> u32 {
        match self {
            Scroll::Tail => 0,
            Scroll::Back(rows) => rows,
        }
    }

    /// The window standing `rows` behind the tail, and the tail itself for none.
    pub fn of(rows: u32) -> Scroll {
        match rows {
            0 => Scroll::Tail,
            rows => Scroll::Back(rows),
        }
    }

    /// Whether this window stands where the run is still being drawn.
    pub fn is_tail(self) -> bool {
        self == Scroll::Tail
    }
}

/// The cell a thumb is drawn of.
pub(crate) const THUMB: char = '█';

/// Where a window stands in a transcript taller than the band it is read in: the rows of the
/// gutter its thumb covers, counted from the band's first row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thumb {
    top: u16,
    span: u16,
}

impl Thumb {
    pub fn span(&self) -> u16 {
        self.span
    }

    pub fn rows(&self) -> Range<u16> {
        self.top..self.top + self.span
    }

    /// The thumb stands in the gutter's glyph column, where the blank column after it is what keeps
    /// the words off it.
    pub(crate) fn paint(&self, area: Rect, y: u16, buffer: &mut Buffer, palette: &Palette) {
        if let Some(row) = y.checked_sub(area.y)
            && self.rows().contains(&row)
        {
            pen::glyph(buffer, area.x, y, THUMB, Ink::Dim, palette);
        }
    }
}

/// Which way a sideways key walks a fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sideways {
    Back,
    Forward,
}

/// How many columns one press of a sideways key walks a fence.
const WALK: u16 = 8;

/// The three facts a band is laid at: the `cols` it is drawn in, the `surface` a picture in an answer
/// is measured against, and the `ground` a call's path is judged against. Rows laid at another measure
/// are rows drawn from facts that have moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Measure {
    cols: u16,
    surface: Surface,
    ground: Option<Ground>,
}

/// A cache no band has been handed yet holds the narrowest one, because a band is never narrower than
/// the column that draws nothing.
impl Default for Measure {
    fn default() -> Measure {
        Measure {
            cols: 1,
            surface: Surface::none(),
            ground: None,
        }
    }
}

impl Measure {
    /// The measure of a band `cols` wide, held at no fewer than one column: a band narrower than that
    /// draws nothing.
    pub fn new(cols: u16, surface: Surface, ground: Option<&Ground>) -> Measure {
        Measure {
            cols: cols.max(1),
            surface,
            ground: ground.cloned(),
        }
    }
}

/// Every entry a transcript holds, each with the rows it was laid out as at one measure,
/// and nothing it was not drawn as.
#[derive(Clone, Debug, Default)]
pub struct Rows {
    /// The measure every row here was laid at, and the one the next rows will be laid at.
    measure: Measure,
    held: Vec<Held>,
    /// The words the wire has fed the run's newest turn, which the live entry is laid from and
    /// retired with.
    in_flight: Stream,
    /// How many layouts this cache has done: the instrument the rows tests hold the laying out to,
    /// read from outside this crate and never by it.
    lays: usize,
    /// Whether the stop this cache holds last is the one the turn in flight ended, which stands above
    /// it, and never the stop a run before it left.
    ended: bool,
    /// The marker the newest answer the record holds wears, and none while the frame's marker heads
    /// a turn in flight or a row of its own.
    head: Option<Head>,
}

#[derive(Clone, Debug)]
struct Held {
    what: What,
    /// What came back, when this holds a call whose result has landed: kept rather than
    /// layered over the rows, so a width change re-lays the settled row.
    result: Option<Entry>,
    rows: Option<Vec<Row>>,
    /// Where this entry stands in the work around it, and the fold the reader left that whole work
    /// in where this entry opens one.
    place: Place,
    /// The fold the reader left this result in, which is theirs and survives every re-lay of the
    /// rows under it.
    fold: Fold,
    /// How many rows this entry's closed fold keeps back, and nothing where its whole fold holds
    /// it: a fact of its last layout, because what a fold holds depends on the width.
    kept: Option<usize>,
    /// How far each fence of this entry has been walked, by the block's ordinal: kept off the rows so
    /// a line laid again is walked where the reader left it, and dropped with them at a new width.
    walks: Vec<(usize, u16)>,
}

#[derive(Clone, Debug)]
enum What {
    Fact(Entry),
    Live {
        revision: u64,
        /// The marker the newest answer wore: laid into the rows it heads, so a change here
        /// is a change to the picture.
        head: Option<Head>,
    },
}

impl Rows {
    pub fn new() -> Rows {
        Rows::default()
    }

    /// Holds the stop a run ended on at the transcript's end: the words that run last said are theirs
    /// to keep above it, so the reason a run stopped is the last word of the run it ended.
    pub(crate) fn stopped(&mut self, stop: Stop) {
        // The record draws the stop it wrote, so a stop the band already holds is that stop drawn
        // once, and never another under it.
        let drawn = self
            .held
            .iter()
            .rev()
            .find_map(|held| match &held.what {
                What::Fact(entry) => Some(entry),
                What::Live { .. } => None,
            })
            .is_some_and(|entry| matches!(entry, Entry::Stop { stop: drawn } if *drawn == stop));
        if !drawn {
            self.push(Entry::Stop { stop });
        }
        self.ended = true;
    }

    /// Lays `head` over the newest answer the record holds, and drops that answer's rows where it
    /// moved: a marker that moved is laid again where it now stands. Nothing else wears it.
    pub fn heading(&mut self, head: Option<Head>) {
        if self.head == head {
            return;
        }
        self.head = head;
        if let Some(at) = self.newest_answer() {
            self.held[at].rows = None;
        }
    }

    /// The newest answer the record holds: the one entry a marker with no run in flight heads.
    fn newest_answer(&self) -> Option<usize> {
        self.held
            .iter()
            .rposition(|held| matches!(held.what, What::Fact(Entry::Assistant { .. })))
    }

    /// Whether the record holds an answer, which is the one row a marker with no run in flight can
    /// head.
    pub(crate) fn holds_answer(&self) -> bool {
        self.newest_answer().is_some()
    }

    pub fn push(&mut self, entry: Entry) {
        let answer = matches!(entry, Entry::Assistant { .. });
        // A newer answer takes the marker from the one that wore it, so that one is laid again
        // without it: two answers never wear one marker.
        if self.head.is_some()
            && answer
            && let Some(at) = self.newest_answer()
        {
            self.held[at].rows = None;
        }
        if let Entry::Result { turn, id, .. } = &entry
            && let Some(at) = self.pending(*turn, id.as_ref())
        {
            self.settle(at, entry);
            return;
        }
        // An answer landing is the record holding the turn's words, so the turn goes with it: one
        // word drawn twice is one word in two rows.
        if answer {
            self.retire();
        } else {
            self.drop_live();
        }
        let place = self.land(&entry);
        self.held.push(Held {
            what: What::Fact(entry),
            result: None,
            rows: None,
            place,
            fold: Fold::Closed,
            kept: None,
            walks: Vec::new(),
        });
    }

    /// Where an entry landing at the transcript's end stands, and what its landing does to the work
    /// around it: a thought, call or result joins the work above it, and a prompt or a stop ends
    /// that work.
    fn land(&mut self, entry: &Entry) -> Place {
        let of_work = work_entry(entry);
        if of_work {
            self.past();
        }
        let within = self.open_work().is_some();
        if matches!(entry, Entry::Prompt { .. } | Entry::Stop { .. })
            && let Some(at) = self.last_work()
        {
            self.held[at].rows = None;
        }
        match (of_work, within) {
            (false, _) => Place::Alone,
            (true, true) => Place::Within,
            (true, false) => Place::Opens(Fold::Closed),
        }
    }

    /// The entry that opens the last work the transcript holds, which is the work a prompt or a stop
    /// landing now settles: the answer above it is the exchange's end and no row of that work.
    fn last_work(&self) -> Option<usize> {
        (0..self.held.len())
            .rev()
            .find(|at| matches!(self.held[*at].place, Place::Opens(_)))
    }

    /// The entry that opens the work the transcript ends in, and nothing where the entry it ends on
    /// stands alone.
    fn open_work(&self) -> Option<usize> {
        let last = self.held.len().checked_sub(1)?;
        if !self.held[last].place.in_work() {
            return None;
        }
        (0..=last)
            .rev()
            .find(|at| matches!(self.held[*at].place, Place::Opens(_)))
    }

    /// An answer the run went past is work of its own: a thought, call or result after it says the
    /// turn ended and the run went on, so the answer joins the work and the rows it drew are the
    /// work's.
    fn past(&mut self) {
        let Some(at) = self.held.len().checked_sub(1) else {
            return;
        };
        if self.held[at].place != Place::Alone
            || !matches!(self.held[at].what, What::Fact(Entry::Assistant { .. }))
        {
            return;
        }
        let above = at
            .checked_sub(1)
            .is_some_and(|above| self.held[above].place.in_work());
        self.held[at].place = match above {
            true => Place::Within,
            false => Place::Opens(Fold::Closed),
        };
        self.held[at].rows = None;
    }

    /// Holds the turn this cache owns as its last entry. A turn asked again without having moved
    /// lays nothing, and the mark counts as movement because it is laid into the rows it heads: a
    /// held one would wear a clock that has moved on.
    pub fn live(&mut self, head: Option<&Head>) {
        if self.in_flight.is_empty() && head.is_none() {
            self.drop_live();
            return;
        }
        let revision = self.in_flight.revision();
        let head = head.cloned();
        let at = self.live_at();
        if let Some(held) = at.map(|at| &self.held[at])
            && let What::Live {
                revision: laid,
                head: mark,
            } = &held.what
            && held.rows.is_some()
            && *laid == revision
            && *mark == head
        {
            return;
        }
        let mut rows = self.live_rows(head.as_ref());
        match at {
            Some(at) => {
                self.walking(at, &mut rows);
                let held = &mut self.held[at];
                held.what = What::Live { revision, head };
                held.rows = Some(rows);
            }
            None => {
                let held = Held {
                    what: What::Live { revision, head },
                    result: None,
                    rows: Some(rows),
                    place: Place::Alone,
                    fold: Fold::Closed,
                    kept: None,
                    walks: Vec::new(),
                };
                match self.ended {
                    true => self.held.insert(self.held.len() - 1, held),
                    false => self.held.push(held),
                }
                self.ended = false;
            }
        }
        self.lays += 1;
    }

    /// The newest answer alone wears `head`, and a turn that has said no answer gets a row of its own
    /// for it: a frame waiting on a wire must not look idle.
    fn live_rows(&self, head: Option<&Head>) -> Vec<Row> {
        let newest = self
            .in_flight
            .said()
            .enumerate()
            .filter(|(_, (_, said))| *said == Said::Answer)
            .map(|(at, _)| at)
            .last();
        let mut rows = Vec::new();
        for (at, (words, said)) in self.in_flight.said().enumerate() {
            let head = match Some(at) == newest {
                true => head,
                false => None,
            };
            rows.extend(match said {
                Said::Reasoning => thought(words, self.measure.cols),
                Said::Answer => answer_on(
                    words,
                    self.measure.cols,
                    head,
                    &self.measure.surface,
                    markdown::Wholeness::Arriving,
                ),
            });
        }
        if let (None, Some(head)) = (newest, head) {
            rows.push(head_row(head, self.measure.cols));
        }
        rows
    }

    /// Whether the turn this cache holds has said an answer rather than a thought alone.
    pub(crate) fn says_answer(&self) -> bool {
        self.in_flight.says_answer()
    }

    /// Where the turn in flight stands, which a stop that landed under it leaves in its own place:
    /// the turn is found rather than taken to be the newest entry.
    fn live_at(&self) -> Option<usize> {
        self.held
            .iter()
            .position(|held| matches!(held.what, What::Live { .. }))
    }

    /// Drops the turn in flight with its entry: the run it belonged to has said its last word, or
    /// the record holds those words as entries now.
    pub(crate) fn retire(&mut self) {
        self.in_flight = Stream::new();
        self.drop_live();
    }

    /// Drops the live entry alone, wherever it stands: an entry landing takes its place in the band,
    /// and the turn it was laid from is still the one in flight.
    fn drop_live(&mut self) {
        self.held
            .retain(|held| !matches!(held.what, What::Live { .. }));
        self.ended = false;
    }

    /// The call a result of `turn` settles: the one whose identity it names, and a result that names
    /// none — one a transcript written before calls had identities holds — the oldest call of that
    /// turn still waiting, so a wave of calls and the wave answering it pair in written order.
    fn pending(&self, turn: u32, id: Option<&Redacted>) -> Option<usize> {
        let mut waiting = self
            .held
            .iter()
            .enumerate()
            .filter_map(|(at, held)| match &held.what {
                What::Fact(Entry::Call {
                    turn: of,
                    id: named,
                    ..
                }) if held.result.is_none() && *of == turn => Some((at, named.as_ref())),
                _ => None,
            });
        match id {
            Some(id) => waiting
                .find(|(_, named)| *named == Some(id))
                .map(|(at, _)| at),
            None => waiting.next().map(|(at, _)| at),
        }
    }

    pub fn settle(&mut self, at: usize, result: Entry) {
        let held = &mut self.held[at];
        held.result = Some(result);
        held.rows = None;
    }

    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    pub fn lays(&self) -> usize {
        self.lays
    }

    /// Hands the cache the band it is drawn at, answering whether the band reflowed: true only when
    /// the width moved.
    pub fn measure(&mut self, measure: Measure) -> bool {
        if measure == self.measure {
            return false;
        }
        let reflowed = measure.cols != self.measure.cols;
        self.measure = measure;
        self.unlay();
        reflowed
    }

    /// Walks the fence `fence` of entry `at` one step sideways, answering whether it moved: every row
    /// of that fence moves and no other, and a fence walked as far as its widest line goes no further.
    /// The walk is the entry's, so a line laid again keeps it until a new width drops it.
    pub fn sideways(&mut self, at: usize, fence: usize, way: Sideways) -> bool {
        let cols = self.measure.cols;
        let Some(held) = self.held.get_mut(at) else {
            return false;
        };
        let Some(rows) = held.rows.as_ref() else {
            return false;
        };
        let widest = rows
            .iter()
            .filter(|row| row.line_of(fence))
            .map(Row::width)
            .max();
        let Some(widest) = widest else {
            return false;
        };
        let walk = held
            .walks
            .iter()
            .find(|(id, _)| *id == fence)
            .map_or(0, |(_, walk)| *walk);
        // The widest line is the bound: walked past it, the tail would stand past the band's edge
        // with nothing left to bring to it.
        let most = widest.saturating_sub(cols);
        let to = match way {
            Sideways::Back => walk.saturating_sub(WALK),
            Sideways::Forward => walk.saturating_add(WALK).min(most),
        };
        if to == walk {
            return false;
        }
        if let Some(rows) = held.rows.as_mut() {
            for row in rows {
                if row.line_of(fence) {
                    row.walked(to);
                }
            }
        }
        match held.walks.iter_mut().find(|(id, _)| *id == fence) {
            Some((_, walk)) => *walk = to,
            None => held.walks.push((fence, to)),
        }
        true
    }

    /// `rows` walked where the reader left this entry's fences: the one thing about them that is the
    /// entry's and not the width's.
    fn walking(&self, at: usize, rows: &mut [Row]) {
        let walks = &self.held[at].walks;
        for row in rows {
            let Some(fence) = row.fence() else {
                continue;
            };
            if let Some((_, walk)) = walks.iter().find(|(id, _)| *id == fence.id()) {
                row.walked(*walk);
            }
        }
    }

    /// Drops every row, and the walk its fences were left in with them: a fact a row was laid out
    /// against has moved, and a row is only a drawing of the facts that made it.
    fn unlay(&mut self) {
        for held in &mut self.held {
            held.rows = None;
            held.walks.clear();
        }
    }

    /// The rows of entry `at`, laid out now if they do not exist and kept for the next paint: a
    /// settled call is its own rows and then the result's under them, a settled work is the one row
    /// that counts it, and the live entry has no fact behind it, so nothing here.
    pub fn rows(&mut self, at: usize) -> &[Row] {
        let work = self.work(at);
        let one = self.one_row(&work);
        if one && at != work.start {
            return &[];
        }
        let cols = self.measure.cols;
        let surface = &self.measure.surface;
        let ground = self.measure.ground.as_ref();
        let held = &self.held[at];
        if held.rows.is_none() {
            let laid = match (&held.what, held.result.as_ref()) {
                _ if one => Some((vec![self.work_row(&work, cols)], None)),
                (What::Fact(entry), None) => {
                    let head = match self.newest_answer() == Some(at) {
                        true => self.head.as_ref(),
                        false => None,
                    };
                    Some(laid(entry, cols, surface, ground, held.fold, head))
                }
                (What::Fact(call), Some(result)) => {
                    Some(answered(call, result, cols, surface, ground, held.fold))
                }
                (What::Live { .. }, _) => None,
            };
            if let Some((mut rows, kept)) = laid {
                self.lays += 1;
                self.walking(at, &mut rows);
                // The transcript's first row has nothing above it to be separated from, so the
                // exchange that opens it opens on its own words.
                if at == 0 && is_prompt(&self.held[at]) {
                    rows.remove(0);
                }
                self.held[at].rows = Some(rows);
                self.held[at].kept = kept;
            }
        }
        self.held[at].rows.as_deref().unwrap_or_default()
    }

    /// The entries of the work `at` stands in: the run of work entries around it, and an entry that
    /// stands alone in a work of one.
    fn work(&self, at: usize) -> Range<usize> {
        let start = (0..=at)
            .rev()
            .find(|at| !matches!(self.held[*at].place, Place::Within))
            .unwrap_or(at);
        let mut end = start + 1;
        while end < self.held.len() && matches!(self.held[end].place, Place::Within) {
            end += 1;
        }
        start..end
    }

    /// Whether the record has written past `work`, which is a prompt or a stop standing after it: a
    /// turn's answer and the turn in flight both leave the run filling the work it is in.
    fn written_past(&self, work: &Range<usize>) -> bool {
        self.held[work.end..].iter().any(|held| {
            matches!(
                held.what,
                What::Fact(Entry::Prompt { .. } | Entry::Stop { .. })
            )
        })
    }

    /// What `work` holds, counted off the entries it stands in: the calls it made with the files
    /// they read, and the time of every result, the result that settled a call included.
    fn tally(&self, work: &Range<usize>) -> Tally {
        let mut tally = Tally::default();
        for held in &self.held[work.clone()] {
            if let What::Fact(Entry::Call {
                name, arguments, ..
            }) = &held.what
            {
                tally.calls += 1;
                if let Some(path) = spell::file(&name.to_string(), &arguments.to_string()) {
                    tally.files.insert(path);
                }
            }
            if let Some(Entry::Result { elapsed_ms, .. }) = result_of(held) {
                tally.elapsed_ms = tally.elapsed_ms.saturating_add(*elapsed_ms);
            }
        }
        tally
    }

    /// Whether `work` holds a number to say, counted without laying the row that says it: a row
    /// that would count nothing is no row.
    fn says_something(&self, work: &Range<usize>) -> bool {
        self.held[work.clone()].iter().any(|held| {
            matches!(held.what, What::Fact(Entry::Call { .. }))
                || matches!(result_of(held), Some(Entry::Result { elapsed_ms, .. }) if *elapsed_ms > 0)
        })
    }

    /// Whether `work` is a work the band draws as a row: the record has written past it and it
    /// holds a number to say, so it is one row closed and the rows it stood for opened.
    fn worth_a_row(&self, work: &Range<usize>) -> bool {
        self.written_past(work) && self.says_something(work)
    }

    /// Whether `work` is drawn as the one row that counts it: a work worth a row the reader has not
    /// opened.
    fn one_row(&self, work: &Range<usize>) -> bool {
        matches!(self.held[work.start].place, Place::Opens(Fold::Closed)) && self.worth_a_row(work)
    }

    /// The one row a settled work is drawn as: what it holds rather than how many rows it stands
    /// in, in the dim ink of a fact.
    fn work_row(&self, work: &Range<usize>, cols: u16) -> Row {
        marked(
            &KEPT.to_string(),
            Ink::Dim,
            vec![(self.tally(work).words(), Ink::Dim)],
            cols,
        )
    }

    /// The newest work a key may open: the newest one the band draws as a row, closed or open.
    fn newest_work(&self) -> Option<Range<usize>> {
        (0..self.held.len())
            .rev()
            .filter(|at| matches!(self.held[*at].place, Place::Opens(_)))
            .map(|at| self.work(at))
            .find(|work| self.worth_a_row(work))
    }

    /// The newest settled work's fold, opened if it is closed and folded if it is open: false where
    /// the transcript holds no work drawn as a row.
    pub fn toggle_newest(&mut self) -> bool {
        let Some(work) = self.newest_work() else {
            return false;
        };
        self.toggle(work.start)
    }

    /// The fold of the work `at` stands in, opened if closed and folded if open: what a press on a
    /// work's head row does. A work the band draws as no row, and an entry in no work, move nothing.
    pub fn toggle(&mut self, at: usize) -> bool {
        let work = self.work(at);
        if !self.worth_a_row(&work) {
            return false;
        }
        let to = match self.held[work.start].place {
            Place::Opens(Fold::Closed) => Fold::Open,
            Place::Opens(Fold::Open) => Fold::Closed,
            _ => return false,
        };
        self.opened(work.start, to)
    }

    /// The work entry `at` stands in, which is the one row of it a press opens or folds, and none
    /// for an entry the cache does not hold.
    pub fn work_of(&self, at: usize) -> Option<Range<usize>> {
        (at < self.held.len()).then(|| self.work(at))
    }

    /// The prompt that asked the exchange entry `at` belongs to: the one at it, or the last one
    /// above it.
    fn prompt_above(&self, at: usize) -> Option<usize> {
        (0..=at).rev().find(|entry| is_prompt(&self.held[*entry]))
    }

    /// Whether entry `at` is the answer a run ended on rather than the work of one.
    pub fn is_answer(&self, at: usize) -> bool {
        self.held.get(at).is_some_and(|held| {
            matches!(held.what, What::Fact(Entry::Assistant { .. })) && held.place == Place::Alone
        })
    }

    /// The entries of the exchange entry `at` belongs to: from the prompt that asked it to the entry
    /// before the next prompt, and none where no prompt stands above it.
    pub fn exchange(&self, at: usize) -> Option<Range<usize>> {
        if at >= self.held.len() {
            return None;
        }
        let start = self.prompt_above(at)?;
        let end = (start + 1..self.held.len())
            .find(|entry| is_prompt(&self.held[*entry]))
            .unwrap_or(self.held.len());
        Some(start..end)
    }

    /// Opens the fold entry `at` stands on, and answers whether this call is what moved it: the work
    /// where `at` belongs to a settled one the reader has not opened, and otherwise the fold the
    /// result itself keeps. A work drawn as no row and an entry with no fold answer nothing.
    pub fn open(&mut self, at: usize) -> bool {
        if at >= self.held.len() {
            return false;
        }
        let work = self.work(at);
        if self.one_row(&work) {
            return self.opened(work.start, Fold::Open);
        }
        self.fold(at, Fold::Open)
    }

    /// The work opening at `at` left in the fold `to`, which is a frame to paint when this call
    /// moved it: one already in `to` repaints nothing, and the rows it drew are laid once again.
    fn opened(&mut self, at: usize, to: Fold) -> bool {
        match self.held[at].place {
            Place::Opens(fold) if fold != to => {}
            _ => return false,
        }
        self.held[at].place = Place::Opens(to);
        self.held[at].rows = None;
        true
    }

    /// The result entry `at` left in the fold `to`, which is a frame to paint when this call moved
    /// it: one already in `to` repaints nothing, and a result its whole fold holds is no key's.
    fn fold(&mut self, at: usize, to: Fold) -> bool {
        let Some(held) = self.held.get(at) else {
            return false;
        };
        if !holds_result(held) || held.fold == to {
            return false;
        }
        if self.held[at].rows.is_none() {
            self.rows(at);
        }
        if self.held[at].kept.is_none() {
            return false;
        }
        self.held[at].fold = to;
        self.held[at].rows = None;
        true
    }

    /// A copy of the last answer takes the line breaks the model wrote, not the ones the
    /// band wrapped.
    pub fn last_answer(&self) -> Option<String> {
        self.held.iter().rev().find_map(|held| match &held.what {
            What::Fact(Entry::Assistant { text, .. }) => Some(text.to_string()),
            _ => None,
        })
    }

    /// The source of fence `fence` in entry `at`, one line per row it was laid as and the line breaks
    /// it was written with, a row that keeps none standing as a blank line: nothing where no row of
    /// the fence is laid, which is a fence that drew no line of its own.
    pub fn source(&self, at: usize, fence: usize) -> Option<String> {
        let rows = self.held.get(at)?.rows.as_ref()?;
        let lines: Vec<&str> = rows
            .iter()
            .filter(|row| row.line_of(fence))
            .map(|row| row.source().unwrap_or_default())
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    /// The transcript's whole height in rows, laying out every entry this cache has never
    /// seen: the one door to measuring a history, and a window never needs it.
    pub fn height(&mut self) -> u32 {
        let mut height: u32 = 0;
        for at in 0..self.held.len() {
            height = height.saturating_add(u32::try_from(self.rows(at).len()).unwrap_or(u32::MAX));
        }
        height
    }

    /// The top row of every prompt the record holds, counted from the transcript's first row: the
    /// blank row it opens under, but for the prompt the transcript opens with, whose words are its
    /// own top row.
    pub fn prompts(&mut self) -> Vec<u32> {
        let mut tops = Vec::new();
        let mut at: u32 = 0;
        for entry in 0..self.held.len() {
            if is_prompt(&self.held[entry]) {
                tops.push(at);
            }
            at = at.saturating_add(u32::try_from(self.rows(entry).len()).unwrap_or(u32::MAX));
        }
        tops
    }

    /// The thumb a window `scroll` rows behind the tail of this transcript draws in a band `band`
    /// rows tall, and none where there is nothing left out of view or the band holds no gutter.
    pub fn thumb(&mut self, scroll: Scroll, band: u16) -> Option<Thumb> {
        let total = self.height();
        // A band this narrow leaves a wide glyph no room beside the gutter, and the row gives the
        // gutter up for it: there the thumb would stand on the word instead.
        if self.measure.cols <= pen::cells(INDENT) + 1
            || band < 2
            || total <= u32::from(band)
            || scroll.back() == 0
        {
            return None;
        }
        let deepest = u64::from(total - u32::from(band));
        let above = deepest.saturating_sub(u64::from(scroll.back()));
        let band = u64::from(band);
        let total = u64::from(total);
        // The band's share of the whole, rounded to the nearest cell and never to none: a thumb the
        // reader cannot see says nothing about where they stand.
        let span = ((band * band + total / 2) / total).max(1);
        let room = band - span;
        let top = (room * above + deepest / 2) / deepest;
        Some(Thumb {
            top: u16::try_from(top.min(room)).unwrap_or(u16::MAX),
            span: u16::try_from(span).unwrap_or(u16::MAX),
        })
    }

    /// How far back a window of `height` rows may stand so its top is the transcript's first row.
    pub fn deepest(&mut self, height: u16) -> u32 {
        self.height().saturating_sub(u32::from(height))
    }

    /// The entries a band of `height` rows at `scroll` intersects, in order. The walk is
    /// measured from the tail and stops once the band is filled, so a window costs the
    /// entries between the tail and it, laid out once each.
    pub fn window(&mut self, scroll: Scroll, height: u16) -> Window {
        let back = scroll.back();
        let top = back.saturating_add(u32::from(height));
        let mut below = 0;
        let mut pieces = Vec::new();
        for at in (0..self.held.len()).rev() {
            if below >= top {
                break;
            }
            let filled = u32::try_from(self.rows(at).len()).unwrap_or(u32::MAX);
            let skip = below.saturating_add(filled).saturating_sub(top);
            let cut = back.saturating_sub(below);
            let rows = u16::try_from(filled)
                .unwrap_or(u16::MAX)
                .saturating_sub(u16::try_from(skip).unwrap_or(u16::MAX))
                .saturating_sub(u16::try_from(cut).unwrap_or(u16::MAX));
            if rows > 0 {
                pieces.push(Piece {
                    entry: at,
                    skip: u16::try_from(skip).unwrap_or(u16::MAX),
                    rows,
                });
            }
            below = below.saturating_add(filled);
        }
        pieces.reverse();
        Window { pieces }
    }
}

impl Deltas for Rows {
    fn text(&mut self, fragment: &str) {
        self.in_flight.text(fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.in_flight.reasoning(fragment);
    }
}

/// Whether this entry is the reader's own words, which is the row an exchange opens with.
fn is_prompt(held: &Held) -> bool {
    matches!(held.what, What::Fact(Entry::Prompt { .. }))
}

/// Whether this entry is the work of its exchange: a turn, a thought, a call or a result. A
/// prompt, an answer and a stop are the exchange's ends and stand alone.
fn work_entry(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::Turn { .. } | Entry::Reasoning { .. } | Entry::Call { .. } | Entry::Result { .. }
    )
}

/// The result this entry holds: either the fact itself, or the result that settled the call it
/// holds.
fn result_of(held: &Held) -> Option<&Entry> {
    match &held.what {
        What::Fact(result @ Entry::Result { .. }) => Some(result),
        _ => held.result.as_ref(),
    }
}

/// Whether this entry is a result, and so holds a fold of its own.
fn holds_result(held: &Held) -> bool {
    result_of(held).is_some()
}

#[derive(Clone, Debug, Default)]
pub struct Window {
    pieces: Vec<Piece>,
}

impl Window {
    pub fn pieces(&self) -> &[Piece] {
        &self.pieces
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub entry: usize,
    pub skip: u16,
    pub rows: u16,
}
