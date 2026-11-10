//! The rows an entry is drawn as, cached beside the width and the surface they were
//! laid out at: either one changing drops them all, because a row's height is a fact
//! about that measurement and about nothing else.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;
use sindri_core::records::Entry;
use sindri_core::run::Exit;

use crate::ink::{Ink, Look};
use crate::markdown;
use crate::palette::{Band, Palette};
use crate::picture::{self, Surface};
use crate::pulse::Head;
use crate::stream::Stream;

/// How far a body sits inside the row that names it, and the blank gutter an answer is
/// printed from — the columns the newest answer's marker takes the place of.
const INDENT: &str = "  ";

/// The hairline a thought opens with: two columns, the margin an answer is set from,
/// so the two kinds of row line up.
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
    /// The picture this row stands on, when it is the first row of a block one is drawn
    /// over: the placement goes to the terminal, the row's words are what a copy takes.
    picture: Option<picture::Picture>,
    /// The band this row sits on, when it is one row of a well: a code fence is a block of
    /// cells, so each of its rows paints the surface under its words.
    band: Option<Band>,
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
            picture: None,
            band: None,
        }
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
            picture: None,
            band: None,
        }
    }

    pub fn width(&self) -> u16 {
        let columns: usize = self.spans.iter().map(|(text, _)| glyphs(text)).sum();
        u16::try_from(columns).unwrap_or(u16::MAX)
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
        Row {
            picture: self.picture.clone(),
            band: self.band,
            ..Row::of(spans)
        }
    }

    pub fn glyphs(&self) -> String {
        self.spans.iter().map(|(text, _)| text.as_str()).collect()
    }

    pub fn paint(&self, area: Rect, buffer: &mut Buffer, palette: &Palette) {
        if let Some(band) = self.band {
            buffer.set_style(area, palette.surface(band));
        }
        let mut x = area.x;
        for (text, look) in &self.spans {
            if x >= area.right() {
                return;
            }
            let room = usize::from(area.right() - x);
            let (next, _) = buffer.set_stringn(x, area.y, text, room, look.style(palette));
            x = next;
        }
    }
}

pub fn layout(entry: &Entry, cols: u16) -> Vec<Row> {
    layout_on(entry, cols, &Surface::none())
}

pub fn layout_on(entry: &Entry, cols: u16, surface: &Surface) -> Vec<Row> {
    let cols = cols.max(1);
    match entry {
        Entry::Turn { turn } => vec![rule(&format!("turn {turn} "), cols)],
        Entry::Call {
            name, arguments, ..
        } => opening(&name.to_string(), &arguments.to_string(), cols),
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
    }
}

pub(crate) fn thought(text: &str, cols: u16) -> Vec<Row> {
    let (gutter, room) = margin(THOUGHT, cols);
    wrap(text, room)
        .into_iter()
        .map(|line| Row::of([(gutter.to_owned(), Ink::Dim), (line, Ink::Muted)]))
        .collect()
}

/// The rows an answer is drawn as, set in the answer's gutter — except the row the run's marker
/// heads, which stands the marker and its blank column in that gutter and lays that row's words
/// out beside them.
pub(crate) fn answer_on(text: &str, cols: u16, head: Option<&Head>, surface: &Surface) -> Vec<Row> {
    let (gutter, room) = margin(INDENT, cols);
    // The marker's trailing blank column counts against the width; the gutter it stands
    // in the place of does not.
    let reserve = match head {
        Some(head) => head
            .width()
            .saturating_add(1)
            .saturating_sub(u16::try_from(glyphs(gutter)).unwrap_or(u16::MAX)),
        None => 0,
    };
    let mut rows = markdown::rows_on(text, room, reserve, surface).into_iter();
    let mut laid: Vec<Row> = Vec::new();
    if let Some(head) = head
        && let Some(first) = rows.next()
    {
        let mut spans: Vec<(String, Look)> = head
            .spans()
            .iter()
            .map(|(text, ink)| (text.clone(), Look::plain(*ink)))
            .collect();
        spans.push((String::from(" "), Look::plain(Ink::Plain)));
        spans.extend(first.spans().iter().cloned());
        laid.push(Row::of(spans));
    }
    laid.extend(rows.map(|row| row.indented(gutter, Ink::Plain)));
    laid
}

