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
//! `Call` and `Result` rows are deliberately plain here. What a tool row says — `✓ exit
//! 0`, `✗ exit N`, `✗ timed out`, `✗ cancelled` — and the faded ink §12 paints tool
//! calls in are ticket #40's contract: it changes these two shapes and their golden
//! frame in the same commit, and nothing else has to move with it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;
use sindri_core::records::Entry;

use crate::ink::Ink;

/// How far a body sits inside the row that names it: one step, the step a past run is
/// read back with too, so a call and what it answered read as one thing said in two
/// rows rather than as two entries that happened to follow each other.
const INDENT: &str = "  ";

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
    /// The runs of glyphs, left to right, each written in the ink it was given.
    spans: Vec<(String, Ink)>,
}

impl Row {
    /// A row of `spans`, each run of glyphs drawn in its own ink.
    pub fn of<I>(spans: I) -> Row
    where
        I: IntoIterator<Item = (String, Ink)>,
    {
        Row {
            spans: spans.into_iter().collect(),
        }
    }

    /// The columns this row takes when it is drawn.
    pub fn width(&self) -> u16 {
        let columns: usize = self.spans.iter().map(|(text, _)| glyphs(text)).sum();
        u16::try_from(columns).unwrap_or(u16::MAX)
    }

    /// Draws this row across the top row of `area`, from its left edge and clipped at
    /// its right one: a row is laid out at a width and drawn at that width or narrower,
    /// so the band's right edge is where a row stops and never where it runs over.
    pub fn paint(&self, area: Rect, buffer: &mut Buffer) {
        let mut x = area.x;
        for (text, ink) in &self.spans {
            if x >= area.right() {
                return;
            }
            let room = usize::from(area.right() - x);
            let (next, _) = buffer.set_stringn(x, area.y, text, room, ink.style());
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
        } => {
            let mut rows = vec![naming("call ", name, "", cols)];
            rows.extend(body(&arguments.to_string(), cols));
            rows
        }
        Entry::Result {
            name,
            text,
            bytes,
            elapsed_ms,
            ..
        } => {
            let mut rows = vec![naming(
                "result ",
                name,
                &format!(" {bytes} bytes · {elapsed_ms} ms"),
                cols,
            )];
            rows.extend(body(&text.to_string(), cols));
            rows
        }
        Entry::Assistant { text, .. } => wrap(&text.to_string(), cols)
            .into_iter()
            .map(|line| Row::of([(line, Ink::Plain)]))
            .collect(),
    }
}

/// The row that names a tool call: what it was, said in the ink secondary prose is read
/// in, and the facts §12 keeps beside it, clipped at the band's width when a name and
/// its numbers are wider than the row they are the whole of. Deliberately plain: what a
/// tool row says, and the faded ink §12 paints it in, are #40's to decide and this is
/// only the shape it will be decided inside.
fn naming(lead: &str, name: &str, facts: &str, cols: u16) -> Row {
    Row::of([(clip(&format!("{lead}{name}{facts}"), cols), Ink::Muted)])
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
    let step = u16::try_from(glyphs(INDENT)).unwrap_or(cols);
    let room = cols.saturating_sub(step).max(1);
    wrap(text, room)
        .into_iter()
        .map(|line| Row::of([(clip(&format!("{INDENT}{line}"), cols), Ink::Plain)]))
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
fn split_at(word: &str, room: usize) -> (String, &str) {
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
fn glyphs(text: &str) -> usize {
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
    /// The fact, as the run wrote it: kept here so an entry and its rows cannot be told
    /// apart by a caller handing in one where the other was expected.
    entry: Entry,
    /// The rows it is drawn as, once they exist at this width.
    rows: Option<Vec<Row>>,
}

impl Rows {
    /// A cache that holds no entry and has laid nothing out: the width arrives with the
    /// first frame, from the band the rows are drawn in.
    pub fn new() -> Rows {
        Rows::default()
    }

    /// Holds one more entry, unlaid: a fact the run has just written, appended where
    /// the run wrote it.
    pub fn push(&mut self, entry: Entry) {
        self.held.push(Held { entry, rows: None });
    }

    /// How many entries this cache holds.
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
    /// read wrong.
    pub fn rows(&mut self, at: usize) -> &[Row] {
        let cols = self.cols;
        let held = &mut self.held[at];
        if held.rows.is_none() {
            let rows = layout(&held.entry, cols);
            held.rows = Some(rows);
            self.lays += 1;
        }
        held.rows.as_deref().unwrap_or_default()
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
