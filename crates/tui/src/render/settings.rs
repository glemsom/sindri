//! The settings modal: one row per key of the config file, the path saving
//! writes to, and the warning a secret earns. A secret's value is
//! never drawn; the row says where the value comes from and nothing more. The
//! model row carries the one fact the modal did not have before — how many
//! names the wire serves, and which wire named them — as faint text beside the
//! name this run holds, and, under the row while a fragment is being typed in
//! it, the names that fragment reaches with the one a `Tab` would take. The
//! provider row is a choice the same way and drawn by the same list: `Enter` on
//! it offers the wires the run parses under it, the one it is on marked.
//! The extra headers row draws the table itself while its fold stands open: one
//! `Name: value` line per header with the fold's cursor marked and the offer to
//! add a header under the last, the way a list of names stands under the model
//! row. The count of headers is a faint note on the row, in the place the model
//! row says what the wire named.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget};
use sindri_core::config::{DEFAULT_MAX_TURNS, Key, toggle_shown};
use sindri_core::exec::DEFAULT_TIMEOUT_SECS;
use unicode_width::UnicodeWidthChar;

use crate::markdown::text::{self, clip};
use crate::settings::{Field, Fold, List, Served, Settings, counted};
use crate::theme;

const LABEL: usize = 20;
/// The cells a line of an open table spends on the fold's mark: the value's
/// column moves right of it for as long as the table stands open, so every line
/// of a table is drawn from one column.
const MARK: usize = 2;
const MARGIN: u16 = 4;
/// The narrowest a modal still reads at, and the widest: past the wide one a
/// row of setting and value grows no easier to read for the columns it costs.
const NARROW: u16 = 30;
const WIDE: u16 = 110;

/// The keys the row with the cursor takes: the arrows and the two ends of it
/// move the cursor, the two keys either side of it erase, and `Ctrl-U` is still
/// the one way to unsay a value. `↑↓`, `Tab` and `BackTab` walk the rows rather
/// than the row, which is what the line says they do.
const HINTS: &str =
    "←→ Home/End move · ↑↓ Tab rows · Bksp/Del erase · Ctrl-U clear · Enter save · Esc discard";

/// The keys while the names stand open under the model row: the fold has taken
/// the arrows, `Enter` and `Esc` while it stands, so the line says what they do
/// there rather than what they do outside it.
const FOLD_HINTS: &str =
    "↑↓ move the names · Enter take one and close · Esc close and keep the draft";

/// The keys while the headers row's table stands open and the keys are the
/// fold's: the arrows walk its lines, `a` and `d` add and remove one, and
/// `Enter` is the rung that hands the line its own keys — which is where the two
/// letters become characters of a header.
const TABLE_HINTS: &str =
    "↑↓ move the lines · a add · d delete · Ctrl-U empty · Enter edit · Esc close";

/// And the keys while one line is being written: the fold has handed it the
/// keyboard, so what the line says is the row's own and what is left of the fold
/// is `Enter`, `Ctrl-U` and `Esc`.
const WRITE_HINTS: &str = "Enter done · Ctrl-U empty the line · Esc close and keep the draft";

