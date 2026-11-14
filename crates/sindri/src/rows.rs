//! The rows an entry is drawn as, cached beside the width and the surface they were
//! laid out at: either one changing drops them all, because a row's height is a fact
//! about that measurement and about nothing else.

use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::open::{self, Ground, Opening};
use sindri_core::records::Entry;
use sindri_core::run::Exit;
use sindri_core::tools::OPEN;

use crate::ink::{Ink, Look};
use crate::link::{Link, Links, Way};
use crate::mark;
use crate::markdown;
use crate::palette::{Band, Palette};
use crate::pen::{self, Clip};
use crate::picture::{self, Surface};
use crate::pulse::Head;
use crate::size::{self, Sized, Sizes};
use crate::stream::Stream;

/// The gutter every row of every entry opens with, two columns of it: the glyph that says what the
/// row is and the blank after it, so every entry's words begin in one column.
pub(crate) const INDENT: &str = "  ";

const RULE: &str = "─ ";
const SIGIL: &str = "› ";
const THOUGHT: &str = "│ ";

const PENDING: char = '…';
const CHECK: char = '✓';
const CROSS: char = '✗';
const HAIRLINE: char = '─';

/// One row of the transcript: never wider than the width it was laid out at, bar the
/// one glyph that is itself wider than that whole width and that no layout can split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    spans: Vec<(String, Look)>,
    /// The runs of this row's cells that are ways to somewhere, measured from its left edge: a
    /// destination is no cell's own, so it is kept beside the words and written by the frame.
    links: Vec<Link>,
    /// The picture this row stands on, when it is the first row of a block one is drawn
    /// over: the placement goes to the terminal, the row's words are what a copy takes.
    picture: Option<picture::Picture>,
    /// The band this row sits on, when it is one row of a well: a code fence is a block of
    /// cells, so each of its rows paints the surface under its words.
    band: Option<Band>,
    /// The runs of this row a terminal draws at more than one cell: the row is measured by the
    /// cells they leave, and its spans say the words they are written as.
    sizes: Vec<Sized>,
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
            band: None,
            sizes: Vec::new(),
        }
    }

    /// The same row with one more run drawn at more than one cell: the run covers the cells of the
    /// words its span says, so a row of them is measured by the footprint and not by the glyphs.
    pub fn sized(mut self, run: Sized) -> Row {
        self.sizes.push(run);
        self
    }

    /// The same row, drawn on `band`: the surface reaches the cells its words leave blank, so a
    /// well is one block and not a stripe behind its glyphs.
    pub fn banded(mut self, band: Band) -> Row {
        self.band = Some(band);
        self
    }

    pub fn band(&self) -> Option<Band> {
        self.band
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
            band: None,
            sizes: Vec::new(),
        }
    }

    /// The run of this row's cells that begins `left` columns in and is `width` columns wide is a
    /// way to `destination`: the words are drawn as they are, and the frame writes the way around
    /// them. A row that names none is drawn as it was.
    pub fn linking(mut self, left: u16, width: u16, destination: impl Into<String>) -> Row {
        self.links.push(Link::new(left, width, destination));
        self
    }

    pub(crate) fn links(&self) -> &[Link] {
        &self.links
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

    pub fn indented(&self, gutter: &str, ink: Ink) -> Row {
        if self.spans.is_empty() {
            return self.clone();
        }
        let mut spans: Vec<(String, Look)> = Vec::with_capacity(self.spans.len() + 1);
        spans.push((String::from(gutter), Look::plain(ink)));
        spans.extend(self.spans.iter().cloned());
        let shifts = pen::cells(gutter);
        Row {
            picture: self.picture.clone(),
            band: self.band,
            links: self.links.iter().map(|link| link.shifted(shifts)).collect(),
            sizes: self.sizes.iter().map(|run| run.shifted(shifts)).collect(),
            ..Row::of(spans)
        }
    }

    /// This row with `other`'s words set after its own, and the ways `other` names moved past the
    /// columns this row covers: two cells of a table are one row, and each keeps its own ways.
    pub(crate) fn followed_by(self, other: &Row) -> Row {
        let shifts = self.width();
        let mut spans = self.spans;
        spans.extend(other.spans.iter().cloned());
        let mut links = self.links;
        links.extend(other.links.iter().map(|link| link.shifted(shifts)));
        let mut sizes = self.sizes;
        sizes.extend(other.sizes.iter().map(|run| run.shifted(shifts)));
        Row {
            spans,
            links,
            picture: self.picture,
            band: self.band,
            sizes,
        }
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
        pen::write(buffer, area, &self.spans, Clip::Edge, palette);
        links.lay(area.y, area.x, &self.links);
        sizes.lay(area.y, area.x, &self.sizes);
    }
}

