//! The answer as a reader sees it: a heading scale, links that keep their
//! target, lists, quotes, tables, inline code, and fenced code with truecolour
//! highlighting, and a picture as a block of its own. No
//! raw HTML, no maths.

mod blocks;
mod code;
mod heading;
mod mermaid;
mod picture;
mod role;
mod table;
pub(crate) mod text;

pub use picture::{Picture, Written};

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::picture::Store;
use crate::theme;
use picture::Image;
use table::Table;

/// An answer as rows, and the pictures among them: `store` is what measures a
/// source, and `None` where the terminal draws none, which makes every picture
/// its caption and the reason.
pub fn lines(source: &str, width: u16, store: Option<&Store>) -> Written {
    lines_in(source, width, store, 0..usize::MAX)
}

/// The rows of `rows` out of the answer, and the count it is worth whole: a
/// frame asks for the rows its window holds, and the answer above and below them
/// is counted rather than laid out. A caller that wants every row asks for
/// `lines`.
pub fn lines_in(source: &str, width: u16, store: Option<&Store>, rows: Range<usize>) -> Written {
    walk(source, width, store, rows, 0)
}

/// How many rows the answer is worth at `width`, counted and not built: what a
/// frame places a window against, and the number a scroll-back, `End`, `Home`
/// and the new-rows badge all land on.
pub fn height(source: &str, width: u16, store: Option<&Store>) -> usize {
    counted(source, width, store, None).rows
}

/// Where an answer's last block begins, and the rows it is worth above it: the
/// two figures a streamed delta is counted from, so the answer above the block
/// it landed in is counted once and never read again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cut {
    /// Bytes into the answer at the first character of its last top-level block.
    pub at: usize,
    /// The rows the answer is worth above that block.
    pub above: usize,
}

/// An answer as a count: the rows it is worth, and the cut the next delta
/// landing in it is counted from.
#[derive(Debug, Clone, Copy)]
pub struct Counted {
    pub rows: usize,
    pub cut: Cut,
}

/// The rows the answer is worth, and the cut its tail is counted from. From a
/// `cut` left by the last count it reads only the block that cut opens: the
/// answer above it is what `above` already says it is worth, so a fence that
/// closed there is neither laid out nor highlighted again — which is what makes
/// a delta cost the rows it touched and not the answer it landed in.
pub fn counted(source: &str, width: u16, store: Option<&Store>, from: Option<Cut>) -> Counted {
    let Some(from) = from.filter(|from| source.is_char_boundary(from.at)) else {
        let written = walk(source, width, store, 0..0, 0);
        return Counted {
            rows: written.total,
            cut: written.cut,
        };
    };
    let written = walk(&source[from.at..], width, store, 0..0, from.above);
    // The writer is handed the rows the answer is worth above the cut, so what
    // it counts is the answer's own arithmetic and only the bytes are the
    // tail's: the one figure that has to be moved is where the block starts.
    Counted {
        rows: written.total,
        cut: Cut {
            at: from.at + written.cut.at,
            above: written.cut.above,
        },
    }
}

/// The answer walked whole, with `rows` of it kept and `above` rows already
/// counted before the first of them: the one place the parser is driven, and so
/// the one place the answer's row count and its rows are made from the same
/// walk.
fn walk(
    source: &str,
    width: u16,
    store: Option<&Store>,
    rows: Range<usize>,
    above: usize,
) -> Written {
    let mut writer = Writer::new(width.max(1) as usize, store, source);
    writer.keep = rows;
    // The rows above this source are the answer's, and they are what tells the
    // writer a block introduces itself with a blank row: an answer counted from
    // its first byte has none, and one counted from a cut has them all.
    writer.apart = above > 0;
    writer.counted = above;
    writer.cut = Cut { at: 0, above };
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        writer.event(event, range);
    }
    writer.finish()
}

struct List {
    next: Option<u64>,
}

struct Writer<'a> {
    width: usize,
    /// The answer being written, whole: a fence's place in it is read from
    /// these bytes and nowhere else, because the parser says where a block
    /// ends and nothing about whether its delimiter arrived.
    source: &'a str,
    lines: Vec<Line<'static>>,
    /// The pictures the writer placed, and the row each block starts at: what
    /// the frame draws where, once it knows what the terminal can draw.
    pictures: Vec<Picture>,
    pending: Vec<Span<'static>>,
    indent: String,
    quote: usize,
    lists: Vec<List>,
    emphasis: Modifier,
    heading: Option<u8>,
    /// The target of the link being written, empty when it points nowhere: a
    /// link with no URL is its words and nothing else.
    link: Option<String>,
    apart: bool,
    code: Option<(Option<String>, String)>,
    table: Option<Table>,
    row: usize,
    in_table_head: bool,
    /// What measures a picture, or nothing where the terminal draws none: the
    /// writer asks it and never reads a file itself.
    store: Option<&'a Store>,
    /// The image being written: its source, and the alt text it carried.
    image: Option<Image>,
    /// How far into the source the code block being written has been read: the
    /// end of the last of its text, and nothing else.
    code_read: usize,
    /// The rows of the answer the caller wants, in the answer's own rows: what
    /// the window holds. Every row outside it is counted and never built.
    keep: Range<usize>,
    /// How many rows the answer has said so far, and so where the next one
    /// goes: the count is kept in the answer's coordinates even where the rows
    /// themselves are not.
    counted: usize,
    /// Where the last top-level block of the answer begins, and the rows above
    /// it: the place a delta that lands in it is counted from.
    cut: Cut,
    /// How deep in blocks the writer is: a block inside another — a paragraph
    /// in a list item, a row of a table — is no place to resume the answer at.
    depth: usize,
}