/// The gutter a body opens with at `cols`, and the columns its words have beside it: a
/// band too narrow to hold both the gutter and one glyph gives up the gutter, never the
/// invariant that no row is laid out wider than the width it was laid out for.
fn margin(gutter: &str, cols: u16) -> (&str, u16) {
    let width = u16::try_from(glyphs(gutter)).unwrap_or(cols);
    // Two columns, because that is the widest one glyph is: a margin that left less would
    // cost a row the whole width it was laid out for.
    match cols > width.saturating_add(1) {
        true => (gutter, cols - width),
        false => ("", cols),
    }
}

fn opening(name: &str, arguments: &str, cols: u16) -> Vec<Row> {
    let mut rows = vec![fitted(vec![(format!("{PENDING} {name}"), Ink::Dim)], cols)];
    rows.extend(body(arguments, cols));
    rows
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
        Some(exit) => fitted(
            vec![
                (mark(exit).to_string(), ink_of(exit)),
                (format!(" {}  {named}", exit.word()), Ink::Dim),
            ],
            cols,
        ),
        None => fitted(vec![(named, Ink::Dim)], cols),
    };
    let mut rows = vec![header];
    rows.extend(body(text, cols));
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
        room = room.saturating_sub(glyphs(&head));
        kept.push((head, ink));
        if !rest.is_empty() {
            break;
        }
    }
    Row::of(kept)
}

fn clip(text: &str, room: u16) -> String {
    if glyphs(text) <= usize::from(room) {
        return String::from(text);
    }
    split_at(text, usize::from(room.max(1))).0
}

fn body(text: &str, cols: u16) -> Vec<Row> {
    if text.is_empty() {
        return Vec::new();
    }
    let (gutter, room) = margin(INDENT, cols);
    wrap(text, room)
        .into_iter()
        .map(|line| Row::of([(gutter.to_owned(), Ink::Plain), (line, Ink::Plain)]))
        .collect()
}

fn rule(lead: &str, cols: u16) -> Row {
    let lead = clip(lead, cols);
    let filled = usize::from(cols).saturating_sub(glyphs(&lead));
    let line = format!("{lead}{}", HAIRLINE.to_string().repeat(filled));
    Row::of([(line, Ink::Dim)])
}

fn wrap(text: &str, cols: u16) -> Vec<String> {
    wrap_under(text, cols, cols)
}

fn wrap_under(text: &str, first: u16, room: u16) -> Vec<String> {
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
                if width + gap + glyphs(word) <= limit {
                    if gap == 1 {
                        row.push(' ');
                        width += 1;
                    }
                    row.push_str(word);
                    width += glyphs(word);
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
        let mut encoded = [0; 4];
        let wide = glyphs(glyph.encode_utf8(&mut encoded));
        if at > 0 && width + wide > room {
            return (word[..at].to_owned(), &word[at..]);
        }
        width += wide;
    }
    (word.to_owned(), "")
}

/// The columns a run of glyphs takes, measured and never counted: a glyph can be two
/// cells wide.
pub(crate) fn glyphs(text: &str) -> usize {
    Span::raw(text).width()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scroll {
    #[default]
    Tail,
    Back(u32),
}

impl Scroll {
    fn back(self) -> u32 {
        match self {
            Scroll::Tail => 0,
            Scroll::Back(rows) => rows,
        }
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
        for held in &mut self.held {
            held.rows = None;
        }
    }

    pub fn retune(&mut self, surface: Surface) {
        if surface == self.surface {
            return;
        }
        self.surface = surface;
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
            let rows = layout_on(fact, cols, &self.surface);
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
