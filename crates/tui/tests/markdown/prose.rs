//! The prose this ticket is about: a link that keeps its target, a heading
//! scale ranked by level, a fence's language title, and a diff washed as well
//! as coloured (ADR-0011 §24).

use super::*;

use ratatui::style::Modifier;

#[test]
fn a_link_keeps_its_text_and_the_target_it_points_at() {
    let source = "see [the docs](https://example.com/x) for more";
    let lines = markdown::lines(source, 60);
    let shown = texts(&lines);
    let row = lines
        .iter()
        .find(|line| text(line).contains("the docs"))
        .expect("the link's own words");

    assert!(
        colours(row).contains(&sindri_tui::theme::PROMPT),
        "a link's words are the frame's accent: {row:?}"
    );
    assert!(
        shown
            .iter()
            .any(|line| line.contains("https://example.com/x")),
        "and the target is not thrown away: {shown:?}"
    );
    let target = row
        .spans
        .iter()
        .find(|span| span.content.contains("https://example.com/x"))
        .expect("the target beside the words");
    assert_eq!(
        target.style.fg,
        Some(sindri_tui::theme::FAINT),
        "the target is dimmed so the words still lead: {row:?}"
    );
}

#[test]
fn a_link_with_no_target_is_its_text_and_no_bracket_at_all() {
    let source = "a [plain words]() here";
    let lines = markdown::lines(source, 60);
    let shown = texts(&lines).join(" ");

    assert!(shown.contains("plain words"), "{shown:?}");
    assert!(
        !shown.contains('[') && !shown.contains(']') && !shown.contains('‹'),
        "a link with nowhere to go is the words and nothing else: {shown:?}"
    );
    let row = lines
        .iter()
        .find(|line| text(line).contains("plain words"))
        .expect("the words themselves");
    assert!(
        !colours(row).contains(&sindri_tui::theme::PROMPT),
        "and they are not dressed as a link that leads somewhere: {row:?}"
    );
}

/// How heavy a heading is, as the rows say it: bold counts, and so does the
/// rule beneath — a heading's rank is read off the frame, not off the source.
fn rank(lines: &[Line<'_>], needle: &str) -> (bool, usize) {
    let index = lines
        .iter()
        .position(|line| text(line).contains(needle))
        .unwrap_or_else(|| panic!("no heading says {needle:?}: {:?}", texts(lines)));
    let bold = lines[index].spans[0]
        .style
        .add_modifier
        .contains(Modifier::BOLD);
    let rule = lines
        .get(index + 1)
        .filter(|line| text(line).chars().all(|ch| ch == '─'))
        .map_or(0, |line| text(line).chars().count());
    (bold, rule)
}

#[test]
fn headings_are_ranked_by_their_level() {
    let source = "# One\n\n## Two\n\n### Three\n\n#### Four\n";
    let lines = markdown::lines(source, 40);

    for id in ["One", "Two", "Three", "Four"] {
        let line = lines
            .iter()
            .find(|line| text(line) == id)
            .unwrap_or_else(|| panic!("{id} is a heading: {:?}", texts(&lines)));
        assert_eq!(
            line.spans[0].style.fg,
            Some(sindri_tui::theme::HEADING),
            "{id} is painted as a heading"
        );
    }

    let (one, two, three, four) = (
        rank(&lines, "One"),
        rank(&lines, "Two"),
        rank(&lines, "Three"),
        rank(&lines, "Four"),
    );
    assert!(one > two, "H1 outranks H2: {one:?} vs {two:?}");
    assert!(two > three, "H2 outranks H3: {two:?} vs {three:?}");
    assert_eq!(
        three, four,
        "H3 and deeper share one rank: {three:?} vs {four:?}"
    );
    assert!(one.1 > 0, "H1 rules the column beneath it");
}

#[test]
fn a_heading_in_a_quote_rules_inside_the_quote() {
    let lines = markdown::lines("> ## Quoted\n", 40);
    let shown = texts(&lines);
    let head = lines
        .iter()
        .position(|line| text(line).contains("Quoted"))
        .expect("the heading inside the quote");
    assert!(shown[head].starts_with("│ "), "{shown:?}");
    assert!(
        shown[head + 1].starts_with("│ ─"),
        "the rule sits in the quote it belongs to: {shown:?}"
    );
    assert!(
        !shown[head + 1].contains("──│"),
        "and does not run across the quote's marker: {shown:?}"
    );
}

#[test]
fn a_fence_that_names_no_language_carries_no_title() {
    let source = "```\nplain words\n```\n";
    let lines = markdown::lines(source, 30);
    let shown: Vec<String> = lines.iter().map(raw).collect();
    let top = &shown[0];

    assert!(top.starts_with('╭') && top.ends_with('╮'), "{shown:?}");
    assert!(
        top.chars().all(|ch| "╭─╮".contains(ch)),
        "an unnamed fence's edge says nothing but its edge: {shown:?}"
    );
}

#[test]
fn a_diff_row_is_tinted_as_well_as_coloured() {
    let source = "```diff\n--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n same\n```\n";
    let lines = markdown::lines(source, 40);
    let row = |needle: &str| {
        lines
            .iter()
            .find(|line| text(line).contains(needle))
            .unwrap_or_else(|| panic!("the {needle} row: {:?}", texts(&lines)))
    };

    let added = row("+new");
    assert!(
        added
            .spans
            .iter()
            .any(|span| span.style.fg == Some(sindri_tui::theme::DIFF_ADDED)),
        "an addition keeps its own hue: {added:?}"
    );
    assert!(
        added
            .spans
            .iter()
            .any(|span| span.style.bg == Some(sindri_tui::theme::DIFF_ADDED_BG)),
        "and is tinted, so it is the added row in a terminal that lost its green: {added:?}"
    );
    let removed = row("-old");
    assert!(
        removed
            .spans
            .iter()
            .any(|span| span.style.bg == Some(sindri_tui::theme::DIFF_REMOVED_BG)),
        "a removal is tinted the other way: {removed:?}"
    );
    let same = row(" same");
    assert!(
        text(same).starts_with("│  same"),
        "an untouched row is in the gutter like the rest: {same:?}"
    );
    assert!(
        same.spans
            .iter()
            .all(|span| span.style.bg == Some(sindri_tui::theme::PANEL)),
        "and it stays on the panel, with no tint of its own: {same:?}"
    );
    assert!(
        text(row("+new")).contains("+new"),
        "the glyph is the fact, the tint only says it faster: {:?}",
        texts(&lines)
    );
}
