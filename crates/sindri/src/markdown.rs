//! Markdown becomes the rows an entry is drawn as: `pulldown-cmark` finds the constructs
//! and this module decides what each of them looks like, and a fenced block the text ends
//! inside is still drawn as a block, because the wire streams one and never doubles back.

use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

use crate::ink::{Face, Ink, Look};
use crate::mermaid::{self, Fence};
use crate::palette::Band;
use crate::picture::{self, Picture, Surface};
use crate::rows::{Row, glyphs, split_at};

/// The parser's options: GFM tables on, because CommonMark leaves tables out. No other extension
/// is enabled, so a footnote, a strikethrough and a task list parse as plain prose.
const OPTIONS: Options = Options::ENABLE_TABLES;

pub fn rows(text: &str, cols: u16) -> Vec<Row> {
    rows_under(text, cols, 0)
}

/// The same markdown with `reserve` columns given up on the first row alone, the one a
/// marker stands beside: every row after it is laid out at the full width, which is what
/// makes a live answer and a settled one the same picture with one glyph moved.
pub fn rows_under(text: &str, cols: u16, reserve: u16) -> Vec<Row> {
    rows_on(text, cols, reserve, &Surface::none())
}

/// The same markdown on a `surface`: a picture is a block, so how many rows one takes is
/// the layout's business and only the surface can answer it.
pub fn rows_on(text: &str, cols: u16, reserve: u16, surface: &Surface) -> Vec<Row> {
    let mut room = Room::new(cols.max(1), surface);
    room.reserve = usize::from(reserve);
    // Each event is handed the span it came from, which is how a mermaid fence knows
    // whether its own closing line has arrived or the wire is still writing it.
    for (event, span) in Parser::new_ext(text, OPTIONS).into_offset_iter() {
        room.event(event, &text[span]);
    }
    room.finish()
}

fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// The ink a kind of word is drawn in, read off the scope `syntect` puts it in: a `syntect` theme
/// colours a cell by RGB, where a cell of this frame is an ink a fixture writes down by name.
fn kinds() -> &'static [(Scope, Ink)] {
    static KINDS: OnceLock<Vec<(Scope, Ink)>> = OnceLock::new();
    KINDS.get_or_init(|| {
        [
            ("comment", Ink::Dim),
            ("string", Ink::Muted),
            ("keyword.operator", Ink::Plain),
            ("keyword", Ink::Accent),
            ("storage", Ink::Accent),
            ("constant", Ink::Muted),
        ]
        .into_iter()
        .filter_map(|(name, ink)| Scope::new(name).ok().map(|scope| (scope, ink)))
        .collect()
    })
}

fn heading(level: HeadingLevel) -> Look {
    match level {
        HeadingLevel::H1 | HeadingLevel::H2 => Look::new(Ink::Plain, Face::Bold),
        HeadingLevel::H3 => Look::new(Ink::Dim, Face::Bold),
        HeadingLevel::H4 | HeadingLevel::H5 | HeadingLevel::H6 => Look::plain(Ink::Plain),
    }
}

/// The room a block is being laid out in, and the rows it has been laid out as so far.
/// The gutters a construct adds are subtracted from the room before a word is placed and
/// never after, so no row this writes runs over the width and gets clipped by the paint.
struct Room {
    wide: u16,
    rows: Vec<Row>,
    line: Vec<(String, Look)>,
    used: usize,
    spaced: bool,
    block: Look,
    lead: Vec<(String, Look)>,
    /// The markers of the list items open, outermost first: every row is indented by the ones
    /// outside it and by the width of the innermost, so each depth costs the columns a `•` takes.
    marks: Vec<String>,
    lists: Vec<Option<u64>>,
    first: bool,
    /// Whether the last row put down is a blank one, so two blocks never stand under two blanks.
    blank: bool,
    /// The code block in hand: the highlighter's parse carries from one line of it to the next,
    /// so it lives with the block and not with the room.
    code: Option<Code>,
    mermaid: Option<Mermaid>,
    /// The table in hand: read whole before a row of it is drawn, because a column is as wide
    /// as the widest cell the table holds.
    table: Option<Table>,
    /// The link being said: its destination and the words said over it, so the URL is printed
    /// only when it says something the words did not.
    link: Option<(String, String)>,
    /// The columns the first row gives up to a marker laid at the frame's left edge, and
    /// nothing once that row has been written.
    reserve: usize,
    surface: Surface,
    /// The picture being read: the source it points at and the words written over it, which
    /// are what its block is spoken of as. Nothing is drawn until it closes, because its words
    /// are inside it.
    image: Option<(String, String)>,
}

