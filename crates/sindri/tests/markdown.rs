//! Markdown laid out as the rows a transcript draws, one construct at a time: `pulldown-cmark` finds
//! the constructs and this front end decides what each of them looks like, so a heading's weight and
//! a code block's gutter are held here; a whole screen of them is `frames`'.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use sindri::graphics::CellSize;
use sindri::ink::{Face, Faces, Ink, Look};
use sindri::link::Links;
use sindri::markdown;
use sindri::palette::{Band, Palette};
use sindri::picture::Surface;
use sindri::rows::Row;
use sindri::size::Sizes;

fn words(row: &Row) -> String {
    row.spans().iter().map(|(text, _)| text.as_str()).collect()
}

fn drawn(rows: &[Row]) -> Vec<String> {
    rows.iter().map(words).collect()
}

fn looks(row: &Row) -> Vec<Look> {
    row.spans().iter().map(|(_, look)| *look).collect()
}

fn wears(row: &Row, word: &str, look: Look) -> bool {
    row.spans()
        .iter()
        .any(|(said, worn)| said == word && *worn == look)
}

/// One level of heading as a reader meets it: the look its words wear, and the rule it carries
/// under them.
fn heading_block(level: usize) -> (Look, Option<String>) {
    let said = format!("{} L{level}", "#".repeat(level));
    let word = format!("L{level}");
    let rows = markdown::rows(&said, 40);
    let row = rows
        .iter()
        .find(|row| words(row).contains(&word))
        .unwrap_or_else(|| panic!("the row an h{level} is said on: {}", drawn(&rows).join("|")));
    let looked: Vec<Look> = row
        .spans()
        .iter()
        .filter(|(said, _)| said == &word)
        .map(|(_, look)| *look)
        .collect();
    assert_eq!(
        looked.len(),
        1,
        "the heading's own words wear one look: {row:?}"
    );
    let rule = drawn(&rows)
        .into_iter()
        .find(|row| row.chars().all(|glyph| glyph == '═' || glyph == '─'));
    (looked[0], rule)
}

#[test]
fn no_level_of_a_heading_is_drawn_as_the_prose_around_it() {
    let prose = Look::plain(Ink::Plain);

    for level in 1..=6 {
        assert_ne!(
            heading_block(level).0,
            prose,
            "an h{level} is drawn as the prose around it, so its level says nothing"
        );
    }
}

#[test]
fn the_scale_of_headings_runs_from_bold_prose_to_dim_bold() {
    let blocks: Vec<(Look, Option<String>)> = (1..=6).map(heading_block).collect();
    let scale: Vec<Look> = blocks.iter().map(|(look, _)| *look).collect();

    for (at, pair) in blocks.windows(2).enumerate() {
        assert_ne!(
            pair[0],
            pair[1],
            "a level {} and the level under it are drawn the same way: {blocks:?}",
            at + 1
        );
    }
    assert_eq!(
        scale[2],
        Look::new(Ink::Muted, Face::Bold),
        "the third level is muted and bold: {scale:?}"
    );
    assert_eq!(
        scale[3],
        Look::plain(Ink::Muted),
        "the fourth is muted and no weight: {scale:?}"
    );
    for (at, look) in scale.iter().enumerate().skip(4) {
        assert_eq!(look.ink(), Ink::Dim, "an h{} is dim: {scale:?}", at + 1);
        assert!(
            look.faces().has(Face::Bold),
            "an h{} is dim and bold: {scale:?}",
            at + 1
        );
    }
    assert!(
        !scale[4].faces().has(Face::Italic) && scale[5].faces().has(Face::Italic),
        "the lowest level is the one worn slanted, so it and the level over it are not one \
         look: {scale:?}"
    );
}

#[test]
fn an_h1_stands_under_one_blank_row_of_its_own() {
    let rows = markdown::rows("prose first\n\n# One", 40);

    assert_eq!(
        drawn(&rows),
        ["prose first", "", "One", "═══"],
        "the one blank row above an h1 is the block's, and the rule is the h1's own"
    );
}

#[test]
fn an_answer_that_opens_on_an_h1_opens_on_a_blank_row() {
    let rows = markdown::rows("# The fence rule", 40);
    assert_eq!(drawn(&rows)[..2], ["", "The fence rule"]);
    assert_eq!(rows[0].width(), 0, "the blank row says nothing at all");
}

#[test]
fn the_two_highest_levels_carry_a_rule_as_wide_as_their_own_words() {
    let h1 = drawn(&markdown::rows("# The fence rule", 40));
    assert_eq!(
        h1,
        ["", "The fence rule", "══════════════"],
        "the highest level is ruled under, as wide as the words above it"
    );

    let h2 = drawn(&markdown::rows("## A row is one line", 40));
    assert_eq!(
        h2,
        ["A row is one line", "─────────────────"],
        "the level under it carries a rule of its own, and one of a single glyph, so the two \
         are drawn apart"
    );

    for level in 3..=6 {
        let rows = drawn(&markdown::rows(&format!("{} low", "#".repeat(level)), 40));
        assert_eq!(
            rows,
            ["low"],
            "a level this low is told apart by its ink and carries no rule"
        );
    }
}

