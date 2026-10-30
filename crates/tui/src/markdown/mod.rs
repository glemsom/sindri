//! The answer as a reader sees it: headings, lists, quotes, tables, inline
//! code, and fenced code with truecolour highlighting. No images, no raw
//! HTML, no maths (ADR-0011).

mod code;
mod table;
pub(crate) mod text;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme;
use table::Table;

pub fn lines(source: &str, width: u16) -> Vec<Line<'static>> {
    let mut writer = Writer::new(width.max(1) as usize);
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    for event in Parser::new_ext(source, options) {
        writer.event(event);
    }
    writer.finish()
}

struct List {
    next: Option<u64>,
}

struct Writer {
    width: usize,
    lines: Vec<Line<'static>>,
    pending: Vec<Span<'static>>,
    indent: String,
    quote: usize,
    lists: Vec<List>,
    emphasis: Modifier,
    heading: bool,
    apart: bool,
    code: Option<(Option<String>, String)>,
    table: Option<Table>,
    row: usize,
    in_table_head: bool,
}

impl Writer {
    fn new(width: usize) -> Self {
        Writer {
            width,
            lines: Vec::new(),
            pending: Vec::new(),
            indent: String::new(),
            quote: 0,
            lists: Vec::new(),
            emphasis: Modifier::empty(),
            heading: false,
            apart: false,
            code: None,
            table: None,
            row: 0,
            in_table_head: false,
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush();
        self.lines
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => match (&mut self.code, &mut self.table) {
                (Some((_, body)), _) => body.push_str(&text),
                (None, Some(_)) => self.pending.push(Span::raw(text.to_string())),
                (None, None) => {
                    let style = self.style();
                    self.pending.push(Span::styled(text.to_string(), style));
                }
            },
            Event::Code(text) => self
                .pending
                .push(Span::styled(text.to_string(), theme::code())),
            Event::SoftBreak => self.pending.push(Span::raw(" ")),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.block_end();
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(self.width),
                    theme::faint(),
                )));
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { .. } => self.heading = true,
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
            }
            Tag::Table(_) => self.table = Some(Table::default()),
            Tag::TableHead => {
                self.in_table_head = true;
                self.row = 0;
            }
            Tag::TableRow => {
                self.row = self.table.as_ref().map_or(0, |table| table.rows.len());
            }
            Tag::Emphasis => self.emphasis.insert(Modifier::ITALIC),
            Tag::Strong => self.emphasis.insert(Modifier::BOLD),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.heading = false;
                self.block_end();
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
            TagEnd::Emphasis => self.emphasis.remove(Modifier::ITALIC),
            TagEnd::Strong => self.emphasis.remove(Modifier::BOLD),
            _ => {}
        }
    }

    fn item(&mut self) {
        self.flush();
        let Some(list) = self.lists.last_mut() else {
            return;
        };
        let marker = match list.next.as_mut() {
            Some(next) => {
                let marker = format!("{next}. ");
                *next += 1;
                marker
            }
            None => "• ".to_string(),
        };
        self.indent = " ".repeat(self.lists.len().saturating_sub(1) * 2) + &marker;
    }

    fn style(&self) -> Style {
        if self.heading {
            return Style::new().fg(theme::HEADING).add_modifier(Modifier::BOLD);
        }
        let mut style = Style::new();
        if self.emphasis.contains(Modifier::BOLD) {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.emphasis.contains(Modifier::ITALIC) {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    }

    fn block_end(&mut self) {
        self.flush();
        self.apart = true;
        self.indent.clear();
        self.pending.clear();
    }

    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let visible = self
            .pending
            .iter()
            .any(|span| !span.content.trim().is_empty());
        let spans = std::mem::take(&mut self.pending);
        if !visible {
            return;
        }
        if self.apart && !self.lines.is_empty() {
            self.lines.push(Line::default());
        }
        self.apart = false;
        let mut prefix = "│ ".repeat(self.quote);
        prefix.push_str(&self.indent);
        self.indent.clear();
        let cells = self.width.saturating_sub(prefix.chars().count());
        self.lines.extend(text::wrap(
            text::glyphs(&spans),
            &prefix,
            theme::faint(),
            cells,
        ));
    }

    fn code_block(&mut self) {
        let Some((language, body)) = self.code.take() else {
            return;
        };
        if self.apart && !self.lines.is_empty() {
            self.lines.push(Line::default());
        }
        self.apart = false;
        self.lines.extend(code::lines(
            body.trim_end_matches('\n'),
            language.as_deref(),
        ));
        self.apart = true;
    }

    fn cell(&mut self) {
        let text: String = self
            .pending
            .drain(..)
            .map(|span| span.content.to_string())
            .collect();
        let row = self.row;
        if let Some(table) = self.table.as_mut() {
            table.cell(row, text.trim().to_string());
            if self.in_table_head && table.head <= row {
                table.head = row + 1;
            }
        }
        self.apart = false;
    }

    fn table_block(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        if self.apart && !self.lines.is_empty() {
            self.lines.push(Line::default());
        }
        self.lines.extend(table::lines(&table, self.width));
        self.apart = true;
    }
}