/// Draws the modal and answers with the field rows its box showed, which is
/// what a page moves the cursor by, and with the cell the cursor stands on —
/// `None` where the row under it draws no value of its own to point at.
pub fn draw(
    settings: &Settings,
    served: &Served,
    area: Rect,
    buf: &mut Buffer,
) -> (usize, Option<Position>) {
    // The box is sized to what it has to say, not to a guess: the file's path
    // is the one fact it cannot shorten and still state, so a frame too narrow
    // for it at the margin the box would like gives the margin up rather than
    // the path, and only a frame narrower than the path itself wraps it.
    let path = settings.path().display().to_string();
    let needed = u16::try_from("file ".len() + cells(&path) + 2).unwrap_or(u16::MAX);
    let width = area
        .width
        .saturating_sub(MARGIN * 2)
        .max(needed)
        .clamp(NARROW, WIDE)
        .min(area.width);
    let columns = width.saturating_sub(2) as usize;
    // And a path the frame still cannot hold breaks at its separators, never
    // inside a name: what the modal names is a file, and half a name on each of
    // two rows is not one.
    let broken = broken_at_separators(&path, columns);

    // Above the list: the file saving writes to, and — when the door opened
    // the modal — what is absent. Pinned, because a row a user needs is never
    // the row that fell off.
    let mut head: Vec<Line<'static>> = Vec::new();
    head.extend(text::wrap(
        text::glyphs(&[
            Span::styled("file ", theme::faint()),
            Span::raw(broken.clone()),
        ]),
        "",
        theme::faint(),
        columns,
    ));
    head.push(Line::default());
    if let Some(missing) = settings.missing() {
        // The door's modal: why it opened, and the four facts a person needs
        // before typing a credential into it.
        for (said, style) in [
            ("no credential: this run has none".to_string(), theme::bad()),
            (
                format!("type it here, or set {}", missing.key_vars()),
                theme::faint(),
            ),
            (
                format!("written to {broken} · 0600 · the value is never shown again"),
                theme::faint(),
            ),
        ] {
            head.extend(text::wrap(
                text::glyphs(&[Span::styled(said, style)]),
                "",
                style,
                columns,
            ));
        }
        head.push(Line::default());
    }

    // Below it: the warning a staged secret earns, the refusal a failed save
    // earns — the one row that must never be the row that disappears — and
    // the keys. Pinned too.
    let mut foot: Vec<Line<'static>> = vec![Line::default()];
    if settings.staged_secret() {
        foot.extend(text::wrap(
            text::glyphs(&[Span::styled(
                format!("plaintext: saving writes this secret, unencrypted, to {broken}"),
                theme::bad(),
            )]),
            "",
            theme::bad(),
            columns,
        ));
        foot.push(Line::default());
    }
    if let Some(error) = settings.error() {
        foot.extend(text::wrap(
            text::glyphs(&[Span::styled(format!("not saved · {error}"), theme::bad())]),
            "",
            theme::bad(),
            columns,
        ));
    }
    // The keys the row with the cursor takes, said by the fold that has them:
    // the headers table's, the names under the model row's, or the modal's own.
    let hints = match settings.headers() {
        Fold::Writing => WRITE_HINTS,
        Fold::Choosing => TABLE_HINTS,
        Fold::Shut if settings.list().folded() => FOLD_HINTS,
        Fold::Shut => HINTS,
    };
    foot.push(Line::from(Span::styled(
        clip(hints, columns),
        theme::faint(),
    )));

    // The box is sized to what it has to say, not to a guess: a long path
    // wraps and the modal grows, and when it still does not fit, it grows no
    // further than the terminal and the list scrolls instead.
    let rows = settings.fields().len();
    // A row is as tall as the value it holds: the headers row is one line per
    // header, and every other row is the one line it always was. A table folded
    // open is one line taller for the offer under it.
    let heights: Vec<usize> = settings
        .fields()
        .iter()
        .map(|field| tall(settings, field))
        .collect();
    let value_lines: usize = heights.iter().sum();
    // The list stands under the row that offers words — the model row's names,
    // the provider row's wires — and only there: the box is sized for it, so the
    // keys below are never the rows a list pushes off.
    let list_rows = match settings.offering() {
        true => settings.list().len(),
        false => 0,
    };
    let wanted = head.len() + value_lines + list_rows + foot.len() + 2;
    let height = (wanted as u16).min(area.height);
    // The refusal and the keys are never the rows that fall off: when the
    // pinned rows need more room than the box has, the head is what gives way
    // and the list gets whatever is left — even nothing.
    let room = (height as usize).saturating_sub(2);
    if head.len() + foot.len() > room {
        head.truncate(room.saturating_sub(foot.len()));
    }
    // The list shows a window around the row the cursor is on, so the chosen
    // row is on screen however far past the fold it is.
    let body = room.saturating_sub(head.len() + foot.len());
    // What the row being typed in has to say is never what falls off: the list's
    // rows come out of the same room the field rows do, the chosen one included.
    let listed = list_rows.min(body.saturating_sub(1));
    // A fold taller than the room it has is scrolled rather than cut at the top:
    // the window stands around the name the cursor is on, so the row it is on is
    // the row that is drawn.
    let from = match listed {
        0 => 0,
        listed => settings
            .list()
            .chosen()
            .saturating_add(1)
            .saturating_sub(listed),
    };
    let to = (from + listed).min(list_rows);
    let (first, last) = window(&heights, settings.chosen(), body.saturating_sub(listed));

    let boxed = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    let block = super::overlay::paged(
        "settings",
        super::overlay::More {
            above: first > 0,
            below: last < rows,
        },
    );
    let inner = block.inner(boxed);

    let mut lines = head;
    // Where the row being typed in stands: a cell of the box's own inside, so
    // the terminal's cursor is drawn where the next character lands rather than
    // hidden behind a modal that covers the composer's.
    let mut cursor: Option<Position> = None;
    for (offset, field) in settings.fields()[first..last].iter().enumerate() {
        let chosen = first + offset == settings.chosen();
        let top = lines.len();
        let (drawn, at) = row(settings, served, field, chosen, columns);
        // A row is one line per line of its value, so the reverse that marks
        // the row with the cursor covers every line of it.
        lines.extend(drawn.into_iter().map(|line| match chosen {
            true => line.patch_style(Style::new().add_modifier(Modifier::REVERSED)),
            false => line,
        }));
        if let Some(at) = at {
            let cell = Position::new(
                inner.x + at.column,
                inner.y + u16::try_from(top + at.line).unwrap_or(u16::MAX),
            );
            // Only a cell the box actually draws: a row taller than the room it
            // has is not a place the cursor can be put.
            cursor = inside(inner, cell).then_some(cell);
        }
        if chosen {
            lines.extend(list_lines(settings.list(), columns, from, to));
        }
    }
    lines.extend(foot);
    // The box is drawn under the rows and not over them: a block paints its
    // style across the whole of itself, the inside included, so a box drawn last
    // washes every row's ink — the faint of a label, the count beside it — into
    // the surface's own. The rows are drawn after it, inside it, so the inks the
    // rows were built with are the inks a reader sees.
    Clear.render(boxed, buf);
    block.render(boxed, buf);
    Paragraph::new(lines)
        .style(theme::raised())
        .render(inner, buf);
    (last - first, cursor)
}

