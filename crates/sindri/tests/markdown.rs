//! Markdown laid out as the rows the transcript draws, one construct of §12's table at
//! a time: the parse is `pulldown-cmark`'s and the rendering is ours, because the
//! layout is the product and no crate matches it (#52).
//!
//! What is held here is the picture at the row: a row's words and the look each run of
//! them is drawn in, so a heading's weight and a code block's gutter are the assertion
//! and not an opinion about the code that made them. The frames in `frames` hold the
//! same constructs as a whole screen at 80 columns; these hold them one at a time.

use sindri::ink::{Face, Ink, Look};
use sindri::markdown;
use sindri::rows::Row;

/// The words a row is drawn with, its gutter and all.
fn words(row: &Row) -> String {
    row.spans().iter().map(|(text, _)| text.as_str()).collect()
}

/// The row every construct is drawn as, as its words.
fn drawn(rows: &[Row]) -> Vec<String> {
    rows.iter().map(words).collect()
}

/// The looks a row's spans are in, in order.
fn looks(row: &Row) -> Vec<Look> {
    row.spans().iter().map(|(_, look)| *look).collect()
}

/// §12's first row of the table: an h1 is bold and stands under one blank row, and the
/// five levels under it are set in the weight and the ink their own row asks for.
#[test]
fn a_heading_is_bold_and_an_h1_stands_under_one_blank_row() {
    let rows = markdown::rows(
        "prose first\n\n# One\n## Two\n### Three\n#### Four\n##### Five\n###### Six",
        40,
    );
    assert_eq!(
        drawn(&rows),
        [
            "prose first",
            "",
            "One",
            "",
            "Two",
            "",
            "Three",
            "",
            "Four",
            "",
            "Five",
            "",
            "Six"
        ],
        "every block stands under one blank row, and the h1 under one of its own"
    );
    assert_eq!(
        looks(&rows[2]),
        [Look::new(Ink::Plain, Face::Bold)],
        "an h1 is set bold"
    );
    assert_eq!(
        looks(&rows[4]),
        [Look::new(Ink::Plain, Face::Bold)],
        "and so is an h2"
    );
    assert_eq!(
        looks(&rows[6]),
        [Look::new(Ink::Dim, Face::Bold)],
        "an h3 is bold in dim ink"
    );
    for level in [8, 10, 12] {
        assert_eq!(
            looks(&rows[level]),
            [Look::plain(Ink::Plain)],
            "h4 to h6 are plain: {:?}",
            rows[level]
        );
    }
}

/// The h1's blank row is above the heading itself and not only above the block before
/// it: an answer that opens on a heading opens on one blank row.
#[test]
fn an_answer_that_opens_on_an_h1_opens_on_a_blank_row() {
    let rows = markdown::rows("# The fence rule", 40);
    assert_eq!(drawn(&rows), ["", "The fence rule"]);
    assert_eq!(rows[0].width(), 0, "the blank row says nothing at all");
}

/// §12's inline rows: a run of inline code is set reversed, and a link says its words
/// and then the URL in dim ink when the URL is not those words — so what a reader sees
/// is what the writer wrote, and a bare `[keys](keys)` says `keys` once.
#[test]
fn inline_code_is_reversed_and_a_link_names_the_url_the_words_hide() {
    let rows = markdown::rows(
        "run `make check` first, see [the gate](https://example.test/gate) and [keys](keys).",
        80,
    );
    assert_eq!(
        drawn(&rows),
        ["run make check first, see the gate https://example.test/gate and keys."],
        "the link's words stand where they were written and the URL follows them"
    );

    let spans = rows[0].spans();
    assert!(
        spans.contains(&(
            String::from("make check"),
            Look::new(Ink::Muted, Face::Reversed)
        )),
        "a run of inline code is reversed over `muted`, so that it is not the mark's \
         own look: {spans:?}"
    );
    assert!(
        spans.contains(&(
            String::from("https://example.test/gate"),
            Look::plain(Ink::Dim)
        )),
        "the URL is read in the ink a fact beside the prose is read in: {spans:?}"
    );
    assert_eq!(
        drawn(&rows)[0].matches("keys").count(),
        1,
        "a link whose words are its URL says them once: {spans:?}"
    );
}

