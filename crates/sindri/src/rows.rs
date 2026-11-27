use std::collections::BTreeSet;
use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri_core::open::{Ground, Opening};
use sindri_core::provider::Deltas;
use sindri_core::records::Entry;
use sindri_core::redact::Redacted;
use sindri_core::run::{Exit, Stop};
use sindri_core::tools::OPEN;

use crate::ink::{Ink, Look};
use crate::link::{Link, Links, Way};
use crate::mark;
use crate::markdown;
use crate::palette::{Band, Palette};
use crate::pen::{self, Clip};
use crate::picture::{self, Surface};
use crate::pulse::{self, Head};
use crate::size::{self, Sized, Sizes};
use crate::spell;
use crate::stream::{Said, Stream};

pub(crate) const INDENT: &str = "  ";

pub const MARGIN: u16 = 2;

pub(crate) const fn margins() -> u16 {
    MARGIN.saturating_mul(2)
}

pub(crate) const fn margined(band: Rect) -> Rect {
    Rect {
        x: band.x.saturating_add(MARGIN),
        width: band.width.saturating_sub(margins()),
        ..band
    }
}

pub(crate) const SIGIL: &str = "› ";

pub(crate) const SPINE: &str = "┃";

const THOUGHT: &str = "│ ";

const PENDING: char = '…';
const CHECK: char = '✓';
const CROSS: char = '✗';
const KEPT: char = '⋯';

const CUT: char = '…';

const RULE: char = '│';

const RULE_INSET: &str = "    ";

const INVOCATION_SOURCE: usize = usize::MAX;
const RESULT_SOURCE: usize = usize::MAX - 1;

const FOLD_HEAD: usize = 4;

const FOLD_KEPT: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fold {
    Closed,
    Open,
}

