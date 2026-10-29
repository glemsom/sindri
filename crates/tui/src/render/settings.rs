//! The settings modal: one row per key of the config file, the path saving
//! writes to, and the warning a secret earns (ADR-0014). A secret's value is
//! never drawn; the row says where the value comes from and nothing more.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};
use sindri_core::config::Key;

use sindri_core::config::toggle_shown;

use crate::markdown::text::{self, clip};
use crate::settings::{Field, Settings};
use crate::theme;

/// How wide the key column is.
const LABEL: usize = 20;
/// How much of the frame the modal leaves to itself.
const MARGIN: u16 = 4;

/// The hint line under the fields.
const HINTS: &str = "↑↓/Tab move · Ctrl-U clear · Enter save · Esc discard";

/// Draws the modal over whatever is underneath it.
pub fn draw(settings: &Settings, area: Rect, buf: &mut Buffer) {
    let width = area.width.saturating_sub(MARGIN * 2).clamp(30, 110);
    let columns = width.saturating_sub(2) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    // The path is the point of the modal: it says which file a save lands in,
    // and it is what the warning about a secret names. It is never elided.
    lines.extend(text::wrap(
        text::glyphs(&[
            Span::styled("file ", theme::faint()),
            Span::raw(settings.path().display().to_string()),
        ]),
        "",
        theme::faint(),
        columns,
    ));
    lines.push(Line::default());
    for (index, field) in settings.fields().iter().enumerate() {
        let line = row(settings, field, columns);
        lines.push(if index == settings.chosen() {
            line.patch_style(Style::new().add_modifier(Modifier::REVERSED))
        } else {
            line
        });
    }
    lines.push(Line::default());
    if settings.staged_secret() {
        lines.extend(text::wrap(
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
        lines.push(Line::default());
    }
    if let Some(error) = settings.error() {
        lines.extend(text::wrap(
            text::glyphs(&[Span::styled(format!("not saved · {error}"), theme::bad())]),
            "",
            theme::bad(),
            columns,
        ));
    }
    lines.push(Line::from(Span::styled(
        clip(HINTS, columns),
        theme::faint(),
    )));

    // The box is sized to what it has to say, not to a guess: a long path wraps
    // and the modal grows, so nothing a user needs is the row that fell off.
    let height = (lines.len() as u16 + 2).min(area.height);
    let boxed = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    Clear.render(boxed, buf);
    let inner = Block::bordered().border_style(theme::faint()).inner(boxed);
    Paragraph::new(lines).render(inner, buf);
    Block::bordered()
        .border_style(theme::faint())
        .title(Span::styled(" settings ", theme::faint()))
        .render(boxed, buf);
}

/// One field: its key, and what the field would write.
fn row(settings: &Settings, field: &Field, columns: usize) -> Line<'static> {
    let label = format!("{:<LABEL$}", clip(field.key.name(), LABEL));
    let (shown, style) = value(settings, field);
    Line::from(vec![
        Span::styled(label, theme::faint()),
        Span::styled(clip(&shown, columns.saturating_sub(LABEL)), style),
    ])
}

/// One field's value, and how loud it is: a secret is the frame's quiet answer
/// about where the value comes from, never the value.
fn value(settings: &Settings, field: &Field) -> (String, Style) {
    let typed = field.text.trim();
    match field.key {
        Key::ApiKey if typed.is_empty() && settings.document().is_set(field.key) => {
            ("•••• set in the file (hidden)".to_string(), theme::faint())
        }
        Key::ApiKey if typed.is_empty() => ("from environment".to_string(), theme::faint()),
        Key::ExtraHeaders if typed.is_empty() && settings.document().is_set(field.key) => (
            "•••• set in the file (values hidden)".to_string(),
            theme::faint(),
        ),
        // A toggle the file does not name is the run's own default, which is
        // off (ADR-0004), and says so rather than pretending the file said it.
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