#[test]
fn a_rule_is_no_wider_than_the_room_the_heading_had() {
    let rows = markdown::rows("## a heading too wide for one row of this room", 20);

    for row in &rows {
        assert!(
            row.width() <= 20,
            "a row ran past the room it was laid out for: {row:?}"
        );
    }
    let rule = rows
        .iter()
        .find(|row| words(row).starts_with('─'))
        .expect("the rule under the heading");
    assert_eq!(
        rule.width(),
        19,
        "a wrapped heading is ruled by its widest row, which the room held: {rows:?}"
    );
}

#[test]
fn a_heading_in_a_quote_keeps_the_bar_and_carries_its_rule_behind_it() {
    let rows = markdown::rows("> ## a heading behind a bar", 40);

    assert_eq!(
        drawn(&rows),
        ["▏ a heading behind a bar", "▏ ──────────────────────",],
        "the rule is the heading's own width and stands behind the quote's bar"
    );
    for row in &rows {
        assert_eq!(
            row.spans().first(),
            Some(&(String::from("▏ "), Look::plain(Ink::Dim))),
            "every row of the quote opens with its bar: {row:?}"
        );
    }
}

#[test]
fn a_heading_that_opens_an_item_hangs_its_rule_under_the_items_gutter() {
    let rows = markdown::rows("- ## an item's heading", 40);

    assert_eq!(
        drawn(&rows),
        ["• an item's heading", "  ─────────────────"],
        "the rule hangs under the words, past the bullet's own columns"
    );
}

#[test]
fn inline_code_is_reversed_over_muted() {
    let rows = markdown::rows("run `make check` first", 80);

    assert_eq!(drawn(&rows), ["run make check first"]);
    assert!(
        wears(
            &rows[0],
            "make check",
            Look::new(Ink::Muted, Face::Reversed)
        ),
        "a run of inline code is reversed over `muted`, so that it is not the mark's \
         own look: {:?}",
        rows[0].spans()
    );
}

/// The line a way's words are drawn under.
fn way() -> Look {
    Look::plain(Ink::Plain).underlined(Ink::Dim)
}

/// The ways `rows` name, painted one to a frame row from its first.
fn ways(rows: &[Row]) -> Links {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 120, u16::try_from(rows.len()).unwrap()));
    let mut links = Links::none();
    for (at, row) in rows.iter().enumerate() {
        let area = Rect::new(0, u16::try_from(at).unwrap(), 120, 1);
        row.paint(
            area,
            &mut buffer,
            &Palette::TRUECOLOR,
            &mut links,
            &mut Sizes::none(),
        );
    }
    links
}

#[test]
fn a_link_is_drawn_as_its_words_marked_and_its_destination_travels_beside_the_row() {
    let rows = markdown::rows("see [the gate](https://example.test/gate) now", 40);

    assert_eq!(
        drawn(&rows),
        ["see the gate now"],
        "the link is the words it was written as, and the URL is not said beside them"
    );
    assert!(
        wears(&rows[0], "the gate", way()),
        "the words are marked with the line a way wears, so the row reads as a way to \
         somewhere: {:?}",
        rows[0].spans()
    );

    let links = ways(&rows);
    assert_eq!(
        links.at(0, 4),
        Some("https://example.test/gate"),
        "the words' first cell opens the destination the words were written over"
    );
    assert_eq!(
        links.at(0, 11),
        Some("https://example.test/gate"),
        "and a cell the link's own space covers is the way's too"
    );
    assert_eq!(
        links.at(0, 12),
        None,
        "no cell past the words is a way there"
    );
    assert_eq!(
        links.at(0, 3),
        None,
        "nor the space before them: a way begins at its own words"
    );
}

#[test]
fn a_link_whose_words_wrap_is_one_destination_on_each_row() {
    let rows = markdown::rows(
        "see [a gate that is named over two rows](https://example.test/gate)",
        20,
    );

    assert_eq!(
        drawn(&rows),
        ["see a gate that is", "named over two rows"],
        "the words stand where they were written, and no URL follows them"
    );
    let ways = ways(&rows);
    assert_eq!(
        ways.at(0, 4),
        Some("https://example.test/gate"),
        "the row the first of the words was written on names the destination"
    );
    assert_eq!(
        ways.at(0, 17),
        Some("https://example.test/gate"),
        "and so does the cell its last word ends on"
    );
    assert_eq!(
        ways.at(0, 18),
        None,
        "and the way ends there, before the row does"
    );
    assert_eq!(
        ways.at(0, 3),
        None,
        "and the words before the link are no way there"
    );
    assert_eq!(
        ways.at(1, 0),
        Some("https://example.test/gate"),
        "the row the link was written on next carries the whole destination too, never half \
         of it"
    );
    assert_eq!(
        ways.at(1, 18),
        Some("https://example.test/gate"),
        "and its last cell is the way's own"
    );
    assert_eq!(
        ways.at(1, 19),
        None,
        "and the way ends where its last word does"
    );
}

