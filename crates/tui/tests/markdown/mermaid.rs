//! A fence that names `mermaid` is a picture in the answer: the store measures
//! the diagram it holds and the writer reserves those rows and captions them
//! `‹mermaid›`. A fence that will not parse is the panel it was, with the
//! parser's own reason under it, because a reader must never lose a diagram's
//! source to a renderer's opinion.

use super::*;

use sindri_tui::graphics::Graphics;
use sindri_tui::markdown;
use sindri_tui::picture::{Refusal, Store, Worth};

/// The store a test measures with: the made-up Kitty answer the other picture
/// tests use. No workspace is needed — a fence is its own source.
fn store() -> Store {
    Store::new(Some(&Graphics::kitty()), ".").expect("a Kitty answer is a store")
}

/// A diagram the crate draws: flat and wide, so it is drawn at its own size.
const DIAGRAM: &str = "stateDiagram-v2\n    [*] --> Resting\n    Resting --> [*]\n";

/// A diagram drawn top to bottom and long with it: ten nodes, one under the
/// next, whose height is the dimension that carries its words. Committed beside
/// the crate so the store, the writer and the frame all measure one diagram.
const PORTRAIT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/diagrams/ten-nodes.mmd"
));

/// A diagram no parser can read: an edge with nothing on the far side.
const BROKEN: &str = "flowchart TD\n    A --> \n";

fn fence(language: &str, body: &str) -> String {
    format!("```{language}\n{body}```\n")
}

#[test]
fn a_mermaid_fence_reserves_the_rows_the_store_measured_and_captions_them() {
    let block = markdown::lines(&fence("mermaid", DIAGRAM), 60, Some(&store()));
    let shown = texts(&block.rows);
    let picture = block
        .pictures
        .first()
        .unwrap_or_else(|| panic!("the fence is a block: {shown:?}"));

    let rows = usize::from(picture.worth.rows());
    assert!(rows >= 1, "a diagram takes rows: {shown:?}");
    assert_eq!(picture.at, 0, "the block opens the answer: {shown:?}");
    assert_eq!(
        shown.len(),
        rows + 1,
        "the rows it was measured at, and the caption under them: {shown:?}"
    );
    assert!(
        shown[..rows].iter().all(String::is_empty),
        "the rows are the graphic's: {shown:?}"
    );
    assert_eq!(
        shown[rows], "‹mermaid›",
        "the fence's own name is the caption"
    );
    assert_eq!(picture.caption, "‹mermaid›");
    assert_eq!(picture.src, "mermaid", "a fence is its own source");
}

#[test]
fn a_mermaid_fence_that_will_not_parse_is_its_panel_and_the_parser_s_reason() {
    let broken = fence("mermaid", BROKEN);
    let block = markdown::lines(&broken, 60, Some(&store()));
    let shown = texts(&block.rows);
    let plain = texts(&markdown::lines(&broken, 60, None).rows);

    assert!(
        block.pictures.is_empty(),
        "nothing is measured, so there is nothing to draw: {shown:?}"
    );
    assert_eq!(
        shown[..plain.len()],
        plain[..],
        "the reader still reads the source, in the panel it was: {shown:?}"
    );
    assert!(
        shown.len() > plain.len(),
        "and the parser's reason under it: {shown:?}"
    );
    assert!(
        shown[..plain.len()]
            .iter()
            .any(|row| row.contains("flowchart TD")),
        "no cell of the source was swallowed: {shown:?}"
    );
    for reason in &block.rows[plain.len()..] {
        assert!(
            reason
                .spans
                .iter()
                .all(|span| span.style == sindri_tui::theme::faint()),
            "the reason is faint ink: {reason:?}"
        );
    }
    let said = shown[plain.len()..].join(" ");
    assert!(
        said.contains("expected"),
        "and it is what the parser said: {said:?}"
    );
}

