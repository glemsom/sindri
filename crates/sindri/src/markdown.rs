//! Markdown becomes the rows a transcript entry is drawn as: `pulldown-cmark` finds the
//! constructs and this module decides what each of them looks like (§12, #52).
//!
//! The parse is a crate's because a hand-rolled one would be a second CommonMark, and
//! the rendering is ours because the layout is the product: which ink a heading is set
//! in, where a table's rule falls, how a code block's gutter holds its language. Every
//! construct §12's table names is a row or a run of them, and the table is the contract
//! this module keeps — headings, lists, quotes, tables, code with `syntect`'s
//! highlighting, inline code, and a link that prints its URL when the URL is not the
//! words it was written over.
//!
//! A row is written here and measured here: [`Room`] is the one place words are flowed
//! into rows no wider than the columns a band left them, and the gutters a construct
//! adds — a quote's bar, a code block's, a list item's hanging indent — are subtracted
//! from the room before a word is placed, never after. A row that ran over its width
//! would be clipped by the paint and lost from whatever copies it.
//!
//! One construct is not drawn here at all. A mermaid fence is a picture (§12): its source
//! goes to [`crate::mermaid`], which draws it or refuses with a reason, and this module draws
//! the source with that reason under it — the words the block copies as, and what a reader
//! sees when the picture cannot be placed over them.
//!
//! A picture an answer points at is a block too, and the one block whose height this module
//! cannot work out for itself: how many rows an `src` takes is its own size over the cell it
//! is drawn at, which only [`crate::picture`] can answer. So the surface a picture is measured
//! against — the workspace, the cell, whether this terminal places one — is handed to the
//! layout, and a source that resolves inside the workspace becomes a block of the cells it
//! takes with the words it is spoken of as at the head of them. Those words are what the block
//! copies as, and the picture is placed over them by [`crate::pictures`] and never here. A
//! source that does not resolve is one row saying why, because a picture refused with no
//! reason is a block that silently disappeared (§12, #55).
//!
//! Two rules outlive any construct. A block still open when the text ends — a fence the
//! wire is still streaming — is drawn as the block it opened, so a half-written code
//! block is code and never prose that ate the rest of the answer (ADR-0008). And a
//! table wider than the room wraps inside its cells rather than reflowing into a
//! paragraph: a reader of a narrow terminal loses columns, never content.

use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

use crate::ink::{Face, Ink, Look};
use crate::mermaid::{self, Fence};
use crate::picture::{self, Picture, Surface};
use crate::rows::{Row, glyphs, split_at};

/// The turns of the parser §12's table asks for, and no others: a table is the one
/// construct CommonMark leaves out, and nothing else here is drawn differently for
/// being a footnote, a strikethrough or a task list.
const OPTIONS: Options = Options::ENABLE_TABLES;

/// The markdown in `text` as the rows it is drawn as at `cols` columns: the whole of
/// what the transcript asks this module for.
pub fn rows(text: &str, cols: u16) -> Vec<Row> {
    rows_under(text, cols, 0)
}

/// The same markdown at `cols` columns with `reserve` of them given up on the first row
/// alone: the columns a marker standing at the frame's left edge takes out of the words
/// beside it (§39). Every row after the first is laid out at the full width, which is
/// what makes a live answer and a settled one the same picture with one glyph moved.
pub fn rows_under(text: &str, cols: u16, reserve: u16) -> Vec<Row> {
    rows_on(text, cols, reserve, &Surface::none())
}

/// The same markdown on a `surface`: the workspace a picture's source is resolved inside, the
/// cell it is measured against, and whether this terminal places one. A picture is a block, so
/// how many rows it takes is the layout's business and the surface is what that is asked of
/// (§12, #55).
pub fn rows_on(text: &str, cols: u16, reserve: u16, surface: &Surface) -> Vec<Row> {
    let mut room = Room::new(cols.max(1), surface);
    room.reserve = usize::from(reserve);
    // Each event is handed the source it came from: a fence's span is the block from its
    // opening line to its last, which is how a mermaid fence knows whether its closing line
    // has arrived or the wire is still writing it (§12, ADR-0008).
    for (event, span) in Parser::new_ext(text, OPTIONS).into_offset_iter() {
        room.event(event, &text[span]);
    }
    room.finish()
}