pub fn layout(entry: &Entry, cols: u16) -> Vec<Row> {
    layout_on(entry, cols, &Surface::none(), None)
}

/// The same entry on a `surface`, with the `ground` a run judges a target from: a path a call names
/// is a way only where that judgement holds it openable, and a frame handed none lays every path as
/// the words it was written as.
pub fn layout_on(entry: &Entry, cols: u16, surface: &Surface, ground: Option<&Ground>) -> Vec<Row> {
    let cols = cols.max(1);
    match entry {
        Entry::Turn { turn } => vec![divider(&format!("turn {turn} "), cols)],
        Entry::Call {
            name, arguments, ..
        } => opening(&name.to_string(), &arguments.to_string(), cols, ground),
        Entry::Result {
            name,
            text,
            bytes,
            elapsed_ms,
            exit,
            ..
        } => settled(
            &name.to_string(),
            *exit,
            &text.to_string(),
            *bytes,
            *elapsed_ms,
            cols,
        ),
        Entry::Assistant { text, .. } => answer_on(&text.to_string(), cols, None, surface),
        Entry::Reasoning { text, .. } => thought(&text.to_string(), cols),
        Entry::Prompt { text } => asked(&text.to_string(), cols),
    }
}

fn asked(text: &str, cols: u16) -> Vec<Row> {
    let (gutter, room) = margin(SIGIL, cols);
    let blanks = match gutter.is_empty() {
        true => "",
        false => INDENT,
    };
    let mut rows = vec![divider("prompt ", cols)];
    let mut lines = wrap(text, room).into_iter();
    if let Some(first) = lines.next() {
        rows.push(words_past(gutter, Ink::Plain, Ink::Plain, first, cols));
    }
    rows.extend(lines.map(|line| words_past(blanks, Ink::Plain, Ink::Plain, line, cols)));
    rows
}

pub(crate) fn thought(text: &str, cols: u16) -> Vec<Row> {
    let (gutter, room) = margin(THOUGHT, cols);
    wrap(text, room)
        .into_iter()
        .map(|line| words_past(gutter, Ink::Dim, Ink::Muted, line, cols))
        .collect()
}

/// The rows an answer is drawn as, set in the answer's gutter — except the row the run's marker
/// heads, which stands the marker and its blank column in that gutter and lays that row's words
/// out beside them.
pub(crate) fn answer_on(text: &str, cols: u16, head: Option<&Head>, surface: &Surface) -> Vec<Row> {
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
    let mut rows = markdown::rows_in(text, cols, reserve, gutter, surface).into_iter();
    let mut laid: Vec<Row> = Vec::new();
    if let Some(head) = head
        && let Some(first) = rows.next()
    {
        // The marker stands in the gutter and the words it says take the reserve, so the
        // answer's first row begins where every other row of it does.
        let mut spans: Vec<(String, Look)> = head_row(head, cols).spans().to_vec();
        spans.push((String::from(" "), Look::plain(Ink::Plain)));
        // The markdown row opens with the gutter the marker takes, so its first span goes.
        spans.extend(first.spans().iter().skip(1).cloned());
        laid.push(Row {
            band: first.band,
            links: first
                .links()
                .iter()
                .map(|link| link.shifted(reserve))
                .collect(),
            sizes: first.sizes.iter().map(|run| run.shifted(reserve)).collect(),
            ..Row::of(spans)
        });
    }
    laid.extend(rows);
    laid
}