#[test]
fn a_link_inside_a_quote_carries_its_destination_past_the_quotes_bar() {
    let rows = markdown::rows("> see [the gate](https://example.test/gate)", 40);

    assert_eq!(drawn(&rows), ["▏ see the gate"]);
    let links = ways(&rows);
    assert_eq!(
        links.at(0, 6),
        Some("https://example.test/gate"),
        "the way begins where the words it covers do, past the bar the quote is set behind"
    );
    assert_eq!(
        links.at(0, 5),
        None,
        "never on the bar the words were moved past"
    );
}

#[test]
fn a_link_a_table_holds_is_a_way_to_its_destination_as_its_words() {
    let rows = markdown::rows(
        "| a link |\n| --- |\n| [the gate](https://example.test/gate) |",
        60,
    );

    assert_eq!(
        drawn(&rows).last().map(String::as_str),
        Some("the gate"),
        "a cell is measured whole before a row of it is drawn, and it still says its words \
         alone: {rows:?}"
    );
    assert!(
        wears(rows.last().expect("a row"), "the gate", way()),
        "and the words wear the line a way wears: {:?}",
        rows.last().map(Row::spans)
    );
    let links = ways(&rows);
    assert_eq!(
        links.at(2, 0),
        Some("https://example.test/gate"),
        "the cell's first cell opens the destination the words were written over"
    );
    assert_eq!(links.at(2, 7), Some("https://example.test/gate"));
    assert_eq!(links.at(2, 8), None, "and the way ends with the words");
}

#[test]
fn a_link_a_table_in_a_quote_holds_carries_its_destination_past_the_bar() {
    let rows = markdown::rows(
        "> | a link |\n> | --- |\n> | [the gate](https://example.test/gate) |",
        60,
    );

    assert_eq!(
        drawn(&rows).last().map(String::as_str),
        Some("▏ the gate"),
        "the table keeps the quote's own gutter: {rows:?}"
    );
    assert_eq!(
        ways(&rows).at(2, 2),
        Some("https://example.test/gate"),
        "and the way the cell names is moved with its words, past the bar"
    );
    assert_eq!(
        ways(&rows).at(2, 1),
        None,
        "never onto the bar the words were moved past"
    );
}

#[test]
fn a_code_span_inside_a_link_is_a_way_to_the_destination_in_its_own_look() {
    let rows = markdown::rows("read [the `gate` word](https://example.test/gate)", 60);

    assert_eq!(drawn(&rows), ["read the gate word"]);
    assert!(
        wears(
            &rows[0],
            "gate",
            Look::new(Ink::Muted, Face::Reversed).underlined(Ink::Dim)
        ),
        "a code span keeps its own look and wears the line a way wears: {:?}",
        rows[0].spans()
    );
    let links = ways(&rows);
    assert_eq!(
        links.at(0, 9),
        Some("https://example.test/gate"),
        "and it is a way to the destination the words were written over"
    );
    assert_eq!(links.at(0, 12), Some("https://example.test/gate"));
}

#[test]
fn a_link_a_table_holds_that_spells_its_own_destination_says_it_once() {
    let rows = markdown::rows(
        "| link |\n| --- |\n| [https://x.test/s](https://x.test/s) |",
        60,
    );

    assert!(
        drawn(&rows)
            .iter()
            .all(|row| row.matches("https://x.test/s").count() <= 1),
        "the destination is said once, as the words that spell it: {rows:?}"
    );
    assert_eq!(
        ways(&rows).at(2, 0),
        Some("https://x.test/s"),
        "and those words are the way there"
    );
}

#[test]
fn a_link_whose_words_spell_its_destination_says_it_once() {
    let rows = markdown::rows(
        "read [https://example.test/spec](https://example.test/spec)",
        60,
    );

    assert_eq!(
        drawn(&rows),
        ["read https://example.test/spec"],
        "a link written over its own URL says the destination once, as its words"
    );
    assert!(
        wears(&rows[0], "https://example.test/spec", way()),
        "and the words are the way there: {:?}",
        rows[0].spans()
    );
    assert_eq!(
        ways(&rows).at(0, 5),
        Some("https://example.test/spec"),
        "so a reader opens the destination from the words that spell it"
    );
}

#[test]
fn inline_code_keeps_the_spacing_it_was_written_with() {
    let rows = markdown::rows("call `a  b` now", 40);
    assert_eq!(drawn(&rows), ["call a  b now"]);
    assert!(
        rows[0]
            .spans()
            .contains(&(String::from("a  b"), Look::new(Ink::Muted, Face::Reversed))),
        "the code's two spaces are still two: {:?}",
        rows[0].spans()
    );
}