/// The syntax definitions a fence's language is looked up in, read once: a run that
/// streams a hundred code blocks parses them against one set of grammars.
fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// The inks a kind of word is drawn in, read off the scope `syntect` puts it in: a
/// comment is a fact to consult, a keyword is where the eye lands, and a string or a
/// number is quieter than the prose around it (§12).
///
/// A theme is deliberately not spent here. `syntect`'s own themes colour a cell by RGB,
/// and a cell of this frame is drawn in an ink and a face a fixture can write down by
/// name (ADR-0012) — so what a word *is* is read off its scope and this front end
/// decides which role that kind of word spends.
fn kinds() -> &'static [(Scope, Ink)] {
    static KINDS: OnceLock<Vec<(Scope, Ink)>> = OnceLock::new();
    KINDS.get_or_init(|| {
        [
            ("comment", Ink::Dim),
            ("string", Ink::Muted),
            // An operator is punctuation rather than a word, and the eye lands on the
            // words: `let x = 1` is read with the operator in the prose's own ink.
            ("keyword.operator", Ink::Plain),
            ("keyword", Ink::Accent),
            // `let` is a storage type to the grammars that name it, and it is as much a
            // word of the language as `fn` is.
            ("storage", Ink::Accent),
            ("constant", Ink::Muted),
        ]
        .into_iter()
        .filter_map(|(name, ink)| Scope::new(name).ok().map(|scope| (scope, ink)))
        .collect()
    })
}

/// The look a heading of this level is set in, cut from §12's table: an h1 and an h2 are
/// bold, an h3 is bold and dim, and h4 down to h6 say nothing but their words.
fn heading(level: HeadingLevel) -> Look {
    match level {
        HeadingLevel::H1 | HeadingLevel::H2 => Look::new(Ink::Plain, Face::Bold),
        HeadingLevel::H3 => Look::new(Ink::Dim, Face::Bold),
        HeadingLevel::H4 | HeadingLevel::H5 | HeadingLevel::H6 => Look::plain(Ink::Plain),
    }
}

/// The room a block is being laid out in, and the rows it has been laid out as so far.
///
/// One value writes a whole entry's markdown: a word is placed at the end of the row in
/// hand, and the row is put down when the next word would not fit. What a construct adds
/// — a bar down the left of a quote, the blanks an unwrapped list item's second row opens
/// with — is part of every row it produces, so a wrapped code line stays inside its
/// block's gutter.
struct Room {
    /// The columns a row has to spend, before the gutters in force take theirs.
    wide: u16,
    /// The rows finished so far, top to bottom.
    rows: Vec<Row>,
    /// The row being written: its runs of glyphs, left to right.
    line: Vec<(String, Look)>,
    /// The columns `line` takes.
    used: usize,
    /// Whether a word has been written to `line`, so the next one is set off by a space.
    spaced: bool,
    /// The look the words of the block in hand are drawn in.
    block: Look,
    /// The runs every row of the block in hand opens with, outside its hanging indent:
    /// the bar a quote or a code block is set behind.
    lead: Vec<(String, Look)>,
    /// The markers of the list items open, outermost first. Every row is indented by
    /// the markers outside it and by the width of the innermost, which is what keeps a
    /// wrapped item aligned under its own words and what makes a depth cost the two
    /// columns a `•` takes (§12).
    marks: Vec<String>,
    /// The lists open, outermost first, each holding the number its next item is drawn
    /// with — nothing for a list that is drawn with a bullet.
    lists: Vec<Option<u64>>,
    /// Whether `line` is the first row of the innermost list item, and so carries the
    /// marker the rows under it leave blank.
    first: bool,
    /// Whether the last row put down is a blank one, so two blocks never stand under two
    /// blank rows.
    blank: bool,
    /// The code block in hand, while one is open: what carries the highlighter's parse
    /// from one line of it to the next.
    code: Option<Code>,
    /// The mermaid fence in hand, while one is open: the source being read as a diagram
    /// rather than as code, and whether its closing line has arrived.
    mermaid: Option<Mermaid>,
    /// The table in hand, while one is open: read whole before a row of it is drawn,
    /// because a column is as wide as the widest cell the table holds.
    table: Option<Table>,
    /// The link being said, while its words are being written: the destination it points
    /// at and the words said over it, so the URL is printed only when it says something
    /// the words did not.
    link: Option<(String, String)>,
    /// The columns the first row gives up to a marker laid at the frame's left edge, and
    /// nothing once that row has been written: what keeps the mark's row from running over
    /// the width the rest of the answer was laid out at (#39).
    reserve: usize,
    /// The surface a picture is measured against: the workspace an `src` resolves inside, the
    /// cell it is drawn at, and whether this terminal places one (§12, #55).
    surface: Surface,
    /// The picture being read, while one is being written: the source it points at and the
    /// alternative words written over it, which are the words its block is spoken of as.
    /// Nothing is drawn until the picture closes, because its words are inside it.
    image: Option<(String, String)>,
}

