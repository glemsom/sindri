//! The markdown seam: what an answer *looks like* once rendered — a table
//! with aligned columns, code with truecolour spans, a diff coloured by line.
//! These are assertions about what the user sees, never about how it was
//! composed.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use ratatui::style::{Color, Modifier};
use ratatui::text::Line;
use sindri_tui::markdown;

fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// The rows of an answer with no picture in it: the same writer a frame uses,
/// handed no store, because these are assertions about prose.
fn lines(source: &str, width: u16) -> Vec<Line<'static>> {
    markdown::lines(source, width, None).rows
}

fn texts(lines: &[Line<'_>]) -> Vec<String> {
    lines.iter().map(text).collect()
}

fn raw(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn colours(line: &Line<'_>) -> Vec<Color> {
    line.spans
        .iter()
        .map(|span| span.style.fg.unwrap_or(Color::Reset))
        .collect()
}

#[test]
fn headings_lists_quotes_and_inline_code_render_as_themselves() {
    let source =
        "# Title\n\nSome *emphasis* and `inline`.\n\n- one\n  - nested\n- two\n\n> quoted\n";
    let lines = lines(source, 60);
    let shown = texts(&lines);
    assert!(shown.contains(&"Title".to_string()), "{shown:?}");
    let title = lines
        .iter()
        .find(|line| text(line) == "Title")
        .expect("the heading line");
    assert!(
        title.spans[0].style.add_modifier.contains(Modifier::BOLD),
        "a heading is bold"
    );
    assert!(
        shown.iter().any(|line| line.contains("• one")),
        "an unordered item is a bullet: {shown:?}"
    );
    assert!(
        shown.iter().any(|line| line.starts_with("  • nested")),
        "a nested item is indented: {shown:?}"
    );
    assert!(
        shown.iter().any(|line| line.starts_with("│ quoted")),
        "a quote is marked: {shown:?}"
    );
    let inline = lines
        .iter()
        .find(|line| text(line).contains("inline"))
        .expect("the inline-code line");
    assert!(
        inline
            .spans
            .iter()
            .any(|span| span.content.as_ref() == "inline"
                && span.style.fg == Some(Color::Rgb(0xe5, 0xc0, 0x7b))),
        "inline code is coloured: {inline:?}"
    );
}

#[test]
fn a_table_renders_its_cells_in_aligned_columns() {
    let source = "| file | change |\n| --- | ---: |\n| src/lib.rs | +12 |\n| Cargo.toml | -3 |\n";
    let lines = lines(source, 60);
    let shown = texts(&lines);
    let header = shown
        .iter()
        .find(|line| line.contains("file"))
        .expect("the header row");
    assert!(header.contains("change"), "both header cells: {shown:?}");
    let first = shown
        .iter()
        .find(|line| line.contains("src/lib.rs"))
        .expect("the first row");
    assert!(first.contains("+12"), "the cells of a row stay together");
    let column = header.find("change").expect("header column");
    let value = first.find("+12").expect("value column");
    assert!(
        column <= value,
        "the right-hand column is aligned under its header: {shown:?}"
    );
    let header_line = lines
        .iter()
        .find(|line| text(line).contains("file"))
        .expect("the header line");
    assert!(
        header_line.spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD),
        "a header row is bold"
    );
}

#[test]
fn fenced_code_is_highlighted_in_truecolour() {
    let source = "```rust\nfn main() { let x = 1; }\n```\n";
    let lines = lines(source, 60);
    let code = lines
        .iter()
        .find(|line| text(line).contains("fn main"))
        .expect("the code line");
    let colours = colours(code);
    assert!(
        colours
            .iter()
            .any(|colour| matches!(colour, Color::Rgb(..))),
        "a language with a syntax is highlighted in truecolour: {code:?}"
    );
    assert!(
        colours
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1,
        "more than one token is coloured: {code:?}"
    );
}

/// A fence's code is painted in the theme's seven roles and nothing else, so
/// a borrowed theme's grey cannot come back: which scope is which role is the
/// language's business, and every hex is the theme's. `tests/theme.rs` holds
/// the same fence to `theme::owns`, cell by cell.
#[test]
fn a_fence_is_painted_in_the_themes_own_roles() {
    let source = "```rust\n// a note\nfn main() -> Result<(), Box<dyn Error>> {\n    \
                  let answer: u32 = 42;\n    println!(\"answer {}\", answer);\n    Ok(())\n}\n```\n";
    let lines = lines(source, 60);
    let roles = [
        sindri_tui::theme::PLAIN,
        sindri_tui::theme::COMMENT,
        sindri_tui::theme::KEYWORD,
        sindri_tui::theme::FUNCTION,
        sindri_tui::theme::TYPE,
        sindri_tui::theme::NUMBER,
        sindri_tui::theme::STRING,
    ];
    // A body row is the gutter, the code, the fill that squares it and the
    // edge: the code is what is between the two, and it is a role.
    let body = &lines[1..lines.len() - 1];
    for row in body {
        let code = &row.spans[1..row.spans.len() - 1];
        for span in code.iter().filter(|span| span.style.fg.is_some()) {
            let fg = span.style.fg;
            assert!(
                fg.is_some_and(|fg| roles.contains(&fg)),
                "{:?} is painted in {fg:?}, which is not a role the theme names",
                span.content
            );
        }
    }
    // The role a run of characters is painted in, by the words it spells.
    let role = |needle: &str| {
        let span = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.contains(needle))
            .unwrap_or_else(|| panic!("no run spells {needle:?}: {:?}", lines));
        span.style.fg
    };
    assert_eq!(role("// a note"), Some(sindri_tui::theme::COMMENT));
    assert_eq!(role("fn"), Some(sindri_tui::theme::KEYWORD));
    assert_eq!(role("42"), Some(sindri_tui::theme::NUMBER));
    assert_eq!(role("\"answer {}\""), Some(sindri_tui::theme::STRING));
    assert_eq!(role("main"), Some(sindri_tui::theme::FUNCTION));
    assert_eq!(
        role(", answer);"),
        Some(sindri_tui::theme::PLAIN),
        "a variable is plain ink: a role the theme names, and the one an \
         unrecognised scope falls back to"
    );
}