#[test]
fn a_strong_run_is_bold_and_an_emphasis_run_is_italic_in_prose_and_in_a_list() {
    let rows = markdown::rows(
        "a **strong** word and an *emphasised* one\n\n- a **strong** item\n- an *emphasised* one",
        40,
    );

    assert_eq!(
        drawn(&rows),
        [
            "a strong word and an emphasised one",
            "• a strong item",
            "• an emphasised one"
        ],
        "the marks are drawn and not spoken"
    );
    assert!(
        wears(&rows[0], "strong", Look::new(Ink::Plain, Face::Bold)),
        "a strong word is the frame's own ink worn bold: {:?}",
        rows[0].spans()
    );
    assert!(
        wears(&rows[0], "emphasised", Look::new(Ink::Plain, Face::Italic)),
        "an emphasised one is the same ink worn italic: {:?}",
        rows[0].spans()
    );
    assert!(
        wears(&rows[1], "strong", Look::new(Ink::Plain, Face::Bold)),
        "a list's bullet is its own and its strong word is still bold: {:?}",
        rows[1].spans()
    );
    assert!(
        wears(&rows[2], "emphasised", Look::new(Ink::Plain, Face::Italic)),
        "and its emphasised word still italic: {:?}",
        rows[2].spans()
    );
}

#[test]
fn emphasis_inside_a_heading_wears_the_headings_weight_and_keeps_its_level() {
    let both = Faces::BOLD.and(Faces::ITALIC);
    let wanted = [
        Look::new(Ink::Plain, both),
        Look::new(Ink::Plain, both),
        Look::new(Ink::Muted, both),
        Look::new(Ink::Muted, Faces::ITALIC),
        Look::new(Ink::Dim, both),
        Look::new(Ink::Dim, both.and(Faces::ITALIC)),
    ];

    for (at, look) in wanted.into_iter().enumerate() {
        let rows = markdown::rows(&format!("{} an *q* word", "#".repeat(at + 1)), 40);
        let said = rows
            .iter()
            .flat_map(Row::spans)
            .find(|(text, _)| text.contains('q'));
        assert_eq!(
            said.map(|(_, worn)| *worn),
            Some(look),
            "the emphasised word on the level {} is {look:?}, because the mark and the \
             heading's own weight compose and the ink stays the level's: {rows:?}",
            at + 1
        );
    }
}

#[test]
fn emphasis_inside_a_quote_keeps_the_quotes_ink_and_its_gutter() {
    let rows = markdown::rows("> an *emphasised* word behind a bar", 24);

    for row in &rows {
        assert_eq!(
            row.spans().first(),
            Some(&(String::from("▏ "), Look::plain(Ink::Dim))),
            "every row of the quote still opens with its bar: {row:?}"
        );
    }
    assert!(
        rows.iter()
            .any(|row| wears(row, "emphasised", Look::new(Ink::Plain, Face::Italic))),
        "an emphasised word in a quote is the quote's own ink worn italic: {rows:?}"
    );
}

#[test]
fn a_fences_words_are_drawn_as_the_fence_drew_them_and_a_mark_inside_one_stays_a_glyph() {
    let rows = markdown::rows(
        "```\n**bold** and *italic* stay glyphs\n```\n\nand `a **b** c` in a span",
        40,
    );

    assert_eq!(
        drawn(&rows),
        [
            "▏ **bold** and *italic* stay glyphs",
            "",
            "and a **b** c in a span"
        ],
        "the marks are glyphs of code and never weights of a word"
    );
    assert!(
        rows[0].spans()[1..]
            .iter()
            .all(|(_, look)| *look == Look::plain(Ink::Plain)),
        "not one word of a fence wears a face the fence did not give it: {:?}",
        rows[0].spans()
    );
    assert!(
        wears(&rows[2], "a **b** c", Look::new(Ink::Muted, Face::Reversed)),
        "a span keeps the look code is read in, mark and all: {:?}",
        rows[2].spans()
    );
}

#[test]
fn an_emphasis_that_opens_and_never_closes_leaves_the_rows_after_it_unemphasised() {
    let rows = markdown::rows(
        "an **open mark that never closes and the words after it\n\nand the block under it",
        20,
    );

    assert!(
        words(&rows[0]).contains("**open"),
        "a mark no closing mark answers is a glyph and not a weight: {rows:?}"
    );
    assert!(
        rows.iter()
            .flat_map(Row::spans)
            .all(|(_, look)| *look == Look::plain(Ink::Plain)),
        "and every row after it is drawn plainly: {rows:?}"
    );
}