/// Whether a cell is one the box draws: the cursor is never placed on a corner
/// or past the edge the paragraph falls off.
fn inside(inner: Rect, at: Position) -> bool {
    at.x >= inner.x
        && at.x < inner.x + inner.width
        && at.y >= inner.y
        && at.y < inner.y + inner.height
}

/// A path is a token with places to break — its separators — and one too long
/// for the row breaks there rather than at the width: every segment but the
/// last keeps its `/`, so each row reads as a path continuing rather than as a
/// second one, and a file's name is never cut in half. A single segment longer
/// than the row has no such place, so it is left to the wrapper.
fn broken_at_separators(path: &str, columns: usize) -> String {
    let columns = columns.max(1);
    let segments: Vec<&str> = path.split_inclusive('/').collect();
    // The file's name and the directory it sits in are the two segments a
    // reader reads as one fact, so the row breaks before them rather than
    // between them: every other break is the plain one, because a path that
    // spends a row on each of its segments is a path that costs the list its
    // rows.
    let tail = segments.len().saturating_sub(2);
    let mut broken = String::new();
    let mut used = 0;
    for (at, segment) in segments.iter().enumerate() {
        let taken = cells(segment);
        let after = if at + 1 >= tail {
            segments.get(at + 1).map_or(0, |next| cells(next))
        } else {
            0
        };
        if used > 0 && used + taken + after > columns {
            broken.push('\n');
            used = 0;
        }
        broken.push_str(segment);
        used += taken;
    }
    broken
}

