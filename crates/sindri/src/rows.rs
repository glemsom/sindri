//! The transcript's rows: one entry of the core's records laid out as the rows it is
//! drawn as, each row a run of glyphs in one ink, kept in a cache beside the width it
//! was laid out at (ADR-0008). Rendering lives in this front end and not in the core
//! because an entry is a fact and never a rendering (ADR-0007), and because the core
//! may not name a terminal at all (ADR-0005).
//!
//! The unit is the entry. A [`Rows`] holds every entry it was handed, each with the
//! rows it was laid out as or with nothing when this cache has never laid it out, so a
//! height is free once an entry has been seen, a streaming tick that changes one entry
//! re-lays that entry alone, and a width change drops what the cache held and reflows
//! on the next paint — once, because a paint lays an entry out to draw it and then
//! keeps the answer.
//!
//! The turn in flight is the exception that proves the rule: a [`Stream`] is no fact
//! (ADR-0007), so [`Rows::live`] hands the cache the rows it is drawn as right now and
//! the cache holds them as its last entry — the live one — laid out again from the stream
//! whenever a fragment has landed and never laid out from an entry, because there is no
//! entry yet. A fact landing retires it, so what a turn said is drawn from the entries it
//! settled into and is never on the screen twice.
//!
//! What a band of rows shows is [`Rows::window`]: the entries it intersects, with how
//! much of each is inside it. The window is measured from the tail backwards, so
//! following the newest row never asks how tall the history is — it lays out the
//! entries it shows and stops at the first row that fills the band, which is §12's
//! rule that only on-screen rows are laid out.
//!
//! What that buys is the plain case and what it costs is the scrolled one, and the
//! decision is this: a window is a row offset from the tail, so resolving one walks
//! the entries between the tail and the window and lays out every one of them on the
//! way. Each is laid out once and kept, so the whole history scrolled through costs
//! one layout per entry and scrolling it again costs none — but a reader who jumps a
//! long way back through a history this process has never drawn pays for what they
//! jumped over. Heights come from the cache when it has them and from a layout when it
//! does not, and nothing asks for all of them: [`Rows::height`] is the one door to
//! measuring a whole transcript, it lays out every entry the cache has never seen, and
//! #50's marking and `↓ 4 new rows` are what will spend it. The window never does.
//!
//! An answer and a thought are the two things the model says, and each is drawn in its
//! own gutter and its own ink (§4): an answer from the two blank columns that #39's pulse
//! will sit in, and a thought inside the hairline rule that says it was said to the
//! side. Markdown is #52's, and it replaces the plain rendering below; the one rule
//! that outlives it is ADR-0008's — a fenced block is drawn as code until its closing
//! fence arrives, so a fence still streaming is a code block and never prose that ate
//! the rest of the answer.
//!
//! `Call` and `Result` rows are deliberately plain here. What a tool row says — `✓ exit
//! 0`, `✗ exit N`, `✗ timed out`, `✗ cancelled` — and the faded ink §12 paints tool
//! calls in are ticket #40's contract: it changes these two shapes and their golden
//! frame in the same commit, and nothing else has to move with it.
//!
//! A call is one row. It opens naming the call in the faded ink §12 paints tool calls
//! in, and it settles saying what happened to it — `✓ exit 0`, `✗ exit N`, `✗ timed out`,
//! `✗ cancelled` — the check or the cross in `ok`/`err` and the call's name beside it.
//! The verdict is a fact the core carries (ADR-0007), never a sentence this file reads
//! back out of the text, so the picture cannot say a call succeeded because the words
//! under it happened to begin with `exit 0`.
//!
//! The record keeps two entries, a call and its result, and the picture keeps one row:
//! a result settles the row the call it answers opened rather than opening a line under
//! it, which is why [`Rows::settle`] exists — the row is laid again, alone, and the
//! history above and below it keeps what it was drawn as (ADR-0008).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;
use sindri_core::records::Entry;
use sindri_core::run::Exit;

use crate::ink::{Ink, Look};
use crate::markdown;
use crate::stream::Stream;