#[test]
fn a_bullet_marks_each_item_and_two_spaces_mark_each_depth() {
    let rows = markdown::rows("- one\n- two\n  - inner\n- three", 40);
    assert_eq!(
        drawn(&rows),
        ["• one", "• two", "  • inner", "• three"],
        "one row per item, a nested item two columns inside the one it belongs to"
    );
    for row in &rows {
        assert!(
            looks(row)
                .iter()
                .all(|look| *look == Look::plain(Ink::Plain)),
            "a list's words are drawn in the frame's own ink: {row:?}"
        );
    }
}

#[test]
fn an_ordered_list_numbers_its_items_and_hangs_its_wrapped_rows_under_them() {
    let rows = markdown::rows("3. three\n4. four\n5. a fifth item long enough to wrap", 20);
    assert_eq!(
        drawn(&rows),
        [
            "3. three",
            "4. four",
            "5. a fifth item long",
            "   enough to wrap"
        ],
        "the number the list starts at is the first number drawn"
    );
}

#[test]
fn a_blockquote_sets_its_words_behind_a_dim_bar() {
    let rows = markdown::rows("> a fence still open is a code block, never prose", 24);
    assert_eq!(
        drawn(&rows),
        [
            "▏ a fence still open is",
            "▏ a code block, never",
            "▏ prose"
        ],
        "the quote wraps inside its bar"
    );
    for row in &rows {
        assert_eq!(
            row.spans().first(),
            Some(&(String::from("▏ "), Look::plain(Ink::Dim))),
            "every row of a quote opens with the bar, in dim ink: {row:?}"
        );
    }
}

#[test]
fn a_code_block_is_guttered_by_its_language_and_highlighted() {
    let rows = markdown::rows("```rust\nfn main() {\n    let x = 1;\n}\n```", 40);
    assert_eq!(
        drawn(&rows),
        ["▏ rust", "▏ fn main() {", "▏     let x = 1;", "▏ }"],
        "the tag stands over the code and the code keeps its own four spaces"
    );
    for row in &rows {
        assert_eq!(
            row.spans().first(),
            Some(&(String::from("▏ "), Look::plain(Ink::Dim))),
            "every row of the block is set behind the bar: {row:?}"
        );
    }
    assert!(
        rows[2]
            .spans()
            .iter()
            .any(|(text, look)| text == "let" && *look == Look::plain(Ink::Accent)),
        "a keyword is the ink the eye lands on: {:?}",
        rows[2].spans()
    );
    assert!(
        rows[2]
            .spans()
            .iter()
            .any(|(text, look)| text == "1" && *look == Look::plain(Ink::Muted)),
        "a number is a constant, and constants are read in muted ink: {:?}",
        rows[2].spans()
    );
}

#[test]
fn a_language_no_syntax_definition_names_is_drawn_plain_and_says_nothing() {
    let rows = markdown::rows("```frobnicate\nlet x = 1; # not rust\n```", 40);
    assert_eq!(
        drawn(&rows),
        ["▏ frobnicate", "▏ let x = 1; # not rust"],
        "an unknown language is a tag and its lines, and nothing else"
    );
    assert!(
        rows[1].spans()[1..]
            .iter()
            .all(|(_, look)| *look == Look::plain(Ink::Plain)),
        "not one word of it is highlighted, the bar aside: {:?}",
        rows[1].spans()
    );
}

#[test]
fn a_fence_still_open_at_the_end_of_the_text_is_a_code_block() {
    let rows = markdown::rows("the rule is one boolean:\n```rust\nfn main() {", 40);
    assert_eq!(
        drawn(&rows),
        ["the rule is one boolean:", "", "▏ rust", "▏ fn main() {"],
        "the fence and what follows it are drawn as code, under one blank row"
    );
    assert_eq!(
        rows[3].spans()[0],
        (String::from("▏ "), Look::plain(Ink::Dim)),
        "and a fence still open is guttered like a fence that closed"
    );
}

#[test]
fn a_mermaid_fence_that_draws_is_drawn_as_the_source_it_was_written_as() {
    let rows = markdown::rows("```mermaid\nflowchart TD\n  A-->B\n```", 200);
    assert_eq!(
        drawn(&rows),
        ["▏ mermaid", "▏ flowchart TD", "▏   A-->B"],
        "a diagram is drawn as its source and says no reason, because nothing was refused"
    );
}

#[test]
fn a_mermaid_fence_that_will_not_draw_shows_its_source_under_the_reason() {
    let rows = drawn(&markdown::rows("```mermaid\nnot a diagram\n```", 200));
    assert_eq!(
        rows[..2],
        ["▏ mermaid", "▏ not a diagram"],
        "the source is drawn as the block it was written as: {rows:?}"
    );
    let reason = rows[2].to_lowercase();
    assert!(
        reason.contains("could not be drawn") && reason.contains("diagram header"),
        "the row under the source is the reason, in the renderer's own words: {rows:?}"
    );
}

