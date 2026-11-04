//! The settings modal: one row per key of the config file, the path saving
//! writes to, and the warning a secret earns. A secret's value is
//! never drawn; the row says where the value comes from and nothing more. The
//! model row carries the one fact the modal did not have before — how many
//! names the wire serves, and which wire named them — as faint text beside the
//! name this run holds, and, under the row while a fragment is being typed in
//! it, the names that fragment reaches with the one a `Tab` would take.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget};
use sindri_core::config::Key;

use sindri_core::config::toggle_shown;
use unicode_width::UnicodeWidthChar;

use crate::markdown::text::{self, clip};
use crate::settings::{Field, List, Served, Settings};
use crate::theme;

const LABEL: usize = 20;
const MARGIN: u16 = 4;
/// The narrowest a modal still reads at, and the widest: past the wide one a
/// row of setting and value grows no easier to read for the columns it costs.
const NARROW: u16 = 30;
const WIDE: u16 = 110;

const HINTS: &str =
    "↑↓ move or match · Tab/→ take · PgUp/PgDn page · Ctrl-U clear · Enter save · Esc discard";

/// The keys while the names stand open under the model row: the fold has taken
/// the arrows, `Enter` and `Esc` while it stands, so the line says what they do
/// there rather than what they do outside it.
const FOLD_HINTS: &str =
    "↑↓ move the names · Enter take one and close · Esc close and keep the draft";

/// Draws the modal and answers with the field rows its box showed, which is
/// what a page moves the cursor by.
pub fn draw(settings: &Settings, served: &Served, area: Rect, buf: &mut Buffer) -> usize {
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
    let hints = match settings.list().folded() {
        true => FOLD_HINTS,
        false => HINTS,
    };
    foot.push(Line::from(Span::styled(
        clip(hints, columns),
        theme::faint(),
    )));

    // The box is sized to what it has to say, not to a guess: a long path
    // wraps and the modal grows, and when it still does not fit, it grows no
    // further than the terminal and the list scrolls instead.
    let rows = settings.fields().len();
    // The list stands under the model row, and only there: the box is sized for
    // it, so the keys below are never the rows a list pushes off.
    let filtering = settings
        .fields()
        .get(settings.chosen())
        .is_some_and(|field| field.key == Key::Model);
    let list_rows = match filtering {
        true => settings.list().len(),
        false => 0,
    };
    let wanted = head.len() + rows + list_rows + foot.len() + 2;
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
    let fielded = body.saturating_sub(listed);
    let first = settings.chosen().saturating_add(1).saturating_sub(fielded);
    let last = rows.min(first + fielded);

    let mut lines = head;
    for (offset, field) in settings.fields()[first..last].iter().enumerate() {
        let chosen = first + offset == settings.chosen();
        let line = row(settings, served, field, columns);
        lines.push(match chosen {
            true => line.patch_style(Style::new().add_modifier(Modifier::REVERSED)),
            false => line,
        });
        if chosen {
            lines.extend(list_lines(settings.list(), columns, from, to));
        }
    }
    lines.extend(foot);

    let boxed = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    Clear.render(boxed, buf);
    let block = super::overlay::paged(
        "settings",
        super::overlay::More {
            above: first > 0,
            below: last < rows,
        },
    );
    let inner = block.inner(boxed);
    Paragraph::new(lines)
        .style(theme::raised())
        .render(inner, buf);
    block.render(boxed, buf);
    last - first
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

fn row(settings: &Settings, served: &Served, field: &Field, columns: usize) -> Line<'static> {
    let label = format!("{:<LABEL$}", clip(field.key.name(), LABEL));
    let (shown, style) = value(settings, field);
    let shown = clip(&shown, columns.saturating_sub(LABEL));
    let mut spans = vec![Span::styled(label, theme::faint())];
    // What the wire said about its own names, on the row of the name this run
    // holds and after it: faint, because it is a hint about the wire and not a
    // value the file carries.
    let note = match field.key {
        Key::Model => served.note(),
        _ => None,
    };
    let room = columns.saturating_sub(LABEL + cells(&shown));
    let note = match note {
        Some(note) if shown.is_empty() => clip(&note, room),
        // The separator is drawn only where something follows it: a row with no
        // cells left for a hint ends at the value.
        Some(note) => match clip(&note, room.saturating_sub(3)) {
            clipped if clipped.is_empty() => String::new(),
            clipped => format!(" · {clipped}"),
        },
        None => String::new(),
    };
    spans.push(Span::styled(shown, style));
    if !note.is_empty() {
        spans.push(Span::styled(note, theme::faint()));
    }
    Line::from(spans)
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

fn value(settings: &Settings, field: &Field) -> (String, Style) {
    let typed = field.text.trim();
    match field.key {
        Key::ApiKey if typed.is_empty() && settings.document().is_set(field.key) => {
            ("•••• set in the file (hidden)".to_string(), theme::faint())
        }
        // The door's modal: the environment was read and had nothing in it,
        // so the row says where the value would come from and that it did not.
        Key::ApiKey
            if typed.is_empty()
                && settings
                    .missing()
                    .is_some_and(|missing| missing.wants_key()) =>
        {
            (
                "from environment (not set — type it here)".to_string(),
                theme::bad(),
            )
        }
        Key::ApiKey if typed.is_empty() => ("from environment".to_string(), theme::faint()),
        Key::ExtraHeaders if typed.is_empty() && settings.document().is_set(field.key) => (
            "•••• set in the file (values hidden)".to_string(),
            theme::faint(),
        ),
        // A toggle the file does not name says so rather than pretending the
        // file said it, and `thinking` names the default it falls back to: off.
        Key::Thinking if typed.is_empty() => {
            (format!("{} (default)", toggle_shown(typed)), theme::plain())
        }
        // The reports and the agent's socket are on unless something silences
        // them, so the row states the default it is running on.
        Key::Status | Key::SshAgent if typed.is_empty() => {
            ("on (default)".to_string(), theme::plain())
        }
        Key::Thinking | Key::Status | Key::SshAgent => {
            (toggle_shown(typed).to_string(), theme::plain())
        }
        _ if field.key.secret() && !typed.is_empty() => (
            "•••• (will be written to the file)".to_string(),
            theme::faint(),
        ),
        _ => (field.text.clone(), theme::plain()),
    }
}
