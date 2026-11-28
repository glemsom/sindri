use std::sync::OnceLock;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

use crate::graphics::Ceiling;
use crate::ink::{Face, Faces, Ink, Look};
use crate::link::Way;
use crate::mermaid::{self, Fence};
use crate::pen;
use crate::picture::{self, Picture, Surface};
use crate::rows::{Row, split_at};
use crate::size::{self, Sized};

const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_GFM);

const UNDERLINE: Ink = Ink::Dim;

const FAINT_ACCENT: Look = Look::plain(Ink::Accent).wearing(Faces::FAINT);

const ACCENT_BOLD: Look = Look::plain(Ink::Accent).wearing(Faces::BOLD);
const BOLD_PLAIN: Look = Look::plain(Ink::Plain).wearing(Faces::BOLD);

const HAIRLINE: char = '─';

const GUIDE: char = '│';

const SUPERSCRIPT: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wholeness {
    Whole,
    Arriving,
}

const BULLET: &str = "• ";

const TICK: char = '✓';
const BOX: char = '☐';

pub(crate) const BAR: &str = "▏ ";

pub fn rows(text: &str, cols: u16) -> Vec<Row> {
    rows_under(text, cols, 0)
}

pub fn rows_under(text: &str, cols: u16, reserve: u16) -> Vec<Row> {
    rows_on(text, cols, reserve, &Surface::none())
}

pub fn rows_on(text: &str, cols: u16, reserve: u16, surface: &Surface) -> Vec<Row> {
    rows_in(text, cols, reserve, "", surface, Wholeness::Whole)
}

pub(crate) fn rows_in(
    text: &str,
    cols: u16,
    reserve: u16,
    gutter: &str,
    surface: &Surface,
    wholeness: Wholeness,
) -> Vec<Row> {
    let mut room = Room::new(cols.max(1), surface);
    room.reserve = usize::from(reserve);
    room.wholeness = wholeness;
    if !gutter.is_empty() {
        room.base = vec![(String::from(gutter), Look::plain(Ink::Plain))];
    }
    for (event, span) in Parser::new_ext(text, OPTIONS).into_offset_iter() {
        room.event(event, &text[span]);
    }
    room.finish()
}

pub fn body(text: &str, cols: u16, way: &Way) -> Option<Vec<Row>> {
    if way.written().is_empty() {
        return None;
    }
    let mut room = Room::new(cols.max(1), &Surface::none());
    let plain = Look::plain(Ink::Plain);
    let marked = plain.underlined(UNDERLINE);
    let mut named = false;
    for line in text.split('\n') {
        let mut rest = line;
        while let Some(at) = rest.find(way.written()) {
            let (before, from) = rest.split_at(at);
            let (named_words, after) = from.split_at(way.written().len());
            room.prose(before, plain);
            room.destination = Some(way.destination().to_owned());
            room.prose(named_words, marked);
            room.destination = None;
            rest = after;
            named = true;
        }
        room.prose(rest, plain);
        room.flush();
    }
    named.then_some(room.rows)
}

fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn kinds() -> &'static [(Scope, Look)] {
    static KINDS: OnceLock<Vec<(Scope, Look)>> = OnceLock::new();
    KINDS.get_or_init(|| {
        [
            ("comment", Look::new(Ink::Dim, Face::Italic)),
            ("entity.name.function", BOLD_PLAIN),
            ("support.function", BOLD_PLAIN),
            ("variable.function", BOLD_PLAIN),
            ("support.macro", BOLD_PLAIN),
            ("entity.other.attribute-name", Look::plain(Ink::Mark)),
            ("variable.annotation", Look::plain(Ink::Mark)),
            ("entity.name.type", Look::plain(Ink::Err)),
            ("entity.name.struct", Look::plain(Ink::Err)),
            ("entity.name.class", Look::plain(Ink::Err)),
            ("entity.name.enum", Look::plain(Ink::Err)),
            ("entity.name.trait", Look::plain(Ink::Err)),
            ("entity.name.impl", Look::plain(Ink::Err)),
            ("support.type", Look::plain(Ink::Err)),
            ("support.class", Look::plain(Ink::Err)),
            ("string", Look::plain(Ink::Ok)),
            ("keyword.operator", Look::plain(Ink::Plain)),
            ("keyword", ACCENT_BOLD),
            ("storage", ACCENT_BOLD),
            ("constant", Look::plain(Ink::Muted)),
        ]
        .into_iter()
        .filter_map(|(name, look)| Scope::new(name).ok().map(|scope| (scope, look)))
        .collect()
    })
}

fn large(level: HeadingLevel) -> bool {
    matches!(level, HeadingLevel::H1 | HeadingLevel::H2)
}