/// How far a body sits inside the row that names it, and how far an answer is printed
/// from the frame's left edge: one step, the step a past run is read back with too, so
/// a call and what it answered read as one thing said in two rows rather than as two
/// entries that happened to follow each other. An answer's own gutter is this margin
/// held open and blank — the two columns #39's pulse at the head of the newest answer
/// will sit in.
const INDENT: &str = "  ";

/// The gutter a thought row opens with: a hairline down the left of the prose the run
/// reasoned, so a thought reads as something said beside the answer and not as part of
/// it (§12). Two columns wide, which is the margin an answer is set from, so the two
/// kinds of row line up under each other.
const THOUGHT: &str = "│ ";

/// The glyph a call opens with, before anything has happened to it: a row that claims
/// nothing and is drawn all the same, because a call in flight is a row of the
/// transcript and not a gap in it.
const PENDING: char = '…';

/// The glyph a call that finished well settles on, and the one a call that did not: the
/// whole difference between §12's four states is this pair, the words beside them and
/// the ink they are drawn in.
const CHECK: char = '✓';
const CROSS: char = '✗';

/// The glyph a turn opens with, repeated to the right edge: the transcript's own
/// hairline, one column wide, so a long history reads as a ledger with dividers in it
/// rather than as one wall of prose.
const HAIRLINE: char = '─';

/// One row of the transcript: the runs of glyphs it is made of, each in its own ink,
/// and never wider than the width it was laid out at — bar the one glyph that is itself
/// wider than that whole width, which a one-column band would meet and no layout can
/// split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The runs of glyphs, left to right, each written in the look it was given.
    spans: Vec<(String, Look)>,
}

impl Row {
    /// A row of `spans`, each run of glyphs drawn in its own ink. A run of no glyphs is
    /// no run at all: it is dropped here, so a band too narrow to hold a gutter beside
    /// one glyph of prose leaves no empty span behind for a reader to puzzle at.
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
        }
    }

    /// A row that says nothing: the blank row two blocks of markdown stand apart by,
    /// which is still a row of the transcript and not a gap in it.
    pub fn blank() -> Row {
        Row { spans: Vec::new() }
    }

    /// The columns this row takes when it is drawn.
    pub fn width(&self) -> u16 {
        let columns: usize = self.spans.iter().map(|(text, _)| glyphs(text)).sum();
        u16::try_from(columns).unwrap_or(u16::MAX)
    }

    /// The runs of glyphs this row is made of, left to right, each with the ink it is
    /// written in: what a caller reads a laid-out row back as — the words a picture
    /// copies, the gutter a test holds a row to — without a second way to lay one out.
    pub fn spans(&self) -> &[(String, Look)] {
        &self.spans
    }

    /// This row set one gutter further in: the margin an entry is printed from, which
    /// a markdown row knows nothing of and the entry that asked for it knows the whole
    /// of (§12's answer gutter). A row of nothing stays a row of nothing.
    pub fn indented(&self, gutter: &str, ink: Ink) -> Row {
        if self.spans.is_empty() {
            return self.clone();
        }
        let mut spans: Vec<(String, Look)> = Vec::with_capacity(self.spans.len() + 1);
        spans.push((String::from(gutter), Look::plain(ink)));
        spans.extend(self.spans.iter().cloned());
        Row::of(spans)
    }

    /// Draws this row across the top row of `area`, from its left edge and clipped at
    /// its right one: a row is laid out at a width and drawn at that width or narrower,
    /// so the band's right edge is where a row stops and never where it runs over.
    pub fn paint(&self, area: Rect, buffer: &mut Buffer) {
        let mut x = area.x;
        for (text, look) in &self.spans {
            if x >= area.right() {
                return;
            }
            let room = usize::from(area.right() - x);
            let (next, _) = buffer.set_stringn(x, area.y, text, room, look.style());
            x = next;
        }
    }
}

