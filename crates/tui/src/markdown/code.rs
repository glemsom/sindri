//! Fenced code: the language's own colours where the language is known, and a
//! diff coloured line by line — an agent emits code and diffs more than
//! anything else. A fence is drawn as the panel it is, with a
//! gutter down its left edge and a line too long for it cut and marked
//! rather than silently swallowed.

use std::sync::OnceLock;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use unicode_width::UnicodeWidthChar;

use crate::markdown::text::{Glyph, clip};
use crate::theme;

/// The panel's left edge and the column of air that is its gutter.
const GUTTER: &str = "│ ";
/// The panel's right edge, one column off the code.
const EDGE: &str = " │";
/// Below this many columns a panel would be all border and no code, so the
/// fence falls back to bare rows.
const MIN_PANEL: usize = 12;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let mut themes = ThemeSet::load_defaults().themes;
        themes
            .remove("base16-ocean.dark")
            .or_else(|| themes.into_values().next())
            .unwrap_or_default()
    })
}

fn is_diff(language: &str) -> bool {
    matches!(language, "diff" | "patch" | "udiff")
}

/// The fence as the frame draws it: a panel across the column it was given.
pub fn panel(body: &str, language: Option<&str>, width: usize) -> Vec<Line<'static>> {
    let rows = lines(body, language);
    let width = width.max(1);
    if width < MIN_PANEL {
        return rows
            .iter()
            .map(|row| bare(row, width))
            .collect::<Vec<Line<'static>>>();
    }
    let mut panel = Vec::with_capacity(rows.len() + 2);
    panel.push(top(language, width));
    panel.extend(rows.iter().map(|row| body_row(row, width)));
    panel.push(bottom(width));
    panel
}

fn lines(body: &str, language: Option<&str>) -> Vec<Line<'static>> {
    match language {
        Some(language) if is_diff(language) => body.lines().map(diff_line).collect(),
        Some(language) => highlight(body, language),
        None => plain(body),
    }
}

/// The text as it was written, with no colour at all: a wrong highlight is
/// worse than no highlight.
fn plain(body: &str) -> Vec<Line<'static>> {
    body.lines()
        .map(|line| Line::from(Span::raw(line.to_string())))
        .collect()
}

/// A diff's rows: the `+` or the `-` is the fact, the hue says it faster, and
/// the wash under the row says it again where the hue cannot be seen.
fn diff_line(line: &str) -> Line<'static> {
    let style = if line.starts_with("+++") || line.starts_with("---") {
        Style::new().fg(theme::DIFF_FILE)
    } else if line.starts_with('+') {
        Style::new().fg(theme::DIFF_ADDED).bg(theme::DIFF_ADDED_BG)
    } else if line.starts_with('-') {
        Style::new()
            .fg(theme::DIFF_REMOVED)
            .bg(theme::DIFF_REMOVED_BG)
    } else if line.starts_with("@@") {
        Style::new().fg(theme::DIFF_HUNK)
    } else {
        Style::new()
    };
    Line::from(Span::styled(line.to_string(), style))
}

fn highlight(body: &str, language: &str) -> Vec<Line<'static>> {
    let Some(syntax) = syntaxes().find_syntax_by_token(language) else {
        return plain(body);
    };
    let mut highlighter = HighlightLines::new(syntax, theme());
    let set = syntaxes();
    let mut lines = Vec::new();
    for line in syntect::util::LinesWithEndings::from(body) {
        let ranges = highlighter.highlight_line(line, set).unwrap_or_default();
        let spans: Vec<Span<'static>> = ranges
            .iter()
            .map(|(style, text)| {
                Span::styled(
                    text.trim_end_matches('\n').to_string(),
                    Style::new().fg(Color::Rgb(
                        style.foreground.r,
                        style.foreground.g,
                        style.foreground.b,
                    )),
                )
            })
            .collect();
        lines.push(Line::from(spans));
    }
    lines
}

/// The panel's edge: the theme's faint, on the panel it is drawn over.
fn edge() -> Style {
    theme::faint().patch(theme::surface())
}

fn top(language: Option<&str>, width: usize) -> Line<'static> {
    let title = language
        .filter(|token| !token.is_empty())
        .map(|token| format!(" {} ", clip(token, width.saturating_sub(5))))
        .unwrap_or_default();
    let used = 2 + title.chars().count() + 1;
    Line::from(Span::styled(
        format!("╭─{title}{}╮", "─".repeat(width.saturating_sub(used))),
        edge(),
    ))
}

fn bottom(width: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(width.saturating_sub(2))),
        edge(),
    ))
}

/// One row of code between the panel's edges, padded so the right edge is a
/// column and not a suggestion.
fn body_row(row: &Line<'static>, width: usize) -> Line<'static> {
    let surface = surface_of(row);
    let edge = theme::faint().patch(surface);
    let content = width.saturating_sub(GUTTER.chars().count() + EDGE.chars().count());
    let glyphs = cut(glyphsof(row, surface), content, surface);
    let used: usize = glyphs
        .iter()
        .map(|(ch, _)| UnicodeWidthChar::width(*ch).unwrap_or(0))
        .sum();
    let mut spans = vec![Span::styled(GUTTER.to_string(), edge)];
    spans.extend(runs(glyphs));
    if content > used {
        spans.push(Span::styled(" ".repeat(content - used), surface));
    }
    spans.push(Span::styled(EDGE.to_string(), edge));
    Line::from(spans)
}

/// A row with no panel to sit in: cut to the width, marked the same way.
fn bare(row: &Line<'static>, width: usize) -> Line<'static> {
    let surface = surface_of(row);
    Line::from(runs(cut(glyphsof(row, surface), width, surface)))
}

/// The surface a row sits on: the panel it is drawn over, or the wash a diff
/// row brought with it, so the gutter and the fill agree with the row.
fn surface_of(row: &Line<'static>) -> Style {
    Style::new().bg(row
        .spans
        .first()
        .and_then(|span| span.style.bg)
        .unwrap_or(theme::PANEL))
}

/// Every glyph of the row, on that surface whatever its foreground.
fn glyphsof(row: &Line<'static>, surface: Style) -> Vec<Glyph> {
    row.spans
        .iter()
        .flat_map(|span| {
            let style = span.style.patch(surface);
            span.content.chars().map(move |ch| (ch, style))
        })
        .collect()
}

/// The first `width` columns, and an ellipsis instead of the rest: a line the
/// panel could not hold says it was cut.
fn cut(glyphs: Vec<Glyph>, width: usize, surface: Style) -> Vec<Glyph> {
    let width = width.max(1);
    let total: usize = glyphs
        .iter()
        .map(|(ch, _)| UnicodeWidthChar::width(*ch).unwrap_or(0))
        .sum();
    if total <= width {
        return glyphs;
    }
    let mut kept = Vec::new();
    let mut used = 0;
    for (ch, style) in glyphs {
        let cells = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cells > width.saturating_sub(1) {
            break;
        }
        used += cells;
        kept.push((ch, style));
    }
    kept.push(('…', theme::faint().patch(surface)));
    kept
}

/// Glyphs back into spans, one per run of one style.
fn runs(glyphs: Vec<Glyph>) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut run_style = Style::default();
    for (ch, style) in glyphs {
        if style != run_style && !run.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut run), run_style));
        }
        run_style = style;
        run.push(ch);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, run_style));
    }
    spans
}