fn heading(level: HeadingLevel) -> (Look, Option<char>) {
    match level {
        HeadingLevel::H1 => (Look::new(Ink::Accent, Face::Bold), Some('═')),
        HeadingLevel::H2 => (Look::new(Ink::Plain, Face::Bold), Some('─')),
        HeadingLevel::H3 => (Look::new(Ink::Muted, Face::Bold), None),
        HeadingLevel::H4 => (Look::plain(Ink::Muted), None),
        HeadingLevel::H5 => (Look::new(Ink::Dim, Face::Bold), None),
        HeadingLevel::H6 => (Look::new(Ink::Dim, Faces::BOLD.and(Faces::ITALIC)), None),
    }
}

fn alert(kind: BlockQuoteKind) -> (&'static str, Ink) {
    match kind {
        BlockQuoteKind::Note => ("Note", Ink::Muted),
        BlockQuoteKind::Tip => ("Tip", Ink::Ok),
        BlockQuoteKind::Important => ("Important", Ink::Accent),
        BlockQuoteKind::Warning => ("Warning", Ink::Plain),
        BlockQuoteKind::Caution => ("Caution", Ink::Err),
    }
}

struct Rule {
    mark: char,
    wide: usize,
}

impl Rule {
    fn new(mark: char) -> Rule {
        Rule { mark, wide: 0 }
    }

    fn wrote(&mut self, cells: usize) {
        self.wide = self.wide.max(cells);
    }

    fn run(&self, room: usize) -> String {
        pen::filled(pen::columns(self.wide.min(room)), self.mark)
    }
}

struct Mark {
    text: String,
    ink: Ink,
}

struct Laid {
    left: usize,
    width: usize,
    destination: String,
}

