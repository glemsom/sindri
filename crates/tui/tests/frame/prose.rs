//! The prose seam, read off the cells the frame painted: a heading scale, a
//! link that keeps its target, a fence whose edge names its language, and a
//! code line too long for the column saying so.

use super::*;

use ratatui::buffer::Buffer;
use sindri_tui::theme;

fn answered(text: &str) -> Buffer {
    let mut app = app();
    app.event(Event::TextDelta {
        turn: 1,
        text: text.to_string(),
    });
    buffer(&mut app, 100, 30)
}

fn rows(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// The first cell of `needle` on its row, so a style is read where the words
/// are rather than where they were composed.
fn cell_of(shown: &[String], row: usize, needle: &str) -> (u16, u16) {
    let column = shown[row]
        .find(needle)
        .unwrap_or_else(|| panic!("row {row} does not say {needle:?}: {shown:#?}"));
    (column as u16, row as u16)
}

const PROSE: &str = "# Heading one

## Heading two

A [link](https://example.com/x) in prose.

```rust
fn main() {}
```

```text
the-quick-brown-fox-jumps-over-the-lazy-dog-and-keeps-on-running-past-the-end
```
";

#[test]
fn a_heading_scale_a_link_a_fences_title_and_a_cut_line_all_reach_the_cells() {
    let buf = answered(PROSE);
    let shown = rows(&buf);

    // A heading scale: H1 and H2 are HEADING, and the shallower one rules more
    // of the column beneath it.
    let one = row_of(&shown, "Heading one");
    let two = row_of(&shown, "Heading two");
    assert_eq!(
        buf[cell_of(&shown, one, "Heading one")].fg,
        theme::HEADING,
        "a heading is painted as one: {shown:#?}"
    );
    let rule = |row: usize| shown[row].chars().filter(|ch| *ch == '─').count();
    assert!(rule(one + 1) > 0, "H1 rules beneath itself: {shown:#?}");
    assert!(
        rule(one + 1) > rule(two + 1) && rule(two + 1) > 0,
        "and H2 rules a smaller share of it: {shown:#?}"
    );

    // A link keeps its words and its target, in two weights.
    let link = row_of(&shown, "https://example.com/x");
    assert_eq!(
        buf[cell_of(&shown, link, "link")].fg,
        theme::PROMPT,
        "a link's words are the accent: {shown:#?}"
    );
    assert_eq!(
        buf[cell_of(&shown, link, "https://")].fg,
        theme::FAINT,
        "its target is dimmed beside them: {shown:#?}"
    );

    // A fence's top edge carries the language the fence named.
    assert!(seen(&shown, "╭─ rust "), "the rust title: {shown:#?}");
    assert!(seen(&shown, "╭─ text "), "the text title: {shown:#?}");

    // A line wider than the panel is cut and says so, in the panel's edge.
    let long = row_of(&shown, "the-quick");
    assert!(
        shown[long].contains('…'),
        "an over-long line is marked cut: {shown:#?}"
    );
    assert!(
        !seen(&shown, "past-the-end"),
        "and the rest of it is not silently clipped in: {shown:#?}"
    );
}

fn seen(shown: &[String], needle: &str) -> bool {
    shown.iter().any(|row| row.contains(needle))
}
