//! Markdown laid out as the rows a transcript draws, one construct at a time: `pulldown-cmark` finds
//! the constructs and this front end decides what each of them looks like, so a heading's weight and
//! a code block's gutter are held here; a whole screen of them is `frames`'.

use sindri::ink::{Face, Faces, Ink, Look};
use sindri::markdown;
use sindri::palette::Band;
use sindri::rows::Row;

fn words(row: &Row) -> String {
    row.spans().iter().map(|(text, _)| text.as_str()).collect()
}

fn drawn(rows: &[Row]) -> Vec<String> {
    rows.iter().map(words).collect()
}

fn looks(row: &Row) -> Vec<Look> {
    row.spans().iter().map(|(_, look)| *look).collect()
}

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

#[test]
fn an_answer_that_opens_on_an_h1_opens_on_a_blank_row() {
    let rows = markdown::rows("# The fence rule", 40);
    assert_eq!(drawn(&rows), ["", "The fence rule"]);
    assert_eq!(rows[0].width(), 0, "the blank row says nothing at all");
}

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
        rows[0]
            .spans()
            .contains(&(String::from("strong"), Look::new(Ink::Plain, Face::Bold))),
        "a strong word is the frame's own ink worn bold: {:?}",
        rows[0].spans()
    );
    assert!(
        rows[0].spans().contains(&(
            String::from("emphasised"),
            Look::new(Ink::Plain, Face::Italic)
        )),
        "an emphasised one is the same ink worn italic: {:?}",
        rows[0].spans()
    );
    assert!(
        rows[1]
            .spans()
            .contains(&(String::from("strong"), Look::new(Ink::Plain, Face::Bold))),
        "a list's bullet is its own and its strong word is still bold: {:?}",
        rows[1].spans()
    );
    assert!(
        rows[2].spans().contains(&(
            String::from("emphasised"),
            Look::new(Ink::Plain, Face::Italic)
        )),
        "and its emphasised word still italic: {:?}",
        rows[2].spans()
    );
}

/// A heading is a weight of its own, so an emphasis inside one is that weight and the mark's
/// together: an h1's italic is bold and italic, an h3's is dim and both, and an h4's — which is
/// set in no weight of its own — is the mark alone.
#[test]
fn emphasis_inside_a_heading_wears_the_headings_weight_and_keeps_its_level() {
    let rows = markdown::rows(
        "# an *a* word\n\n## a *b* word\n\n### a *c* word\n\n#### a *d* word",
        40,
    );
    let heads: Vec<&Row> = rows.iter().filter(|row| !words(row).is_empty()).collect();

    let wanted = [
        ("a", Look::new(Ink::Plain, Faces::BOLD.and(Faces::ITALIC))),
        ("b", Look::new(Ink::Plain, Faces::BOLD.and(Faces::ITALIC))),
        ("c", Look::new(Ink::Dim, Faces::BOLD.and(Faces::ITALIC))),
        ("d", Look::new(Ink::Plain, Faces::ITALIC)),
    ];
    assert_eq!(heads.len(), wanted.len(), "one row per heading: {rows:?}");
    for (row, (letter, look)) in heads.iter().zip(wanted) {
        assert!(
            row.spans().contains(&(String::from(letter), look)),
            "the emphasised {letter:?} is {look:?}, because the mark and the heading's own \
             weight compose and the ink stays the level's: {:?}",
            row.spans()
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
        rows.iter().any(|row| row.spans().contains(&(
            String::from("emphasised"),
            Look::new(Ink::Plain, Face::Italic)
        ))),
        "an emphasised word in a quote is the quote's own ink worn italic: {:?}",
        rows.iter().map(Row::spans).collect::<Vec<_>>()
    );
}

/// CommonMark reads no mark inside a fence, so the wire's markdown is drawn as the wire wrote it:
/// a renderer that reformatted a fence's words would be editing what it was told to keep.
#[test]
fn a_fences_words_are_drawn_as_the_fence_drew_them_and_a_mark_inside_one_stays_a_glyph() {
    let rows = markdown::rows("```\n**bold** and *italic* stay glyphs\n```", 40);

    assert_eq!(
        drawn(&rows),
        ["▏ **bold** and *italic* stay glyphs"],
        "the marks are glyphs of the fence and never weights of a word"
    );
    assert!(
        rows[0].spans()[1..]
            .iter()
            .all(|(_, look)| *look == Look::plain(Ink::Plain)),
        "not one word of a fence wears a face the fence did not give it: {:?}",
        rows[0].spans()
    );
}

#[test]
fn an_emphasis_that_opens_and_never_closes_leaves_the_rows_after_it_unemphasised() {
    let rows = markdown::rows(
        "an **open mark that never closes and the words after it",
        20,
    );

    assert!(
        words(&rows[0]).contains("**open"),
        "a mark no closing mark answers is a glyph and not a weight: {:?}",
        drawn(&rows)
    );
    assert!(
        rows.iter()
            .flat_map(Row::spans)
            .all(|(_, look)| *look == Look::plain(Ink::Plain)),
        "and every row of it is drawn plainly: {:?}",
        rows.iter().map(Row::spans).collect::<Vec<_>>()
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