#[test]
fn a_diff_is_coloured_by_line() {
    let source =
        "```diff\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n same\n```\n";
    let lines = lines(source, 60);
    // The fence is a panel now, so each row carries its gutter as well as its
    // line: the line's own colour is the one the gutter does not have.
    let added = lines
        .iter()
        .find(|line| text(line).contains("+new"))
        .expect("the added line");
    assert!(
        colours(added).contains(&Color::Rgb(0x6a, 0xc9, 0x7a)),
        "additions are green: {added:?}"
    );
    let removed = lines
        .iter()
        .find(|line| text(line).contains("-old"))
        .expect("the removed line");
    assert!(
        colours(removed).contains(&Color::Rgb(0xe0, 0x6c, 0x75)),
        "removals are red: {removed:?}"
    );
    let hunk = lines
        .iter()
        .find(|line| text(line).contains("@@"))
        .expect("the hunk header");
    assert!(
        colours(hunk).contains(&Color::Rgb(0x61, 0xaf, 0xef)),
        "hunks are blue: {hunk:?}"
    );
}

#[test]
fn long_lines_wrap_at_the_column_width_and_lose_nothing() {
    let source = "the quick brown fox jumps over the lazy dog and keeps running well past the edge";
    let lines = lines(source, 30);
    assert!(lines.len() > 1, "a long paragraph takes more than one row");
    assert!(
        lines.iter().all(|line| text(line).chars().count() <= 30),
        "no row is wider than the column: {:?}",
        texts(&lines)
    );
    let joined = texts(&lines).join(" ");
    assert_eq!(
        joined, source,
        "wrapping is a display decision, not an edit"
    );
}

#[test]
fn a_fence_is_a_panel_with_a_gutter_and_a_border_that_says_where_it_ends() {
    let source = "```diff\n--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n```\n";
    let lines = lines(source, 40);
    let shown: Vec<String> = lines.iter().map(raw).collect();

    assert!(
        shown[0].starts_with('╭') && shown[0].ends_with('╮'),
        "the panel's top edge is rounded and names the language: {shown:?}"
    );
    assert!(shown[0].contains(" diff "), "{shown:?}");
    let last = shown.last().expect("the panel's bottom edge");
    assert!(
        last.starts_with('╰') && last.ends_with('╯'),
        "the panel closes: {shown:?}"
    );
    for row in &shown[1..shown.len() - 1] {
        assert!(
            row.starts_with("│ ") && row.ends_with(" │"),
            "every row of code sits in the gutter, flush against both edges: {row:?}"
        );
    }
    assert!(
        shown.iter().all(|row| row.chars().count() == 40),
        "the panel is one rectangle at the column's width: {shown:?}"
    );
    // A changed row brings its own wash, a step off the panel; every other
    // span is the panel itself. Nothing is left on the terminal's black.
    let surfaces = [
        sindri_tui::theme::PANEL,
        sindri_tui::theme::DIFF_ADDED_BG,
        sindri_tui::theme::DIFF_REMOVED_BG,
    ];
    assert!(
        lines
            .iter()
            .flatten()
            .all(|span| span.style.bg.is_some_and(|bg| surfaces.contains(&bg))),
        "the panel is filled, so the text sits on a surface and not on the terminal: {lines:?}"
    );
}

/// Below `MIN_PANEL` columns a panel would be all border and no code, so the
/// fence gives up its frame rather than its lines: the ink changes with the
/// theme, the rows it is drawn on do not.
#[test]
fn a_fence_too_narrow_for_a_panel_falls_back_to_bare_rows() {
    let source = "```rust\nfn main() { let answer = 42; }\n```\n";
    let lines = lines(source, 11);
    let shown: Vec<String> = lines.iter().map(raw).collect();

    assert!(
        !shown
            .iter()
            .any(|row| row.contains('╭') || row.contains('│')),
        "no frame, no gutter: {shown:?}"
    );
    assert!(
        shown.iter().all(|row| row.chars().count() <= 11),
        "every row is cut to the column: {shown:?}"
    );
    assert!(
        lines
            .iter()
            .flatten()
            .any(|span| span.style.fg == Some(sindri_tui::theme::KEYWORD)),
        "and the language is still highlighted: {lines:?}"
    );
}

#[test]
fn a_fenced_line_wider_than_the_panel_is_cut_and_says_so() {
    let source = "```\nthe-quick-brown-fox-jumps-over-the-lazy-dog-again-and-again\nshort\n```\n";
    let lines = lines(source, 24);
    let shown: Vec<String> = lines.iter().map(raw).collect();
    let long = shown
        .iter()
        .find(|row| row.contains("the-quick"))
        .expect("the long line");

    assert!(
        long.contains('…'),
        "a line that did not fit says so rather than ending mid-word: {shown:?}"
    );
    assert!(
        !shown.iter().any(|row| row.contains("again")),
        "the rest of it is not on the next row: it was cut, not wrapped: {shown:?}"
    );
    assert!(
        shown.iter().all(|row| row.chars().count() == 24),
        "and the panel is still a rectangle: {shown:?}"
    );
}

mod pictures;
mod prose;