#[test]
fn a_mermaid_fence_still_streaming_names_no_reason_because_nothing_was_asked() {
    let closed = drawn(&markdown::rows("```mermaid\nnot a diagram\n```", 200));
    let open = drawn(&markdown::rows("```mermaid\nnot a diagram", 200));

    assert!(
        closed.iter().any(|row| row.contains("could not be drawn")),
        "a closed fence that will not draw names its reason: {closed:?}"
    );
    assert!(
        !open.iter().any(|row| row.contains("could not be drawn")),
        "a fence still arriving is never a diagram, so no renderer was asked for one: {open:?}"
    );
    assert!(
        open.iter().any(|row| row.contains("not a diagram")),
        "and its source is drawn all the same, so the answer does not lose what was written: \
         {open:?}"
    );
}

#[test]
fn a_table_rules_one_line_under_its_header_and_draws_no_outer_border() {
    let rows = markdown::rows(
        "| construct | rendering |\n| --- | --- |\n| heading | bold |\n| code | reversed |",
        40,
    );
    assert_eq!(
        drawn(&rows),
        [
            "construct │ rendering",
            "──────────┼──────────",
            "heading   │ bold",
            "code      │ reversed",
        ],
        "the columns are padded to one width and the rule is the header's own row"
    );
    for row in &rows {
        assert!(
            !words(row).starts_with('│') && !words(row).ends_with('│'),
            "a table draws no outer border: {row:?}"
        );
    }
}

#[test]
fn a_tables_header_row_is_drawn_as_a_header_and_its_bars_recede_to_the_dim_ink() {
    let rows = markdown::rows(
        "| construct | rendering |\n| --- | --- |\n| heading | bold |\n| code | reversed |",
        40,
    );

    assert_eq!(
        drawn(&rows),
        [
            "construct │ rendering",
            "──────────┼──────────",
            "heading   │ bold",
            "code      │ reversed",
        ],
        "the header's words, the rule under them, and the body under the rule"
    );

    let header = Look::new(Ink::Plain, Face::Bold);
    assert!(
        wears(&rows[0], "construct", header) && wears(&rows[0], "rendering", header),
        "a header cell is the frame's ink worn bold, so the eye lands on it: {:?}",
        rows[0].spans()
    );
    assert!(
        wears(&rows[2], "heading", Look::plain(Ink::Plain)),
        "and a body cell beside it is the prose around it: {:?}",
        rows[2].spans()
    );

    let dim = Look::plain(Ink::Dim);
    for row in &rows {
        let bars: Vec<&Look> = row
            .spans()
            .iter()
            .filter(|(said, _)| said.contains('│'))
            .map(|(_, look)| look)
            .collect();
        assert!(
            bars.is_empty() || bars.iter().all(|look| **look == dim),
            "the bars that join the columns recede to the dim ink, and the header's does too: \
             {row:?}"
        );
    }
    assert_eq!(
        looks(&rows[1]),
        [dim],
        "and so does the rule under the header: {:?}",
        rows[1]
    );
}

#[test]
fn a_table_wider_than_the_room_wraps_inside_its_cells_and_keeps_every_word() {
    let text = "| a heading is bold | a code block is guttered by its language | an unknown \
                language is plain |\n| --- | --- | --- |\n| one blank row above an h1 | the \
                table's rule sits under the header and never around it | a link prints its \
                url |\n";
    let rows = markdown::rows(text, 40);

    for row in &rows {
        assert!(
            row.width() <= 40,
            "a row ran past the room it was laid out for: {row:?}"
        );
    }
    assert!(
        rows.len() > 4,
        "a table this wide wraps rather than standing on one row per table row: {}",
        rows.len()
    );
    let flat = drawn(&rows).join(" ");
    for cell in [
        "guttered", "unknown", "language", "header", "never", "around", "prints", "url", "one",
        "blank", "row", "above",
    ] {
        assert!(flat.contains(cell), "the word {cell:?} was lost: {flat}");
    }
    let rules: Vec<String> = drawn(&rows)
        .into_iter()
        .filter(|row| row.contains('┼'))
        .collect();
    assert_eq!(rules.len(), 1usize, "one rule, under the header: {rules:?}");
    assert!(
        rules[0].chars().all(|glyph| glyph == '─' || glyph == '┼'),
        "the rule is the header's own row with its words spent as rules: {:?}",
        rules[0]
    );
}

#[test]
fn a_table_too_narrow_for_one_row_wraps_each_column_under_its_own_heading() {
    let rows = markdown::rows(
        "| a | b | c |\n| --- | --- | --- |\n| one | two | three |",
        8,
    );

    assert_eq!(
        drawn(&rows),
        [
            "a",
            "────────",
            "one",
            "",
            "b",
            "────────",
            "two",
            "",
            "c",
            "────────",
            "three",
        ],
        "a column keeps its own heading over its own cells when no row of the table fits"
    );

    let header = Look::new(Ink::Plain, Face::Bold);
    for at in [0, 4, 8] {
        assert_eq!(
            looks(&rows[at]),
            [header],
            "the heading over a column is still drawn as a header: {:?}",
            rows[at]
        );
        assert_eq!(
            looks(&rows[at + 1]),
            [Look::plain(Ink::Dim)],
            "under a dim rule"
        );
    }
}

