//! Fenced code: the language's own colours where the language is known, and a
//! diff coloured line by line — an agent emits code and diffs more than
//! anything else (ADR-0011 §6).

use std::sync::OnceLock;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::theme;

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

/// Whether the fence names a diff rather than a language.
fn is_diff(language: &str) -> bool {
    matches!(language, "diff" | "patch" | "udiff")
}

/// One fenced block, as rows. A fence with no language keeps its own text and
/// no colour rather than guessing.
pub fn lines(body: &str, language: Option<&str>) -> Vec<Line<'static>> {
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

/// One diff row, coloured by what it does rather than by what it says.
fn diff_line(line: &str) -> Line<'static> {
    let style = if line.starts_with("+++") || line.starts_with("---") {
        Style::new().fg(theme::DIFF_FILE)
    } else if line.starts_with('+') {
        Style::new().fg(theme::DIFF_ADDED)
    } else if line.starts_with('-') {
        Style::new().fg(theme::DIFF_REMOVED)
    } else if line.starts_with("@@") {
        Style::new().fg(theme::DIFF_HUNK)
    } else {
        Style::new()
    };
    Line::from(Span::styled(line.to_string(), style))
}

/// A known language's own tokens, at truecolour. An unknown language is shown
/// plainly: a wrong highlight is worse than no highlight.
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