impl<'a> Writer<'a> {
    fn new(width: usize, store: Option<&'a Store>, source: &'a str) -> Self {
        Writer {
            width,
            source,
            lines: Vec::new(),
            pictures: Vec::new(),
            pending: Vec::new(),
            indent: String::new(),
            quote: 0,
            lists: Vec::new(),
            emphasis: Modifier::empty(),
            heading: None,
            link: None,
            apart: false,
            code: None,
            table: None,
            row: 0,
            in_table_head: false,
            store,
            image: None,
            code_read: 0,
            keep: 0..usize::MAX,
            counted: 0,
            cut: Cut { at: 0, above: 0 },
            depth: 0,
        }
    }

    fn finish(mut self) -> Written {
        self.flush();
        Written {
            rows: self.lines,
            pictures: self.pictures,
            total: self.counted,
            cut: self.cut,
        }
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) {
        self.boundary(&event, &range);
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            // Inside an image, the text *is* the picture's words: they are the
            // caption, never prose of their own.
            Event::Text(text) => {
                if let Some(alt) = self.alt() {
                    alt.push_str(&text);
                    return;
                }
                match (&mut self.code, &mut self.table) {
                    (Some((_, body)), _) => {
                        body.push_str(&text);
                        self.code_read = self.code_read.max(range.end);
                    }
                    (None, Some(_)) => self.pending.push(Span::raw(text.to_string())),
                    (None, None) => {
                        let style = self.style();
                        self.pending.push(Span::styled(text.to_string(), style));
                    }
                }
            }
            Event::Code(text) => {
                if let Some(alt) = self.alt() {
                    alt.push_str(&text);
                    return;
                }
                self.pending
                    .push(Span::styled(text.to_string(), theme::code()));
            }
            // A soft break is a space of the prose around it, in the prose's
            // ink: a raw span would leave the cell to the terminal.
            Event::SoftBreak => {
                let style = self.style();
                self.pending.push(Span::styled(" ", style));
            }
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.block_end();
                self.keep(vec![Line::from(Span::styled(
                    "─".repeat(self.width),
                    theme::faint(),
                ))]);
            }
            _ => {}
        }
    }

    /// Records where the last block the answer is made of begins, and the rows
    /// above it: a block still arriving keeps the place it opened at, so the
    /// delta that lengthens it is counted from there and not from the answer's
    /// first row.
    fn boundary(&mut self, event: &Event<'_>, range: &Range<usize>) {
        match event {
            Event::Start(_) => {
                if self.depth == 0 {
                    self.open(range.start);
                }
                self.depth += 1;
            }
            Event::End(_) => self.depth = self.depth.saturating_sub(1),
            // A rule is a block with no tag to open or close it.
            Event::Rule if self.depth == 0 => self.open(range.start),
            _ => {}
        }
    }

    fn open(&mut self, at: usize) {
        self.cut = Cut {
            at,
            above: self.counted,
        };
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => self.heading = Some(level as u8),
            Tag::BlockQuote(_) => self.quote += 1,
            Tag::List(start) => self.lists.push(List { next: start }),
            Tag::Item => self.item(),
            Tag::CodeBlock(kind) => {
                let language = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let token = info.split_whitespace().next().unwrap_or_default();
                        (!token.is_empty()).then(|| token.to_string())
                    }
                    CodeBlockKind::Indented => None,
                };
                self.code = Some((language, String::new()));
                self.code_read = 0;
            }
            Tag::Table(_) => self.table = Some(Table::default()),
            Tag::TableHead => {
                self.in_table_head = true;
                self.row = 0;
            }
            Tag::TableRow => {
                self.row = self.table.as_ref().map_or(0, |table| table.rows.len());
            }
            Tag::Link { dest_url, .. } => self.link = Some(dest_url.to_string()),
            Tag::Image { dest_url, .. } => {
                self.image = Some(Image {
                    src: dest_url.to_string(),
                    alt: String::new(),
                });
            }
            Tag::Emphasis => self.emphasis.insert(Modifier::ITALIC),
            Tag::Strong => self.emphasis.insert(Modifier::BOLD),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(level) => {
                self.heading = None;
                self.block_end();
                self.heading_rule(level);
            }
            TagEnd::BlockQuote(_) => {
                self.quote = self.quote.saturating_sub(1);
                self.block_end();
            }
            TagEnd::Paragraph => self.block_end(),
            TagEnd::List(_) => {
                self.lists.pop();
                if self.lists.is_empty() {
                    self.apart = true;
                }
            }
            TagEnd::Item => self.flush(),
            TagEnd::CodeBlock => self.code_block(),
            TagEnd::TableCell => self.cell(),
            TagEnd::TableRow => {}
            TagEnd::TableHead => self.in_table_head = false,
            TagEnd::Table => self.table_block(),
            TagEnd::Link => self.link_end(),
            TagEnd::Image => self.image_end(),
            TagEnd::Emphasis => self.emphasis.remove(Modifier::ITALIC),
            TagEnd::Strong => self.emphasis.remove(Modifier::BOLD),
            _ => {}
        }
    }

    fn style(&self) -> Style {
        if let Some(level) = self.heading {
            return heading::style(level);
        }
        let mut style = theme::plain();
        if self
            .link
            .as_deref()
            .is_some_and(|target| !target.is_empty())
        {
            style = style.fg(theme::PROMPT);
        }
        if self.emphasis.contains(Modifier::BOLD) {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.emphasis.contains(Modifier::ITALIC) {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    }

    /// A link's target, dimmed beside its words. A link that names nowhere
    /// adds nothing at all: the words were already drawn as plain text.
    fn link_end(&mut self) {
        let Some(target) = self.link.take().filter(|target| !target.is_empty()) else {
            return;
        };
        self.pending
            .push(Span::styled(format!(" ‹{target}›"), theme::faint()));
    }
}