/// The cells a row of text takes, as the wrapper counts them: a wide glyph is
/// two, and a path holds whatever its directories are named.
fn cells(text: &str) -> usize {
    text.chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

/// Where a row's cursor is drawn: which of the row's own lines, and which cell
/// of the value's column — a cell of the row and never of the frame, so a box
/// that has moved costs the cursor a move and not a recount.
struct Cursor {
    line: usize,
    column: u16,
}

/// What a row draws for the field it edits: the text, the ink it is written in,
/// and whether that text is the field's own value. A secret is drawn as a mask
/// and a toggle as one of its two words, so neither can point at a character of
/// the value it holds — and neither pretends to.
struct Drawn {
    text: String,
    style: Style,
    own: bool,
}

impl Drawn {
    /// A row drawn as the value it holds, character for character.
    fn own(text: String) -> Drawn {
        Drawn {
            text,
            style: theme::plain(),
            own: true,
        }
    }

    /// A row drawn as words of its own: a mask, a default, a toggle's two names.
    fn said(text: impl Into<String>, style: Style) -> Drawn {
        Drawn {
            text: text.into(),
            style,
            own: false,
        }
    }
}

/// One field as the rows it draws, and where its cursor stands among them when
/// the row is the one the keys are typing in.
fn row(
    settings: &Settings,
    served: &Served,
    field: &Field,
    chosen: bool,
    columns: usize,
) -> (Vec<Line<'static>>, Option<Cursor>) {
    // The headers row with its table open is drawn as the list of lines it is:
    // a mark, the table's own text, and the offer under it.
    if folded(settings, field) {
        return table(served, field, chosen, columns);
    }
    let label = format!("{:<LABEL$}", clip(field.key.name(), LABEL));
    let drawn = value(settings, field);
    // The cells of the row the value has: what the label and the box's own edge
    // leave of it, and one column of the box at each side.
    let room = columns.saturating_sub(LABEL);
    // The line of the value the cursor is on and the cells that line spends
    // before it, which is what the row scrolls to rather than cutting off.
    let (line, cell) = match chosen && drawn.own {
        true => place(&drawn.text, field.cursor()),
        false => (0, 0),
    };
    let mut lines = Vec::new();
    let mut cursor = None;
    // The value's own lines: a row is as tall as the value it holds, and a line
    // of it wider than the row is a window onto it rather than a cut.
    let held: Vec<&str> = match drawn.text.is_empty() {
        true => vec![""],
        false => drawn.text.lines().collect(),
    };
    for (index, text) in held.iter().enumerate() {
        let here = index == line;
        let offset = match here && drawn.own && chosen {
            true => scroll(room, cell),
            false => 0,
        };
        let shown = visible(text, offset, room);
        if index == 0 {
            let note = note(field, served, &shown, room);
            let mut spans = vec![
                Span::styled(label.clone(), theme::faint()),
                Span::styled(shown, drawn.style),
            ];
            if !note.is_empty() {
                spans.push(Span::styled(note, theme::faint()));
            }
            lines.push(Line::from(spans));
        } else {
            // The rest of a value that is a list of lines — the headers, one per
            // line — under the row and in the value's own column, so the list
            // reads as one list rather than as a row and a second row's name.
            let said = format!("{}{shown}", " ".repeat(LABEL));
            lines.push(Line::from(Span::styled(said, drawn.style)));
        }
        if here && drawn.own && chosen {
            cursor = Some(Cursor {
                line: index,
                column: u16::try_from(LABEL + cell.saturating_sub(offset)).unwrap_or(u16::MAX),
            });
        }
    }
    (lines, cursor)
}

/// What a row says about its own value after the value on its first line: the
/// names the wire offered on the model row, the count of headers the headers row
/// holds — faint, because it is a fact about where the value comes from and not
/// a value the file carries. A value scrolled to its cursor has no columns left
/// for a note, and the note is what goes.
fn note(field: &Field, served: &Served, shown: &str, room: usize) -> String {
    let note = match field.key {
        Key::Model => served.note(),
        Key::ExtraHeaders => Some(counted(&field.text)),
        _ => None,
    };
    let left = room.saturating_sub(cells(shown));
    match note {
        Some(note) if shown.is_empty() => clip(&note, left),
        Some(note) => match clip(&note, left.saturating_sub(3)) {
            clipped if clipped.is_empty() => String::new(),
            clipped => format!(" · {clipped}"),
        },
        None => String::new(),
    }
}

/// The headers row with its table folded open: the row's name, then one line per
/// header in the file's own `Name: value` spelling with the fold's cursor marked,
/// and the offer to add a header under the last of them. The text is the value's
/// own and unresolved, so a `{session}` in it is drawn as written; the mark is
/// the row's own cursor, so the line it stands on is the line the next keystroke
/// is a keystroke of.
fn table(
    served: &Served,
    field: &Field,
    chosen: bool,
    columns: usize,
) -> (Vec<Line<'static>>, Option<Cursor>) {
    let label = format!("{:<LABEL$}", clip(field.key.name(), LABEL));
    let room = columns.saturating_sub(LABEL + MARK);
    // The line the cursor is in and the cells it spends before itself on that
    // line: a table too narrow for a line is scrolled to the cursor there, and
    // every other line of it is drawn from its own start.
    let (line, cell) = match chosen {
        true => place(&field.text, field.cursor()),
        false => (0, 0),
    };
    let offset = match chosen {
        true => scroll(room, cell),
        false => 0,
    };
    let mut lines = Vec::new();
    let mut cursor = None;
    for (index, text) in field.text.split('\n').enumerate() {
        let here = index == line;
        let mark = match here {
            true => "▸ ",
            false => "  ",
        };
        let lead = match index {
            0 => label.clone(),
            _ => " ".repeat(LABEL),
        };
        let shown = visible(text, if here { offset } else { 0 }, room);
        let after = match index {
            0 => note(field, served, &shown, room),
            _ => String::new(),
        };
        // The line's mark is the row's own ink where it stands and faint where
        // it does not, so the fold's cursor is read before the table is.
        let mut spans = vec![
            Span::styled(lead, theme::faint()),
            Span::styled(
                mark,
                match here {
                    true => theme::plain(),
                    false => theme::faint(),
                },
            ),
            Span::styled(shown, theme::plain()),
        ];
        if !after.is_empty() {
            spans.push(Span::styled(after, theme::faint()));
        }
        lines.push(Line::from(spans));
        if here && chosen {
            cursor = Some(Cursor {
                line: index,
                column: u16::try_from(LABEL + MARK + cell.saturating_sub(offset))
                    .unwrap_or(u16::MAX),
            });
        }
    }
    // The offer the fold draws under the last line: the line `a` adds a header
    // on, and faint, because it is a thing the row would do and not a header.
    lines.push(Line::from(Span::styled(
        format!("{}+ add a header", " ".repeat(LABEL + MARK)),
        theme::faint(),
    )));
    (lines, cursor)
}

/// Where a character index of a value lands among the cells it is drawn in: the
/// line of the value, and the cells that line spends before the cursor. A glyph
/// two cells wide spends both, because the terminal draws both.
fn place(text: &str, at: usize) -> (usize, usize) {
    let mut line = 0;
    let mut cell = 0;
    for ch in text.chars().take(at) {
        match ch {
            '\n' => {
                line += 1;
                cell = 0;
            }
            ch => cell += UnicodeWidthChar::width(ch).unwrap_or(0),
        }
    }
    (line, cell)
}

/// The cells a row gives up on the left of a value too wide for it: none while
/// the cursor's cell fits, and otherwise just enough that the cursor is the
/// row's last cell rather than one past it — a field too narrow for its value
/// is scrolled, never a cursor hidden behind the row's edge.
fn scroll(room: usize, cursor: usize) -> usize {
    cursor.saturating_sub(room.saturating_sub(1))
}

/// The cells of one line of a value from `offset` cells in, as many as the row
/// has room for: the line itself while nothing is given up, and never a glyph
/// cut in half.
fn visible(line: &str, offset: usize, room: usize) -> String {
    let mut dropped = 0;
    let mut used = 0;
    let mut shown = String::new();
    for ch in line.chars() {
        let wide = UnicodeWidthChar::width(ch).unwrap_or(0);
        if dropped < offset {
            dropped += wide;
            continue;
        }
        if used + wide > room {
            break;
        }
        used += wide;
        shown.push(ch);
    }
    shown
}

/// The lines a row takes: one per line of the value it holds, and — while the
/// headers row's table stands open — the offer to add a header under them. Only
/// the headers hold more than one, and a row that holds nothing is still a row.
fn tall(settings: &Settings, field: &Field) -> usize {
    match folded(settings, field) {
        true => field.text.split('\n').count() + 1,
        false => field.text.lines().count().max(1),
    }
}

/// Whether this row is the headers row with its table standing open under it.
fn folded(settings: &Settings, field: &Field) -> bool {
    field.key == Key::ExtraHeaders && settings.headers().folded()
}

/// The rows a body of `room` lines shows: the row with the cursor and as many
/// of its neighbours as fit, walked back first so the chosen row is on screen
/// however far past the fold it is. The count is in lines and the answer is
/// still a range of rows, because a row is as tall as the value it holds.
fn window(heights: &[usize], chosen: usize, room: usize) -> (usize, usize) {
    if room == 0 {
        return (chosen + 1, chosen + 1);
    }
    let mut used = heights[chosen];
    let mut first = chosen;
    while first > 0 && used + heights[first - 1] <= room {
        first -= 1;
        used += heights[first];
    }
    let mut last = chosen + 1;
    while last < heights.len() && used + heights[last] <= room {
        used += heights[last];
        last += 1;
    }
    (first, last)
}

/// The list under the model row: the names the fragment reaches, nearest first,
/// or every name the wire offered while the fold stands open, the one a `Tab`
/// would take marked and drawn in the row's own ink. Faint, and under the
/// value's column, because a list is what the row would say and not what the
/// file holds. `from..to` is the window of it the box has room for.
fn list_lines(list: &List, columns: usize, from: usize, to: usize) -> Vec<Line<'static>> {
    list.names()[from.min(to)..to]
        .iter()
        .enumerate()
        .map(|(offset, name)| {
            let at = from + offset;
            let marked = at == list.chosen();
            let mark = match marked {
                true => "▸ ",
                false => "  ",
            };
            let said = format!("{}{mark}{name}", " ".repeat(LABEL));
            let style = match marked {
                true => theme::plain(),
                false => theme::faint(),
            };
            Line::from(Span::styled(clip(&said, columns), style))
        })
        .collect()
}

