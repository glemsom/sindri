//! The blocks an answer is made of, written as rows: a paragraph, a fence, a
//! table, a rule. Each is produced whole and handed to `keep`, which counts it in
//! the answer's own rows and builds only the rows of it a window holds — so a
//! block nobody sees costs a count and no ink, and a frame paid for what it shows.

use pulldown_cmark::HeadingLevel;
use ratatui::text::{Line, Span};

use crate::markdown::{code, heading, mermaid, table, text};
use crate::theme;

use super::Writer;

impl Writer<'_> {
    pub(super) fn item(&mut self) {
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

    /// The row beneath a heading, as its level asks for: the words have just
    /// been flushed, so the rule is the next row of the block.
    pub(super) fn heading_rule(&mut self, level: HeadingLevel) {
        let prefix = "│ ".repeat(self.quote);
        if let Some(rule) = heading::rule(level as u8, &prefix, self.width) {
            self.keep(vec![rule]);
        }
    }

    pub(super) fn block_end(&mut self) {
        self.flush();
        self.apart = true;
        self.indent.clear();
        self.pending.clear();
    }

    /// Whether the rows a block is about to make hold a row the caller wants: a
    /// block wholly above the window or below it is counted and not painted.
    pub(super) fn wanted(&self, rows: usize) -> bool {
        self.wanted_from(self.counted, rows)
    }

    /// The rows a block made, counted whole and kept where the caller asked for
    /// them: the answer is counted in its own rows even where none of them is
    /// built.
    pub(super) fn keep(&mut self, rows: Vec<Line<'static>>) {
        let start = self.counted;
        self.counted += rows.len();
        if !self.wanted_from(start, rows.len()) {
            return;
        }
        let skip = self.keep.start.saturating_sub(start);
        let take = self.keep.end.saturating_sub(start).min(rows.len());
        self.lines.extend(rows.into_iter().take(take).skip(skip));
    }

    /// The same question for rows that already have their place in the answer.
    pub(super) fn wanted_from(&self, start: usize, rows: usize) -> bool {
        start < self.keep.end && start.saturating_add(rows) > self.keep.start
    }

    pub(super) fn flush(&mut self) {
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
        if self.apart && self.counted > 0 {
            self.keep(vec![Line::default()]);
        }
        self.apart = false;
        let mut prefix = "│ ".repeat(self.quote);
        prefix.push_str(&self.indent);
        self.indent.clear();
        let cells = self.width.saturating_sub(prefix.chars().count());
        let mut glyphs = text::glyphs(&spans);
        // A row never opens with a space the block did not ask for: the indent
        // is the writer's, and text that resumes below a picture's caption
        // starts its row with its first word.
        let lead = glyphs.iter().take_while(|(ch, _)| *ch == ' ').count();
        glyphs.drain(..lead);
        self.keep(text::wrap(glyphs, &prefix, theme::faint(), cells));
    }

    /// Whether the fence that has just ended is still arriving: its body runs
    /// to the end of the answer, so its closing delimiter is a line the stream
    /// has not sent yet and the body will be longer on the next delta. Read
    /// from the bytes rather than from the parser, which ends an unterminated
    /// block at the document's end and says nothing about why.
    pub(super) fn arriving(&self) -> bool {
        self.source
            .get(self.code_read..)
            .is_some_and(|tail| tail.trim().is_empty())
    }

    pub(super) fn code_block(&mut self) {
        let Some((language, body)) = self.code.take() else {
            return;
        };
        if self.apart && self.counted > 0 {
            self.keep(vec![Line::default()]);
        }
        self.apart = false;
        // A fence naming `mermaid` is a picture where a picture is a block. It
        // branches here so that the panel below stays the one thing every other
        // fence is drawn as.
        let arriving = self.arriving();
        let refused = match mermaid::fence(self, language.as_deref(), &body, arriving) {
            mermaid::Fence::Drawn => return,
            mermaid::Fence::Panel => None,
            mermaid::Fence::Refused(reason) => Some(reason),
        };
        // A fence the window does not hold is counted and never painted: its
        // rows are a count and its ink is nobody's, and painting it is what a
        // long answer costs a frame that shows its last forty rows.
        let body = body.trim_end_matches('\n');
        let rows = code::height(body, self.width);
        if self.wanted(rows) {
            self.keep(code::panel(body, language.as_deref(), self.width));
        } else {
            self.counted += rows;
        }
        // A fence nothing would draw keeps its source, with the parser's words
        // under it in the ink a caption keeps: no cell of it is swallowed.
        if let Some(reason) = refused {
            let words = [Span::styled(reason, theme::faint())];
            let rows = text::wrap(text::glyphs(&words), "", theme::faint(), self.width);
            self.keep(rows);
        }
        self.apart = true;
    }

    pub(super) fn cell(&mut self) {
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

    pub(super) fn table_block(&mut self) {
        let Some(table) = self.table.take() else {
            return;
        };
        if self.apart && self.counted > 0 {
            self.keep(vec![Line::default()]);
        }
        self.keep(table::lines(&table, self.width));
        self.apart = true;
    }
}