impl Room {
    /// A room `wide` columns across that has written nothing, measuring its pictures against
    /// `surface`.
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

    /// One event of the parse, drawn: the construct it opens, closes or is made of, with the
    /// source it was read from when the event is a block and its own span says something.
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
                // Inside a block the text is code: its own lines, its own spacing, and
                // the ink the highlighter gives each kind of word.
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
                    // Inside a table the text is a cell's, and a cell is measured only
                    // once the whole table has been read.
                    Some(table) => table.text(&text, self.block),
                    // Inside a picture the text is its alternative words, kept aside for the
                    // row the picture is spoken of on rather than set into the prose: a
                    // picture's words are the block's, not the paragraph's (§12, #55).
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
            // Inline code keeps the spacing it was written with, because a run of code's
            // spacing is its meaning: the run is set reversed and broken at the row's
            // edge and nowhere else. It is reversed over `muted` and not over `plain`
            // because a mark is the terminal's reverse video and spends no colour, so the
            // two would otherwise be one look — see `Look::of`.
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
            // A soft break is the line the source was wrapped on and not one the writer
            // drew: the paragraph is reflowed to the room, so the two lines are one.
            Event::SoftBreak => match &mut self.table {
                Some(table) => table.text(" ", self.block),
                None => match &mut self.image {
                    // A line break inside a picture's words is the line they were written on
                    // and not one the reader drew: the words are flowed like prose.
                    Some((_, alt)) => alt.push(' '),
                    None => self.spaced = true,
                },
            },
            Event::HardBreak => self.flush_if_written(),
            _ => {}
        }
    }

    /// A heading opens: it stands under one blank row, an h1 under one of its own even
    /// when it opens the answer, and its words are set in the look its level asks for.
    fn open_heading(&mut self, level: HeadingLevel) {
        self.gap();
        if level == HeadingLevel::H1 && self.rows.is_empty() {
            self.push(Row::blank());
        }
        self.block = heading(level);
    }

    /// The words of a paragraph, heading or item: set into the room one at a time, so a
    /// word the row cannot hold is the next row's and never a clipped one. The spacing
    /// the source put between two words is the spacing they are drawn with, and its
    /// absence is kept too — a full stop written against a link's word stays against it.
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

    /// One run of glyphs placed at the end of the row in hand, set off from what came
    /// before it by a space when `space` says so: carried to the next row when it does
    /// not fit, and broken only when it is alone on its row and still wider than the
    /// room — a run is kept whole where a whole run fits, because half a word is not the
    /// word that was written.
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

    /// Glyphs placed at the end of the row in hand exactly as they were written: what a
    /// run of code is, where the writer's own spacing is the code's meaning.
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

    /// The table in hand as the rows it is drawn as: each cell wrapped to the width its
    /// column was given, the header ruled off, and no border around the whole — the
    /// rules are the header's own row with its words spent as `─` and its bars as `┼`,
    /// so a rule and the row above it cannot disagree by a column (§12).
    ///
    /// A table wider than the room wraps inside its cells and never reflows into a
    /// paragraph. A table so wide that not even one column each can be drawn is set a
    /// cell to a row: a narrow terminal loses columns, never content.
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

    /// A table too wide to hold one column each, as one cell a row: every word kept,
    /// wrapped to the room, with the header still ruled off from the body.
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

    /// A code block opens: a `▏` bar and its language's tag over the code, and a parse
    /// that carries from line to line (§12). A block whose language names no syntax
    /// definition is drawn plain and says nothing about it — an unknown language is the
    /// wire's business and never the run's error.
    fn open_code(&mut self, kind: CodeBlockKind, span: &str) {
        self.gap();
        self.lead.push((String::from("▏ "), Look::plain(Ink::Dim)));
        let tag = match kind {
            CodeBlockKind::Fenced(tag) => tag.trim().to_owned(),
            CodeBlockKind::Indented => String::new(),
        };
        if !tag.is_empty() {
            self.place(&tag, Look::plain(Ink::Dim), false);
            self.flush();
        }
        self.code = Some(Code::of(&tag));
        // A mermaid fence is a picture and not code (§12): what is written here is kept so
        // the drawing can be made when the block closes, and so the reason it could not be
        // drawn can be printed under the source. The rows are drawn the same either way,
        // because a diagram's block is what copies as its words, and the picture is placed
        // over these rows by `close_code` and by nothing else. The drawing is made once per
        // layout of the fence; what is kept between paints is the placement, in
        // `pictures::Pictures::held` under the source and the cells it took, so a repaint of
        // the same band transmits nothing and draws no second raster (ADR-0008).
        if tag.split_whitespace().next() == Some("mermaid") {
            let fence = match closed(span) {
                true => Fence::Closed,
                false => Fence::Streaming,
            };
            // The block began on the row the language was written on when the fence carried
            // one, and on the first row of the source when it did not.
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
    /// source it stands on, nothing at all while it is still arriving, and one sentence under
    /// its source when it did not draw — because a reader looking at a diagram that is not
    /// there cannot otherwise tell a fence they wrote badly from a build that cannot draw it
    /// (§12, #54).
    ///
    /// A terminal that places no picture is left the source, whole: the fence is still drawn
    /// and still refused in words, and only the placement is what a terminal without the
    /// protocol does without (§12, ADR-0003).
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
                        Ok(measured) => self.reserve(mermaid.at, &measured),
                        Err(refusal) => self.caption(&refusal.message()),
                    }
                }
                _ => {}
            }
        }
        self.lead.pop();
        self.code = None;
    }

    /// The block a picture was measured into, from the row it opened on: the rows the block
    /// already stands on, and as many more of its own gutter as the picture needs, so the
    /// source a reader copies and the cells the picture is placed over are one height.
    ///
    /// The picture is the first row's, because that is where the block begins: what is placed
    /// over a block starts at the top of it, and the words under the placement are what a copy
    /// of the block takes (§12).
    fn reserve(&mut self, at: usize, picture: &Picture) {
        let height = at.saturating_add(usize::from(picture.rows()));
        while self.rows.len() < height {
            self.rows.push(Row::of(self.opening()));
        }
        if let Some(row) = self.rows.get_mut(at) {
            *row = row.clone().pictured_over(picture.clone());
        }
    }

    /// One sentence under the block in hand, in the ink a fact beside the prose is read in:
    /// what a picture that is not there is said with, so a fence that vanished never reads as
    /// a fence the writer never wrote (§12).
    fn caption(&mut self, reason: &str) {
        self.prose(reason, Look::plain(Ink::Dim));
        self.flush_if_written();
    }

    /// Glyphs placed at the end of the row in hand exactly as they were written, broken
    /// at the room's edge and nowhere else: what a row of code is, where the writer's own
    /// spacing is the code's meaning.
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

    /// A quote opens: a `▏` bar in dim ink down the left of every row of the words
    /// quoted, so what a run was handed reads as something said beside the answer and
    /// never as part of it (§12). A quote inside a quote is a second bar.
    fn open_quote(&mut self) {
        self.gap();
        self.lead.push((String::from("▏ "), Look::plain(Ink::Dim)));
    }

    /// An item opens: its marker is the bullet §12 gives every unordered item, or the
    /// number this list has counted up to, and it is drawn on the item's first row only.
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

    /// An item closes: its words go down as the rows they wrapped into, and the next
    /// row of the item outside it leaves this item's marker blank rather than redrawing
    /// it, because a marker names one line and not every line under it.
    fn close_item(&mut self) {
        self.flush_if_written();
        self.marks.pop();
        self.first = false;
    }

    /// A link closes: its words stand where they were written, and a destination that
    /// says something else follows them in the ink a fact beside the prose is read in
    /// (§12). A link written over its own URL says it once.
    fn close_link(&mut self) {
        let Some((dest, said)) = self.link.take() else {
            return;
        };
        if said.trim() != dest {
            self.place(&dest, Look::plain(Ink::Dim), true);
        }
    }

    /// A picture closes: the block it is, spoken of as its words on the first of its rows and
    /// measured into the cells it takes under them (§12, #55).
    ///
    /// The words are written even where the terminal places the picture over them, because
    /// they are what the block copies as: the copy reads the layout and the terminal reads the
    /// placement, and neither has to ask the other what the picture was of.
    ///
    /// A source that cannot be drawn is a row saying what the picture was of and why it is not
    /// there — never a gap where a picture was, and never a row that runs over the band. A
    /// terminal that places no picture is given the words alone: the rows of a block it cannot
    /// fill would be a hole in the transcript, and a hole is not an honest way to say that a
    /// picture is not drawn.
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
                let block = self.blocked(&words, look, &measured);
                self.rows.extend(block);
            }
        }
        // A block stands under one blank row, the same one every other block stands under,
        // and it says so itself: the last row of a picture is a cell of the picture and does
        // not stand in for the gap after it.
        self.blank = false;
        self.gap();
    }

    /// The rows a picture takes: its words at the head of them, in the block's own gutter, and
    /// the cells it was measured into under that — empty, because the picture is what says the
    /// rest.
    ///
    /// The words are laid out by the same room every other word is, so an alt text longer than
    /// the band wraps onto rows of the block instead of running over its edge. A block is
    /// therefore the taller of the two: the rows the words needed, or the picture itself.
    fn blocked(&mut self, words: &str, look: Look, picture: &Picture) -> Vec<Row> {
        let before = self.rows.len();
        self.prose(words, look);
        self.flush_if_written();
        let mut rows = self.rows.split_off(before);
        while rows.len() < usize::from(picture.rows()) {
            // The block's own gutter runs down the rows the picture takes, so a picture in a
            // quote is still a block of it.
            rows.push(Row::of(self.opening()));
        }
        if let Some(first) = rows.first().cloned() {
            rows[0] = first.pictured_over(picture.clone());
        }
        rows
    }

    /// One blank row between two blocks, so a heading is not taken for the last line of
    /// the paragraph above it: nothing at the top of the answer and never two blank rows
    /// where a block already stands under one.
    fn gap(&mut self) {
        if !self.rows.is_empty() && !self.blank {
            self.push(Row::blank());
        }
    }

    /// Puts the row in hand down as the row it is: the block's lead and the item's
    /// hanging indent in front of it, unless it is the item's first row and already
    /// carries the marker that indent stands for.
    fn flush(&mut self) {
        let opening = self.opening();
        self.line.clear();
        self.push(Row::of(opening));
        self.used = 0;
        self.spaced = false;
        self.first = false;
    }

    /// The runs every row of the block in hand opens with: the block's lead, the hanging indent
    /// of the items it is inside, and then the row in hand. One reading of a block's gutter, so
    /// a row written by [`Room::flush`] and a row written by hand — a picture's — open the same
    /// way.
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

    /// Puts the row in hand down when it says something: what ends a block, and what a
    /// hard break in the source does.
    fn flush_if_written(&mut self) {
        if !self.line.is_empty() {
            self.flush();
        }
    }

    /// One row put down, and whether it is blank read back from it rather than counted
    /// a second way.
    fn push(&mut self, row: Row) {
        self.blank = row.spans().is_empty();
        self.rows.push(row);
    }

    /// The columns a word of the block in hand has: what the gutters in force leave of
    /// the width, and at least one, because a row that could hold nothing would break
    /// every word into nothing forever.
    fn room(&self) -> usize {
        let bars: usize = self.lead.iter().map(|(text, _)| glyphs(text)).sum();
        let marks: usize = self.marks.iter().map(|mark| glyphs(mark)).sum();
        // The reserve holds only while no row has been written: it is the first row alone
        // that a marker stands beside.
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

    /// Every row this text wrote, and one row of nothing when it wrote none — an entry
    /// with nothing to say is a row of the transcript and not a gap in it.
    fn finish(mut self) -> Vec<Row> {
        self.flush_if_written();
        if self.rows.is_empty() {
            self.push(Row::blank());
        }
        self.rows
    }
}