/// The row a run's marker heads: the mark stands in the gutter and the words it says begin where
/// the entry's own do.
pub(crate) fn head_row(head: &Head, cols: u16) -> Row {
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

fn opening(name: &str, arguments: &str, cols: u16, ground: Option<&Ground>) -> Vec<Row> {
    let mut rows = vec![marked(
        &PENDING.to_string(),
        Ink::Dim,
        vec![(String::from(name), Ink::Dim)],
        cols,
    )];
    rows.extend(body(arguments, cols, way(name, arguments, ground)));
    rows
}

/// The way a call names, when the run judged it one: the path as its words were written, which is
/// how the row's cells are found, and the file the judgement resolved them to. A call that names
/// nothing the run would open is no way at all.
fn way(name: &str, arguments: &str, ground: Option<&Ground>) -> Option<Way> {
    if name != OPEN {
        return None;
    }
    let written = open::named_path(arguments)?;
    match ground?.judge(arguments).ok()? {
        Opening::File(path) => Some(Way::of(written, &path)),
        Opening::Url(_) => None,
    }
}

/// A result with no verdict is a line a build wrote before the verdict was a fact: the
/// row says the name and the numbers rather than guessing `exit 0` from the text beside
/// it.
fn settled(
    name: &str,
    exit: Option<Exit>,
    text: &str,
    bytes: u64,
    elapsed_ms: u64,
    cols: u16,
) -> Vec<Row> {
    let named = format!("{name}  {bytes} bytes · {elapsed_ms} ms");
    let header = match exit {
        Some(exit) => marked(
            &mark(exit).to_string(),
            ink_of(exit),
            vec![(format!("{}  {named}", exit.word()), Ink::Dim)],
            cols,
        ),
        None => {
            let (gutter, room) = margin(INDENT, cols);
            fitted(vec![(named, Ink::Dim)], room).indented(gutter, Ink::Plain)
        }
    };
    let mut rows = vec![header];
    rows.extend(body(text, cols, None));
    rows
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

fn clip(text: &str, room: u16) -> String {
    if pen::cells(text) <= room {
        return String::from(text);
    }
    split_at(text, usize::from(room.max(1))).0
}

/// The words of a tool's arguments or its result, set in the gutter a body is set from. A body
/// that names a way to a file is laid with that path's own cells a way there, and every other
/// body is laid as the wrapping lays it.
fn body(text: &str, cols: u16, way: Option<Way>) -> Vec<Row> {
    if text.is_empty() {
        return Vec::new();
    }
    let (gutter, room) = margin(INDENT, cols);
    let named = way.and_then(|way| markdown::body(text, room, &way));
    match named {
        Some(rows) => rows
            .into_iter()
            .map(|row| row.indented(gutter, Ink::Plain))
            .collect(),
        None => wrap(text, room)
            .into_iter()
            .map(|line| words_past(gutter, Ink::Plain, Ink::Plain, line, cols))
            .collect(),
    }
}

/// The row between one turn and the next: the hairline it opens with, the words naming the turn,
/// and the pen's fill out to the band's edge.
fn divider(lead: &str, cols: u16) -> Row {
    let (gutter, room) = margin(RULE, cols);
    let lead = clip(lead, room);
    let filled = room.saturating_sub(pen::cells(&lead));
    let line = format!("{lead}{}", pen::filled(filled, HAIRLINE));
    Row::of([(line, Ink::Dim)]).indented(gutter, Ink::Dim)
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
    pub fn top(&self) -> u16 {
        self.top
    }

    pub fn span(&self) -> u16 {
        self.span
    }

    pub fn rows(&self) -> Range<u16> {
        self.top..self.top + self.span
    }

    /// The thumb stands in the gutter's glyph column: the gutter's blank column is what keeps the
    /// words off it, so it takes no column the words were set in. It draws one cell, on `row` of
    /// the band, and nothing on a row the thumb does not cover.
    pub(crate) fn paint(&self, area: Rect, row: u16, buffer: &mut Buffer, palette: &Palette) {
        if !self.rows().contains(&row) {
            return;
        }
        let y = area.y.saturating_add(row);
        pen::glyph(buffer, area.x, y, THUMB, Ink::Dim, palette);
    }
}

/// Every entry a transcript holds, each with the rows it was laid out as at one width,
/// and nothing it was not drawn as.
#[derive(Clone, Debug, Default)]
pub struct Rows {
    cols: u16,
    held: Vec<Held>,
    /// How many layouts this cache has done. No code reads it: a test holds the laying out to a
    /// count rather than to a shape.
    lays: usize,
    /// What a picture in an answer is measured against — the cell, and whether this terminal
    /// places one — because how many rows a picture takes is part of the rows it was laid out as.
    surface: Surface,
    ground: Option<Ground>,
}

#[derive(Clone, Debug)]
struct Held {
    what: What,
    /// What came back, when this holds a call whose result has landed: kept rather than
    /// layered over the rows, so a width change re-lays the settled row.
    result: Option<Entry>,
    rows: Option<Vec<Row>>,
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

    /// Holds one more entry, unlaid. A result settles the row the call it answers opened
    /// rather than opening a line under it — unless this cache never held that call, which
    /// is an older transcript's entry read on its own, and then it opens a row like any other.
    pub fn push(&mut self, entry: Entry) {
        if let Entry::Result { turn, .. } = &entry
            && let Some(at) = self.pending(*turn)
        {
            self.settle(at, entry);
            return;
        }
        self.retire();
        self.held.push(Held {
            what: What::Fact(entry),
            result: None,
            rows: None,
        });
    }

    /// Holds the turn in flight as the cache's last entry. A stream asked again without
    /// having moved lays nothing, and the mark counts as movement because it is laid into the
    /// rows it heads: a held one would wear a clock that has moved on.
    pub fn live(&mut self, stream: &Stream, head: Option<&Head>) {
        if stream.is_empty() && head.is_none() {
            self.retire();
            return;
        }
        let revision = stream.revision();
        let head = head.cloned();
        if let Some(Held {
            what:
                What::Live {
                    revision: at,
                    head: mark,
                },
            rows: Some(_),
            ..
        }) = self.held.last()
            && *at == revision
            && *mark == head
        {
            return;
        }
        let rows = stream.rows_on(self.cols, head.as_ref(), &self.surface);
        match self.held.last_mut() {
            Some(held) if matches!(held.what, What::Live { .. }) => {
                held.what = What::Live { revision, head };
                held.rows = Some(rows);
            }
            _ => self.held.push(Held {
                what: What::Live { revision, head },
                result: None,
                rows: Some(rows),
            }),
        }
        self.lays += 1;
    }

    fn retire(&mut self) {
        if matches!(
            self.held.last().map(|held| &held.what),
            Some(What::Live { .. })
        ) {
            self.held.pop();
        }
    }

    /// The oldest call of `turn` whose result has not landed, because a wave of calls and
    /// the wave of results that answers it have to pair up in the order both were written.
    fn pending(&self, turn: u32) -> Option<usize> {
        self.held.iter().position(|held| {
            held.result.is_none()
                && matches!(&held.what, What::Fact(Entry::Call { turn: at, .. }) if *at == turn)
        })
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

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn lays(&self) -> usize {
        self.lays
    }

    /// A different width drops every row: this cache holds one width at a time, so going
    /// back to a width is a reflow and not a lookup.
    pub fn resize(&mut self, cols: u16) {
        let cols = cols.max(1);
        if cols == self.cols {
            return;
        }
        self.cols = cols;
        self.unlay();
    }

    /// Where the run stands when it judges a target: a ground that arrives or moves drops every
    /// row, because whether a path a call names is a way is a fact of that row.
    pub fn stand(&mut self, ground: Option<&Ground>) {
        if ground == self.ground.as_ref() {
            return;
        }
        self.ground = ground.cloned();
        self.unlay();
    }

    pub fn retune(&mut self, surface: Surface) {
        if surface == self.surface {
            return;
        }
        self.surface = surface;
        self.unlay();
    }

    /// Drops every row: a fact a row was laid out against has moved, and a row is only a drawing
    /// of the facts that made it.
    fn unlay(&mut self) {
        for held in &mut self.held {
            held.rows = None;
        }
    }

    /// The rows of entry `at`, laid out now if they do not exist and kept for the next
    /// paint. `at` is an index this cache handed out; the live entry has no fact behind it,
    /// so nothing is drawn for it here.
    pub fn rows(&mut self, at: usize) -> &[Row] {
        let cols = self.cols;
        let held = &self.held[at];
        let fact = match (&held.what, held.result.as_ref()) {
            (What::Fact(entry), None) => Some(entry),
            (What::Fact(_), Some(result)) => Some(result),
            (What::Live { .. }, _) => None,
        };
        if held.rows.is_none()
            && let Some(fact) = fact
        {
            let rows = layout_on(fact, cols, &self.surface, self.ground.as_ref());
            self.lays += 1;
            self.held[at].rows = Some(rows);
        }
        self.held[at].rows.as_deref().unwrap_or_default()
    }

    /// A copy of the last answer takes the line breaks the model wrote, not the ones the
    /// band wrapped.
    pub fn last_answer(&self) -> Option<String> {
        self.held.iter().rev().find_map(|held| match &held.what {
            What::Fact(Entry::Assistant { text, .. }) => Some(text.to_string()),
            _ => None,
        })
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

    /// The thumb a window `scroll` rows behind the tail of a transcript `total` rows tall draws in
    /// a band `band` rows tall, and none where the transcript is no taller than the band, the band
    /// is one row, the band has given up its gutter, or the reader is at the tail.
    pub fn thumb(&self, scroll: Scroll, total: u32, band: u16) -> Option<Thumb> {
        if self.cols() <= pen::cells(INDENT)
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