impl Fold {
    fn other(self) -> Fold {
        match self {
            Fold::Closed => Fold::Open,
            Fold::Open => Fold::Closed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Alone,
    Opens(Fold),
    Within,
}

impl Place {
    fn in_work(self) -> bool {
        matches!(self, Place::Opens(_) | Place::Within)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolOutcome {
    Running,
    Failed,
    Settled,
}

impl ToolOutcome {
    fn ink(self) -> Ink {
        match self {
            Self::Running => Ink::Accent,
            Self::Failed => Ink::Err,
            Self::Settled => Ink::Dim,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    spans: Vec<(String, Look)>,
    links: Vec<Link>,
    picture: Option<picture::Picture>,
    sizes: Vec<Sized>,
    fence: Option<Fence>,
    source: Option<String>,
    names: Option<usize>,
    band: Option<Band>,
    tool_width: Option<u16>,
    tool_compact: bool,
    tool_outcome: ToolOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fence {
    id: usize,
    gutter: u16,
    walk: u16,
}

impl Fence {
    pub fn id(self) -> usize {
        self.id
    }

    pub fn gutter(self) -> u16 {
        self.gutter
    }

    pub fn walk(self) -> u16 {
        self.walk
    }
}

impl Row {
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
            sizes: Vec::new(),
            fence: None,
            source: None,
            names: None,
            band: None,
            tool_width: None,
            tool_compact: false,
            tool_outcome: ToolOutcome::Settled,
        }
    }

    pub fn sized(mut self, run: Sized) -> Row {
        self.sizes.push(run);
        self
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
            sizes: Vec::new(),
            fence: None,
            source: None,
            names: None,
            band: None,
            tool_width: None,
            tool_compact: false,
            tool_outcome: ToolOutcome::Settled,
        }
    }

    pub fn standing(mut self, band: Band) -> Row {
        self.band = Some(band);
        self
    }

    pub(crate) fn tool_surface(&self) -> Option<(u16, Ink, bool)> {
        self.tool_width.map(|width| {
            let ink = self.tool_outcome.ink();
            (width, ink, self.tool_compact)
        })
    }

    pub fn stands_on(&self) -> Option<Band> {
        self.band
    }

    pub fn fenced(mut self, id: usize, gutter: u16) -> Row {
        self.fence = Some(Fence {
            id,
            gutter,
            walk: 0,
        });
        self
    }

    pub fn fence(&self) -> Option<Fence> {
        self.fence
    }

    pub fn sourced(mut self, source: impl Into<String>) -> Row {
        self.source = Some(source.into());
        self
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn naming(mut self, fence: usize) -> Row {
        self.names = Some(fence);
        self
    }

    pub fn fence_id(&self) -> Option<usize> {
        self.fence.map(Fence::id).or(self.names)
    }

    pub fn line_of(&self, id: usize) -> bool {
        self.fence.is_some_and(|line| line.id() == id)
    }

    pub(crate) fn walked(&mut self, by: u16) {
        if let Some(fence) = &mut self.fence {
            fence.walk = by;
        }
    }

    pub fn cut(&self, room: u16) -> bool {
        self.fence
            .is_some_and(|fence| u32::from(fence.walk) + u32::from(room) < u32::from(self.width()))
    }

    pub fn linking(mut self, left: u16, width: u16, destination: impl Into<String>) -> Row {
        self.links.push(Link::new(left, width, destination));
        self
    }

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

    fn shifted(self, by: u16) -> Row {
        Row {
            spans: self.spans,
            links: self.links.iter().map(|link| link.shifted(by)).collect(),
            sizes: self.sizes.iter().map(|run| run.shifted(by)).collect(),
            picture: self.picture,
            fence: self.fence,
            source: self.source,
            names: self.names,
            band: self.band,
            tool_width: self.tool_width,
            tool_compact: self.tool_compact,
            tool_outcome: self.tool_outcome,
        }
    }

    pub fn guttered(self, gutter: &str, ink: Ink) -> Row {
        if self.spans.is_empty() {
            return self;
        }
        let mut row = self.shifted(pen::cells(gutter));
        if !gutter.is_empty() {
            row.spans
                .insert(0, (String::from(gutter), Look::plain(ink)));
        }
        row
    }

    pub(crate) fn followed_by(self, other: &Row) -> Row {
        let shifts = self.width();
        let mut moved = other.clone().shifted(shifts);
        let mut row = self;
        row.spans.append(&mut moved.spans);
        row.links.append(&mut moved.links);
        row.sizes.append(&mut moved.sizes);
        row
    }

    pub(crate) fn headed_by(self, head: &Row, by: u16) -> Row {
        let mut row = self.shifted(by);
        let mut spans = head.spans.clone();
        spans.push((String::from(" "), Look::plain(Ink::Plain)));
        spans.extend(row.spans.split_off(row.spans.len().min(1)));
        row.spans = spans;
        row
    }

    pub fn glyphs(&self) -> String {
        self.spans.iter().map(|(text, _)| text.as_str()).collect()
    }

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
            let surface = match band {
                Band::Prompt => Rect {
                    width: area.width.saturating_sub(MARGIN),
                    ..area
                },
                _ => area,
            };
            pen::claim(buffer, surface, palette.ground(Some(band)));
        }
        match self.fence.filter(|fence| fence.walk > 0) {
            Some(fence) => {
                let (gutter, words) = gutter_and_words(&self.spans, fence.gutter);
                let mut spans = gutter;
                spans.extend(walked(&words, fence.walk));
                pen::write(buffer, area, &spans, Clip::Edge, palette);
            }
            None => pen::write(buffer, area, &self.spans, Clip::Edge, palette),
        }
        if self.tool_width.is_some() && area.x.saturating_add(MARGIN) < area.right() {
            pen::glyph(buffer, area.x + MARGIN, area.y, RULE, Ink::Dim, palette);
        }
        if area.width > 0 && self.cut(area.width) {
            pen::glyph(
                buffer,
                area.right().saturating_sub(1),
                area.y,
                CUT,
                Ink::Dim,
                palette,
            );
        }
        links.lay(area.y, area.x, &self.links);
        sizes.lay(area.y, area.x, &self.sizes);
    }
}

type Spans = Vec<(String, Look)>;

fn gutter_and_words(spans: &[(String, Look)], columns: u16) -> (Spans, Spans) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut at = 0u16;
    for (text, look) in spans {
        let room = columns.saturating_sub(at);
        if room == 0 {
            right.push((text.clone(), *look));
            continue;
        }
        let width = pen::cells(text);
        if width <= room {
            left.push((text.clone(), *look));
            at = at.saturating_add(width);
            continue;
        }
        let (head, tail) = split_at(text, usize::from(room));
        left.push((head, *look));
        right.push((String::from(tail), *look));
        at = columns;
    }
    (left, right)
}

fn walked(spans: &[(String, Look)], columns: u16) -> Spans {
    let mut past = columns;
    let mut kept = Vec::new();
    for (text, look) in spans {
        let (dropped, rest) = drop_columns(text, past);
        past = past.saturating_sub(dropped);
        if !rest.is_empty() {
            kept.push((String::from(rest), *look));
        }
    }
    kept
}

fn drop_columns(text: &str, columns: u16) -> (u16, &str) {
    let mut at = 0u16;
    for (index, glyph) in text.char_indices() {
        if at >= columns {
            return (at, &text[index..]);
        }
        at = at.saturating_add(pen::cell(glyph));
    }
    (at, "")
}

pub fn layout(entry: &Entry, cols: u16) -> Vec<Row> {
    layout_on(entry, cols, &Surface::none(), None)
}

pub fn layout_on(entry: &Entry, cols: u16, surface: &Surface, ground: Option<&Ground>) -> Vec<Row> {
    laid(entry, cols, INDENT, surface, ground, Fold::Closed, None).0
}

fn laid(
    entry: &Entry,
    cols: u16,
    gutter: &str,
    surface: &Surface,
    ground: Option<&Ground>,
    fold: Fold,
    head: Option<&Head>,
) -> (Vec<Row>, Option<usize>) {
    let cols = cols.max(1);
    match entry {
        Entry::Turn { .. } | Entry::Stop { stop: Stop::Final } => (Vec::new(), None),
        Entry::Call {
            name, arguments, ..
        } => (
            vec![call_row(
                &name.to_string(),
                &arguments.to_string(),
                None,
                None,
                None,
                cols,
                ground,
            )],
            None,
        ),
        Entry::Result {
            name,
            text,
            bytes,
            elapsed_ms,
            exit,
            ..
        } => folded(
            result_rows(
                Some(&name.to_string()),
                *exit,
                &text.to_string(),
                *bytes,
                *elapsed_ms,
                cols,
                gutter,
            ),
            cols,
            fold,
            exit.is_some_and(|exit| !exit.is_ok()),
        ),
        Entry::Assistant { text, .. } => (
            answer_on(
                &text.to_string(),
                cols,
                head,
                surface,
                markdown::Wholeness::Whole,
            ),
            None,
        ),
        Entry::Reasoning { text, .. } => (thought(&text.to_string(), cols), None),
        Entry::Prompt { text } => {
            let (lead, words) = asked(&text.to_string(), cols);
            let (words, kept) = folded_prompt(words, cols, fold);
            let mut rows = vec![lead];
            rows.extend(words);
            (rows, kept)
        }
        Entry::Stop { stop } => (stopped(stop, cols), None),
    }
}

fn folded(mut rows: Vec<Row>, cols: u16, fold: Fold, failed: bool) -> (Vec<Row>, Option<usize>) {
    let Some(kept) = rows
        .len()
        .checked_sub(FOLD_HEAD)
        .filter(|kept| *kept > FOLD_KEPT)
    else {
        return (rows, None);
    };
    if fold == Fold::Open {
        return (rows, Some(kept));
    }
    let diagnostic = failed
        .then(|| {
            rows.iter()
                .rposition(|row| row.source().is_some_and(|text| !text.trim().is_empty()))
        })
        .flatten()
        .filter(|at| *at >= FOLD_HEAD)
        .map(|at| rows[at].clone());
    rows.truncate(FOLD_HEAD);
    if let Some(diagnostic) = diagnostic {
        rows[FOLD_HEAD - 1] = diagnostic;
    }
    rows.push(kept_back(kept, cols).naming(RESULT_SOURCE));
    (rows, Some(kept))
}

fn folded_prompt(mut words: Vec<Row>, cols: u16, fold: Fold) -> (Vec<Row>, Option<usize>) {
    let Some(kept) = words
        .len()
        .checked_sub(FOLD_HEAD)
        .filter(|kept| *kept > FOLD_KEPT)
    else {
        return (words, None);
    };
    if fold == Fold::Open {
        return (words, Some(kept));
    }
    words.truncate(FOLD_HEAD);
    words.push(kept_back(kept, cols).standing(Band::Prompt));
    (words, Some(kept))
}

fn kept_back(kept: usize, cols: u16) -> Row {
    let words = format!("{kept} rows kept back");
    marked(&KEPT.to_string(), Ink::Dim, vec![(words, Ink::Dim)], cols)
}

#[derive(Debug, Default)]
struct Tally {
    calls: usize,
    files: BTreeSet<String>,
    elapsed_ms: u64,
}

impl Tally {
    fn words(&self) -> String {
        let mut said: Vec<String> = Vec::new();
        if self.calls > 0 {
            said.push(format!("{} call{}", self.calls, plural(self.calls)));
        }
        if !self.files.is_empty() {
            said.push(format!(
                "{} file{}",
                self.files.len(),
                plural(self.files.len())
            ));
        }
        if self.elapsed_ms > 0 {
            said.push(took(self.elapsed_ms));
        }
        said.join(" · ")
    }
}

fn took(ms: u64) -> String {
    match ms {
        ms if ms < 1_000 => format!("{ms} ms"),
        ms => pulse::format_duration_spaced(ms),
    }
}

fn plural(count: usize) -> &'static str {
    match count {
        1 => "",
        _ => "s",
    }
}

fn stopped(stop: &Stop, cols: u16) -> Vec<Row> {
    let (marker, ink) = stop_mark(stop);
    let mut rows = vec![marked(
        &marker.to_string(),
        ink,
        vec![(String::from(stop.word()), Ink::Dim)],
        cols,
    )];
    if let Stop::Error(message) = stop {
        let said = message.to_string();
        let line = said.lines().next().unwrap_or_default();
        rows.extend(body(line, cols, INDENT, None, Ink::Muted));
    }
    rows
}

fn asked(text: &str, cols: u16) -> (Row, Vec<Row>) {
    let opening = format!("{SPINE}{}", SIGIL.trim_end());
    let (gutter, room) = margin(&opening, cols);
    let words = wrap(text, room)
        .into_iter()
        .enumerate()
        .map(|(at, line)| quoted(at == 0 && !gutter.is_empty(), line, cols))
        .collect();
    (Row::blank(), words)
}

fn quoted(opens: bool, line: String, cols: u16) -> Row {
    let marker = match opens {
        true => SIGIL.trim_end(),
        false => " ",
    };
    let gutter = pen::cells(SPINE).saturating_add(pen::cells(marker));
    let row = match gutter.saturating_add(pen::cells(&line)) > cols {
        true => Row::of([(line, Ink::Plain)]),
        false => Row::of([
            (String::from(SPINE), Ink::Dim),
            (String::from(marker), Ink::Plain),
            (line, Ink::Plain),
        ]),
    };
    row.standing(Band::Prompt)
}

fn thought(text: &str, cols: u16) -> Vec<Row> {
    let (gutter, room) = margin(THOUGHT, cols);
    wrap(text, room)
        .into_iter()
        .map(|line| words_past(gutter, Ink::Dim, Ink::Muted, line, cols))
        .collect()
}

fn answer_on(
    text: &str,
    cols: u16,
    head: Option<&Head>,
    surface: &Surface,
    wholeness: markdown::Wholeness,
) -> Vec<Row> {
    let (head, reserve) = reserved(head, cols);
    let mut rows = answer_rows(text, cols, reserve, surface, wholeness);
    if let Some(head) = head
        && let Some(first) = rows.first_mut()
    {
        headed(first, head, cols, reserve);
    }
    rows
}

fn headed(row: &mut Row, head: &Head, cols: u16, reserve: u16) {
    let first = std::mem::replace(row, Row::blank());
    *row = first.headed_by(&head_row(head, cols), reserve);
}

fn reserved(head: Option<&Head>, cols: u16) -> (Option<&Head>, u16) {
    let (gutter, room) = margin(INDENT, cols);
    let reserve = head.map_or(0, |head| {
        head.width()
            .saturating_add(1)
            .saturating_sub(pen::cells(gutter))
    });
    match reserve < room {
        true => (head, reserve),
        false => (None, 0),
    }
}

fn answer_rows(
    text: &str,
    cols: u16,
    reserve: u16,
    surface: &Surface,
    wholeness: markdown::Wholeness,
) -> Vec<Row> {
    let (gutter, _) = margin(INDENT, cols);
    markdown::rows_in(text, cols, reserve, gutter, surface, wholeness)
}

fn head_row(head: &Head, cols: u16) -> Row {
    let Some(((glyph, ink), words)) = head.spans().split_first() else {
        return Row::blank();
    };
    let words = words
        .iter()
        .map(|(text, ink)| (String::from(text.trim_start()), *ink))
        .collect();
    marked(glyph, *ink, words, cols)
}

fn marked(marker: &str, ink: Ink, words: Vec<(String, Ink)>, cols: u16) -> Row {
    let (gutter, room) = margin(INDENT, cols);
    let words = fitted(words, room);
    match gutter.is_empty() {
        true => words,
        false => Row::of([(String::from(marker), ink), (String::from(" "), Ink::Plain)])
            .followed_by(&words),
    }
}

fn margin(gutter: &str, cols: u16) -> (&str, u16) {
    let width = pen::cells(gutter);
    match cols > width {
        true => (gutter, cols - width),
        false => ("", cols),
    }
}

fn opening(
    name: &str,
    arguments: &str,
    cols: u16,
    gutter: &str,
    ground: Option<&Ground>,
    exit: Option<Exit>,
) -> Vec<Row> {
    let words = spell::of(name, arguments);
    let mut rows = tool_head(name, exit, None, cols);
    let continuation = if cols > 16 { "  " } else { "" };
    let mut invocation = match way(name, arguments, &words, ground) {
        Some(way) => body(&words, cols, gutter, Some(way), Ink::Plain),
        None => wrap_under(
            &words,
            margin(gutter, cols).1,
            margin(gutter, cols)
                .1
                .saturating_sub(pen::cells(continuation)),
        )
        .into_iter()
        .enumerate()
        .map(|(at, line)| {
            let inset = if at == 0 { "" } else { continuation };
            words_past(
                margin(gutter, cols).0,
                Ink::Plain,
                Ink::Plain,
                format!("{inset}{line}"),
                cols,
            )
        })
        .collect(),
    };
    for row in &mut invocation {
        *row = row
            .clone()
            .fenced(INVOCATION_SOURCE, pen::cells(margin(gutter, cols).0))
            .sourced("");
    }
    if let Some(row) = invocation.first_mut() {
        *row = row.clone().sourced(spell::source(name, arguments));
    }
    rows.extend(invocation);
    rows
}

fn way(name: &str, arguments: &str, written: &str, ground: Option<&Ground>) -> Option<Way> {
    if name != OPEN || written.is_empty() {
        return None;
    }
    match ground?.judge(arguments).ok()? {
        Opening::File(path) => Some(Way::of(written, &path)),
        Opening::Url(_) => None,
    }
}

fn result_rows(
    name: Option<&str>,
    exit: Option<Exit>,
    text: &str,
    bytes: u64,
    elapsed_ms: u64,
    cols: u16,
    gutter: &str,
) -> Vec<Row> {
    let mut rows = name.map_or_else(Vec::new, |name| {
        tool_head(name, exit, Some(elapsed_ms), cols)
    });
    let gutter = margin(gutter, cols).0;
    rows.extend(text.split('\n').filter(|_| !text.is_empty()).map(|line| {
        Row::of([
            (String::from(gutter), Ink::Muted),
            (String::from(line), Ink::Muted),
        ])
        .fenced(RESULT_SOURCE, pen::cells(gutter))
        .sourced(line)
    }));
    let detail = match exit {
        Some(exit) => format!("{} · {bytes} bytes", exit.word()),
        None => format!("{bytes} bytes"),
    };
    rows.extend(body(&detail, cols, gutter, None, Ink::Dim));
    rows
}

fn call_row(
    name: &str,
    arguments: &str,
    exit: Option<Exit>,
    elapsed_ms: Option<u64>,
    kept: Option<usize>,
    cols: u16,
    ground: Option<&Ground>,
) -> Row {
    let (marker, ink) = match (exit, elapsed_ms) {
        (Some(exit), _) if exit.is_ok() => (CHECK, Ink::Ok),
        (Some(_), _) => (CROSS, Ink::Err),
        (None, Some(_)) => (PENDING, Ink::Dim),
        (None, None) => (PENDING, Ink::Dim),
    };
    let outcome = match (exit, elapsed_ms) {
        (Some(exit), _) if !exit.is_ok() => format!("{} ", exit.word()),
        (None, None) => String::from("running "),
        _ => String::new(),
    };
    let command = spell::of(name, arguments)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let count = kept.map_or_else(String::new, |kept| format!(" {KEPT} {kept} rows kept back"));
    let mut tool = format!("{name} ");
    let mut duration = elapsed_ms.map_or_else(String::new, |ms| format!(" {}", took(ms)));
    let room = margin(INDENT, cols).1;
    let width = |tool: &str, duration: &str| {
        pen::cells(&format!("{tool}{outcome}{command}{duration}{count}"))
    };
    if name == "bash" && width(&tool, &duration) > room {
        tool.clear();
    }
    if width(&tool, &duration) > room {
        duration.clear();
    }
    let fixed = pen::cells(&format!("{tool}{outcome}{duration}{count}"));
    let allowance = room.saturating_sub(fixed);
    let command = if pen::cells(&command) > allowance {
        let (head, _) = split_at(&command, usize::from(allowance.saturating_sub(1)));
        if allowance == 0 {
            String::new()
        } else {
            format!("{head}{CUT}")
        }
    } else {
        command
    };
    let row = Row::of([
        (format!("{marker} "), ink),
        (format!("{tool}{outcome}"), Ink::Dim),
        (command.clone(), Ink::Plain),
        (format!("{duration}{count}"), Ink::Dim),
    ])
    .fenced(INVOCATION_SOURCE, 2)
    .sourced(spell::source(name, arguments));
    match way(name, arguments, &command, ground) {
        Some(way) => row.linking(
            2 + pen::cells(&format!("{tool}{outcome}")),
            pen::cells(&command),
            way.destination(),
        ),
        None => row,
    }
}

fn tool_head(name: &str, exit: Option<Exit>, elapsed_ms: Option<u64>, cols: u16) -> Vec<Row> {
    let (marker, ink, outcome) = match (exit, elapsed_ms) {
        (Some(exit), _) if exit.is_ok() => (CHECK, Ink::Ok, String::new()),
        (Some(exit), _) => (CROSS, Ink::Err, exit.word().to_owned()),
        (None, Some(_)) => (PENDING, Ink::Dim, String::new()),
        (None, None) => (PENDING, Ink::Dim, String::from("running")),
    };
    let words = format!("{name} {outcome}");
    let duration = elapsed_ms
        .map(|ms| format!(" {}", took(ms)))
        .filter(|duration| {
            pen::cells(&words)
                .saturating_add(pen::cells(duration))
                .saturating_add(2)
                <= cols
        });
    let room = margin(INDENT, cols).1;
    wrap(&words, room)
        .into_iter()
        .enumerate()
        .map(|(at, line)| {
            if at == 0 {
                let mut spans = vec![(line, Ink::Plain)];
                if let Some(duration) = &duration {
                    spans.push((duration.clone(), Ink::Dim));
                }
                marked(&marker.to_string(), ink, spans, cols)
            } else {
                words_past(margin(INDENT, cols).0, Ink::Plain, Ink::Plain, line, cols)
            }
        })
        .collect()
}

fn answered(
    call: &Entry,
    result: &Entry,
    cols: u16,
    gutter: &str,
    surface: &Surface,
    ground: Option<&Ground>,
    fold: Fold,
) -> (Vec<Row>, Option<usize>) {
    let cols = cols.max(1);
    let (
        Entry::Call {
            name, arguments, ..
        },
        Entry::Result {
            text,
            bytes,
            elapsed_ms,
            exit,
            ..
        },
    ) = (call, result)
    else {
        let (mut rows, _) = laid(call, cols, gutter, surface, ground, fold, None);
        let (under, kept) = laid(result, cols, gutter, surface, ground, fold, None);
        rows.extend(under);
        return (rows, kept);
    };
    if fold == Fold::Closed {
        let output = result_rows(
            None,
            *exit,
            &text.to_string(),
            *bytes,
            *elapsed_ms,
            cols,
            gutter,
        );
        let diagnostic = exit
            .is_some_and(|exit| !exit.is_ok())
            .then(|| {
                output
                    .iter()
                    .rfind(|row| row.source().is_some_and(|text| !text.trim().is_empty()))
                    .cloned()
            })
            .flatten();
        let invocation = opening(
            &name.to_string(),
            &arguments.to_string(),
            cols,
            gutter,
            ground,
            *exit,
        );
        let kept =
            invocation.len().saturating_sub(1) + output.len() - usize::from(diagnostic.is_some());
        let mut rows = vec![call_row(
            &name.to_string(),
            &arguments.to_string(),
            *exit,
            Some(*elapsed_ms),
            Some(kept),
            cols,
            ground,
        )];
        rows.extend(diagnostic);
        return (rows, Some(kept));
    }
    let invocation = opening(
        &name.to_string(),
        &arguments.to_string(),
        cols,
        gutter,
        ground,
        *exit,
    );
    let mut rows = tool_head(&name.to_string(), *exit, Some(*elapsed_ms), cols);
    let live_head = tool_head(&name.to_string(), *exit, None, cols).len();
    rows.extend(invocation.into_iter().skip(live_head));
    let (under, kept) = folded(
        result_rows(
            None,
            *exit,
            &text.to_string(),
            *bytes,
            *elapsed_ms,
            cols,
            gutter,
        ),
        cols,
        Fold::Closed,
        exit.is_some_and(|exit| !exit.is_ok()),
    );
    rows.extend(under);
    if kept.is_some() {
        let detail = match exit {
            Some(exit) => format!("{} · {bytes} bytes", exit.word()),
            None => format!("{bytes} bytes"),
        };
        rows.extend(body(&detail, cols, gutter, None, Ink::Dim));
    }
    (rows, kept)
}

pub(crate) fn stop_mark(stop: &Stop) -> (char, Ink) {
    match stop {
        Stop::Final => (CHECK, Ink::Ok),
        Stop::MaxTurns | Stop::Budget(_) | Stop::Cancelled | Stop::Error(_) => (CROSS, Ink::Err),
    }
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

fn body(text: &str, cols: u16, gutter: &str, way: Option<Way>, ink: Ink) -> Vec<Row> {
    if text.is_empty() {
        return Vec::new();
    }
    let (gutter, room) = margin(gutter, cols);
    let named = way.and_then(|way| markdown::body(text, room, &way));
    match named {
        Some(rows) => rows
            .into_iter()
            .map(|row| row.guttered(gutter, ink))
            .collect(),
        None => wrap(text, room)
            .into_iter()
            .map(|line| words_past(gutter, ink, ink, line, cols))
            .collect(),
    }
}

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
    pub fn back(self) -> u32 {
        match self {
            Scroll::Tail => 0,
            Scroll::Back(rows) => rows,
        }
    }

    pub fn of(rows: u32) -> Scroll {
        match rows {
            0 => Scroll::Tail,
            rows => Scroll::Back(rows),
        }
    }

    pub fn is_tail(self) -> bool {
        self == Scroll::Tail
    }
}

pub(crate) const THUMB: char = '█';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thumb {
    top: u16,
    span: u16,
}

impl Thumb {
    pub fn span(&self) -> u16 {
        self.span
    }