/// A mermaid fence in hand: the source it is being read as, whether the wire has finished
/// writing it, and where in the answer its block began.
struct Mermaid {
    /// The fence's own words, drawn on the screen and handed to the renderer.
    source: String,
    /// Whether the closing line has arrived. A fence still arriving is never rendered, so a
    /// half-written diagram costs no pass and never flickers (ADR-0008).
    fence: Fence,
    /// The row the block opened on, which is the row its picture is placed over: the words
    /// under it are the source a reader copies and the block its height is reserved for (§12).
    at: usize,
}

/// Whether the closing line of a fenced block has arrived, read off the block `span` the
/// parser handed over: the first line opens it with three or more backticks or tildes, and
/// the last line closes it with a run of the same character, no shorter, and nothing but
/// spaces after it. A block whose last line is content is one the wire is still writing.
///
/// The parse cannot be asked this: it closes the block at the end of the text it was given,
/// so a fence still streaming and a fence that closed arrive as the same two events.
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

/// The fence `line` holds, as the character it is drawn with, how many of them it carries
/// and what follows that run: `None` for a line that is not one, because it is indented more
/// than the three columns a fence may be, or because its run is shorter than three.
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

/// One code block in hand, and the parse `syntect` carries across its lines: a string
/// that opens on one line and closes on the next is one string only because the parse
/// outlives the line.
struct Code {
    /// The parse of the block so far, and nothing when the language is one no syntax
    /// definition names — the block is then drawn plain and says nothing.
    parse: Option<(ParseState, ScopeStack)>,
}