/// One entry laid out at `cols` columns: the rows it is drawn as, which is the whole of
/// what the cache holds for it.
///
/// Every variant is facts the entry carries and nothing it does not: the tool, the
/// arguments as the wire wrote them, the words the model answered with. Markdown inside
/// an answer is #38's, the pulse at the head of the newest one is #39's, and the tool
/// row's own contract is #40's.
pub fn layout(entry: &Entry, cols: u16) -> Vec<Row> {
    let cols = cols.max(1);
    match entry {
        Entry::Turn { turn } => vec![rule(&format!("turn {turn} "), cols)],
        Entry::Call {
            name, arguments, ..
        } => opening(name, &arguments.to_string(), cols),
        Entry::Result {
            name,
            text,
            bytes,
            elapsed_ms,
            exit,
            ..
        } => settled(name, *exit, &text.to_string(), *bytes, *elapsed_ms, cols),
        Entry::Assistant { text, .. } => answer(&text.to_string(), cols),
        Entry::Reasoning { text, .. } => thought(&text.to_string(), cols),
    }
}

/// The rows a thought is drawn as: the prose the run reasoned, set inside the rule
/// §12 gives reasoning and read in the ink secondary prose is read in, so what a run
/// said to itself is legible as thinking and never mistaken for the answer.
pub(crate) fn thought(text: &str, cols: u16) -> Vec<Row> {
    let (gutter, room) = margin(THOUGHT, cols);
    wrap(text, room)
        .into_iter()
        .map(|line| Row::of([(gutter.to_owned(), Ink::Dim), (line, Ink::Muted)]))
        .collect()
}

/// The rows an answer is drawn as: its markdown, laid out by [`markdown`] and set in the
/// gutter an answer is printed from — its own margin held open and blank, because an
/// answer is the thing being read and needs no rule beside it.
///
/// What the constructs look like is `markdown`'s, because the layout is the product and
/// the ticket that wrote it (#52) owns all of it: a heading's weight, a table's rule, a
/// code block's gutter and its highlighting. The one rule this file kept before the
/// parse was here is ADR-0008's, and it moved with the parse: a fence the wire has not
/// closed yet ends the text with the block still open, and `markdown` draws it as the
/// code block it is rather than as prose that ate the rest of the answer.
pub(crate) fn answer(text: &str, cols: u16) -> Vec<Row> {
    let (gutter, room) = margin(INDENT, cols);
    markdown::rows(text, room)
        .into_iter()
        .map(|row| row.indented(gutter, Ink::Plain))
        .collect()
}

/// The gutter a body opens with at `cols`, and the columns its words have beside it.
///
/// The gutter is given up rather than the invariant: a band too narrow to hold the
/// gutter and one glyph of prose is a band whose rows carry the prose alone, because a
/// row laid out wider than the width it was laid out for is a row the paint would clip
/// and whatever copies it would lose.
fn margin(gutter: &str, cols: u16) -> (&str, u16) {
    let width = u16::try_from(glyphs(gutter)).unwrap_or(cols);
    // Two columns, because that is the widest one glyph is: a margin that left less
    // than that would cost a row the whole width it was laid out for.
    match cols > width.saturating_add(1) {
        true => (gutter, cols - width),
        false => ("", cols),
    }
}

/// The row a call opens: the name in the faded ink a tool row is painted in, and the
/// arguments one step in under it — so a call that has not settled yet names itself and
/// claims nothing about how it will end.
fn opening(name: &str, arguments: &str, cols: u16) -> Vec<Row> {
    let mut rows = vec![fitted(vec![(format!("{PENDING} {name}"), Ink::Dim)], cols)];
    rows.extend(body(arguments, cols));
    rows
}

/// The row a call settles into: what happened to it — the check or the cross in the ink
/// a finished thing is read in, and its words faded beside it — the call's name, and the
/// two numbers the run paid for it. §12's row says the exit code and not the command:
/// what a finished call has to say is how it ended, and a reader who wants the command
/// has the row above it, where the model asked for it.
///
/// A result with no verdict is a line a build wrote before the verdict was a fact
/// (ADR-0007): the row says the name and the numbers and claims nothing it does not
/// know, rather than guessing `✓ exit 0` from the text beside it.
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

/// The glyph a verdict settles on: a check for the one that finished well, a cross for
/// every other way a call can end.
fn mark(exit: Exit) -> char {
    if exit.is_ok() { CHECK } else { CROSS }
}

