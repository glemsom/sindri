//! The markdown seam: what an answer *looks like* once rendered — a table with
//! aligned columns, code with truecolour spans, a diff coloured by line. These
//! are assertions about what the user sees, never about how it was composed
//! (ADR-0011 §6).

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use ratatui::style::{Color, Modifier};
use ratatui::text::Line;
use sindri_tui::markdown;

/// The visible text of a line, styles dropped.
fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn texts(lines: &[Line<'_>]) -> Vec<String> {
    lines.iter().map(text).collect()
}

/// Every distinct foreground colour on the line.
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
    let lines = markdown::lines(source, 60);
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
    let lines = markdown::lines(source, 60);
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
    let lines = markdown::lines(source, 60);
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

#[test]
fn a_diff_is_coloured_by_line() {
    let source =
        "```diff\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n same\n```\n";
    let lines = markdown::lines(source, 60);
    let added = lines
        .iter()
        .find(|line| text(line) == "+new")
        .expect("the added line");
    assert_eq!(
        colours(added)[0],
        Color::Rgb(0x6a, 0xc9, 0x7a),
        "additions are green"
    );
    let removed = lines
        .iter()
        .find(|line| text(line) == "-old")
        .expect("the removed line");
    assert_eq!(
        colours(removed)[0],
        Color::Rgb(0xe0, 0x6c, 0x75),
        "removals are red"
    );
    let hunk = lines
        .iter()
        .find(|line| text(line).starts_with("@@"))
        .expect("the hunk header");
    assert_eq!(
        colours(hunk)[0],
        Color::Rgb(0x61, 0xaf, 0xef),
        "hunks are blue"
    );
}

#[test]
fn long_lines_wrap_at_the_column_width_and_lose_nothing() {
    let source = "the quick brown fox jumps over the lazy dog and keeps running well past the edge";
    let lines = markdown::lines(source, 30);
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