impl Code {
    /// The parse a block whose language is `tag` opens: the tag is read as a name or as
    /// a file extension, so `rust`, `rs` and `Rust` are one language.
    fn of(tag: &str) -> Code {
        let parse = match tag.is_empty() {
            true => None,
            false => syntaxes()
                .find_syntax_by_token(tag)
                .map(|syntax| (ParseState::new(syntax), ScopeStack::new())),
        };
        Code { parse }
    }

    /// One stretch of the block's text, as the runs of each of its lines: a code row is
    /// one line of the code, kept exactly as written.
    fn highlight(&mut self, text: &str) -> Vec<Vec<(String, Look)>> {
        let mut lines = Vec::new();
        let mut rest = text.split('\n').peekable();
        while let Some(line) = rest.next() {
            // The newline that ended the last line is not a line of its own: a block
            // whose text ends in one has no blank row after its last row of code.
            if rest.peek().is_none() && line.is_empty() {
                break;
            }
            lines.push(self.line(line));
        }
        lines
    }

    /// One line of code as the runs it is drawn as, each in the ink its kind of word is
    /// read in: the parse says where one kind of word ends and the next begins, and a
    /// parse that fails is a line drawn plain rather than a run that stops.
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
            // A scope stack operation this build cannot apply leaves the stack as it
            // was: the line is drawn in the ink of the scopes that did apply.
            let _ = scopes.apply(&op);
        }
        if at < line.len() {
            runs.push((String::from(&line[at..]), Look::plain(ink_of(scopes))));
        }
        runs
    }
}