impl Room {
    fn new(wide: u16, surface: &Surface) -> Room {
        Room {
            wide,
            rows: Vec::new(),
            line: Vec::new(),
            used: 0,
            spaced: false,
            block: Look::plain(Ink::Plain),
            lead: Vec::new(),
            marks: Vec::new(),
            lists: Vec::new(),
            first: true,
            blank: true,
            link: None,
            code: None,
            mermaid: None,
            table: None,
            reserve: 0,
            surface: surface.clone(),
            image: None,
        }
    }

    fn event(&mut self, event: Event, span: &str) {
        match event {
            Event::Start(Tag::Paragraph) => self.gap(),
            Event::End(TagEnd::Paragraph) => self.flush_if_written(),
            Event::Start(Tag::Heading { level, .. }) => self.open_heading(level),
            Event::End(TagEnd::Heading(_)) => {
                self.flush_if_written();
                self.block = Look::plain(Ink::Plain);
            }
            Event::Start(Tag::BlockQuote(_)) => self.open_quote(),
            Event::End(TagEnd::BlockQuote(_)) => {
                self.flush_if_written();
                self.lead.pop();
            }
            Event::Start(Tag::List(start)) => {
                self.flush_if_written();
                self.lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                self.flush_if_written();
                self.lists.pop();
            }
            Event::Start(Tag::Item) => self.open_item(),
            Event::End(TagEnd::Item) => self.close_item(),
            Event::Start(Tag::Table(_)) => {
                self.gap();
                self.table = Some(Table::default());
            }
            Event::End(TagEnd::Table) => self.draw_table(),
            Event::Start(Tag::TableHead) => {
                if let Some(table) = &mut self.table {
                    table.heading = true;
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(table) = &mut self.table {
                    table.close_head();
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(table) = &mut self.table {
                    table.row.clear();
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(table) = &mut self.table {
                    table.close_row();
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(table) = &mut self.table {
                    table.cell.clear();
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(table) = &mut self.table {
                    table.close_cell();
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => self.open_code(kind, span),
            Event::End(TagEnd::CodeBlock) => self.close_code(),
            Event::Text(text) => match &mut self.code {
                Some(code) => {
                    if let Some(mermaid) = &mut self.mermaid {
                        mermaid.source.push_str(&text);
                    }
                    let lines = code.highlight(&text);
                    for runs in lines {
                        for (text, look) in runs {
                            self.verbatim(&text, look);
                        }
                        self.flush();
                    }
                }
                None => match &mut self.table {
                    // A cell is measured only once the whole table has been read.
                    Some(table) => table.text(&text, self.block),
                    // A picture's words are kept aside for the row it is spoken of on:
                    // they are the block's, not the paragraph's.
                    None => match &mut self.image {
                        Some((_, alt)) => alt.push_str(&text),
                        None => {
                            if let Some((_, said)) = &mut self.link {
                                said.push_str(&text);
                            }
                            self.prose(&text, self.block);
                        }
                    },
                },
            },
            // Inline code keeps the spacing it was written with. It is reversed over
            // `muted` rather than over no colour, because `mark` is reverse over no colour
            // and the two would otherwise be one look.
            Event::Code(text) => {
                let reversed = Look::new(Ink::Muted, Face::Reversed);
                match &mut self.table {
                    Some(table) => table.text(&text, reversed),
                    None => {
                        self.place(&text, reversed, self.spaced);
                        self.spaced = false;
                    }
                }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                self.image = Some((String::from(&*dest_url), String::new()));
            }
            Event::End(TagEnd::Image) => self.close_image(),
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.link = Some((String::from(&*dest_url), String::new()));
            }
            Event::End(TagEnd::Link) => self.close_link(),
            // A soft break is the line the source was wrapped on, not one the writer drew:
            // the paragraph is reflowed to the room.
            Event::SoftBreak => match &mut self.table {
                Some(table) => table.text(" ", self.block),
                None => match &mut self.image {
                    Some((_, alt)) => alt.push(' '),
                    None => self.spaced = true,
                },
            },
            Event::HardBreak => self.flush_if_written(),
            _ => {}
        }
    }

    fn open_heading(&mut self, level: HeadingLevel) {
        self.gap();
        if level == HeadingLevel::H1 && self.rows.is_empty() {
            self.push(Row::blank());
        }
        self.block = heading(level);
    }

    /// The words of a paragraph, heading or item, placed one at a time, so a word the row
    /// cannot hold is the next row's and never a clipped one. The spacing the source put between
    /// two runs is kept, so a full stop written against a link's word stays against it.
    fn prose(&mut self, text: &str, look: Look) {
        let mut rest = text;
        let mut space = self.spaced;
        while !rest.is_empty() {
            let trimmed = rest.trim_start_matches(char::is_whitespace);
            if trimmed.len() != rest.len() {
                space = true;
                rest = trimmed;
            }
            if rest.is_empty() {
                break;
            }
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            self.place(&rest[..end], look, space);
            rest = &rest[end..];
            space = false;
        }
        self.spaced = space;
    }

    /// One run of glyphs placed at the end of the row in hand, carried to the next row when it
    /// does not fit and broken only when it is alone on its row and still wider than the room:
    /// a run is kept whole where a whole run fits, because half a word is not the word written.
    fn place(&mut self, text: &str, look: Look, space: bool) {
        let mut rest = text;
        let mut space = space;
        loop {
            let room = self.room();
            let gap = usize::from(space && self.used > 0);
            if self.used + gap + glyphs(rest) <= room {
                if gap == 1 {
                    let at = self.block;
                    self.extend(" ", at);
                }
                self.extend(rest, look);
                return;
            }
            if self.used > 0 {
                self.flush();
                space = false;
                continue;
            }
            let (head, tail) = split_at(rest, room);
            self.extend(&head, look);
            self.flush();
            rest = tail;
            space = false;
            if rest.is_empty() {
                return;
            }
        }
    }

    fn extend(&mut self, text: &str, look: Look) {
        if text.is_empty() {
            return;
        }
        self.used = self.used.saturating_add(glyphs(text));
        match self.line.last_mut() {
            Some((last, at)) if *at == look => last.push_str(text),
            _ => self.line.push((String::from(text), look)),
        }
    }

    /// The table in hand as the rows it is drawn as, each cell wrapped to its column's width
    /// and its header ruled off from the header's own row, so a rule and the row above it cannot
    /// disagree by a column. A table past the room wraps inside its cells, never into a paragraph.
    fn draw_table(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        let columns = table.columns();
        if columns == 0 {
            return;
        }
        let available = self.room();
        let naturals: Vec<usize> = (0..columns).map(|at| table.width(at)).collect();
        let Some(widths) = fit(&naturals, available.saturating_sub(3 * (columns - 1))) else {
            self.draw_cells(&table, available);
            return;
        };

        let head = table.lines(&widths);
        for line in &head {
            let row = joined(line, &widths);
            self.push(row);
        }
        self.push(rule(&widths));
        for cells in table.body() {
            for line in lines_of(cells, &widths) {
                let row = joined(&line, &widths);
                self.push(row);
            }
        }
    }

    fn draw_cells(&mut self, table: &Table, available: usize) {
        for (at, cells) in std::iter::once(&table.head)
            .chain(table.rows.iter())
            .enumerate()
        {
            if at == 1 {
                self.push(Row::of([("─".repeat(available), Look::plain(Ink::Dim))]));
            }
            for cell in cells {
                for line in lines_of(std::slice::from_ref(cell), &[available]) {
                    let row = joined(&line, &[available]);
                    self.push(row);
                }
            }
        }
    }

    /// A code block opens with a `▏` bar and its language's tag, and a parse that carries from
    /// line to line. A language no syntax definition names is drawn plain rather than refused:
    /// an unknown language is the wire's business and never the run's error.
    fn open_code(&mut self, kind: CodeBlockKind, span: &str) {
        self.gap();
        self.lead.push((String::from("▏ "), Look::plain(Ink::Dim)));
        let tag = match kind {
            CodeBlockKind::Fenced(tag) => tag.trim().to_owned(),
            CodeBlockKind::Indented => String::new(),
        };
        self.code = Some(Code::of(&tag));
        if !tag.is_empty() {
            self.place(&tag, Look::plain(Ink::Dim), false);
            self.flush();
        }
        // A mermaid fence is a picture and not code: its source is kept so the drawing can be
        // made when the block closes, and so the reason it could not be drawn prints under it.
        if tag.split_whitespace().next() == Some("mermaid") {
            let fence = match closed(span) {
                true => Fence::Closed,
                false => Fence::Streaming,
            };
            let at = match tag.is_empty() {
                true => self.rows.len(),
                false => self.rows.len().saturating_sub(1),
            };
            self.mermaid = Some(Mermaid {
                source: String::new(),
                fence,
                at,
            });
        }
    }

    /// A code block closes. A mermaid fence says what became of it — a picture placed over the
    /// source, nothing while it is still arriving, one sentence under the source when it did not
    /// draw — because a diagram that is simply absent cannot be told from a build that cannot draw.
    fn close_code(&mut self) {
        self.flush_if_written();
        if let Some(mermaid) = self.mermaid.take() {
            match mermaid::block(&mermaid.source, mermaid.fence) {
                mermaid::Block::Refused { caption } => self.caption(&caption),
                mermaid::Block::Picture(diagram) if self.surface.draws() => {
                    let room = u16::try_from(self.room()).unwrap_or(u16::MAX);
                    let measured =
                        picture::drawing(diagram.source(), diagram.svg(), room, &self.surface);
                    match measured {
                        Ok(measured) => self.pictured(mermaid.at, &measured),
                        Err(refusal) => self.caption(&refusal.message()),
                    }
                }
                // A fence still arriving, and a picture this terminal cannot place, are drawn as
                // the source alone.
                _ => {}
            }
        }
        self.lead.pop();
        self.code = None;
    }

    /// Draws `picture` over the block that begins at `at`, growing the block to the picture's
    /// height: the bar of the quote it is in runs down the rows added, so the source a reader
    /// copies and the cells the picture is placed over are one height.
    fn pictured(&mut self, at: usize, picture: &Picture) {
        let height = at.saturating_add(usize::from(picture.rows()));
        while self.rows.len() < height {
            self.rows.push(Row::of(self.opening()));
        }
        if let Some(row) = self.rows.get_mut(at) {
            *row = row.clone().pictured_over(picture.clone());
        }
    }

    /// One sentence under the block in hand: what a picture that is not there is said with.
    fn caption(&mut self, reason: &str) {
        self.prose(reason, Look::plain(Ink::Dim));
        self.flush_if_written();
    }

    fn verbatim(&mut self, text: &str, look: Look) {
        let mut rest = text;
        while !rest.is_empty() {
            let room = self.room().saturating_sub(self.used).max(1);
            let (head, tail) = split_at(rest, room);
            self.extend(&head, look);
            if tail.is_empty() {
                return;
            }
            self.flush();
            rest = tail;
        }
    }

    fn open_quote(&mut self) {
        self.gap();
        self.lead.push((String::from("▏ "), Look::plain(Ink::Dim)));
    }

    fn open_item(&mut self) {
        self.flush_if_written();
        let marker = match self.lists.last_mut() {
            Some(Some(next)) => {
                let marker = format!("{next}. ");
                *next = next.saturating_add(1);
                marker
            }
            _ => String::from("• "),
        };
        self.marks.push(marker);
        self.first = true;
    }

    fn close_item(&mut self) {
        self.flush_if_written();
        self.marks.pop();
        self.first = false;
    }

    /// A destination that says something else follows the link's words; a link written over
    /// its own URL says it once.
    fn close_link(&mut self) {
        let Some((dest, said)) = self.link.take() else {
            return;
        };
        if said.trim() != dest {
            self.place(&dest, Look::plain(Ink::Dim), true);
        }
    }

    /// A picture closes: the block it is, spoken of as its words on the first of its rows and
    /// measured into the cells it takes under them. The words are written even where the terminal
    /// places the picture over them, because they are what a copy of the block takes.
    fn close_image(&mut self) {
        let Some((source, alt)) = self.image.take() else {
            return;
        };
        self.flush_if_written();
        self.gap();
        let words = picture::words(&source, &alt);
        let look = Look::plain(Ink::Dim);
        let room = u16::try_from(self.room()).unwrap_or(u16::MAX);
        match picture::measure(&source, room, &self.surface) {
            Err(refusal) => {
                self.prose(&words, look);
                self.flush_if_written();
                self.caption(&refusal.message());
            }
            Ok(_) if !self.surface.draws() => {
                self.prose(&words, look);
                self.flush_if_written();
            }
            Ok(measured) => {
                let at = self.rows.len();
                self.prose(&words, look);
                self.flush_if_written();
                self.pictured(at, &measured);
            }
        }
        // The last row of a picture is a cell of the picture, so it does not stand in for
        // the gap after it.
        self.blank = false;
        self.gap();
    }

    /// One blank row between two blocks, nothing at the top of the answer and never two blank
    /// rows where a block already stands under one.
    fn gap(&mut self) {
        if !self.rows.is_empty() && !self.blank {
            self.push(Row::blank());
        }
    }

    fn flush(&mut self) {
        let opening = self.opening();
        self.line.clear();
        let row = Row::of(opening);
        // A fence is one block of cells, so the tag and every line under it paint the well.
        self.push(match self.code.is_some() {
            true => row.banded(Band::Well),
            false => row,
        });
        self.used = 0;
        self.spaced = false;
        self.first = false;
    }

    fn opening(&self) -> Vec<(String, Look)> {
        let mut spans = self.lead.clone();
        for (at, mark) in self.marks.iter().enumerate() {
            let innermost = at + 1 == self.marks.len();
            let text = match innermost && self.first {
                true => mark.clone(),
                false => " ".repeat(glyphs(mark)),
            };
            spans.push((text, Look::plain(Ink::Plain)));
        }
        spans.extend(self.line.iter().cloned());
        spans
    }

    fn flush_if_written(&mut self) {
        if !self.line.is_empty() {
            self.flush();
        }
    }

    fn push(&mut self, row: Row) {
        self.blank = row.spans().is_empty();
        self.rows.push(row);
    }

    /// The columns a word of the block in hand has: at least one, because a row that could hold
    /// nothing would break every word into nothing forever.
    fn room(&self) -> usize {
        let bars: usize = self.lead.iter().map(|(text, _)| glyphs(text)).sum();
        let marks: usize = self.marks.iter().map(|mark| glyphs(mark)).sum();
        // The reserve holds only while no row has been written: a marker stands beside the
        // first row alone.
        let reserve = match self.rows.is_empty() {
            true => self.reserve,
            false => 0,
        };
        usize::from(self.wide)
            .saturating_sub(bars)
            .saturating_sub(marks)
            .saturating_sub(reserve)
            .max(1)
    }

    /// One row of nothing when the text wrote none: an entry with nothing to say is still a row
    /// of the transcript.
    fn finish(mut self) -> Vec<Row> {
        self.flush_if_written();
        if self.rows.is_empty() {
            self.push(Row::blank());
        }
        self.rows
    }
}

struct Mermaid {
    source: String,
    /// Whether the closing line has arrived. A fence still arriving is never rendered, so a
    /// half-written diagram costs no pass and never flickers.
    fence: Fence,
    /// The row the block opened on, which is the row its picture is placed over. A fence that
    /// carried a language opened it one row up, so the two cases count from different rows.
    at: usize,
}

/// Whether the closing line of a fenced block has arrived, read off the span the parser
/// handed over: the parse closes the block at the end of whatever text it was given, so a
/// fence still streaming and one that closed arrive as the same two events.
fn closed(span: &str) -> bool {
    let Some(open) = span.lines().next() else {
        return false;
    };
    let Some(close) = span.lines().next_back() else {
        return false;
    };
    let Some((marker, width, _)) = fence_line(open) else {
        return false;
    };
    match fence_line(close) {
        Some((last, run, tail)) => last == marker && run >= width && tail.trim().is_empty(),
        None => false,
    }
}

/// `None` for a line that is not a fence: indented more than the three columns a fence may
/// be, or carrying a run shorter than three.
fn fence_line(line: &str) -> Option<(char, usize, &str)> {
    let line = line.trim_end();
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let body = &line[indent..];
    let marker = body
        .chars()
        .next()
        .filter(|glyph| *glyph == '`' || *glyph == '~')?;
    let width = body.chars().take_while(|glyph| *glyph == marker).count();
    match width {
        0..=2 => None,
        _ => Some((marker, width, &body[width..])),
    }
}

/// One code block in hand, and the parse `syntect` carries across its lines: a string that
/// opens on one line and closes on the next is one string only because the parse outlives it.
struct Code {
    /// Nothing when the language is one no syntax definition names: the block is drawn plain.
    parse: Option<(ParseState, ScopeStack)>,
}

impl Code {
    fn of(tag: &str) -> Code {
        let parse = match tag.is_empty() {
            true => None,
            false => syntaxes()
                .find_syntax_by_token(tag)
                .map(|syntax| (ParseState::new(syntax), ScopeStack::new())),
        };
        Code { parse }
    }

    fn highlight(&mut self, text: &str) -> Vec<Vec<(String, Look)>> {
        let mut lines = Vec::new();
        let mut rest = text.split('\n').peekable();
        while let Some(line) = rest.next() {
            // The newline that ended the last line is not a line of its own: a block whose
            // text ends in one has no blank row after it.
            if rest.peek().is_none() && line.is_empty() {
                break;
            }
            lines.push(self.line(line));
        }
        lines
    }

    /// One line of code as the runs it is drawn as. A parse that fails is a line drawn plain
    /// rather than a run that stops.
    fn line(&mut self, line: &str) -> Vec<(String, Look)> {
        let Some((parse, scopes)) = &mut self.parse else {
            return vec![(String::from(line), Look::plain(Ink::Plain))];
        };
        let Ok(ops) = parse.parse_line(line, syntaxes()) else {
            return vec![(String::from(line), Look::plain(Ink::Plain))];
        };
        let mut runs: Vec<(String, Look)> = Vec::new();
        let mut at = 0;
        for (offset, op) in ops {
            let offset = offset.min(line.len());
            if offset > at && line.is_char_boundary(offset) {
                runs.push((String::from(&line[at..offset]), Look::plain(ink_of(scopes))));
                at = offset;
            }
            // A scope operation this build cannot apply leaves the stack as it was, so the
            // line keeps the ink of the scopes that did apply.
            let _ = scopes.apply(&op);
        }
        if at < line.len() {
            runs.push((String::from(&line[at..]), Look::plain(ink_of(scopes))));
        }
        runs
    }
}

/// The ink the innermost kind of scope stands for. The stack is read from its top down, so
/// the most particular kind of word wins over the block it sits in, and a scope no kind
/// names is the frame's own ink.
fn ink_of(scopes: &ScopeStack) -> Ink {
    for scope in scopes.as_slice().iter().rev() {
        if let Some((_, ink)) = kinds().iter().find(|(kind, _)| kind.is_prefix_of(*scope)) {
            return *ink;
        }
    }
    Ink::Plain
}

#[derive(Debug, Default)]
struct Table {
    head: Vec<Cell>,
    rows: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    cell: Vec<(String, Look)>,
    heading: bool,
}

type Cell = Vec<(String, Look)>;

impl Table {
    fn text(&mut self, text: &str, look: Look) {
        if text.is_empty() {
            return;
        }
        self.cell.push((String::from(text), look));
    }

    fn close_cell(&mut self) {
        self.row.push(std::mem::take(&mut self.cell));
    }

    fn close_head(&mut self) {
        self.head = std::mem::take(&mut self.row);
        self.heading = false;
    }

    fn close_row(&mut self) {
        if self.heading {
            self.close_head();
            return;
        }
        self.rows.push(std::mem::take(&mut self.row));
    }

    fn columns(&self) -> usize {
        std::iter::once(&self.head)
            .chain(self.rows.iter())
            .map(Vec::len)
            .max()
            .unwrap_or(0)
    }

    fn width(&self, at: usize) -> usize {
        std::iter::once(&self.head)
            .chain(self.rows.iter())
            .map(|cells| cells.get(at).map_or(0, |cell| measured(cell)))
            .max()
            .unwrap_or(0)
    }

    fn lines(&self, widths: &[usize]) -> Vec<Vec<Cell>> {
        lines_of(&self.head, widths)
    }

    fn body(&self) -> &[Vec<Cell>] {
        &self.rows
    }
}

/// The columns of a table laid out in `budget` columns: a column keeps its own width where
/// the table fits, and where it does not every column takes an equal share and what a narrow
/// one leaves goes to the next that can spend it. Nothing when not even one column each fits.
fn fit(naturals: &[usize], budget: usize) -> Option<Vec<usize>> {
    if budget < naturals.len() {
        return None;
    }
    let mut widths = vec![0; naturals.len()];
    let mut left = budget;
    let mut open = naturals.len();
    for (at, natural) in naturals.iter().enumerate() {
        let share = (left / open).max(1);
        widths[at] = (*natural).min(share);
        left -= widths[at];
        open -= 1;
    }
    while left > 0 {
        let mut spent = false;
        for at in 0..naturals.len() {
            if left == 0 {
                break;
            }
            if widths[at] < naturals[at] {
                widths[at] += 1;
                left -= 1;
                spent = true;
            }
        }
        if !spent {
            break;
        }
    }
    Some(widths)
}

fn lines_of(cells: &[Cell], widths: &[usize]) -> Vec<Vec<Cell>> {
    let wrapped: Vec<Vec<Cell>> = widths
        .iter()
        .enumerate()
        .map(|(at, width)| {
            // A picture's height is not a column's width, so this room has no surface for
            // one: a cell is measured for the words that fit it.
            let mut room = Room::new(u16::try_from(*width).unwrap_or(u16::MAX), &Surface::none());
            for (text, look) in cells.get(at).into_iter().flatten() {
                room.prose(text, *look);
            }
            room.flush_if_written();
            room.rows
                .into_iter()
                .map(|row| row.spans().to_vec())
                .collect::<Vec<Cell>>()
        })
        .collect();
    let tall = wrapped.iter().map(Vec::len).max().unwrap_or(0);
    (0..tall)
        .map(|line| {
            wrapped
                .iter()
                .map(|cell| cell.get(line).cloned().unwrap_or_default())
                .collect()
        })
        .collect()
}

fn joined(line: &[Cell], widths: &[usize]) -> Row {
    let mut spans: Vec<(String, Look)> = Vec::new();
    for (at, width) in widths.iter().enumerate() {
        if at > 0 {
            spans.extend(
                Row::of([(String::from(" │ "), Ink::Plain)])
                    .spans()
                    .to_vec(),
            );
        }
        let cell: &[(String, Look)] = line.get(at).map(Vec::as_slice).unwrap_or_default();
        spans.extend(cell.iter().cloned());
        // The last column keeps no blank past its words: padding it would draw cells that
        // say nothing, and a row is a run of glyphs and not a box.
        let last = at + 1 == widths.len();
        let pad = match last {
            true => 0,
            false => width.saturating_sub(measured(cell)),
        };
        if pad > 0 {
            spans.push((" ".repeat(pad), Look::plain(Ink::Plain)));
        }
    }
    Row::of(spans)
}

/// The rule under a header: the header's own row with every word spent as `─` and every bar
/// as `┼`, so the rule is exactly as wide as the row above it.
fn rule(widths: &[usize]) -> Row {
    let mut text = String::new();
    for (at, width) in widths.iter().enumerate() {
        if at > 0 {
            text.push_str("─┼─");
        }
        text.push_str(&"─".repeat(*width));
    }
    Row::of([(text, Ink::Dim)])
}

fn measured(cell: &[(String, Look)]) -> usize {
    cell.iter().map(|(text, _)| glyphs(text)).sum()
}