/// Inline code keeps its own spacing: `a  b` is two spaces of code and not a word
/// boundary, so a run of code is written as it was written and broken at the row's edge
/// and nowhere else.
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

/// §12's unordered row: a `•` per item and two spaces per depth, so a list of lists is
/// read as a shape before it is read as words.
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

/// §12's ordered row: `1.` numbering per item, in the order the items were written and
/// from the number the list starts at, with the same two spaces per depth — and a
/// second row of a wrapped item set under the item's own words rather than under its
/// number.
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

/// §12's quote row: a `▏` bar down the left of what was quoted, in dim ink, on every
/// row the quote wraps into — the bar is a fact about the block and not a glyph of its
/// first line.
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

/// §12's code row: a `▏` gutter carrying the language's tag, the code inside it drawn
/// with the spacing it was written with, and its words set in the ink their kind is read
/// in — `syntect` decides what kind a word is, and this front end decides which ink.
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

/// A language no syntax definition names is drawn plain and says nothing about it: the
/// block is still a block, its tag is still its tag, and a wire that streams a language
/// this build has never heard of costs the run no error (§12).
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

/// ADR-0008's rule, kept when the plain rendering goes: a fence the wire has not closed
/// yet is a code block and never prose that ate the rest of the answer — the block stays
/// open to the end of the text and is drawn as the block it opened.
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

/// `text` laid out until its diagrams have drawn, or as far as a bound of this test's own
/// allows: `mermaid::BOUND` is a stall a *reader* should not feel, and a machine running the
/// whole suite at once can spend it on scheduling rather than on drawing. A fence the renderer
/// really will not draw still fails here — that is what a refusal is — but a fence that was
/// merely starved of a processor is laid out again.
fn laid_out_until_drawn(text: &str, cols: u16) -> Vec<sindri::rows::Row> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let rows = markdown::rows(text, cols);
        let starved = drawn(&rows)
            .iter()
            .any(|line| line.contains("took longer than"));
        if !starved || std::time::Instant::now() > deadline {
            return rows;
        }
    }
}

/// §12's picture row, as far as a layout with no surface can show it: a mermaid fence the
/// renderer draws is drawn here as the code it was written as — the words its block copies as,
/// and all a reader whose terminal places no picture sees — with nothing under it, because
/// there is nothing to say. What a frame that *does* place pictures adds over these rows is
/// `pictures`' (#55).
#[test]
fn a_mermaid_fence_that_draws_is_drawn_as_the_source_it_was_written_as() {
    let rows = laid_out_until_drawn("```mermaid\nflowchart TD\n  A-->B\n```", 200);
    assert_eq!(
        drawn(&rows),
        ["▏ mermaid", "▏ flowchart TD", "▏   A-->B"],
        "a diagram is drawn as its source and says no reason, because nothing was refused"
    );
}

/// #54's refusal row: a mermaid fence the renderer will not read shows its source with one
/// sentence under it naming the reason, because a reader looking at a diagram that did not
/// draw cannot otherwise tell a fence they wrote badly from a build that cannot draw it.
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

/// ADR-0008's rule on the picture side: a fence whose closing line has not arrived is not a
/// diagram yet. The proof is the source itself — the same prose that comes back with a
/// reason once the fence closes comes back with none while it is open, which is only
/// possible if the renderer was never asked.
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

/// §12's table row: `│` between the columns, a `─` rule under the header and no outer
/// border, so a table reads as a block of the answer rather than as a box drawn around
/// one.
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

/// The 200-column table at 40 columns, which is §12's own promise: it wraps inside its
/// cells rather than reflowing into a paragraph, it never loses a word, and it draws one
/// rule under its header however many rows it takes.
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