/// The ink the innermost kind of scope stands for: `comment.line.double-slash.rust` is
/// a comment, `keyword.control.rust` a keyword, and a scope no kind names is the frame's
/// own ink. The stack is read from its top down, so the most particular kind of word
/// wins over the block it sits in.
fn ink_of(scopes: &ScopeStack) -> Ink {
    for scope in scopes.as_slice().iter().rev() {
        if let Some((_, ink)) = kinds().iter().find(|(kind, _)| kind.is_prefix_of(*scope)) {
            return *ink;
        }
    }
    Ink::Plain
}

/// One table in hand: the cells it has been read as, until the whole of it has been read
/// and a row of it can be drawn at the width its columns deserve.
#[derive(Debug, Default)]
struct Table {
    /// The header row's cells, left to right.
    head: Vec<Cell>,
    /// The body rows, in order, each its cells.
    rows: Vec<Vec<Cell>>,
    /// The cells of the row being read.
    row: Vec<Cell>,
    /// The runs of the cell being read.
    cell: Vec<(String, Look)>,
    /// Whether the row being read is the header.
    heading: bool,
}

/// One cell of a table, as the runs it was written with: inline code inside a cell is as
/// reversed as inline code in a paragraph, and its words are what the column is measured
/// by.
type Cell = Vec<(String, Look)>;

