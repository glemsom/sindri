//! The settings modal: one row per key of the config file, the path saving
//! writes to, and the warning a secret earns. A secret's value is
//! never drawn; the row says where the value comes from and nothing more.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget};
use sindri_core::config::Key;

use sindri_core::config::toggle_shown;

use crate::markdown::text::{self, clip};
use crate::settings::{Field, Settings};
use crate::theme;

const LABEL: usize = 20;
const MARGIN: u16 = 4;

const HINTS: &str = "↑↓/Tab move · PgUp/PgDn page · Ctrl-U clear · Enter save · Esc discard";

/// Draws the modal and answers with the field rows its box showed, which is
/// what a page moves the cursor by.
pub fn draw(settings: &Settings, area: Rect, buf: &mut Buffer) -> usize {
    let width = area.width.saturating_sub(MARGIN * 2).clamp(30, 110);
    let columns = width.saturating_sub(2) as usize;

    // Above the list: the file saving writes to, and — when the door opened
    // the modal — what is absent. Pinned, because a row a user needs is never
    // the row that fell off.
    let mut head: Vec<Line<'static>> = Vec::new();
    head.extend(text::wrap(
        text::glyphs(&[
            Span::styled("file ", theme::faint()),
            Span::raw(settings.path().display().to_string()),
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
                format!(
                    "written to {} · 0600 · the value is never shown again",
                    settings.path().display()
                ),
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
                format!(
                    "plaintext: saving writes this secret, unencrypted, to {}",
                    settings.path().display()
                ),
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
    foot.push(Line::from(Span::styled(
        clip(HINTS, columns),
        theme::faint(),
    )));

    // The box is sized to what it has to say, not to a guess: a long path
    // wraps and the modal grows, and when it still does not fit, it grows no
    // further than the terminal and the list scrolls instead.
    let rows = settings.fields().len();
    let wanted = head.len() + rows + foot.len() + 2;
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
    let first = settings.chosen().saturating_add(1).saturating_sub(body);
    let last = rows.min(first + body);

    let mut lines = head;
    for (offset, field) in settings.fields()[first..last].iter().enumerate() {
        let line = row(settings, field, columns);
        lines.push(if first + offset == settings.chosen() {
            line.patch_style(Style::new().add_modifier(Modifier::REVERSED))
        } else {
            line
        });
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

fn row(settings: &Settings, field: &Field, columns: usize) -> Line<'static> {
    let label = format!("{:<LABEL$}", clip(field.key.name(), LABEL));
    let (shown, style) = value(settings, field);
    Line::from(vec![
        Span::styled(label, theme::faint()),
        Span::styled(clip(&shown, columns.saturating_sub(LABEL)), style),
    ])
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
        // A toggle the file does not name is the run's own default,
        // which is off, and says so rather than pretending
        // the file said it.
        Key::Thinking if typed.is_empty() => {
            (format!("{} (default)", toggle_shown(typed)), Style::new())
        }
        Key::Thinking => (toggle_shown(typed).to_string(), Style::new()),
        _ if field.key.secret() && !typed.is_empty() => (
            "•••• (will be written to the file)".to_string(),
            theme::faint(),
        ),
        _ => (field.text.clone(), Style::new()),
    }
}
