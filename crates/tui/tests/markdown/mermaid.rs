//! A fence that names `mermaid` is a picture in the answer: the store measures
//! the diagram it holds and the writer reserves those rows and captions them
//! `‹mermaid›`. A fence that will not parse is the panel it was, with the
//! parser's own reason under it, because a reader must never lose a diagram's
//! source to a renderer's opinion.

use super::*;

use sindri_tui::graphics::Graphics;
use sindri_tui::markdown;
use sindri_tui::picture::{self, Refusal, Store, Worth};

/// The store a test measures with: the made-up Kitty answer the other picture
/// tests use. No workspace is needed — a fence is its own source.
fn store() -> Store {
    Store::new(Some(&Graphics::kitty()), ".").expect("a Kitty answer is a store")
}

/// The rows an answer is written as once the drawings it asks for have been
/// made: a fence is laid out as its panel, the store asks, the drawing is made —
/// on a thread of its own in the frame, on this one here — and the answer is
/// handed back, which is the state every frame after the first one sees. A test
/// has no loop, so it is the loop.
fn rows(store: &Store, text: &str, width: u16) -> markdown::Written {
    let asked = markdown::lines(text, width, Some(store));
    let wanted = store.take_wanted();
    if wanted.is_empty() {
        return asked;
    }
    for body in &wanted {
        store.keep(body, picture::drawn(body));
    }
    markdown::lines(text, width, Some(store))
}

/// What a fence's body is worth once its drawing has been made.
fn worth(store: &Store, body: &str, columns: u16) -> Worth {
    let worth = store.fence(body, columns);
    for body in store.take_wanted() {
        store.keep(&body, picture::drawn(&body));
    }
    match worth {
        Worth::Pending => store.fence(body, columns),
        drawn => drawn,
    }
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

/// A fence as a stream has so far sent it: the opening delimiter and the body,
/// and no line yet closing it.
fn arriving(language: &str, body: &str) -> String {
    format!("```{language}\n{body}")
}

#[test]
fn a_mermaid_fence_reserves_the_rows_the_store_measured_and_captions_them() {
    let block = rows(&store(), &fence("mermaid", DIAGRAM), 60);
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
    let block = rows(&store(), &broken, 60);
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

/// A fence the store has asked for a drawing of and not been handed yet is the
/// panel it was, with nothing under it: "not drawn yet" is silence, and a refusal
/// is a sentence the caption prints. The frame that asks pays the renderer
/// nothing for it.
#[test]
fn a_fence_waiting_for_its_drawing_is_its_panel_and_says_nothing_of_it() {
    let store = store();
    let asked = markdown::lines(&fence("mermaid", DIAGRAM), 60, Some(&store));
    let shown = texts(&asked.rows);
    let undrawn = texts(&markdown::lines(&fence("mermaid", DIAGRAM), 60, None).rows);

    assert!(
        asked.pictures.is_empty(),
        "nothing is measured, so there is nothing to draw: {shown:?}"
    );
    assert_eq!(
        shown, undrawn,
        "the panel every fence without a graphic is: {shown:?}"
    );
    assert!(
        !shown.iter().any(|row| row.contains('‹')),
        "and nothing is said of a diagram nothing has refused: {shown:?}"
    );
    assert_eq!(
        store.take_wanted(),
        vec![DIAGRAM.trim_end_matches('\n').to_string()],
        "and the body is what the frame asked to have drawn"
    );
}

/// A fence whose closing delimiter the stream has not sent is the panel it is,
/// and no diagram is made of it: its body is a prefix the next delta lengthens,
/// and a layout is tens of milliseconds of a frame spent on a figure thrown
/// away. Not asking the store at all is what the rows say: they are the ones a
/// frame that draws no picture at all would write.
#[test]
fn a_fence_still_arriving_shows_its_source_and_asks_for_no_diagram() {
    let block = markdown::lines(&arriving("mermaid", PORTRAIT), 60, Some(&store()));
    let shown = texts(&block.rows);
    let undrawn = texts(&markdown::lines(&arriving("mermaid", PORTRAIT), 60, None).rows);

    assert!(
        block.pictures.is_empty(),
        "a fence still arriving is no block: {shown:?}"
    );
    assert_eq!(
        shown, undrawn,
        "and it is the panel every fence without a graphic is: {shown:?}"
    );
    assert!(
        shown.iter().any(|row| row.contains("refuse the fence")),
        "the source is on screen while it arrives: {shown:?}"
    );
    assert!(
        !shown.iter().any(|row| row.contains('‹')),
        "and nothing is said of a diagram that was never measured: {shown:?}"
    );
}

/// A diagram is a block the instant the closing delimiter lands, in an answer
/// whose prose is still streaming: what arrives after a closed fence does not
/// wait for the answer to end, and the rows the panel stood in for are the
/// diagram's as soon as the loop has drawn it.
#[test]
fn a_fence_that_closed_is_drawn_while_the_answer_runs_on() {
    let closed = format!("```mermaid\n{PORTRAIT}```\n\nand the prose after it\n");
    let block = rows(&store(), &closed, 60);
    let shown = texts(&block.rows);

    assert_eq!(
        block.pictures.len(),
        1,
        "a closed fence is a diagram: {shown:?}"
    );
    assert!(
        shown
            .iter()
            .any(|row| row.contains("and the prose after it")),
        "with the prose under it still streaming: {shown:?}"
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
    let worth = worth(&store(), BROKEN, 60);
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
    let block = rows(&store(), &fence("mermaid", PORTRAIT), 60);
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
    let block = rows(&store(), &fence("mermaid", DIAGRAM), 60);
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