#[test]
fn a_cell_wider_than_a_frame_that_holds_no_row_wraps_inside_the_room_it_has() {
    let rows = markdown::rows(
        "| a construct | b | c |\n| --- | --- | --- |\n| a cell that wraps | two | three |",
        8,
    );

    for row in &rows {
        assert!(
            row.width() <= 16,
            "a row ran past the room it was laid out for: {row:?}"
        );
    }
    let flat = drawn(&rows).join(" ");
    assert!(
        flat.contains("a cell that") && flat.contains("wraps"),
        "a cell's words are kept when it wraps: {flat}"
    );
    assert!(
        !flat.contains('│'),
        "no bar is drawn between two columns that stand one over the other: {flat}"
    );
}

#[test]
fn a_table_in_a_quote_keeps_the_gutter_of_the_block_it_stands_in() {
    let rows = markdown::rows(
        "> a quote says\n>\n> | a | b |\n> | --- | --- |\n> | one | two |",
        40,
    );

    assert_eq!(
        drawn(&rows),
        [
            "▏ a quote says",
            "",
            "▏ a   │ b",
            "▏ ────┼────",
            "▏ one │ two",
        ],
        "every row of a table in a quote stands behind the bar the prose does"
    );
    for row in &rows[2..] {
        assert_eq!(
            row.spans().first(),
            Some(&(String::from("▏ "), Look::plain(Ink::Dim))),
            "the table's own rows open with the bar: {row:?}"
        );
        assert!(
            row.width() <= 38,
            "and they are laid out in the room the bar leaves them: {row:?}"
        );
    }
}

#[test]
fn a_heading_and_a_table_still_arriving_are_drawn_as_the_blocks_they_will_be() {
    let heading = markdown::rows("## a heading the wire is still writing", 40);
    assert_eq!(
        drawn(&heading),
        [
            "a heading the wire is still writing",
            "───────────────────────────────────",
        ],
        "a heading is ruled under what has arrived of it"
    );

    let table = markdown::rows("| a | b |\n| --- | --- |\n| one | two", 40);
    assert_eq!(
        drawn(&table),
        ["a   │ b", "────┼────", "one │ two"],
        "a table still arriving is a header and the body that has arrived"
    );
    let header = Look::new(Ink::Plain, Face::Bold);
    assert!(
        wears(&table[0], "a", header) && wears(&table[0], "b", header),
        "a header cell is drawn as a header whether or not the table closed: {:?}",
        table[0].spans()
    );
    assert!(
        wears(&table[2], "one", Look::plain(Ink::Plain)),
        "and the row under the rule is body: {:?}",
        table[2].spans()
    );
}

#[test]
fn a_code_fence_sits_in_the_well_and_the_prose_around_it_sits_on_nothing() {
    let rows = markdown::rows(
        "prose above\n\n```rust\nfn main() {}\n```\n\nprose below",
        40,
    );

    let bands: Vec<(String, Option<Band>)> =
        rows.iter().map(|row| (words(row), row.band())).collect();

    assert_eq!(
        bands,
        vec![
            (String::from("prose above"), None),
            (String::new(), None),
            (String::from("▏ rust"), Some(Band::Well)),
            (String::from("▏ fn main() {}"), Some(Band::Well)),
            (String::new(), None),
            (String::from("prose below"), None),
        ],
        "the well holds the fence whole and stops at its edges"
    );
}

/// A diagram that will not draw keeps its source in the well, and the sentence saying why sits
/// under it as prose: the well is the fence's, and not everything the block said.
#[test]
fn the_sentence_saying_a_diagram_will_not_draw_is_prose_and_not_code() {
    let rows = markdown::rows("```mermaid\nnot a diagram\n```", 200);
    let under: Vec<(String, Option<Band>)> =
        rows.iter().map(|row| (words(row), row.band())).collect();

    assert_eq!(
        under[..3].iter().map(|(_, band)| *band).collect::<Vec<_>>(),
        [Some(Band::Well), Some(Band::Well), None],
        "the tag and the source are the well's, and the row under them is no code: {under:?}"
    );
}

/// The surface a terminal that draws text at more than one cell is measured against: its headings
/// are laid out as the cells the sizing leaves, and drawn as the words they are.
fn large() -> Surface {
    Surface::of(None, CellSize::NOMINAL, false, true)
}