    pub fn rows(&self) -> Range<u16> {
        self.top..self.top + self.span
    }

    pub(crate) fn paint(&self, area: Rect, y: u16, buffer: &mut Buffer, palette: &Palette) {
        if let Some(row) = y.checked_sub(area.y)
            && self.rows().contains(&row)
        {
            pen::glyph(buffer, area.x, y, THUMB, Ink::Dim, palette);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sideways {
    Back,
    Forward,
}

const WALK: u16 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Measure {
    cols: u16,
    surface: Surface,
    ground: Option<Ground>,
}

impl Default for Measure {
    fn default() -> Measure {
        Measure {
            cols: 1,
            surface: Surface::none(),
            ground: None,
        }
    }
}

impl Measure {
    pub fn new(cols: u16, surface: Surface, ground: Option<&Ground>) -> Measure {
        Measure {
            cols: cols.max(1),
            surface,
            ground: ground.cloned(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Rows {
    measure: Measure,
    held: Vec<Held>,
    in_flight: Stream,
    lays: usize,
    laid: usize,
    ended: bool,
    head: Option<Head>,
    unmarked: Option<Unmarked>,
}

#[derive(Clone, Debug)]
struct Unmarked {
    reserve: u16,
    first: usize,
    row: Row,
}

#[derive(Clone, Debug)]
struct Held {
    what: What,
    result: Option<Entry>,
    rows: Option<Vec<Row>>,
    place: Place,
    fold: Fold,
    kept: Option<usize>,
    walks: Vec<(usize, u16)>,
}

impl Held {
    fn walk(&self, row: &mut Row) {
        let Some(fence) = row.fence() else {
            return;
        };
        if let Some((_, walk)) = self.walks.iter().find(|(id, _)| *id == fence.id()) {
            row.walked(*walk);
        }
    }
}

#[derive(Clone, Debug)]
enum What {
    Fact(Entry),
    Live { revision: u64, head: Option<Head> },
}

impl What {
    fn is_tool(&self) -> bool {
        matches!(self, Self::Fact(Entry::Call { .. } | Entry::Result { .. }))
    }

    fn is_prose(&self) -> bool {
        matches!(
            self,
            Self::Fact(Entry::Assistant { .. } | Entry::Reasoning { .. })
        )
    }
}

impl Rows {
    pub fn new() -> Rows {
        Rows::default()
    }

    pub(crate) fn stopped(&mut self, stop: Stop) {
        let drawn = self
            .held
            .iter()
            .rev()
            .find_map(|held| match &held.what {
                What::Fact(entry) => Some(entry),
                What::Live { .. } => None,
            })
            .is_some_and(|entry| matches!(entry, Entry::Stop { stop: drawn } if *drawn == stop));
        if !drawn {
            self.push(Entry::Stop { stop });
        }
        self.ended = true;
    }

    pub fn heading(&mut self, head: Option<Head>) {
        if self.head == head {
            return;
        }
        self.head = head;
        if let Some(at) = self.newest_answer() {
            self.held[at].rows = None;
        }
    }

    fn newest_answer(&self) -> Option<usize> {
        self.held
            .iter()
            .rposition(|held| matches!(held.what, What::Fact(Entry::Assistant { .. })))
    }

    pub(crate) fn holds_answer(&self) -> bool {
        self.newest_answer().is_some()
    }

    pub fn push(&mut self, entry: Entry) {
        let answer = matches!(entry, Entry::Assistant { .. });
        if self.head.is_some()
            && answer
            && let Some(at) = self.newest_answer()
        {
            self.held[at].rows = None;
        }
        if let Entry::Result { turn, id, .. } = &entry
            && let Some(at) = self.pending(*turn, id.as_ref())
        {
            self.settle(at, entry);
            return;
        }
        if answer {
            self.retire();
        } else {
            self.drop_live();
        }
        let place = self.land(&entry);
        self.held.push(Held {
            what: What::Fact(entry),
            result: None,
            rows: None,
            place,
            fold: Fold::Closed,
            kept: None,
            walks: Vec::new(),
        });
    }

    fn land(&mut self, entry: &Entry) -> Place {
        let of_work = work_entry(entry);
        if of_work {
            self.past();
        }
        let within = self.open_work().is_some();
        if matches!(entry, Entry::Prompt { .. } | Entry::Stop { .. })
            && let Some(at) = self.last_work()
        {
            self.held[at].rows = None;
        }
        match (of_work, within) {
            (false, _) => Place::Alone,
            (true, true) => Place::Within,
            (true, false) => Place::Opens(Fold::Closed),
        }
    }

    fn last_work(&self) -> Option<usize> {
        (0..self.held.len())
            .rev()
            .find(|at| matches!(self.held[*at].place, Place::Opens(_)))
    }

    fn open_work(&self) -> Option<usize> {
        let last = self.held.len().checked_sub(1)?;
        if !self.held[last].place.in_work() {
            return None;
        }
        (0..=last)
            .rev()
            .find(|at| matches!(self.held[*at].place, Place::Opens(_)))
    }

    fn past(&mut self) {
        let Some(at) = self.held.len().checked_sub(1) else {
            return;
        };
        if self.held[at].place != Place::Alone
            || !matches!(self.held[at].what, What::Fact(Entry::Assistant { .. }))
        {
            return;
        }
        let above = at
            .checked_sub(1)
            .is_some_and(|above| self.held[above].place.in_work());
        self.held[at].place = match above {
            true => Place::Within,
            false => Place::Opens(Fold::Closed),
        };
        self.held[at].rows = None;
    }

    pub fn live(&mut self, head: Option<&Head>) {
        if self.in_flight.is_empty() && head.is_none() {
            self.drop_live();
            return;
        }
        let revision = self.in_flight.revision();
        let head = head.cloned();
        let at = self.live_at();
        if let Some(held) = at.map(|at| &self.held[at])
            && let What::Live {
                revision: laid,
                head: mark,
            } = &held.what
            && held.rows.is_some()
            && *laid == revision
            && *mark == head
        {
            return;
        }
        if let Some(at) = at
            && self.stepped(at, revision, head.clone())
        {
            return;
        }
        let mut rows = self.live_rows(head.as_ref());
        match at {
            Some(at) => {
                self.walking(at, &mut rows);
                let held = &mut self.held[at];
                held.what = What::Live { revision, head };
                held.rows = Some(rows);
            }
            None => {
                let held = Held {
                    what: What::Live { revision, head },
                    result: None,
                    rows: Some(rows),
                    place: Place::Alone,
                    fold: Fold::Closed,
                    kept: None,
                    walks: Vec::new(),
                };
                match self.ended {
                    true => self.held.insert(self.held.len() - 1, held),
                    false => self.held.push(held),
                }
                self.ended = false;
            }
        }
        self.lays += 1;
    }

    fn stepped(&mut self, at: usize, revision: u64, head: Option<Head>) -> bool {
        let cols = self.measure.cols;
        let (fitted, reserve) = reserved(head.as_ref(), cols);
        let What::Live { revision: laid, .. } = &self.held[at].what else {
            return false;
        };
        if *laid != revision {
            return false;
        }
        let Some(unmarked) = self
            .unmarked
            .as_ref()
            .filter(|unmarked| unmarked.reserve == reserve)
        else {
            return false;
        };
        let mut row = unmarked.row.clone();
        if let Some(head) = fitted {
            headed(&mut row, head, cols, reserve);
        }
        let held = &mut self.held[at];
        held.walk(&mut row);
        let Some(stand) = held
            .rows
            .as_mut()
            .and_then(|rows| rows.get_mut(unmarked.first))
        else {
            return false;
        };
        *stand = row;
        held.what = What::Live { revision, head };
        self.lays += 1;
        self.laid += 1;
        true
    }

    fn live_rows(&mut self, head: Option<&Head>) -> Vec<Row> {
        let cols = self.measure.cols;
        let surface = &self.measure.surface;
        let (head, reserve) = reserved(head, cols);
        let newest = self
            .in_flight
            .said()
            .enumerate()
            .filter(|(_, (_, said))| *said == Said::Answer)
            .map(|(at, _)| at)
            .last();
        let mut unmarked = None;
        let mut rows: Vec<Row> = Vec::new();
        for (at, (words, said)) in self.in_flight.said().enumerate() {
            let heads = Some(at) == newest;
            let reserve = match heads {
                true => reserve,
                false => 0,
            };
            let mut drawn = match said {
                Said::Reasoning => thought(words, cols),
                Said::Answer => {
                    answer_rows(words, cols, reserve, surface, markdown::Wholeness::Arriving)
                }
            };
            if heads && let Some(row) = drawn.first().cloned() {
                unmarked = Some(Unmarked {
                    reserve,
                    first: rows.len(),
                    row,
                });
                if let Some(head) = head
                    && let Some(first) = drawn.first_mut()
                {
                    headed(first, head, cols, reserve);
                }
            }
            rows.extend(drawn);
        }
        if let (None, Some(head)) = (newest, head) {
            rows.push(head_row(head, cols));
        }
        self.laid += rows.len();
        self.unmarked = unmarked;
        rows
    }

    pub(crate) fn says_answer(&self) -> bool {
        self.in_flight.says_answer()
    }

    fn live_at(&self) -> Option<usize> {
        self.held
            .iter()
            .position(|held| matches!(held.what, What::Live { .. }))
    }

    pub(crate) fn retire(&mut self) {
        self.in_flight = Stream::new();
        self.drop_live();
    }

    fn drop_live(&mut self) {
        self.held
            .retain(|held| !matches!(held.what, What::Live { .. }));
        self.ended = false;
        self.unmarked = None;
    }

    fn pending(&self, turn: u32, id: Option<&Redacted>) -> Option<usize> {
        let mut waiting = self
            .held
            .iter()
            .enumerate()
            .filter_map(|(at, held)| match &held.what {
                What::Fact(Entry::Call {
                    turn: of,
                    id: named,
                    ..
                }) if held.result.is_none() && *of == turn => Some((at, named.as_ref())),
                _ => None,
            });
        match id {
            Some(id) => waiting
                .find(|(_, named)| *named == Some(id))
                .map(|(at, _)| at),
            None => waiting.next().map(|(at, _)| at),
        }
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

    pub fn lays(&self) -> usize {
        self.lays
    }

    pub fn laid(&self) -> usize {
        self.laid
    }

    pub fn measure(&mut self, measure: Measure) -> bool {
        if measure == self.measure {
            return false;
        }
        let reflowed = measure.cols != self.measure.cols;
        self.measure = measure;
        self.unlay();
        reflowed
    }

    pub fn sideways(&mut self, at: usize, fence: usize, way: Sideways) -> bool {
        let cols = self.measure.cols;
        let Some(held) = self.held.get_mut(at) else {
            return false;
        };
        let Some(rows) = held.rows.as_ref() else {
            return false;
        };
        let widest = rows
            .iter()
            .filter(|row| row.line_of(fence))
            .map(Row::width)
            .max();
        let Some(widest) = widest else {
            return false;
        };
        let walk = held
            .walks
            .iter()
            .find(|(id, _)| *id == fence)
            .map_or(0, |(_, walk)| *walk);
        let most = widest.saturating_sub(cols);
        let to = match way {
            Sideways::Back => walk.saturating_sub(WALK),
            Sideways::Forward => walk.saturating_add(WALK).min(most),
        };
        if to == walk {
            return false;
        }
        if let Some(rows) = held.rows.as_mut() {
            for row in rows {
                if row.line_of(fence) {
                    row.walked(to);
                }
            }
        }
        match held.walks.iter_mut().find(|(id, _)| *id == fence) {
            Some((_, walk)) => *walk = to,
            None => held.walks.push((fence, to)),
        }
        true
    }

    fn walking(&self, at: usize, rows: &mut [Row]) {
        let held = &self.held[at];
        for row in rows {
            held.walk(row);
        }
    }

    fn unlay(&mut self) {
        for held in &mut self.held {
            held.rows = None;
            held.walks.clear();
        }
        self.unmarked = None;
    }

    pub fn rows(&mut self, at: usize) -> &[Row] {
        let work = self.work(at);
        let one = self.one_row(&work);
        if one && at != work.start {
            return &[];
        }
        let tool = self.held[at].what.is_tool() && !one;
        let ruled = tool && self.measure.cols > 16;
        let rule = pen::cells(RULE_INSET);
        let gutter = match ruled {
            true => "",
            false => INDENT,
        };
        let cols = self.measure.cols.saturating_sub(match ruled {
            true => rule,
            false => 0,
        });
        let surface = &self.measure.surface;
        let ground = self.measure.ground.as_ref();
        let held = &self.held[at];
        if held.rows.is_none() {
            let laid = match (&held.what, held.result.as_ref()) {
                _ if one => Some((vec![self.work_row(&work, cols)], None)),
                (What::Fact(entry), None) => {
                    let head = match self.newest_answer() == Some(at) {
                        true => self.head.as_ref(),
                        false => None,
                    };
                    Some(laid(entry, cols, gutter, surface, ground, held.fold, head))
                }
                (What::Fact(call), Some(result)) => Some(answered(
                    call, result, cols, gutter, surface, ground, held.fold,
                )),
                (What::Live { .. }, _) => None,
            };
            if let Some((mut rows, kept)) = laid {
                if ruled {
                    let result = held.result.as_ref().or(match &held.what {
                        What::Fact(entry @ Entry::Result { .. }) => Some(entry),
                        _ => None,
                    });
                    let compact = held.fold == Fold::Closed && kept.is_none() && result.is_some_and(|entry| {
                        matches!(entry, Entry::Result { exit: Some(exit), text, .. }
                            if exit.is_ok() && (text.to_string().is_empty() || text.to_string().split('\n').count() <= FOLD_HEAD))
                    });
                    let width = rows
                        .iter()
                        .map(Row::width)
                        .max()
                        .unwrap_or(0)
                        .min(cols)
                        .saturating_add(rule);
                    for row in &mut rows {
                        let mut inset = row.clone().shifted(rule);
                        inset
                            .spans
                            .insert(0, (String::from(RULE_INSET), Look::plain(Ink::Dim)));
                        if let Some(fence) = &mut inset.fence {
                            fence.gutter = fence.gutter.saturating_add(rule);
                        }
                        inset.tool_width = Some(width);
                        inset.tool_compact = compact;
                        inset.tool_outcome = match result {
                            Some(Entry::Result {
                                exit: Some(exit), ..
                            }) if !exit.is_ok() => ToolOutcome::Failed,
                            Some(_) => ToolOutcome::Settled,
                            None => ToolOutcome::Running,
                        };
                        *row = inset;
                    }
                }
                if at > 0 && !rows.is_empty() {
                    let prior = self.held[at - 1].what.is_tool();
                    let prose = self.held[at].what.is_prose();
                    let prior_prose = self.held[at - 1].what.is_prose();
                    if (tool && prior_prose) || (prose && prior) {
                        rows.insert(0, Row::blank());
                    }
                }
                self.lays += 1;
                self.walking(at, &mut rows);
                if at == 0 && is_prompt(&self.held[at]) {
                    rows.remove(0);
                }
                self.laid += rows.len();
                self.held[at].rows = Some(rows);
                self.held[at].kept = kept;
            }
        }
        self.held[at].rows.as_deref().unwrap_or_default()
    }

    fn work(&self, at: usize) -> Range<usize> {
        let start = (0..=at)
            .rev()
            .find(|at| !matches!(self.held[*at].place, Place::Within))
            .unwrap_or(at);
        let mut end = start + 1;
        while end < self.held.len() && matches!(self.held[end].place, Place::Within) {
            end += 1;
        }
        start..end
    }

    fn written_past(&self, work: &Range<usize>) -> bool {
        self.held[work.end..].iter().any(|held| {
            matches!(
                held.what,
                What::Fact(Entry::Prompt { .. } | Entry::Stop { .. })
            )
        })
    }

    fn tally(&self, work: &Range<usize>) -> Tally {
        let mut tally = Tally::default();
        for held in &self.held[work.clone()] {
            if let What::Fact(Entry::Call {
                name, arguments, ..
            }) = &held.what
            {
                tally.calls += 1;
                if let Some(path) = spell::file(&name.to_string(), &arguments.to_string()) {
                    tally.files.insert(path);
                }
            }
            if let Some(Entry::Result { elapsed_ms, .. }) = result_of(held) {
                tally.elapsed_ms = tally.elapsed_ms.saturating_add(*elapsed_ms);
            }
        }
        tally
    }

    fn says_something(&self, work: &Range<usize>) -> bool {
        self.held[work.clone()].iter().any(|held| {
            matches!(held.what, What::Fact(Entry::Call { .. }))
                || matches!(result_of(held), Some(Entry::Result { elapsed_ms, .. }) if *elapsed_ms > 0)
        })
    }

    fn worth_a_row(&self, work: &Range<usize>) -> bool {
        self.written_past(work) && self.says_something(work)
    }

    fn one_row(&self, work: &Range<usize>) -> bool {
        matches!(self.held[work.start].place, Place::Opens(Fold::Closed)) && self.worth_a_row(work)
    }

    fn work_row(&self, work: &Range<usize>, cols: u16) -> Row {
        marked(
            &KEPT.to_string(),
            Ink::Dim,
            vec![(self.tally(work).words(), Ink::Dim)],
            cols,
        )
    }

    fn newest_fold(&mut self) -> Option<usize> {
        for at in (0..self.held.len()).rev() {
            if holds_fold(&self.held[at]) && !self.one_row(&self.work(at)) {
                self.rows(at);
                if self.held[at].kept.is_some() {
                    return Some(at);
                }
            } else if matches!(self.held[at].place, Place::Opens(_))
                && self.worth_a_row(&self.work(at))
            {
                return Some(at);
            }
        }
        None
    }

    pub fn toggle_newest(&mut self) -> bool {
        let Some(at) = self.newest_fold() else {
            return false;
        };
        match holds_fold(&self.held[at]) && !self.one_row(&self.work(at)) {
            true => self.fold(at, self.held[at].fold.other()),
            false => self.toggle(at),
        }
    }

    pub fn toggle_result(&mut self, at: usize) -> bool {
        if at >= self.held.len()
            || result_of(&self.held[at]).is_none()
            || self.one_row(&self.work(at))
        {
            return false;
        }
        self.fold(at, self.held[at].fold.other())
    }

    pub fn toggle(&mut self, at: usize) -> bool {
        let work = self.work(at);
        if !self.worth_a_row(&work) {
            return false;
        }
        let Place::Opens(fold) = self.held[work.start].place else {
            return false;
        };
        self.opened(work.start, fold.other())
    }

    pub fn press(&mut self, at: usize, row: u16) -> bool {
        let Some(held) = self.held.get(at) else {
            return false;
        };
        if !is_prompt(held) {
            if holds_fold(held) && !self.one_row(&self.work(at)) {
                let fold = held.fold;
                let rows = self.rows(at);
                let target = match fold {
                    Fold::Closed => rows.iter().position(|row| {
                        row.fence_id() == Some(RESULT_SOURCE) && row.source().is_none()
                    }),
                    Fold::Open => rows.iter().position(|row| row.line_of(RESULT_SOURCE)),
                };
                if target == Some(usize::from(row)) {
                    return self.fold(at, fold.other());
                }
            }
            return row == 0 && self.work(at).start == at && self.toggle(at);
        }
        let fold = self.held[at].fold;
        let rows = self.rows(at);
        let words = rows
            .iter()
            .position(|row| row.stands_on() == Some(Band::Prompt));
        let moves = match fold {
            Fold::Closed => usize::from(row).checked_add(1) == Some(rows.len()),
            Fold::Open => words == Some(usize::from(row)),
        };
        moves && self.fold(at, fold.other())
    }

    pub fn work_of(&self, at: usize) -> Option<Range<usize>> {
        (at < self.held.len()).then(|| self.work(at))
    }

    fn prompt_above(&self, at: usize) -> Option<usize> {
        (0..=at).rev().find(|entry| is_prompt(&self.held[*entry]))
    }

    pub fn is_answer(&self, at: usize) -> bool {
        self.held.get(at).is_some_and(|held| {
            matches!(held.what, What::Fact(Entry::Assistant { .. })) && held.place == Place::Alone
        })
    }

    pub fn exchange(&self, at: usize) -> Option<Range<usize>> {
        if at >= self.held.len() {
            return None;
        }
        let start = self.prompt_above(at)?;
        let end = (start + 1..self.held.len())
            .find(|entry| is_prompt(&self.held[*entry]))
            .unwrap_or(self.held.len());
        Some(start..end)
    }

    pub fn open(&mut self, at: usize) -> bool {
        if at >= self.held.len() {
            return false;
        }
        let work = self.work(at);
        if self.one_row(&work) {
            return self.opened(work.start, Fold::Open);
        }
        self.fold(at, Fold::Open)
    }

    fn opened(&mut self, at: usize, to: Fold) -> bool {
        match self.held[at].place {
            Place::Opens(fold) if fold != to => {}
            _ => return false,
        }
        self.held[at].place = Place::Opens(to);
        self.held[at].rows = None;
        true
    }

    fn fold(&mut self, at: usize, to: Fold) -> bool {
        let Some(held) = self.held.get(at) else {
            return false;
        };
        if !holds_fold(held) || held.fold == to {
            return false;
        }
        if self.held[at].rows.is_none() {
            self.rows(at);
        }
        if self.held[at].kept.is_none() && result_of(&self.held[at]).is_none() {
            return false;
        }
        self.held[at].fold = to;
        self.held[at].rows = None;
        true
    }

    pub fn folded_prompt(&self, at: usize) -> Option<String> {
        let held = self.held.get(at)?;
        match &held.what {
            What::Fact(Entry::Prompt { text }) => Some(text.to_string()),
            _ => None,
        }
    }

    pub fn last_answer(&self) -> Option<String> {
        self.held.iter().rev().find_map(|held| match &held.what {
            What::Fact(Entry::Assistant { text, .. }) => Some(text.to_string()),
            _ => None,
        })
    }

    pub fn source(&self, at: usize, fence: usize) -> Option<String> {
        let held = self.held.get(at)?;
        if fence == RESULT_SOURCE
            && let Some(Entry::Result { text, .. }) = result_of(held)
        {
            return Some(text.to_string());
        }
        let rows = held.rows.as_ref()?;
        let lines: Vec<&str> = rows
            .iter()
            .filter(|row| row.line_of(fence))
            .filter_map(|row| {
                row.source()
                    .filter(|source| fence != INVOCATION_SOURCE || !source.is_empty())
            })
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    pub fn height(&mut self) -> u32 {
        let mut height: u32 = 0;
        for at in 0..self.held.len() {
            height = height.saturating_add(u32::try_from(self.rows(at).len()).unwrap_or(u32::MAX));
        }
        height
    }

    pub fn prompts(&mut self) -> Vec<u32> {
        let mut tops = Vec::new();
        let mut at: u32 = 0;
        for entry in 0..self.held.len() {
            if is_prompt(&self.held[entry]) {
                tops.push(at);
            }
            at = at.saturating_add(u32::try_from(self.rows(entry).len()).unwrap_or(u32::MAX));
        }
        tops
    }

    pub fn thumb(&mut self, scroll: Scroll, band: u16) -> Option<Thumb> {
        let total = self.height();
        if self.measure.cols <= pen::cells(INDENT) + 1
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

    pub fn deepest(&mut self, height: u16) -> u32 {
        self.height().saturating_sub(u32::from(height))
    }

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
            let rows = u16::try_from(filled.saturating_sub(skip).saturating_sub(cut))
                .expect("visible rows fit the band's height");
            if rows > 0 {
                pieces.push(Piece {
                    entry: at,
                    skip: skip as usize,
                    rows,
                });
            }
            below = below.saturating_add(filled);
        }
        pieces.reverse();
        Window { pieces }
    }
}

impl Deltas for Rows {
    fn text(&mut self, fragment: &str) {
        self.in_flight.text(fragment);
    }

    fn reasoning(&mut self, fragment: &str) {
        self.in_flight.reasoning(fragment);
    }
}

fn is_prompt(held: &Held) -> bool {
    matches!(held.what, What::Fact(Entry::Prompt { .. }))
}

fn work_entry(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::Turn { .. } | Entry::Reasoning { .. } | Entry::Call { .. } | Entry::Result { .. }
    )
}

fn result_of(held: &Held) -> Option<&Entry> {
    match &held.what {
        What::Fact(result @ Entry::Result { .. }) => Some(result),
        _ => held.result.as_ref(),
    }
}

fn holds_fold(held: &Held) -> bool {
    result_of(held).is_some() || is_prompt(held)
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
    pub skip: usize,
    pub rows: u16,
}