struct Room {
    wide: u16,
    rows: Vec<Row>,
    line: Vec<(String, Look)>,
    used: usize,
    spaced: bool,
    block: Look,
    faces: Vec<Face>,
    lead: Vec<(String, Look)>,
    base: Vec<(String, Look)>,
    marks: Vec<Mark>,
    lists: Vec<Option<u64>>,
    first: bool,
    blank: bool,
    labelled: Option<usize>,
    code: Option<Code>,
    mermaid: Option<Mermaid>,
    table: Option<Table>,
    rule: Option<Rule>,
    destination: Option<String>,
    laid: Vec<Laid>,
    reserve: usize,
    surface: Surface,
    image: Option<(String, String)>,
    scale: u16,
    refs: Vec<String>,
    notes: Vec<Note>,
    gathering: Option<Note>,
    wholeness: Wholeness,
    fence: Option<usize>,
    opened: usize,
    source: Option<String>,
    naming: Option<usize>,
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
            faces: Vec::new(),
            lead: Vec::new(),
            base: Vec::new(),
            marks: Vec::new(),
            lists: Vec::new(),
            first: true,
            blank: true,
            labelled: None,
            destination: None,
            laid: Vec::new(),
            code: None,
            mermaid: None,
            table: None,
            rule: None,
            reserve: 0,
            surface: surface.clone(),
            image: None,
            scale: 1,
            refs: Vec::new(),
            notes: Vec::new(),
            gathering: None,
            wholeness: Wholeness::Whole,
            fence: None,
            opened: 0,
            source: None,
            naming: None,
        }
    }

    fn event(&mut self, event: Event, span: &str) {
        if self.gathering.is_some() || matches!(event, Event::Start(Tag::FootnoteDefinition(_))) {
            self.note(event, span);
            return;
        }
        match event {
            Event::FootnoteReference(name) => self.reference(&name),
            Event::Start(Tag::Paragraph) => self.open_block(),
            Event::End(TagEnd::Paragraph) => self.flush_if_written(),
            Event::Start(Tag::Heading { level, .. }) => self.open_heading(level),
            Event::End(TagEnd::Heading(_)) => self.close_heading(),
            Event::Start(Tag::BlockQuote(kind)) => self.open_quote(kind),
            Event::End(TagEnd::BlockQuote(_)) => {
                self.flush_if_written();
                self.lead.pop();
                self.labelled = None;
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
            Event::TaskListMarker(checked) => self.mark_task(checked),
            Event::End(TagEnd::Item) => self.close_item(),
            Event::Start(Tag::Table(aligns)) => {
                self.open_block();
                self.table = Some(Table {
                    aligns,
                    ..Table::default()
                });
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
            Event::Start(Tag::Strong) => self.faces.push(Face::Bold),
            Event::Start(Tag::Emphasis) => self.faces.push(Face::Italic),
            Event::Start(Tag::Strikethrough) => self.faces.push(Face::Crossed),
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                self.faces.pop();
            }
            Event::Start(Tag::CodeBlock(kind)) => self.open_code(kind, span),
            Event::End(TagEnd::CodeBlock) => self.close_code(),
            Event::Text(text) => match &mut self.code {
                Some(code) => {
                    if let Some(mermaid) = &mut self.mermaid {
                        mermaid.source.push_str(&text);
                    }
                    for line in code.highlight(&text) {
                        for (text, look) in line.runs {
                            self.extend(&text, look);
                        }
                        self.source = Some(line.source);
                        self.flush();
                    }
                }
                None => {
                    let look = self.prose_look();
                    let way = self.destination.clone();
                    match &mut self.table {
                        Some(table) => table.text(&text, look, way.as_deref()),
                        None => match &mut self.image {
                            Some((_, alt)) => alt.push_str(&text),
                            None => self.prose(&text, look),
                        },
                    }
                }
            },
            Event::Code(text) => {
                let look = match self.destination.is_some() {
                    true => Look::plain(Ink::Accent).underlined(UNDERLINE),
                    false => Look::plain(Ink::Accent),
                };
                self.run(&text, look);
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                self.image = Some((String::from(&*dest_url), String::new()));
            }
            Event::End(TagEnd::Image) => self.close_image(),
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.destination = Some(String::from(&*dest_url));
            }
            Event::End(TagEnd::Link) => self.destination = None,
            Event::SoftBreak => {
                let look = self.prose_look();
                let way = self.destination.clone();
                match &mut self.table {
                    Some(table) => table.text(" ", look, way.as_deref()),
                    None => match &mut self.image {
                        Some((_, alt)) => alt.push(' '),
                        None => self.spaced = true,
                    },
                }
            }
            Event::Rule => self.draw_rule(),
            Event::InlineHtml(tag) => {
                if breaking(&tag) {
                    self.end_row();
                }
            }
            Event::HardBreak => self.end_row(),
            _ => {}
        }
    }

    fn note(&mut self, event: Event, span: &str) {
        match event {
            Event::Start(Tag::FootnoteDefinition(name)) => {
                self.gathering = Some(Note {
                    name: String::from(&*name),
                    events: Vec::new(),
                    ended: false,
                });
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                if let Some(mut note) = self.gathering.take() {
                    note.ended = ended(span);
                    self.referenced(&note.events);
                    self.notes.push(note);
                }
            }
            event => {
                if let Some(note) = &mut self.gathering {
                    note.events.push((event.into_static(), String::from(span)));
                }
            }
        }
    }

    fn reference(&mut self, name: &str) {
        let at = match self.refs.iter().position(|known| known == name) {
            Some(at) => at,
            None => {
                self.refs.push(String::from(name));
                self.refs.len() - 1
            }
        };
        self.run(&superscript(at.saturating_add(1)), Look::plain(Ink::Dim));
    }

    fn referenced(&mut self, events: &[(Event<'static>, String)]) {
        for (event, _) in events {
            let Event::FootnoteReference(name) = event else {
                continue;
            };
            if !self.refs.iter().any(|known| known == &**name) {
                self.refs.push(name.to_string());
            }
        }
    }

    fn run(&mut self, text: &str, look: Look) {
        let way = self.destination.clone();
        match &mut self.table {
            Some(table) => table.text(text, look, way.as_deref()),
            None => {
                self.place(text, look, self.spaced);
                self.spaced = false;
            }
        }
    }

    fn open_heading(&mut self, level: HeadingLevel) {
        self.open_block();
        if level == HeadingLevel::H1 && self.rows.is_empty() {
            self.push(Row::blank());
        }
        let (look, rule) = heading(level);
        self.block = look;
        self.rule = rule.map(Rule::new);
        self.scale = match self.surface.sizing() && large(level) {
            true => size::SCALE,
            false => 1,
        };
    }

    fn close_heading(&mut self) {
        self.flush_if_written();
        let rule = self.rule.take();
        self.block = Look::plain(Ink::Plain);
        self.scale = 1;
        let run = rule.map_or_else(String::new, |rule| rule.run(self.room_cells()));
        if !run.is_empty() {
            let mut spans = self.gutter();
            spans.push((run, FAINT_ACCENT));
            self.push(Row::of(spans));
        }
    }

    fn end_row(&mut self) {
        match &mut self.table {
            Some(table) => table.break_line(),
            None => self.flush_if_written(),
        }
    }

    fn draw_rule(&mut self) {
        self.flush_if_written();
        self.open_block();
        let mut spans = self.gutter();
        spans.push((
            pen::filled(pen::columns(self.room_cells()), HAIRLINE),
            Look::plain(Ink::Dim),
        ));
        self.first = false;
        self.push(Row::of(spans));
    }

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

    fn worn(&self) -> Faces {
        self.faces
            .iter()
            .fold(Faces::PLAIN, |worn, face| worn.and(Faces::wearing(*face)))
    }

    fn way(&self) -> Look {
        Look::new(Ink::Accent, self.block.wearing(self.worn()).faces()).underlined(UNDERLINE)
    }

    fn prose_look(&self) -> Look {
        match self.destination.is_some() {
            true => self.way(),
            false => self.block.wearing(self.worn()),
        }
    }

    fn place(&mut self, text: &str, look: Look, space: bool) {
        let mut rest = text;
        let mut space = space;
        loop {
            let room = self.room();
            let gap = usize::from(space && self.used > 0);
            if self.used + gap + usize::from(pen::cells(rest)) <= room {
                if gap == 1 {
                    self.between();
                }
                self.linked(rest, look);
                return;
            }
            if self.used > 0 {
                self.flush();
                space = false;
                continue;
            }
            let (head, tail) = split_at(rest, room);
            self.linked(&head, look);
            self.flush();
            rest = tail;
            space = false;
            if rest.is_empty() {
                return;
            }
        }
    }

    fn between(&mut self) {
        match self.reaches() {
            true => {
                let at = self.way();
                self.extend(" ", at);
                if let Some(run) = self.laid.last_mut() {
                    run.width = run.width.saturating_add(1);
                }
            }
            false => {
                let at = self.block;
                self.extend(" ", at);
            }
        }
    }

    fn reaches(&self) -> bool {
        let Some(destination) = &self.destination else {
            return false;
        };
        self.laid.last().is_some_and(|run| {
            run.destination == *destination && run.left.saturating_add(run.width) == self.used
        })
    }

    fn linked(&mut self, text: &str, look: Look) {
        let left = self.used;
        let width = usize::from(pen::cells(text));
        self.extend(text, look);
        let Some(destination) = &self.destination else {
            return;
        };
        match self.laid.last_mut() {
            Some(run) if run.destination == *destination && run.left + run.width == left => {
                run.width = run.width.saturating_add(width);
            }
            _ => self.laid.push(Laid {
                left,
                width,
                destination: destination.clone(),
            }),
        }
    }

    fn extend(&mut self, text: &str, look: Look) {
        if text.is_empty() {
            return;
        }
        self.used = self.used.saturating_add(usize::from(pen::cells(text)));
        match self.line.last_mut() {
            Some((last, at)) if *at == look => last.push_str(text),
            _ => self.line.push((String::from(text), look)),
        }
    }

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
            self.draw_columns(&table, available);
            return;
        };

        for line in table.lines(&widths) {
            let row = joined(&line, &widths, &table.aligns);
            self.guttered(row);
        }
        let rule = header_rule(&widths);
        self.guttered(rule);
        for cells in table.body() {
            for line in lines_of(cells, &widths) {
                let row = joined(&line, &widths, &table.aligns);
                self.guttered(row);
            }
        }
    }

    fn draw_columns(&mut self, table: &Table, available: usize) {
        for at in 0..table.columns() {
            if at > 0 {
                self.gap();
            }
            let cells =
                std::iter::once(table.head.get(at)).chain(table.rows.iter().map(|row| row.get(at)));
            for (position, cell) in cells.enumerate() {
                if let Some(cell) = cell {
                    let align = table.aligns.get(at..).unwrap_or_default();
                    for line in lines_of(std::slice::from_ref(cell), &[available]) {
                        let row = joined(&line, &[available], align);
                        self.guttered(row);
                    }
                }
                if position == 0 {
                    let rule = Row::of([(
                        pen::filled(pen::columns(available), '─'),
                        Look::plain(Ink::Dim),
                    )]);
                    self.guttered(rule);
                }
            }
        }
    }

    fn open_code(&mut self, kind: CodeBlockKind, span: &str) {
        self.open_block();
        self.lead.push((String::from(BAR), Look::plain(Ink::Dim)));
        let tag = match kind {
            CodeBlockKind::Fenced(tag) => tag.trim().to_owned(),
            CodeBlockKind::Indented => String::new(),
        };
        let info = info(&tag);
        self.code = Some(Code::of(info.language));
        if !tag.is_empty() {
            self.place(info.language, FAINT_ACCENT, false);
            self.draw_name(info.file);
            self.naming = Some(self.opened);
            self.flush();
        }
        self.fence = Some(self.opened);
        self.opened = self.opened.saturating_add(1);
        if info.language == "mermaid" {
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

    fn draw_name(&mut self, file: &str) {
        let room = self.room().saturating_sub(self.used).saturating_sub(1);
        if file.is_empty() || room == 0 {
            return;
        }
        let (kept, _) = split_at(file, room);
        self.extend(" ", Look::plain(Ink::Plain));
        self.extend(&kept, FAINT_ACCENT);
    }

    fn close_code(&mut self) {
        self.fence = None;
        self.flush_if_written();
        self.code = None;
        if let Some(mermaid) = self.mermaid.take() {
            match mermaid::block(&mermaid.source, mermaid.fence) {
                mermaid::Block::Refused { caption } => self.caption(&caption),
                mermaid::Block::Picture(diagram) if self.surface.draws() => {
                    let ceiling = Ceiling::new(
                        u16::try_from(self.room()).unwrap_or(u16::MAX),
                        self.surface.rows(),
                    );
                    let measured =
                        picture::drawing(diagram.source(), diagram.svg(), ceiling, &self.surface);
                    match measured {
                        Ok(measured) => self.pictured(mermaid.at, &measured),
                        Err(refusal) => self.caption(&refusal.message()),
                    }
                }
                _ => {}
            }
        }
        self.lead.pop();
    }

    fn pictured(&mut self, at: usize, picture: &Picture) {
        let height = at.saturating_add(usize::from(picture.rows()));
        while self.rows.len() < height {
            self.rows.push(Row::of(self.spans()));
        }
        if let Some(row) = self.rows.get_mut(at) {
            *row = row.clone().pictured_over(picture.clone());
        }
    }

    fn caption(&mut self, reason: &str) {
        self.prose(reason, Look::plain(Ink::Dim));
        self.flush_if_written();
    }

    fn open_quote(&mut self, kind: Option<BlockQuoteKind>) {
        if kind.is_some() {
            self.flush_if_written();
        }
        self.open_block();
        self.lead.push((String::from(BAR), Look::plain(Ink::Dim)));
        if let Some(kind) = kind {
            self.draw_label(kind);
        }
    }

    fn draw_label(&mut self, kind: BlockQuoteKind) {
        let (name, ink) = alert(kind);
        let (kept, cut) = split_at(name, self.room_cells());
        let name = match cut.is_empty() {
            true => kept,
            false => {
                let (kept, _) = split_at(name, self.room_cells().saturating_sub(1));
                format!("{kept}…")
            }
        };
        let mut spans = self.gutter();
        spans.push((name, Look::plain(ink)));
        self.first = false;
        self.push(Row::of(spans));
        self.labelled = Some(self.rows.len());
    }

    fn open_item(&mut self) {
        self.flush_if_written();
        let marker = match self.lists.last_mut() {
            Some(Some(next)) => {
                let marker = format!("{next}. ");
                *next = next.saturating_add(1);
                marker
            }
            _ => String::from(BULLET),
        };
        self.marks.push(Mark {
            text: marker,
            ink: Ink::Accent,
        });
        self.first = true;
    }

    fn mark_task(&mut self, checked: bool) {
        let (glyph, ink) = match checked {
            true => (TICK, Ink::Ok),
            false => (BOX, Ink::Dim),
        };
        let Some(mark) = self.marks.last_mut() else {
            return;
        };
        let boxed = pen::pad(&glyph.to_string(), pen::cells(BULLET));
        let bullet = mark.text == BULLET;
        mark.text = match bullet {
            true => boxed,
            false => format!("{}{boxed}", mark.text),
        };
        mark.ink = ink;
    }

    fn close_item(&mut self) {
        self.flush_if_written();
        self.marks.pop();
        self.first = false;
    }

    fn close_image(&mut self) {
        let Some((source, alt)) = self.image.take() else {
            return;
        };
        self.flush_if_written();
        self.gap();
        let words = picture::words(&source, &alt);
        let look = Look::plain(Ink::Dim);
        let ceiling = Ceiling::new(
            u16::try_from(self.room()).unwrap_or(u16::MAX),
            self.surface.rows(),
        );
        match picture::measure(&source, ceiling, &self.surface) {
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
        self.blank = false;
        self.gap();
    }

    fn open_block(&mut self) {
        self.faces.clear();
        self.gap();
    }

    fn gap(&mut self) {
        let under_label = self.labelled.take() == Some(self.rows.len());
        if !self.rows.is_empty() && !self.blank && !under_label {
            self.push(Row::blank());
        }
    }

    fn flush(&mut self) {
        let gutter = self.columns();
        let runs = self.runs(gutter);
        let scale = usize::from(self.scale);
        if let Some(rule) = &mut self.rule {
            rule.wrote(match runs.is_empty() {
                true => self.used,
                false => self.used.saturating_mul(scale),
            });
        }
        let mut row = Row::of(self.spans());
        let source = self.source.take();
        if let Some(fence) = self.fence {
            row = row.fenced(fence, pen::columns(gutter));
            if let Some(source) = source {
                row = row.sourced(source);
            }
        }
        if let Some(fence) = self.naming.take() {
            row = row.naming(fence);
        }
        let sized = !runs.is_empty();
        for run in runs {
            row = row.sized(run);
        }
        for run in std::mem::take(&mut self.laid) {
            row = row.linking(
                u16::try_from(gutter.saturating_add(run.left.saturating_mul(scale)))
                    .unwrap_or(u16::MAX),
                u16::try_from(run.width.saturating_mul(scale)).unwrap_or(u16::MAX),
                run.destination,
            );
        }
        self.line.clear();
        self.push(row);
        if sized {
            self.first = false;
            let under = Row::of(self.gutter());
            self.push(under);
        }
        self.used = 0;
        self.spaced = false;
        self.first = false;
    }

    fn runs(&self, gutter: usize) -> Vec<Sized> {
        if self.scale == 1 {
            return Vec::new();
        }
        let room = self.room_cells();
        let mut runs: Vec<Sized> = Vec::new();
        let mut left = gutter;
        let mut used = 0usize;
        for (text, look) in &self.line {
            let run = Sized::new(pen::columns(left), self.scale, text.as_str(), *look);
            used = used.saturating_add(usize::from(run.width()));
            if used > room {
                return Vec::new();
            }
            left = left.saturating_add(usize::from(run.width()));
            runs.push(run);
        }
        runs
    }

    fn spans(&self) -> Vec<(String, Look)> {
        let mut spans = self.gutter();
        spans.extend(self.line.iter().cloned());
        spans
    }

    fn columns(&self) -> usize {
        self.gutter()
            .iter()
            .map(|(text, _)| usize::from(pen::cells(text)))
            .sum()
    }

    fn bars(&self) -> &[(String, Look)] {
        let bars = match self.lead.is_empty() {
            true => &self.base,
            false => &self.lead,
        };
        let width = bars
            .iter()
            .map(|(text, _)| pen::cells(text))
            .fold(0, u16::saturating_add);
        match self.wide > width {
            true => bars,
            false => &[],
        }
    }

    fn gutter(&self) -> Vec<(String, Look)> {
        let mut spans = self.bars().to_vec();
        for (at, mark) in self.marks.iter().enumerate() {
            let innermost = at + 1 == self.marks.len();
            let (text, ink) = match innermost && self.first {
                true => (mark.text.clone(), mark.ink),
                false => (pen::blank(pen::cells(&mark.text)), Ink::Plain),
            };
            spans.push((text, Look::plain(ink)));
        }
        spans
    }

    fn guttered(&mut self, row: Row) {
        if self.bars().is_empty() && self.marks.is_empty() {
            self.push(row);
            return;
        }
        let gutter = Row::of(self.gutter());
        self.first = false;
        self.push(gutter.followed_by(&row));
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
    /// nothing would break every word into nothing forever. A glyph drawn at more than one cell
    /// spends that many of them, so a large block has the band's columns divided by its scale.
    fn room(&self) -> usize {
        (self.room_cells() / usize::from(self.scale)).max(1)
    }

    fn room_cells(&self) -> usize {
        let bars: usize = self
            .bars()
            .iter()
            .map(|(text, _)| usize::from(pen::cells(text)))
            .sum();
        let marks: usize = self
            .marks
            .iter()
            .map(|mark| usize::from(pen::cells(&mark.text)))
            .sum();
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

    fn finish(mut self) -> Vec<Row> {
        self.flush_if_written();
        if self.rows.is_empty() {
            self.push(Row::blank());
        }
        self.foot();
        self.rows
    }

    fn foot(&mut self) {
        let definitions = self.definitions();
        if definitions.is_empty() {
            return;
        }
        self.gap();
        let mut spans = self.gutter();
        spans.push((
            pen::filled(pen::columns(self.room_cells()), HAIRLINE),
            Look::plain(Ink::Dim),
        ));
        self.push(Row::of(spans));
        for row in definitions {
            self.push(row);
        }
    }

    fn definitions(&mut self) -> Vec<Row> {
        let mut laid = Vec::new();
        let mut opened = self.opened;
        for (at, name) in self.refs.iter().enumerate() {
            let Some(note) = self.notes.iter().find(|note| note.name == *name) else {
                continue;
            };
            if self.wholeness == Wholeness::Arriving && !note.ended {
                continue;
            }
            let number = at.saturating_add(1);
            let mut room = Room::new(self.wide, &self.surface);
            room.base.clone_from(&self.base);
            room.refs.clone_from(&self.refs);
            room.opened = opened;
            room.marks.push(Mark {
                text: format!("{} ", superscript(number)),
                ink: Ink::Dim,
            });
            for (event, span) in &note.events {
                room.event(event.clone(), span);
            }
            room.flush_if_written();
            if room.rows.is_empty() {
                let gutter = room.gutter();
                room.push(Row::of(gutter));
            }
            opened = room.opened;
            laid.extend(room.rows);
        }
        self.opened = opened;
        laid
    }
}

struct Note {
    name: String,
    events: Vec<(Event<'static>, String)>,
    ended: bool,
}

struct Mermaid {
    source: String,
    fence: Fence,
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

/// Whether the blank line that ends a definition's block has arrived, read off the span the parser
/// handed over: the parse closes the block at the end of whatever text it was given, so a
/// definition still streaming arrives as the same two events as one that ended.
fn ended(span: &str) -> bool {
    span.lines()
        .next_back()
        .is_some_and(|line| line.trim().is_empty())
}

fn breaking(tag: &str) -> bool {
    let Some(inner) = tag
        .trim()
        .strip_prefix('<')
        .and_then(|tag| tag.strip_suffix('>'))
    else {
        return false;
    };
    match inner.get(..2) {
        Some(name) if name.eq_ignore_ascii_case("br") => inner[2..]
            .chars()
            .all(|glyph| glyph == '/' || glyph.is_whitespace()),
        _ => false,
    }
}

fn superscript(number: usize) -> String {
    number
        .to_string()
        .chars()
        .filter_map(|glyph| glyph.to_digit(10))
        .map(|digit| SUPERSCRIPT[digit as usize])
        .collect()
}

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

struct CodeLine {
    source: String,
    runs: Vec<(String, Look)>,
}

struct Code {
    parse: Option<(ParseState, ScopeStack)>,
    diff: bool,
}

impl Code {
    fn of(language: &str) -> Code {
        let parse = match language.is_empty() {
            true => None,
            false => syntaxes()
                .find_syntax_by_token(language)
                .map(|syntax| (ParseState::new(syntax), ScopeStack::new())),
        };
        Code {
            parse,
            diff: language == "diff",
        }
    }

    fn highlight(&mut self, text: &str) -> Vec<CodeLine> {
        let mut lines = Vec::new();
        let mut rest = text.split('\n').peekable();
        while let Some(line) = rest.next() {
            // The newline that ended the last line is not a line of its own: a block whose
            // text ends in one has no blank row after it.
            if rest.peek().is_none() && line.is_empty() {
                break;
            }
            let runs = self.line(line);
            lines.push(CodeLine {
                source: String::from(line),
                runs,
            });
        }
        lines
    }

    fn line(&mut self, line: &str) -> Vec<(String, Look)> {
        if self.diff
            && let Some(ink) = ink_of(line)
        {
            return vec![(String::from(line), Look::plain(ink))];
        }
        let Some((parse, scopes)) = &mut self.parse else {
            return guided(vec![(String::from(line), Look::plain(Ink::Plain))]);
        };
        let Ok(ops) = parse.parse_line(line, syntaxes()) else {
            return guided(vec![(String::from(line), Look::plain(Ink::Plain))]);
        };
        let mut runs: Vec<(String, Look)> = Vec::new();
        let mut at = 0;
        for (offset, op) in ops {
            let offset = offset.min(line.len());
            if offset > at && line.is_char_boundary(offset) {
                runs.push((String::from(&line[at..offset]), look_of(scopes)));
                at = offset;
            }
            // A scope operation this build cannot apply leaves the stack as it was, so the
            // line keeps the ink of the scopes that did apply.
            let _ = scopes.apply(&op);
        }
        if at < line.len() {
            runs.push((String::from(&line[at..]), look_of(scopes)));
        }
        guided(runs)
    }
}

/// The one ink a row of a `diff` fence wears for what it holds, read off the row itself so no state
/// is carried across rows: a file header is its marker and the space and name after it, so `+++`
/// and `---` are never an addition and a removal, and a row that is neither is the parse's to draw.
fn ink_of(line: &str) -> Option<Ink> {
    if line.starts_with("@@") {
        return Some(Ink::Dim);
    }
    if line.starts_with("--- ") || line.starts_with("+++ ") {
        return Some(Ink::Muted);
    }
    match line.as_bytes().first()? {
        b'+' => Some(Ink::Ok),
        b'-' => Some(Ink::Err),
        _ => None,
    }
}

fn info(tag: &str) -> Info<'_> {
    let tag = tag.trim();
    let language = tag.split_whitespace().next().unwrap_or_default();
    Info {
        language,
        file: tag[language.len()..].trim(),
    }
}

struct Info<'a> {
    language: &'a str,
    file: &'a str,
}

fn guided(runs: Vec<(String, Look)>) -> Vec<(String, Look)> {
    let mut lead = 0;
    let mut last = None;
    for (at, (text, _)) in runs.iter().enumerate() {
        let spaces = leading(text);
        if spaces > 0 {
            lead += spaces;
            last = Some((at, spaces));
        }
        if spaces < text.len() {
            break;
        }
    }
    let Some((at, spaces)) = last.filter(|_| lead >= 2) else {
        return runs;
    };
    let mut laid = Vec::with_capacity(runs.len() + 2);
    for (index, (text, look)) in runs.into_iter().enumerate() {
        if index != at {
            laid.push((text, look));
            continue;
        }
        let (before, after) = (&text[..spaces - 1], &text[spaces..]);
        if !before.is_empty() {
            laid.push((String::from(before), look));
        }
        laid.push((GUIDE.to_string(), Look::plain(Ink::Dim)));
        if !after.is_empty() {
            laid.push((String::from(after), look));
        }
    }
    laid
}

fn leading(text: &str) -> usize {
    text.bytes()
        .take_while(|byte| *byte == b' ' || *byte == b'\t')
        .count()
}

fn look_of(scopes: &ScopeStack) -> Look {
    for scope in scopes.as_slice().iter().rev() {
        if let Some((_, look)) = kinds().iter().find(|(kind, _)| kind.is_prefix_of(*scope)) {
            return *look;
        }
    }
    Look::plain(Ink::Plain)
}

#[derive(Debug, Default)]
struct Table {
    head: Vec<Cell>,
    rows: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    cell: Cell,
    heading: bool,
    aligns: Vec<Alignment>,
}

type Cell = Vec<Said>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Said {
    Words {
        text: String,
        look: Look,
        destination: Option<String>,
    },
    Break,
}

impl Table {
    fn text(&mut self, text: &str, look: Look, destination: Option<&str>) {
        if text.is_empty() {
            return;
        }
        let look = match self.heading {
            true => look.wearing(Faces::BOLD),
            false => look,
        };
        let destination = destination.map(str::to_owned);
        match self.cell.last_mut() {
            Some(Said::Words {
                text: said,
                look: worn,
                destination: way,
            }) if *worn == look && *way == destination => said.push_str(text),
            _ => self.cell.push(Said::Words {
                text: String::from(text),
                look,
                destination,
            }),
        }
    }

    fn break_line(&mut self) {
        if !self.cell.is_empty() {
            self.cell.push(Said::Break);
        }
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

    fn lines(&self, widths: &[usize]) -> Vec<Vec<Row>> {
        lines_of(&self.head, widths)
    }

    fn body(&self) -> &[Vec<Cell>] {
        &self.rows
    }
}

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

fn lines_of(cells: &[Cell], widths: &[usize]) -> Vec<Vec<Row>> {
    let wrapped: Vec<Vec<Row>> = widths
        .iter()
        .enumerate()
        .map(|(at, width)| {
            let mut room = Room::new(u16::try_from(*width).unwrap_or(u16::MAX), &Surface::none());
            for said in cells.get(at).into_iter().flatten() {
                match said {
                    Said::Words {
                        text,
                        look,
                        destination,
                    } => {
                        room.destination = destination.clone();
                        room.prose(text, *look);
                    }
                    Said::Break => room.flush_if_written(),
                }
            }
            room.destination = None;
            room.flush_if_written();
            room.rows
        })
        .collect();
    let tall = wrapped.iter().map(Vec::len).max().unwrap_or(0);
    (0..tall)
        .map(|line| {
            wrapped
                .iter()
                .map(|cell| cell.get(line).cloned().unwrap_or_else(Row::blank))
                .collect()
        })
        .collect()
}

fn joined(line: &[Row], widths: &[usize], aligns: &[Alignment]) -> Row {
    let mut row = Row::blank();
    for (at, width) in widths.iter().enumerate() {
        if at > 0 {
            row = row.followed_by(&Row::of([(String::from(" │ "), Look::plain(Ink::Dim))]));
        }
        let cell = line.get(at);
        let wrote = cell.map_or(0, Row::width);
        let last = at + 1 == widths.len();
        let spare = match last && wrote == 0 {
            true => 0,
            false => width.saturating_sub(usize::from(wrote)),
        };
        let before = match aligns.get(at).copied().unwrap_or(Alignment::None) {
            Alignment::Right => spare,
            Alignment::Center => spare / 2,
            _ => 0,
        };
        if before > 0 {
            row = row.followed_by(&blanks(before));
        }
        if let Some(cell) = cell {
            row = row.followed_by(cell);
        }
        let after = match last {
            true => 0,
            false => spare - before,
        };
        if after > 0 {
            row = row.followed_by(&blanks(after));
        }
    }
    row
}

fn blanks(cells: usize) -> Row {
    Row::of([(pen::blank(pen::columns(cells)), Look::plain(Ink::Plain))])
}

fn header_rule(widths: &[usize]) -> Row {
    let mut text = String::new();
    for (at, width) in widths.iter().enumerate() {
        if at > 0 {
            text.push_str("─┼─");
        }
        text.push_str(&pen::filled(pen::columns(*width), '─'));
    }
    Row::of([(text, Look::plain(Ink::Dim))])
}

fn measured(cell: &[Said]) -> usize {
    let mut widest = 0;
    let mut line = 0;
    for said in cell {
        match said {
            Said::Words { text, .. } => line += usize::from(pen::cells(text)),
            Said::Break => {
                widest = widest.max(line);
                line = 0;
            }
        }
    }
    widest.max(line)
}