/// What a row says for its field: the value it holds where the value is the
/// row's to draw, and words of the modal's own where it is not — a secret's
/// mask, a toggle's two names, the default a row nobody has set is running on.
fn value(settings: &Settings, field: &Field) -> Drawn {
    let typed = field.text.trim();
    match field.key {
        Key::ApiKey if typed.is_empty() && settings.document().is_set(field.key) => {
            Drawn::said("•••• set in the file (hidden)", theme::faint())
        }
        // The door's modal: the environment was read and had nothing in it,
        // so the row says where the value would come from and that it did not.
        Key::ApiKey
            if typed.is_empty()
                && settings
                    .missing()
                    .is_some_and(|missing| missing.wants_key()) =>
        {
            Drawn::said("from environment (not set — type it here)", theme::bad())
        }
        Key::ApiKey if typed.is_empty() => Drawn::said("from environment", theme::faint()),
        // A toggle the file does not name says so rather than pretending the
        // file said it, and `thinking` names the default it falls back to: off.
        Key::Thinking if typed.is_empty() => {
            Drawn::said(format!("{} (default)", toggle_shown(typed)), theme::plain())
        }
        // The reports and the agent's socket are on unless something silences
        // them, so the row states the default it is running on.
        Key::Status | Key::SshAgent if typed.is_empty() => {
            Drawn::said("on (default)", theme::plain())
        }
        Key::Thinking | Key::Status | Key::SshAgent => {
            Drawn::said(toggle_shown(typed), theme::plain())
        }
        // The two counts are rows of the same shape: a file that names neither
        // is still a run with both, so the row states the number a run falls
        // back to — read from the constant `Config` itself reads, never typed
        // beside the row, so the two cannot state different numbers. The words
        // are the row's and not the field's, which is what keeps a save of a row
        // nobody typed in writing nothing.
        Key::MaxTurns if typed.is_empty() => {
            Drawn::said(format!("{DEFAULT_MAX_TURNS} (default)"), theme::plain())
        }
        Key::Timeout if typed.is_empty() => {
            Drawn::said(format!("{DEFAULT_TIMEOUT_SECS} (default)"), theme::plain())
        }
        _ if field.key.secret() && !typed.is_empty() => {
            Drawn::said("•••• (will be written to the file)", theme::faint())
        }
        _ => Drawn::own(field.text.clone()),
    }
}