impl Table {
    /// Glyphs written into the cell being read.
    fn text(&mut self, text: &str, look: Look) {
        if text.is_empty() {
            return;
        }
        self.cell.push((String::from(text), look));
    }

    /// The cell being read closes, and joins the row it is a column of.
    fn close_cell(&mut self) {
        self.row.push(std::mem::take(&mut self.cell));
    }

    /// The header closes, and the cells read so far are the header rather than a row of
    /// the body.
    fn close_head(&mut self) {
        self.head = std::mem::take(&mut self.row);
        self.heading = false;
    }

    /// A body row closes.
    fn close_row(&mut self) {
        if self.heading {
            self.close_head();
            return;
        }
        self.rows.push(std::mem::take(&mut self.row));
    }

    /// How many columns the widest row of this table holds.
    fn columns(&self) -> usize {
        std::iter::once(&self.head)
            .chain(self.rows.iter())
            .map(Vec::len)
            .max()
            .unwrap_or(0)
    }

    /// The columns column `at` needs to be drawn unwrapped, over every cell the table
    /// holds: the widest cell of it, which is what a table wider than the room has to
    /// give up a share of.
    fn width(&self, at: usize) -> usize {
        std::iter::once(&self.head)
            .chain(self.rows.iter())
            .map(|cells| cells.get(at).map_or(0, |cell| measured(cell)))
            .max()
            .unwrap_or(0)
    }

    /// The header row as the rows its cells wrap into at `widths`.
    fn lines(&self, widths: &[usize]) -> Vec<Vec<Cell>> {
        lines_of(&self.head, widths)
    }

    /// The body rows, each still to be wrapped.
    fn body(&self) -> &[Vec<Cell>] {
        &self.rows
    }
}

/// The columns of a table laid out in `budget` columns: a column keeps its own width
/// where the table fits, and where it does not every column is given an equal share and
/// what a narrow one leaves goes to the next that can spend it. Nothing when even one
/// column each cannot be drawn.
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

/// One row of a table as the rows it is drawn as: each cell wrapped to its column's
/// width, and as many rows as the tallest cell of the row takes.
fn lines_of(cells: &[Cell], widths: &[usize]) -> Vec<Vec<Cell>> {
    let wrapped: Vec<Vec<Cell>> = widths
        .iter()
        .enumerate()
        .map(|(at, width)| {
            // A cell is measured for the words that fit it and never for a picture: a
            // picture's height is not a column's width, so this room has no surface.
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

/// One line of a table as the row it is drawn as: the cells padded to their columns'
/// widths and set off from each other by one bar, with nothing along the table's own
/// left and right edges (§12 draws no outer border).
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
        // The last column keeps no blank past its words: padding it would draw cells
        // that say nothing, and a row is a run of glyphs and not a box.
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

/// The rule §12 draws under a header: the header's own row with every word spent as `─`
/// and every bar as `┼`, so the rule is exactly as wide as the row above it.
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

/// The columns a cell takes when it is drawn on one row: what a column is measured by.
fn measured(cell: &[(String, Look)]) -> usize {
    cell.iter().map(|(text, _)| glyphs(text)).sum()
}