#[test]
fn a_mermaid_fence_in_a_list_item_or_a_table_cell_keeps_the_words_it_had() {
    let item = "- ```mermaid\n  stateDiagram-v2\n  ```\n";
    let block = markdown::lines(item, 60, Some(&store()));
    let shown = texts(&block.rows);
    assert!(
        shown.iter().any(|row| row.contains("stateDiagram-v2")),
        "an item shows the source it was given: {shown:?}"
    );
    assert!(
        block.pictures.is_empty(),
        "a list item is not a place a block goes: {shown:?}"
    );
    assert!(
        !shown.iter().any(|row| row.contains("‹mermaid›")),
        "and it keeps no caption: {shown:?}"
    );

    // A table cell cannot hold a fenced block at all, so its source is the
    // words in the cell — which is what the writer already draws.
    let table = "| note |\n| --- |\n| `stateDiagram-v2` |\n";
    let block = markdown::lines(table, 60, Some(&store()));
    assert!(block.pictures.is_empty(), "a cell is not a block either");
}

#[test]
fn a_terminal_that_draws_no_picture_draws_a_mermaid_fence_as_any_other_fence() {
    let mine = texts(&markdown::lines(&fence("mermaid", DIAGRAM), 60, None).rows);
    let other = texts(&markdown::lines(&fence("mmd", DIAGRAM), 60, None).rows);

    assert!(
        mine[0].contains("mermaid"),
        "the panel is titled with the name the model wrote: {mine:?}"
    );
    assert_eq!(
        mine[1..],
        other[1..],
        "and is the same panel every fence without a graphic is: {mine:?}"
    );
    assert!(
        !mine.iter().any(|row| row.contains('‹')),
        "a frame that draws nothing says nothing about a diagram: {mine:?}"
    );
}

#[test]
fn a_mermaid_fence_that_will_not_parse_is_refused_in_the_stores_own_words() {
    let worth = store().fence(BROKEN, 60);
    assert_eq!(worth.rows(), 0, "a refusal takes no rows");
    match worth {
        Worth::Refused(reason @ Refusal::Diagram(_)) => {
            assert!(!reason.said().is_empty(), "and it says why");
        }
        other => panic!("a broken fence is a typed reason: {other:?}"),
    }
}

#[test]
fn a_portrait_mermaid_fence_takes_more_rows_than_a_photograph_may() {
    // The mirror of `a_picture_takes_the_rows_the_store_measured_and_not_one_of
    // _its_own`: a ten-node flowchart TD is 585×984 pixels, which at sixty
    // columns is forty-five rows — over the twenty a photograph may take and
    // inside the forty a diagram may, so the writer reserves the diagram's
    // allowance whole rather than a picture's shrunken one.
    let block = markdown::lines(&fence("mermaid", PORTRAIT), 60, Some(&store()));
    let shown = texts(&block.rows);
    let picture = block
        .pictures
        .first()
        .unwrap_or_else(|| panic!("the fence is a block: {shown:?}"));

    assert_eq!(
        picture.worth.rows(),
        sindri_tui::picture::DIAGRAM_ROWS,
        "a diagram gets the diagram allowance: {shown:?}"
    );
    assert!(
        picture.worth.rows() > sindri_tui::picture::MAX_ROWS,
        "which is more than a picture may take, or this test proves nothing"
    );
    assert_eq!(
        shown.len(),
        usize::from(sindri_tui::picture::DIAGRAM_ROWS) + 1,
        "the rows it was measured at, and the caption under them: {shown:?}"
    );
    assert!(
        shown[..usize::from(sindri_tui::picture::DIAGRAM_ROWS)]
            .iter()
            .all(String::is_empty),
        "and every one of them is the graphic's: {shown:?}"
    );
    assert_eq!(
        shown[usize::from(sindri_tui::picture::DIAGRAM_ROWS)],
        "‹mermaid›",
        "the fence's name is still the caption: {shown:?}"
    );
}

#[test]
fn a_diagram_copies_as_the_fence_body_it_was_drawn_from() {
    // The ticket's bug: a block that copies as `‹mermaid›` is not a diagram and
    // not a document. What a reader pastes out of the frame is the source.
    let block = markdown::lines(&fence("mermaid", DIAGRAM), 60, Some(&store()));
    let picture = block
        .pictures
        .first()
        .unwrap_or_else(|| panic!("the fence is a block: {:?}", texts(&block.rows)));

    assert_eq!(
        picture.copy,
        DIAGRAM.trim_end_matches('\n'),
        "a copy of the block is the mermaid that drew it"
    );
    assert_ne!(
        picture.copy, picture.caption,
        "and not the caption a copy of an image yields"
    );
    assert_eq!(
        picture.caption, "‹mermaid›",
        "the caption it is drawn under is still the fence's own name"
    );
}