#[test]
fn the_two_highest_headings_are_drawn_at_twice_the_cell_and_a_lower_level_is_not() {
    let rows = markdown::rows_on("# One\n\n## Two\n\n### Three", 40, 0, &large());

    assert_eq!(
        drawn(&rows),
        [
            "",
            "One",
            "",
            "══════",
            "",
            "Two",
            "",
            "──────",
            "",
            "Three"
        ],
        "the two highest levels stand on a large row with half a row of their own under it, \
         and are ruled as wide as the cells the sizing leaves; the third is the words it is"
    );
    for (at, word) in [(1usize, "One"), (5, "Two")] {
        let row = &rows[at];
        assert_eq!(
            row.width(),
            6,
            "three glyphs at twice the cell are six columns, and the row is measured by them: \
             {row:?}"
        );
        assert_eq!(
            row.glyphs(),
            word,
            "the row is written as its words and drawn as the cells around them: {row:?}"
        );
    }
}

#[test]
fn a_terminal_that_draws_no_large_text_is_handed_the_heading_it_is_handed_today() {
    let said = "# One\n\n## Two\n\n### Three";

    assert_eq!(
        drawn(&markdown::rows_on(said, 40, 0, &Surface::none())),
        ["", "One", "═══", "", "Two", "───", "", "Three"],
        "a terminal that draws no large text is handed one row per heading and no half row"
    );
    assert_eq!(
        drawn(&markdown::rows_on(
            said,
            40,
            0,
            &Surface::of(None, CellSize::NOMINAL, false, false)
        )),
        drawn(&markdown::rows_on(said, 40, 0, &Surface::none())),
        "a surface that says the terminal sizes no text is the surface of a terminal that \
         places no picture"
    );
}

#[test]
fn a_large_heading_wider_than_the_band_is_cut_by_the_bands_edge_and_never_by_a_column() {
    let rows = markdown::rows_on("# abcdefghijklmnopqrstuvwxyz", 20, 0, &large());

    assert_eq!(
        drawn(&rows),
        [
            "",
            "abcdefghij",
            "",
            "klmnopqrst",
            "",
            "uvwxyz",
            "",
            "════════════════════",
        ],
        "ten glyphs at twice the cell fill the band's twenty columns, and the rule under them is \
         as wide as the widest of them"
    );
    let widths: Vec<u16> = rows.iter().map(Row::width).collect();
    assert_eq!(
        widths,
        [0, 20, 0, 20, 0, 12, 0, 20],
        "each large row is the cells its glyphs leave, and never a column past the band"
    );

    let wide = markdown::rows_on("# 日本語日本語", 20, 0, &large());
    let widths: Vec<u16> = wide.iter().map(Row::width).collect();
    assert_eq!(
        widths,
        [0, 20, 0, 4, 0, 20],
        "a glyph two cells wide leaves four at twice the cell, so five of them are the bands's \
         twenty columns and the sixth is the row under it"
    );
}

#[test]
fn a_wrapped_large_heading_keeps_one_large_row_to_a_line_and_the_rows_under_it_keep_the_gutter() {
    let rows = markdown::rows_on(
        "> ## a heading behind a bar, wide enough to be drawn over several lines",
        20,
        0,
        &large(),
    );

    let said: Vec<String> = rows.iter().map(words).collect();
    let rule = said.len() - 1;
    assert!(
        said[rule].starts_with("▏ ─"),
        "the heading closes under its rule, behind the bar it stands in: {said:?}"
    );
    assert_eq!(
        rule % 2,
        0,
        "every line of the heading is a large row with half a row under it: {said:?}"
    );
    for (at, row) in rows.iter().enumerate().take(rule) {
        assert_eq!(
            row.spans().first().map(|(text, _)| text.as_str()),
            Some("▏ "),
            "row {at} of the quote kept its bar: {said:?}"
        );
        assert!(
            row.width() <= 20,
            "row {at} ran past the band it was laid out for: {said:?}"
        );
        if at % 2 == 1 {
            assert_eq!(
                said[at], "▏ ",
                "the row under a large one is the block's: it keeps the bar and says nothing else"
            );
        }
    }
}

#[test]
fn a_heading_a_band_leaves_no_whole_cell_for_is_cut_by_its_edge_and_never_by_a_column() {
    let cut = markdown::rows_on("# 日本", 3, 0, &large());
    assert_eq!(
        drawn(&cut),
        ["", "日", "本", "══"],
        "a glyph at twice the cell takes four columns and a band three wide holds none of it, so \
         the heading is drawn as the words it says and ruled under them"
    );
    for row in &cut {
        assert!(
            row.width() <= 3,
            "a row ran past the band it was laid out for: {row:?}"
        );
    }

    let whole = markdown::rows_on("# 日", 4, 0, &large());
    assert_eq!(
        drawn(&whole),
        ["", "日", "", "════"],
        "a band four wide holds the glyph and the row under it, so the heading is drawn at twice \
         the cell and fills the band's edge exactly"
    );
}