/// The ink a verdict's glyph is drawn in. Two inks and four states: what tells `✗ exit N`
/// from `✗ timed out` is the words, which are the record's own, and not a shade of red.
fn ink_of(exit: Exit) -> Ink {
    if exit.is_ok() { Ink::Ok } else { Ink::Err }
}

/// One row of `spans`, cut at `cols` columns: a row is laid out at a width and never runs
/// over it, so the run of glyphs the width ends in is cut where the width ends.
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

/// `text` clipped to `room` columns: what a row keeps when what it says is one run of
/// glyphs with no space in it to break, which a name and its numbers are.
fn clip(text: &str, room: u16) -> String {
    if glyphs(text) <= usize::from(room) {
        return String::from(text);
    }
    split_at(text, usize::from(room.max(1))).0
}

/// A body of text as the rows it is drawn as: the entry's own words, wrapped at what the
/// step in from the left edge leaves of `cols`, and set one step in from the name that
/// introduced them.
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

/// The divider a turn opens with: the turn, clipped to the band when the band is
/// narrower than the word, and the hairline filling what is left of the width.
fn rule(lead: &str, cols: u16) -> Row {
    let lead = clip(lead, cols);
    let filled = usize::from(cols).saturating_sub(glyphs(&lead));
    let line = format!("{lead}{}", HAIRLINE.to_string().repeat(filled));
    Row::of([(line, Ink::Dim)])
}

