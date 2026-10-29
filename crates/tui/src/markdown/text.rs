//! Turning styled spans into rows that fit a column: wrapping is a display
//! decision, so nothing is dropped and nothing is reworded.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

/// One character and the style it carries.
pub type Glyph = (char, Style);

/// Flattens spans into glyphs, keeping every character and its style.
pub fn glyphs(spans: &[Span<'static>]) -> Vec<Glyph> {
    spans
        .iter()
        .flat_map(|span| {
            let style = span.style;
            span.content.chars().map(move |ch| (ch, style))
        })
        .collect()
}

/// Wraps glyphs to `width` columns, breaking at the last space of a row and
/// mid-word only when a word cannot fit. Every row carries `prefix` in
/// `prefix_style`, which does not count against the width of the row.
pub fn wrap(
    glyphs: Vec<Glyph>,
    prefix: &str,
    prefix_style: Style,
    width: usize,
) -> Vec<Line<'static>> {
    let mut rows: Vec<Line<'static>> = Vec::new();
    let mut row: Vec<Glyph> = Vec::new();
    let mut used = 0usize;
    let width = width.max(1);

    for (ch, style) in glyphs {
        if ch == '\n' {
            rows.push(line(std::mem::take(&mut row), prefix, prefix_style));
            used = 0;
            continue;
        }
        let cells = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cells > width && !row.is_empty() {
            let at = row
                .iter()
                .rposition(|(ch, _)| *ch == ' ')
                .unwrap_or(row.len());
            let mut rest = row.split_off(at.min(row.len()));
            while rest.first().is_some_and(|(ch, _)| *ch == ' ') {
                rest.remove(0);
            }
            rows.push(line(std::mem::take(&mut row), prefix, prefix_style));
            row = rest;
            used = row
                .iter()
                .map(|(ch, _)| UnicodeWidthChar::width(*ch).unwrap_or(0))
                .sum();
        }
        row.push((ch, style));
        used += cells;
    }
    if !row.is_empty() {
        rows.push(line(row, prefix, prefix_style));
    }
    if rows.is_empty() {
        rows.push(Line::default());
    }
    rows
}

/// The first `width` display columns of `text`, cut rather than wrapped: for a
/// row that shows an excerpt of something longer. A character is never split,
/// so a wide glyph is either wholly in or wholly out.
pub fn clip(text: &str, width: usize) -> String {
    let mut used = 0;
    let mut shown = String::new();
    for ch in text.chars() {
        let cells = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cells > width {
            break;
        }
        used += cells;
        shown.push(ch);
    }
    shown
}

/// One row of glyphs as a line, trailing spaces dropped and equal styles joined.
fn line(mut glyphs: Vec<Glyph>, prefix: &str, prefix_style: Style) -> Line<'static> {
    while glyphs.last().is_some_and(|(ch, _)| *ch == ' ') {
        glyphs.pop();
    }
    let mut spans: Vec<Span<'static>> = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::styled(prefix.to_string(), prefix_style));
    }
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
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::{glyphs, wrap};
    use ratatui::style::{Color, Style};
    use ratatui::text::Span;

    #[test]
    fn a_row_with_no_glyphs_is_a_blank_line() {
        let rows = wrap(Vec::new(), "│ ", Style::new().fg(Color::DarkGray), 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].spans.len(), 0, "a blank row carries no prefix");
    }

    #[test]
    fn a_word_wider_than_the_column_is_cut_rather_than_overflowing() {
        let spans = vec![Span::styled("abcdefghijkl", Style::default())];
        let rows = wrap(glyphs(&spans), "", Style::default(), 5);
        let shown: Vec<String> = rows
            .iter()
            .map(|row| row.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(shown, vec!["abcde", "fghij", "kl"]);
    }
}