/// `text` broken into rows no wider than `cols`: words kept whole where they fit, a
/// word longer than a row broken across rows, and every line break the text has kept,
/// because the paragraphs of an answer are the writer's and not the layout's.
fn wrap(text: &str, cols: u16) -> Vec<String> {
    let room = usize::from(cols.max(1));
    let mut rows: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let mut row = String::new();
        let mut width = 0;
        for word in line.split_whitespace() {
            let mut word = word;
            loop {
                let gap = usize::from(!row.is_empty());
                if width + gap + glyphs(word) <= room {
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
                // The word is alone on its row and still does not fit: write the
                // longest head of it that does, and carry the rest to the next row.
                let (head, rest) = split_at(word, room);
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
/// first glyph wider than the room is taken anyway: a row that carried nothing would
/// break the word nowhere and forever.
///
/// `markdown` breaks a word with this too, so a row is measured by one measure and a
/// word this file calls wide is wide to both.
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

/// The columns a run of glyphs takes: measured and never counted, because a glyph can
/// be two cells wide and a row is a count of cells.
pub(crate) fn glyphs(text: &str) -> usize {
    Span::raw(text).width()
}

/// Where the transcript's window sits: on the newest row, or holding still some rows
/// above it while the run goes on below (§12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scroll {
    /// Following the tail, so the newest row is on the band's last row and a fact that
    /// lands moves the picture.
    #[default]
    Tail,
    /// Held `rows` rows above the tail, which is where a reader who scrolled back is
    /// standing and what `↓ 4 new rows` counts its arrivals from.
    Back(u32),
}

impl Scroll {
    /// How many rows above the tail this position holds: what a window is measured in,
    /// and the whole of what the two cases differ by.
    fn back(self) -> u32 {
        match self {
            Scroll::Tail => 0,
            Scroll::Back(rows) => rows,
        }
    }
}

/// Every entry a transcript holds, each with the rows it was laid out as, and the width
/// those rows were laid out at.
///
/// A cache rather than a renderer: it is told the entries and the width, and it answers
/// with rows. What paints them is `view`'s transcript band, so a second front end can
/// spend the same layout and only the ink would differ.
#[derive(Clone, Debug, Default)]
pub struct Rows {
    /// The width every laid-out row here was laid out for, and nothing until a frame
    /// names one.
    cols: u16,
    /// The entries, in order, each with its rows once they have been laid out.
    held: Vec<Held>,
    /// How many times an entry has been laid out here, a re-layout after a width change
    /// included: ADR-0008's three rules are statements about this number, so it is a
    /// count a test can read rather than one a comment makes.
    lays: usize,
}

/// One entry and the rows it was laid out as, or nothing when this cache has never laid
/// it out at the width it holds.
#[derive(Clone, Debug)]
struct Held {
    /// What this entry is, which is the whole of the difference between a fact and the
    /// turn the run is still writing.
    what: What,
    /// What came back, when what this holds is a call whose result has landed: the fact that
    /// settles the row the call opened. Kept rather than layered over the rows, so a
    /// width change re-lays the settled row and not the one the call was drawn as while
    /// it ran. A live entry has no result: nothing has settled a turn still in flight.
    result: Option<Entry>,
    /// The rows it is drawn as, once they exist at this width.
    rows: Option<Vec<Row>>,
}

/// What one entry of the cache is: a fact the run wrote down, or the live entry of the
/// turn in flight, which is no fact yet (ADR-0007) and which this cache can only lay out
/// from the stream it is handed.
#[derive(Clone, Debug)]
enum What {
    /// The fact, as the run wrote it: kept here so an entry and its rows cannot be told
    /// apart by a caller handing in one where the other was expected.
    Fact(Entry),
    /// The turn in flight, as the fragment count its rows were laid from: laid out from
    /// the stream by [`Rows::live`] and by nothing else, so the rows of a live entry are
    /// always the rows of the fragments that have landed.
    Live { revision: u64 },
}

impl Rows {
    /// A cache that holds no entry and has laid nothing out: the width arrives with the
    /// first frame, from the band the rows are drawn in.
    pub fn new() -> Rows {
        Rows::default()
    }

    /// Holds one more entry, unlaid: a fact the run has just written, appended where
    /// the run wrote it.
    ///
    /// A result does not open a row of its own — it settles the row the call it answers
    /// opened, because one call is one row (§12) and a finished call is not a second
    /// line under the one it started. A result whose call this cache never held, which
    /// is an older transcript's entry read on its own, has nothing to settle and opens
    /// a row like any other fact.
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

    /// Holds the turn in flight as the transcript's live entry: the rows the stream is
    /// drawn as now, laid again every time a fragment lands and by nobody else, so a
    /// tick that changes the live entry costs one layout of it and no layouts of the
    /// history above it (ADR-0008).
    ///
    /// A stream that has said nothing holds no live entry, and one that is asked again
    /// without having moved is the same picture and costs nothing: the revision is what
    /// tells the two apart, so a paint that changed nothing does not wrap the same text
    /// again. A width change is not a paint that changed nothing — it drops the rows
    /// like every other entry's — and the next frame lays them out again.
    pub fn live(&mut self, stream: &Stream) {
        if stream.is_empty() {
            self.retire();
            return;
        }
        let revision = stream.revision();
        if let Some(Held {
            what: What::Live { revision: at },
            rows: Some(_),
            ..
        }) = self.held.last()
            && *at == revision
        {
            return;
        }
        let rows = stream.rows(self.cols);
        match self.held.last_mut() {
            Some(held) if matches!(held.what, What::Live { .. }) => {
                held.what = What::Live { revision };
                held.rows = Some(rows);
            }
            _ => self.held.push(Held {
                what: What::Live { revision },
                result: None,
                rows: Some(rows),
            }),
        }
        self.lays += 1;
    }

    /// Drops the live entry, if this cache holds one: a fact landing is the end of the
    /// turn in flight, and the block that was drawn while it streamed is the fact's own
    /// rows now — so the turn is drawn from what it settled into and never twice.
    fn retire(&mut self) {
        if matches!(
            self.held.last().map(|held| &held.what),
            Some(What::Live { .. })
        ) {
            self.held.pop();
        }
    }

    /// The entry a result for `turn` settles: the oldest call of that turn whose own
    /// result has not landed. Oldest and not newest, because the run writes a call and
    /// then its result or a wave of calls and then their results, and the two orders
    /// have to pair up the same way.
    fn pending(&self, turn: u32) -> Option<usize> {
        self.held.iter().position(|held| {
            held.result.is_none()
                && matches!(&held.what, What::Fact(Entry::Call { turn: at, .. }) if *at == turn)
        })
    }

    /// Settles the call at `at` with the result that answered it: the row the call
    /// opened says what happened to it now, and this entry alone is laid out again —
    /// everything above and below it keeps what it was drawn as (ADR-0008). `at` is an
    /// index this cache handed out, so a cache that does not hold it has been read wrong.
    pub fn settle(&mut self, at: usize, result: Entry) {
        let held = &mut self.held[at];
        held.result = Some(result);
        held.rows = None;
    }

    /// How many rows this cache holds: the facts it was given and, while a turn is in
    /// flight, the live entry of that turn as one more — less the results, which settle
    /// the row the call they answer opened instead of opening one of their own.
    pub fn len(&self) -> usize {
        self.held.len()
    }

    /// Whether this cache holds no entry at all.
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// The width every row here is laid out at.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// How many entries this cache has laid out, counting a re-layout after a width
    /// change: the instrument a test reads instead of the shape of this code.
    pub fn lays(&self) -> usize {
        self.lays
    }

    /// Tells the cache the width its rows are drawn at. The same width is nothing at
    /// all, so a paint at an unchanged size re-lays nothing; a different one drops every
    /// row the cache held, which is the one-width cache ADR-0008 decided on — going back
    /// to a width is a reflow and not a lookup.
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

    /// The rows of entry `at`, laid out now if this cache has never laid them out at the
    /// width it holds and kept for the next paint. `at` is an index this cache handed
    /// out — the one a [`Piece`] carries — so a cache that does not hold it has been
    /// read wrong. The live entry is the one entry with no fact behind it: its rows are
    /// the stream's and [`Rows::live`] is what lays them, so a cache that has not been
    /// handed a stream draws nothing for it rather than inventing a row.
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
            let rows = layout(fact, cols);
            self.lays += 1;
            self.held[at].rows = Some(rows);
        }
        self.held[at].rows.as_deref().unwrap_or_default()
    }

    /// The transcript's whole height in rows, laying out every entry this cache has
    /// never seen: the one door to measuring a history, and the trade-off the module's
    /// header records. A window never needs it — it is here for what counts rows the
    /// picture does not draw, which is #50's marking and the arrivals below a reader who
    /// scrolled back.
    pub fn height(&mut self) -> u32 {
        let mut height: u32 = 0;
        for at in 0..self.held.len() {
            height = height.saturating_add(u32::try_from(self.rows(at).len()).unwrap_or(u32::MAX));
        }
        height
    }

    /// The entries a band of `height` rows at `scroll` intersects, in order, each with
    /// the rows of it that are inside the band: what the transcript paints, and nothing
    /// the band never reaches.
    ///
    /// The band is the rows `back` to `back + height` above the tail, so an entry is in
    /// it when that range overlaps the rows the entry fills. Entries are measured from
    /// the tail backwards and the walk stops as soon as the band is filled, which is why
    /// a long history costs the entries a reader can see and no others.
    pub fn window(&mut self, scroll: Scroll, height: u16) -> Window {
        let back = scroll.back();
        let top = back.saturating_add(u32::from(height));
        // The rows of the transcript below the entry being measured.
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

/// What a band of rows shows of the transcript: the entries it intersects, in order,
/// and how much of each is inside it.
#[derive(Clone, Debug, Default)]
pub struct Window {
    /// The entries the band reaches, top to bottom and contiguous, so a painter walks
    /// them in order and never has to work out where one starts.
    pieces: Vec<Piece>,
}

impl Window {
    /// The entries this window shows, in order.
    pub fn pieces(&self) -> &[Piece] {
        &self.pieces
    }
}

/// One entry of a window: which entry, how many rows of its top are above the band's
/// first row, and how many rows of it the band draws from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    /// The entry's index in the cache, which is the number [`Rows::rows`] is asked for.
    pub entry: usize,
    /// How many of the entry's rows are cut off above the band's first row: what makes
    /// a scroll cut an entry in half rather than count it whole.
    pub skip: u16,
    /// How many rows of the entry, from `skip` on, the band draws.
    pub rows: u16,
}
